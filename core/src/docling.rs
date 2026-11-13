//! Optional external document conversion. The index only reads its Markdown output.
use anyhow::{anyhow, bail, Context, Result};
use sag_protocol::{DoclingProfile, DoclingProfileStatus, DoclingStatus};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock, Mutex, RwLock,
    },
    time::Duration,
};

#[path = "docling_pdf_session.rs"]
mod pdf_session;
pub(crate) use pdf_session::PdfSession;

pub const OUTPUT_DIRECTORY: &str = ".sag-converted";
const VERSION: &str = "2.140.0";
const UV_VERSION: &str = "0.12.1";
const UV_EXECUTABLE: &str = if cfg!(windows) { "uv.exe" } else { "uv" };
const PYTHON_VERSION: &str = "3.12.13";
const DIAGNOSTIC_LIMIT: u64 = 8 * 1024 * 1024;
// Installing a component commits far more than a conversion job: importing
// torch and the OCR runtime takes most of a GiB, and the model downloader
// buffers shards while it writes them. A 2 GiB supervisor limit turned that
// peak into a bare allocation failure in the middle of the model download, so
// the limit is only a guard against runaway growth now.
const INSTALL_MEMORY_FLOOR: u64 = 4 * 1024 * 1024 * 1024;
const INSTALL_MEMORY_CEILING: u64 = 8 * 1024 * 1024 * 1024;
fn install_memory_limit(reserved: u64) -> u64 {
    reserved.clamp(INSTALL_MEMORY_FLOOR, INSTALL_MEMORY_CEILING)
}
const MEDIA_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "m4a", "aac", "ogg", "flac", "mp4", "avi", "mov", "mkv", "webm",
];

// Pinned to Docling's FormatToExtensions. Existing native text formats stay native.
const FORMATS: &[(&str, &str)] = &[
    ("PDF", "pdf"),
    ("Word", "docx dotx docm dotm doc dot rtf"),
    ("PowerPoint", "pptx potx ppsx pptm potm ppsm ppt pot pps"),
    ("Excel / CSV", "xlsx xlsm xlsb xltx xltm xls xlt csv"),
    ("OpenDocument", "odt ott ods ots odp otp"),
    ("Apple iWork", "pages key numbers"),
    ("图片与扫描件（OCR）", "jpg jpeg png tif tiff bmp webp gif"),
    (
        "网页与文本",
        "html htm xhtml mhtml mht md markdown txt text qmd rmd adoc asciidoc asc vtt tex latex",
    ),
    ("电子书与邮件", "epub eml msg boxnote"),
    ("音频", "wav mp3 m4a aac ogg flac"),
    ("视频", "mp4 avi mov mkv webm"),
    (
        "结构化文档",
        "xml nxml xbrl dclg dclx json tar.gz ebc ebcdic afp",
    ),
];
/// Formats that need a downloaded Docling component. Formats the built-in
/// converter handles are listed by [`builtin_formats`] instead.
pub fn formats() -> Vec<(String, Vec<String>)> {
    grouped_formats(|ext| ext == "pdf" || !crate::anydoc::handles_extension(ext))
}
/// Formats converted in process without any download.
pub fn builtin_formats() -> Vec<(String, Vec<String>)> {
    grouped_formats(crate::anydoc::handles_extension)
}
fn grouped_formats(select: impl Fn(&str) -> bool) -> Vec<(String, Vec<String>)> {
    FORMATS
        .iter()
        .filter_map(|(name, extensions)| {
            let extensions: Vec<_> = extensions
                .split_whitespace()
                .filter(|ext| is_source(Path::new(&format!("file.{ext}"))) && select(ext))
                .map(str::to_owned)
                .collect();
            (!extensions.is_empty()).then(|| ((*name).into(), extensions))
        })
        .collect()
}
/// Whether a source should be converted under the given folder options:
/// built-in formats always are; component formats only when the user
/// enabled and selected them.
pub fn should_convert(path: &Path, options: &crate::types::FolderIndexOptions) -> bool {
    if crate::anydoc::handles(path) {
        if let Some(extensions) = &options.extensions {
            return path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    extensions
                        .iter()
                        .any(|selected| selected.eq_ignore_ascii_case(extension))
                });
        }
        return options.file_types.text;
    }
    options.convert_other_formats && is_selected(path, &options.conversion_formats)
}

pub fn is_source(path: &Path) -> bool {
    if in_output(path) {
        return false;
    }
    if crate::anydoc::handles(path) {
        return true;
    }
    if crate::text::is_supported(path) {
        return false;
    }
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    FORMATS.iter().any(|(_, extensions)| {
        extensions.split_whitespace().any(|ext| {
            name.strip_suffix(ext)
                .is_some_and(|prefix| prefix.ends_with('.'))
        })
    })
}


fn is_image(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| {
        matches!(ext.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png" | "tif" | "tiff" | "bmp" | "webp" | "gif")
    })
}
pub fn is_pem_key(path: &Path) -> Result<bool> {
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("key"))
    {
        return Ok(false);
    }
    let mut header = [0; 32];
    let length = fs::File::open(path)?.read(&mut header)?;
    Ok(header[..length].starts_with(b"-----BEGIN "))
}

/// Conversion stopped because the component this source needs is not installed.
/// Scans remember these failures and skip the file until the component arrives
/// or the source changes, instead of parsing it again on every start.
#[derive(Debug)]
pub struct ComponentMissing(pub String);
impl std::fmt::Display for ComponentMissing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
}
impl std::error::Error for ComponentMissing {}
pub fn component_missing(error: &anyhow::Error) -> bool {
    error.downcast_ref::<ComponentMissing>().is_some()
}
pub fn required_profile(source: &Path) -> DoclingProfile {
    if source.extension().is_some_and(|ext| MEDIA_EXTENSIONS.iter().any(|media| ext.eq_ignore_ascii_case(media))) {
        DoclingProfile::Media
    } else {
        DoclingProfile::Documents
    }
}
pub fn is_selected(path: &Path, formats: &Option<Vec<String>>) -> bool {
    formats.as_ref().is_none_or(|formats| {
        let name = path.file_name().unwrap().to_string_lossy().to_lowercase();
        formats
            .iter()
            .any(|extension| name.ends_with(&format!(".{extension}")))
    })
}

pub fn in_output(path: &Path) -> bool {
    path.components()
        .any(|part| part.as_os_str() == OUTPUT_DIRECTORY)
}

fn uv_download_url() -> Result<String> {
    let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        _ => bail!("当前系统暂不支持自动安装 Docling"),
    };
    let suffix = if cfg!(windows) { "zip" } else { "tar.gz" };
    Ok(format!(
        "https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/uv-{platform}.{suffix}"
    ))
}

enum OfflineImport {
    Component(PathBuf),
    Uv(PathBuf),
}

