//! Hosted document parsing. No API credential is attached to storage requests.
use anyhow::{bail, ensure, Context, Result};
use futures_util::StreamExt;
use reqwest::{Client, Response, Url};
use sag_protocol::mineru::{MineruConfig, MineruProvider};
use sag_protocol::runtime::RuntimeSettings;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Condvar, LazyLock, Mutex},
    time::Duration,
};

const MIB: u64 = 1024 * 1024;
const ZIP_LIMIT: u64 = 200 * MIB;
const PAGE_LIMIT: u32 = 200;

pub fn supports(source: &Path) -> bool {
    source
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            [
                "pdf", "png", "jpg", "jpeg", "jp2", "webp", "gif", "bmp", "doc", "docx", "ppt",
                "pptx", "xls", "xlsx",
            ]
            .iter()
            .any(|known| ext.eq_ignore_ascii_case(known))
        })
}
pub fn save_key(dir: &Path, config: &MineruConfig, key: &str) -> Result<()> {
    crate::models::save_secret(dir, config.provider.credential_kind(), key)
}
pub fn key(dir: &Path, config: &MineruConfig) -> Result<String> {
    crate::models::secret(dir, config.provider.credential_kind())
        .context("请在模型设置中填写当前 MinerU 服务商的 API Key")
}
pub fn has_key(dir: &Path, config: &MineruConfig) -> Result<bool> {
    crate::models::has_secret(dir, config.provider.credential_kind())
}
pub fn clear_key(dir: &Path, config: &MineruConfig) -> Result<()> {
    crate::models::clear_secret(dir, config.provider.credential_kind())
}
fn safe_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).context("MinerU 返回的地址无效")?;
    let secure = url.scheme() == "https";
    #[cfg(test)]
    let secure = secure || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    ensure!(
        secure && url.host_str().is_some() && url.username().is_empty() && url.password().is_none(),
        "MinerU 返回的地址不安全"
    );
    Ok(url)
}
fn client(timeout: u64) -> Result<Client> {
    Ok(Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15.min(timeout)))
        .timeout(Duration::from_secs(timeout))
        .build()?)
}
fn endpoint(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path)
}
fn task_path(provider: MineruProvider, id: &str) -> String {
    match provider {
        MineruProvider::Ai302 => format!("302/v2/mineru/task?task_id={id}"),
        MineruProvider::Official => format!("extract-results/batch/{id}"),
    }
}
fn sanitized(error: anyhow::Error) -> anyhow::Error {
    // reqwest and JSON errors can include signed storage URLs or remote content.
    if error.chain().any(|cause| cause.is::<reqwest::Error>()) {
        anyhow::anyhow!("MinerU 网络请求失败或超时，请检查网络后重试")
    } else {
        error
    }
}
async fn checked(response: Response) -> Result<Response> {
    ensure!(
        response.status().is_success(),
        "MinerU 请求失败（HTTP {}），请检查密钥、额度或稍后重试",
        response.status().as_u16()
    );
    Ok(response)
}
async fn bytes(response: Response, limit: u64) -> Result<Vec<u8>> {
    let response = checked(response).await?;
    ensure!(
        response.content_length().unwrap_or(0) <= limit,
        "MinerU 返回内容超过大小限制"
    );
    let mut stream = response.bytes_stream();
    let mut output = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        ensure!(
            output.len() as u64 + chunk.len() as u64 <= limit,
            "MinerU 返回内容超过大小限制"
        );
        output.extend_from_slice(&chunk);
    }
    Ok(output)
}
async fn response_json(response: Response, success: i64) -> Result<Value> {
    let value: Value = serde_json::from_slice(&bytes(response, MIB).await?)
        .map_err(|_| anyhow::anyhow!("MinerU 返回了无效 JSON"))?;
    ensure!(
        value["code"].as_i64() == Some(success),
        "MinerU 服务拒绝请求，请检查密钥、额度及文件限制"
    );
    Ok(value["data"].clone())
}
async fn submission_json(response: Response, success: i64, path: &Path) -> Result<Value> {
    // Explicit client rejection confirms that no task was accepted. Keep the
    // uncertainty marker for transport loss/server failure instead of charging twice.
    if response.status().is_client_error() {
        let _ = std::fs::remove_file(path);
    }
    response_json(response, success).await
}

fn field<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    value[name]
        .as_str()
        .filter(|value| !value.is_empty())
        .with_context(|| format!("MinerU 响应缺少 {name}"))
}
fn task_id(value: &Value, name: &str) -> Result<String> {
    let id = field(value, name)?;
    ensure!(
        id.len() <= 256
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)),
        "MinerU 任务 ID 无效"
    );
    Ok(id.into())
}

