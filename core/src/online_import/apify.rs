use super::{
    digest, documents_dir, ensure_index_root, install_content, name_part, next_run, now, remote,
    tracked_item, Outcome, Report,
};
use crate::{indexer, models, service::Service};
use anyhow::{bail, ensure, Context, Result};
use reqwest::{Client, Response, StatusCode, Url};
use rusqlite::params;
use sag_protocol::online_import::{
    ApifyAction, ApifyMapping, Connection, Platform, Schedule, Source,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

const ORIGIN: &str = "https://api.apify.com/v2";
const CRAWLER: &str = "apify/website-content-crawler";
const BODY_LIMIT: usize = 12 * 1024 * 1024;
const IMPORT_PAGE: usize = 20;
const STORE_PAGE: usize = 20;

struct Apify<'a> {
    http: Client,
    origin: &'a str,
    token: &'a str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Run {
    id: String,
    act_id: String,
    status: String,
    default_dataset_id: String,
    default_key_value_store_id: String,
}

#[derive(Deserialize, Serialize)]
struct StoreActor {
    id: String,
    username: String,
    name: String,
    title: String,
    description: Option<String>,
}

#[derive(Deserialize)]
struct StorePage {
    total: usize,
    items: Vec<StoreActor>,
}

fn identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
}

fn actor_id(raw: &str) -> Result<String> {
    let raw = raw.trim();
    if let Ok(url) = Url::parse(raw) {
        ensure!(
            url.scheme() == "https"
                && matches!(url.host_str(), Some("apify.com" | "www.apify.com"))
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none(),
            "请输入 Apify Store 链接或 Actor ID"
        );
        let mut parts = url
            .path_segments()
            .context("请输入 Apify Store 链接或 Actor ID")?;
        if let (Some(owner), Some(name)) = (parts.next(), parts.next()) {
            return actor_id(&format!("{owner}~{name}"));
        }
        bail!("请输入 Apify Store 链接或 Actor ID");
    }
    if let Some((owner, name)) = raw.split_once(['/', '~']) {
        ensure!(
            identifier(owner) && identifier(name),
            "请输入 Apify Store 链接或 Actor ID"
        );
        return Ok(format!("{owner}~{name}"));
    }
    ensure!(identifier(raw), "请输入 Apify Store 链接或 Actor ID");
    Ok(raw.into())
}

