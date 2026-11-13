//! Public, read-only HTTP entry point; management stays on authenticated local RPC.
use crate::{
    sag_mcp::SagTools,
    service::Service,
    store::{self, Store},
};
use anyhow::{bail, Context, Result};
use http_body_util::{combinators::BoxBody, BodyExt, Full, Limited};
use hyper::{
    body::{Bytes, Incoming},
    Request, Response, StatusCode,
};
use hyper_util::rt::{TokioIo, TokioTimer};
use sag_protocol::ApiAction;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::{
    net::TcpListener,
    sync::Semaphore,
    task::{JoinHandle, JoinSet},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

mod mcp;
mod web;

#[derive(Clone, Serialize, Deserialize)]
struct Key {
    id: String,
    name: String,
    prefix: String,
    created_at: i64,
    digest: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    secret: Option<String>,
    #[serde(skip)]
    cancel: CancellationToken,
}
#[derive(Clone, Serialize, Deserialize)]
struct Config {
    enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    web_code: Option<String>,
    #[serde(skip)]
    api_cancel: CancellationToken,
    #[serde(skip)]
    web_cancel: CancellationToken,
    host: String,
    port: u16,
    keys: Vec<Key>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            web_code: None,
            api_cancel: CancellationToken::new(),
            web_cancel: CancellationToken::new(),
            host: "0.0.0.0".into(),
            port: 24681,
            keys: vec![],
        }
    }
}
struct Running {
    host: String,
    port: u16,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}
pub struct ApiService {
    data: PathBuf,
    config: Mutex<Config>,
    running: Mutex<Option<Running>>,
    last_error: Mutex<Option<String>>,
    gate: tokio::sync::Mutex<()>,
}
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().context("缺少配置目录")?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(windows)]
    crate::server::windows_permissions::protect(file.path())?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}
