//! Immutable, verified conversion components. Only the small active descriptor
//! changes at activation; a failed install leaves the previous component usable.
use anyhow::{bail, Context, Result};
use sag_protocol::DoclingProfile;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
pub const VERSION: &str = "2.140.0-documents.1";
pub const PROFILE: &str = "documents";
pub const MEDIA_VERSION: &str = "2.140.0-media.1";
fn identity(profile: DoclingProfile) -> (&'static str, &'static str) {
    match profile {
        DoclingProfile::Documents => (PROFILE, VERSION),
        DoclingProfile::Media => ("media", MEDIA_VERSION),
    }
}
#[derive(Deserialize, Serialize)]
pub struct Manifest {
    pub schema: u32,
    pub component_version: String,
    pub profile: String,
    pub platform: String,
    pub python: String,
    pub site_packages: String,
    pub models: String,
    pub files: Vec<File>,
    #[serde(default)]
    pub supported_extensions: Vec<String>,
}
#[derive(Deserialize, Serialize)]
pub struct File {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Deserialize, Serialize)]
struct Active {
    schema: u32,
    directory: String,
    component_version: String,
}
pub struct Runtime {
    pub root: PathBuf,
    pub python: PathBuf,
    pub site_packages: PathBuf,
    pub models: PathBuf,
}
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}
fn relative(value: &str) -> Result<&Path> {
    let path = Path::new(value);
    if value.is_empty()
        || path
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
    {
        bail!("组件包含无效相对路径")
    }
    Ok(path)
}
fn inside(root: &Path, path: &str) -> Result<PathBuf> {
    let path = root.join(relative(path)?);
    let resolved = path.canonicalize()?;
    if !resolved.starts_with(root.canonicalize()?) {
        bail!(
            "组件符号链接越过组件目录：{} -> {}",
            path.display(),
            resolved.display()
        )
    }
    Ok(path)
}
fn manifest(root: &Path, profile: DoclingProfile) -> Result<Manifest> {
    let path = root.join("manifest.json");
    let data = read_control(&path, 16 * 1024 * 1024, "组件清单")?;
    let m: Manifest = serde_json::from_slice(&data)?;
    let (name, version) = identity(profile);
    if m.schema != 1
        || m.component_version != version
        || m.profile != name
        || m.platform != platform()
    {
        bail!("组件版本、文档能力或平台与当前应用不匹配")
    }
    Ok(m)
}
pub fn runtime(root: &Path) -> Result<Runtime> {
    runtime_for(root, DoclingProfile::Documents)
}
pub fn runtime_for(root: &Path, profile: DoclingProfile) -> Result<Runtime> {
    let m = manifest(root, profile)?;
    let python = inside(root, &m.python)?;
    let site_packages = inside(root, &m.site_packages)?;
    let models = inside(root, &m.models)?;
    if !python.is_file() || !site_packages.is_dir() || !models.is_dir() {
        bail!("组件运行时或模型目录缺失")
    }
    Ok(Runtime {
        root: root.to_path_buf(),
        python,
        site_packages,
        models,
    })
}
pub fn active(home: &Path) -> Result<Runtime> {
    active_for(home, DoclingProfile::Documents)
}
pub fn active_for(home: &Path, profile: DoclingProfile) -> Result<Runtime> {
    let (_, version) = identity(profile);
    let data = read_control(&home.join("active.json"), 8192, "组件激活记录")?;
    let active: Active = serde_json::from_slice(&data)?;
    if active.schema != 1 || active.component_version != version {
        bail!("组件激活记录版本不匹配")
    }
    let root = inside(home, &active.directory)?;
    let ready = read_control(&root.join("ready"), 256, "组件就绪标记")?;
    if std::str::from_utf8(&ready)?.trim() != version {
        bail!("组件尚未通过文档转换验收")
    }
    runtime_for(&root, profile)
}
pub fn verify(root: &Path, cancel: &dyn Fn() -> bool) -> Result<()> {
    verify_for(root, cancel, DoclingProfile::Documents)
}
pub fn verify_for(root: &Path, cancel: &dyn Fn() -> bool, profile: DoclingProfile) -> Result<()> {
    let m = manifest(root, profile)?;
    if m.files.is_empty() {
        bail!("组件清单没有可校验文件")
    }
    let mut expected = BTreeSet::new();
    for file in m.files {
        if cancel() {
            bail!("已取消组件安装")
        }
        relative(&file.path)?;
        if file.path == "manifest.json"
            || file.path == "ready"
            || !expected.insert(file.path.clone())
        {
            bail!("组件清单包含重复或保留路径")
        }
        let path = inside(root, &file.path)?;
        let mut input = open_regular(&path)?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            if cancel() {
                bail!("已取消组件安装")
            }
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        if input.metadata()?.len() != file.size || format!("{:x}", hash.finalize()) != file.sha256 {
            bail!("组件文件校验失败：{}", file.path)
        }
    }
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(root)?;
        if entry.file_type().is_symlink() {
            let target = fs::read_link(path)?;
            if target.is_absolute() || !path.canonicalize()?.starts_with(root.canonicalize()?) {
                bail!(
                    "组件符号链接越过组件目录：{} -> {}",
                    path.display(),
                    target.display()
                )
            }
        } else if entry.file_type().is_file()
            && relative != Path::new("manifest.json")
            && !expected.contains(&relative.to_string_lossy().replace('\\', "/"))
        {
            bail!("组件包含清单外文件：{}", relative.display())
        } else if !entry.file_type().is_file() && !entry.file_type().is_dir() {
            bail!("组件包含不支持的特殊文件：{}", relative.display())
        }
    }
    runtime_for(root, profile)?;
    Ok(())
}
pub fn stage(
    home: &Path,
    source: &Path,
    cancel: &dyn Fn() -> bool,
    probe: impl FnOnce(&Runtime) -> Result<()>,
) -> Result<()> {
    verify(source, cancel)?;
    let components = home.join("components");
    fs::create_dir_all(&components)?;
    if components
        .canonicalize()?
        .starts_with(source.canonicalize()?)
    {
        bail!("组件来源不能包含安装目标目录")
    }
    let staging = tempfile::Builder::new()
        .prefix("documents-")
        .tempdir_in(&components)?;
    for entry in walkdir::WalkDir::new(source).follow_links(false) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(source)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        if cancel() {
            bail!("已取消组件安装")
        }
        let destination = staging.path().join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(destination)?;
        } else if entry.file_type().is_symlink() {
            let target = fs::read_link(entry.path())?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, destination)?;
            #[cfg(windows)]
            {
                // Copy the verified target on Windows; creating links otherwise
                // requires Developer Mode/admin rights on a clean machine.
                if entry.path().is_dir() {
                    bail!("Windows 组件不能包含目录符号链接")
                }
                fs::copy(entry.path(), destination)?;
            }
        } else if entry.file_type().is_file() {
            let mut from = open_regular(entry.path())?;
            let mut to = fs::File::create(&destination)?;
            let mut buffer = [0u8; 256 * 1024];
            loop {
                if cancel() {
                    bail!("已取消组件安装")
                }
                let n = from.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                to.write_all(&buffer[..n])?;
            }
            fs::set_permissions(&destination, fs::metadata(entry.path())?.permissions())?;
        } else {
            bail!("组件包含不支持的特殊文件")
        }
    }
    activate(home, staging, cancel, probe)
}