impl Apify<'_> {
    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let request = self.http.request(method, format!("{}{path}", self.origin));
        if self.token.is_empty() {
            request
        } else {
            request.bearer_auth(self.token)
        }
    }

    async fn check(&self, response: Response) -> Result<Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let body: Value = serde_json::from_slice(&remote::bounded_body(response, 64 * 1024).await?)
            .unwrap_or(Value::Null);
        let detail = body
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let detail = if self.token.is_empty() {
            detail.to_owned()
        } else {
            detail.replace(self.token, "[token]")
        };
        bail!(
            "Apify HTTP {} {}",
            status.as_u16(),
            detail.chars().take(400).collect::<String>()
        );
    }

    async fn json(&self, request: reqwest::RequestBuilder) -> Result<Value> {
        let response = self
            .check(request.send().await.context("无法连接 Apify")?)
            .await?;
        serde_json::from_slice(&remote::bounded_body(response, BODY_LIMIT).await?)
            .context("Apify 返回了无效的 JSON")
    }

    async fn actor(&self, id: &str) -> Result<Value> {
        let id = actor_id(id)?;
        let actor = self
            .json(self.request(reqwest::Method::GET, &format!("/actors/{id}")))
            .await?;
        Ok(actor["data"].clone())
    }

    async fn store(&self, search: &str, offset: usize) -> Result<Value> {
        let search = search.trim();
        ensure!(search.chars().count() <= 200, "搜索词最多 200 个字符");
        let sort = if search.is_empty() {
            "popularity"
        } else {
            "relevance"
        };
        let request = self
            .request(reqwest::Method::GET, "/store")
            .query(&[("limit", STORE_PAGE), ("offset", offset)])
            .query(&[("sortBy", sort)]);
        let request = if search.is_empty() {
            request
        } else {
            request.query(&[("search", search)])
        };
        let body = self.json(request).await?;
        let page: StorePage = serde_json::from_value(body["data"].clone())
            .context("Apify 未返回有效的 Actor 列表")?;
        let next = offset.saturating_add(page.items.len());
        Ok(json!({
            "items":page.items,"offset":offset,"limit":STORE_PAGE,
            "next":(next > offset && next < page.total).then_some(next),
        }))
    }

    async fn describe(&self, id: &str) -> Result<Value> {
        let actor = self.actor(id).await?;
        let id = actor["id"].as_str().context("Apify 未返回 Actor ID")?;
        ensure!(identifier(id), "Apify 返回了无效的 Actor ID");
        let build = self
            .json(self.request(
                reqwest::Method::GET,
                &format!("/actors/{id}/builds/default"),
            ))
            .await?;
        let definition = &build["data"]["actorDefinition"]["input"];
        let schema = if definition.is_object() {
            definition.clone()
        } else {
            build["data"]["inputSchema"]
                .as_str()
                .map(serde_json::from_str)
                .transpose()?
                .unwrap_or(Value::Null)
        };
        let slug = format!(
            "{}/{}",
            actor["username"].as_str().unwrap_or_default(),
            actor["name"].as_str().unwrap_or(id)
        );
        Ok(json!({
            "id": id,
            "slug": slug,
            "title": actor["title"].as_str().unwrap_or(&slug),
            "schema": schema,
            "preset": slug == CRAWLER,
        }))
    }

    async fn run(&self, id: &str) -> Result<Run> {
        ensure!(identifier(id), "Apify 运行 ID 无效");
        let body = self
            .json(self.request(reqwest::Method::GET, &format!("/actor-runs/{id}")))
            .await?;
        serde_json::from_value(body["data"].clone()).context("Apify 未返回有效的运行信息")
    }

    async fn start(&self, actor: &str, input: &Value) -> Result<String> {
        // Starting a run can incur charges. Never automatically retry this POST.
        let body = self
            .json(
                self.request(reqwest::Method::POST, &format!("/actors/{actor}/runs"))
                    .json(input),
            )
            .await?;
        let id = body
            .pointer("/data/id")
            .and_then(Value::as_str)
            .filter(|id| identifier(id))
            .context("Apify 未返回运行 ID")?;
        Ok(id.to_owned())
    }

    async fn abort(&self, id: &str) -> Result<()> {
        ensure!(identifier(id), "Apify 运行 ID 无效");
        self.json(self.request(reqwest::Method::POST, &format!("/actor-runs/{id}/abort")))
            .await?;
        Ok(())
    }

    async fn rows(&self, run: &Run, offset: usize, limit: usize) -> Result<DataPage> {
        ensure!(run.status == "SUCCEEDED", "Apify 任务尚未成功完成");
        let id = &run.default_dataset_id;
        ensure!(identifier(id), "Apify 数据集 ID 无效");
        let response = self
            .check(
                self.request(reqwest::Method::GET, &format!("/datasets/{id}/items"))
                    .query(&[("offset", offset), ("limit", limit)])
                    .send()
                    .await
                    .context("无法读取 Apify 数据集")?,
            )
            .await?;
        let total = response
            .headers()
            .get("x-apify-pagination-total")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok());
        let rows: Vec<Value> =
            serde_json::from_slice(&remote::bounded_body(response, BODY_LIMIT).await?)
                .context("Apify 未返回有效的数据集")?;
        if total == Some(0) || (offset == 0 && rows.is_empty()) {
            return self.output_record(run, offset, limit).await;
        }
        let end = offset
            .checked_add(rows.len())
            .context("Apify 数据集页码无效")?;
        let more = !rows.is_empty() && total.map_or(rows.len() == limit, |total| end < total);
        Ok(DataPage {
            rows,
            next: more.then_some(end),
            total,
        })
    }

    async fn output_record(&self, run: &Run, offset: usize, limit: usize) -> Result<DataPage> {
        let id = &run.default_key_value_store_id;
        ensure!(identifier(id), "Apify 存储 ID 无效");
        let response = self
            .request(
                reqwest::Method::GET,
                &format!("/key-value-stores/{id}/records/OUTPUT"),
            )
            .send()
            .await
            .context("无法读取 Apify 输出")?;
        let rows = if response.status() == StatusCode::NOT_FOUND {
            Vec::new()
        } else {
            let response = self.check(response).await?;
            let media = response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default();
            let is_json = media.starts_with("application/json");
            ensure!(
                is_json || media.starts_with("text/"),
                "Apify 输出需要是 JSON 或文本"
            );
            let bytes = remote::bounded_body(response, BODY_LIMIT).await?;
            let value: Value = if is_json {
                serde_json::from_slice(&bytes).context("Apify 返回了无效的 JSON")?
            } else {
                json!(String::from_utf8(bytes)?)
            };
            match value {
                Value::Array(rows) => rows,
                Value::Null => Vec::new(),
                row => vec![row],
            }
        };
        let total = rows.len();
        Ok(DataPage {
            rows: rows.into_iter().skip(offset).take(limit).collect(),
            next: offset
                .saturating_add(limit)
                .lt(&total)
                .then(|| offset + limit),
            total: Some(total),
        })
    }
}

struct DataPage {
    rows: Vec<Value>,
    next: Option<usize>,
    total: Option<usize>,
}

fn fields(
    value: &Value,
    path: &str,
    label: &str,
    depth: usize,
    result: &mut Vec<(String, String)>,
) {
    if depth == 0 || result.len() >= 500 {
        return;
    }
    if let Some(entries) = value.as_object() {
        for (key, value) in entries {
            let path = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
            let label = if label.is_empty() {
                key.to_owned()
            } else {
                format!("{label}.{key}")
            };
            if !result.iter().any(|(existing, _)| existing == &path) {
                result.push((path.clone(), label.clone()));
                if result.len() >= 500 {
                    return;
                }
            }
            fields(value, &path, &label, depth - 1, result);
        }
    }
}