fn load(data: &Path) -> Result<Config> {
    match std::fs::read(data.join("http-api.json")) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).context("API 配置无效")?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(e.into()),
    }
}
pub fn offline_status(data: &Path) -> Result<Value> {
    let config = load(data)?;
    validate_address(&config.host, config.port)?;
    status_value(&config, false, None, crate::api_autostart::enabled(data)?)
}
pub fn stop_offline(data: &Path) -> Result<Value> {
    // Take the same process lock as server startup to avoid racing a launch.
    std::fs::create_dir_all(data)?;
    let _lock = crate::cli_service::lock_instance(data)?;
    let mut config = load(data)?;
    config.enabled = false;
    write_private(
        &data.join("http-api.json"),
        &serde_json::to_vec_pretty(&config)?,
    )?;
    offline_status(data)
}
fn status_value(
    c: &Config,
    running: bool,
    last_error: Option<String>,
    autostart: bool,
) -> Result<Value> {
    let ip: IpAddr = c.host.parse().expect("stored API address validated");
    let local = if ip.is_unspecified() {
        if ip.is_ipv4() {
            "127.0.0.1".into()
        } else {
            "[::1]".into()
        }
    } else if ip.is_ipv6() {
        format!("[{ip}]")
    } else {
        ip.to_string()
    };
    let mut lan = vec![];
    // Enumerate interfaces locally; no public network request is used.
    let addresses = if ip.is_unspecified() {
        if_addrs::get_if_addrs().map(|ifs| ifs.into_iter().map(|i| i.ip()).collect::<Vec<_>>())
    } else {
        Ok(vec![ip])
    };
    for addr in addresses? {
        if addr.is_loopback() || addr.is_unspecified() || addr.is_ipv4() != ip.is_ipv4() {
            continue;
        }
        let host = if addr.is_ipv6() {
            format!("[{addr}]")
        } else {
            addr.to_string()
        };
        lan.push(format!("http://{host}:{}", c.port));
    }
    lan.sort();
    lan.dedup();
    Ok(
        json!({"enabled":c.enabled,"running":c.enabled&&running,"web_enabled":c.web_code.is_some(),"web_running":c.web_code.is_some()&&running,"host":c.host,"port":c.port,"local_url":format!("http://{local}:{}",c.port),"lan_urls":lan,"key_count":c.keys.len(),"autostart":autostart,"last_error":last_error}),
    )
}
impl ApiService {
    pub fn open(data: &Path) -> Result<Arc<Self>> {
        let config = load(data)?;
        validate_address(&config.host, config.port)?;
        Ok(Arc::new(Self {
            data: data.into(),
            config: Mutex::new(config),
            running: Mutex::new(None),
            last_error: Mutex::new(None),
            gate: tokio::sync::Mutex::new(()),
        }))
    }
    fn save(&self, config: Config) -> Result<()> {
        write_private(
            &self.data.join("http-api.json"),
            &serde_json::to_vec_pretty(&config)?,
        )?;
        *self.config.lock().unwrap() = config;
        Ok(())
    }
    pub fn status(&self) -> Result<Value> {
        let running = self
            .running
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|r| !r.task.is_finished());
        status_value(
            &self.config.lock().unwrap(),
            running,
            self.last_error.lock().unwrap().clone(),
            crate::api_autostart::enabled(&self.data)?,
        )
    }
    pub async fn restore(self: &Arc<Self>, service: &Arc<Service>) {
        let config = self.config.lock().unwrap().clone();
        if config.enabled || config.web_code.is_some() {
            if let Err(e) = self.start(service, config).await {
                *self.last_error.lock().unwrap() = Some(e.to_string());
                eprintln!("API 恢复失败：{e:#}");
            }
        }
    }
    fn start<'a>(
        self: &'a Arc<Self>,
        service: &'a Arc<Service>,
        config: Config,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            validate_address(&config.host, config.port)?;
            if config.web_code.is_some() && !web::available() {
                bail!("当前构建未包含网页资源")
            }
            if let Some(r) = self
                .running
                .lock()
                .unwrap()
                .as_ref()
                .filter(|r| !r.task.is_finished())
            {
                if r.host != config.host || r.port != config.port {
                    bail!("请先关闭网页服务和 API，再修改监听地址或端口")
                }
                return self.save(config);
            }
            let listener = TcpListener::bind((config.host.as_str(), config.port))
                .await
                .map_err(|e| {
                    anyhow::anyhow!(
                        "无法监听 API 地址和端口 {}:{}：{e}",
                        config.host,
                        config.port
                    )
                })?;
            self.save(config.clone())?;
            let cancel = service.stop.child_token();
            let weak = Arc::downgrade(service);
            let api = self.clone();
            let stop = cancel.clone();
            let task = tokio::spawn(async move {
                if let Err(e) = listen(listener, weak, api.clone(), stop).await {
                    *api.last_error.lock().unwrap() = Some(e.to_string());
                    eprintln!("API 监听失败：{e:#}");
                }
            });
            *self.running.lock().unwrap() = Some(Running {
                host: config.host,
                port: config.port,
                cancel,
                task,
            });
            *self.last_error.lock().unwrap() = None;
            Ok(())
        })
    }
    pub(crate) async fn reset(&self) -> Result<()> {
        let _gate = self.gate.lock().await;
        let keys = self.config.lock().unwrap().keys.clone();
        self.save(Config::default())?;
        for key in keys {
            key.cancel.cancel();
        }
        self.shutdown().await?;
        if crate::api_autostart::enabled(&self.data)? {
            let data = self.data.clone();
            tokio::task::spawn_blocking(move || {
                crate::api_autostart::configure(&data, &std::env::current_exe()?, false)
            })
            .await??;
        }
        Ok(())
    }
    pub async fn shutdown(&self) -> Result<()> {
        let running = self.running.lock().unwrap().take();
        if let Some(r) = running {
            r.cancel.cancel();
            r.task.await.context("API 停止任务失败")?;
        }
        Ok(())
    }
    pub async fn manage(
        self: &Arc<Self>,
        action: ApiAction,
        service: &Arc<Service>,
    ) -> Result<Value> {
        let _gate = self.gate.lock().await;
        let web_action = matches!(action, ApiAction::WebStart { .. } | ApiAction::WebStop);
        match action {
            ApiAction::Status => self.status(),
            ApiAction::Start { host, port } | ApiAction::WebStart { host, port } => {
                let mut c = self.config.lock().unwrap().clone();
                c.host = host.unwrap_or(c.host);
                c.port = port.unwrap_or(c.port);
                if web_action {
                    c.web_code.get_or_insert_with(|| uuid::Uuid::new_v4().simple().to_string());
                } else {
                    c.enabled = true;
                }
                self.start(service, c).await?;
                self.status()
            }
            ApiAction::Stop | ApiAction::WebStop => {
                let mut c = self.config.lock().unwrap().clone();
                let cancel = if web_action {
                    c.web_code = None;
                    std::mem::take(&mut c.web_cancel)
                } else {
                    c.enabled = false;
                    std::mem::take(&mut c.api_cancel)
                };
                let idle = !c.enabled && c.web_code.is_none();
                self.save(c)?;
                cancel.cancel();
                if idle { self.shutdown().await?; }
                *self.last_error.lock().unwrap() = None;
                self.status()
            }
            ApiAction::WebCode => {
                let c = self.config.lock().unwrap();
                Ok(json!({"code":c.web_code.as_deref().context("请先启用网页服务")?}))
            }
            ApiAction::WebRotateCode => {
                let mut c = self.config.lock().unwrap().clone();
                anyhow::ensure!(c.web_code.is_some(), "请先启用网页服务");
                c.web_code = Some(uuid::Uuid::new_v4().simple().to_string());
                let cancel = std::mem::take(&mut c.web_cancel);
                self.save(c)?;
                cancel.cancel();
                self.status()
            }
            ApiAction::Configure { host, port } => {
                validate_address(&host, port)?;
                if self
                    .running
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_some_and(|r| !r.task.is_finished())
                {
                    bail!("请先关闭网页服务和 API，再修改监听地址或端口")
                }
                let mut c = self.config.lock().unwrap().clone();
                c.host = host;
                c.port = port;
                self.save(c)?;
                self.status()
            }
            ApiAction::ListKeys => Ok(Value::Array(
                self.config
                    .lock()
                    .unwrap()
                    .keys
                    .iter()
                    .map(key_metadata)
                    .collect(),
            )),
            ApiAction::CopyKey { id } => {
                let config = self.config.lock().unwrap();
                let secret = match id {
                    Some(id) => config.keys.iter().find(|key| key.id == id)
                        .context("API Key 不存在")?.secret.as_ref(),
                    None => config.keys.iter().rev().find_map(|key| key.secret.as_ref()),
                }.context("请先创建 API Key")?;
                Ok(json!({"key":secret}))
            }
            ApiAction::CreateKey { name } => {
                if name.trim().is_empty() || name.chars().count() > 100 {
                    bail!("Key 名称需为 1–100 个字符")
                }
                let secret = format!(
                    "sag_{}{}",
                    uuid::Uuid::new_v4().simple(),
                    uuid::Uuid::new_v4().simple()
                );
                let key = Key {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: name.trim().into(),
                    prefix: format!("{}…{}", &secret[..12], &secret[secret.len() - 4..]),
                    created_at: store::now(),
                    digest: Sha256::digest(secret.as_bytes()).to_vec(),
                    secret: Some(secret.clone()),
                    cancel: CancellationToken::new(),
                };
                let mut result = key_metadata(&key);
                result["key"] = json!(secret);
                let mut c = self.config.lock().unwrap().clone();
                c.keys.push(key);
                self.save(c)?;
                Ok(result)
            }
            ApiAction::RevokeKey { id } => {
                let mut c = self.config.lock().unwrap().clone();
                let key = c
                    .keys
                    .iter()
                    .find(|k| k.id == id)
                    .context("API Key 不存在")?
                    .clone();
                c.keys.retain(|k| k.id != id);
                self.save(c)?;
                key.cancel.cancel();
                Ok(json!({"revoked":id}))
            }
            ApiAction::Autostart { enabled } => {
                let data = self.data.clone();
                let result = tokio::task::spawn_blocking(move || match enabled {
                    Some(value) => {
                        crate::api_autostart::configure(&data, &std::env::current_exe()?, value)
                    }
                    None => crate::api_autostart::enabled(&data),
                })
                .await??;
                Ok(json!({"enabled":result}))
            }
        }
    }
    fn authenticate(&self, request: &Request<Incoming>) -> Option<(CancellationToken, CancellationToken)> {
        let header = request
            .headers()
            .get(hyper::header::AUTHORIZATION)?
            .to_str()
            .ok()?;
        let (scheme, secret) = header.split_once(' ')?;
        if !scheme.eq_ignore_ascii_case("Bearer") || secret.is_empty() {
            return None;
        }
        let digest = Sha256::digest(secret.as_bytes());
        let c = self.config.lock().unwrap();
        if !c.enabled { return None; }
        c
            .keys
            .iter()
            .find(|k| bool::from(k.digest.ct_eq(digest.as_slice())))
            .map(|k| (k.cancel.clone(), c.api_cancel.clone()))
    }
}
fn key_metadata(k: &Key) -> Value {
    json!({"id":k.id,"name":k.name,"prefix":k.prefix,"created_at":k.created_at,"copyable":k.secret.is_some()})
}
fn validate_address(host: &str, port: u16) -> Result<()> {
    let _: IpAddr = host.parse().context("请输入有效的监听 IP 地址")?;
    if port == 0 {
        bail!("端口范围为 1–65535")
    }
    Ok(())
}