/// Hash a freshly downloaded runtime in place. The candidate never moves, so
/// managed Python and generated wheel entry points keep their original paths.
pub fn write_manifest(root: &Path, python: &Path, cancel: &dyn Fn() -> bool) -> Result<()> {
    write_manifest_for(root, python, cancel, DoclingProfile::Documents)
}
pub fn write_manifest_for(
    root: &Path,
    python: &Path,
    cancel: &dyn Fn() -> bool,
    profile: DoclingProfile,
) -> Result<()> {
    normalize_internal_links(root, cancel)?;
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if cancel() {
            bail!("已取消组件安装")
        }
        if entry.file_type().is_dir() {
            continue;
        }
        if entry.path() == root.join("manifest.json") || entry.path() == root.join("ready") {
            continue;
        }
        if entry.file_type().is_symlink() {
            let target = fs::read_link(entry.path())?;
            if target.is_absolute()
                || !entry
                    .path()
                    .canonicalize()?
                    .starts_with(root.canonicalize()?)
            {
                bail!(
                    "组件符号链接越过组件目录：{} -> {}",
                    entry.path().display(),
                    target.display()
                )
            }
            continue;
        }
        let mut input = open_regular(entry.path())?;
        let mut hash = Sha256::new();
        let mut size = 0;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            if cancel() {
                bail!("已取消组件安装")
            }
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
            size += n as u64;
        }
        files.push(File {
            path: entry
                .path()
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/"),
            size,
            sha256: format!("{:x}", hash.finalize()),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let (name, version) = identity(profile);
    let manifest = Manifest {
        schema: 1,
        component_version: version.into(),
        profile: name.into(),
        platform: platform(),
        python: python
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/"),
        site_packages: "site-packages".into(),
        models: "models".into(),
        files,
        supported_extensions: vec![],
    };
    let mut file = fs::File::create(root.join("manifest.json"))?;
    serde_json::to_writer(&mut file, &manifest)?;
    file.sync_all()?;
    Ok(())
}

fn normalize_internal_links(root: &Path, cancel: &dyn Fn() -> bool) -> Result<()> {
    let boundary = root.canonicalize()?;
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if cancel() {
            bail!("已取消组件安装")
        }
        if !entry.file_type().is_symlink() {
            continue;
        }
        let path = entry.path();
        let target = fs::read_link(path)?;
        let resolved = path.canonicalize().with_context(|| {
            format!(
                "无法解析组件符号链接：{} -> {}",
                path.display(),
                target.display()
            )
        })?;
        if !resolved.starts_with(&boundary) {
            bail!(
                "组件符号链接越过组件目录：{} -> {}",
                path.display(),
                target.display()
            )
        }
        if !target.is_absolute() {
            continue;
        }
        // UV creates an absolute major/minor-version directory alias inside
        // its own install root. Preserve the alias with a relative target;
        // an absolute target is never sufficient evidence of containment.
        let parent = path
            .parent()
            .context("组件链接缺少父目录")?
            .canonicalize()?;
        let from: Vec<_> = parent.components().collect();
        let to: Vec<_> = resolved.components().collect();
        let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
        let mut relative = PathBuf::new();
        for _ in common..from.len() {
            relative.push("..");
        }
        for part in &to[common..] {
            relative.push(part.as_os_str());
        }
        if relative.as_os_str().is_empty() {
            relative.push(".");
        }
        #[cfg(unix)]
        {
            fs::remove_file(path)?;
            std::os::unix::fs::symlink(&relative, path)?;
        }
        #[cfg(windows)]
        {
            if resolved.is_dir() {
                // uv creates this alias as a junction. Rewriting it as a
                // directory symlink needs Developer Mode or admin rights
                // (error 1314) and the runtime never uses the alias: the
                // manifest points at the fully versioned directory. Drop it.
                let _ = relative;
                fs::remove_dir(path)?;
            } else {
                fs::remove_file(path)?;
                fs::copy(&resolved, path)?;
            }
        }
    }
    Ok(())
}