fn unwrap_payload(mut value:Value)->Value {
    for _ in 0..4 {if let Some(text)=value.as_str(){if let Ok(decoded)=serde_json::from_str::<Value>(text){if decoded!=value {value=decoded;continue;}}}break;}value
}
async fn opensource_json(response:Response,limit:u64)->Result<Value> {
    let raw=bytes(response,limit.max(MIB)).await?;
    let value=unwrap_payload(serde_json::from_slice(&raw).unwrap_or_else(|_|json!(String::from_utf8_lossy(&raw))));
    if let Some(code)=value.get("code") {ensure!(["0","200","ok","success"].contains(&code.as_str().map(str::to_owned).unwrap_or_else(||code.to_string()).to_lowercase().as_str()),"MinerU 服务返回错误码，请检查权限、额度和解析参数");}
    Ok(value)
}
fn nested_text(value:&Value,keys:&[&str])->Option<String> {
    let value=unwrap_payload(value.clone());
    if let Some(map)=value.as_object(){for key in keys {if let Some(v)=map.get(*key){if let Some(text)=v.as_str(){if !text.trim().is_empty(){return Some(text.trim().into());}}if let Some(n)=v.as_u64(){return Some(n.to_string());}}}for key in ["data","result","task"] {if let Some(v)=map.get(key){if let Some(found)=nested_text(v,keys){return Some(found);}}}}
    None
}
fn is_markdown(text:&str)->bool {text.contains('\n')||text.starts_with("# ")||text.starts_with("## ")}
fn find_302_task_id(value:&Value)->Option<String> {
    let value=unwrap_payload(value.clone());
    if let Some(text)=value.as_str(){let text=text.trim();let state=text.to_lowercase();if !text.is_empty()&&text.bytes().all(|b|b.is_ascii_alphanumeric()||b"-_".contains(&b))&&!((state.len()==5&&state.starts_with('a')||state.len()==6&&state.starts_with('-'))&&state[1..].bytes().all(|b|b.is_ascii_digit()))&&!["pending","queued","running","processing","done","success","failed","error"].contains(&state.as_str()){return Some(text.into());}return None;}
    if let Some(id)=nested_text(&value,&["task_id","taskId","request_id","id"]){return Some(id);}
    for key in ["data","result","task"] {if let Some(child)=value.get(key){if let Some(id)=find_302_task_id(child){return Some(id);}}}None
}
fn normalize_opensource(value:Value)->Result<Value> {
    let value=unwrap_payload(value);
    if let Some(text)=value.as_str(){let text=text.trim();if text.starts_with("https://")||text.starts_with("http://"){return Ok(json!({"state":"done","full_zip_url":text}));}if is_markdown(text){return Ok(json!({"state":"done","markdown":text}));}let state=text.to_lowercase();if (state.len()==5&&state.starts_with('a')&&state[1..].bytes().all(|c|c.is_ascii_digit()))||(state.len()==6&&state.starts_with('-')&&state[1..].bytes().all(|c|c.is_ascii_digit())){return Ok(json!({"state":"failed"}));}return Ok(json!({"state":match state.as_str(){"failed"|"failure"|"error"|"cancelled"|"canceled"=>"failed",_=>"pending"}}));}
    let state=nested_text(&value,&["status","state","task_status","taskState"]).unwrap_or_default().to_lowercase().replace(['-',' '],"_");
    if ["failed","failure","error","cancelled","canceled"].contains(&state.as_str()){return Ok(json!({"state":"failed"}));}
    if ["created","pending","queued","queueing","running","processing","converting","in_progress"].contains(&state.as_str()){return Ok(json!({"state":"pending"}));}
    if let Some(url)=nested_text(&value,&["full_zip_url","zip_url","download_url","result_url","markdown_url","md_url","file_url"]){safe_url(&url)?;return Ok(json!({"state":"done","full_zip_url":url}));}
    if let Some(md)=nested_text(&value,&["markdown","md_content","markdown_content","content"]){if is_markdown(&md){return Ok(json!({"state":"done","markdown":md}));}}
    if ["done","success","succeeded","completed","finished"].contains(&state.as_str()) {if let Some(url)=nested_text(&value,&["url"]){safe_url(&url)?;return Ok(json!({"state":"done","full_zip_url":url}));}bail!("MinerU 任务已完成，但未返回正文或结果地址");}
    for key in ["data","result","task"] {if let Some(v)=value.get(key){return normalize_opensource(v.clone());}}
    Ok(json!({"state":"pending"}))
}
async fn finish_302(client:&Client,result:&Value,part:&Part,require_pages:bool,limit:u64)->Result<String> {
    if let Some(md)=result.get("markdown").and_then(Value::as_str){ensure!(!require_pages,"MinerU 分段结果缺少页码映射，尝试本地解析");ensure!(md.len() as u64<=limit,"MinerU 正文超过配置上限");return Ok(md.into());}
    let response=client.get(safe_url(field(result,"full_zip_url")?)?).send().await?;
    let content=bytes(response,ZIP_LIMIT).await?;
    if content.starts_with(b"PK\x03\x04"){return archive_content_with_limit(content,part.start,part.end,require_pages,limit);}
    let payload=unwrap_payload(serde_json::from_slice(&content).unwrap_or_else(|_|json!(String::from_utf8_lossy(&content))));
    let result=normalize_opensource(payload)?;
    if let Some(md)=result.get("markdown").and_then(Value::as_str){ensure!(!require_pages,"MinerU 分段结果缺少页码映射，尝试本地解析");ensure!(md.len() as u64<=limit,"MinerU 正文超过配置上限");return Ok(md.into());}
    bail!("MinerU 下载结果不是有效的 Markdown 或 ZIP")
}

