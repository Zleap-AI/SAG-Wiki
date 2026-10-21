//! Optional external document conversion. The index only reads its Markdown output.
use anyhow::{bail, Context, Result};
use sag_protocol::DoclingStatus;
use sha2::{Digest, Sha256};
use std::{
    fs, io::{Read, Write}, path::{Path, PathBuf},
    process::Command, sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex, RwLock, LazyLock},
    time::Duration,
};

pub const OUTPUT_DIRECTORY: &str = ".sag-converted";
const VERSION: &str = "2.140.0";
const UV_VERSION: &str = "0.12.1";

// Pinned to Docling's FormatToExtensions. Existing native text formats stay native.
const FORMATS: &[(&str, &str)] = &[
        ("PDF", "pdf"),
        ("Word", "docx dotx docm dotm doc dot rtf"),
        ("PowerPoint", "pptx potx ppsx pptm potm ppsm ppt pot pps"),
        ("Excel / CSV", "xlsx xlsm xltx xltm xls xlt csv"),
        ("OpenDocument", "odt ott ods ots odp otp"),
        ("Apple iWork", "pages key numbers"),
        ("图片", "jpg jpeg png tif tiff bmp webp gif"),
        ("音频", "wav mp3 m4a aac ogg flac"),
        ("视频", "mp4 avi mov mkv webm"),
        ("网页与文本", "html htm xhtml mhtml mht md markdown txt text qmd rmd adoc asciidoc asc vtt tex latex"),
        ("电子书与邮件", "epub eml msg boxnote"),
        ("结构化文档", "xml nxml xbrl dclg dclx json tar.gz ebc ebcdic afp"),
    ];
pub fn formats() -> Vec<(String, Vec<String>)> {
    FORMATS.iter().filter_map(|(name, extensions)| {
        let extensions: Vec<_> = extensions.split_whitespace().filter(|ext| is_source(Path::new(&format!("file.{ext}")))).map(str::to_owned).collect();
        (!extensions.is_empty()).then(|| ((*name).into(), extensions))
    }).collect()
}

pub fn is_source(path: &Path) -> bool {
    if crate::text::is_supported(path) || in_output(path) { return false; }
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
    FORMATS.iter().any(|(_, extensions)| extensions.split_whitespace().any(|ext| name.strip_suffix(ext).is_some_and(|prefix| prefix.ends_with('.'))))
}

pub fn is_selected(path: &Path, formats: &Option<Vec<String>>) -> bool {
    formats.as_ref().is_none_or(|formats| {
        let name = path.file_name().unwrap().to_string_lossy().to_lowercase();
        formats.iter().any(|extension| name.ends_with(&format!(".{extension}")))
    })
}

pub fn in_output(path: &Path) -> bool {
    path.components().any(|part| part.as_os_str() == OUTPUT_DIRECTORY)
}

pub struct Docling {
    home: PathBuf,
    status: Mutex<DoclingStatus>,
    cancel: AtomicBool,
    usage: RwLock<()>,
}