fn body_text(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_owned();
    }
    if value.is_object() || value.is_array() {
        return format!(
            "~~~json\n{}\n~~~",
            serde_json::to_string_pretty(value).expect("JSON Value is serializable")
        );
    }
    value.to_string()
}

fn selected_text<'a>(row: &'a Value, pointer: &Option<String>) -> Option<&'a str> {
    row.pointer(pointer.as_deref()?)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn make_document(
    row: &Value,
    mapping: &ApifyMapping,
    number: usize,
) -> Option<(remote::Item, remote::Content)> {
    let parts: Vec<_> = mapping
        .content
        .iter()
        .filter_map(|pointer| {
            let value = row.pointer(pointer)?;
            if value.is_null() || value.as_str().is_some_and(|text| text.trim().is_empty()) {
                return None;
            }
            let body = body_text(value);
            if mapping.content.len() == 1 {
                Some(body)
            } else {
                let label = pointer
                    .trim_start_matches('/')
                    .replace('/', ".")
                    .replace("~1", "/")
                    .replace("~0", "~")
                    .replace(['\r', '\n'], " ");
                Some(format!("## {label}\n\n{body}"))
            }
        })
        .collect();
    if parts.is_empty() {
        return None;
    }
    let title = selected_text(row, &mapping.title)
        .map(|text| text.replace(['\r', '\n'], " "))
        .unwrap_or_else(|| format!("Apify {number}"));
    let url = selected_text(row, &mapping.url)
        .and_then(|url| Url::parse(url).ok())
        .filter(|url| {
            matches!(url.scheme(), "http" | "https")
                && url.username().is_empty()
                && url.password().is_none()
        });
    let identity = mapping
        .id
        .as_deref()
        .and_then(|pointer| row.pointer(pointer))
        .filter(|id| id.is_number() || id.as_str().is_some_and(|id| !id.trim().is_empty()))
        .map(Value::to_string)
        .or_else(|| url.as_ref().map(ToString::to_string))
        .unwrap_or_else(|| digest(row.to_string().as_bytes()));
    let body = parts.join("\n\n");
    let attribution = url
        .as_ref()
        .map(|url| format!("\n\n<{}>", url.as_str()))
        .unwrap_or_default();
    let heading = if body.starts_with("# ") {
        String::new()
    } else {
        format!("# {title}\n\n")
    };
    let bytes = format!("{heading}{body}{attribution}\n").into_bytes();
    let item = remote::Item {
        id: identity,
        title,
        version: digest(&bytes),
        link: String::new(),
        folders: vec![],
        kind: "apify".into(),
        inline: None,
    };
    Some((
        item,
        remote::Content {
            bytes,
            extension: "md".into(),
            assets: vec![],
        },
    ))
}

fn destination_parent(directory: &str) -> Result<PathBuf> {
    Ok(if directory.is_empty() {
        documents_dir()
            .context("无法获取文稿目录")?
            .join("sag-wiki/data-sources/apify")
    } else {
        let dir = Path::new(directory);
        ensure!(dir.is_absolute() && dir.is_dir(), "请选择已存在的保存目录");
        dir.canonicalize()?
    })
}

fn apify_source(actor: &Value, directory: &str) -> Result<Source> {
    let parent = destination_parent(directory)?;
    let actor_id = actor["id"].as_str().context("Apify 未返回 Actor ID")?;
    let name = actor["name"].as_str().unwrap_or(actor_id);
    let id = digest(format!("apify:{actor_id}:{}", parent.display()).as_bytes());
    let destination = parent.join(format!("{}-{}", name_part(name), &id[..8]));
    Ok(Source {
        id,
        name: name.into(),
        connection: Connection {
            platform: Platform::Apify,
            drive: false,
            user_access: false,
            app_id: actor_id.into(),
            address: String::new(),
        },
        resources: vec![],
        directory: destination.to_string_lossy().into_owned(),
        schedule: Schedule::Manual,
        daily_time: String::new(),
        paused: false,
        api: None,
    })
}

fn preset_mapping(actor: &Value) -> Result<ApifyMapping> {
    ensure!(
        actor["username"] == "apify" && actor["name"] == "website-content-crawler",
        "此 Actor 不适用网页爬虫预设"
    );
    Ok(ApifyMapping {
        title: Some("/metadata/title".into()),
        url: Some("/url".into()),
        id: None,
        content: vec!["/markdown".into()],
    })
}

#[derive(Default)]
struct Counts {
    imported: usize,
    updated: usize,
    skipped: usize,
    conflicts: usize,
    empty: usize,
}