pub struct Docling {
    home: PathBuf,
    ocr_workers: Arc<crate::docling_worker::WorkerPool>,
    source: Mutex<Option<PathBuf>>,
    status: Arc<Mutex<DoclingStatus>>,
    /// Per-package byte accounting for uv's non-TTY output, which only
    /// prints one line when a download starts and one when it finishes.
    packages: Arc<Mutex<Packages>>,
    cancel: Arc<AtomicBool>,
    profile: DoclingProfile,
    usage: RwLock<()>,
}
#[derive(Default)]
struct Packages {
    sizes: std::collections::BTreeMap<String, u64>,
    done: std::collections::BTreeSet<String>,
}
impl Packages {
    fn clear(&mut self) {
        self.sizes.clear();
        self.done.clear();
    }
    fn total(&self) -> u64 {
        self.sizes.values().sum()
    }
    fn downloaded(&self) -> u64 {
        self.done.iter().filter_map(|name| self.sizes.get(name)).sum()
    }
}
fn byte_scale(unit: &str) -> f64 {
    match unit.to_ascii_lowercase().as_str() {
        "kib" => 1024.0,
        "mib" => 1048576.0,
        "gib" => 1073741824.0,
        "kb" => 1000.0,
        "mb" => 1e6,
        "gb" => 1e9,
        _ => 1.0,
    }
}