/// This is a reachability/authentication probe, not a document conversion.
pub async fn test_connection(config: &MineruConfig, key: &str) -> Result<String> {
    test_connection_at(config, key, config.provider.base_url()).await
}
async fn test_connection_at(config: &MineruConfig, key: &str, base: &str) -> Result<String> {
    config.validate().map_err(anyhow::Error::msg)?;
    ensure!(!key.trim().is_empty(), "请填写 MinerU API Key");
    let result = async {
        let response = client(config.request_timeout_secs.min(15))?
            .get(endpoint(
                base,
                &if config.provider==MineruProvider::Ai302 {"302/v2/mineru/task".into()} else {task_path(config.provider,"sag-connection-check-no-task")},
            ))
            .bearer_auth(key.trim())
            .send()
            .await?;
        match response.status().as_u16() {

            401 => bail!("MinerU 鉴权未通过（HTTP 401）。请确认密钥属于当前服务商，且未过期或被撤销"),
            403 => bail!("MinerU 拒绝访问（HTTP 403）。请检查当前密钥的接口权限、IP 白名单或服务商访问限制；这不一定是密钥错误"),
            _ => {}
        }
        if config.provider==MineruProvider::Ai302 && matches!(response.status().as_u16(),400|422) {return Ok("MinerU 接口可达；未提交解析任务，尚未验证实际解析权限".into());}
        if response.status().as_u16() == 404 {
            return Ok("服务可达；测试任务不存在，尚未确认密钥有效，未上传文件".into());
        }
        let data: Value = serde_json::from_slice(&bytes(response, MIB).await?)
            .map_err(|_| anyhow::anyhow!("MinerU 测试响应无效"))?;
        match data["code"].as_i64() {
            Some(200) if config.provider == MineruProvider::Ai302 => Ok("服务可达；尚未确认实际解析权限，未上传文件".into()),
            Some(-60012) | Some(0) => Ok("服务可达；尚未确认实际解析权限，未上传文件".into()),
            _ => bail!("MinerU 测试被拒绝，请检查密钥、额度和服务状态"),
        }
    }
    .await;
    result.map_err(sanitized)
}

// Admission is separate from local Docling's memory budget.
static ACTIVE: LazyLock<(Mutex<HashMap<MineruProvider, usize>>, Condvar)> =
    LazyLock::new(|| (Mutex::new(HashMap::new()), Condvar::new()));
struct Admission(MineruProvider);
impl Drop for Admission {
    fn drop(&mut self) {
        let mut counts = ACTIVE.0.lock().unwrap();
        *counts.entry(self.0).or_default() -= 1;
        ACTIVE.1.notify_all();
    }
}
fn admission(config: &MineruConfig, cancel: &impl Fn() -> bool) -> Result<Admission> {
    let mut counts = ACTIVE.0.lock().unwrap();
    loop {
        ensure!(!cancel(), "已停止 MinerU 转换，未发布输出");
        if *counts.entry(config.provider).or_default() < config.concurrency {
            *counts.entry(config.provider).or_default() += 1;
            return Ok(Admission(config.provider));
        }
        counts = ACTIVE
            .1
            .wait_timeout(counts, Duration::from_millis(50))
            .unwrap()
            .0;
    }
}

// A resumed task journal must have only one submitter, even when two index jobs
// reach the same source simultaneously. The service owns one process per user.
static TASK_LOCKS: LazyLock<(Mutex<HashSet<PathBuf>>, Condvar)> =
    LazyLock::new(|| (Mutex::new(HashSet::new()), Condvar::new()));
struct TaskLock(PathBuf);
impl Drop for TaskLock {
    fn drop(&mut self) {
        TASK_LOCKS.0.lock().unwrap().remove(&self.0);
        TASK_LOCKS.1.notify_all();
    }
}
fn task_lock(path: &Path, cancel: &impl Fn() -> bool) -> Result<TaskLock> {
    let mut active = TASK_LOCKS.0.lock().unwrap();
    loop {
        ensure!(!cancel(), "已停止等待 MinerU 任务，未发布输出");
        if active.insert(path.to_path_buf()) {
            return Ok(TaskLock(path.to_path_buf()));
        }
        active = TASK_LOCKS
            .1
            .wait_timeout(active, Duration::from_millis(50))
            .unwrap()
            .0;
    }
}