async fn import(
    service: &Service,
    apify: &Apify<'_>,
    run: &str,
    directory: &str,
    offset: usize,
    preset: bool,
    mut mapping: ApifyMapping,
    saved: Option<Source>,
) -> Result<Value> {
    let _access = service
        .online_import
        .access
        .try_read()
        .context("正在清除本地数据")?;
    let run = apify.run(run).await?;
    let actor = apify.actor(&run.act_id).await?;
    if preset {
        mapping = preset_mapping(&actor)?;
    }
    ensure!(!mapping.content.is_empty(), "请选择要导入的正文字段");
    let page = apify.rows(&run, offset, IMPORT_PAGE).await?;
    let _import = service.online_import.apify_import.lock().await;
    let is_saved = saved.is_some();
    let source = if let Some(saved) = saved {
        ensure!(
            saved.connection.app_id == run.act_id,
            "Apify 运行与当前连接不匹配"
        );
        ensure!(
            !service
                .online_import
                .active
                .lock()
                .unwrap()
                .contains_key(&saved.id),
            "请先停止此连接的同步任务"
        );
        saved_source(service, &saved.id)?
    } else {
        let proposed = apify_source(&actor, directory)?;
        service.store.connection().execute(
            "INSERT INTO online_sources(id,config) VALUES(?1,?2) ON CONFLICT(id) DO NOTHING",
            params![proposed.id, serde_json::to_string(&proposed)?],
        )?;
        super::source(&service.store, &proposed.id)?.0
    };
    let counts = write_page(service, &source, &page.rows, offset, &mapping)?;
    if is_saved && !preset && page.next.is_none() {
        let mut credentials = saved_credentials(service, &source.id)?;
        credentials.mapping = Some(mapping);
        let secret = serde_json::to_string(&credentials)?;
        models::save_online_secret(&service.data_dir, &source.id, Some(&secret))?;
    }
    Ok(json!({
        "directory": source.directory, "imported": counts.imported, "updated": counts.updated,
        "skipped": counts.skipped, "conflicts": counts.conflicts, "empty": counts.empty, "next": page.next,
    }))
}

fn write_page(
    service: &Service,
    source: &Source,
    rows: &[Value],
    offset: usize,
    mapping: &ApifyMapping,
) -> Result<Counts> {
    let directory = Path::new(&source.directory);
    let documents = documents_dir();
    let mut indexed_root = None;
    let mut counts = Counts::default();
    for (index, row) in rows.iter().enumerate() {
        let Some((item, content)) = make_document(row, mapping, offset + index + 1) else {
            counts.empty += 1;
            continue;
        };
        let previous = tracked_item(&service.store, source, &item)?.map(|(_, files)| files);
        match install_content(&service.store, source, directory, &item, previous, content)? {
            Outcome::Conflict(_) => counts.conflicts += 1,
            Outcome::Skipped => counts.skipped += 1,
            Outcome::Written {
                path,
                updated: existed,
            } => {
                if existed {
                    counts.updated += 1;
                } else {
                    counts.imported += 1;
                }
                if indexed_root.is_none() {
                    indexed_root =
                        ensure_index_root(service, source, directory, documents.as_deref(), true)?;
                }
                let root = indexed_root.as_ref().context("无法添加 Apify 导入目录")?;
                service.scan.send(indexer::IndexRequest::Paths {
                    root: root.clone(),
                    paths: vec![path.canonicalize()?],
                })?;
            }
        }
    }
    Ok(counts)
}

#[derive(Deserialize, Serialize)]
struct Credentials {
    token: String,
    input: Value,
    #[serde(default)]
    mapping: Option<ApifyMapping>,
}

fn saved_source(service: &Service, id: &str) -> Result<Source> {
    let (source, _) = super::source(&service.store, id)?;
    ensure!(
        source.connection.platform == Platform::Apify && !source.connection.address.is_empty(),
        "连接不存在"
    );
    Ok(source)
}

fn saved_credentials(service: &Service, id: &str) -> Result<Credentials> {
    let secret = models::online_secret(&service.data_dir, id)?.context("请填写连接凭据")?;
    Ok(serde_json::from_str(&secret)?)
}

pub(super) async fn sync(
    service: &Service,
    source: &Source,
    cancel: &CancellationToken,
    report: &mut Report,
) -> Result<()> {
    let credentials = saved_credentials(service, &source.id)?;
    let apify = Apify {
        http: remote::client()?,
        origin: ORIGIN,
        token: &credentials.token,
    };
    sync_run(service, source, cancel, report, &apify, &credentials).await
}

async fn sync_run(
    service: &Service,
    source: &Source,
    cancel: &CancellationToken,
    report: &mut Report,
    apify: &Apify<'_>,
    credentials: &Credentials,
) -> Result<()> {
    let actor = actor_id(&source.connection.app_id)?;
    let mapping = if actor_id(&source.connection.address)? == actor_id(CRAWLER)? {
        let info = tokio::select! { biased;
            _ = cancel.cancelled() => return Ok(()),
            result = apify.actor(&actor) => result?,
        };
        preset_mapping(&info)?
    } else {
        credentials
            .mapping
            .clone()
            .context("先选择字段并完成一次导入，再设置定时同步。")?
    };
    if cancel.is_cancelled() {
        return Ok(());
    }
    let id = apify.start(&actor, &credentials.input).await?;
    let run = loop {
        let status = tokio::select! { biased;
            _ = cancel.cancelled() => return apify.abort(&id).await,
            result = apify.run(&id) => result?,
        };
        ensure!(status.act_id == actor, "Apify 运行与当前连接不匹配");
        match status.status.as_str() {
            "SUCCEEDED" => break status,
            "READY" | "RUNNING" | "TIMING-OUT" | "ABORTING" => (),
            "ABORTED" => {
                cancel.cancel();
                return Ok(());
            }
            status => bail!("Apify 运行结束（{status}）"),
        }
        tokio::select! { biased;
            _ = cancel.cancelled() => return apify.abort(&id).await,
            _ = tokio::time::sleep(Duration::from_secs(2)) => (),
        }
    };
    let mut offset = 0;
    loop {
        let page = tokio::select! { biased;
            _ = cancel.cancelled() => return Ok(()),
            result = apify.rows(&run, offset, IMPORT_PAGE) => result?,
        };
        let _lock = service.online_import.apify_import.lock().await;
        if cancel.is_cancelled() {
            return Ok(());
        }
        let counts = write_page(service, source, &page.rows, offset, &mapping)?;
        report.imported += counts.imported;
        report.updated += counts.updated;
        report.skipped += counts.skipped + counts.empty;
        report.conflicts += counts.conflicts;
        match page.next {
            Some(next) => offset = next,
            None => return Ok(()),
        }
    }
}