impl Docling {
    pub fn new(data: &Path) -> Self {
        let worker_capacity = std::thread::available_parallelism()
            .map(|value| value.get() / 2)
            .unwrap_or(1)
            .max(1)
            .min(4);
        Self {
            home: data.join("docling"),
            ocr_workers: Arc::new(crate::docling_worker::WorkerPool::new(worker_capacity)),
            source: Mutex::new(None),
            status: Arc::new(Mutex::new(DoclingStatus::default())),
            packages: Arc::new(Mutex::new(Packages::default())),
            cancel: Arc::new(AtomicBool::new(false)),
            profile: DoclingProfile::Documents,
            usage: RwLock::new(()),
        }
    }
    fn binary(&self, name: &str) -> PathBuf {
        self.home.join("bin").join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.into()
        })
    }
    pub fn installed(&self) -> bool {
        crate::docling_component::active(&self.home).is_ok() || self.legacy_installed()
    }
    fn legacy_installed(&self) -> bool {
        crate::docling_component::read_control(&self.home.join("ready"), 256, "组件就绪标记")
            .is_ok_and(|ready| {
                std::str::from_utf8(&ready).is_ok_and(|ready| ready.trim() == VERSION)
            })
            && self.binary("docling").is_file()
    }
    pub fn has_profile(&self, profile: DoclingProfile) -> bool {
        match profile {
            DoclingProfile::Documents => self.installed(),
            DoclingProfile::Media => {
                crate::docling_component::active_for(&self.home.join("media"), profile).is_ok()
            }
        }
    }
    pub fn require_formats(&self, extensions: &Option<Vec<String>>) -> Result<()> {
        // `None` keeps a legacy "all formats" choice; built-in formats always
        // work and component formats are reported per file during indexing.
        let Some(extensions) = extensions else { return Ok(()) };
        for extension in extensions {
            let extension = extension.to_ascii_lowercase();
            if crate::anydoc::handles_extension(&extension) {
                continue;
            }
            let profile = if MEDIA_EXTENSIONS.contains(&extension.as_str()) {
                DoclingProfile::Media
            } else if is_source(Path::new(&format!("file.{extension}"))) {
                DoclingProfile::Documents
            } else {
                bail!("不支持的转换格式：{extension}")
            };
            if !self.has_profile(profile) {
                match profile {
                    DoclingProfile::Documents => bail!("所选格式需要先安装文档组件"),
                    DoclingProfile::Media => bail!("所选格式需要先安装音视频组件"),
                }
            }
        }
        Ok(())
    }
    pub fn status(&self) -> DoclingStatus {
        let mut status = self.status.lock().unwrap().clone();
        status.installed = self.installed();
        status.formats = formats();
        status.builtin_formats = builtin_formats();
        status.profiles = [DoclingProfile::Documents, DoclingProfile::Media]
            .into_iter()
            .map(|id| DoclingProfileStatus {
                id,
                installed: self.has_profile(id),
                extensions: formats()
                    .into_iter()
                    .flat_map(|(_, extensions)| extensions)
                    .filter(|extension| {
                        MEDIA_EXTENSIONS.contains(&extension.as_str())
                            == (id == DoclingProfile::Media)
                    })
                    .collect(),
            })
            .collect();
        status
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn set_component_source(&self, path: PathBuf) -> Result<()> {
        if !path.join("manifest.json").is_file() {
            bail!("预构建组件缺少 manifest.json")
        }
        *self.source.lock().unwrap() = Some(path.canonicalize()?);
        Ok(())
    }
    pub fn uninstall(&self) -> Result<()> {
        let mut status = self.status.lock().unwrap();
        if status.running {
            bail!("Docling 正在安装，请先取消下载再卸载")
        }
        let _usage = self
            .usage
            .try_write()
            .map_err(|error| anyhow::anyhow!("Docling 正在使用，暂时无法卸载：{error}"))?;
        self.ocr_workers.clear();
        fs::remove_dir_all(&self.home).context("无法删除 Docling 组件目录")?;
        *status = DoclingStatus {
            stage: "uninstalled".into(),
            ..Default::default()
        };
        Ok(())
    }

    fn progress(&self, stage: &str, message: impl Into<String>) {
        self.packages.lock().unwrap().clear();
        let mut status = self.status.lock().unwrap();
        status.stage = stage.into();
        status.message = message.into();
        status.downloaded = 0;
        status.total = None;
        status.percent = None;
        status.packages_done = 0;
        status.packages_total = 0;
    }
    pub fn start(self: &Arc<Self>) -> Result<()> {
        self.start_profiles(vec![DoclingProfile::Documents])
    }
    /// An explicit offline source applies to this installation only. Never
    /// register it as the source of later online retries or media installs.
    /// A folder with a manifest is a complete component; a folder holding only
    /// the extracted uv archive replaces the first download, and the remaining
    /// steps still install online.
    pub fn import_component(self: &Arc<Self>, path: PathBuf) -> Result<()> {
        if path.join("manifest.json").is_file() {
            let source = OfflineImport::Component(path.canonicalize()?);
            return self.start_profiles_from(vec![DoclingProfile::Documents], Some(source));
        }
        // Extracting the archive may add one folder level, e.g. uv-<platform>/uv.
        let uv = walkdir::WalkDir::new(&path)
            .max_depth(2)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name() == UV_EXECUTABLE && entry.path().is_file())
            .min_by_key(|entry| entry.depth());
        let Some(uv) = uv else {
            bail!("所选目录既不是完整组件（缺少 manifest.json），也不包含 {UV_EXECUTABLE}，请选择解压后的目录")
        };
        let source = OfflineImport::Uv(uv.path().canonicalize()?);
        self.start_profiles_from(vec![DoclingProfile::Documents], Some(source))
    }
    pub fn start_profiles(self: &Arc<Self>, profiles: Vec<DoclingProfile>) -> Result<()> {
        self.start_profiles_from(profiles, None)
    }
    fn start_profiles_from(
        self: &Arc<Self>,
        profiles: Vec<DoclingProfile>,
        offline: Option<OfflineImport>,
    ) -> Result<()> {
        let profiles: Vec<_> = [DoclingProfile::Documents, DoclingProfile::Media]
            .into_iter()
            .filter(|profile| profiles.contains(profile))
            .collect();
        if profiles.is_empty() {
            bail!("请选择要安装的转换组件")
        }
        let mut status = self.status.lock().unwrap();
        if status.running {
            bail!("Docling 正在下载")
        }
        if profiles.iter().all(|profile| self.has_profile(*profile)) {
            return Ok(());
        }
        self.cancel.store(false, Ordering::Relaxed);
        self.packages.lock().unwrap().clear();
        *status = DoclingStatus {
            running: true,
            requested_profiles: profiles.clone(),
            stage: "runtime".into(),
            ..Default::default()
        };
        drop(status);
        let this = self.clone();
        std::thread::spawn(move || {
            let result = profiles.into_iter().try_for_each(|profile| {
                if this.has_profile(profile) {
                    return Ok(());
                }
                let view = this.profile_view(profile);
                match &offline {
                    Some(OfflineImport::Component(source)) => {
                        *view.source.lock().unwrap() = Some(source.clone());
                    }
                    Some(OfflineImport::Uv(uv)) => view.import_uv(uv)?,
                    None => {}
                }
                view.install()
            });
            let mut status = this.status.lock().unwrap();
            status.running = false;
            match result {
                Ok(()) => {
                    status.stage = "done".into();
                    status.message = "Docling 已准备就绪".into();
                }
                Err(error) if this.cancel.load(Ordering::Relaxed) => {
                    status.stage = "cancelled".into();
                    status.error = None;
                    status.message = error.to_string();
                }
                Err(error) => {
                    status.stage = "error".into();
                    status.error = Some(format!("{error:#}"));
                }
            }
        });
        Ok(())
    }
    fn profile_view(&self, profile: DoclingProfile) -> Self {
        Self {
            home: if profile == DoclingProfile::Media {
                self.home.join("media")
            } else {
                self.home.clone()
            },
            ocr_workers: self.ocr_workers.clone(),
            source: Mutex::new(self.source.lock().unwrap().clone()),
            status: self.status.clone(),
            packages: self.packages.clone(),
            cancel: self.cancel.clone(),
            profile,
            usage: RwLock::new(()),
        }
    }
    fn command(&self, executable: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(executable);
        let mut search_path = vec![self.home.join("bin")];
        if let Some(path) = std::env::var_os("PATH") {
            search_path.extend(std::env::split_paths(&path));
        }
        command.env(
            "PATH",
            std::env::join_paths(search_path).expect("component directory has a valid PATH"),
        );
        command
            .env("UV_TOOL_DIR", self.home.join("tools"))
            .env("UV_TOOL_BIN_DIR", self.home.join("bin"))
            .env("UV_PYTHON_INSTALL_DIR", self.home.join("python"))
            .env("UV_CACHE_DIR", self.home.join("cache"))
            .env("HF_HOME", self.home.join("huggingface"))
            .env("PYTHONUNBUFFERED", "1")
            // Docling writes UTF-8; Windows otherwise decodes files and stderr with the ANSI code page.
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .env("NO_COLOR", "1");
        if cfg!(target_os = "linux") || cfg!(target_os = "windows") {
            command.env("UV_TORCH_BACKEND", "cpu");
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command
    }
    fn install(&self) -> Result<()> {
        fs::create_dir_all(&self.home)?;
        fs::write(self.home.join("install-diagnostics.log"), [])?;
        let source = self
            .source
            .lock()
            .unwrap()
            .clone()
            .or_else(|| std::env::var_os("SAG_DOCLING_COMPONENT_DIR").map(PathBuf::from));
        let cancel = || self.cancel.load(Ordering::Relaxed);
        if let Some(source) = source.filter(|_| self.profile == DoclingProfile::Documents) {
            self.progress("dependencies", "正在校验并准备预构建文档组件…");
            return crate::docling_component::stage(&self.home, &source, &cancel, |runtime| {
                self.probe_component(runtime, &cancel)
            });
        }
        let uv = self.bootstrap_uv()?;
        self.install_online(&uv)
    }
    fn install_online(&self, uv: &Path) -> Result<()> {
        let cancel = || self.cancel.load(Ordering::Relaxed);
        let components = self.home.join("components");
        fs::create_dir_all(&components)?;
        let staging = tempfile::Builder::new()
            .prefix(if self.profile == DoclingProfile::Media {
                "media-online-"
            } else {
                "documents-online-"
            })
            .tempdir_in(&components)?;
        let python_home = staging.path().join("python");
        let packages = staging.path().join("site-packages");
        let models = staging.path().join("models");
        let mut command = self.command(uv);
        command.env("UV_PYTHON_INSTALL_DIR", &python_home).args([
            "python",
            "install",
            PYTHON_VERSION,
            "--no-bin",
            "--no-registry",
            "--no-config",
        ]);
        self.progress("runtime", "正在下载 Python 运行环境…");
        self.run_install(&mut command, Duration::from_secs(600))
            .map_err(|error| self.with_hint(error, |error| anyhow!(
                "无法下载 Python 运行环境：{error:#}。如无法访问 GitHub，可设置 UV_PYTHON_INSTALL_MIRROR 环境变量（例如 https://registry.npmmirror.com/-/binary/python-build-standalone），重启 SAG 服务后重试。"
            )))?;
        let python = walkdir::WalkDir::new(&python_home)
            .max_depth(4)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .find(|entry| {
                entry.file_name()
                    == if cfg!(windows) {
                        "python.exe"
                    } else {
                        "python3.12"
                    }
                    && entry.path().is_file()
            })
            .context("下载的 Python 运行环境缺少解释器")?
            .into_path();
        let mut command = self.command(uv);
        command
            .env("UV_PYTHON_INSTALL_DIR", &python_home)
            .args(["pip", "install", "--python"])
            .arg(&python)
            .args(["--managed-python", "--no-python-downloads", "--target"])
            .arg(&packages)
            .args(["--link-mode", "copy", "--compile-bytecode", "--no-config"]);
        if self.profile == DoclingProfile::Media {
            let lock = staging.path().join("requirements.txt");
            fs::write(&lock, media_requirements()?)?;
            command
                .args(["--require-hashes", "--requirements"])
                .arg(&lock);
            if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
                command.arg("--no-build");
            } else {
                let constraints = staging.path().join("build-constraints.txt");
                fs::write(
                    &constraints,
                    include_str!("../../scripts/docling/build-constraints-media.txt"),
                )?;
                command
                    .args([
                        "--only-binary",
                        ":all:",
                        "--no-binary",
                        "openai-whisper",
                        "--build-constraints",
                    ])
                    .arg(constraints);
            }
        } else {
            let lock = staging.path().join("requirements.txt");
            fs::write(&lock, document_requirements()?)?;
            command
                .args(["--no-build", "--require-hashes", "--requirements"])
                .arg(lock);
        }
        self.progress(
            "dependencies",
            if self.profile == DoclingProfile::Media {
                "正在下载并安装音视频组件…"
            } else {
                "正在下载并安装 Docling 文档组件…"
            },
        );
        self.run_install(&mut command, Duration::from_secs(1800))
            .map_err(|error| self.with_hint(error, |error| anyhow!(
                "无法下载组件依赖：{error:#}。如无法访问 PyPI，可设置 UV_DEFAULT_INDEX 环境变量（例如 https://pypi.tuna.tsinghua.edu.cn/simple），重启 SAG 服务后重试。"
            )))?;
        let mut command = self.command(&python);

        command
            .env("PYTHONPATH", &packages)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("HF_HOME", staging.path().join("huggingface"))
            .env_remove("HF_HUB_OFFLINE")
            .env_remove("TRANSFORMERS_OFFLINE")
            .env_remove("HF_DATASETS_OFFLINE");
        if self.profile == DoclingProfile::Media {
            self.progress("models", "正在下载音视频转写模型和 FFmpeg…");
            let setup = staging.path().join("media-setup.py");
            fs::write(&setup, MEDIA_SETUP)?;
            command
                .arg(&setup)
                .arg("--download-media-models")
                .arg(if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
                    "mlx"
                } else {
                    "native"
                })
                .arg(staging.path());
        } else {
            self.progress("models", "正在下载文档识别模型…");
            // Unit tests use a local uv fixture and must not depend on the
            // public model host being reachable from the test environment.
            #[cfg(not(test))]
            self.check_model_host()?;
            command
                .args([
                    "-m",
                    "docling.cli.tools",
                    "models",
                    "download",
                    "layout",
                    "tableformer",
                    "rapidocr",
                    "--rapidocr-backend-lang",
                    "onnxruntime:ch",
                    "--output-dir",
                ])
                .arg(&models);
        }
        self.run_model_download(&mut command, Duration::from_secs(1800))
            .map_err(|error| {
                self.with_hint(error, |error| {
                    anyhow!(
                        "无法下载文档识别模型：{error:#}。请检查网络、磁盘和内存占用后重试；如果一直失败，可导入离线文档组件。"
                    )
                })
            })?;
        crate::docling_component::write_manifest_for(
            staging.path(),
            &python,
            &cancel,
            self.profile,
        )?;
        crate::docling_component::activate_for(
            &self.home,
            staging,
            &cancel,
            |runtime| self.probe_component(runtime, &cancel),
            self.profile,
        )
    }
    /// Fails fast when the model host is unreachable. Without this the
    /// Hugging Face client retries silently and the user watches a frozen
    /// 0% for up to half an hour before any error appears.
    #[cfg_attr(test, allow(dead_code))]
    fn check_model_host(&self) -> Result<()> {
        let endpoint = std::env::var("HF_ENDPOINT")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "https://huggingface.co".into());
        let url = format!("{}/api/models/docling-project/docling-layout-heron", endpoint.trim_end_matches('/'));
        self.status.lock().unwrap().message = format!("正在连接模型服务 {endpoint}…");
        let probe: Result<std::result::Result<(), reqwest::Error>> = tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
            let client = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(15))
                .build()?;
            let request = client.head(&url).send();
            tokio::pin!(request);
            loop {
                tokio::select! {
                    result = &mut request => break Ok(result.map(|_| ())),
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {
                        if self.cancel.load(Ordering::Relaxed) { bail!("已取消下载，可稍后重试") }
                    }
                }
            }
        });
        match probe? {
            Ok(()) => Ok(()),
            Err(error) => bail!(
                "无法连接模型服务 {endpoint}：{error}。请检查网络或代理；如需镜像，可在启动前设置 HF_ENDPOINT 环境变量（例如 https://hf-mirror.com）。"
            ),
        }
    }
    fn run_install(&self, command: &mut Command, timeout: Duration) -> Result<()> {
        let cancel = || self.cancel.load(Ordering::Relaxed);
        let permit = crate::docling_budget::acquire(&cancel)?;
        self.run_install_with_budget(command, timeout, permit.bytes)
    }
    fn run_model_download(&self, command: &mut Command, timeout: Duration) -> Result<()> {
        let cancel = || self.cancel.load(Ordering::Relaxed);
        // Reserve once: retries share one execution deadline after admission.
        // Keeping the candidate allows Docling to reuse completed model files.
        let permit = crate::docling_budget::acquire(&cancel)?;
        let start = std::time::Instant::now();
        for attempt in 1..=3 {
            let remaining = timeout.saturating_sub(start.elapsed());
            let result = self.run_install_with_budget(command, remaining, permit.bytes);
            match result {
                Err(error)
                    if attempt < 3
                        && error
                            .downcast_ref::<crate::process_supervisor::Failure>()
                            .is_some_and(|failure| {
                                failure.kind == crate::process_supervisor::FailureKind::Exit
                            }) =>
                {
                    self.progress("models", "模型下载中断，正在重试…");
                }
                result => return result,
            }
        }
        unreachable!("the third model download attempt always returns")
    }
    fn run_install_with_budget(
        &self,
        command: &mut Command,
        timeout: Duration,
        bytes: u64,
    ) -> Result<()> {
        let cancel = || self.cancel.load(Ordering::Relaxed);
        let mut diagnostics = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.home.join("install-diagnostics.log"))?;
        let mut remaining = DIAGNOSTIC_LIMIT.saturating_sub(diagnostics.metadata()?.len());
        crate::process_supervisor::run(
            command,
            crate::process_supervisor::Limits {
                timeout,
                memory_bytes: Some(install_memory_limit(bytes)),
            },
            &cancel,
            |line| {
                if remaining >= line.len() as u64 + 1 {
                    let _ = diagnostics.write_all(line);
                    let _ = diagnostics.write_all(b"\n");
                    remaining -= line.len() as u64 + 1;
                }
                self.update_download_line(line);
            },
        )
        .map_err(Into::into)
    }
    /// Keeps cancellation messages intact; other failures gain a recovery hint.
    fn with_hint(
        &self,
        error: anyhow::Error,
        hint: impl FnOnce(anyhow::Error) -> anyhow::Error,
    ) -> anyhow::Error {
        if self.cancel.load(Ordering::Relaxed) {
            error
        } else {
            hint(error)
        }
    }
    fn uv_version(&self, executable: &Path) -> Option<String> {
        let mut version = String::new();
        crate::process_supervisor::run(
            self.command(executable).arg("--version"),
            crate::process_supervisor::Limits {
                timeout: Duration::from_secs(15),
                memory_bytes: Some(256 * 1024 * 1024),
            },
            &|| self.cancel.load(Ordering::Relaxed),
            |line| {
                if version.len() < 2048 {
                    version.push_str(&String::from_utf8_lossy(line));
                }
            },
        )
        .ok()?;
        let mut words = version.split_whitespace();
        (words.next() == Some("uv")).then(|| words.next().map(str::to_owned))?
    }
    /// Installs a uv the user downloaded manually, so the GitHub download in
    /// `bootstrap_uv` is skipped. The source folder is left untouched.
    fn import_uv(&self, source: &Path) -> Result<()> {
        self.progress("runtime", "正在校验导入的运行环境管理器…");
        let url = uv_download_url()?;
        if fs::metadata(source)?.len() > 128 * 1024 * 1024 {
            bail!("运行环境管理器文件过大")
        }
        fs::create_dir_all(&self.home)?;
        // Check the copy, not the selected file: a byte copy drops the
        // browser's quarantine/Mark-of-the-Web attributes that can block
        // running a manually downloaded binary. Keep `.exe` so Windows runs it.
        let mut candidate = tempfile::Builder::new()
            .suffix(if cfg!(windows) { ".exe" } else { "" })
            .tempfile_in(&self.home)?;
        std::io::copy(&mut fs::File::open(source)?, candidate.as_file_mut())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            candidate
                .as_file()
                .set_permissions(fs::Permissions::from_mode(0o755))?;
        }
        candidate.as_file().sync_all()?;
        // Windows cannot run a file while this process holds it open for writing.
        let candidate = candidate.into_temp_path();
        let version = self.uv_version(&candidate);
        if self.cancel.load(Ordering::Relaxed) {
            bail!("已取消下载，可稍后重试")
        }
        match version {
            Some(version) if version == UV_VERSION => {}
            Some(version) => bail!(
                "导入的 uv 版本为 {version}，当前需要 {UV_VERSION}。请用浏览器下载 {url}，解压后重新导入。"
            ),
            None => bail!(
                "所选的 {UV_EXECUTABLE} 无法运行或不是 uv。请用浏览器下载 {url}，解压后重新导入。"
            ),
        }
        candidate
            .persist(self.home.join(UV_EXECUTABLE))
            .map_err(|error| error.error)?;
        Ok(())
    }
    fn bootstrap_uv(&self) -> Result<PathBuf> {
        let executable = self.home.join(UV_EXECUTABLE);
        if executable.is_file() && self.uv_version(&executable).as_deref() == Some(UV_VERSION) {
            return Ok(executable);
        }
        let url = uv_download_url()?;
        self.progress("runtime", "正在下载运行环境管理器…");
        // Only the document installer offers the import entry.
        let mut archive = self.download_archive(&url).map_err(|error| {
            if self.profile != DoclingProfile::Documents {
                return error;
            }
            self.with_hint(error, |error| anyhow!(
                "无法下载运行环境管理器：{error:#}。可用浏览器下载 {url}，解压后点击“导入离线组件”并选择解压后的目录。"
            ))
        })?;
        use std::io::Seek;
        archive.rewind()?;
        let mut candidate = tempfile::NamedTempFile::new_in(&self.home)?;
        let mut copy = |input: &mut dyn Read, size: u64| -> Result<()> {
            if size > 128 * 1024 * 1024 {
                bail!("运行环境管理器文件过大")
            }
            let mut buffer = [0u8; 64 * 1024];
            let mut copied = 0u64;
            loop {
                if self.cancel.load(Ordering::Relaxed) {
                    bail!("已取消下载，可稍后重试")
                }
                let n = input.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                copied += n as u64;
                if copied > 128 * 1024 * 1024 {
                    bail!("运行环境管理器文件过大")
                }
                candidate.write_all(&buffer[..n])?;
            }
            Ok(())
        };
        if cfg!(windows) {
            let mut zip = zip::ZipArchive::new(archive)?;
            let mut file = zip.by_name("uv.exe")?;
            let size = file.size();
            copy(&mut file, size)?;
        } else {
            let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(archive));
            let mut found = false;
            let mut expanded = 0u64;
            for entry in archive.entries()? {
                if self.cancel.load(Ordering::Relaxed) {
                    bail!("已取消下载，可稍后重试")
                }
                let mut entry = entry?;
                expanded = expanded.saturating_add(entry.header().size()?);
                if expanded > 256 * 1024 * 1024 {
                    bail!("运行环境下载包过大")
                }
                if entry.path()?.file_name().is_some_and(|name| name == "uv")
                    && entry.header().entry_type().is_file()
                {
                    let size = entry.header().size()?;
                    copy(&mut entry, size)?;
                    found = true;
                    break;
                }
            }
            if !found {
                bail!("运行环境下载包中缺少 uv")
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                candidate
                    .as_file()
                    .set_permissions(fs::Permissions::from_mode(0o755))?;
            }
        }
        candidate.as_file().sync_all()?;
        candidate
            .persist(&executable)
            .map_err(|error| error.error)?;
        Ok(executable)
    }
    fn probe_component(
        &self,
        runtime: &crate::docling_component::Runtime,
        cancel: &dyn Fn() -> bool,
    ) -> Result<()> {
        self.progress(
            "models",
            if self.profile == DoclingProfile::Media {
                "正在离线验证音频转写、视频采帧和 FFmpeg…"
            } else {
                "正在离线验证 OCR、布局和表格模型…"
            },
        );
        let permit = crate::docling_budget::acquire(cancel)?;
        let probe = tempfile::tempdir()?;
        let source = probe.path().join(if self.profile == DoclingProfile::Media {
            "probe.wav"
        } else {
            "probe.pdf"
        });
        if self.profile == DoclingProfile::Media {
            // Synthetic 16 kHz mono speech generated with macOS Samantha; no user recordings.
            fs::write(&source, include_bytes!("../assets/media-probe.wav"))?;
        } else {
            fs::write(&source, smoke_pdf())?;
        }
        let output = probe.path().join("probe.md");
        let runner = probe.path().join("runner.py");
        fs::write(
            &runner,
            if self.profile == DoclingProfile::Media {
                MEDIA_RUNNER
            } else {
                RUNNER
            },
        )?;
        let mut command = self.runtime_command(runtime, &runner);
        command
            .arg(&source)
            .arg("--output-file")
            .arg(&output)
            .arg("--artifacts-path")
            .arg(&runtime.models)
            .arg("--probe");
        fs::create_dir_all(&self.home)?;
        let mut diagnostics = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.home.join("install-diagnostics.log"))?;
        let mut remaining = DIAGNOSTIC_LIMIT.saturating_sub(diagnostics.metadata()?.len());
        crate::process_supervisor::run(
            &mut command,
            crate::process_supervisor::Limits {
                // Installation includes one-time library/model cold initialization.
                timeout: Duration::from_secs(240),
                memory_bytes: Some(permit.bytes),
            },
            cancel,
            |line| {
                if remaining >= line.len() as u64 + 1 {
                    let _ = diagnostics.write_all(line);
                    let _ = diagnostics.write_all(b"\n");
                    remaining -= line.len() as u64 + 1;
                }
                self.update_download_line(line);
            },
        )?;
        if !output.is_file() || fs::read_to_string(output)?.trim().is_empty() {
            bail!("组件未完成真实文档转换")
        }
        if self.profile == DoclingProfile::Documents {
            let source = probe.path().join("probe.png");
            fs::write(&source, include_bytes!("../assets/image-probe.png"))?;
            let output = probe.path().join("image-probe.md");
            let mut command = self.runtime_command(runtime, &runner);
            command
                .arg(&source)
                .arg("--output-file")
                .arg(&output)
                .arg("--artifacts-path")
                .arg(&runtime.models)
                .arg("--probe");
            crate::process_supervisor::run(
                &mut command,
                crate::process_supervisor::Limits {
                    timeout: Duration::from_secs(240),
                    memory_bytes: Some(permit.bytes),
                },
                cancel,
                |line| {
                    if remaining >= line.len() as u64 + 1 {
                        let _ = diagnostics.write_all(line);
                        let _ = diagnostics.write_all(b"\n");
                        remaining -= line.len() as u64 + 1;
                    }
                    self.update_download_line(line);
                },
            )?;
            if !output.is_file() || fs::read_to_string(output)?.trim().is_empty() {
                bail!("组件未完成真实图片 OCR 转换")
            }
        }
        Ok(())
    }
    fn runtime_command(
        &self,
        runtime: &crate::docling_component::Runtime,
        runner: &Path,
    ) -> Command {
        let mut command = self.command(&runtime.python);
        let mut paths = vec![runtime.root.join("bin")];
        if let Some(path) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&path));
        }
        command.env(
            "PATH",
            std::env::join_paths(paths).expect("runtime PATH is valid"),
        );
        command
            .arg(runner)
            .env("PYTHONPATH", &runtime.site_packages)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("HF_HUB_OFFLINE", "1")
            .env("TRANSFORMERS_OFFLINE", "1")
            .env("HF_DATASETS_OFFLINE", "1");
        command
    }
    fn download_archive(&self, url: &str) -> Result<fs::File> {
        use futures_util::StreamExt;
        tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
            if self.cancel.load(Ordering::Relaxed) { bail!("已取消下载，可稍后重试") }
            self.status.lock().unwrap().downloaded = 0;
            let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(20)).timeout(Duration::from_secs(600)).build()?;
            let request = client.get(url).send();
            tokio::pin!(request);
            let response = loop {
                tokio::select! {
                    result = &mut request => break result?.error_for_status()?,
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {
                        if self.cancel.load(Ordering::Relaxed) { bail!("已取消下载，可稍后重试") }
                    }
                }
            };
            if response.content_length().is_some_and(|size| size > 64 * 1024 * 1024) { bail!("运行环境下载包过大") }
            self.status.lock().unwrap().total = response.content_length();
            let mut stream = response.bytes_stream();
            let mut archive = tempfile::tempfile()?;
            loop {
                tokio::select! {
                    chunk = stream.next() => match chunk {
                        Some(chunk) => {
                            let bytes = chunk?;
                            if self.status.lock().unwrap().downloaded.saturating_add(bytes.len() as u64) > 64 * 1024 * 1024 { bail!("运行环境下载包过大") }
                            archive.write_all(&bytes)?;
                            self.status.lock().unwrap().downloaded += bytes.len() as u64;
                        }
                        None => break,
                    },
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {
                        if self.cancel.load(Ordering::Relaxed) { bail!("已取消下载，可稍后重试") }
                    }
                }
            }
            Ok(archive)
        })
    }
    fn update_download_line(&self, raw: &[u8]) -> String {
        static ANSI: LazyLock<regex::Regex> = LazyLock::new(|| {
            regex::Regex::new(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))").unwrap()
        });
        static BYTES: LazyLock<regex::Regex> = LazyLock::new(|| {
            regex::Regex::new(r"(?i)([0-9]+(?:\.[0-9]+)?)\s*(KiB|MiB|GiB|kB|MB|GB|B)\s*/\s*([0-9]+(?:\.[0-9]+)?)\s*(KiB|MiB|GiB|kB|MB|GB|B)").unwrap()
        });
        static PERCENT: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"\b([0-9]{1,3}(?:\.[0-9]+)?)%").unwrap());
        static BAR: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"\s+-{3,}\s+").unwrap());
        static SPACING: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r" {2,}").unwrap());
        // uv without a TTY: "Downloading torch (121.4MiB)" then " Downloaded torch".
        static STARTED: LazyLock<regex::Regex> = LazyLock::new(|| {
            regex::Regex::new(r"(?im)^\s*Downloading\s+(\S+)\s+\(([0-9]+(?:\.[0-9]+)?)\s*(KiB|MiB|GiB|kB|MB|GB|B)\)").unwrap()
        });
        static FINISHED: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"(?im)^\s*Downloaded\s+(\S+)\s*$").unwrap());
        static SETTLED: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"(?im)^\s*(?:Prepared|Installed)\s+[0-9]+\s+packages?").unwrap());
        let text = String::from_utf8_lossy(raw);
        let clean = ANSI.replace_all(&text, "");
        let clean = BAR.replace_all(&clean, ": ");
        let clean = SPACING.replace_all(&clean, "\n").trim().to_owned();
        if clean.is_empty() {
            return clean;
        }
        let mut status = self.status.lock().unwrap();
        status.message = clean
            .chars()
            .rev()
            .take(2000)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        status.downloaded = 0;
        status.total = None;
        status.percent = None;
        let mut packages = self.packages.lock().unwrap();
        for started in STARTED.captures_iter(&clean) {
            let bytes = (started[2].parse::<f64>().unwrap() * byte_scale(&started[3])) as u64;
            packages.sizes.insert(started[1].to_owned(), bytes);
        }
        for finished in FINISHED.captures_iter(&clean) {
            let name = finished[1].to_owned();
            packages.sizes.entry(name.clone()).or_insert(0);
            packages.done.insert(name);
        }
        if SETTLED.is_match(&clean) {
            let names: Vec<_> = packages.sizes.keys().cloned().collect();
            packages.done.extend(names);
        }
        let downloads: Vec<_> = BYTES.captures_iter(&clean).collect();
        if !downloads.is_empty() {
            status.downloaded = downloads
                .iter()
                .map(|bytes| (bytes[1].parse::<f64>().unwrap() * byte_scale(&bytes[2])) as u64)
                .sum();
            status.total = Some(
                downloads
                    .iter()
                    .map(|bytes| (bytes[3].parse::<f64>().unwrap() * byte_scale(&bytes[4])) as u64)
                    .sum(),
            );
            status.percent = None;
        } else if let Some(percent) = PERCENT.captures(&clean) {
            status.percent = Some(percent[1].parse::<f64>().unwrap().min(100.0));
            status.total = None;
            status.downloaded = 0;
        } else if !packages.sizes.is_empty() {
            status.downloaded = packages.downloaded();
            status.total = Some(packages.total());
            status.percent = None;
        }
        status.packages_done = packages.done.len() as u32;
        status.packages_total = packages.sizes.len() as u32;
        clean
    }
    #[cfg(test)]
    fn run(&self, command: &mut Command, installing: bool) -> Result<()> {
        crate::process_supervisor::run(
            command,
            crate::process_supervisor::Limits {
                timeout: Duration::from_secs(if installing { 1800 } else { 300 }),
                memory_bytes: if installing {
                    Some(2 * 1024 * 1024 * 1024)
                } else {
                    Some(4 * 1024 * 1024 * 1024)
                },
            },
            &|| self.cancel.load(Ordering::Relaxed),
            |line| {
                if installing {
                    self.update_download_line(line);
                }
            },
        )
        .map_err(Into::into)
    }
    pub fn convert(&self, source: &Path, output: &Path) -> Result<()> {
        self.convert_cancellable(source, output, || false)
    }
    fn convert_image_with_worker(
        &self,
        source: &Path,
        output: &Path,
        runtime: &sag_protocol::runtime::RuntimeSettings,
        cancel: &dyn Fn() -> bool,
    ) -> Result<crate::docling_worker::ImageOutcome> {
        let component =
            crate::docling_component::active_for(&self.home, DoclingProfile::Documents).ok();
        let legacy_python = self.home.join(if cfg!(windows) {
            "tools/docling-slim/Scripts/python.exe"
        } else {
            "tools/docling-slim/bin/python"
        });
        let Some((runtime_root, models)) = component
            .as_ref()
            .map(|component| (component.root.clone(), component.models.clone()))
            .or_else(|| {
                legacy_python
                    .is_file()
                    .then(|| (legacy_python.clone(), self.home.join("models")))
            })
        else {
            return Ok(crate::docling_worker::ImageOutcome::Fallback);
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(runtime.docling_timeout_secs);
        let result = self.ocr_workers.convert(
            &runtime_root,
            source,
            output,
            deadline,
            runtime,
            cancel,
            || {
                let mut runner = tempfile::NamedTempFile::new()?;
                runner.write_all(RUNNER.as_bytes())?;
                runner.flush()?;
                let runner = runner.into_temp_path();
                let runner_path = runner.as_ref() as &Path;
                let mut command = if let Some(component) = &component {
                    self.runtime_command(component, runner_path)
                } else {
                    let mut command = self.command(&legacy_python);
                    command
                        .arg(runner_path)
                        .env("PYTHONDONTWRITEBYTECODE", "1")
                        .env("HF_HUB_OFFLINE", "1")
                        .env("TRANSFORMERS_OFFLINE", "1")
                        .env("HF_DATASETS_OFFLINE", "1");
                    command
                };
                command
                    .arg("--worker")
                    .arg("--artifacts-path")
                    .arg(&models);
                let waiting_cancel = || {
                    self.ocr_workers.release_outdated(runtime);
                    cancel()
                };
                let permit = crate::docling_budget::acquire_bytes_with_runtime(
                    crate::docling_budget::IMAGE_WORKER_BYTES,
                    runtime,
                    &waiting_cancel,
                    deadline,
                )?;
                Ok(crate::docling_worker::SpawnSpec {
                    command,
                    runner,
                    runtime_root: runtime_root.clone(),
                    permit,
                    runtime: runtime.clone(),
                })
            },
        );
        match result {
            Ok(outcome) => Ok(outcome),
            Err(error) if image_worker_must_stop(&error) => Err(error),
            Err(error) => {
                eprintln!("图片 OCR Worker 不可用，回退到 Docling：{error:#}");
                Ok(crate::docling_worker::ImageOutcome::Fallback)
            }
        }
    }
    pub(crate) fn pdf_session(&self, runtime: &sag_protocol::runtime::RuntimeSettings) -> PdfSession<'_> {
        PdfSession::new(self, runtime)
    }
    pub fn convert_cancellable(
        &self,
        source: &Path,
        output: &Path,
        cancel: impl Fn() -> bool + Sync,
    ) -> Result<()> {
        self.convert_with_runtime(source, output, &Default::default(), cancel)
    }
    pub fn convert_with_runtime(
        &self, source: &Path, output: &Path,
        runtime: &sag_protocol::runtime::RuntimeSettings,
        cancel: impl Fn() -> bool + Sync,
    ) -> Result<()> {
        let mut handoff = None;
        if crate::anydoc::handles(source) {
            match crate::anydoc::convert_with_runtime(source, output, runtime, &cancel) {
                Ok(crate::anydoc::Outcome::Converted) => return Ok(()),
                Ok(crate::anydoc::Outcome::NeedsOcr(detail)) => {
                    if !self.has_profile(DoclingProfile::Documents) {
                        return Err(ComponentMissing(format!("扫描件需要 OCR（{detail}），请在文件设置中安装图片识别组件后重试")).into());
                    }
                    handoff = Some(format!("内置转换需要 OCR：{detail}"));
                }
                Err(error) if crate::anydoc::is_handoff_error(&error)
                    && self.has_profile(DoclingProfile::Documents) =>
                {
                    handoff = Some(format!("内置转换失败：{error:#}"));
                }
                Err(error) => return Err(error),
            }
        }
        self.convert_docling_runtime(source, output, runtime, cancel, handoff, None)
    }
    pub(crate) fn data_directory(&self) -> &Path { self.home.parent().expect("Docling directory has a parent") }
    pub fn convert_docling_with_runtime(
        &self, source: &Path, output: &Path, runtime: &sag_protocol::runtime::RuntimeSettings,
        cancel: impl Fn() -> bool + Sync,
    ) -> Result<()> { self.convert_docling_runtime(source, output, runtime, cancel, None, None) }
    fn convert_docling_runtime(
        &self, source: &Path, output: &Path, runtime: &sag_protocol::runtime::RuntimeSettings,
        cancel: impl Fn() -> bool + Sync, handoff: Option<String>,
        session: Option<&PdfSession<'_>>,
    ) -> Result<()> {
        let _usage = session.is_none().then(|| self.usage.read().unwrap());
        let profile = required_profile(source);
        if !self.has_profile(profile) {
            let message = if profile == DoclingProfile::Media {
                "请在文件设置中安装音视频组件后重试"
            } else if handoff.is_some() || is_image(source) {
                "请在文件设置中安装图片识别组件后重试"
            } else {
                "此格式需要 Docling 文档组件，请在文件设置中安装图片识别组件后重试"
            };
            return Err(ComponentMissing(message.into()).into());
        }
        let component_home = if profile == DoclingProfile::Media {
            self.home.join("media")
        } else {
            self.home.clone()
        };
        let cancelled = || cancel();
        if fs::metadata(source)?.len() > runtime.conversion_input_mib as u64 * 1024 * 1024 {
            return Err(crate::process_supervisor::Failure::new(
                crate::process_supervisor::FailureKind::Memory,
                format!("文档超过 {} MiB 转换输入限制", runtime.conversion_input_mib),
            )
            .into());
        }
        let parent = output.parent().context("转换文件缺少父目录")?;
        fs::create_dir_all(parent)?;
        // Atomic publication: partial/failed conversion must never become indexed Markdown.
        let staging = tempfile::tempdir_in(parent)?;
        let temporary = staging.path().join("document.md");
        let runner = staging.path().join("runner.py");
        fs::write(
            &runner,
            if profile == DoclingProfile::Media {
                MEDIA_RUNNER
            } else {
                RUNNER
            },
        )?;
        let deadline =
            std::time::Instant::now() + Duration::from_secs(runtime.docling_outer_timeout_secs());
        let read_temporary = || -> Result<String> {
            let metadata = fs::metadata(&temporary).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    anyhow::Error::new(crate::process_supervisor::Failure::new(
                        crate::process_supervisor::FailureKind::Compatibility,
                        "Docling 未生成 Markdown",
                    ))
                } else {
                    error.into()
                }
            })?;
            if metadata.len() > runtime.conversion_output_mib as u64 * 1024 * 1024 {
                return Err(crate::process_supervisor::Failure::new(
                    crate::process_supervisor::FailureKind::Memory,
                    format!("转换输出超过 {} MiB 限制", runtime.conversion_output_mib),
                )
                .into());
            }
            let content = fs::read_to_string(&temporary).context("Docling 未生成 Markdown")?;
            if content.trim().is_empty() {
                bail!("Docling 未提取到正文")
            }
            Ok(content)
        };
        let convert = |backend: Option<&str>| -> Result<String> {
            if backend.is_none() {
                if let Some(session) = session {
                    match session.convert_raw(source, &temporary, deadline, &cancelled)? {
                        crate::docling_worker::ImageOutcome::Converted => return read_temporary(),
                        crate::docling_worker::ImageOutcome::Fallback => {}
                    }
                }
            }
            if backend.is_none() && profile == DoclingProfile::Documents && is_image(source) {
                match self.convert_image_with_worker(source, &temporary, runtime, &cancelled)? {
                    crate::docling_worker::ImageOutcome::Converted => return read_temporary(),
                    crate::docling_worker::ImageOutcome::Fallback => {}
                }
            }
            let permit = crate::docling_budget::acquire_with_runtime(runtime, &cancelled)?;
            let mut command;
            if let Ok(runtime) = crate::docling_component::active_for(&component_home, profile) {
                command = self.runtime_command(&runtime, &runner);
                command
                    .arg(source)
                    .arg("--artifacts-path")
                    .arg(&runtime.models)
                    .arg("--output-file")
                    .arg(&temporary);
            } else {
                let python = self.home.join(if cfg!(windows) {
                    "tools/docling-slim/Scripts/python.exe"
                } else {
                    "tools/docling-slim/bin/python"
                });
                if python.is_file() {
                    command = self.command(python);
                    command
                        .arg(&runner)
                        .arg(source)
                        .arg("--artifacts-path")
                        .arg(self.home.join("models"))
                        .arg("--output-file")
                        .arg(&temporary)
                        .env("PYTHONDONTWRITEBYTECODE", "1")
                        .env("HF_HUB_OFFLINE", "1")
                        .env("TRANSFORMERS_OFFLINE", "1");
                } else {
                    // Existing external CLI integrations remain supervised. New
                    // installs always use the verified component Python runner.
                    command = self.command(self.binary("docling"));
                    command
                        .arg(source)
                        .args([
                            "--to",
                            "md",
                            "--image-export-mode",
                            "placeholder",
                            "--abort-on-error",
                            "--document-timeout",
                        ])
                        .arg(runtime.docling_timeout_secs.to_string())
                        .arg("--artifacts-path")
                        .arg(self.home.join("models"))
                        .arg("--output-file")
                        .arg(&temporary);
                }
            }
            command.env("SAG_CONVERSION_SETTINGS", serde_json::to_string(runtime)?);
            if let Some(backend) = backend {
                command.args(["--pdf-backend", backend]);
            }
            crate::process_supervisor::run(
                &mut command,
                crate::process_supervisor::Limits {
                    timeout: deadline.saturating_duration_since(std::time::Instant::now()),
                    memory_bytes: Some(permit.bytes),
                },
                &cancelled,
                |_| {},
            )?;
            read_temporary()
        };
        if let Some(reason) = &handoff {
            eprintln!("{}：交给 Docling 组件转换（{reason}）", source.display());
        }
        let content = match convert(None) {
            Ok(content) => content,
            Err(first)
                if retryable_conversion_error(&first)
                    && source
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf")) =>
            {
                if temporary.exists() {
                    fs::remove_file(&temporary)?;
                }
                convert(Some("pypdfium2")).map_err(|second| {
                    let message =
                        format!("PDF 默认引擎转换失败：{first}\n备用 PDF 引擎转换失败：{second}");
                    second.context(message)
                })?
            }
            Err(error) => return Err(error),
        };
        if cancelled() {
            return Err(crate::process_supervisor::Failure::new(
                crate::process_supervisor::FailureKind::Cancelled,
                "已停止转换，未发布输出",
            )
            .into());
        }
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        // Relative source link survives moving the whole knowledge folder.
        let depth = output
            .ancestors()
            .position(|path| {
                path.file_name()
                    .is_some_and(|name| name == OUTPUT_DIRECTORY)
            })
            .context("转换路径必须位于 .sag-converted")?;
        let root = output.ancestors().nth(depth + 1).unwrap();
        let relative = source
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        let link = format!("{}{}", "../".repeat(depth), relative);
        let link = link
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect::<String>();
        writeln!(
            file,
            "[{}](<{link}>)\n",
            source
                .file_name()
                .unwrap()
                .to_string_lossy()
                .replace(['[', ']'], "")
        )?;
        file.write_all(content.as_bytes())?;
        file.persist(output).map_err(|error| error.error)?;
        Ok(())
    }
}