#[derive(Debug)]
struct Part {
    bytes: Vec<u8>,
    start: u32,
    end: u32,
    name: String,
}
#[cfg(test)]
fn prepare(source: &Path, limit: u64, cancel: &impl Fn() -> bool) -> Result<Vec<Part>> {
    prepare_with_runtime(source, limit, &RuntimeSettings::default(), cancel)
}
fn prepare_with_runtime(
    source: &Path,
    limit: u64,
    runtime: &RuntimeSettings,
    cancel: &impl Fn() -> bool,
) -> Result<Vec<Part>> {
    let input_limit = runtime.conversion_input_mib as u64 * MIB;
    ensure!(!cancel(), "已停止 MinerU 转换，未发布输出");
    ensure!(supports(source), "MinerU 不支持此文件格式");
    ensure!(
        std::fs::metadata(source)?.len() <= input_limit,
        "文档超过配置的转换输入大小限制"
    );
    let mut content = Vec::new();
    std::fs::File::open(source)?
        .take(input_limit + 1)
        .read_to_end(&mut content)?;
    ensure!(
        !content.is_empty() && content.len() as u64 <= input_limit,
        "文档为空或超过转换输入限制"
    );
    let name = source
        .file_name()
        .context("缺少文件名")?
        .to_string_lossy()
        .into_owned();
    if !source
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
    {
        ensure!(
            content.len() as u64 <= limit,
            "文件超过所选 MinerU 服务商的上传大小限制"
        );
        return Ok(vec![Part {
            bytes: content,
            start: 0,
            end: 0,
            name,
        }]);
    }
    let document =
        lopdf::Document::load_mem(&content).context("无法读取 PDF 页码，尝试下一解析器")?;
    let count = u32::try_from(document.get_pages().len()).context("PDF 页数超过支持范围")?;
    ensure!(count > 0, "PDF 中没有可解析页面");
    ensure!(
        count as usize <= runtime.max_document_pages,
        "文档页数超过配置上限"
    );
    if count <= PAGE_LIMIT && content.len() as u64 <= limit {
        return Ok(vec![Part {
            bytes: content,
            start: 1,
            end: count,
            name,
        }]);
    }
    let mut ranges = std::collections::VecDeque::new();
    let mut start = 1;
    while start <= count {
        let end = (start + PAGE_LIMIT - 1).min(count);
        ranges.push_back((start, end));
        start = end + 1;
    }
    let mut parts = Vec::new();
    while let Some((start, end)) = ranges.pop_front() {
        ensure!(!cancel(), "已停止 MinerU 转换，未发布输出");
        let mut split = document.clone();
        let remove = (1..=count)
            .filter(|page| *page < start || *page > end)
            .collect::<Vec<_>>();
        split.delete_pages(&remove);
        split.prune_objects();
        split.compress();
        let mut bytes = Vec::new();
        split.save_to(&mut bytes)?;
        if bytes.len() as u64 > limit {
            ensure!(start < end, "PDF 单页超过所选 MinerU 服务商上传限制");
            let middle = start + (end - start) / 2;
            ranges.push_front((middle + 1, end));
            ranges.push_front((start, middle));
            continue;
        }
        parts.push(Part {
            bytes,
            start,
            end,
            name: format!("pages-{start}-{end}.pdf"),
        });
    }
    Ok(parts)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Task {
    id: Option<String>,
    upload: Option<String>,
    uploaded: bool,
    submitted_at: u64,
    #[serde(default,skip_serializing_if="Option::is_none")]
    result: Option<Value>,
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn write_task(path: &Path, task: &Task) -> Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().context("任务记录路径无效")?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(&serde_json::to_vec(task)?)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}
fn journal(output: &Path, part: &Part, config: &MineruConfig, key: &str) -> Result<PathBuf> {
    let dir = output
        .parent()
        .context("输出路径缺少父目录")?
        .join(".sag-mineru-tasks");
    if let Ok(meta) = dir.symlink_metadata() {
        ensure!(
            !meta.file_type().is_symlink(),
            "MinerU 任务目录不能为符号链接"
        );
    }
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let mut hash = Sha256::new();
    hash.update(config.content_identity());
    hash.update(key);
    hash.update(&part.bytes);
    Ok(dir.join(format!("{:x}.json", hash.finalize())))
}
fn read_task(path: &Path) -> Result<Option<Task>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(
            serde_json::from_slice(&bytes).context("MinerU 任务记录无效")?,
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn inline_images(text: &str) -> String {
    // Result assets expire remotely; use explicit image placeholders rather than broken relative links.
    static IMAGES: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"!\[([^\]]*)\]\([^\n]*?\)|(?is:<img\b[^>]*>)").unwrap()
    });
    IMAGES
        .replace_all(text, |captures: &regex::Captures| {
            format!(
                "[图片：{}]",
                captures.get(1).map_or("", |value| value.as_str())
            )
        })
        .into_owned()
}
fn effective_markdown(text: &str) -> Result<String> {
    let output = inline_images(text);
    // Ignore only presentation when deciding whether OCR produced usable text.
    // Keep the original Markdown, formulas and table bodies in the actual result.
    static MARKUP: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?s:<!--.*?-->|<[^>]*>)|\[图片：\s*\]|(?i:&nbsp;|&#160;|&#x0*a0;)")
            .unwrap()
    });
    let content = MARKUP.replace_all(&output, "");
    ensure!(
        content.chars().any(char::is_alphanumeric),
        "MinerU 未提取到有效正文，尝试下一解析器"
    );
    Ok(output)
}

