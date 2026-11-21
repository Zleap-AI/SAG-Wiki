use anyhow::{bail, ensure, Context, Result};
use futures_util::StreamExt;
use reqwest::{header::HeaderMap, Client, Method, Response, StatusCode, Url};
use sag_protocol::online_import::{Connection, Platform};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    net::IpAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

mod github;
mod gitlab;
mod dingtalk;
mod ima;
pub(super) use github::{github_address, github_resources};
pub(super) use gitlab::gitlab_address;

const JSON_LIMIT: usize = 12 * 1024 * 1024;
const FILE_LIMIT: usize = 100 * 1024 * 1024;

#[derive(Clone, Serialize)]
pub(super) struct Resource {
    pub id: String,
    pub title: String,
    pub expandable: bool,
}
#[derive(Serialize)]
pub(super) struct Page {
    pub items: Vec<Resource>,
    pub cursor: Option<String>,
}

pub(super) struct Item {
    pub id: String,
    pub title: String,
    pub version: String,
    pub link: String,
    pub folders: Vec<String>,
    pub(super) kind: String,
    pub(super) inline: Option<String>,
}

pub(super) struct Content {
    pub bytes: Vec<u8>,
    pub extension: String,
    pub assets: Vec<(String, Vec<u8>)>,
}

pub(super) fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [first, second, third, _] = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_multicast()
                || first == 0
                || first >= 224
                || (first == 100 && (64..=127).contains(&second))
                || (first == 192 && second == 0 && third == 0)
                || (first == 198 && (18..=19).contains(&second)))
        }
        IpAddr::V6(ip) => ip.to_ipv4_mapped().map_or_else(
            || {
                ip.segments()[0] & 0xe000 == 0x2000
                    && !(ip.segments()[0] == 0x2001 && ip.segments()[1] == 0xdb8)
            },
            |ip| public_ip(ip.into()),
        ),
    }
}

struct PublicDns;
impl reqwest::dns::Resolve for PublicDns {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let addresses = tokio::net::lookup_host((name.as_str(), 0))
                .await?
                .collect::<Vec<_>>();
            if addresses.is_empty() || !addresses.iter().all(|address| public_ip(address.ip())) {
                return Err(std::io::Error::other("数据源地址必须可公开访问").into());
            }
            Ok(Box::new(addresses.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

fn public_url(address: &str) -> Result<Url> {
    let url = Url::parse(address).context("请输入有效的订阅地址")?;
    ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.username().is_empty()
            && url.password().is_none(),
        "数据源地址必须使用 HTTP 或 HTTPS，且不能包含凭据"
    );
    let host = url.host_str().context("数据源地址缺少主机名")?;
    if let Ok(ip) = host.trim_matches(['[', ']']).parse() {
        ensure!(public_ip(ip), "数据源地址必须可公开访问");
    }
    Ok(url)
}

pub(super) fn client() -> Result<Client> {
    Ok(Client::builder()
        .no_proxy()
        .dns_resolver(Arc::new(PublicDns))
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .user_agent("SAG Wiki data importer")
        .build()?)
}

#[derive(Clone, Copy, Debug)]
pub(super) enum AccountLimit {
    Rate { platform: Platform, retry_at: i64 },
    Quota(Platform),
}

impl fmt::Display for AccountLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Rate { platform: Platform::Github, .. } => "GitHub 请求已达限额，请稍后重试。",
            Self::Rate { platform: Platform::Lark, .. } => "Lark 暂时限制了请求频率，请稍后重试。",
            Self::Quota(Platform::Lark) => "Lark 本月的 API 调用额度已用完。",
            Self::Rate { .. } => "飞书暂时限制了请求频率，请稍后重试。",
            Self::Quota(_) => "飞书本月的 API 调用额度已用完。",
        })
    }
}

impl std::error::Error for AccountLimit {}

#[derive(Default)]
pub(super) struct AccountLimits(Mutex<HashMap<Platform, HashMap<String, i64>>>);

impl AccountLimits {
    pub(super) fn retry_at(&self, platform: Platform, app_id: &str) -> Option<i64> {
        let mut limits = self.0.lock().unwrap();
        let windows = limits.get_mut(&platform)?;
        let now = super::now();
        windows.retain(|_, until| *until > now);
        windows.get(app_id).copied()
    }

    pub(super) fn check(&self, platform: Platform, app_id: &str) -> Result<()> {
        if let Some(retry_at) = self.retry_at(platform, app_id) {
            return Err(AccountLimit::Rate { platform, retry_at }.into());
        }
        Ok(())
    }

    pub(super) fn check_response(
        &self,
        platform: Platform,
        app_id: &str,
        status: StatusCode,
        retry_after: i64,
        code: Option<i64>,
    ) -> Result<()> {
        if code == Some(99991403) {
            return Err(AccountLimit::Quota(platform).into());
        }
        if code == Some(99991400) || status == StatusCode::TOO_MANY_REQUESTS {
            let mut windows = self.0.lock().unwrap();
            let until = windows.entry(platform).or_default().entry(app_id.into()).or_default();
            *until = (*until).max(super::now().saturating_add(retry_after));
            return Err(AccountLimit::Rate { platform, retry_at: *until }.into());
        }
        Ok(())
    }
}

pub(super) fn account_retry_after(headers: &HeaderMap) -> i64 {
    ["x-ogw-ratelimit-reset", "retry-after"]
        .into_iter()
        .filter_map(|name| headers.get(name)?.to_str().ok()?.parse::<i64>().ok())
        .find(|seconds| *seconds > 0)
        .unwrap_or(60)
}

async fn request(
    builder: reqwest::RequestBuilder,
    account: Option<(&AccountLimits, Platform, &str)>,
) -> Result<Response> {
    for attempt in 0..=2 {
        if let Some((limits, platform, app_id)) = account {
            limits.check(platform, app_id)?;
        }
        let response = builder
            .try_clone()
            .context("无法重试数据源请求")?
            .send()
            .await;
        match response {
            Ok(response)
                if response.status().is_success() || response.status().is_redirection() =>
            {
                return Ok(response)
            }
            Ok(response)
                if matches!(response.status(), StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS)
                    && (response.url().host_str() == Some("api.github.com")
                        || response.headers().contains_key("x-github-request-id")) =>
            {
                let headers = response.headers();
                let exhausted = headers.get("x-ratelimit-remaining").is_some_and(|value| value == "0");
                let limit_header = |name: &str| headers.get(name)?.to_str().ok()?.parse::<i64>().ok();
                let reset = exhausted.then(|| limit_header("x-ratelimit-reset")).flatten();
                let retry_after = limit_header("retry-after").filter(|seconds| *seconds > 0);
                let throttled = response.status() == StatusCode::TOO_MANY_REQUESTS
                    || exhausted || retry_after.is_some()
                    || serde_json::from_slice::<Value>(&bounded_body(response, 64 * 1024).await?)
                        .ok().and_then(|body| body["message"].as_str().map(|message| message.to_ascii_lowercase()))
                        .is_some_and(|message| message.contains("rate limit"));
                if throttled {
                    let now = super::now();
                    let retry_at = reset.into_iter().chain(retry_after.map(|seconds| now.saturating_add(seconds)))
                        .filter(|until| *until > now).max().unwrap_or(now + 60);
                    return Err(AccountLimit::Rate { platform: Platform::Github, retry_at }.into());
                }
                bail!("授权失败或无权读取该资源");
            }
            Ok(response)
                if attempt < 2
                    && (response.status().is_server_error()
                        || (account.is_none() && response.status() == StatusCode::TOO_MANY_REQUESTS)) =>
            {
                let wait = response
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(1 << attempt);
                ensure!(wait <= 30, "请求受限，请稍后重试");
                tokio::time::sleep(Duration::from_secs(wait)).await;
            }
            Ok(response) if account.is_none() && response.status() == StatusCode::NOT_FOUND => {
                return response.error_for_status().context("数据源请求失败（HTTP 404）");
            }
            Ok(response) => {
                let status = response.status();
                if let Some((limits, platform, app_id)) = account.filter(|_| status.is_client_error()) {
                    let retry_after = account_retry_after(response.headers());
                    let code = bounded_body(response, 64 * 1024)
                        .await
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                        .and_then(|value| value["code"].as_i64());
                    limits.check_response(platform, app_id, status, retry_after, code)?;
                }
                if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
                    bail!("授权失败或无权读取该资源");
                }
                bail!("数据源请求失败（HTTP {}）", status.as_u16());
            }
            Err(error) if attempt < 2 && (error.is_timeout() || error.is_connect()) => {
                tokio::time::sleep(Duration::from_secs(1 << attempt)).await
            }
            Err(_) => bail!("无法连接数据源，请检查网络和地址"),
        }
    }
    unreachable!()
}

async fn bytes(response: Response, limit: usize) -> Result<Vec<u8>> {
    ensure!(response.status().is_success(), "平台返回了不受支持的重定向");
    bounded_body(response, limit).await
}

pub(super) async fn bounded_body(response: Response, limit: usize) -> Result<Vec<u8>> {
    ensure!(
        response.content_length().unwrap_or(0) <= limit as u64,
        "远端文件超过大小上限"
    );
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| anyhow::anyhow!("读取远端内容失败"))?;
        ensure!(body.len() + chunk.len() <= limit, "远端文件超过大小上限");
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

