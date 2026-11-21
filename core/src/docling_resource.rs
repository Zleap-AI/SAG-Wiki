use super::{Docling, Ordering};
use crate::docling_component;
use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use sag_protocol::DoclingProfile;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Seek, Write},
    path::{Component, Path},
    time::Duration,
};

const ORIGIN: &str = "https://resource.zleap.com";
const RELEASES: &str = "/api/public/v1/resources/sag-wiki-docling/releases?channel=stable";
const MAX_ARCHIVE: u64 = 4 * 1024 * 1024 * 1024;
const MAX_EXPANDED: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Deserialize)]
struct Releases {
    items: Vec<Release>,
}
#[derive(Deserialize)]
struct Release {
    version: String,
    variants: Vec<Variant>,
}
#[derive(Deserialize)]
struct Variant {
    recommended: bool,
    files: Vec<Asset>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Asset {
    filename: String,
    size: u64,
    sha256: String,
    download_path: String,
}

fn select_asset(releases: Releases, platform: &str) -> Result<(String, Asset)> {
    let mut assets = releases
        .items
        .into_iter()
        .filter(|release| {
            docling_component::compatible_version(DoclingProfile::Documents, &release.version)
        })
        .flat_map(|release| {
            let name = format!("Docling-{}-{platform}.zip", release.version);
            release
                .variants
                .into_iter()
                .filter(|variant| variant.recommended)
                .flat_map(|variant| variant.files)
                .filter(move |asset| asset.filename == name)
                .map(move |asset| (release.version.clone(), asset))
        });
    let (version, asset) = assets
        .next()
        .context("资源站未提供适用于当前系统或版本的 Docling 安装包")?;
    let ids = asset
        .download_path
        .strip_prefix("/downloads/sag-wiki-docling/")
        .and_then(|path| path.split_once('/'));
    if assets.next().is_some()
        || asset.size == 0
        || asset.size > MAX_ARCHIVE
        || asset.sha256.len() != 64
        || !asset.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !ids.is_some_and(|(release, artifact)| {
            uuid::Uuid::parse_str(release).is_ok() && uuid::Uuid::parse_str(artifact).is_ok()
        })
    {
        bail!("资源站的 Docling 安装包信息无效");
    }
    Ok((version, asset))
}

impl Docling {
    pub(super) fn install_resource(&self) -> Result<()> {
        let platform = docling_component::platform();
        if !matches!(platform.as_str(), "macos-aarch64" | "windows-x86_64") {
            bail!("资源站未提供适用于当前系统或版本的 Docling 安装包");
        }
        let components = self.home.join("components");
        fs::create_dir_all(&components)?;
        fs::write(self.home.join("install-diagnostics.log"), [])?;
        let cancel = || self.cancel.load(Ordering::Relaxed);
        self.progress("runtime", "正在检查 Docling 安装包…");
        let (version, asset, mut archive) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                tokio::select! {
                    result = self.download_resource(ORIGIN, &platform) => result,
                    _ = async {
                        while !cancel() { tokio::time::sleep(Duration::from_millis(100)).await; }
                    } => bail!("已取消组件安装"),
                }
            })
            .context("无法从资源站下载 Docling 安装包")?;
        self.progress("dependencies", "正在解压并校验 Docling 安装包…");
        let staging = tempfile::Builder::new()
            .prefix("documents-")
            .tempdir_in(&components)?;
        let root = asset.filename.trim_end_matches(".zip");
        archive.rewind()?;
        unpack(archive, Path::new(root), staging.path(), &cancel)?;
        if docling_component::manifest(staging.path(), DoclingProfile::Documents)?.component_version
            != version
        {
            bail!("Docling 安装包版本与发布记录不符");
        }
        docling_component::activate(&self.home, staging, &cancel, |runtime| {
            self.progress("models", "正在检查 Docling 是否可用…");
            self.probe_component(runtime, &cancel)
        })
    }

    async fn download_resource(
        &self,
        origin: &str,
        platform: &str,
    ) -> Result<(String, Asset, fs::File)> {
        let builder = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none());
        let client = sag_client::network_proxy::apply_proxy(builder, &self.download_proxy)?.build()?;
        let response = client
            .get(format!("{origin}{RELEASES}"))
            .send()
            .await?
            .error_for_status()?;
        if !response.status().is_success() {
            bail!("资源站未返回 Docling 版本列表");
        }
        let (version, asset) = select_asset(response.json().await?, platform)?;
        let cache = self.home.join("downloads");
        fs::create_dir_all(&cache)
            .with_context(|| format!("无法创建 Docling 下载缓存目录：{}", cache.display()))?;
        let cached = cache.join(format!("{}.zip", asset.sha256.to_ascii_lowercase()));
        match fs::File::open(&cached) {
            Ok(mut archive) => {
                self.progress("runtime", "正在校验已下载的 Docling 安装包…");
                if archive.metadata()?.len() == asset.size {
                    let mut hash = Sha256::new();
                    let mut buffer = [0u8; 256 * 1024];
                    loop {
                        if self.cancel.load(Ordering::Relaxed) {
                            bail!("已取消组件安装");
                        }
                        let count = archive.read(&mut buffer)?;
                        if count == 0 {
                            break;
                        }
                        hash.update(&buffer[..count]);
                    }
                    if format!("{:x}", hash.finalize()).eq_ignore_ascii_case(&asset.sha256) {
                        archive.rewind()?;
                        return Ok((version, asset, archive));
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("无法读取 Docling 下载缓存：{}", cached.display()))
            }
        }
        self.progress("runtime", "正在从资源站下载 Docling 安装包…");
        let mut output = tempfile::NamedTempFile::new_in(&cache)?;
        let response = client
            .get(format!("{origin}{}", asset.download_path))
            .send()
            .await?
            .error_for_status()?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|size| size != asset.size)
        {
            bail!("Docling 安装包的下载响应无效");
        }
        self.status.lock().unwrap().total = Some(asset.size);
        let mut stream = response.bytes_stream();
        let mut hash = Sha256::new();
        let mut downloaded = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            downloaded += chunk.len() as u64;
            if downloaded > asset.size {
                bail!("Docling 安装包大小与发布记录不符");
            }
            hash.update(&chunk);
            output.write_all(&chunk)?;
            self.status.lock().unwrap().downloaded = downloaded;
        }
        if downloaded != asset.size
            || !format!("{:x}", hash.finalize()).eq_ignore_ascii_case(&asset.sha256)
        {
            bail!("Docling 安装包校验失败");
        }
        output.as_file().sync_all()?;
        if self.cancel.load(Ordering::Relaxed) {
            bail!("已取消组件安装");
        }
        let mut archive = output
            .persist(&cached)
            .map_err(|error| error.error)
            .with_context(|| format!("无法保存 Docling 下载缓存：{}", cached.display()))?;
        archive.rewind()?;
        Ok((version, asset, archive))
    }
}