#[cfg(test)]
fn archive_content(archive: Vec<u8>, start: u32, end: u32, require_pages: bool) -> Result<String> {
    archive_content_with_limit(
        archive,
        start,
        end,
        require_pages,
        RuntimeSettings::default().conversion_output_mib as u64 * MIB,
    )
}
fn archive_content_with_limit(
    archive: Vec<u8>,
    start: u32,
    end: u32,
    require_pages: bool,
    output_limit: u64,
) -> Result<String> {
    let mut zip =
        zip::ZipArchive::new(std::io::Cursor::new(archive)).context("MinerU 结果不是有效 ZIP")?;
    let mut full = None;
    let mut content_list = None;
    for i in 0..zip.len() {
        let entry = zip.by_index(i)?;
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        if entry.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name != "full.md" && !name.ends_with("_content_list.json") {
            continue;
        }
        ensure!(entry.size() <= output_limit, "MinerU 正文超过大小限制");
        let mut text = String::new();
        entry.take(output_limit + 1).read_to_string(&mut text)?;
        ensure!(text.len() as u64 <= output_limit, "MinerU 正文超过大小限制");
        if name == "full.md" {
            full = Some(text);
        } else {
            content_list = Some(text);
        }
    }
    // Full Markdown preserves every supported construct for ordinary files.
    if !require_pages {
        let full = full.context("MinerU 结果中缺少 full.md")?;
        ensure!(!full.trim().is_empty(), "MinerU 未返回有效正文");
        return effective_markdown(&full);
    }
    if let Some(list) = content_list {
        let list: Value = serde_json::from_str(&list).context("MinerU 页码映射无效")?;
        let rows = list.as_array().context("MinerU 页码映射无效")?;
        let mut output = String::new();
        let mut previous = None;
        let mut body_bytes = 0;
        for row in rows {
            let page = u32::try_from(
                row["page_idx"]
                    .as_u64()
                    .context("MinerU 页码映射缺少页号")?,
            )
            .context("MinerU 页码映射无效")?;
            let original = start
                .max(1)
                .checked_add(page)
                .context("MinerU 页码映射无效")?;
            ensure!(end == 0 || original <= end, "MinerU 页码映射超出原文件范围");
            let strings = |name: &str| -> Result<String> {
                match row.get(name) {
                    None | Some(Value::Null) => Ok(String::new()),
                    Some(Value::String(text)) => Ok(text.clone()),
                    Some(Value::Array(items)) => items
                        .iter()
                        .map(|item| {
                            item.as_str()
                                .map(str::to_owned)
                                .context("MinerU 结构化正文格式无效")
                        })
                        .collect::<Result<Vec<_>>>()
                        .map(|items| items.join("\n\n")),
                    _ => bail!("MinerU 结构化正文格式无效"),
                }
            };
            // MinerU names the same value differently across versions, so take the
            // first populated field and ignore a block that carries nothing usable.
            let text_of = |name: &str| -> String {
                match row.get(name) {
                    Some(Value::String(text)) => text.clone(),
                    Some(Value::Array(items)) => items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                        .join("\n\n"),
                    _ => String::new(),
                }
            };
            let first = |names: &[&str]| {
                names
                    .iter()
                    .map(|name| text_of(name))
                    .find(|value| !value.trim().is_empty())
                    .unwrap_or_default()
            };
            let kind = row["type"].as_str().unwrap_or("");
            let mut body = String::new();
            match kind {
                // Running headers, folios and footers repeat on every page and only
                // add retrieval noise; full.md leaves them out for single-part files.
                "header" | "footer" | "page_number" => continue,
                "image" => {
                    body.push_str(&format!("[图片：{}]", strings("image_caption")?));
                    body.push_str(&strings("image_footnote")?);
                }
                "chart" => {
                    body.push_str(&format!("[图片：{}]", first(&["chart_caption", "content", "text"])));
                    body.push_str(&first(&["chart_footnote"]));
                }
                "table" => {
                    body.push_str(&strings("table_caption")?);
                    body.push_str("\n\n");
                    let table = strings("table_body")?;
                    ensure!(
                        !table.trim().is_empty(),
                        "MinerU 分段表格缺少正文，尝试下一解析器"
                    );
                    body.push_str(&table);
                    body.push_str("\n\n");
                    body.push_str(&strings("table_footnote")?);
                }
                "list" => {
                    for item in strings("list_items")?.split("\n\n") {
                        body.push_str("- ");
                        body.push_str(item);
                        body.push('\n');
                    }
                }
                "equation" => {
                    let text = strings("text")?;
                    ensure!(!text.trim().is_empty(), "MinerU 分段公式缺少正文");
                    body.push_str("$$\n");
                    body.push_str(&text);
                    body.push_str("\n$$");
                }
                "text" => {
                    if let Some(level) = row["text_level"].as_u64() {
                        body.push_str(&"#".repeat(level.clamp(1, 6) as usize));
                        body.push(' ');
                    }
                    body.push_str(&strings("text")?);
                }
                "code" => {
                    body.push_str(&first(&["code_caption"]));
                    body.push_str("\n\n");
                    let code = first(&["code_body", "text"]);
                    ensure!(!code.trim().is_empty(), "MinerU 分段代码块缺少正文，尝试下一解析器");
                    // Outrun any backtick run inside the code so the fence cannot close early.
                    let fence = "`".repeat(
                        code.split(|value| value != '`')
                            .map(str::len)
                            .max()
                            .unwrap_or(0)
                            .max(2)
                            + 1,
                    );
                    let language = first(&["guess_lang", "code_language"]);
                    let language = language.trim();
                    let language = if language
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"+-_#".contains(&byte))
                    {
                        language
                    } else {
                        ""
                    };
                    body.push_str(&fence);
                    body.push_str(language);
                    body.push('\n');
                    body.push_str(code.trim_end_matches(['\n', '\r']));
                    body.push('\n');
                    body.push_str(&fence);
                    body.push('\n');
                    body.push_str(&first(&["code_footnote"]));
                }
                other => {
                    // Unknown named types keep their text; only a block with nothing
                    // readable at all fails, and the message names it for diagnosis.
                    let body_key = format!("{other}_body");
                    let text_key = format!("{other}_text");
                    let text = first(&["text", "content", "body", &body_key, &text_key]);
                    let invalid = format!("\"{}\"", other.chars().take(32).collect::<String>());
                    ensure!(
                        !text.trim().is_empty(),
                        "MinerU 分段第 {original} 页包含不支持的类型 {invalid}，尝试下一解析器"
                    );
                    body.push_str(&text);
                }
            }
            if body.trim().is_empty() {
                continue;
            }
            body_bytes += body.trim().len();
            if previous != Some(original) {
                output.push_str(&format!(
                    "\n\n<a id=\"page-{original}\"></a>\n<!-- source-page:{original} -->\n\n"
                ));
                previous = Some(original);
            }
            output.push_str(&body);
            output.push_str("\n\n");
        }
        ensure!(body_bytes > 0, "MinerU 未返回有效正文");
        return effective_markdown(&output);
    }
    bail!("MinerU 分段结果缺少页码映射，尝试下一解析器")
}
#[cfg(test)]
fn markdown(archive: Vec<u8>) -> Result<String> {
    archive_content(archive, 0, 0, false)
}

