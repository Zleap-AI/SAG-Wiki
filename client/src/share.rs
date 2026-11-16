//! Temporary Markdown sharing, shared by the desktop and standalone CLI.
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use futures_util::StreamExt;
use pulldown_cmark::{Event, Parser, Tag};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};
use tempfile::TempDir;
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

const ENDPOINT: &str = "https://api.zleap.com/share";
const MAX: u64 = 50 * 1024 * 1024;
const MAX_ENTRIES: usize = 2000;

#[derive(Clone, Serialize, Deserialize)]
pub struct ShareSettings {
    pub directory: String,
    pub clipboard: bool,
}
impl Default for ShareSettings {
    fn default() -> Self {
        Self {
            directory: String::new(),
            clipboard: true,
        }
    }
}
pub fn settings() -> Result<ShareSettings> {
    let path = sag_protocol::data_dir()?.join("share-settings.json");
    let mut value: ShareSettings = match fs::read(&path) {
        Ok(data) => serde_json::from_slice(&data).context("分享设置文件损坏")?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ShareSettings::default(),
        Err(e) => return Err(e.into()),
    };
    if value.directory.is_empty() {
        value.directory = dirs::document_dir()
            .context("无法获取文档目录")?
            .join("sag-wiki/share")
            .to_string_lossy()
            .into_owned();
    }
    Ok(value)
}
pub fn save_settings(value: &ShareSettings) -> Result<()> {
    if !Path::new(&value.directory).is_absolute() {
        bail!("请选择完整的接收目录")
    }
    let path = sag_protocol::data_dir()?.join("share-settings.json");
    fs::create_dir_all(path.parent().unwrap())?;
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    serde_json::to_writer(&mut temp, value)?;
    temp.persist(path)?;
    Ok(())
}
#[derive(Clone, Serialize)]
pub struct SharePreview {
    pub id: String,
    pub name: String,
    pub markdown_count: usize,
    pub image_count: usize,
    pub size: u64,
}
pub struct PreparedShare {
    pub preview: SharePreview,
    directory: TempDir,
    request_key: String,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadedShare {
    pub code: String,
    pub name: String,
    pub size: u64,
    pub expires_at: String,
}
#[derive(Serialize)]
pub struct ReceivedShare {
    pub path: String,
    pub markdown_count: usize,
    pub image_count: usize,
}

pub fn normalize_code(value: &str) -> Result<String> {
    let value = value.trim().to_ascii_uppercase();
    let value = value
        .strip_prefix("分享码：")
        .or_else(|| value.strip_prefix("SHARE CODE:"))
        .map(|text| text.lines().next().unwrap_or("").trim())
        .unwrap_or(&value);
    let code = value.strip_prefix("SAG-").unwrap_or(&value);
    if code.len() != 8
        || !code
            .bytes()
            .all(|b| b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789".contains(&b))
    {
        bail!("请输入有效的 8 位分享码")
    }
    Ok(code.into())
}
fn safe_path(name: &str) -> Result<()> {
    for part in name.split('/') {
        let base = part.split('.').next().unwrap_or("").to_uppercase();
        let device = ["CON", "PRN", "AUX", "NUL"].contains(&base.as_str())
            || ["COM", "LPT"].iter().any(|prefix| {
                base.strip_prefix(prefix)
                    .is_some_and(|n| n.chars().count() == 1 && "123456789¹²³".contains(n))
            });
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with([' ', '.'])
            || device
            || part
                .chars()
                .any(|c| c.is_control() || "<>:\\|?*\"".contains(c))
        {
            bail!("文件或目录名称不兼容，请重命名：{name}")
        }
    }
    Ok(())
}
fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}
fn image_type(ext: &str) -> Option<&'static str> {
    match ext {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "avif" => Some("image/avif"),
        _ => None,
    }
}
fn archive_name(path: &Path) -> Result<String> {
    let mut names = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(s) => names.push(s.to_str().context("文件名必须为 Unicode")?),
            _ => bail!("文件路径超出分享目录"),
        }
    }
    let name = names.join("/");
    safe_path(&name)?;
    Ok(name)
}
fn references(body: &str) -> Vec<String> {
    let mut values = Vec::new();
    for event in Parser::new(body) {
        match event {
            Event::Start(Tag::Image { dest_url, .. }) => values.push(dest_url.to_string()),
            Event::Html(html) | Event::InlineHtml(html) => {
                let document = scraper::Html::parse_fragment(&html);
                for node in document.select(&scraper::Selector::parse("img").unwrap()) {
                    if node.value().attr("srcset").is_some() {
                        continue;
                    }
                    if let Some(src) = node.value().attr("src") {
                        values.push(src.into());
                    }
                }
            }
            _ => {}
        }
    }
    values
}
fn local_image(document: &Path, root: &Path, raw: &str) -> Result<Option<PathBuf>> {
    let lower = raw.to_ascii_lowercase();
    if lower.starts_with("https://")
        || lower.starts_with("http://")
        || raw.starts_with("//")
        || raw.starts_with('#')
    {
        return Ok(None);
    }
    let raw = raw.split(['?', '#']).next().unwrap_or("");
    let Ok(decoded) = percent_encoding::percent_decode_str(raw).decode_utf8() else {
        return Ok(None);
    };
    if decoded.is_empty()
        || decoded.contains([':', '\\'])
        || Path::new(decoded.as_ref()).is_absolute()
    {
        return Ok(None);
    }
    let relative = document
        .parent()
        .unwrap()
        .strip_prefix(root)?
        .join(decoded.as_ref());
    let mut normalized = PathBuf::new();
    for c in relative.components() {
        match c {
            Component::Normal(s) => normalized.push(s),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Ok(None);
                }
            }
            _ => return Ok(None),
        }
    }
    if archive_name(&normalized).is_err() {
        return Ok(None);
    }
    let path = root.join(&normalized);
    if image_type(&extension(&path)).is_none() {
        return Ok(None);
    }
    let resolved = match path.canonicalize() {
        Ok(path) => path,
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound
                    | std::io::ErrorKind::PermissionDenied
                    | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(None)
        }
        Err(e) => return Err(e.into()),
    };
    if resolved != path || !resolved.starts_with(root) {
        return Ok(None);
    }
    if !fs::metadata(&path)?.is_file() {
        return Ok(None);
    }
    Ok(Some(path))
}
fn check_content(path: &Path, bytes: &[u8]) -> Result<()> {
    let ext = extension(path);
    if ext == "md" {
        let text = std::str::from_utf8(bytes).context("Markdown 必须为 UTF-8 编码")?;
        if text.contains('\0') {
            bail!("Markdown 包含无效字符：{}", path.display())
        }
    } else if let Some(expected) = image_type(&ext) {
        if infer::get(bytes).map(|v| v.mime_type()) != Some(expected) {
            bail!("图片内容与格式不符：{}", path.display())
        }
    } else {
        bail!("分享包只允许 Markdown 和图片")
    }
    Ok(())
}
fn bounded_read(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        bail!("分享内容不能超过 50 MB")
    }
    Ok(bytes)
}
pub fn prepare(path: &Path) -> Result<PreparedShare> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        bail!("不能分享符号链接")
    }
    let source = path.canonicalize()?;
    let root = if source.is_dir() {
        source.clone()
    } else {
        source.parent().unwrap().to_owned()
    };
    let mut documents = Vec::new();
    if source.is_dir() {
        for entry in walkdir::WalkDir::new(&source).follow_links(false) {
            let entry = entry?;
            if entry.file_type().is_file() && extension(entry.path()) == "md" {
                documents.push(entry.path().to_owned());
            }
            if documents.len() > MAX_ENTRIES {
                bail!("分享包最多包含 2000 个文件")
            }
        }
    } else if extension(&source) == "md" {
        documents.push(source.clone());
    }
    if documents.is_empty() {
        bail!("没有可分享的 Markdown 文件")
    }
    documents.sort();
    let mut contents = BTreeMap::new();
    for document in &documents {
        let bytes = fs::read(document)?;
        check_content(document, &bytes)?;
        for image in references(std::str::from_utf8(&bytes)?) {
            if let Some(path) = local_image(document, &root, &image)? {
                contents.entry(path).or_insert(None);
            }
        }
        contents.insert(document.clone(), Some(bytes));
    }
    if contents.len() > MAX_ENTRIES {
        bail!("分享包最多包含 2000 个文件")
    }
    let mut seen = HashSet::new();
    let mut image_count = 0;
    let mut prepared_contents = Vec::new();
    for (path, content) in contents {
        let bytes = if let Some(bytes) = content {
            bytes
        } else {
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                    ) =>
                {
                    continue
                }
                Err(e) => return Err(e.into()),
            };
            if check_content(&path, &bytes).is_err() {
                continue;
            }
            image_count += 1;
            bytes
        };
        let name = archive_name(path.strip_prefix(&root)?)?;
        if !seen.insert(name.nfc().collect::<String>().to_lowercase()) {
            bail!("分享内容存在重名文件：{name}")
        }
        prepared_contents.push((name, bytes));
    }
    let dir = tempfile::tempdir()?;
    let file = File::create(dir.path().join("package.zip"))?;
    let mut zip = zip::ZipWriter::new(file);
    for (name, bytes) in prepared_contents {
        zip.start_file(
            name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )?;
        zip.write_all(&bytes)?;
    }
    zip.finish()?;
    let size = fs::metadata(dir.path().join("package.zip"))?.len();
    if size > MAX {
        bail!("压缩文件不能超过 50 MB")
    }
    let name = if source.is_dir() {
        source.file_name()
    } else {
        source.file_stem()
    }
    .and_then(|s| s.to_str())
    .context("无效的分享名称")?
    .to_owned();
    safe_path(&name)?;
    Ok(PreparedShare {
        preview: SharePreview {
            id: Uuid::new_v4().to_string(),
            name,
            markdown_count: documents.len(),
            image_count,
            size,
        },
        directory: dir,
        request_key: URL_SAFE_NO_PAD.encode(
            [
                Uuid::new_v4().as_bytes().as_slice(),
                Uuid::new_v4().as_bytes().as_slice(),
            ]
            .concat(),
        ),
    })
}
fn http() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(150))
        .build()?)
}
async fn response_error(response: reqwest::Response) -> anyhow::Error {
    let status = response.status();
    // Never echo URLs or server response bodies containing signed credentials.
    anyhow::anyhow!(match status.as_u16() {
        404 => "分享不可用或已过期",
        429 => "请求过于频繁，请稍后重试",
        413 => "分享文件不能超过 50 MB",
        _ => "分享服务请求失败，请稍后重试",
    })
}
pub async fn upload(prepared: &PreparedShare) -> Result<UploadedShare> {
    let file = bounded_read(&prepared.directory.path().join("package.zip"), MAX)?;
    let form = reqwest::multipart::Form::new()
        .text("requestId", prepared.preview.id.clone())
        .text("name", prepared.preview.name.clone())
        .part(
            "file",
            reqwest::multipart::Part::bytes(file)
                .file_name("package.zip")
                .mime_str("application/zip")?,
        );
    let response = http()?
        .post(ENDPOINT)
        .header("X-Share-Request-Key", &prepared.request_key)
        .multipart(form)
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("上传失败，请检查网络后重试"))?;
    if !response.status().is_success() {
        return Err(response_error(response).await);
    }
    Ok(response.json().await.context("分享服务返回无效数据")?)
}
#[derive(Deserialize)]
struct DownloadRequest {
    url: String,
    headers: BTreeMap<String, String>,
}
impl DownloadRequest {
    fn validated(self) -> Result<(reqwest::Url, reqwest::header::HeaderMap)> {
        let url =
            reqwest::Url::parse(&self.url).map_err(|_| anyhow::anyhow!("分享下载地址无效"))?;
        if url.scheme() != "https"
            || !matches!(
                url.host_str(),
                Some("share.zleap.com" | "sag-share.oss-cn-hangzhou.aliyuncs.com")
            )
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.path().starts_with("/shares/")
            || !url.path().ends_with("/package.zip")
        {
            bail!("分享下载地址无效")
        }
        let allowed = [
            "authorization",
            "x-oss-date",
            "x-oss-content-sha256",
            "x-oss-security-token",
        ];
        let mut headers = reqwest::header::HeaderMap::new();
        for (key, value) in self.headers {
            if !allowed.contains(&key.to_ascii_lowercase().as_str()) {
                bail!("分享下载请求头无效")
            }
            let key = reqwest::header::HeaderName::from_bytes(key.as_bytes())
                .map_err(|_| anyhow::anyhow!("分享下载请求头无效"))?;
            let mut value = reqwest::header::HeaderValue::from_str(&value)
                .map_err(|_| anyhow::anyhow!("分享下载请求头无效"))?;
            value.set_sensitive(true);
            if headers.insert(key, value).is_some() {
                bail!("分享下载请求头重复")
            }
        }
        if !allowed.iter().all(|name| headers.contains_key(*name)) {
            bail!("分享下载请求头不完整")
        }
        Ok((url, headers))
    }
}
pub async fn receive(code: &str, directory: &Path) -> Result<ReceivedShare> {
    let code = normalize_code(code)?;
    let client = http()?;
    let response = client
        .get(ENDPOINT)
        .query(&[("code", &code)])
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("无法连接分享服务"))?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(response_error(response).await);
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct DownloadInfo {
        name: String,
        size: u64,
        sha256: String,
        download: DownloadRequest,
    }
    let mut metadata = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| anyhow::anyhow!("读取分享信息失败"))?;
        if metadata.len() + chunk.len() > 32 * 1024 {
            bail!("分享信息过大")
        }
        metadata.extend_from_slice(&chunk);
    }
    let info: DownloadInfo =
        serde_json::from_slice(&metadata).map_err(|_| anyhow::anyhow!("分享信息无效"))?;
    let name = info.name;
    safe_path(&name)?;
    if name.contains('/') {
        bail!("分享名称无效")
    }
    let expected_size = info.size;
    if expected_size > MAX {
        bail!("分享文件不能超过 50 MB")
    }
    let expected_hash = info.sha256;
    let (url, headers) = info.download.validated()?;
    let response = client
        .get(url)
        .headers(headers)
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("下载失败，请重试"))?;
    if !response.status().is_success() {
        return Err(response_error(response).await);
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| anyhow::anyhow!("下载中断，请重试"))?;
        if bytes.len() + chunk.len() > MAX as usize {
            bail!("分享文件超过大小限制")
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() as u64 != expected_size
        || format!("{:x}", Sha256::digest(&bytes)) != expected_hash
    {
        bail!("下载文件校验失败，请重试")
    }
    let directory = directory.to_owned();
    tokio::task::spawn_blocking(move || extract(&bytes, &directory, &name)).await?
}
fn extract(bytes: &[u8], directory: &Path, name: &str) -> Result<ReceivedShare> {
    fs::create_dir_all(directory)?;
    let directory = directory.canonicalize()?;
    let staging = tempfile::tempdir_in(&directory)?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    if archive.len() > MAX_ENTRIES {
        bail!("分享包文件数量超限")
    }
    let mut seen = HashSet::new();
    let mut markdown_count = 0;
    let mut image_count = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().trim_end_matches('/').to_owned();
        safe_path(&name)?;
        if !seen.insert(name.nfc().collect::<String>().to_lowercase()) {
            bail!("分享包存在重名路径")
        }
        let mode = entry.unix_mode().unwrap_or(0) & 0o170000;
        if mode != 0 && mode != 0o100000 && mode != 0o040000 {
            bail!("分享包包含符号链接或特殊文件")
        }
        let path = staging.path().join(&name);
        if entry.is_dir() {
            fs::create_dir_all(path)?;
            continue;
        }
        let mut data = Vec::new();
        let expected_size = entry.size();
        (&mut entry)
            .take(expected_size.saturating_add(1))
            .read_to_end(&mut data)?;
        if data.len() as u64 != entry.size() {
            bail!("分享包解压大小异常")
        }
        check_content(Path::new(&name), &data)?;
        if extension(Path::new(&name)) == "md" {
            markdown_count += 1
        } else {
            image_count += 1
        }
        fs::create_dir_all(path.parent().unwrap())?;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?
            .write_all(&data)?;
    }
    if markdown_count == 0 {
        bail!("没有可接收的 Markdown 文件")
    }
    let mut candidate = directory.join(name);
    let mut suffix = 2;
    loop {
        match fs::create_dir(&candidate) {
            Ok(()) => break,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                candidate = directory.join(format!("{name} ({suffix})"));
                suffix += 1
            }
            Err(e) => return Err(e.into()),
        }
    }
    let moved = (|| -> Result<()> {
        for entry in fs::read_dir(staging.path())? {
            let entry = entry?;
            fs::rename(entry.path(), candidate.join(entry.file_name()))?
        }
        Ok(())
    })();
    if let Err(error) = moved {
        fs::remove_dir_all(&candidate)?;
        return Err(error);
    }
    Ok(ReceivedShare {
        path: candidate.to_string_lossy().into_owned(),
        markdown_count,
        image_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/l9sAAAAASUVORK5CYII=";
    fn fixture(root: &Path) {
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::create_dir_all(root.join("images")).unwrap();
        fs::write(
            root.join("nested/note.MD"),
            "# Example\n![image](../images/photo.png)\n![remote](https://example.com/photo.png)",
        )
        .unwrap();
        fs::write(
            root.join("images/photo.png"),
            base64::engine::general_purpose::STANDARD
                .decode(PNG)
                .unwrap(),
        )
        .unwrap();
        fs::write(root.join("ignored.txt"), "excluded").unwrap();
        fs::write(root.join("images/unused.png"), "excluded").unwrap();
    }
    fn zip_entries(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, data) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    #[test]
    fn collects_only_markdown_and_referenced_images_and_preserves_existing_files() {
        let source = tempfile::tempdir().unwrap();
        fixture(source.path());
        let prepared = prepare(source.path()).unwrap();
        assert_eq!(
            (
                prepared.preview.markdown_count,
                prepared.preview.image_count
            ),
            (1, 1)
        );
        let bytes = fs::read(prepared.directory.path().join("package.zip")).unwrap();
        let target = tempfile::tempdir().unwrap();
        let first = extract(&bytes, target.path(), "Notes").unwrap();
        let second = extract(&bytes, target.path(), "Notes").unwrap();
        assert!(Path::new(&first.path).join("nested/note.MD").exists());
        assert!(Path::new(&first.path).join("images/photo.png").exists());
        assert!(!Path::new(&first.path).join("ignored.txt").exists());
        assert!(!Path::new(&first.path).join("images/unused.png").exists());
        assert!(second.path.ends_with("Notes (2)"));
    }
    #[test]
    fn validates_codes_and_cross_platform_names() {
        assert!(ShareSettings::default().clipboard);
        assert_eq!(normalize_code(" sag-abcdefgh ").unwrap(), "ABCDEFGH");
        assert_eq!(
            normalize_code("分享码：SAG-ABCDEFGH\n复制此分享码，打开SAG-Wiki即可下载对应文件内容")
                .unwrap(),
            "ABCDEFGH"
        );
        assert_eq!(
            normalize_code("Share code: SAG-ABCDEFGH\nCopy this share code and open SAG-Wiki to download the shared files.").unwrap(),
            "ABCDEFGH"
        );
        for code in ["ABCD1234", "ABCDEFG", "ABCDEFGH2", "https://example.com"] {
            assert!(normalize_code(code).is_err());
        }
        for name in [
            "../note.md",
            "/note.md",
            "a\\b.md",
            "CON.md",
            "LPT².txt",
            "a?.md",
            "folder./note.md",
        ] {
            assert!(safe_path(name).is_err(), "{name}");
        }
    }
    #[test]
    fn skips_invalid_image_references_and_preserves_markdown() {
        let source = tempfile::tempdir().unwrap();
        let png = base64::engine::general_purpose::STANDARD
            .decode(PNG)
            .unwrap();
        fs::write(source.path().join("photo.png"), &png).unwrap();
        fs::write(source.path().join("bad.png"), "not an image").unwrap();
        fs::write(source.path().join("vector.svg"), "<svg/>").unwrap();
        let outside = tempfile::NamedTempFile::with_suffix(".png").unwrap();
        fs::write(outside.path(), &png).unwrap();
        for body in [
            "![x](missing.png)",
            "![x](../outside.png)",
            "![x](data:image/png;base64,AAAA)",
            "<img src='photo.png' srcset='photo.png 2x'>",
            "![x](vector.svg)",
            "![x](bad.png)",
            "![x](%FF.png)",
            &format!("![x]({})", outside.path().display()),
        ] {
            let body = format!("# Note\n{body}\n\n![valid](photo.png)");
            fs::write(source.path().join("note.md"), &body).unwrap();
            let prepared = prepare(source.path()).unwrap();
            assert_eq!(prepared.preview.markdown_count, 1);
            assert_eq!(prepared.preview.image_count, 1, "{body}");
            let bytes = fs::read(prepared.directory.path().join("package.zip")).unwrap();
            let target = tempfile::tempdir().unwrap();
            let received = extract(&bytes, target.path(), "Notes").unwrap();
            let root = Path::new(&received.path);
            assert_eq!(fs::read_to_string(root.join("note.md")).unwrap(), body);
            assert_eq!(fs::read(root.join("photo.png")).unwrap(), png);
            assert_eq!(fs::read_dir(root).unwrap().count(), 2);
        }
    }
    #[cfg(unix)]
    #[test]
    fn skips_linked_images_and_rejects_linked_archives() {
        let source = tempfile::tempdir().unwrap();
        fixture(source.path());
        let external = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(external.path(), source.path().join("link.png")).unwrap();
        fs::write(source.path().join("note.md"), "![x](link.png)").unwrap();
        let prepared = prepare(source.path()).unwrap();
        assert_eq!(prepared.preview.image_count, 1);
        let bytes = fs::read(prepared.directory.path().join("package.zip")).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert!(archive.by_name("link.png").is_err());
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.add_symlink(
            "link.md",
            "/etc/passwd",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        assert!(extract(&bytes, source.path(), "Received").is_err());
    }
    #[test]
    fn rejects_unsafe_archives_without_leaving_output() {
        let target = tempfile::tempdir().unwrap();
        let invalid: Vec<Vec<(&str, &[u8])>> = vec![
            vec![("../escape.md", b"text")],
            vec![("/escape.md", b"text")],
            vec![("note.md", b"text"), ("NOTE.md", b"text")],
            vec![("note.md", b"text"), ("note.md/a.md", b"text")],
            vec![("note.md", b"text"), ("fake.png", b"not a png")],
            vec![("note.md", b"\xff")],
            vec![("app.exe", b"text")],
        ];
        for entries in invalid {
            assert!(extract(&zip_entries(&entries), target.path(), "Received").is_err());
            assert_eq!(fs::read_dir(target.path()).unwrap().count(), 0);
        }
    }
    #[test]
    fn bounded_reads_stop_over_limit() {
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(file.path(), b"12345").unwrap();
        assert!(bounded_read(file.path(), 4).is_err());
    }
    #[test]
    fn allows_content_over_limit_when_zip_is_under_limit() {
        let source = tempfile::tempdir().unwrap();
        let document = source.path().join("note.md");
        let body = "![image](photo.png)";
        fs::write(&document, body).unwrap();
        let mut image = File::create(source.path().join("photo.png")).unwrap();
        image
            .write_all(
                &base64::engine::general_purpose::STANDARD
                    .decode(PNG)
                    .unwrap(),
            )
            .unwrap();
        image.set_len(MAX).unwrap();
        let prepared = prepare(source.path()).unwrap();
        let bytes = fs::read(prepared.directory.path().join("package.zip")).unwrap();
        assert_eq!(prepared.preview.size, bytes.len() as u64);
        assert!(prepared.preview.size < MAX);
        let target = tempfile::tempdir().unwrap();
        let received = extract(&bytes, target.path(), "Notes").unwrap();
        assert_eq!(
            fs::metadata(Path::new(&received.path).join("photo.png"))
                .unwrap()
                .len(),
            MAX
        );
        assert_eq!(
            fs::read_to_string(Path::new(&received.path).join("note.md")).unwrap(),
            body
        );
    }
    #[test]
    fn counts_only_unique_packaged_files() {
        let source = tempfile::tempdir().unwrap();
        let body = "![first](photo.png)\n![again](photo.png)\n![missing](missing.png)\n![invalid](bad.png)";
        let png = base64::engine::general_purpose::STANDARD
            .decode(PNG)
            .unwrap();
        fs::write(source.path().join("note.md"), body).unwrap();
        fs::write(source.path().join("photo.png"), &png).unwrap();
        fs::write(source.path().join("bad.png"), "invalid image").unwrap();
        File::create(source.path().join("ignored.bin"))
            .unwrap()
            .set_len(MAX + 1)
            .unwrap();
        let prepared = prepare(source.path()).unwrap();
        let bytes = fs::read(prepared.directory.path().join("package.zip")).unwrap();
        assert_eq!(prepared.preview.size, bytes.len() as u64);
        assert_eq!(
            zip::ZipArchive::new(std::io::Cursor::new(bytes))
                .unwrap()
                .len(),
            2
        );
        assert_eq!(prepared.preview.markdown_count, 1);
        assert_eq!(prepared.preview.image_count, 1);
    }
    #[test]
    fn rejects_zip_over_50_mb_after_compression() {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("note.md"), "![image](photo.png)").unwrap();
        let mut image = base64::engine::general_purpose::STANDARD
            .decode(PNG)
            .unwrap();
        let mut state = 0x123456789abcdef_u64;
        while image.len() <= MAX as usize + 1024 * 1024 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            image.extend_from_slice(&state.to_le_bytes());
        }
        fs::write(source.path().join("photo.png"), image).unwrap();
        assert_eq!(
            prepare(source.path()).err().unwrap().to_string(),
            "压缩文件不能超过 50 MB"
        );
    }
    #[test]
    fn validates_download_origin_and_only_forwards_oss_auth_headers() {
        let request = |url: &str, extra: Option<(&str, &str)>| {
            let mut headers: BTreeMap<String, String> = [
                ("Authorization", "OSS4-HMAC-SHA256 test"),
                ("x-oss-date", "20261117T000000Z"),
                ("x-oss-content-sha256", "UNSIGNED-PAYLOAD"),
                ("x-oss-security-token", "token"),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
            if let Some((k, v)) = extra {
                headers.insert(k.into(), v.into());
            }
            DownloadRequest {
                url: url.into(),
                headers,
            }
        };
        let good = "https://share.zleap.com/shares/id/package.zip";
        let (url, headers) = request(good, None).validated().unwrap();
        assert!(url.query().is_none());
        assert!(headers["authorization"].is_sensitive());
        assert!(request(
            "https://sag-share.oss-cn-hangzhou.aliyuncs.com/shares/id/package.zip",
            None
        )
        .validated()
        .is_ok());
        assert!(request(good, Some(("Cookie", "private")))
            .validated()
            .is_err());
        assert!(request(good, Some(("authorization", "duplicate")))
            .validated()
            .is_err());
        for url in [
            "http://share.zleap.com/shares/id/package.zip",
            "https://share.zleap.com/shares/id/package.zip?signature=secret",
            "https://share.zleap.com/private.zip",
            "https://share.zleap.com.evil.example/shares/id/package.zip",
            "https://example.com/shares/id/package.zip",
            "http://sag-share.oss-cn-hangzhou.aliyuncs.com/shares/id/package.zip",
            "https://sag-share.oss-cn-hangzhou.aliyuncs.com/shares/id/package.zip?signature=secret",
            "https://sag-share.oss-cn-hangzhou.aliyuncs.com/private.zip",
        ] {
            assert!(request(url, None).validated().is_err());
        }
    }
    #[tokio::test]
    #[ignore = "Uploads synthetic fixtures to the live one-hour share service"]
    async fn live_upload_retry_and_repeat_download() {
        let source = tempfile::tempdir().unwrap();
        fixture(source.path());
        let prepared = prepare(source.path()).unwrap();
        let uploaded = upload(&prepared).await.unwrap();
        let retry = upload(&prepared).await.unwrap();
        assert_eq!(retry.code, uploaded.code);
        assert_eq!(retry.expires_at, uploaded.expires_at);
        let target = tempfile::tempdir().unwrap();
        for _ in 0..2 {
            let received = receive(&uploaded.code, target.path()).await.unwrap();
            assert_eq!((received.markdown_count, received.image_count), (1, 1));
            assert_eq!(
                fs::read(Path::new(&received.path).join("nested/note.MD")).unwrap(),
                fs::read(source.path().join("nested/note.MD")).unwrap()
            );
            assert_eq!(
                fs::read(Path::new(&received.path).join("images/photo.png")).unwrap(),
                fs::read(source.path().join("images/photo.png")).unwrap()
            );
        }
    }
}