async fn download(client: &Client, mut url: Url, limit: usize) -> Result<Vec<u8>> {
    for _ in 0..=5 {
        url = public_url(url.as_str())?;
        let response = request(client.get(url.clone()), None).await?;
        if !response.status().is_redirection() {
            return bytes(response, limit).await;
        }
        let location = response
            .headers()
            .get("location")
            .context("下载跳转缺少地址")?
            .to_str()?;
        url = url.join(location).context("下载跳转地址无效")?;
    }
    bail!("下载跳转过多")
}

fn value<'a>(object: &'a Value, field: &str) -> Result<&'a str> {
    object[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .with_context(|| format!("数据源响应缺少 {field}"))
}

fn list<'a>(object: &'a Value, field: &str) -> Result<&'a [Value]> {
    Ok(object[field]
        .as_array()
        .with_context(|| format!("数据源响应缺少 {field}"))?)
}

fn id_part(id: &str) -> Result<&str> {
    ensure!(
        !id.is_empty()
            && id.len() <= 256
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
        "远端资源标识无效"
    );
    Ok(id)
}

fn ext(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn markdown(body: String) -> Content {
    Content {
        bytes: body.into_bytes(),
        extension: "md".into(),
        assets: vec![],
    }
}

fn normalized_markdown(text: &str) -> String {
    text.trim().replace('\r', "")
}

fn feed_markdown(html: &str, address: &str) -> String {
    let base = Url::parse(address).ok();
    fn render(element: scraper::ElementRef<'_>, base: Option<&Url>, depth: usize) -> String {
        let tag = element.value().name();
        if depth > 100 || matches!(tag, "script" | "style" | "template" | "iframe" | "object") {
            return String::new();
        }
        let mut text = String::new();
        for child in element.children() {
            if let Some(element) = scraper::ElementRef::wrap(child) {
                text.push_str(&render(element, base, depth + 1));
            } else if let Some(value) = child.value().as_text() {
                text.push_str(value);
            }
        }
        match tag {
            "a" => element
                .attr("href")
                .and_then(|href| Url::parse(href).ok().or_else(|| base?.join(href).ok()))
                .filter(|url| matches!(url.scheme(), "http" | "https"))
                .map(|url| format!("[{text}](<{url}>)"))
                .unwrap_or(text),
            "strong" | "b" => format!("**{text}**"),
            "em" | "i" => format!("*{text}*"),
            "li" => format!("- {}\n", text.trim()),
            "p" | "div" | "ul" | "ol" | "section" => format!("\n\n{}\n\n", text.trim()),
            "pre" => format!("\n\n````\n{}\n````\n\n", text.trim()),
            "br" => "\n".into(),
            "img" => element.attr("alt").unwrap_or("").into(),
            _ => text,
        }
    }
    render(
        scraper::Html::parse_fragment(html).root_element(),
        base.as_ref(),
        0,
    )
    .trim()
    .into()
}

fn title_notion(page: &Value) -> String {
    page["properties"]
        .as_object()
        .and_then(|props| props.values().find_map(|val| val["title"].as_array()))
        .map(|runs| {
            runs.iter()
                .filter_map(|run| run["plain_text"].as_str())
                .collect::<String>()
        })
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| "Notion".into())
}

pub(super) struct Remote {
    http: Client,
    connection: Connection,
    token: String,
    account_limits: Arc<AccountLimits>,
    github_files: tokio::sync::Mutex<HashMap<Option<String>, Arc<[github::File]>>>,
    #[cfg(test)]
    test_origin: Option<String>,
}

impl Remote {
    pub async fn open(
        connection: Connection,
        token: Option<String>,
        account_limits: &Arc<AccountLimits>,
    ) -> Result<Self> {
        let platform = connection.platform;
        ensure!(platform != Platform::Api, "请选择 API 请求");
        if platform == Platform::Rss {
            public_url(&connection.address)?;
        }
        let mut remote = Self {
            http: client()?,
            connection,
            token: token.unwrap_or_default(),
            account_limits: account_limits.clone(),
            github_files: Default::default(),
            #[cfg(test)]
            test_origin: None,
        };
        if platform.requires_secret() {
            ensure!(!remote.token.is_empty(), "请填写连接凭据");
        }
        if platform == Platform::Dingtalk {
            remote.token = remote.dingtalk_token().await?;
        }
        if matches!(platform, Platform::Feishu | Platform::Lark) && !remote.connection.user_access {
            ensure!(!remote.connection.app_id.is_empty(), "请填写 App ID");
            let url = format!(
                "{}/open-apis/auth/v3/tenant_access_token/internal",
                remote.origin()
            );
            let response = request(
                remote
                    .http
                    .post(url)
                    .json(&json!({"app_id":remote.connection.app_id,"app_secret":remote.token})),
                remote.account_request(),
            )
            .await?;
            let body = remote.json_body(response).await?;
            ensure!(body["code"].as_i64() == Some(0), "飞书或 Lark 应用授权失败");
            remote.token = value(&body, "tenant_access_token")?.into();
        }
        Ok(remote)
    }

    fn origin(&self) -> &str {
        #[cfg(test)]
        if let Some(origin) = &self.test_origin {
            return origin;
        }
        match self.connection.platform {
            Platform::Feishu => "https://open.feishu.cn",
            Platform::Lark => "https://open.larksuite.com",
            Platform::Notion => "https://api.notion.com",
            Platform::Yuque => "https://www.yuque.com",
            Platform::Github => "https://api.github.com",
            Platform::Gitlab => self.gitlab_origin(),
            Platform::Dingtalk => "https://api.dingtalk.com",
            Platform::Ima => "https://ima.qq.com",
            Platform::Rss | Platform::Apify | Platform::Api => unreachable!(),
        }
    }

    fn call(&self, method: Method, path: &str, query: &[(&str, &str)]) -> reqwest::RequestBuilder {
        let builder = self
            .http
            .request(method, format!("{}{path}", self.origin()))
            .query(query);
        match self.connection.platform {
            Platform::Notion => builder
                .bearer_auth(&self.token)
                .header("Notion-Version", "2026-03-11"),
            Platform::Yuque => builder.header("X-Auth-Token", &self.token),
            Platform::Github | Platform::Gitlab if self.token.is_empty() => builder,
            Platform::Gitlab => builder.header("PRIVATE-TOKEN", &self.token),
            Platform::Dingtalk => builder.header("x-acs-dingtalk-access-token", &self.token),
            Platform::Ima => builder
                .header("ima-openapi-clientid", &self.connection.app_id)
                .header("ima-openapi-apikey", &self.token),
            _ => builder.bearer_auth(&self.token),
        }
    }

    fn account_request(&self) -> Option<(&AccountLimits, Platform, &str)> {
        self.connection.platform.supports_account_login().then_some((
            &self.account_limits,
            self.connection.platform,
            self.connection.app_id.as_str(),
        ))
    }

    async fn json_body(&self, response: Response) -> Result<Value> {
        let status = response.status();
        let retry_after = account_retry_after(response.headers());
        let body: Value = serde_json::from_slice(&bytes(response, JSON_LIMIT).await?)?;
        if let Some((limits, platform, app_id)) = self.account_request() {
            limits.check_response(platform, app_id, status, retry_after, body["code"].as_i64())?;
        }
        Ok(body)
    }

    async fn json(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Value>,
    ) -> Result<Value> {
        let builder = self.call(method, path, query);
        let builder = match body {
            Some(body) => builder.json(&body),
            None => builder,
        };
        let response = request(builder, self.account_request()).await?;
        let value = self.json_body(response).await?;
        if matches!(self.connection.platform, Platform::Feishu | Platform::Lark) {
            if self.connection.user_access && matches!(value["code"].as_i64(), Some(99991663 | 99991668 | 99991677)) {
                return Err(super::account_auth::Reconnect(self.connection.platform).into());
            }
            ensure!(
                value["code"].as_i64() == Some(0),
                "飞书或 Lark 请求失败（{}）",
                value["code"].as_i64().context("平台响应格式无效")?
            );
        }
        Ok(value)
    }