pub(super) async fn handle(
    service: &Service,
    token: &str,
    connection: Option<&str>,
    action: ApifyAction,
) -> Result<Value> {
    let token = token.trim();
    match action {
        ApifyAction::Store { search, offset } => {
            let apify = Apify {
                http: remote::client()?,
                origin: ORIGIN,
                token: "",
            };
            return apify.store(&search, offset).await;
        }
        ApifyAction::GetConnection => {
            let id = connection.context("连接不存在")?;
            saved_source(service, id)?;
            let credentials = saved_credentials(service, id)?;
            return Ok(
                json!({"input": credentials.input, "has_mapping": credentials.mapping.is_some()}),
            );
        }
        ApifyAction::SaveConnection {
            name,
            actor,
            reference,
            input,
            directory,
            schedule,
            daily_time,
            paused,
        } => {
            let _access = service
                .online_import
                .access
                .try_read()
                .context("正在清除本地数据")?;
            let _lock = service.online_import.apify_import.lock().await;
            let active = service.online_import.active.lock().unwrap();
            ensure!(
                connection.is_none_or(|id| !active.contains_key(id)),
                "请先停止此连接的同步任务"
            );
            let name = name.trim();
            ensure!(
                !name.is_empty() && name.chars().count() <= 100,
                "请输入不超过 100 字的连接名称"
            );
            ensure!(identifier(&actor), "请输入 Apify Store 链接或 Actor ID");
            actor_id(&reference)?;
            ensure!(input.is_object(), "Actor 参数必须是 JSON 对象");
            let mut source = if let Some(id) = connection {
                let source = saved_source(service, id)?;
                ensure!(
                    source.connection.app_id == actor,
                    "已保存的连接不能更换 Actor"
                );
                source
            } else {
                let id = uuid::Uuid::new_v4().to_string();
                let destination = destination_parent(&directory)?.join(format!(
                    "{}-{}",
                    name_part(name),
                    &id[..8]
                ));
                Source {
                    id,
                    name: String::new(),
                    connection: Connection {
                        platform: Platform::Apify,
                        drive: false,
                        user_access: false,
                        app_id: actor,
                        address: reference,
                    },
                    resources: vec![],
                    directory: destination.to_string_lossy().into_owned(),
                    schedule: Schedule::Manual,
                    daily_time: String::new(),
                    paused: false,
                    api: None,
                }
            };
            source.name = name.to_owned();
            source.schedule = schedule;
            source.daily_time = daily_time;
            source.paused = paused;
            if schedule == Schedule::Daily {
                chrono::NaiveTime::parse_from_str(&source.daily_time, "%H:%M")
                    .context("请选择每日同步时间")?;
            }
            let previous: Option<Credentials> = match connection {
                Some(id) => models::online_secret(&service.data_dir, id)?
                    .map(|secret| serde_json::from_str(&secret))
                    .transpose()?,
                None => None,
            };
            let token = if token.is_empty() {
                previous
                    .as_ref()
                    .map(|record| record.token.clone())
                    .context("请输入 Apify API Token")?
            } else {
                token.to_owned()
            };
            let mapping = previous.and_then(|record| record.mapping);
            if schedule != Schedule::Manual {
                ensure!(
                    actor_id(&source.connection.address)? == actor_id(CRAWLER)?
                        || mapping.is_some(),
                    "先选择字段并完成一次导入，再设置定时同步。"
                );
            }
            let next = next_run(&source, now())?;
            let credentials = serde_json::to_string(&Credentials {
                token,
                input,
                mapping,
            })?;
            models::save_online_secret(&service.data_dir, &source.id, Some(&credentials))?;
            service.store.connection().execute(
                "INSERT INTO online_sources(id,config,next_at) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET config=excluded.config,next_at=excluded.next_at",
                params![source.id, serde_json::to_string(&source)?,next],
            )?;
            return Ok(json!(source));
        }
        ApifyAction::RemoveConnection => {
            let id = connection.context("连接不存在")?;
            let _access = service
                .online_import
                .access
                .try_read()
                .context("正在清除本地数据")?;
            let _lock = service.online_import.apify_import.lock().await;
            let active = service.online_import.active.lock().unwrap();
            ensure!(!active.contains_key(id), "请先停止此连接的同步任务");
            saved_source(service, id)?;
            models::save_online_secret(&service.data_dir, id, None)?;
            service
                .store
                .connection()
                .execute("DELETE FROM online_sources WHERE id=?1", [id])?;
            return Ok(Value::Null);
        }
        _ => {}
    }
    let saved = connection.map(|id| saved_source(service, id)).transpose()?;
    let stored = if token.is_empty() {
        connection
            .map(|id| saved_credentials(service, id))
            .transpose()?
    } else {
        None
    };
    let token = stored
        .as_ref()
        .map_or(token, |record| record.token.as_str());
    ensure!(
        matches!(action, ApifyAction::Describe { .. }) || !token.is_empty(),
        "请输入 Apify API Token"
    );
    let apify = Apify {
        http: remote::client()?,
        origin: ORIGIN,
        token,
    };
    execute(service, &apify, action, saved).await
}