type HttpResponse = Response<BoxBody<Bytes, std::convert::Infallible>>;
fn response(status: StatusCode, value: Value) -> HttpResponse {
    Response::builder()
        .status(status)
        .header("content-type", "application/json; charset=utf-8")
        .header("cache-control", "no-store")
        .body(Full::new(Bytes::from(serde_json::to_vec(&value).unwrap())).boxed())
        .unwrap()
}
#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}
impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl ToString) -> Self {
        Self {
            status,
            code,
            message: message.to_string(),
        }
    }
    fn internal(e: impl ToString) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", e)
    }
    fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", "未授权")
    }
    fn stopped() -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, "api_stopped", "API 已关闭")
    }
    fn bad(e: impl ToString) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request", e)
    }
    fn response(self, id: &str) -> HttpResponse {
        response(
            self.status,
            json!({"request_id":id,"error":{"code":self.code,"message":self.message}}),
        )
    }
}
async fn listen(
    listener: TcpListener,
    service: Weak<Service>,
    api: Arc<ApiService>,
    stop: CancellationToken,
) -> Result<()> {
    let connections = Arc::new(Semaphore::new(64));
    let mut jobs = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            _=stop.cancelled()=>break,
            accepted=listener.accept()=>{
                let (stream,_)=accepted?;
                let Ok(permit)=connections.clone().try_acquire_owned() else {continue};
                let local=stream.local_addr()?;
                let service=service.clone();let api=api.clone();let stop=stop.clone();
                jobs.spawn(async move {
                    let _permit=permit;
                    let disconnected=CancellationToken::new();let _guard=disconnected.clone().drop_guard();
                    let disconnect=disconnected.clone();let shutdown=stop.clone();
                    let handler=hyper::service::service_fn(move|request|{
                        let service=service.clone();let api=api.clone();let stop=stop.clone();let disconnected=disconnected.clone();
                        async move {Ok::<_,std::convert::Infallible>(handle_http(request,service,api,stop,disconnected,local).await)}
                    });
                    let mut builder=hyper::server::conn::http1::Builder::new();
                    builder.timer(TokioTimer::new()).header_read_timeout(Duration::from_secs(10));
                    let connection=builder.serve_connection(TokioIo::new(stream),handler);tokio::pin!(connection);
                    let result=tokio::select! {
                        result=&mut connection=>result,
                        _=shutdown.cancelled()=>{connection.as_mut().graceful_shutdown();connection.await},
                    };
                    disconnect.cancel();
                    if let Err(e)=result {if !e.is_incomplete_message()&&!e.is_closed(){eprintln!("API HTTP 连接结束：{e}")}}
                });
            },
            result=jobs.join_next(),if !jobs.is_empty()=>{result.unwrap().context("API 连接任务失败")?;},
        }
    }
    drop(listener);
    while let Some(result) = jobs.join_next().await {
        result.context("API 连接停止失败")?;
    }
    Ok(())
}
async fn handle_http(
    mut request: Request<Incoming>,
    service: Weak<Service>,
    api: Arc<ApiService>,
    stop: CancellationToken,
    disconnected: CancellationToken,
    local: SocketAddr,
) -> HttpResponse {
    let started = Instant::now();
    let id = uuid::Uuid::new_v4().to_string();
    if web::owns(request.uri().path()) {
        return web::handle(request, service, &api, stop, disconnected, local, &id).await;
    }
    let is_mcp = request.uri().path() == "/mcp";
    let Some((key, api_enabled)) = api.authenticate(&request) else {
        let mut response = ApiError::unauthorized().response(&id);
        if is_mcp {
            response.headers_mut().insert(hyper::header::WWW_AUTHENTICATE, hyper::header::HeaderValue::from_static("Bearer"));
        }
        return response;
    };
    let Some(service) = service.upgrade() else {
        return ApiError::stopped().response(&id);
    };
    let cancel = CancellationToken::new();
    let _guard = cancel.clone().drop_guard();
    let operation = async {
        if is_mcp {
            request.headers_mut().remove(hyper::header::AUTHORIZATION);
            mcp::handle(request, service, cancel.clone(), started, &id, local).await
        } else {
            dispatch(request, service, cancel.clone(), started, &id).await.map(|value| response(StatusCode::OK, value))
        }
    };
    let result = tokio::select! {
        biased;
        _=key.cancelled()=>Err(ApiError::unauthorized()),
        _=api_enabled.cancelled()=>Err(ApiError::stopped()),
        _=stop.cancelled()=>Err(ApiError::stopped()),
        _=disconnected.cancelled()=>Err(ApiError::new(StatusCode::REQUEST_TIMEOUT,"request_cancelled","调用方已断开")),
        result=operation=>result,
    };
    match result {
        Ok(response) => response,
        Err(e) => e.response(&id),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchRequest {
    query: String,
    #[serde(default = "deep")]
    mode: String,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default = "limit")]
    limit: usize,
}
fn deep() -> String {
    "deep".into()
}
fn limit() -> usize {
    10
}
async fn dispatch(
    request: Request<Incoming>,
    service: Arc<Service>,
    cancel: CancellationToken,
    started: Instant,
    id: &str,
) -> std::result::Result<Value, ApiError> {
    let path = request.uri().path();
    let expected = match path {
        "/v1/search" => hyper::Method::POST,
        "/v1/folders" | "/v1/status" => hyper::Method::GET,
        _ => {
            return Err(ApiError::new(
                StatusCode::NOT_FOUND,
                "not_found",
                "接口不存在",
            ))
        }
    };
    if request.method() != expected {
        return Err(ApiError::new(
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
            "HTTP 方法不受支持",
        ));
    }
    match path {
        "/v1/status" => {
            let c = service.api.config.lock().unwrap().clone();
            let roots = service
                .store
                .roots()
                .map_err(ApiError::internal)?
                .into_iter()
                .map(|r| json!({"path":r.path,"status":r.status,"indexed_files":r.count}))
                .collect::<Vec<_>>();
            Ok(
                json!({"version":sag_protocol::BUILD_VERSION,"api":{"enabled":c.enabled,"running":true},"decision_configured":service.cli.status().map_err(ApiError::internal)?.jev_configured,"roots":roots}),
            )
        }
        "/v1/folders" => {
            let url = reqwest::Url::parse(&format!("http://localhost{}", request.uri()))
                .map_err(ApiError::bad)?;
            let mut path = None;
            for (key, value) in url.query_pairs() {
                if key != "path" || path.is_some() {
                    return Err(ApiError::bad("仅支持一个 path 查询参数"));
                }
                path = Some(value.into_owned());
            }
            let store = service.store.clone();
            tokio::task::spawn_blocking(move || folders(&store, path.as_deref()))
                .await
                .map_err(ApiError::internal)?
        }
        "/v1/search" => {
            let content_type = request
                .headers()
                .get(hyper::header::CONTENT_TYPE)
                .and_then(|h| h.to_str().ok())
                .unwrap_or("");
            if !content_type
                .split(';')
                .next()
                .is_some_and(|s| s.trim().eq_ignore_ascii_case("application/json"))
            {
                return Err(ApiError::bad("Content-Type 必须为 application/json"));
            }
            let collected = tokio::time::timeout_at(
                started + Duration::from_secs(30),
                Limited::new(request.into_body(), sag_protocol::MAX_REQUEST as usize).collect(),
            )
            .await
            .map_err(|_| {
                ApiError::new(
                    StatusCode::REQUEST_TIMEOUT,
                    "invalid_request",
                    "请求体接收超时",
                )
            })?;
            let bytes = collected
                .map_err(|e| {
                    if e.is::<http_body_util::LengthLimitError>() {
                        ApiError::new(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            "request_too_large",
                            "请求体过大",
                        )
                    } else {
                        ApiError::bad(e)
                    }
                })?
                .to_bytes();
            let input: SearchRequest = serde_json::from_slice(&bytes).map_err(ApiError::bad)?;
            run_search(service, input, cancel, started, id.to_owned()).await
        }
        _ => unreachable!(),
    }
}
async fn run_search(
    service: Arc<Service>,
    input: SearchRequest,
    cancel: CancellationToken,
    started: Instant,
    id: String,
) -> std::result::Result<Value, ApiError> {
    if input.query.trim().is_empty()
        || input.query.chars().count() > 2000
        || !(1..=100).contains(&input.limit)
        || !matches!(input.mode.as_str(), "deep" | "fast")
    {
        return Err(ApiError::bad("query 需为 1–2000 个字符，mode 为 deep 或 fast，limit 范围 1–100"));
    }
    let deadline = started + Duration::from_secs(if input.mode == "fast" { 30 } else { 60 });
    let operation = async {
        let permit = tokio::select! {_=cancel.cancelled()=>return Err(ApiError::stopped()),permit=service.requests.clone().acquire_owned()=>permit.map_err(ApiError::internal)?};
        let work = tokio::spawn(async move {
            let _permit = permit;
            search(service, input, cancel, started, id).await
        });
        work.await.map_err(ApiError::internal)?
    };
    tokio::time::timeout_at(deadline, operation).await.map_err(|_| {
        ApiError::new(StatusCode::GATEWAY_TIMEOUT, "search_timeout", "搜索请求超时")
    })?
}
fn directory(store: &Store, value: &str) -> std::result::Result<PathBuf, ApiError> {
    let path = Path::new(value);
    if !path.is_absolute() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid_path",
            "请使用完整目录路径",
        ));
    }
    let canonical = path.canonicalize().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ApiError::new(StatusCode::NOT_FOUND, "directory_not_found", "目录不存在")
        } else {
            ApiError::bad(e)
        }
    })?;
    if !canonical.is_dir() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid_path",
            "路径必须为目录",
        ));
    }
    store.indexed_path(&canonical).map_err(|_| {
        ApiError::new(
            StatusCode::FORBIDDEN,
            "path_not_allowed",
            "目录不在允许的知识库范围内",
        )
    })
}
fn scopes(store: &Store, paths: &[String]) -> std::result::Result<Vec<String>, ApiError> {
    let mut paths = if paths.is_empty() {
        store
            .roots()
            .map_err(ApiError::internal)?
            .into_iter()
            .map(|r| directory(store, &r.path))
            .collect::<std::result::Result<Vec<_>, _>>()?
    } else {
        paths
            .iter()
            .map(|p| directory(store, p))
            .collect::<std::result::Result<Vec<_>, _>>()?
    };
    paths.sort();
    paths.dedup();
    let mut selected: Vec<PathBuf> = vec![];
    for path in paths {
        if !selected.iter().any(|p| path.starts_with(p)) {
            selected.push(path)
        }
    }
    selected
        .iter()
        .map(|p| store::path_text(p).map_err(ApiError::internal))
        .collect()
}
fn folders(store: &Store, path: Option<&str>) -> std::result::Result<Value, ApiError> {
    let values = if let Some(path) = path {
        let path = directory(store, path)?;
        let settings = store.settings().map_err(ApiError::internal)?;
        let root = store
            .roots()
            .map_err(ApiError::internal)?
            .into_iter()
            .find(|r| path.starts_with(&r.path))
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::FORBIDDEN,
                    "path_not_allowed",
                    "目录已从知识库移除",
                )
            })?;
        let mut rules = crate::ignore_rules::matcher(Path::new(&root.path));
        let mut values = vec![];
        for entry in store
            .entries(&path, settings.show_hidden)
            .map_err(ApiError::internal)?
        {
            let indexed_path = path.join(&entry.name);
            if entry.directory
                && !store.is_excluded(&indexed_path).map_err(ApiError::internal)?
                && !crate::ignore_rules::is_ignored(&mut rules, &indexed_path, true)
                    .map_err(ApiError::internal)?
            {
                values.push(json!({"name":entry.name,"path":entry.path}))
            }
        }
        values
    } else {
        store
            .roots()
            .map_err(ApiError::internal)?
            .into_iter()
            .map(|r| json!({"name":r.name,"path":r.path}))
            .collect()
    };
    Ok(json!({"folders":values}))
}
async fn search(
    service: Arc<Service>,
    input: SearchRequest,
    cancel: CancellationToken,
    started: Instant,
    id: String,
) -> std::result::Result<Value, ApiError> {
    let store = service.store.clone();
    let paths = input.paths.clone();
    let scope = tokio::task::spawn_blocking(move || scopes(&store, &paths))
        .await
        .map_err(ApiError::internal)??;
    let canonical_scope = scope.iter().map(|path| Path::new(path).canonicalize())
        .collect::<std::io::Result<Vec<_>>>().map_err(ApiError::internal)?;
    let settings = service.store.settings().map_err(ApiError::internal)?;
    if input.mode == "deep"
        && !service
            .cli
            .status()
            .map_err(ApiError::internal)?
            .jev_configured
    {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "decision_not_configured",
            "请先配置决策模型",
        ));
    }
    let candidates = if scope.is_empty() {
        vec![]
    } else if input.mode == "fast" {
        let store = service.store.clone();
        let query = input.query.clone();
        let scope = scope.clone();
        let limit = input.limit;
        tokio::task::spawn_blocking(move || {
            let expression = crate::text::match_query(&query);
            if expression.is_empty() {
                Ok(vec![])
            } else {
                store.candidate_query(&expression, None, &scope, &[], &[], limit)
            }
        })
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?
    } else {
        let tools = SagTools {
            store: service.store.clone(),
            client: service.http_client(),
            data_dir: service.data_dir.clone(),
            settings,
            scope: scope.clone(),
            references: vec![],
            enabled: vec!["search".into()],
            cancel: cancel.clone(),
            progress: Arc::new(|_, _| {}),
        };
        tokio::select! {_=cancel.cancelled()=>return Err(ApiError::stopped()),result=tools.execute("search",json!({"query":input.query.trim(),"top_k":input.limit}))=>result.map_err(|e| {
            if e.downcast_ref::<crate::models::DecisionRequestError>().is_some() {
                ApiError::new(StatusCode::BAD_GATEWAY,"decision_request_failed",e)
            } else { ApiError::internal(e) }
        })?.sources}
    };
    if cancel.is_cancelled() {
        return Err(ApiError::stopped());
    }
    let store = service.store.clone();
    let token = cancel.clone();
    let (results,truncated)=tokio::task::spawn_blocking(move||{
        let mut groups=indexmap::IndexMap::<(String,String),(String,Vec<(usize,usize)>)>::new();
        for candidate in candidates {
            let (_,ranges)=groups.entry((candidate.path,candidate.heading)).or_insert_with(||(candidate.hash,vec![]));
            ranges.extend(candidate.source_spans.iter().map(|span|(span.start_byte,span.end_byte)));
        }
        let mut results=vec![];let mut bytes=0;
        for ((path,heading),(hash,mut ranges)) in groups {
            if token.is_cancelled(){return Err(ApiError::stopped())}
            let document=store.document(Path::new(&path)).map_err(ApiError::internal)?;
            if document.hash!=hash{return Err(ApiError::internal("文件已修改，请重新搜索"))}
            let canonical=store.authorize(Path::new(&path)).map_err(ApiError::internal)?;
            if !canonical_scope.iter().any(|s|canonical.starts_with(s)){return Err(ApiError::internal("结果超出搜索范围"))}
            ranges.sort_unstable();let mut merged:Vec<(usize,usize)>=vec![];
            for (start,end) in ranges{if let Some(last)=merged.last_mut().filter(|last|start<=last.1){last.1=last.1.max(end)}else{merged.push((start,end))}}
            let lines=crate::text::Lines::new(&document.body);
            for (start,end) in merged {
                let span=lines.span(&document.body,start,end);
                let result=json!({"path":document.path,"heading":heading,"start_line":span.start_line,"end_line":span.end_line,"content":document.body[span.start_byte..span.end_byte]});
                bytes+=serde_json::to_vec(&result).map_err(ApiError::internal)?.len()+1;
                if bytes>sag_protocol::MAX_RESPONSE as usize-1024{return Ok((results,true))}
                results.push(result);
                if results.len()==input.limit{return Ok((results,false))}
            }
        }
        Ok((results,false))
    }).await.map_err(ApiError::internal)??;
    let output = json!({"request_id":id,"mode":input.mode,"elapsed_ms":started.elapsed().as_millis() as u64,"results":results,"truncated":truncated});
    Ok(output)
}

#[cfg(test)]
mod tests;