    async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        self.json(Method::GET, path, query, None).await
    }

    async fn feishu_page(
        &self,
        path: &str,
        params: &[(&str, &str)],
        items: &str,
        cursor: Option<&str>,
    ) -> Result<(Vec<Value>, Option<String>)> {
        let mut query = params.to_vec();
        if let Some(cursor) = cursor {
            query.push(("page_token", cursor));
        }
        let response = self.get(path, &query).await?;
        let data = &response["data"];
        let next = if data["has_more"] == true {
            Some(value(data, "page_token")?.into())
        } else {
            None
        };
        let rows = match data.get(items) {
            None | Some(Value::Null) => vec![],
            Some(_) => list(data, items)?.to_vec(),
        };
        Ok((rows, next))
    }

    async fn feishu_all(
        &self,
        path: &str,
        query: &[(&str, &str)],
        field: &str,
    ) -> Result<Vec<Value>> {
        let mut cursor = None;
        let mut seen = HashSet::new();
        let mut all = vec![];
        loop {
            let (rows, next) = self
                .feishu_page(path, query, field, cursor.as_deref())
                .await?;
            all.extend(rows);
            let Some(next) = next else {
                return Ok(all);
            };
            ensure!(
                seen.len() < 1000 && seen.insert(next.clone()),
                "数据源分页未正常结束"
            );
            cursor = Some(next);
        }
    }

    async fn yuque_page(
        &self,
        path: &str,
        cursor: Option<&str>,
    ) -> Result<(Vec<Value>, Option<String>)> {
        let offset = cursor
            .unwrap_or("0")
            .parse::<usize>()
            .context("分页位置无效")?;
        let result = self
            .get(path, &[("limit", "100"), ("offset", &offset.to_string())])
            .await?;
        let rows = list(&result, "data")?.to_vec();
        let next = offset + rows.len();
        let more = rows.len() == 100
            && result["meta"]["total"]
                .as_u64()
                .is_none_or(|total| (next as u64) < total);
        Ok((rows, more.then(|| next.to_string())))
    }

    async fn yuque_all(&self, path: &str) -> Result<Vec<Value>> {
        let mut all = vec![];
        let mut cursor = None;
        let mut first_ids = HashSet::new();
        loop {
            let (rows, next) = self.yuque_page(path, cursor.as_deref()).await?;
            if let Some(first) = rows.first() {
                ensure!(
                    first_ids.len() < 1000 && first_ids.insert(first["id"].to_string()),
                    "语雀分页返回了重复内容"
                );
            }
            all.extend(rows);
            if next.is_none() {
                return Ok(all);
            }
            cursor = next;
        }
    }

    async fn feed(&self) -> Result<feed_rs::model::Feed> {
        let url = public_url(&self.connection.address)?;
        let raw = download(&self.http, url.clone(), JSON_LIMIT).await?;
        feed_rs::parser::Builder::new()
            .base_uri(Some(url.as_str()))
            .build()
            .parse(raw.as_slice())
            .context("无法读取 RSS 或 Atom 订阅")
    }

    pub async fn browse(&self, parent: Option<&str>, cursor: Option<&str>) -> Result<Page> {
        let mut items = vec![];
        let next = match self.connection.platform {
            Platform::Apify => bail!("请使用 Apify 导入入口"),
            Platform::Api => unreachable!(),
            Platform::Github => return self.github_browse(parent, cursor).await,
            Platform::Gitlab => return self.gitlab_browse(parent, cursor).await,
            Platform::Dingtalk => return self.dingtalk_browse(parent, cursor).await,
            Platform::Ima => return self.ima_browse(cursor).await,
            Platform::Rss => {
                let feed = self.feed().await?;
                items.push(Resource {
                    id: "feed".into(),
                    title: feed
                        .title
                        .map(|title| title.content)
                        .unwrap_or_else(|| "RSS / Atom".into()),
                    expandable: false,
                });
                None
            }
            Platform::Notion => {
                let mut filter =
                    json!({"filter":{"value":"page","property":"object"},"page_size":50});
                if let Some(cursor) = cursor {
                    filter["start_cursor"] = json!(cursor);
                }
                let result = self
                    .json(Method::POST, "/v1/search", &[], Some(filter))
                    .await?;
                for row in list(&result, "results")? {
                    if row["in_trash"] != true {
                        items.push(Resource {
                            id: value(row, "id")?.into(),
                            title: title_notion(row),
                            expandable: false,
                        });
                    }
                }
                result["next_cursor"].as_str().map(str::to_owned)
            }
            Platform::Yuque => {
                let user = self.get("/api/v2/user", &[]).await?;
                if let Some(group) = parent {
                    let login = id_part(group.strip_prefix("group:").context("语雀团队标识无效")?)?;
                    let (repos, next) = self
                        .yuque_page(&format!("/api/v2/groups/{login}/repos"), cursor)
                        .await?;
                    append_books(&mut items, repos)?;
                    next
                } else {
                    let login = id_part(value(&user["data"], "login")?)?;
                    if cursor.is_none() {
                        let user_id = user["data"]["id"].as_u64().context("语雀用户标识无效")?;
                        let groups = self
                            .get(&format!("/api/v2/users/{user_id}/groups"), &[])
                            .await?;
                        for group in list(&groups, "data")? {
                            items.push(Resource {
                                id: format!("group:{}", value(group, "login")?),
                                title: value(group, "name")?.into(),
                                expandable: true,
                            });
                        }
                    }
                    let (repos, next) = self
                        .yuque_page(&format!("/api/v2/users/{login}/repos"), cursor)
                        .await?;
                    append_books(&mut items, repos)?;
                    next
                }
            }
            _ if self.connection.drive => {
                let token = match parent {
                    Some(folder) => id_part(folder.strip_prefix("drive:").unwrap_or(folder))?,
                    None if self.connection.address.is_empty() && self.connection.user_access => "",
                    None => id_part(&self.connection.address)?,
                };
                let (rows, next) = self
                    .feishu_page(
                        "/open-apis/drive/v1/files",
                        &[("folder_token", token), ("page_size", "100")],
                        "files",
                        cursor,
                    )
                    .await?;
                if parent.is_none() && cursor.is_none() && !token.is_empty() {
                    items.push(Resource {
                        id: format!("drive:{token}"),
                        title: "当前云盘文件夹".into(),
                        expandable: false,
                    });
                }
                for row in rows {
                    if row["type"] == "folder" {
                        items.push(Resource {
                            id: format!("drive:{}", value(&row, "token")?),
                            title: value(&row, "name")?.into(),
                            expandable: true,
                        });
                    }
                }
                next
            }
            _ if parent.is_none() => {
                let (rows, next) = self
                    .feishu_page(
                        "/open-apis/wiki/v2/spaces",
                        &[("page_size", "50")],
                        "items",
                        cursor,
                    )
                    .await?;
                for row in rows {
                    items.push(Resource {
                        id: format!("wiki:{}", value(&row, "space_id")?),
                        title: value(&row, "name")?.into(),
                        expandable: true,
                    });
                }
                next
            }
            _ => {
                let (space, node) = wiki_parts(parent.context("缺少知识库节点")?)?;
                let (path, query) = wiki_query(space, node);
                let (rows, next) = self.feishu_page(&path, &query, "items", cursor).await?;
                for row in rows {
                    items.push(Resource {
                        id: format!("wiki:{space}:{}", value(&row, "node_token")?),
                        title: value(&row, "title")?.into(),
                        expandable: row["has_child"] == true,
                    });
                }
                next
            }
        };
        Ok(Page {
            items,
            cursor: next,
        })
    }

    pub async fn items(&self, resource: &str) -> Result<Vec<Item>> {
        match self.connection.platform {
            Platform::Apify => bail!("请使用 Apify 导入入口"),
            Platform::Api => unreachable!(),
            Platform::Github => self.github_items(resource).await,
            Platform::Gitlab => self.gitlab_items(resource).await,
            Platform::Dingtalk => self.dingtalk_items(resource).await,
            Platform::Ima => self.ima_items(resource).await,
            Platform::Notion => {
                let id = id_part(resource)?;
                let page = self.get(&format!("/v1/pages/{id}"), &[]).await?;
                ensure!(
                    page["in_trash"] != true && page["archived"] != true,
                    "Notion 页面已删除或归档"
                );
                Ok(vec![Item {
                    id: id.into(),
                    title: title_notion(&page),
                    version: value(&page, "last_edited_time")?.into(),
                    link: value(&page, "url")?.into(),
                    folders: vec![],
                    kind: "notion".into(),
                    inline: None,
                }])
            }
            Platform::Rss => {
                let feed = self.feed().await?;
                Ok(feed
                    .entries
                    .into_iter()
                    .filter_map(|entry| {
                        let title = entry
                            .title
                            .map(|text| text.content)
                            .unwrap_or_else(|| "RSS".into());
                        let link = entry
                            .links
                            .iter()
                            .find(|link| link.rel.as_deref().is_none_or(|rel| rel == "alternate"))
                            .map(|link| link.href.clone())
                            .unwrap_or_default();
                        let feed_address = if link.is_empty() {
                            &self.connection.address
                        } else {
                            &link
                        };
                        let body = entry
                            .content
                            .and_then(|content| content.body)
                            .filter(|text| !text.trim().is_empty())
                            .map(|text| feed_markdown(&text, feed_address))
                            .or_else(|| {
                                entry.summary.map(|text| {
                                    format!(
                                        "**Summary**\n\n{}",
                                        feed_markdown(&text.content, feed_address)
                                    )
                                })
                            })?;
                        let id = if entry.id.is_empty() {
                            if link.is_empty() {
                                super::digest(title.as_bytes())
                            } else {
                                link.clone()
                            }
                        } else {
                            entry.id
                        };
                        let version = super::digest(format!("{title}\n{link}\n{body}").as_bytes());
                        Some(Item {
                            id,
                            title,
                            version,
                            link,
                            folders: vec![],
                            kind: "rss".into(),
                            inline: Some(body),
                        })
                    })
                    .collect())
            }
            Platform::Yuque => {
                let book = id_part(
                    resource
                        .strip_prefix("book:")
                        .context("语雀知识库标识无效")?,
                )?;
                let rows = self
                    .yuque_all(&format!("/api/v2/repos/{book}/docs"))
                    .await?;
                let toc = self.get(&format!("/api/v2/repos/{book}/toc"), &[]).await?;
                let toc = list(&toc, "data")?;
                let hierarchy = yuque_hierarchy(toc);
                rows.into_iter()
                    .map(|row| -> Result<Item> {
                        let id = row["id"].as_u64().context("语雀文档标识无效")?;
                        let mut folders = vec![format!("{book}")];
                        if let Some(path) = hierarchy.get(&id) {
                            folders.extend(path.iter().cloned());
                        }
                        Ok(Item {
                            id: id.to_string(),
                            title: value(&row, "title")?.into(),
                            version: row["content_updated_at"].as_str().unwrap_or("").into(),
                            link: String::new(),
                            folders,
                            kind: "yuque".into(),
                            inline: None,
                        })
                    })
                    .collect()
            }
            _ => self.feishu_items(resource).await,
        }
    }

    async fn feishu_items(&self, resource: &str) -> Result<Vec<Item>> {
        let mut pending = vec![(resource.to_string(), vec![])];
        let mut seen = HashSet::new();
        let mut items = vec![];
        if !self.connection.drive {
            let (_, node) = wiki_parts(resource)?;
            if let Some(node) = node {
                let response = self
                    .get("/open-apis/wiki/v2/spaces/get_node", &[("token", node)])
                    .await?;
                let node = &response["data"]["node"];
                items.push(self.feishu_item(node, false)?);
                if node["has_child"] == false {
                    return Ok(items);
                }
            }
        }
        while let Some((parent, folders)) = pending.pop() {
            if !seen.insert(parent.clone()) {
                continue;
            }
            ensure!(
                seen.len() < 5000 && items.len() < 50_000,
                "所选目录太大，请缩小导入范围"
            );
            if self.connection.drive {
                let folder = id_part(
                    parent
                        .strip_prefix("drive:")
                        .context("云盘文件夹标识无效")?,
                )?;
                for row in self
                    .feishu_all(
                        "/open-apis/drive/v1/files",
                        &[("folder_token", folder), ("page_size", "100")],
                        "files",
                    )
                    .await?
                {
                    if row["type"] == "folder" {
                        let mut path = folders.clone();
                        path.push(value(&row, "name")?.into());
                        pending.push((format!("drive:{}", value(&row, "token")?), path));
                    } else {
                        let mut item = self.feishu_item(&row, true)?;
                        item.folders = folders.clone();
                        items.push(item);
                    }
                }
            } else {
                let (space, node) = wiki_parts(&parent)?;
                let (path, query) = wiki_query(space, node);
                for row in self.feishu_all(&path, &query, "items").await? {
                    if row["has_child"] == true {
                        let mut path = folders.clone();
                        path.push(value(&row, "title")?.into());
                        pending
                            .push((format!("wiki:{space}:{}", value(&row, "node_token")?), path));
                    }
                    let mut item = self.feishu_item(&row, false)?;
                    item.folders = folders.clone();
                    items.push(item);
                }
            }
        }
        Ok(items)
    }

    fn feishu_item(&self, row: &Value, drive: bool) -> Result<Item> {
        let region = if self.connection.platform == Platform::Feishu {
            "feishu.cn"
        } else {
            "larksuite.com"
        };
        Ok(Item {
            id: value(row, if drive { "token" } else { "obj_token" })?.into(),
            title: value(row, if drive { "name" } else { "title" })?.into(),
            version: row[if drive {
                "modified_time"
            } else {
                "obj_edit_time"
            }]
            .as_str()
            .unwrap_or("")
            .into(),
            link: if drive {
                row["url"].as_str().unwrap_or("").into()
            } else {
                format!("https://{region}/wiki/{}", value(row, "node_token")?)
            },
            folders: vec![],
            kind: value(row, if drive { "type" } else { "obj_type" })?.into(),
            inline: None,
        })
    }

    pub async fn fetch(&self, item: &Item) -> Result<Content> {
        let mut link = item.link.clone();
        let mut output = match item.kind.as_str() {
            "github" => return self.github_fetch(&item.version).await,
            "gitlab" => return self.gitlab_fetch(&item.version).await,
            "dingtalk" => self.dingtalk_fetch(&item.id).await?,
            "ima" => return self.ima_fetch(item).await,
            "rss" => markdown(item.inline.clone().context("订阅没有内容")?),
            "notion" => {
                let id = id_part(&item.id)?;
                let page = self
                    .get(
                        &format!("/v1/pages/{id}/markdown"),
                        &[("markdown_version", "v2")],
                    )
                    .await?;
                ensure!(
                    page["truncated"] != true
                        && !page["unknown_block_ids"]
                            .as_array()
                            .is_some_and(|ids| !ids.is_empty()),
                    "Notion 页面内容不完整，请检查权限或缩小范围"
                );
                let body = page["markdown"]
                    .as_str()
                    .context("Notion 未返回 Markdown")?;
                self.markdown_assets(body).await?
            }
            "yuque" => {
                let id = id_part(&item.id)?;
                let result = self.get(&format!("/api/v2/repos/docs/{id}"), &[]).await?;
                let doc = &result["data"];
                ensure!(
                    doc["format"] == "markdown" || doc["format"] == "lake",
                    "暂不支持此语雀文档格式"
                );
                let body = value(doc, "body")?;
                ensure!(
                    !body
                        .trim_start()
                        .to_ascii_lowercase()
                        .starts_with("<!doctype html")
                        && !body.trim_start().starts_with("<html"),
                    "语雀未返回文档正文"
                );
                if let (Some(book), Some(slug)) =
                    (doc["book"]["namespace"].as_str(), doc["slug"].as_str())
                {
                    link = format!("https://www.yuque.com/{book}/{slug}");
                }
                self.markdown_assets(body).await?
            }
            "docx" => self.docx(id_part(&item.id)?).await?,
            "doc" | "sheet" => {
                self.export(
                    id_part(&item.id)?,
                    &item.kind,
                    if item.kind == "doc" { "docx" } else { "xlsx" },
                )
                .await?
            }
            "file" => {
                let extension = ext(&item.title);
                ensure!(
                    matches!(
                        extension.as_str(),
                        "md" | "markdown" | "txt" | "html" | "htm"
                    ) || crate::anydoc::handles_extension(&extension),
                    "暂不支持此云盘文件格式"
                );
                let id = id_part(&item.id)?;
                let bytes = self
                    .binary(
                        &format!("/open-apis/drive/v1/files/{id}/download"),
                        FILE_LIMIT,
                    )
                    .await?;
                Content {
                    bytes,
                    extension,
                    assets: vec![],
                }
            }
            _ => bail!("暂不支持此在线文档类型"),
        };
        if output.extension == "md" && item.kind != "file" {
            let title = item.title.replace(['\r', '\n'], " ");
            let attribution = Url::parse(&link)
                .ok()
                .filter(|url| matches!(url.scheme(), "https" | "http"))
                .map(|url| format!("\n\n<{}>", url.as_str().replace(['<', '>'], "")))
                .unwrap_or_default();
            let body = normalized_markdown(std::str::from_utf8(&output.bytes)?);
            output.bytes = format!("# {title}\n\n{body}{attribution}\n").into_bytes();
        }
        Ok(output)
    }

    async fn binary(&self, path: &str, limit: usize) -> Result<Vec<u8>> {
        let response = request(self.call(Method::GET, path, &[]), self.account_request()).await?;
        if response.status().is_redirection() {
            let target = response
                .headers()
                .get("location")
                .context("下载跳转缺少地址")?
                .to_str()?;
            let url = Url::parse(&format!("{}{path}", self.origin()))?.join(target)?;
            ensure!(url.scheme() == "https", "附件下载必须使用 HTTPS");
            return download(&self.http, url, limit).await;
        }
        bytes(response, limit).await
    }

    async fn export(&self, id: &str, kind: &str, extension: &str) -> Result<Content> {
        let result = self
            .json(
                Method::POST,
                "/open-apis/drive/v1/export_tasks",
                &[],
                Some(json!({"token":id,"type":kind,"file_extension":extension})),
            )
            .await?;
        let ticket = id_part(value(&result["data"], "ticket")?)?;
        for _ in 0..30 {
            let response = self
                .get(
                    &format!("/open-apis/drive/v1/export_tasks/{ticket}"),
                    &[("token", id)],
                )
                .await?;
            let result = &response["data"]["result"];
            match result["job_status"].as_i64() {
                Some(0) => {
                    let file = id_part(value(result, "file_token")?)?;
                    let bytes = self
                        .binary(
                            &format!("/open-apis/drive/v1/export_tasks/file/{file}/download"),
                            FILE_LIMIT,
                        )
                        .await?;
                    ensure!(bytes.starts_with(b"PK"), "平台未返回有效的 Office 导出文件");
                    return Ok(Content {
                        bytes,
                        extension: extension.into(),
                        assets: vec![],
                    });
                }
                Some(1 | 2) => tokio::time::sleep(Duration::from_secs(1)).await,
                _ => bail!("平台文档导出失败"),
            }
        }
        bail!("平台文档导出超时")
    }

    async fn markdown_assets(&self, body: &str) -> Result<Content> {
        let patterns = regex::Regex::new(
            r#"!\[[^\]\n]*\]\(<?(https://[^>\s)]+)>?\)|<(?:image|file|pdf|audio|video)\b[^>]*?src="(https://[^"]+)"[^>]*>|\[[^\]\n]+\]\(<?(https://[^>\s)]+)>?\)"#,
        )?;
        let mut text = body.to_owned();
        let mut assets = vec![];
        let mut downloaded = HashSet::new();
        let mut total = 0;
        for matched in patterns.captures_iter(body) {
            let address = matched
                .get(1)
                .or_else(|| matched.get(2))
                .or_else(|| matched.get(3))
                .unwrap()
                .as_str();
            let uri = public_url(address)?;
            let extension = ext(uri.path());
            if !matches!(
                extension.as_str(),
                "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "webp"
                    | "pdf"
                    | "docx"
                    | "xlsx"
                    | "pptx"
                    | "zip"
                    | "mp3"
                    | "mp4"
            ) {
                continue;
            }
            if matched.get(3).is_some() {
                let host = uri.host_str().unwrap_or_default();
                if ![
                    "amazonaws.com",
                    "notion.so",
                    "notion-static.com",
                    "yuque.com",
                    "alicdn.com",
                    "alipayobjects.com",
                ]
                .iter()
                .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
                {
                    continue;
                }
            }
            if !downloaded.insert(address) {
                continue;
            }
            ensure!(
                assets.len() < 64 && total < FILE_LIMIT,
                "文档附件超过导入上限"
            );
            let file = download(&self.http, uri, 20 * 1024 * 1024).await?;
            total += file.len();
            ensure!(total <= FILE_LIMIT, "文档附件超过导入上限");
            let name = format!("{}.{}", super::digest(&file), extension);
            if let Some(tag) = matched.get(2).map(|_| matched.get(0).unwrap().as_str()) {
                let markup =
                    if matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp") {
                        "!"
                    } else {
                        ""
                    };
                text = text.replace(tag, &format!("{markup}[{name}](SAG_ASSETS/{name})"));
            }
            text = text.replace(address, &format!("SAG_ASSETS/{name}"));
            if !assets.iter().any(|(existing, _)| existing == &name) {
                assets.push((name, file));
            }
        }
        Ok(Content {
            bytes: text.into_bytes(),
            extension: "md".into(),
            assets,
        })
    }

    async fn docx(&self, id: &str) -> Result<Content> {
        let blocks = self
            .feishu_all(
                &format!("/open-apis/docx/v1/documents/{id}/blocks"),
                &[("page_size", "100")],
                "items",
            )
            .await?;
        let mut assets = vec![];
        let mut names = HashMap::new();
        let mut total = 0;
        for block in &blocks {
            let Some(token) = block["image"]["token"]
                .as_str()
                .or_else(|| block["file"]["token"].as_str())
            else {
                continue;
            };
            if names.contains_key(token) {
                continue;
            }
            let token = id_part(token)?;
            ensure!(
                assets.len() < 64 && total < FILE_LIMIT,
                "文档附件超过导入上限"
            );
            let file = self
                .binary(
                    &format!("/open-apis/drive/v1/medias/{token}/download"),
                    20 * 1024 * 1024,
                )
                .await?;
            total += file.len();
            ensure!(total <= FILE_LIMIT, "文档附件超过导入上限");
            let extension = if file.starts_with(b"\x89PNG") {
                "png".into()
            } else if file.starts_with(b"\xff\xd8\xff") {
                "jpg".into()
            } else if file.starts_with(b"GIF8") {
                "gif".into()
            } else if file.get(8..12) == Some(b"WEBP") {
                "webp".into()
            } else {
                let extension = ext(block["file"]["name"].as_str().unwrap_or(""));
                if extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
                    && !extension.is_empty()
                {
                    extension
                } else {
                    "bin".into()
                }
            };
            let name = format!("{}.{}", super::digest(&file), extension);
            names.insert(token.to_string(), name.clone());
            if !assets.iter().any(|(existing, _)| existing == &name) {
                assets.push((name, file));
            }
        }
        Ok(Content {
            bytes: render_docx(&blocks, &names)?.into_bytes(),
            extension: "md".into(),
            assets,
        })
    }
}