const RUNNER: &str = include_str!("docling_runner.py");
const MEDIA_RUNNER: &str = include_str!("docling_media_runner.py");
const MEDIA_SETUP: &str = include_str!("docling_media_setup.py");
fn document_requirements() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok(include_str!(
            "../../scripts/docling/requirements-macos-aarch64.txt"
        )),
        ("macos", "x86_64") => Ok(include_str!(
            "../../scripts/docling/requirements-documents-macos-x86_64.txt"
        )),
        ("windows", "x86_64") => Ok(include_str!(
            "../../scripts/docling/requirements-documents-windows-x86_64.txt"
        )),
        ("linux", "x86_64") => Ok(include_str!(
            "../../scripts/docling/requirements-documents-linux-x86_64.txt"
        )),
        ("linux", "aarch64") => Ok(include_str!(
            "../../scripts/docling/requirements-documents-linux-aarch64.txt"
        )),
        _ => bail!("当前系统暂无无需本机编译器的文档组件安装配方"),
    }
}
fn media_requirements() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok(include_str!(
            "../../scripts/docling/requirements-media-macos-aarch64.txt"
        )),
        ("macos", "x86_64") => Ok(include_str!(
            "../../scripts/docling/requirements-media-macos-x86_64.txt"
        )),
        ("windows", "x86_64") => Ok(include_str!(
            "../../scripts/docling/requirements-media-windows-x86_64.txt"
        )),
        ("linux", "x86_64") => Ok(include_str!(
            "../../scripts/docling/requirements-media-linux-x86_64.txt"
        )),
        ("linux", "aarch64") => Ok(include_str!(
            "../../scripts/docling/requirements-media-linux-aarch64.txt"
        )),
        _ => bail!("当前系统暂无无需本机编译器的音视频组件安装配方"),
    }
}
fn image_worker_must_stop(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<crate::process_supervisor::Failure>()
        .is_some_and(|failure| {
            matches!(
                failure.kind,
                crate::process_supervisor::FailureKind::Cancelled
                    | crate::process_supervisor::FailureKind::Timeout
                    | crate::process_supervisor::FailureKind::Memory
            )
        })
}

