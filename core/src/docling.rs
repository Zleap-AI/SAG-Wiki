//! Optional external document conversion. The index only reads its Markdown output.
use anyhow::{bail, Context, Result};
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

pub const OUTPUT_DIRECTORY: &str = ".sag-converted";
const VERSION: &str = "2.140.0";
const UV_VERSION: &str = "0.12.1";
const PYTHON_VERSION: &str = "3.12.13";
const DIAGNOSTIC_LIMIT: u64 = 8 * 1024 * 1024;
const MEDIA_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "m4a", "aac", "ogg", "flac", "mp4", "avi", "mov", "mkv", "webm",
];

// Pinned to Docling's FormatToExtensions. Existing native text formats stay native.
const FORMATS: &[(&str, &str)] = &[
    ("PDF", "pdf"),
    ("Word", "docx dotx docm dotm doc dot rtf"),
    ("PowerPoint", "pptx potx ppsx pptm potm ppsm ppt pot pps"),
    ("Excel / CSV", "xlsx xlsm xltx xltm xls xlt csv"),
    ("OpenDocument", "odt ott ods ots odp otp"),
    ("Apple iWork", "pages key numbers"),
    ("图片", "jpg jpeg png tif tiff bmp webp gif"),
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
pub fn formats() -> Vec<(String, Vec<String>)> {
    FORMATS
        .iter()
        .filter_map(|(name, extensions)| {
            let extensions: Vec<_> = extensions
                .split_whitespace()
                .filter(|ext| is_source(Path::new(&format!("file.{ext}"))))
                .map(str::to_owned)
                .collect();
            (!extensions.is_empty()).then(|| ((*name).into(), extensions))
        })
        .collect()
}

pub fn is_source(path: &Path) -> bool {
    if crate::text::is_supported(path) || in_output(path) {
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

pub struct Docling {
    home: PathBuf,
    source: Mutex<Option<PathBuf>>,
    status: Arc<Mutex<DoclingStatus>>,
    cancel: Arc<AtomicBool>,
    profile: DoclingProfile,
    usage: RwLock<()>,
}

impl Docling {
    pub fn new(data: &Path) -> Self {
        Self {
            home: data.join("docling"),
            source: Mutex::new(None),
            status: Arc::new(Mutex::new(DoclingStatus::default())),
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
        let Some(extensions) = extensions else {
            if self.has_profile(DoclingProfile::Documents)
                || self.has_profile(DoclingProfile::Media)
            {
                return Ok(());
            }
            bail!("请在文件设置中安装文档或音视频组件后重试")
        };
        for extension in extensions {
            let extension = extension.to_ascii_lowercase();
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
        fs::remove_dir_all(&self.home).context("无法删除 Docling 组件目录")?;
        *status = DoclingStatus {
            stage: "uninstalled".into(),
            ..Default::default()
        };
        Ok(())
    }

    fn progress(&self, stage: &str, message: impl Into<String>) {
        let mut status = self.status.lock().unwrap();
        status.stage = stage.into();
        status.message = message.into();
        status.downloaded = 0;
        status.total = None;
        status.percent = None;
    }
    pub fn start(self: &Arc<Self>) -> Result<()> {
        self.start_profiles(vec![DoclingProfile::Documents])
    }
    pub fn start_profiles(self: &Arc<Self>, profiles: Vec<DoclingProfile>) -> Result<()> {
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
            source: Mutex::new(self.source.lock().unwrap().clone()),
            status: self.status.clone(),
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
        self.run_install(&mut command, Duration::from_secs(600))?;
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
        self.run_install(&mut command, Duration::from_secs(1800))?;
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
        self.run_model_download(&mut command, Duration::from_secs(1800))?;
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
                memory_bytes: Some(bytes.min(2 * 1024 * 1024 * 1024)),
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
    fn bootstrap_uv(&self) -> Result<PathBuf> {
        let executable = self.home.join(if cfg!(windows) { "uv.exe" } else { "uv" });
        if executable.is_file() {
            let mut version = String::new();
            let result = crate::process_supervisor::run(
                self.command(&executable).arg("--version"),
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
            );
            if result.is_ok() && version.split_whitespace().nth(1) == Some(UV_VERSION) {
                return Ok(executable);
            }
        }
        let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "aarch64-apple-darwin",
            ("macos", "x86_64") => "x86_64-apple-darwin",
            ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
            ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
            ("windows", "aarch64") => "aarch64-pc-windows-msvc",
            ("windows", "x86_64") => "x86_64-pc-windows-msvc",
            _ => bail!("当前系统暂不支持自动安装 Docling"),
        };
        self.progress("runtime", "正在下载运行环境管理器…");
        let suffix = if cfg!(windows) { "zip" } else { "tar.gz" };
        let url = format!(
            "https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/uv-{platform}.{suffix}"
        );
        let mut archive = self.download_archive(&url)?;
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
        let downloads: Vec<_> = BYTES.captures_iter(&clean).collect();
        if !downloads.is_empty() {
            let scale = |unit: &str| match unit.to_ascii_lowercase().as_str() {
                "kib" => 1024.0,
                "mib" => 1048576.0,
                "gib" => 1073741824.0,
                "kb" => 1000.0,
                "mb" => 1e6,
                "gb" => 1e9,
                _ => 1.0,
            };
            status.downloaded = downloads
                .iter()
                .map(|bytes| (bytes[1].parse::<f64>().unwrap() * scale(&bytes[2])) as u64)
                .sum();
            status.total = Some(
                downloads
                    .iter()
                    .map(|bytes| (bytes[3].parse::<f64>().unwrap() * scale(&bytes[4])) as u64)
                    .sum(),
            );
            status.percent = None;
        } else if let Some(percent) = PERCENT.captures(&clean) {
            status.percent = Some(percent[1].parse::<f64>().unwrap().min(100.0));
            status.total = None;
            status.downloaded = 0;
        }
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
    pub fn convert_cancellable(
        &self,
        source: &Path,
        output: &Path,
        cancel: impl Fn() -> bool + Sync,
    ) -> Result<()> {
        let _usage = self.usage.read().unwrap();
        let profile = if source.extension().is_some_and(|ext| {
            MEDIA_EXTENSIONS
                .iter()
                .any(|media| ext.eq_ignore_ascii_case(media))
        }) {
            DoclingProfile::Media
        } else {
            DoclingProfile::Documents
        };
        if !self.has_profile(profile) {
            if profile == DoclingProfile::Media {
                bail!("请在文件设置中安装音视频组件后重试")
            }
            bail!("请在文件设置中下载 Docling 后重试")
        }
        let component_home = if profile == DoclingProfile::Media {
            self.home.join("media")
        } else {
            self.home.clone()
        };
        let cancelled = || cancel();
        let permit = crate::docling_budget::acquire(&cancelled)?;
        if fs::metadata(source)?.len() > 512 * 1024 * 1024 {
            return Err(crate::process_supervisor::Failure::new(
                crate::process_supervisor::FailureKind::Memory,
                "文档超过 512 MiB 转换输入限制",
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
        let deadline = std::time::Instant::now() + Duration::from_secs(300);
        let convert = |backend: Option<&str>| -> Result<String> {
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
                            "270",
                        ])
                        .arg("--artifacts-path")
                        .arg(self.home.join("models"))
                        .arg("--output-file")
                        .arg(&temporary);
                }
            }
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
            if metadata.len() > 64 * 1024 * 1024 {
                return Err(crate::process_supervisor::Failure::new(
                    crate::process_supervisor::FailureKind::Memory,
                    "转换输出超过 64 MiB 限制",
                )
                .into());
            }
            let content = fs::read_to_string(&temporary).context("Docling 未生成 Markdown")?;
            if content.trim().is_empty() {
                bail!("Docling 未提取到正文")
            }
            Ok(content)
        };
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
