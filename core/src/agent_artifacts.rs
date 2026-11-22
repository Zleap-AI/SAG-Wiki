use crate::{
    indexer::{self, IndexRequest},
    service::Service,
    store::Store,
};
use anyhow::{bail, ensure, Context, Result};
use rusqlite::{params, OptionalExtension};
use sag_protocol::agent_artifacts::{Action, Agent, Format, Options, Source, DEFAULT_FORMATS};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::{self, Read},
    path::{Component, Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant, UNIX_EPOCH},
};
use walkdir::WalkDir;

#[derive(Default)]
pub(crate) struct Jobs {
    pub(crate) scan: Mutex<BTreeMap<Agent, Scan>>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Report {
    changed: usize,
    failed: usize,
    errors: Vec<String>,
}

impl Report {
    fn failed(&mut self, path: &Path, error: &anyhow::Error) {
        self.failed += 1;
        if self.errors.len() < 10 {
            self.errors.push(format!("{} · {error:#}", path.display()));
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Settings {
    options: Options,
    running: bool,
    report: Report,
    #[serde(default)]
    web: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SavedSettings {
    PerAgent(BTreeMap<Agent, Settings>),
    Legacy(Settings),
}

#[derive(Serialize)]
struct Detection {
    agent: Agent,
    directories: Vec<PathBuf>,
}

#[derive(Serialize)]
struct Candidate {
    id: String,
    agent: Agent,
    path: PathBuf,
    name: String,
    format: Format,
    bundle: bool,
    #[serde(skip)]
    relative: PathBuf,
    #[serde(skip)]
    stamp: String,
}

pub(crate) struct Scan {
    token: String,
    options: Options,
    candidates: Vec<Candidate>,
    report: Report,
    finished: Instant,
    web: bool,
}

impl Scan {
    fn page(&self, page: usize) -> serde_json::Value {
        const PAGE_SIZE: usize = 50;
        let total = self.candidates.len();
        let page = page.min(total.saturating_sub(1) / PAGE_SIZE);
        let start = page * PAGE_SIZE;
        serde_json::json!({
            "scan": self.token,
            "total": total,
            "page": page,
            "page_size": PAGE_SIZE,
            "candidates": &self.candidates[start..(start + PAGE_SIZE).min(total)],
            "report": self.report,
        })
    }
}

struct Previous {
    stamp: String,
    digest: Option<String>,
    path: Option<PathBuf>,
    archive_digest: Option<String>,
}

fn detect(agent: Agent, home: &Path, documents: &Path, mac: bool) -> Detection {
    let directories = if !mac {
        Vec::new()
    } else {
        match agent {
            Agent::ClaudeCode => Vec::new(),
            Agent::Codex => {
                let root = documents.join("Codex");
                // Codex saves task files under a dated directory. Only use its
                // explicit outputs folders, which are siblings of scratch files.
                WalkDir::new(&root)
                    .max_depth(3)
                    .into_iter()
                    .filter_entry(|entry| {
                        delivery_entry(entry)
                            && (entry.depth() != 1
                                || entry.file_name() == "outputs"
                                || entry.file_name().to_str().is_some_and(|name| {
                                    chrono::NaiveDate::parse_from_str(name, "%Y-%m-%d").is_ok()
                                }))
                    })
                    .filter_map(Result::ok)
                    .filter(|entry| entry.file_type().is_dir() && entry.file_name() == "outputs")
                    .map(|entry| entry.into_path())
                    .collect()
            }
            Agent::WorkBuddy => [home.join("WorkBuddy"), home.join("Workbuddy")]
                .into_iter()
                .find_map(|path| path.canonicalize().ok().filter(|path| path.is_dir()))
                .into_iter()
                .collect(),
            Agent::ClaudeDesktop => ["Claude", "Claude-3p"]
                .into_iter()
                .flat_map(|app| {
                    WalkDir::new(
                        home.join("Library/Application Support")
                            .join(app)
                            .join("local-agent-mode-sessions"),
                    )
                    .max_depth(4)
                    .into_iter()
                    .filter_map(Result::ok)
                    .filter(|entry| entry.file_type().is_dir() && entry.file_name() == "outputs")
                    .map(|entry| entry.into_path())
                })
                .collect(),
        }
    };
    let mut directories = directories
        .into_iter()
        .filter(|path| fs::symlink_metadata(path).is_ok_and(|entry| entry.is_dir()))
        .collect::<Vec<_>>();
    directories.sort();
    Detection { agent, directories }
}

fn locations(service: &Service, agent: Agent, web: bool) -> Result<(PathBuf, Detection)> {
    if web {
        return Ok((
            service.data_dir.canonicalize()?.join("web-library/agent-artifacts"),
            Detection { agent, directories: Vec::new() },
        ));
    }
    let home = dirs::home_dir().context("无法获取用户目录")?;
    let documents = dirs::document_dir().unwrap_or_else(|| home.join("Documents"));
    Ok((
        documents.join("sag-wiki/agent-artifacts"),
        detect(agent, &home, &documents, cfg!(target_os = "macos")),
    ))
}

fn settings(store: &Store) -> Result<BTreeMap<Agent, Settings>> {
    let db = store.connection();
    let value: Option<String> = db
        .query_row(
            "SELECT value FROM agent_artifact_settings WHERE id=1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    match serde_json::from_str(&value)? {
        SavedSettings::PerAgent(by_agent) => Ok(by_agent),
        SavedSettings::Legacy(state) => Ok(state
            .options
            .sources
            .into_iter()
            .map(|source| {
                (
                    source.agent,
                    Settings {
                        running: state.running && source.enabled,
                        options: Options {
                            sources: vec![source],
                            formats: state.options.formats.clone(),
                            destination: state.options.destination.clone(),
                        },
                        report: state.report.clone(),
                        web: state.web,
                    },
                )
            })
            .collect()),
    }
}

fn save_settings(db: &rusqlite::Connection, value: &BTreeMap<Agent, Settings>) -> Result<()> {
    db.execute(
        "INSERT INTO agent_artifact_settings VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET value=excluded.value",
        [serde_json::to_string(value)?],
    )?;
    Ok(())
}

fn previous(store: &Store) -> Result<HashMap<String, Previous>> {
    let db = store.connection();
    let mut query =
        db.prepare("SELECT id,stamp,digest,path,archive_digest FROM agent_artifact_items")?;
    let rows = query.query_map([], |row| {
        Ok((
            row.get(0)?,
            Previous {
                stamp: row.get(1)?,
                digest: row.get(2)?,
                path: row.get::<_, Option<String>>(3)?.map(PathBuf::from),
                archive_digest: row.get(4)?,
            },
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn record(db: &rusqlite::Connection, id: &str, item: &Previous) -> Result<()> {
    db.execute("INSERT INTO agent_artifact_items VALUES(?1,?2,?3,?4,?5)
        ON CONFLICT(id) DO UPDATE SET stamp=excluded.stamp,digest=excluded.digest,path=excluded.path,archive_digest=excluded.archive_digest",
        params![id,item.stamp,item.digest,item.path.as_ref().map(|p| p.to_string_lossy()),item.archive_digest])?;
    Ok(())
}

fn destination_path(path: &Path) -> Result<PathBuf> {
    ensure!(
        path.is_absolute() && !path.components().any(|part| part == Component::ParentDir),
        "请选择绝对路径的文件夹"
    );
    match path.canonicalize() {
        Ok(path) => {
            ensure!(path.is_dir(), "请选择文件夹");
            Ok(path)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let parent = path.parent().context("保存路径无效")?;
            Ok(destination_path(parent)?.join(path.file_name().context("保存路径无效")?))
        }
        Err(e) => Err(e).with_context(|| format!("路径不可用：{}", path.display())),
    }
}

fn directory(path: &Path) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "请选择绝对路径的文件夹");
    let path = path
        .canonicalize()
        .with_context(|| format!("路径不可用：{}", path.display()))?;
    ensure!(path.is_dir(), "请选择文件夹");
    Ok(path)
}

fn web_paths(store: &Store, options: &mut Options, default: &Path) -> Result<()> {
    let [source] = options.sources.as_mut_slice() else {
        bail!("请选择一个产物来源")
    };
    let path = source.directory.as_ref().context("请选择一个已接入的文件夹")?;
    source.directory = Some(directory(&store.authorize(path)?)?);
    if options.destination == default {
        ensure!(destination_path(default)? == default, "保存位置已变化");
    } else {
        options.destination = directory(&store.authorize(&options.destination)?)?;
    }
    Ok(())
}

fn web_excluded(store: &Store, path: &Path, bundle: bool) -> Result<bool> {
    if store.is_excluded(path)? {
        return Ok(true);
    }
    if bundle {
        for entry in bundle_entries(path) {
            if store.is_excluded(entry?.path())? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn format(path: &Path) -> Option<Format> {
    Some(
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "html" | "htm" => Format::Html,
            "ppt" | "pptx" => Format::Slides,
            "doc" | "docx" => Format::Word,
            "xls" | "xlsx" | "csv" => Format::Spreadsheet,
            "pdf" => Format::Pdf,
            "md" | "markdown" => Format::Markdown,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "svg" => {
                Format::Image
            }
            _ => return None,
        },
    )
}

fn delivery_entry(entry: &walkdir::DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    !entry.path_is_symlink()
        && !name.starts_with('.')
        && !name.starts_with("~$")
        && !matches!(name.as_ref(), "node_modules" | "__MACOSX")
        && !matches!(
            entry.path().extension().and_then(|ext| ext.to_str()),
            Some("tmp" | "part" | "partial" | "crdownload" | "download")
        )
}

fn metadata_stamp(path: &Path, hash: &mut Sha256) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(meta.is_file(), "产物文件不可用");
    let time = meta.modified()?.duration_since(UNIX_EPOCH)?;
    hash.update(meta.len().to_le_bytes());
    hash.update(time.as_nanos().to_le_bytes());
    Ok(())
}

fn bundle_entries(path: &Path) -> impl Iterator<Item = walkdir::Result<walkdir::DirEntry>> {
    WalkDir::new(path)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(delivery_entry)
}

fn stamp(path: &Path, bundle: bool) -> Result<String> {
    let mut hash = Sha256::new();
    if bundle {
        for entry in bundle_entries(path) {
            let entry = entry?;
            if entry.file_type().is_file() {
                hash.update(
                    entry
                        .path()
                        .strip_prefix(path)?
                        .as_os_str()
                        .as_encoded_bytes(),
                );
                hash.update([0]);
                metadata_stamp(entry.path(), &mut hash)?;
            }
        }
    } else {
        metadata_stamp(path, &mut hash)?;
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn candidate(
    agent: Agent,
    path: &Path,
    relative: PathBuf,
    format: Format,
    bundle: bool,
) -> Result<Candidate> {
    let mut hash = Sha256::new();
    hash.update(agent.folder());
    hash.update([0]);
    hash.update(path.as_os_str().as_encoded_bytes());
    Ok(Candidate {
        id: format!("{:x}", hash.finalize()),
        agent,
        relative,
        format,
        bundle,
        name: path
            .file_name()
            .context("文件名无效")?
            .to_string_lossy()
            .into_owned(),
        path: path.to_path_buf(),
        stamp: stamp(path, bundle)?,
    })
}

fn workbuddy_files(root: &Path) -> Result<Vec<PathBuf>> {
    use reqwest::Url;

    #[derive(Deserialize)]
    struct Index {
        artifacts: Vec<Artifact>,
    }
    #[derive(Deserialize)]
    struct Artifact {
        uri: String,
    }

    let index = root.parent().unwrap().join(".workbuddy/artifact-index");
    let entries = match fs::read_dir(&index) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("无法读取 WorkBuddy 产物清单：{}", index.display()))
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_file() || path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let index: Index = serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("WorkBuddy 产物清单格式无效：{}", path.display()))?;
        for artifact in index.artifacts {
            let Some(path) = Url::parse(&artifact.uri)
                .ok()
                .and_then(|url| url.to_file_path().ok())
            else {
                continue;
            };
            if !fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file()) {
                continue;
            }
            let path = path.canonicalize()?;
            if path.starts_with(root) {
                paths.push(path);
            }
        }
    }
    Ok(paths)
}

fn collect_source(
    source: &Source,
    root: &Path,
    destination: &Path,
    formats: &[Format],
    old: &HashMap<String, Previous>,
    found: &mut Vec<Candidate>,
    report: &mut Report,
    stop: &tokio_util::sync::CancellationToken,
) -> Result<()> {
    let available = directory(root)?;
    ensure!(available == root, "交付目录已变化，请重新选择");
    let root = available;
    ensure!(
        !root.starts_with(destination) && !destination.starts_with(&root),
        "交付目录与保存位置不能重叠"
    );
    let mut add = |path: &Path, format: Format, bundle| -> Result<()> {
        let relative = if path == root {
            PathBuf::from(path.file_name().context("文件名无效")?)
        } else {
            path.strip_prefix(&root)?.to_path_buf()
        };
        let item = candidate(source.agent, path, relative, format, bundle)?;
        if old
            .get(&item.id)
            .is_none_or(|record| record.stamp != item.stamp)
        {
            found.push(item);
        }
        Ok(())
    };
    if source.agent == Agent::WorkBuddy && source.directory.is_none() {
        for path in workbuddy_files(&root)? {
            ensure!(!stop.is_cancelled(), "已停止");
            if let Some(format) = format(&path).filter(|format| formats.contains(format)) {
                if let Err(error) = add(&path, format, false) {
                    report.failed(&path, &error);
                }
            }
        }
        return Ok(());
    }
    let walk = WalkDir::new(&root);
    let walk = if source.agent == Agent::Codex && source.directory.is_none() {
        // Unpacked projects under outputs often contain screenshots, build products
        // and samples. Direct files and HTML packages are the default deliveries.
        walk.max_depth(1)
    } else {
        walk
    };
    let mut walk = walk.into_iter().filter_entry(delivery_entry);
    while let Some(entry) = walk.next() {
        ensure!(!stop.is_cancelled(), "已停止");
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                report.failed(&root, &error.into());
                continue;
            }
        };
        let path = entry.path();
        let bundle = entry.file_type().is_dir()
            && ["index.html", "index.htm"]
                .into_iter()
                .any(|name| fs::symlink_metadata(path.join(name)).is_ok_and(|meta| meta.is_file()));
        let format = if bundle {
            walk.skip_current_dir();
            if !formats.contains(&Format::Html) {
                continue;
            }
            Format::Html
        } else if entry.file_type().is_file() {
            let Some(format) = format(path).filter(|format| formats.contains(format)) else {
                continue;
            };
            format
        } else {
            continue;
        };
        if let Err(error) = add(path, format, bundle) {
            report.failed(path, &error);
        }
    }
    Ok(())
}

fn scan(service: &Service, mut options: Options, detected: &Detection, web: bool) -> Result<Scan> {
    if web {
        let (default, _) = locations(service, detected.agent, true)?;
        web_paths(&service.store, &mut options, &default)?;
    }
    options.destination = destination_path(&options.destination)?;
    ensure!(!options.formats.is_empty(), "请选择文件类型");
    let [source] = options.sources.as_slice() else {
        bail!("请选择一个产物来源")
    };
    ensure!(
        source.agent == detected.agent && source.enabled,
        "产物来源无效"
    );
    let old = previous(&service.store)?;
    let mut candidates = Vec::new();
    let mut report = Report::default();
    if source.agent == Agent::ClaudeCode && !web {
        let scope = source.directory.as_deref().map(directory).transpose()?;
        let (paths, issues) = crate::chat_import::claude_deliverables::find(&service.stop)?;
        for issue in issues {
            report.failed(Path::new(&issue.path), &anyhow::anyhow!(issue.error));
        }
        for path in paths {
            ensure!(!service.stop.is_cancelled(), "已停止");
            let result = (|| -> Result<()> {
                match fs::symlink_metadata(&path) {
                    Ok(meta) if meta.is_file() => {}
                    Ok(_) => return Ok(()),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                    Err(error) => return Err(error.into()),
                }
                let canonical = path.canonicalize()?;
                if canonical.starts_with(&options.destination)
                    || scope
                        .as_ref()
                        .is_some_and(|scope| !canonical.starts_with(scope))
                {
                    return Ok(());
                }
                let Some(format) =
                    format(&canonical).filter(|format| options.formats.contains(format))
                else {
                    return Ok(());
                };
                let relative = PathBuf::from(canonical.file_name().context("文件名无效")?);
                let item = candidate(source.agent, &canonical, relative, format, false)?;
                if old
                    .get(&item.id)
                    .is_none_or(|record| record.stamp != item.stamp)
                {
                    candidates.push(item);
                }
                Ok(())
            })();
            if let Err(error) = result {
                report.failed(&path, &error);
            }
        }
    } else {
        let roots = source
            .directory
            .as_ref()
            .map(std::slice::from_ref)
            .unwrap_or(&detected.directories);
        for root in roots {
            if let Err(error) = collect_source(
                source,
                root,
                &options.destination,
                &options.formats,
                &old,
                &mut candidates,
                &mut report,
                &service.stop,
            ) {
                report.failed(root, &error);
            }
            ensure!(!service.stop.is_cancelled(), "已停止");
        }
    }
    if web {
        let mut accessible = Vec::new();
        for item in candidates {
            if !web_excluded(&service.store, &item.path, item.bundle)? {
                accessible.push(item);
            }
        }
        candidates = accessible;
    }
    candidates.sort_by(|a, b| a.agent.cmp(&b.agent).then_with(|| a.path.cmp(&b.path)));
    candidates.dedup_by(|a, b| a.id == b.id);
    Ok(Scan {
        token: uuid::Uuid::new_v4().to_string(),
        options,
        candidates,
        report,
        finished: Instant::now(),
        web,
    })
}

fn read_file(
    path: &Path,
    stop: &tokio_util::sync::CancellationToken,
    mut consume: impl FnMut(&[u8]) -> io::Result<()>,
) -> Result<()> {
    ensure!(fs::symlink_metadata(path)?.is_file(), "产物文件不可用");
    let mut file = fs::File::open(path)?;
    let mut block = [0; 65536];
    loop {
        ensure!(!stop.is_cancelled(), "已停止");
        let n = file.read(&mut block)?;
        if n == 0 {
            break;
        }
        consume(&block[..n])?;
    }
    Ok(())
}

fn digest(path: &Path, bundle: bool, stop: &tokio_util::sync::CancellationToken) -> Result<String> {
    let mut hash = Sha256::new();
    if bundle {
        ensure!(fs::symlink_metadata(path)?.is_dir(), "产物目录不可用");
        for entry in bundle_entries(path) {
            let entry = entry?;
            if entry.file_type().is_file() {
                hash.update(
                    entry
                        .path()
                        .strip_prefix(path)?
                        .as_os_str()
                        .as_encoded_bytes(),
                );
                hash.update([0]);
                hash.update(entry.metadata()?.len().to_le_bytes());
                read_file(entry.path(), stop, |bytes| {
                    hash.update(bytes);
                    Ok(())
                })?;
            }
        }
    } else {
        hash.update(fs::symlink_metadata(path)?.len().to_le_bytes());
        read_file(path, stop, |bytes| {
            hash.update(bytes);
            Ok(())
        })?;
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn ensure_parent(destination: &Path, path: &Path) -> Result<()> {
    let relative = path
        .strip_prefix(destination)
        .context("保存路径不在归档目录内")?;
    let mut current = destination.to_path_buf();
    for component in relative.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(meta) => ensure!(meta.is_dir(), "保存位置不可用：{}", current.display()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir(&current)?,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn unused_path(path: &Path, bundle: bool) -> Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(path.to_path_buf()),
        Ok(_) => {}
        Err(e) => return Err(e.into()),
    }
    let name = if bundle {
        path.file_name()
    } else {
        path.file_stem()
    }
    .context("文件名无效")?
    .to_string_lossy();
    let extension = if bundle { None } else { path.extension() };
    for number in 2.. {
        let mut filename = std::ffi::OsString::from(format!("{name} ({number})"));
        if let Some(extension) = extension {
            filename.push(".");
            filename.push(extension);
        }
        let candidate = path.with_file_name(filename);
        match fs::symlink_metadata(&candidate) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(candidate),
            Ok(_) => {}
            Err(e) => return Err(e.into()),
        }
    }
    unreachable!()
}

fn check_current(item: &Candidate) -> Result<()> {
    ensure!(
        item.path.canonicalize()? == item.path && stamp(&item.path, item.bundle)? == item.stamp,
        "文件仍在写入，将在下次扫描后收集"
    );
    Ok(())
}

fn install_bundle(
    path: &Path,
    staged: &Path,
    stage: &mut tempfile::TempDir,
    replace: bool,
) -> Result<()> {
    if !replace {
        ensure!(!path.try_exists()?, "保存位置已有同名文件");
        fs::rename(staged, path)?;
        return Ok(());
    }
    let before = stage.path().join("previous");
    fs::rename(path, &before)?;
    if let Err(error) = fs::rename(staged, path) {
        if let Err(rollback) = fs::rename(&before, path) {
            stage.disable_cleanup(true);
            let recovery = unused_path(path, true)?;
            fs::rename(&before, &recovery)
                .with_context(|| format!("恢复产物目录失败：{} · {rollback}", before.display()))?;
            stage.disable_cleanup(false);
        }
        return Err(error.into());
    }
    Ok(())
}

fn archive_unchanged(
    path: &Path,
    bundle: bool,
    expected: Option<&str>,
    stop: &tokio_util::sync::CancellationToken,
) -> Result<bool> {
    let Some(expected) = expected else {
        return Ok(false);
    };
    match fs::symlink_metadata(path) {
        Ok(meta)
            if !meta.file_type().is_symlink()
                && meta.is_dir() == bundle
                && path.canonicalize()? == path =>
        {
            if bundle {
                for entry in WalkDir::new(path) {
                    if !delivery_entry(&entry?) {
                        return Ok(false);
                    }
                }
            }
            Ok(digest(path, bundle, stop)? == expected)
        }
        Ok(_) => Ok(false),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn copy_item(
    service: &Service,
    destination: &Path,
    item: &Candidate,
    before: Option<&Previous>,
    web: bool,
) -> Result<Previous> {
    use std::io::Write;
    if web {
        service.store.authorize(&item.path)?;
        ensure!(!web_excluded(&service.store, &item.path, item.bundle)?, "此路径已从知识库移除，请先重新添加");
    }
    check_current(item)?;
    let stop = &service.stop;
    let source_digest = digest(&item.path, item.bundle, stop)?;
    let archive = before.and_then(|item| item.path.clone());
    let archive_digest = before.and_then(|item| item.archive_digest.clone());
    if before.and_then(|item| item.digest.as_deref()) == Some(source_digest.as_str()) {
        return Ok(Previous {
            stamp: item.stamp.clone(),
            digest: Some(source_digest),
            path: archive,
            archive_digest,
        });
    }
    let preferred = destination.join(item.agent.folder()).join(&item.relative);
    let editable_archive = archive
        .as_ref()
        .filter(|path| path.starts_with(destination));
    let preferred = editable_archive.unwrap_or(&preferred);
    if web {
        ensure!(!service.store.is_excluded(preferred)?, "此路径已从知识库移除，请先重新添加");
    }
    fs::create_dir_all(destination)?;
    ensure!(destination.canonicalize()? == destination, "保存位置已变化");
    let parent = preferred.parent().context("保存路径无效")?;
    ensure_parent(destination, parent)?;
    let target = || {
        let replace = editable_archive.is_some()
            && (!web || !preferred.try_exists()? || !web_excluded(&service.store, preferred, item.bundle)?)
            && archive_unchanged(preferred, item.bundle, archive_digest.as_deref(), stop)?;
        let path = if replace { preferred.clone() } else { unused_path(preferred, item.bundle)? };
        if web {
            ensure!(!service.store.is_excluded(&path)?, "此路径已从知识库移除，请先重新添加");
            if item.bundle {
                for entry in bundle_entries(&item.path) {
                    let entry = entry?;
                    let relative = entry.path().strip_prefix(&item.path)?;
                    ensure!(!service.store.is_excluded(&path.join(relative))?, "此路径已从知识库移除，请先重新添加");
                }
            }
        }
        Ok::<_, anyhow::Error>((path, replace))
    };
    let path;
    if item.bundle {
        let mut stage = tempfile::Builder::new()
            .prefix(".agent-artifact-")
            .tempdir_in(parent)?;
        let output = stage.path().join("output");
        fs::create_dir(&output)?;
        for entry in bundle_entries(&item.path) {
            ensure!(!stop.is_cancelled(), "已停止");
            let entry = entry?;
            let child = output.join(entry.path().strip_prefix(&item.path)?);
            if entry.depth() == 0 {
                continue;
            }
            if entry.file_type().is_dir() {
                fs::create_dir(child)?;
            } else if entry.file_type().is_file() {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(child)?;
                read_file(entry.path(), stop, |bytes| file.write_all(bytes))?;
            }
        }
        check_current(item)?;
        ensure!(
            digest(&output, true, stop)? == source_digest,
            "文件仍在写入，将在下次扫描后收集"
        );
        let (chosen, replace) = target()?;
        install_bundle(&chosen, &output, &mut stage, replace)?;
        path = chosen;
    } else {
        let mut stage = tempfile::Builder::new()
            .prefix(".agent-artifact-")
            .tempfile_in(parent)?;
        read_file(&item.path, stop, |bytes| {
            stage.as_file_mut().write_all(bytes)
        })?;
        check_current(item)?;
        ensure!(
            digest(stage.path(), false, stop)? == source_digest,
            "文件仍在写入，将在下次扫描后收集"
        );
        stage.as_file().sync_all()?;
        let (chosen, replace) = target()?;
        if replace {
            stage.persist(&chosen)?;
        } else {
            stage.persist_noclobber(&chosen)?;
        }
        path = chosen;
    }
    Ok(Previous {
        stamp: item.stamp.clone(),
        digest: Some(source_digest.clone()),
        path: Some(path),
        archive_digest: Some(source_digest),
    })
}

fn queue_index(
    service: &Service,
    destination: &Path,
    default: &Path,
    paths: Vec<PathBuf>,
    web: bool,
) -> Result<()> {
    let library = if !web && destination_path(default)? == destination {
        destination.parent().context("保存路径无效")?
    } else {
        destination
    };
    let roots = service.store.roots()?;
    if let Some(root) = roots
        .iter()
        .find(|root| destination.starts_with(&root.path))
    {
        service.scan.send(IndexRequest::Paths {
            root: root.path.clone(),
            paths,
        })?;
    } else {
        indexer::add_root(&service.store, &service.scan, library)?;
    }
    Ok(())
}

fn run(
    service: &Service,
    scan: Scan,
    default: &Path,
    selected: impl Fn(&Candidate) -> bool,
) -> Result<Report> {
    check_web_scan(service, &scan, default)?;
    let old = previous(&service.store)?;
    let mut report = scan.report;
    let mut paths = Vec::new();
    // Recheck the scan after a short quiet period so a file still being written can be retried.
    if !scan.candidates.is_empty() {
        std::thread::sleep(Duration::from_secs(1).saturating_sub(scan.finished.elapsed()));
    }
    for item in scan.candidates.iter().filter(|item| selected(item)) {
        if service.stop.is_cancelled() {
            break;
        }
        let before = old.get(&item.id);
        match copy_item(service, &scan.options.destination, item, before, scan.web) {
            Ok(entry) => {
                if before.and_then(|item| item.digest.as_ref()) != entry.digest.as_ref() {
                    paths.extend(entry.path.iter().cloned());
                }
                record(&service.store.connection(), &item.id, &entry)?;
            }
            Err(error) => report.failed(&item.path, &error),
        }
    }
    report.changed = paths.len();
    if !paths.is_empty() {
        queue_index(service, &scan.options.destination, default, paths, scan.web)?;
        service.notify_artifacts_changed();
    }
    Ok(report)
}

pub(crate) fn run_scheduled(service: &Service) -> Result<()> {
    let _access = service.agent_artifacts.scan.lock().unwrap();
    let mut current = settings(&service.store)?;
    let mut changed = false;
    for (&agent, state) in current.iter_mut().filter(|(_, state)| state.running) {
        let (default, detected) = locations(service, agent, state.web)?;
        let result = (|| {
            ensure!(
                destination_path(&state.options.destination)? == state.options.destination,
                "保存位置已变化"
            );
            let scan = scan(service, state.options.clone(), &detected, state.web)?;
            run(service, scan, &default, |_| true)
        })();
        let report = result.unwrap_or_else(|error| {
            let mut report = Report::default();
            report.failed(&state.options.destination, &error);
            report
        });
        if report.changed > 0 || report.failed > 0 || state.report.failed > 0 {
            state.report = report;
            changed = true;
        }
    }
    if changed {
        save_settings(&service.store.connection(), &current)?;
    }
    Ok(())
}

pub(crate) fn handle(service: &Service, action: Action) -> Result<serde_json::Value> {
    handle_action(service, action, false)
}

pub(crate) fn handle_web(service: &Service, action: Action) -> Result<serde_json::Value> {
    handle_action(service, action, true)
}

fn check_web_scan(service: &Service, scan: &Scan, default: &Path) -> Result<()> {
    if scan.web {
        let mut options = scan.options.clone();
        web_paths(&service.store, &mut options, default)?;
        ensure!(
            options.destination == scan.options.destination
                && options.sources[0].directory == scan.options.sources[0].directory,
            "扫描结果已变化，请重新扫描"
        );
        for item in &scan.candidates {
            ensure!(!web_excluded(&service.store, &item.path, item.bundle)?, "扫描结果已变化，请重新扫描");
        }
    }
    Ok(())
}

fn handle_action(service: &Service, action: Action, web: bool) -> Result<serde_json::Value> {
    let mut pending = service.agent_artifacts.scan.lock().unwrap();
    match action {
        Action::Status { agent } => {
            let (default, detected) = locations(service, agent, web)?;
            let state = settings(&service.store)?
                .remove(&agent)
                .filter(|state| !web || state.web)
                .unwrap_or_else(|| Settings {
                    options: Options {
                        sources: vec![Source {
                            agent,
                            enabled: true,
                            directory: None,
                        }],
                        formats: DEFAULT_FORMATS.to_vec(),
                        destination: default.clone(),
                    },
                    running: false,
                    report: Report::default(),
                    web,
                });
            Ok(serde_json::json!({ "running": state.running, "options": state.options, "detected": [detected], "default_destination": default, "report": state.report }))
        }
        Action::Scan { options } => {
            let [source] = options.sources.as_slice() else {
                bail!("请选择一个产物来源")
            };
            let agent = source.agent;
            let (_, detected) = locations(service, agent, web)?;
            let scan = scan(service, options, &detected, web)?;
            let response = scan.page(0);
            pending.insert(agent, scan);
            Ok(response)
        }
        Action::Page { scan, page } => {
            let scan = pending
                .values()
                .find(|value| value.token == scan && value.web == web)
                .context("扫描结果已变化，请重新扫描")?;
            if web {
                let (default, _) = locations(service, scan.options.sources[0].agent, true)?;
                check_web_scan(service, scan, &default)?;
            }
            Ok(scan.page(page))
        }
        Action::Start {
            scan,
            exclude_all,
            exceptions,
            auto_sync,
        } => {
            let agent = pending
                .iter()
                .find_map(|(agent, value)| (value.token == scan && value.web == web).then_some(*agent))
                .context("扫描结果已变化，请重新扫描")?;
            let scan = pending.remove(&agent).unwrap();
            let (default, _) = locations(service, agent, web)?;
            check_web_scan(service, &scan, &default)?;
            let exceptions = exceptions.into_iter().collect::<HashSet<_>>();
            let selected = |item: &Candidate| exclude_all == exceptions.contains(&item.id);
            let old = previous(&service.store)?;
            let mut current = settings(&service.store)?;
            let mut state = Settings {
                options: scan.options.clone(),
                running: auto_sync,
                report: Report::default(),
                web,
            };
            let excluded = scan
                .candidates
                .iter()
                .filter(|item| auto_sync && !selected(item))
                .map(|item| {
                    let before = old.get(&item.id);
                    let unchanged = check_current(item).and_then(|_| {
                        let value = digest(&item.path, item.bundle, &service.stop)?;
                        check_current(item)?;
                        Ok(value)
                    });
                    unchanged
                        .map(|digest| {
                            (
                                &item.id,
                                Previous {
                                    stamp: item.stamp.clone(),
                                    digest: Some(digest),
                                    path: before.and_then(|item| item.path.clone()),
                                    archive_digest: before
                                        .and_then(|item| item.archive_digest.clone()),
                                },
                            )
                        })
                        .map_err(|error| (item.path.clone(), error))
                })
                .collect::<Vec<_>>();
            {
                let mut db = service.store.connection();
                let tx = db.transaction()?;
                for result in excluded {
                    match result {
                        Ok((id, entry)) => record(&tx, id, &entry)?,
                        Err((path, error)) => state.report.failed(&path, &error),
                    }
                }
                tx.commit()?;
            }
            let report = run(service, scan, &default, selected)?;
            state.report.changed = report.changed;
            state.report.failed += report.failed;
            state.report.errors.extend(report.errors);
            state.report.errors.truncate(10);
            let report = serde_json::to_value(&state.report)?;
            current.insert(agent, state);
            save_settings(&service.store.connection(), &current)?;
            Ok(report)
        }
        Action::Stop { agent } => {
            let mut current = settings(&service.store)?;
            current.get_mut(&agent)
                .filter(|state| !web || state.web)
                .context("自动收集尚未开启")?
                .running = false;
            save_settings(&service.store.connection(), &current)?;
            Ok(serde_json::Value::Null)
        }
    }
}

#[cfg(test)]
mod tests;