async fn execute(
    service: &Service,
    apify: &Apify<'_>,
    action: ApifyAction,
    saved: Option<Source>,
) -> Result<Value> {
    match action {
        ApifyAction::Describe { actor } => apify.describe(&actor).await,
        ApifyAction::Start { actor, input } => {
            let id = actor_id(&actor)?;
            ensure!(
                saved
                    .as_ref()
                    .is_none_or(|source| source.connection.app_id == id),
                "已保存的连接不能更换 Actor"
            );
            ensure!(input.is_object(), "Actor 参数必须是 JSON 对象");
            ensure!(
                saved.is_none_or(|source| source.schedule == Schedule::Manual),
                "请使用 Apify 导入入口"
            );
            Ok(json!({ "id": apify.start(&id, &input).await? }))
        }
        ApifyAction::Status { run } => {
            let result = apify.run(&run).await?;
            Ok(json!({ "id": result.id, "status": result.status }))
        }
        ApifyAction::Cancel { run } => {
            apify.abort(&run).await?;
            Ok(Value::Null)
        }
        ApifyAction::Preview { run } => {
            let run = apify.run(&run).await?;
            let page = apify.rows(&run, 0, 5).await?;
            let mut available = vec![];
            for row in &page.rows {
                fields(row, "", "", 5, &mut available);
            }
            let samples: Vec<_> = page
                .rows
                .iter()
                .map(|row| {
                    let text =
                        serde_json::to_string_pretty(row).expect("JSON Value is serializable");
                    if text.chars().count() > 5000 {
                        format!("{}…", text.chars().take(5000).collect::<String>())
                    } else {
                        text
                    }
                })
                .collect();
            Ok(json!({ "fields": available, "samples": samples, "total": page.total }))
        }
        ApifyAction::Import {
            run,
            directory,
            offset,
            preset,
            mapping,
        } => {
            import(
                service, apify, &run, &directory, offset, preset, mapping, saved,
            )
            .await
        }
        ApifyAction::Store { .. }
        | ApifyAction::GetConnection
        | ApifyAction::SaveConnection { .. }
        | ApifyAction::RemoveConnection => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[test]
    fn accepts_store_links_and_both_forms_of_actor_id() {
        for input in [
            "https://apify.com/apify/website-content-crawler/input-schema",
            CRAWLER,
            "apify~website-content-crawler",
        ] {
            assert_eq!(actor_id(input).unwrap(), "apify~website-content-crawler");
        }
        assert_eq!(actor_id("aYG0l9s7dbB7j3gbS").unwrap(), "aYG0l9s7dbB7j3gbS");
        assert!(actor_id("https://example.org/apify/website-content-crawler").is_err());
    }

    #[tokio::test]
    async fn saved_connections_reopen_without_returning_the_token_and_keep_imported_files() {
        let temp = tempfile::tempdir().unwrap();
        let service = Service::open(temp.path().join("data")).unwrap();
        let input = json!({"startUrls":[{"url":"https://example.org/guide"}]});
        let save = |name: &str| ApifyAction::SaveConnection {
            name: name.to_owned(),
            actor: "actor1".into(),
            reference: CRAWLER.into(),
            input: input.clone(),
            directory: temp.path().to_string_lossy().into_owned(),
            schedule: Schedule::Manual,
            daily_time: String::new(),
            paused: false,
        };
        let saved = handle(&service, "fixture-token", None, save("Docs"))
            .await
            .unwrap();
        let id = saved["id"].as_str().unwrap();
        let directory = Path::new(saved["directory"].as_str().unwrap());
        assert!(!saved.to_string().contains("fixture-token"));
        let reopened = handle(&service, "", Some(id), ApifyAction::GetConnection)
            .await
            .unwrap();
        assert_eq!(reopened, json!({"input": input, "has_mapping": false}));
        let updated = handle(&service, "", Some(id), save("Guides"))
            .await
            .unwrap();
        assert_eq!(updated["name"], "Guides");
        assert_eq!(updated["directory"], saved["directory"]);
        assert_eq!(
            saved_credentials(&service, id).unwrap().token,
            "fixture-token"
        );
        fs::create_dir(directory).unwrap();
        let file = directory.join("guide.md");
        fs::write(&file, "# Guide").unwrap();
        handle(&service, "", Some(id), ApifyAction::RemoveConnection)
            .await
            .unwrap();
        assert!(saved_source(&service, id).is_err());
        assert!(models::online_secret(&service.data_dir, id)
            .unwrap()
            .is_none());
        assert_eq!(fs::read_to_string(file).unwrap(), "# Guide");
        service.finish().await.unwrap();
    }

    #[tokio::test]
    async fn apify_sync_plan_uses_shared_scheduler_and_can_be_paused() {
        let temp = tempfile::tempdir().unwrap();
        let service = Service::open(temp.path().join("data")).unwrap();
        let save = |reference: &str, schedule, paused| ApifyAction::SaveConnection {
            name: "Docs".into(),
            actor: "actor1".into(),
            reference: reference.into(),
            input: json!({}),
            directory: temp.path().to_string_lossy().into_owned(),
            schedule,
            daily_time: "09:00".into(),
            paused,
        };
        let manual = handle(
            &service,
            "fixture-token",
            None,
            save(CRAWLER, Schedule::Manual, false),
        )
        .await
        .unwrap();
        let id = manual["id"].as_str().unwrap();
        assert!(super::super::source(&service.store, id)
            .unwrap()
            .1
            .is_none());
        let before = now();
        handle(
            &service,
            "",
            Some(id),
            save(CRAWLER, Schedule::EverySixHours, false),
        )
        .await
        .unwrap();
        let next = super::super::source(&service.store, id).unwrap().1.unwrap();
        assert!((before + 21600..=now() + 21600).contains(&next));
        assert!(!super::super::start(&service, id, false, true).unwrap());
        handle(
            &service,
            "",
            Some(id),
            save(CRAWLER, Schedule::EverySixHours, true),
        )
        .await
        .unwrap();
        assert!(super::super::source(&service.store, id)
            .unwrap()
            .1
            .is_none());
        assert!(!super::super::start(&service, id, false, true).unwrap());
        assert!(handle(
            &service,
            "fixture-token",
            None,
            save("example/records", Schedule::Daily, false)
        )
        .await
        .is_err());
        service.finish().await.unwrap();
    }

    #[tokio::test]
    async fn loads_actor_and_imports_preset_or_mapped_results_from_one_run() {
        let temp = tempfile::tempdir().unwrap();
        let service = Service::open(temp.path().join("data")).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let actor = json!({"data":{"id":"actor1","username":"apify","name":"website-content-crawler","title":"Official crawler"}});
        let build = json!({"data":{"actorDefinition":{"input":{"properties":{"startUrls":{"type":"array"}}}}}});
        let run = json!({"data":{"id":"run1","actId":"actor1","status":"SUCCEEDED","defaultDatasetId":"dataset1","defaultKeyValueStoreId":"store1"}});
        let rows = json!([{
            "url":"https://example.org/guide","metadata":{"title":"Guide"},"markdown":"# Guide\n\nRead this.",
            "attributes":[1,2],"place/id":"p22"
        }]);
        let routes = [
            (
                "GET /store?limit=20&offset=0&sortBy=popularity ",
                "",
                json!({"data":{"total":2,"items":[{"id":"actor1","username":"apify","name":"website-content-crawler","title":"Official crawler"}]}}),
            ),
            (
                "GET /store?limit=20&offset=20&sortBy=relevance&search=example+search ",
                "",
                json!({"data":{"total":21,"items":[{"id":"actor2","username":"example","name":"records","title":"Example Records","description":"Public records"}]}}),
            ),
            (
                "GET /actors/apify~website-content-crawler ",
                "",
                actor.clone(),
            ),
            ("GET /actors/actor1/builds/default ", "", build),
            ("POST /actors/actor1/runs ", "", run.clone()),
            ("GET /actor-runs/run1 ", "", run.clone()),
            ("GET /actor-runs/run1 ", "", run.clone()),
            (
                "GET /datasets/dataset1/items?offset=0&limit=5 ",
                "X-Apify-Pagination-Total: 1\r\n",
                rows.clone(),
            ),
            ("GET /actor-runs/run1 ", "", run.clone()),
            ("GET /actors/actor1 ", "", actor.clone()),
            (
                "GET /datasets/dataset1/items?offset=0&limit=20 ",
                "X-Apify-Pagination-Total: 1\r\n",
                rows.clone(),
            ),
            ("GET /actor-runs/run1 ", "", run.clone()),
            ("GET /actors/actor1 ", "", actor),
            (
                "GET /datasets/dataset1/items?offset=0&limit=20 ",
                "X-Apify-Pagination-Total: 1\r\n",
                rows.clone(),
            ),
            ("POST /actors/actor1/runs ", "", run.clone()),
            ("GET /actor-runs/run1 ", "", run),
            (
                "GET /datasets/dataset1/items?offset=0&limit=20 ",
                "X-Apify-Pagination-Total: 1\r\n",
                rows,
            ),
        ];
        let handler = tokio::spawn(async move {
            for (expected, headers, body) in routes {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut chunk = [0; 4096];
                while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&chunk[..count]);
                }
                let request = String::from_utf8(request).unwrap();
                assert!(request.starts_with(expected), "{request}");
                assert_eq!(
                    request
                        .to_ascii_lowercase()
                        .contains("authorization: bearer fixture-token"),
                    !expected.starts_with("GET /store?")
                );
                let body = body.to_string();
                let response = format!("HTTP/1.1 200 OK\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let apify = Apify {
            http: Client::builder()
                .no_proxy()
                .timeout(std::time::Duration::from_secs(4))
                .build()
                .unwrap(),
            origin: &origin,
            token: "fixture-token",
        };
        let invoke = |action| execute(&service, &apify, action, None);
        let anonymous = Apify {
            http: apify.http.clone(),
            origin: &origin,
            token: "",
        };
        let popular = anonymous.store("", 0).await.unwrap();
        assert_eq!(popular["items"][0]["id"], "actor1");
        assert_eq!(popular["next"], 1);
        let matching = anonymous.store(" example search ", 20).await.unwrap();
        assert_eq!(matching["items"][0]["title"], "Example Records");
        assert!(matching["next"].is_null());
        let description = invoke(ApifyAction::Describe {
            actor: CRAWLER.into(),
        })
        .await
        .unwrap();
        assert_eq!(description["preset"], true);
        assert_eq!(
            description["schema"]["properties"]["startUrls"]["type"],
            "array"
        );
        let started = invoke(ApifyAction::Start {
            actor: "actor1".into(),
            input: json!({"startUrls":[{"url":"https://example.org"}]}),
        })
        .await
        .unwrap();
        assert_eq!(started["id"], "run1");
        let status = invoke(ApifyAction::Status { run: "run1".into() })
            .await
            .unwrap();
        assert_eq!(status["status"], "SUCCEEDED");
        let preview = invoke(ApifyAction::Preview { run: "run1".into() })
            .await
            .unwrap();
        assert!(preview["fields"]
            .as_array()
            .unwrap()
            .contains(&json!(["/place~1id", "place/id"])));
        for preset in [true, false] {
            let target = temp.path().join(if preset { "preset" } else { "mapped" });
            fs::create_dir(&target).unwrap();
            let saved = if preset {
                None
            } else {
                let connection = handle(
                    &service,
                    "fixture-token",
                    None,
                    ApifyAction::SaveConnection {
                        name: "Records".into(),
                        actor: "actor1".into(),
                        reference: "example/records".into(),
                        input: json!({"query":"guide"}),
                        directory: target.to_string_lossy().into_owned(),
                        schedule: Schedule::Manual,
                        daily_time: "09:00".into(),
                        paused: false,
                    },
                )
                .await
                .unwrap();
                Some(serde_json::from_value::<Source>(connection).unwrap())
            };
            let result = execute(
                &service,
                &apify,
                ApifyAction::Import {
                    run: "run1".into(),
                    directory: target.to_string_lossy().into_owned(),
                    offset: 0,
                    preset,
                    mapping: ApifyMapping {
                        title: Some("/metadata/title".into()),
                        url: Some("/url".into()),
                        id: Some("/place~1id".into()),
                        content: vec!["/attributes".into(), "/place~1id".into()],
                    },
                },
                saved.clone(),
            )
            .await
            .unwrap();
            assert_eq!(result["imported"], 1);
            assert!(result["next"].is_null());
            let path = fs::read_dir(result["directory"].as_str().unwrap())
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            let body = fs::read_to_string(path).unwrap();
            assert!(body.contains("<https://example.org/guide>"));
            if preset {
                assert_eq!(
                    body,
                    "# Guide\n\nRead this.\n\n<https://example.org/guide>\n"
                );
            } else {
                assert!(body.contains("## attributes\n\n~~~json"));
                assert!(body.contains("## place/id\n\np22"));
                assert!(!body.contains("Read this."));
                let saved = saved.unwrap();
                let credentials = saved_credentials(&service, &saved.id).unwrap();
                assert_eq!(
                    credentials.mapping.as_ref().unwrap().content,
                    ["/attributes", "/place~1id"]
                );
                handle(
                    &service,
                    "",
                    Some(&saved.id),
                    ApifyAction::SaveConnection {
                        name: saved.name.clone(),
                        actor: saved.connection.app_id.clone(),
                        reference: saved.connection.address.clone(),
                        input: credentials.input.clone(),
                        directory: String::new(),
                        schedule: Schedule::Hourly,
                        daily_time: "09:00".into(),
                        paused: false,
                    },
                )
                .await
                .unwrap();
                let scheduled = saved_source(&service, &saved.id).unwrap();
                let mut report = Report::default();
                sync_run(
                    &service,
                    &scheduled,
                    &CancellationToken::new(),
                    &mut report,
                    &apify,
                    &credentials,
                )
                .await
                .unwrap();
                assert_eq!((report.imported, report.updated, report.skipped), (0, 0, 1));
            }
        }
        handler.await.unwrap();
        service.finish().await.unwrap();
    }
}