fn yuque_hierarchy(toc: &[Value]) -> HashMap<u64, Vec<String>> {
    let nodes = toc
        .iter()
        .filter_map(|node| Some((node["uuid"].as_str()?, node)))
        .collect::<HashMap<_, _>>();
    let mut paths = HashMap::new();
    for node in toc {
        let Some(id) = node["doc_id"].as_u64().filter(|id| *id > 0) else {
            continue;
        };
        let mut path = vec![];
        let mut parent = node["parent_uuid"].as_str();
        while let Some(group) = parent.and_then(|parent| nodes.get(parent)) {
            if path.len() >= 16 {
                break;
            }
            path.push(group["title"].as_str().unwrap_or("Folder").into());
            parent = group["parent_uuid"].as_str();
        }
        path.reverse();
        paths.insert(id, path);
    }
    paths
}

fn append_books(out: &mut Vec<Resource>, repos: Vec<Value>) -> Result<()> {
    for repo in repos {
        if repo["type"] == "Book" {
            out.push(Resource {
                id: format!(
                    "book:{}",
                    repo["id"].as_u64().context("语雀知识库标识无效")?
                ),
                title: value(&repo, "name")?.into(),
                expandable: false,
            });
        }
    }
    Ok(())
}

fn wiki_parts(resource: &str) -> Result<(&str, Option<&str>)> {
    let resource = resource.strip_prefix("wiki:").context("知识库标识无效")?;
    if let Some((space, node)) = resource.split_once(':') {
        Ok((id_part(space)?, Some(id_part(node)?)))
    } else {
        Ok((id_part(resource)?, None))
    }
}