fn unpack(
    input: impl Read + Seek,
    root: &Path,
    destination: &Path,
    cancel: &dyn Fn() -> bool,
) -> Result<()> {
    let mut zip = zip::ZipArchive::new(input)?;
    let mut remaining = MAX_EXPANDED;
    let mut buffer = [0u8; 256 * 1024];
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let path = Path::new(entry.name());
        if path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
            || entry.name().contains(['\\', ':'])
            || entry.is_symlink()
        {
            bail!("Docling 安装包包含不安全的路径或链接");
        }
        let relative = path
            .strip_prefix(root)
            .context("Docling 安装包目录与文件名不匹配")?;
        let target = destination.join(relative);
        if cancel() {
            bail!("已取消组件安装");
        }
        if entry.is_dir() {
            fs::create_dir_all(target)?;
            continue;
        }
        if relative.as_os_str().is_empty() || !entry.is_file() || entry.size() > remaining {
            bail!("Docling 安装包包含无效文件或解压后过大");
        }
        fs::create_dir_all(target.parent().unwrap())?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        let mut left = entry.size();
        remaining -= left;
        loop {
            if cancel() {
                bail!("已取消组件安装");
            }
            let count = entry.read(&mut buffer)?;
            left = left
                .checked_sub(count as u64)
                .context("Docling 安装包的文件大小无效")?;
            if count == 0 {
                break;
            }
            file.write_all(&buffer[..count])?;
        }
        if left != 0 {
            bail!("Docling 安装包的文件不完整");
        }
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(mode & 0o777))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::AtomicUsize, Arc};

    async fn resource_server(
        body: &[u8],
        expected: &[u8],
    ) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let downloads = Arc::new(AtomicUsize::new(0));
        let count = downloads.clone();
        let metadata = serde_json::to_vec(&serde_json::json!({
            "items": [{
                "version": docling_component::VERSION,
                "variants": [{"recommended": true, "files": [{
                    "filename": format!("Docling-{}-windows-x86_64.zip", docling_component::VERSION),
                    "size": expected.len(), "sha256": format!("{:x}", Sha256::digest(expected)),
                    "downloadPath": format!("/downloads/sag-wiki-docling/{}/{}", uuid::Uuid::nil(), uuid::Uuid::nil())
                }]}]
            }]
        })).unwrap();
        let body = body.to_vec();
        let size = expected.len();
        let task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0u8; 4096];
                let length = stream.read(&mut request).await.unwrap();
                let request = std::str::from_utf8(&request[..length]).unwrap();
                let target = request.split_ascii_whitespace().nth(1).unwrap();
                let path = reqwest::Url::parse(target)
                    .map(|url| url.path().to_owned())
                    .unwrap_or_else(|_| target.to_owned());
                let download = path.starts_with("/downloads/");
                let (payload, length) = if download {
                    count.fetch_add(1, Ordering::Relaxed);
                    (&body, size)
                } else {
                    (&metadata, metadata.len())
                };
                stream.write_all(format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n"
                ).as_bytes()).await.unwrap();
                stream.write_all(payload).await.unwrap();
                stream.shutdown().await.unwrap();
            }
        });
        (origin, downloads, task)
    }

    #[tokio::test]
    async fn downloads_manifest_and_component_through_shared_proxy() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = b"package";
        let (proxy, downloads, server) = resource_server(bytes, bytes).await;
        let mut docling = Docling::new(dir.path());
        docling.download_proxy.mode = sag_protocol::types::ProxyMode::Custom;
        docling.download_proxy.url = proxy;
        tokio::time::timeout(
            Duration::from_secs(5),
            docling.download_resource("http://192.0.2.10", "windows-x86_64"),
        ).await.unwrap().unwrap();
        assert_eq!(downloads.load(Ordering::Relaxed), 1);
        server.abort();
    }

    #[tokio::test]
    async fn retry_after_install_failure_reuses_download_across_instances() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = b"download verified, but not a valid component ZIP";
        let (origin, downloads, server) = resource_server(bytes, bytes).await;
        let first = Docling::new(dir.path());
        let (_, asset, archive) = first
            .download_resource(&origin, "windows-x86_64")
            .await
            .unwrap();
        let staging = tempfile::tempdir().unwrap();
        assert!(unpack(
            archive,
            Path::new(asset.filename.trim_end_matches(".zip")),
            staging.path(),
            &|| false
        )
        .is_err());
        assert!(!first.installed());
        drop(first);
        let retry = Docling::new(dir.path());
        let (_, _, mut archive) = retry
            .download_resource(&origin, "windows-x86_64")
            .await
            .unwrap();
        let mut contents = Vec::new();
        archive.read_to_end(&mut contents).unwrap();
        assert_eq!(contents, bytes);
        assert_eq!(downloads.load(Ordering::Relaxed), 1);
        server.abort();
    }

    #[tokio::test]
    async fn damaged_cache_is_downloaded_again() {
        let bytes = b"verified archive bytes";
        for damaged in [&b"truncated"[..], &b"modified archive bytes"[..]] {
            let dir = tempfile::tempdir().unwrap();
            let converter = Docling::new(dir.path());
            let (origin, downloads, server) = resource_server(bytes, bytes).await;
            let (_, asset, archive) = converter
                .download_resource(&origin, "windows-x86_64")
                .await
                .unwrap();
            drop(archive);
            let cached = converter
                .home
                .join("downloads")
                .join(format!("{}.zip", asset.sha256));
            fs::write(&cached, damaged).unwrap();
            let (_, _, archive) = converter
                .download_resource(&origin, "windows-x86_64")
                .await
                .unwrap();
            drop(archive);
            assert_eq!(fs::read(cached).unwrap(), bytes);
            assert_eq!(downloads.load(Ordering::Relaxed), 2);
            server.abort();
        }
    }

    #[tokio::test]
    async fn incomplete_or_bad_hash_download_is_not_cached() {
        for body in [&b"short"[..], &b"incorrect data"[..]] {
            let dir = tempfile::tempdir().unwrap();
            let converter = Docling::new(dir.path());
            let (origin, _, server) = resource_server(body, b"expected bytes").await;
            assert!(converter
                .download_resource(&origin, "windows-x86_64")
                .await
                .is_err());
            assert_eq!(
                fs::read_dir(converter.home.join("downloads"))
                    .unwrap()
                    .count(),
                0
            );
            server.abort();
        }
    }

    #[tokio::test]
    async fn changed_asset_hash_downloads_the_new_package() {
        let dir = tempfile::tempdir().unwrap();
        let converter = Docling::new(dir.path());
        let (origin, _, old_server) = resource_server(b"old package", b"old package").await;
        let (_, _, archive) = converter
            .download_resource(&origin, "windows-x86_64")
            .await
            .unwrap();
        drop(archive);
        old_server.abort();
        let (origin, downloads, server) = resource_server(b"new package", b"new package").await;
        let (_, _, mut archive) = converter
            .download_resource(&origin, "windows-x86_64")
            .await
            .unwrap();
        let mut contents = String::new();
        archive.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "new package");
        assert_eq!(downloads.load(Ordering::Relaxed), 1);
        server.abort();
    }

    #[test]
    fn selects_system_package_from_the_manually_recommended_compatible_release() {
        let platform = "windows-x86_64";
        let asset = |version: &str, platform: &str| Asset {
            filename: format!("Docling-{version}-{platform}.zip"),
            size: 42,
            sha256: "0".repeat(64),
            download_path: format!(
                "/downloads/sag-wiki-docling/{}/{}",
                uuid::Uuid::nil(),
                uuid::Uuid::nil()
            ),
        };
        let release = |version: &str, recommended| Release {
            version: version.into(),
            variants: vec![Variant {
                recommended,
                files: vec![asset(version, "macos-aarch64"), asset(version, platform)],
            }],
        };
        let (version, file) = select_asset(
            Releases {
                items: vec![
                    release("2.140.0-documents.3", false),
                    release("2.141.0-documents.1", true),
                    release("2.140.0-documents.2", true),
                ],
            },
            platform,
        )
        .unwrap();
        assert_eq!(version, "2.140.0-documents.2");
        assert_eq!(
            file.filename,
            "Docling-2.140.0-documents.2-windows-x86_64.zip"
        );
    }

    #[test]
    fn rejects_archive_paths_and_symlinks_before_writing_outside_staging() {
        use std::io::Cursor;
        use zip::{write::SimpleFileOptions, ZipWriter};
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("staging");
        fs::create_dir(&destination).unwrap();
        for link in [false, true] {
            let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
            if link {
                zip.add_symlink("pkg/python", "../../escape", SimpleFileOptions::default())
                    .unwrap();
            } else {
                zip.start_file("pkg/../escape", SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(b"outside").unwrap();
            }
            let mut archive = zip.finish().unwrap();
            archive.rewind().unwrap();
            assert!(unpack(archive, Path::new("pkg"), &destination, &|| false).is_err());
            assert!(!dir.path().join("escape").exists());
            assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
        }
    }

    #[test]
    #[ignore = "downloads about 1 GB and runs an offline sample conversion"]
    fn installs_published_resource_in_isolated_data_directory() {
        let dir = tempfile::tempdir().unwrap();
        let docling = Docling::new(dir.path());
        docling.install_resource().unwrap();
        assert!(docling.installed());
        assert!(!docling.home.join(super::super::UV_EXECUTABLE).exists());
    }
}