pub fn activate(
    home: &Path,
    staging: tempfile::TempDir,
    cancel: &dyn Fn() -> bool,
    probe: impl FnOnce(&Runtime) -> Result<()>,
) -> Result<()> {
    activate_for(home, staging, cancel, probe, DoclingProfile::Documents)
}
pub fn activate_for(
    home: &Path,
    staging: tempfile::TempDir,
    cancel: &dyn Fn() -> bool,
    probe: impl FnOnce(&Runtime) -> Result<()>,
    profile: DoclingProfile,
) -> Result<()> {
    let (_, version) = identity(profile);
    verify_for(staging.path(), cancel, profile)?;
    probe(&runtime_for(staging.path(), profile)?).context("组件离线转换验收失败，已保留原组件")?;
    if cancel() {
        bail!("已取消组件安装")
    }
    fs::write(staging.path().join("ready"), version)?;
    // Keep the version directory in place: Python distributions can contain
    // path-sensitive files. No venv/shebang relocation is involved.
    let directory = staging
        .path()
        .strip_prefix(home)?
        .to_string_lossy()
        .replace('\\', "/");
    let mut active = tempfile::NamedTempFile::new_in(home)?;
    serde_json::to_writer(
        &mut active,
        &Active {
            schema: 1,
            directory,
            component_version: version.into(),
        },
    )?;
    active.as_file().sync_all()?;
    let retained = staging.keep();
    if let Err(error) = active.persist(home.join("active.json")) {
        let _ = fs::remove_dir_all(retained);
        return Err(error.error.into());
    }
    Ok(())
}
fn open_regular(path: &Path) -> Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        bail!("组件包含不支持的特殊文件")
    }
    Ok(file)
}
pub(crate) fn read_control(path: &Path, limit: u64, label: &str) -> Result<Vec<u8>> {
    // Open nonblocking and inspect the opened descriptor before any read. A
    // metadata check followed by ordinary open can still block on a FIFO.
    let file = open_regular(path)?;
    if file.metadata()?.len() > limit {
        bail!("{label}过大")
    }
    let mut data = Vec::new();
    file.take(limit + 1).read_to_end(&mut data)?;
    if data.len() as u64 > limit {
        bail!("{label}过大")
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(root: &Path) {
        fs::create_dir_all(root.join("python/bin")).unwrap();
        fs::create_dir(root.join("site-packages")).unwrap();
        fs::create_dir(root.join("models")).unwrap();
        let binary = root.join("python/bin/python3.12");
        fs::write(&binary, "test runtime").unwrap();
        let m = Manifest {
            schema: 1,
            component_version: VERSION.into(),
            profile: PROFILE.into(),
            platform: platform(),
            python: "python/bin/python3.12".into(),
            site_packages: "site-packages".into(),
            models: "models".into(),
            supported_extensions: vec!["pdf".into()],
            files: vec![File {
                path: "python/bin/python3.12".into(),
                size: 12,
                sha256: crate::docling::fingerprint(&binary).unwrap(),
            }],
        };
        fs::write(root.join("manifest.json"), serde_json::to_vec(&m).unwrap()).unwrap();
    }
    #[test]
    fn corrupted_or_unlisted_component_files_are_rejected_before_activation() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("source");
        fixture(&source);
        verify(&source, &|| false).unwrap();
        fs::write(source.join("python/bin/python3.12"), "changed").unwrap();
        assert!(verify(&source, &|| false).is_err());
        fixture_rewrite(&source);
        fs::write(source.join("unexpected.py"), "unexpected").unwrap();
        assert!(verify(&source, &|| false).is_err());
    }
    fn fixture_rewrite(root: &Path) {
        fs::remove_dir_all(root).unwrap();
        fixture(root);
    }
    fn media_candidate(home: &Path) -> tempfile::TempDir {
        fs::create_dir_all(home.join("components")).unwrap();
        let candidate = tempfile::tempdir_in(home.join("components")).unwrap();
        fixture(candidate.path());
        write_manifest_for(
            candidate.path(),
            &candidate.path().join("python/bin/python3.12"),
            &|| false,
            DoclingProfile::Media,
        )
        .unwrap();
        candidate
    }
    #[test]
    fn media_activation_and_failures_are_isolated_from_document_runtime() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("source");
        fixture(&source);
        let documents = t.path().join("home");
        stage(&documents, &source, &|| false, |_| Ok(())).unwrap();
        let documents_active = fs::read(documents.join("active.json")).unwrap();
        let media = documents.join("media");
        let candidate = media_candidate(&media);
        assert!(runtime(candidate.path()).is_err());
        activate_for(
            &media,
            candidate,
            &|| false,
            |_| Ok(()),
            DoclingProfile::Media,
        )
        .unwrap();
        let media_active = fs::read(media.join("active.json")).unwrap();
        assert!(active_for(&media, DoclingProfile::Media).is_ok());
        assert!(active(&media).is_err());
        assert!(active_for(&documents, DoclingProfile::Media).is_err());
        let candidate = media_candidate(&media);
        assert!(activate_for(
            &media,
            candidate,
            &|| false,
            |_| bail!("ASR probe failed"),
            DoclingProfile::Media
        )
        .is_err());
        let candidate = media_candidate(&media);
        assert!(activate_for(
            &media,
            candidate,
            &|| true,
            |_| Ok(()),
            DoclingProfile::Media
        )
        .is_err());
        assert_eq!(fs::read(media.join("active.json")).unwrap(), media_active);
        assert_eq!(
            fs::read(documents.join("active.json")).unwrap(),
            documents_active
        );
        assert_eq!(fs::read_dir(media.join("components")).unwrap().count(), 1);
        assert!(active(&documents).is_ok());
    }
    #[test]
    fn failed_probe_and_cancellation_keep_the_previous_active_component() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("source");
        fixture(&source);
        let home = t.path().join("home");
        stage(&home, &source, &|| false, |_| Ok(())).unwrap();
        let old = fs::read(home.join("active.json")).unwrap();
        assert!(active(&home).is_ok());
        assert!(stage(&home, &source, &|| false, |_| bail!("missing OCR model")).is_err());
        assert_eq!(fs::read(home.join("active.json")).unwrap(), old);
        assert!(stage(&home, &source, &|| true, |_| Ok(())).is_err());
        assert_eq!(fs::read(home.join("active.json")).unwrap(), old);
        assert_eq!(fs::read_dir(home.join("components")).unwrap().count(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn external_component_symlinks_are_rejected() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("source");
        fixture(&source);
        std::os::unix::fs::symlink(t.path(), source.join("escape")).unwrap();
        assert!(verify(&source, &|| false).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn generated_manifests_normalize_real_internal_aliases_and_reject_external_links() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("component");
        fixture(&root);
        fs::remove_file(root.join("manifest.json")).unwrap();
        let alias = root.join("python/cpython-3.12-fixture");
        let target = root.join("python/bin");
        // UV creates an absolute alias for its major/minor Python version.
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        write_manifest(&root, &root.join("python/bin/python3.12"), &|| false).unwrap();
        assert!(!fs::read_link(&alias).unwrap().is_absolute());
        assert_eq!(
            alias.canonicalize().unwrap(),
            target.canonicalize().unwrap()
        );
        verify(&root, &|| false).unwrap();
        let outside = temp.path().join("outside-model");
        fs::write(&outside, "outside").unwrap();
        let escape = root.join("models/escape");
        std::os::unix::fs::symlink(&outside, &escape).unwrap();
        let error = write_manifest(&root, &root.join("python/bin/python3.12"), &|| false)
            .unwrap_err()
            .to_string();
        assert!(error.contains(&escape.display().to_string()), "{error}");
        assert!(error.contains(&outside.display().to_string()), "{error}");
        assert_eq!(fs::read_to_string(&outside).unwrap(), "outside");
        assert!(verify(&root, &|| false).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn control_file_fifos_fail_without_blocking() {
        use std::{
            os::unix::ffi::OsStrExt,
            process::{Command, Stdio},
            time::{Duration, Instant},
        };
        if let Some(kind) = std::env::var_os("SAG_TEST_COMPONENT_FIFO_KIND") {
            let root = PathBuf::from(std::env::var_os("SAG_TEST_COMPONENT_FIFO_ROOT").unwrap());
            let result = if kind == "manifest" {
                verify(&root, &|| false)
            } else {
                active(&root).map(|_| ())
            };
            assert!(result.is_err(), "special control file was accepted");
            return;
        }
        for kind in ["manifest", "active", "ready"] {
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("source");
            fixture(&source);
            let home = temp.path().join("home");
            let (root, path) = if kind == "manifest" {
                (source.clone(), source.join("manifest.json"))
            } else {
                stage(&home, &source, &|| false, |_| Ok(())).unwrap();
                let path = if kind == "active" {
                    home.join("active.json")
                } else {
                    let descriptor: Active =
                        serde_json::from_slice(&fs::read(home.join("active.json")).unwrap())
                            .unwrap();
                    home.join(descriptor.directory).join("ready")
                };
                (home, path)
            };
            fs::remove_file(&path).unwrap();
            let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "docling_component::tests::control_file_fifos_fail_without_blocking",
                    "--nocapture",
                ])
                .env("SAG_TEST_COMPONENT_FIFO_KIND", kind)
                .env("SAG_TEST_COMPONENT_FIFO_ROOT", root)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            let completed = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break Some(status);
                }
                if Instant::now() >= deadline {
                    break None;
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            if completed.is_none() {
                child.kill().unwrap();
                child.wait().unwrap();
            }
            assert!(
                completed.is_some_and(|status| status.success()),
                "{kind} FIFO blocked or failed its error contract"
            );
        }
    }
}

#[cfg(all(test, unix))]
#[test]
fn fifo_in_a_component_is_rejected_without_opening_it() {
    use std::os::unix::ffi::OsStrExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("pipe");
    let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    // A minimally valid manifest plus an unlisted FIFO must fail verification.
    std::fs::create_dir(temp.path().join("site-packages")).unwrap();
    std::fs::create_dir(temp.path().join("models")).unwrap();
    std::fs::write(temp.path().join("python"), "runtime").unwrap();
    let manifest = Manifest {
        schema: 1,
        component_version: VERSION.into(),
        profile: PROFILE.into(),
        platform: platform(),
        python: "python".into(),
        site_packages: "site-packages".into(),
        models: "models".into(),
        supported_extensions: vec![],
        files: vec![File {
            path: "python".into(),
            size: 7,
            sha256: crate::docling::fingerprint(&temp.path().join("python")).unwrap(),
        }],
    };
    std::fs::write(
        temp.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(
        verify(temp.path(), &|| false).is_err(),
        "FIFO reached the component copy stage"
    );
}