#[cfg(test)]
async fn parse_part(
    client: &Client,
    base: &str,
    part: &Part,
    path: &Path,
    config: &MineruConfig,
    key: &str,
    require_pages: bool,
    progress: &impl Fn(&str),
) -> Result<String> {
    parse_part_with_limit(
        client,
        base,
        part,
        path,
        config,
        key,
        require_pages,
        progress,
        RuntimeSettings::default().conversion_output_mib as u64 * MIB,
    )
    .await
}
async fn parse_part_with_limit(
    client: &Client,
    base: &str,
    part: &Part,
    path: &Path,
    config: &MineruConfig,
    key: &str,
    require_pages: bool,
    progress: &impl Fn(&str),
    output_limit: u64,
) -> Result<String> {
    let mut task = match read_task(path)? {
        Some(task) => {
            ensure!(
                task.id.is_some() || task.result.is_some(),
                "MinerU 上次提交未返回任务编号，无法续查；已保留记录以避免重复提交，请使用本地解析或联系服务商确认任务"
            );
            task
        }
        None => {
            progress("uploading");
            let mut task = Task {
                result: None,
                id: None,
                upload: None,
                uploaded: false,
                submitted_at: now(),
            };
            // Persist before a non-idempotent request: a lost response must not trigger another paid task.
            write_task(path, &task)?;
            let options = json!({"model_version":config.model_version,"is_ocr":config.ocr,"enable_table":config.table,"enable_formula":config.formula});
            if config.provider != MineruProvider::Official {
                let form = reqwest::multipart::Form::new().part(
                    "file",
                    reqwest::multipart::Part::bytes(part.bytes.clone())
                        .file_name(part.name.clone()).mime_str("application/pdf")?,
                );
                let uploaded = submission_json(
                    client
                        .post(endpoint(base, "302/upload-file"))
                        .bearer_auth(key)
                        .multipart(form)
                        .send()
                        .await?,
                    200,
                    path,
                )
                .await?;
                let file_url = uploaded.as_str().context("MinerU 上传结果缺少文件地址")?;
                safe_url(file_url)?;
                let body=json!({"pdf_url":file_url,"version":config.version,"parse_method":config.parse_method});
                let response=client.post(endpoint(base,"302/v2/mineru/task")).bearer_auth(key.trim()).json(&body).send().await?;
                if response.status().is_client_error(){let _=std::fs::remove_file(path);}
                let created=opensource_json(response,output_limit).await?;
                if let Some(id)=find_302_task_id(&created) {task.id=Some(task_id(&json!({"task_id":id}),"task_id")?);} else {
                    let direct=normalize_opensource(created)?;
                    if direct["state"]=="failed" {let _=std::fs::remove_file(path);bail!("302.AI 拒绝解析请求，请检查密钥权限、额度或文件参数");}
                    ensure!(direct["state"]=="done","MinerU 创建任务未返回任务编号或解析结果");task.result=Some(direct);
                }
                task.uploaded = true;
            } else {
                let mut body = options;
                body["files"] = json!([{"name":part.name,"is_ocr":config.ocr}]);
                let created = submission_json(
                    client
                        .post(endpoint(base, "file-urls/batch"))
                        .bearer_auth(key)
                        .json(&body)
                        .send()
                        .await?,
                    0,
                    path,
                )
                .await?;
                task.id = Some(task_id(&created, "batch_id")?);
                task.upload = Some(
                    created["file_urls"][0]
                        .as_str()
                        .context("MinerU 响应缺少上传地址")?
                        .into(),
                );
            }
            write_task(path, &task)?;
            task
        }
    };
    if !task.uploaded {
        progress("uploading");
        let upload = task.upload.as_deref().context("MinerU 任务缺少上传地址")?;
        ensure!(
            now().saturating_sub(task.submitted_at) < 24 * 3600,
            "MinerU 上传地址已过期，请清除该文件解析缓存后重试"
        );
        checked(
            client
                .put(safe_url(upload)?)
                .body(part.bytes.clone())
                .send()
                .await?,
        )
        .await?;
        task.uploaded = true;
        task.upload = None;
        write_task(path, &task)?;
    }
    if let Some(result)=&task.result {progress("downloading");return finish_302(client,result,part,require_pages,output_limit).await;}
    progress("queued");
    let poll = endpoint(
        base,
        &task_path(config.provider, task.id.as_deref().unwrap()),
    );
    loop {
        let response=client.get(&poll).bearer_auth(key.trim()).send().await?;
        let result = if config.provider == MineruProvider::Ai302 {opensource_json(response,output_limit).await?} else {response_json(response,0).await?};
        let result = if config.provider == MineruProvider::Ai302 { normalize_opensource(result)? } else {result};
        let result = if config.provider == MineruProvider::Official {
            &result["extract_result"][0]
        } else {
            &result
        };
        match field(result, "state")? {
            "done" => {
                progress("downloading");
                if config.provider==MineruProvider::Ai302 {
                    return finish_302(client,result,part,require_pages,output_limit).await;
                }
                return archive_content_with_limit(
                    bytes(
                        client
                            .get(safe_url(field(result, "full_zip_url")?)?)
                            .send()
                            .await?,
                        ZIP_LIMIT,
                    )
                    .await?,
                    part.start,
                    part.end,
                    require_pages,
                    output_limit,
                );
            }
            "failed" => {
                std::fs::remove_file(path)?;
                bail!("MinerU 解析失败，尝试下一解析器");
            }
            "waiting-file" | "pending" => {
                progress("queued");
                tokio::time::sleep(Duration::from_secs(config.poll_interval_secs)).await;
            }
            "running" | "converting" => {
                progress("parsing");
                tokio::time::sleep(Duration::from_secs(config.poll_interval_secs)).await;
            }
            _ => bail!("MinerU 返回了未知任务状态"),
        }
    }
}