fn wiki_query<'a>(space: &str, node: Option<&'a str>) -> (String, Vec<(&'static str, &'a str)>) {
    let mut query = vec![("page_size", "50")];
    if let Some(node) = node {
        query.push(("parent_node_token", node));
    }
    (format!("/open-apis/wiki/v2/spaces/{space}/nodes"), query)
}

fn rich(block: &Value) -> String {
    block["elements"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|element| {
            let run = &element["text_run"];
            let mut text = run["content"]
                .as_str()
                .unwrap_or_else(|| element["equation"]["content"].as_str().unwrap_or(""))
                .to_owned();
            let style = &run["text_element_style"];
            for (key, marker) in [
                ("inline_code", "`"),
                ("bold", "**"),
                ("italic", "*"),
                ("strikethrough", "~~"),
            ] {
                if style[key] == true && !text.is_empty() {
                    text = format!("{marker}{text}{marker}");
                }
            }
            if let Some(url) = style["link"]["url"]
                .as_str()
                .and_then(|url| Url::parse(url).ok())
                .filter(|url| matches!(url.scheme(), "http" | "https"))
            {
                text = format!("[{text}](<{url}>)");
            }
            text
        })
        .collect()
}

fn render_docx(blocks: &[Value], assets: &HashMap<String, String>) -> Result<String> {
    let blocks = blocks
        .iter()
        .map(|block| Ok((value(block, "block_id")?, block)))
        .collect::<Result<HashMap<_, _>>>()?;
    let page = blocks
        .values()
        .find(|block| block["block_type"] == 1)
        .context("飞书文档缺少根节点")?;
    let mut seen = HashSet::new();
    fn render<'a>(
        block: &'a Value,
        blocks: &HashMap<&'a str, &'a Value>,
        assets: &HashMap<String, String>,
        seen: &mut HashSet<&'a str>,
        depth: usize,
    ) -> Result<String> {
        ensure!(
            depth <= 100 && seen.insert(value(block, "block_id")?),
            "飞书文档块结构无效"
        );
        let children = || {
            block["children"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|child| child.as_str())
        };
        let kind = block["block_type"].as_u64().unwrap_or(0);
        if kind == 31 {
            let columns = block["table"]["property"]["column_size"]
                .as_u64()
                .unwrap_or(0) as usize;
            let mut cells = vec![];
            for child in children() {
                if let Some(cell) = blocks.get(child) {
                    cells.push(
                        render(cell, blocks, assets, seen, depth + 1)?
                            .trim()
                            .replace('|', "\\|")
                            .replace('\n', " "),
                    );
                }
            }
            if columns == 0 {
                return Ok(cells.join("\n\n"));
            }
            let mut table = String::new();
            for (index, row) in cells.chunks(columns).enumerate() {
                table.push_str(&format!("| {} |\n", row.join(" | ")));
                if index == 0 {
                    table.push_str(&format!("|{}\n", " --- |".repeat(columns)));
                }
            }
            return Ok(format!("{table}\n"));
        }
        let mut text = match kind {
            2 => rich(&block["text"]),
            3..=11 => format!(
                "{} {}",
                "#".repeat((kind as usize - 1).min(6)),
                rich(&block[format!("heading{}", kind - 2)])
            ),
            12 | 13 => format!(
                "{}{} {}",
                "  ".repeat(depth.saturating_sub(1)),
                if kind == 12 { "-" } else { "1." },
                rich(&block[if kind == 12 { "bullet" } else { "ordered" }])
            ),
            14 => format!("````\n{}\n````", rich(&block["code"])),
            15 => format!("> {}", rich(&block["quote"]).replace('\n', "\n> ")),
            17 => format!(
                "- [{}] {}",
                if block["todo"]["style"]["done"] == true {
                    "x"
                } else {
                    " "
                },
                rich(&block["todo"])
            ),
            22 => "---".into(),
            23 | 27 => {
                let kind_name = if kind == 27 { "image" } else { "file" };
                let token = value(&block[kind_name], "token")?;
                let name = assets.get(token).context("飞书文档附件未下载")?;
                format!(
                    "{}[{}](SAG_ASSETS/{name})",
                    if kind == 27 { "!" } else { "" },
                    block[kind_name]["name"].as_str().unwrap_or(kind_name)
                )
            }
            _ => String::new(),
        };
        if !text.is_empty() {
            text.push_str(if matches!(kind, 12 | 13 | 17) {
                "\n"
            } else {
                "\n\n"
            });
        }
        for child in children() {
            text.push_str(&render(
                blocks.get(child).context("飞书文档有未加载的子块")?,
                blocks,
                assets,
                seen,
                depth + 1,
            )?);
        }
        Ok(text)
    }
    render(page, &blocks, assets, &mut seen, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[tokio::test]
    async fn gitlab_keeps_selected_branch_and_subdirectory() {
        let branch = "release/docs";
        let (mut remote, handler) = fixture(
            Platform::Gitlab,
            false,
            vec![
                ("/projects/group%2Fsub%2Fproject/repository/branches?page=1", json!([{"name":branch}])),
                ("/repository/tree?ref=release%2Fdocs&path=", json!([
                    {"type":"tree","path":"manual","name":"manual"},
                    {"type":"blob","path":"package.json","name":"package.json"}
                ])),
                ("/repository/tree?ref=release%2Fdocs&path=manual&recursive=true", json!([
                    {"type":"blob","mode":"100644","id":"aabb","path":"manual/nested/README.md"},
                    {"type":"blob","mode":"120000","id":"ccdd","path":"manual/link.md"}
                ])),
                ("/repository/blobs/aabb/raw", json!("# README")),
            ],
        ).await;
        remote.connection.address = gitlab_address("https://gitlab.com/group/sub/project.git/").unwrap();
        let branches = remote.browse(None, None).await.unwrap();
        assert_eq!(branches.items[0].title, branch);
        let tree = remote.browse(Some(&branches.items[0].id), None).await.unwrap();
        assert_eq!(tree.items.len(), 1);
        assert_eq!(tree.items[0].title, "manual");
        let files = remote.items(&tree.items[0].id).await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].folders, ["release/docs", "manual", "nested"]);
        let content = remote.fetch(&files[0]).await.unwrap();
        assert!(String::from_utf8(content.bytes).unwrap().contains("# README"));
        handler.await.unwrap();
    }

    #[tokio::test]
    async fn dingtalk_lists_workspaces_and_converts_native_documents() {
        let (mut remote, handler) = fixture(
            Platform::Dingtalk,
            false,
            vec![
                ("/v2.0/wiki/workspaces?operatorId=operator", json!({"workspaces":[{"workspaceId":"space","rootNodeId":"root","name":"团队知识库"}]})),
                ("/v2.0/wiki/nodes?operatorId=operator&parentNodeId=root", json!({"nodes":[
                    {"type":"FILE","category":"ALIDOC","extension":"adoc","nodeId":"doc","name":"指南","modifiedTimestamp":42},
                    {"type":"FILE","category":"ALIDOC","extension":"asheet","nodeId":"sheet","name":"表格"}
                ]})),
                ("/v2.0/wiki/nodes?operatorId=operator&parentNodeId=root", json!({"nodes":[
                    {"type":"FILE","category":"ALIDOC","extension":"adoc","nodeId":"doc","name":"指南","modifiedTimestamp":42}
                ]})),
                ("/v1.0/doc/suites/documents/doc/blocks?operatorId=operator&startIndex=0&endIndex=99", json!({
                    "success":true,"result":{"data":[
                        {"blockType":"heading","heading":{"level":"h2","text":"操作步骤"}},
                        {"blockType":"unorderedList","unorderedList":{"text":"打开设置"}},
                        {"blockType":"unorderedList","unorderedList":{"text":"选择数据源"}}
                    ]}
                })),
            ],
        ).await;
        remote.connection.address = "operator".into();
        let spaces = remote.browse(None, None).await.unwrap();
        let docs = remote.browse(Some(&spaces.items[0].id), None).await.unwrap();
        assert_eq!(docs.items.len(), 1);
        let items = remote.items(&docs.items[0].id).await.unwrap();
        assert_eq!(items[0].version, "42");
        let body = String::from_utf8(remote.fetch(&items[0]).await.unwrap().bytes).unwrap();
        assert!(body.contains("## 操作步骤\n\n- 打开设置\n- 选择数据源"));
        handler.await.unwrap();
    }

    #[tokio::test]
    async fn ima_reads_notes_from_a_selected_knowledge_base() {
        let (mut remote, handler) = fixture(
            Platform::Ima,
            false,
            vec![
                ("/openapi/wiki/v1/get_addable_knowledge_base_list", json!({"code":0,"data":{"addable_knowledge_base_list":[{"id":"kb","name":"资料"}],"is_end":true}})),
                ("/openapi/wiki/v1/get_knowledge_list", json!({"code":0,"data":{"knowledge_list":[{"media_id":"folder_child","media_type":99,"title":"子目录"}],"is_end":true}})),
                ("/openapi/wiki/v1/get_knowledge_list", json!({"code":0,"data":{"knowledge_list":[{"media_id":"note","media_type":11,"title":"笔记"}],"is_end":true}})),
                ("/openapi/wiki/v1/get_media_info", json!({"retcode":0,"data":{"media_type":11,"notebook_ext_info":{"notebook_id":"notebook"}}})),
                ("/openapi/note/v1/get_doc_content", json!({"code":0,"data":{"content":"## 笔记正文"}})),
            ],
        ).await;
        remote.connection.app_id = "client".into();
        let library = remote.browse(None, None).await.unwrap();
        let items = remote.items(&library.items[0].id).await.unwrap();
        assert_eq!(items[0].folders, ["子目录"]);
        assert!(items[0].version.is_empty());
        let content = remote.fetch(&items[0]).await.unwrap();
        assert_eq!(content.bytes, "## 笔记正文".as_bytes());
        handler.await.unwrap();
    }

    #[tokio::test]
    async fn github_rate_limits_are_distinct_from_permission_errors() {
        let (mut remote, handler) = fixture_http(
            Platform::Github,
            false,
            vec![
                ("/branches?", "403 Forbidden", "X-GitHub-Request-Id: fixture\r\nX-RateLimit-Remaining: 0\r\nX-RateLimit-Reset: 4102444800\r\n", json!({"message":"API rate limit exceeded"})),
                ("/branches?", "403 Forbidden", "X-GitHub-Request-Id: fixture\r\n", json!({"message":"You have exceeded a secondary rate limit"})),
                ("/branches?", "429 Too Many Requests", "X-GitHub-Request-Id: fixture\r\nRetry-After: 10\r\n", json!({})),
                ("/branches?", "403 Forbidden", "X-GitHub-Request-Id: fixture\r\nX-RateLimit-Remaining: 58\r\n", json!({"message":"Resource not accessible by integration"})),
            ],
        ).await;
        remote.connection.address = "https://github.com/owner/repo".into();
        let limited = |error: anyhow::Error| {
            assert_eq!(error.to_string(), "GitHub 请求已达限额，请稍后重试。");
            match error.downcast::<AccountLimit>().unwrap() {
                AccountLimit::Rate { platform: Platform::Github, retry_at } => retry_at,
                other => panic!("unexpected limit: {other:?}"),
            }
        };
        let retry_at = limited(remote.browse(None, None).await.err().unwrap());
        assert_eq!(retry_at, 4102444800);
        let now = super::super::now();
        assert!(limited(remote.browse(None, None).await.err().unwrap()) >= now + 60);
        assert!(limited(remote.browse(None, None).await.err().unwrap()) >= now + 10);
        let denied = remote.browse(None, None).await.err().unwrap();
        assert_eq!(denied.to_string(), "授权失败或无权读取该资源");
        handler.await.unwrap();
    }

    #[tokio::test]
    async fn github_lists_and_selects_only_markdown_in_a_repository() {
        let address = github_address("https://github.com/Owner/Repo.git/#user-guide").unwrap();
        assert_eq!(address, "https://github.com/owner/repo");
        assert!(github_address("https://github.com/owner/repo/tree/main").is_err());
        let (mut remote, handler) = fixture_http(
            Platform::Github,
            false,
            vec![
                ("/repos/owner/repo/branches?page=1&per_page=100", "200 OK",
                    "Link: <https://api.github.com/repos/owner/repo/branches?page=2>; rel=\"next\"\r\n",
                    json!([{"name": "alpha"}, {"name": "main"}, {"name": "next"}])),
                ("/repos/owner/repo/branches?page=2&per_page=100", "200 OK", "", json!([{"name": "release/docs"}])),
                ("/repos/owner/repo/branches/release%2Fdocs ", "200 OK", "", json!({
                    "commit": {"commit": {"tree": {"sha": "abcdef"}}}
                })),
                ("/repos/owner/repo/git/trees/abcdef?recursive=1", "200 OK", "", json!({
                "truncated": false,
                "tree": [
                    {"path": "README.md", "sha": "aaa", "mode": "100644"},
                    {"path": "docs/guide.markdown", "sha": "bbb", "mode": "100644"},
                    {"path": "docs/nested/README", "sha": "ccc", "mode": "100644"},
                    {"path": "docs-extra/intro.md", "sha": "ddd", "mode": "100644"},
                    {"path": "docs/notes.txt", "sha": "eee", "mode": "100644"},
                    {"path": "assets/alias.md", "sha": "fff", "mode": "120000"}
                ]
                })),
                ("/repos/owner/repo/git/trees/HEAD?recursive=1", "200 OK", "", json!({
                    "truncated": false,
                    "tree": [{"path": "docs/guide.markdown", "sha": "default", "mode": "100644"}]
                })),
            ],
        ).await;
        remote.connection.address = address;
        let first = remote.browse(None, None).await.unwrap();
        assert_eq!(first.items.iter().map(|item| item.title.as_str()).collect::<Vec<_>>(), ["main", "alpha", "next"]);
        assert_eq!(first.cursor.as_deref(), Some("2"));
        let branches = remote.browse(None, first.cursor.as_deref()).await.unwrap();
        assert_eq!(branches.items[0].id, "github:tree:release/docs:");
        assert!(branches.cursor.is_none());
        let root = remote.browse(Some(&branches.items[0].id), None).await.unwrap();
        assert_eq!(
            root.items.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
            ["github:tree:release/docs:docs", "github:tree:release/docs:docs-extra", "github:blob:release/docs:README.md"]
        );
        let browse = remote.browse(Some(&root.items[0].id), None).await.unwrap();
        assert_eq!(browse.items[0].id, "github:tree:release/docs:docs/nested");
        assert_eq!(browse.items[1].id, "github:blob:release/docs:docs/guide.markdown");
        let selected = remote.items(&root.items[0].id).await.unwrap();
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].id, "release/docs\0docs/guide.markdown");
        assert_eq!(selected[0].version, "bbb");
        assert_eq!(selected[1].folders, ["release/docs", "docs", "nested"]);
        let single = remote.items("github:blob:release/docs:README.md").await.unwrap();
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].id, "release/docs\0README.md");
        for resource in ["github:root", "github:folder:docs", "github:file:docs/guide.markdown"] {
            let legacy = remote.items(resource).await.unwrap();
            assert_eq!(legacy[0].id, "docs/guide.markdown");
            assert_eq!(legacy[0].version, "default");
            assert_eq!(legacy[0].folders, ["docs"]);
        }
        handler.await.unwrap();
    }

    #[tokio::test]
    async fn github_pins_main_on_the_first_page_when_it_is_listed_later() {
        let (mut remote, handler) = fixture_http(
            Platform::Github,
            false,
            vec![
                ("/branches?page=1", "200 OK",
                    "Link: <https://api.github.com/repos/owner/repo/branches?page=2>; rel=\"next\"\r\n",
                    json!([{"name":"feature"}])),
                ("/branches/main ", "200 OK", "", json!({"name":"main"})),
                ("/branches?page=1", "200 OK",
                    "Link: <https://api.github.com/repos/owner/repo/branches?page=2>; rel=\"next\"\r\n",
                    json!([{"name":"feature"}])),
                ("/branches/main ", "404 Not Found", "", json!({"message":"Branch not found"})),
            ],
        ).await;
        remote.connection.address = "https://github.com/owner/repo".into();
        for expected in [vec!["main", "feature"], vec!["feature"]] {
            let page = remote.browse(None, None).await.unwrap();
            assert_eq!(page.items.iter().map(|item| item.title.as_str()).collect::<Vec<_>>(), expected);
            assert_eq!(page.cursor.as_deref(), Some("2"));
        }
        handler.await.unwrap();
    }

    #[tokio::test]
    async fn github_sync_nests_default_destinations_under_one_library_root() {
        use super::super::{sync, Report};
        use sag_protocol::online_import::{Schedule, Source};
        use std::path::PathBuf;

        let temp = tempfile::tempdir().unwrap();
        let service = crate::service::Service::open(temp.path().join("data")).unwrap();
        let documents = temp.path().join("Documents");
        let library = documents.join("sag-wiki");
        let default = library.join("data-sources/github/repository");
        let custom = temp.path().join("custom/repository");
        let resource = "github:blob:main:README.md";
        let mut source = Source {
            id: "default".into(),
            name: "Repository".into(),
            connection: Connection {
                platform: Platform::Github,
                address: "https://github.com/owner/repo".into(),
                drive: false,
                user_access: false,
                app_id: String::new(),
            },
            resources: vec![resource.into()],
            directory: default.to_string_lossy().into_owned(),
            schedule: Schedule::Manual,
            daily_time: "09:00".into(),
            paused: false,
            api: None,
        };

        for (directory, unchanged) in [
            (&default, false),
            (&default, true),
            (&custom, false),
            (&custom, true),
        ] {
            let mut routes = vec![
                ("/repos/owner/repo/branches/main ", json!({"commit":{"commit":{"tree":{"sha":"tree"}}}})),
                ("/repos/owner/repo/git/trees/tree?recursive=1", json!({"truncated":false,"tree":[{"path":"README.md","sha":"blob","mode":"100644"}]})),
            ];
            if !unchanged {
                routes.push(("/repos/owner/repo/git/blobs/blob ", json!("test markdown")));
            }
            let (mut remote, server) = fixture(Platform::Github, false, routes).await;
            remote.connection = source.connection.clone();
            if unchanged {
                let previous_root = if directory == &default { &library } else { &custom };
                let canonical = previous_root.canonicalize().unwrap();
                service.store.remove_root(canonical.to_str().unwrap()).unwrap();
                if directory == &default {
                    service.store.add_root(directory).unwrap();
                }
            } else if directory == &custom {
                source.id = "custom".into();
            }
            source.directory = directory.to_string_lossy().into_owned();
            if !unchanged {
                service.store.connection().execute(
                    "INSERT INTO online_sources(id,config) VALUES(?1,?2)",
                    rusqlite::params![source.id, serde_json::to_string(&source).unwrap()],
                ).unwrap();
            }
            let mut report = Report::default();
            sync(&service, &source, 1, None, &mut report, &remote, Some(&documents)).await.unwrap();
            server.await.unwrap();
            let issues: Vec<_> = report.issues.iter().map(|issue| &issue.reason).collect();
            assert!(issues.is_empty(), "{issues:?}");
            assert_eq!(report.imported, usize::from(!unchanged));
            assert_eq!(report.skipped, usize::from(unchanged));
            let files: Vec<_> = std::fs::read_dir(directory.join("main"))
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect();
            assert_eq!(files.len(), 1);
            assert_eq!(std::fs::read_to_string(&files[0]).unwrap(), "\"test markdown\"");
            let actual: Vec<_> = service.store.roots().unwrap().into_iter()
                .map(|root| PathBuf::from(root.path)).collect();
            let mut expected = vec![library.canonicalize().unwrap()];
            if directory == &custom && !unchanged {
                expected.push(custom.canonicalize().unwrap());
            }
            assert_eq!(actual, expected);
        }
        service.finish().await.unwrap();
    }

    async fn fixture(
        platform: Platform,
        drive: bool,
        routes: Vec<(&'static str, Value)>,
    ) -> (Remote, tokio::task::JoinHandle<()>) {
        fixture_http(
            platform,
            drive,
            routes.into_iter().map(|(path, body)| (path, "200 OK", "", body)).collect(),
        ).await
    }

    async fn fixture_http(
        platform: Platform,
        drive: bool,
        routes: Vec<(&'static str, &'static str, &'static str, Value)>,
    ) -> (Remote, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let test_origin = Some(format!("http://{}", listener.local_addr().unwrap()));
        let handler = tokio::spawn(async move {
            for (expected, status, headers, body) in routes {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut chunk = [0; 4096];
                while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&chunk[..count]);
                }
                let request = String::from_utf8_lossy(&request);
                assert!(
                    request.lines().next().unwrap().contains(expected),
                    "{request}"
                );
                let header = match platform {
                    Platform::Yuque => "x-auth-token: fixture-secret",
                    Platform::Gitlab => "private-token: fixture-secret",
                    Platform::Dingtalk => "x-acs-dingtalk-access-token: fixture-secret",
                    Platform::Ima => "ima-openapi-apikey: fixture-secret",
                    _ => "authorization: bearer fixture-secret",
                };
                assert!(request.to_ascii_lowercase().contains(header));
                let body = body.to_string();
                let response=format!("HTTP/1.1 {status}\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let remote = Remote {
            http: Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(4))
                .build()
                .unwrap(),
            connection: Connection {
                platform,
                drive,
                user_access: false,
                app_id: String::new(),
                address: "folder".into(),
            },
            token: "fixture-secret".into(),
            account_limits: Arc::default(),
            github_files: Default::default(),
            test_origin,
        };
        (remote, handler)
    }

    #[tokio::test]
    async fn feishu_stops_after_first_limit_and_reuses_the_recovery_window() {
        for (status, rate_code) in [("429 Too Many Requests", 99991400), ("400 Bad Request", 99991400)] {
            let (remote, server) = fixture_http(Platform::Feishu, true, vec![
                ("/open-apis/drive/v1/files", status, "x-ogw-ratelimit-reset: 75\r\n", json!({"code":rate_code})),
                ("/open-apis/drive/v1/files", "200 OK", "", json!({"code":0,"data":{"files":[]}})),
            ]).await;
            let started = super::super::now();
            let first = remote.browse(None, None).await.err().unwrap();
            assert_eq!(first.to_string(), "飞书暂时限制了请求频率，请稍后重试。");
            let app_id = &remote.connection.app_id;
            assert!(remote.account_limits.retry_at(Platform::Feishu, app_id).unwrap() >= started + 75);
            assert!(remote.account_limits.check(Platform::Lark, app_id).is_ok());

            let repeated = remote.browse(None, None).await.err().unwrap();
            assert_eq!(repeated.to_string(), first.to_string());
            assert!(!server.is_finished());

            let mut windows = remote.account_limits.0.lock().unwrap();
            windows.get_mut(&Platform::Feishu).unwrap().insert(app_id.clone(), started);
            drop(windows);
            assert!(remote.browse(None, None).await.is_ok());
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn feishu_distinguishes_monthly_quota_on_http_429() {
        let (remote, server) = fixture_http(Platform::Feishu, true, vec![
            ("/open-apis/drive/v1/files", "429 Too Many Requests", "", json!({"code":99991403})),
        ]).await;
        let error = remote.browse(None, None).await.err().unwrap();
        assert_eq!(error.to_string(), "飞书本月的 API 调用额度已用完。");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn feishu_sync_stops_before_the_next_document_when_quota_is_exhausted() {
        use super::super::{sync, Report};
        let temp = tempfile::tempdir().unwrap();
        let service = crate::service::Service::open(temp.path().join("data")).unwrap();
        let (remote, server) = fixture_http(Platform::Feishu, true, vec![
            ("/open-apis/drive/v1/files?", "200 OK", "", json!({"code":0,"data":{"files":[
                {"type":"docx","token":"one","name":"First"},
                {"type":"docx","token":"two","name":"Second"}
            ]}})),
            ("/open-apis/docx/v1/documents/one/blocks", "429 Too Many Requests", "", json!({"code":99991403})),
            ("/open-apis/docx/v1/documents/two/blocks", "200 OK", "", json!({"code":0,"data":{"items":[]}})),
        ]).await;
        let source = sag_protocol::online_import::Source {
            id: "fixture".into(),
            name: "Test".into(),
            connection: remote.connection.clone(),
            resources: vec!["drive:folder".into()],
            directory: temp.path().join("files").to_string_lossy().into_owned(),
            schedule: sag_protocol::online_import::Schedule::Manual,
            daily_time: "09:00".into(),
            paused: false,
            api: None,
        };
        let mut report = Report::default();
        let error = sync(&service, &source, 1, None, &mut report, &remote, None).await.unwrap_err();
        assert!(matches!(error.downcast_ref::<AccountLimit>(), Some(AccountLimit::Quota(Platform::Feishu))));
        assert!(!server.is_finished());
        assert!(report.issues.is_empty());
        server.abort();
        service.finish().await.unwrap();
    }

    #[tokio::test]
    async fn wiki_paginates_and_downloads_blocks_and_drive_lists_the_entered_folder() {
        for platform in [Platform::Feishu, Platform::Lark] {
            let (remote,handler)=fixture(platform,false,vec![
                ("/open-apis/wiki/v2/spaces/space/nodes?",json!({"code":0,"data":{"has_more":true,"page_token":"more","items":[{"node_token":"node","obj_token":"doc","title":"Notes","obj_type":"docx","obj_edit_time":"1"}]}})),
                ("page_token=more",json!({"code":0,"data":{"has_more":false,"items":[]}})),
                ("/open-apis/docx/v1/documents/doc/blocks",json!({"code":0,"data":{"items":[{"block_id":"page","block_type":1,"children":["paragraph"]},{"block_id":"paragraph","block_type":2,"text":{"elements":[{"text_run":{"content":"Indexed evidence"}}]}}]}})),
            ]).await;
            let items = remote.items("wiki:space").await.unwrap();
            assert_eq!(items.len(), 1);
            let bytes = remote.fetch(&items[0]).await.unwrap().bytes;
            let text = String::from_utf8(bytes).unwrap();
            assert!(text.contains("Indexed evidence"));
            assert!(text.contains(if platform == Platform::Feishu {
                "feishu.cn/wiki/node"
            } else {
                "larksuite.com/wiki/node"
            }));
            handler.await.unwrap();
        }
        let (remote,handler)=fixture(Platform::Lark,true,vec![("folder_token=folder",json!({"code":0,"data":{"has_more":false,"files":[{"type":"folder","token":"child","name":"Topic"}]}}))]).await;
        let page = remote.browse(None, None).await.unwrap();
        assert_eq!(page.items[0].id, "drive:folder");
        assert!(page.items[1].expandable);
        handler.await.unwrap();
    }

    #[tokio::test]
    async fn drive_opens_nested_folders_using_the_ids_from_the_listing() {
        for (platform, user_access, root_query) in [
            (Platform::Feishu, true, "folder_token=&"),
            (Platform::Lark, true, "folder_token=&"),
            (Platform::Lark, false, "folder_token=folder&"),
        ] {
            let (mut remote, handler) = fixture(
                platform,
                true,
                vec![
                    (root_query, json!({"code":0,"data":{"files":[{"type":"folder","token":"child","name":"Folder"}]}})),
                    ("folder_token=child&", json!({"code":0,"data":{"files":[{"type":"folder","token":"nested","name":"Nested"}]}})),
                    ("folder_token=nested&", json!({"code":0,"data":{"files":[]}})),
                ],
            ).await;
            remote.connection.user_access = user_access;
            if user_access {
                remote.connection.address.clear();
            }

            let root = remote.browse(None, None).await.unwrap();
            let folder = root.items.iter().find(|item| item.expandable).unwrap();
            assert_eq!(folder.id, "drive:child");
            let children = remote.browse(Some(&folder.id), None).await.unwrap();
            assert_eq!(children.items[0].id, "drive:nested");
            let nested = remote.browse(Some(&children.items[0].id), None).await.unwrap();
            assert!(nested.items.is_empty());
            handler.await.unwrap();
        }
    }

    #[tokio::test]
    async fn account_browses_wiki_spaces_and_imports_a_leaf_page() {
        for platform in [Platform::Feishu, Platform::Lark] {
            let node = json!({
                "node_token": "page", "obj_token": "doc", "title": "Guide",
                "obj_type": "docx", "has_child": false
            });
            let (mut remote, handler) = fixture(
                platform, false, vec![
                    ("GET /open-apis/wiki/v2/spaces?", json!({"code":0,"data":{"items":[{"space_id":"space","name":"Team"}]}})),
                    ("GET /open-apis/wiki/v2/spaces/space/nodes?", json!({"code":0,"data":{"items":[node]}})),
                    ("GET /open-apis/wiki/v2/spaces/get_node?token=page", json!({"code":0,"data":{"node":node}})),
                ],
            ).await;
            remote.connection.user_access = true;

            let spaces = remote.browse(None, None).await.unwrap();
            assert_eq!(spaces.items[0].id, "wiki:space");
            assert!(spaces.items[0].expandable);
            let pages = remote.browse(Some(&spaces.items[0].id), None).await.unwrap();
            assert_eq!(pages.items[0].id, "wiki:space:page");
            assert!(!pages.items[0].expandable);
            let items = remote.items(&pages.items[0].id).await.unwrap();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].id, "doc");
            handler.await.unwrap();
        }
    }

    #[tokio::test]
    async fn notion_incomplete_responses_do_not_become_documents() {
        let (remote,handler)=fixture(Platform::Notion,false,vec![
            ("GET /v1/pages/page ",json!({"last_edited_time":"2026-01-01","url":"https://www.notion.so/page","properties":{"Name":{"title":[{"plain_text":"Guide"}]}}})),
            ("/v1/pages/page/markdown?markdown_version=v2",json!({"truncated":true,"unknown_block_ids":["hidden"],"markdown":"Partial"})),
        ]).await;
        let items = remote.items("page").await.unwrap();
        assert_eq!(items[0].title, "Guide");
        assert!(remote
            .fetch(&items[0])
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("不完整"));
        handler.await.unwrap();
    }

    #[tokio::test]
    async fn yuque_preserves_toc_paths_and_reads_markdown_with_its_book_link() {
        let (remote,handler)=fixture(Platform::Yuque,false,vec![
            ("/api/v2/repos/44/docs?",json!({"data":[{"id":99,"title":"Book page","content_updated_at":"2026-01-01"}]})),
            ("GET /api/v2/repos/44/toc ",json!({"data":[{"uuid":"group","title":"Chapter"},{"uuid":"doc","parent_uuid":"group","doc_id":99}]})),
            ("GET /api/v2/repos/docs/99 ",json!({"data":{"format":"markdown","body":"Important note","book":{"namespace":"org/book"},"slug":"doc"}})),
        ]).await;
        let items = remote.items("book:44").await.unwrap();
        assert_eq!(items[0].folders, ["44", "Chapter"]);
        assert_eq!(items[0].version, "2026-01-01");
        let text = String::from_utf8(remote.fetch(&items[0]).await.unwrap().bytes).unwrap();
        assert!(
            text.contains("Important note") && text.contains("https://www.yuque.com/org/book/doc")
        );
        handler.await.unwrap();
    }

    #[test]
    fn public_feed_addresses_and_markup_keep_only_supported_content() {
        for address in [
            "http://10.2.3.4/feed",
            "http://127.0.0.1",
            "http://169.254.169.254",
            "http://[::ffff:127.0.0.1]",
            "http://[fd00::1]",
            "file:///tmp/feed",
            "https://u:p@example.org",
        ] {
            assert!(public_url(address).is_err(), "{address}");
        }
        assert!(public_url("https://example.org/feed.xml").is_ok());
        assert_eq!(
            feed_markdown(
                "<p><b>Hello</b> <a href='/post'>post</a><script>bad</script></p>",
                "https://example.org/feed"
            ),
            "**Hello** [post](<https://example.org/post>)"
        );
    }
}