impl Docling {
    pub fn new(data: &Path) -> Self {
        Self { home: data.join("docling"), status: Mutex::new(DoclingStatus::default()), cancel: AtomicBool::new(false), usage: RwLock::new(()) }
    }
    fn binary(&self, name: &str) -> PathBuf {
        self.home.join("bin").join(if cfg!(windows) { format!("{name}.exe") } else { name.into() })
    }
    pub fn installed(&self) -> bool {
        self.home.join("ready").is_file() && self.binary("docling").is_file()
    }
    pub fn status(&self) -> DoclingStatus {
        let mut status = self.status.lock().unwrap().clone();
        status.installed = self.installed();
        status.formats = formats();
        status
    }
    pub fn cancel(&self) { self.cancel.store(true, Ordering::Relaxed); }
    pub fn uninstall(&self) -> Result<()> {
        let mut status = self.status.lock().unwrap();
        if status.running { bail!("Docling 正在安装，请先取消下载再卸载") }
        let _usage = self.usage.try_write().map_err(|error| anyhow::anyhow!("Docling 正在使用，暂时无法卸载：{error}"))?;
        fs::remove_dir_all(&self.home).context("无法删除 Docling 组件目录")?;
        *status = DoclingStatus { stage: "uninstalled".into(), ..Default::default() };
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
        let mut status = self.status.lock().unwrap();
        if status.running { bail!("Docling 正在下载") }
        if self.installed() { return Ok(()); }
        self.cancel.store(false, Ordering::Relaxed);
        *status = DoclingStatus { running: true, stage: "runtime".into(), ..Default::default() };
        drop(status);
        let this = self.clone();
        std::thread::spawn(move || {
            let result = this.install();
            let mut status = this.status.lock().unwrap();
            status.running = false;
            match result {
                Ok(()) => { status.stage = "done".into(); status.message = "Docling 已准备就绪".into(); }
                Err(error) if this.cancel.load(Ordering::Relaxed) => { status.stage = "cancelled".into(); status.error = None; status.message = error.to_string(); }
                Err(error) => { status.stage = "error".into(); status.error = Some(format!("{error:#}")); }
            }
        });
        Ok(())
    }
    fn command(&self, executable: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(executable);
        command.current_dir(&self.home);
        command.env_remove("TRIM_API_TOKEN");
        let mut search_path = vec![self.home.join("bin")];
        if let Some(path) = std::env::var_os("PATH") { search_path.extend(std::env::split_paths(&path)); }
        command.env("PATH", std::env::join_paths(search_path).expect("component directory has a valid PATH"));
        command.env("UV_TOOL_DIR", self.home.join("tools"))
            .env("UV_TOOL_BIN_DIR", self.home.join("bin"))
            .env("UV_PYTHON_INSTALL_DIR", self.home.join("python"))
            .env("UV_CACHE_DIR", self.home.join("cache"))
            .env("HF_HOME", self.home.join("huggingface"))
            .env("PYTHONUNBUFFERED", "1")
            .env("NO_COLOR", "1");
        if cfg!(target_os = "linux") || cfg!(target_os = "windows") {
            command.env("UV_TORCH_BACKEND", "cpu");
        }
        #[cfg(unix)] {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)] {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command
    }
    fn install(&self) -> Result<()> {
        fs::create_dir_all(&self.home)?;
        let uv = self.bootstrap_uv()?;
        self.progress("runtime", "正在下载 Python 运行环境…");
        self.run(self.command(&uv).args(["python", "install", "3.12"]), true)?;
        self.progress("dependencies", "正在下载并安装 Docling 及格式支持组件…");
        // Resemblyzer requests the source-only webrtcvad package. The maintained
        // wheel distribution exposes the same module without requiring a NAS compiler.
        let excludes = self.home.join("excludes.txt");
        fs::write(&excludes, "webrtcvad\n")?;
        self.run(self.command(&uv).arg("tool").arg("install").arg("--excludes").arg(&excludes).args([
            "--python", "3.12", "--managed-python", "--force",
            "--with", "imageio-ffmpeg==0.6.0", "--with", "webrtcvad-wheels==2.0.14",
            &format!("docling-slim[standard,format-audio,format-video,format-opendocument,format-iwork,format-xml-xbrl,format-xml-jats,format-xml-uspto,feat-ocr-rapidocr-onnx]=={VERSION}"),
        ]), true)?;
        // Audio/video parsers call ffmpeg. Keep its packaged binary private to this component.
        let mut ffmpeg = None;
        for entry in walkdir::WalkDir::new(self.home.join("tools/docling-slim")) {
            let entry = entry?;
            if entry.file_type().is_file()
                && entry.path().parent().is_some_and(|parent| parent.file_name().is_some_and(|name| name == "binaries"))
                && entry.file_name().to_string_lossy().starts_with("ffmpeg-") {
                ffmpeg = Some(entry.into_path());
                break;
            }
        }
        fs::copy(ffmpeg.context("音视频组件中缺少 ffmpeg")?, self.binary("ffmpeg"))?;
        self.progress("models", "正在下载文档识别模型…");
        self.run(self.command(self.binary("docling-tools"))
            .args(["models", "download", "layout", "tableformer", "rapidocr"])
            .arg("--output-dir").arg(self.home.join("models")), true)?;
        self.run(self.command(self.binary("docling")).arg("--version"), true)?;
        if self.cancel.load(Ordering::Relaxed) { bail!("已取消下载，可稍后重试") }
        fs::write(self.home.join("ready"), VERSION)?;
        Ok(())
    }
    fn bootstrap_uv(&self) -> Result<PathBuf> {
        let executable = self.home.join(if cfg!(windows) { "uv.exe" } else { "uv" });
        if executable.is_file() { return Ok(executable); }
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
        let url = format!("https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/uv-{platform}.{suffix}");
        let mut archive = self.download_archive(&url)?;
        use std::io::Seek;
        archive.rewind()?;
        if cfg!(windows) {
            let mut zip = zip::ZipArchive::new(archive)?;
            let mut file = zip.by_name("uv.exe")?;
            std::io::copy(&mut file, &mut fs::File::create(&executable)?)?;
        } else {
            let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(archive));
            let mut found = false;
            for entry in archive.entries()? {
                let mut entry = entry?;
                if entry.path()?.file_name().is_some_and(|name| name == "uv") {
                    entry.unpack(&executable)?;
                    found = true;
                    break;
                }
            }
            if !found { bail!("运行环境下载包中缺少 uv") }
        }
        Ok(executable)
    }
    fn download_archive(&self, url: &str) -> Result<fs::File> {
        use futures_util::StreamExt;
        tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
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
            self.status.lock().unwrap().total = response.content_length();
            let mut stream = response.bytes_stream();
            let mut archive = tempfile::tempfile()?;
            loop {
                tokio::select! {
                    chunk = stream.next() => match chunk {
                        Some(chunk) => {
                            let bytes = chunk?;
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
        static ANSI: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))").unwrap());
        static BYTES: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?i)([0-9]+(?:\.[0-9]+)?)\s*(KiB|MiB|GiB|kB|MB|GB|B)\s*/\s*([0-9]+(?:\.[0-9]+)?)\s*(KiB|MiB|GiB|kB|MB|GB|B)").unwrap());
        static PERCENT: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"\b([0-9]{1,3}(?:\.[0-9]+)?)%").unwrap());
        static BAR: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"\s+-{3,}\s+").unwrap());
        static SPACING: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r" {2,}").unwrap());
        let text = String::from_utf8_lossy(raw);
        let clean = ANSI.replace_all(&text, "");
        let clean = BAR.replace_all(&clean, ": ");
        let clean = SPACING.replace_all(&clean, "\n").trim().to_owned();
        if clean.is_empty() { return clean; }
        let mut status = self.status.lock().unwrap();
        status.message = clean.chars().rev().take(2000).collect::<String>().chars().rev().collect();
        status.downloaded = 0;
        status.total = None;
        status.percent = None;
        let downloads: Vec<_> = BYTES.captures_iter(&clean).collect();
        if !downloads.is_empty() {
            let scale = |unit: &str| match unit.to_ascii_lowercase().as_str() {
                "kib" => 1024.0, "mib" => 1048576.0, "gib" => 1073741824.0,
                "kb" => 1000.0, "mb" => 1e6, "gb" => 1e9, _ => 1.0,
            };
            status.downloaded = downloads.iter().map(|bytes| (bytes[1].parse::<f64>().unwrap() * scale(&bytes[2])) as u64).sum();
            status.total = Some(downloads.iter().map(|bytes| (bytes[3].parse::<f64>().unwrap() * scale(&bytes[4])) as u64).sum());
            status.percent = None;
        } else if let Some(percent) = PERCENT.captures(&clean) {
            status.percent = Some(percent[1].parse::<f64>().unwrap().min(100.0));
            status.total = None;
            status.downloaded = 0;
        }
        clean
    }
    fn run(&self, command: &mut Command, installing: bool) -> Result<()> {
        // A terminal makes uv and model downloaders report their actual progress.
        // It also sends SIGHUP to its foreground job if the service exits.
        let pair = portable_pty::native_pty_system().openpty(portable_pty::PtySize {
            rows: 24, cols: 160, pixel_width: 0, pixel_height: 0,
        })?;
        let mut builder = portable_pty::CommandBuilder::new(command.get_program());
        builder.args(command.get_args());
        for (key, value) in command.get_envs() {
            match value { Some(value) => builder.env(key, value), None => builder.env_remove(key) }
        }
        builder.env("TERM", "xterm-256color");
        // uv disables progress whenever its parent's Rust logging is configured.
        builder.env_remove("RUST_LOG");
        if let Some(directory) = command.get_current_dir() { builder.cwd(directory); }
        let mut child = pair.slave.spawn_command(builder)
            .with_context(|| format!("无法启动 Docling 组件：{}", command.get_program().to_string_lossy()))?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader()?;
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let mut line = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => { if !line.is_empty() { let _ = tx.send(Ok(line)); } break; }
                    Ok(count) => for byte in &buffer[..count] {
                        if matches!(byte, b'\r' | b'\n') || line.len() >= 8192 {
                            if !line.is_empty() && tx.send(Ok(std::mem::take(&mut line))).is_err() { return; }
                        } else { line.push(*byte); }
                    },
                    Err(error) => { let _ = tx.send(Err(error)); break; }
                }
            }
        });
        let mut lines = std::collections::VecDeque::new();
        let outcome = (|| -> Result<_> {
            loop {
                while let Ok(line) = rx.try_recv() {
                    let line = line?;
                    let clean = if installing { self.update_download_line(&line) } else { String::from_utf8_lossy(&line).into_owned() };
                    if !clean.is_empty() { lines.push_back(clean); if lines.len() > 8 { lines.pop_front(); } }
                }
                if installing && self.cancel.load(Ordering::Relaxed) { bail!("已取消下载，可稍后重试") }
                if let Some(status) = child.try_wait()? { return Ok(status); }
                std::thread::sleep(Duration::from_millis(50));
            }
        })();
        if outcome.is_err() && child.try_wait()?.is_none() {
            let pid = child.process_id().context("无法读取下载进程编号")?;
            #[cfg(unix)] unsafe {
                if libc::kill(-(pid as i32), libc::SIGKILL) != 0 {
                    let error = std::io::Error::last_os_error();
                    if error.raw_os_error() != Some(libc::ESRCH) { return Err(error.into()); }
                }
            }
            #[cfg(windows)] {
                let status = Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).status()?;
                if !status.success() { child.kill()?; }
            }
            child.wait()?;
        }
        drop(pair.master);
        handle.join().expect("Docling output reader panicked");
        for line in rx {
            let line = line?;
            let clean = if installing { self.update_download_line(&line) } else { String::from_utf8_lossy(&line).into_owned() };
            if !clean.is_empty() { lines.push_back(clean); if lines.len()>8 {lines.pop_front();} }
        }
        let status = outcome?;
        if !status.success() { bail!("Docling 执行失败（{status}）：{}", lines.into_iter().collect::<Vec<_>>().join("\n")) }
        Ok(())
    }
    pub fn convert(&self, source: &Path, output: &Path) -> Result<()> {
        let _usage = self.usage.read().unwrap();
        if !self.installed() { bail!("请在文件设置中下载 Docling 后重试") }
        let parent = output.parent().context("转换文件缺少父目录")?;
        fs::create_dir_all(parent)?;
        // Atomic publication: partial/failed conversion must never become indexed Markdown.
        let staging = tempfile::tempdir_in(parent)?;
        let temporary = staging.path().join("document.md");
        self.run(self.command(self.binary("docling"))
            .arg(source).args(["--to", "md", "--image-export-mode", "placeholder", "--abort-on-error", "--document-timeout", "300"])
            .arg("--artifacts-path").arg(self.home.join("models"))
            .arg("--output-file").arg(&temporary), false)?;
        let content = fs::read_to_string(&temporary).context("Docling 未生成 Markdown")?;
        if content.trim().is_empty() { bail!("Docling 未提取到正文") }
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        // Relative source link survives moving the whole knowledge folder.
        let depth = output.ancestors().position(|path| path.file_name().is_some_and(|name| name == OUTPUT_DIRECTORY))
            .context("转换路径必须位于 .sag-converted")?;
        let root = output.ancestors().nth(depth + 1).unwrap();
        let relative = source.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
        let link = format!("{}{}", "../".repeat(depth), relative);
        let link = link.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect::<String>();
        writeln!(file, "[{}](<{link}>)\n", source.file_name().unwrap().to_string_lossy().replace(['[', ']'], ""))?;
        file.write_all(content.as_bytes())?;
        file.persist(output).map_err(|error| error.error)?;
        Ok(())
    }
}

pub fn fingerprint(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 { break; }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn output_path(root: &Path, source: &Path) -> Result<PathBuf> {
    let relative = source.strip_prefix(root)?;
    if relative.components().any(|part| !matches!(part, std::path::Component::Normal(_))) {
        bail!("转换文件必须位于所选文件夹内")
    }
    let mut output = root.join(OUTPUT_DIRECTORY).join(relative);
    output.set_file_name(format!("{}.md", source.file_name().context("缺少文件名")?.to_string_lossy()));
    // Never follow a user-created output link outside the authorized folder.
    for path in output.ancestors().take_while(|path| *path != root) {
        if path.symlink_metadata().is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            bail!(".sag-converted 中包含符号链接，无法安全写入")
        }
    }
    Ok(output)
}

#[cfg(test)]
#[path = "docling_tests.rs"]
mod tests;