#[cfg(test)]
pub fn convert(
    source: &Path,
    output: &Path,
    config: &MineruConfig,
    key: &str,
    cancel: &impl Fn() -> bool,
) -> Result<()> {
    convert_with_progress(source, output, config, key, cancel, &|_| {})
}
#[cfg(test)]
pub fn convert_with_progress(
    source: &Path,
    output: &Path,
    config: &MineruConfig,
    key: &str,
    cancel: &impl Fn() -> bool,
    progress: &impl Fn(&str),
) -> Result<()> {
    convert_at_with_progress(
        source,
        output,
        config,
        key,
        cancel,
        config.provider.base_url(),
        &RuntimeSettings::default(),
        progress,
    )
}
pub fn convert_with_runtime(
    source: &Path,
    output: &Path,
    config: &MineruConfig,
    key: &str,
    runtime: &RuntimeSettings,
    cancel: &impl Fn() -> bool,
    progress: &impl Fn(&str),
) -> Result<()> {
    convert_at_with_progress(
        source,
        output,
        config,
        key,
        cancel,
        config.provider.base_url(),
        runtime,
        progress,
    )
}
#[cfg(test)]
fn convert_at(
    source: &Path,
    output: &Path,
    config: &MineruConfig,
    key: &str,
    cancel: &impl Fn() -> bool,
    base: &str,
) -> Result<()> {
    convert_at_with_progress(
        source,
        output,
        config,
        key,
        cancel,
        base,
        &RuntimeSettings::default(),
        &|_| {},
    )
}
fn convert_at_with_progress(
    source: &Path,
    output: &Path,
    config: &MineruConfig,
    key: &str,
    cancel: &impl Fn() -> bool,
    base: &str,
    runtime: &RuntimeSettings,
    progress: &impl Fn(&str),
) -> Result<()> {
    if config.provider == MineruProvider::Ai302 {
        ensure!(source.extension().is_some_and(|ext|ext.eq_ignore_ascii_case("pdf")),"302.AI 开源部署版仅支持 PDF，尝试本地解析");
    }
    config.validate().map_err(anyhow::Error::msg)?;
    ensure!(!cancel(), "已停止 MinerU 转换，未发布输出");
    ensure!(!key.trim().is_empty(), "请填写 MinerU API Key");
    let _admission = admission(config, cancel)?;
    progress("preparing");
    let started = std::time::Instant::now();
    let parts = prepare_with_runtime(
        source,
        config.provider.upload_limit_bytes(),
        runtime,
        cancel,
    )?;
    let output_limit = runtime.conversion_output_mib as u64 * MIB;
    let prepared_elapsed = started.elapsed();
    let paths = parts
        .iter()
        .map(|part| journal(output, part, config, key))
        .collect::<Result<Vec<_>>>()?;
    // Journals are keyed by segment content: different documents can share a later one.
    // Lock every unique journal in a stable order, including when segments repeat.
    let mut lock_paths = paths.clone();
    lock_paths.sort();
    lock_paths.dedup();
    let _task_locks = lock_paths.iter().map(|path| task_lock(path, cancel)).collect::<Result<Vec<_>>>()?;
    // Waiting for the same source does not consume its conversion time budget.
    let remaining = Duration::from_secs(config.task_timeout_secs).saturating_sub(prepared_elapsed);
    ensure!(!remaining.is_zero(), "MinerU 文件准备超过任务时间限制");
    ensure!(!cancel(), "已停止 MinerU 转换，未发布输出");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result:Result<String>=runtime.block_on(async{
        let operation=async{
            let http=client(config.request_timeout_secs)?;
            let mut output_text=String::new();
            for (part,path) in parts.iter().zip(&paths){
                let text=parse_part_with_limit(&http,base,part,path,config,key,parts.len()>1,progress,output_limit).await?;
                ensure!(output_text.len() as u64+text.len()as u64<=output_limit,"MinerU 合并正文超过大小限制");
                if part.start>0{output_text.push_str(&format!("\n\n<!-- source-pages:{}-{} -->\n\n",part.start,part.end));}
                output_text.push_str(&text);
            }
            Ok(output_text)
        };
        tokio::pin!(operation);
        let deadline=tokio::time::sleep(remaining);tokio::pin!(deadline);
        loop{
            ensure!(!cancel(),"已停止 MinerU 转换，未发布输出");
            tokio::select!{result=&mut operation=>return result,_=&mut deadline=>bail!("MinerU 解析超时，任务已保留以供下次继续查询"),_=tokio::time::sleep(Duration::from_millis(50))=>{}}
        }
    });
    let text = result.map_err(sanitized)?;
    ensure!(
        text.len() as u64 <= output_limit,
        "MinerU 合并正文超过配置的大小限制"
    );
    ensure!(!cancel(), "已停止 MinerU 转换，未发布输出");
    let mut file = tempfile::NamedTempFile::new_in(output.parent().context("输出路径无效")?)?;
    file.write_all(text.as_bytes())?;
    file.persist(output).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
#[path = "mineru_tests.rs"]
mod tests;