fn retryable_conversion_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<crate::process_supervisor::Failure>()
        .is_some_and(|failure| {
            failure.kind == crate::process_supervisor::FailureKind::Compatibility
        })
}
fn smoke_pdf() -> Vec<u8> {
    // A real PDF page with text and a bitmap exercises the standard PDF path.
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        { let content="BT /F1 14 Tf 20 150 Td (SAG document probe) Tj ET\nq 30 0 0 30 20 40 cm BI /W 2 /H 2 /CS /G /BPC 8 /F /ASCIIHexDecode ID 00FF00FF> EI Q";format!("<< /Length {} >>\nstream\n{}\nendstream",content.len(),content) },
    ];
    let mut data = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(data.len());
        data.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = data.len();
    data.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in offsets.iter().skip(1) {
        data.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    data.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    data
}

pub fn fingerprint(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn output_path(root: &Path, source: &Path) -> Result<PathBuf> {
    let relative = source.strip_prefix(root)?;
    if relative
        .components()
        .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        bail!("转换文件必须位于所选文件夹内")
    }
    let mut output = root.join(OUTPUT_DIRECTORY).join(relative);
    output.set_file_name(format!(
        "{}.md",
        source.file_name().context("缺少文件名")?.to_string_lossy()
    ));
    // Never follow a user-created output link outside the authorized folder.
    for path in output.ancestors().take_while(|path| *path != root) {
        if path
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            bail!(".sag-converted 中包含符号链接，无法安全写入")
        }
    }
    Ok(output)
}

#[cfg(test)]
#[path = "docling_tests.rs"]
mod tests;
