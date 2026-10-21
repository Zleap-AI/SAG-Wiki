use super::*;
use axum::{extract::Request, middleware::{self, Next}};
use rmcp::{model::*, service::{RequestContext, RoleServer}, transport::streamable_http_server::{session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService}, ErrorData, ServerHandler};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use std::{collections::BTreeMap, io::Write, net::SocketAddr, sync::Mutex as StdMutex};

pub const PORT: u16 = 15679;

pub fn address() -> Result<SocketAddr> {
    std::env::var("SAG_FNOS_REMOTE_ADDR").unwrap_or_else(|_| format!("0.0.0.0:{PORT}"))
        .parse().context("远程 MCP 监听地址无效")
}

pub struct Access {
    path: PathBuf,
    tokens: StdMutex<BTreeMap<u32, String>>,
    requests: tokio::sync::Semaphore,
}

impl Access {
    pub fn open(data: &Path) -> Result<Self> {
        let path = data.join("remote-access.json");
        let tokens = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("远程接入配置损坏")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self { path, tokens: StdMutex::new(tokens), requests: tokio::sync::Semaphore::new(20) })
    }

    fn save(&self, tokens: &BTreeMap<u32, String>) -> Result<()> {
        std::fs::create_dir_all(self.path.parent().unwrap())?;
        let mut file = tempfile::NamedTempFile::new_in(self.path.parent().unwrap())?;
        file.write_all(&serde_json::to_vec(tokens)?)?;
        file.as_file().sync_all()?;
        file.persist(&self.path)?;
        Ok(())
    }

    pub fn enabled(&self, uid: u32) -> bool { self.tokens.lock().unwrap().contains_key(&uid) }

    pub fn rotate(&self, uid: u32) -> Result<String> {
        let token = format!("sag_{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        let mut tokens = self.tokens.lock().unwrap();
        let mut next = tokens.clone();
        next.insert(uid, format!("{:x}", Sha256::digest(token.as_bytes())));
        self.save(&next)?;
        *tokens = next;
        Ok(token)
    }

    pub fn revoke(&self, uid: u32) -> Result<()> {
        let mut tokens = self.tokens.lock().unwrap();
        let mut next = tokens.clone();
        next.remove(&uid);
        self.save(&next)?;
        *tokens = next;
        Ok(())
    }

    fn user(&self, headers: &HeaderMap) -> Option<u32> {
        let token = headers.get("authorization")?.to_str().ok()?.strip_prefix("Bearer ")?;
        let hash = format!("{:x}", Sha256::digest(token.as_bytes()));
        self.tokens.lock().unwrap().iter().find_map(|(&uid, expected)| bool::from(hash.as_bytes().ct_eq(expected.as_bytes())).then_some(uid))
    }
}

#[derive(Clone, Copy)]
struct RemoteUser(u32);

async fn authorize(State(app): State<App>, mut request: Request, next: Next) -> Response {
    // Remote clients use their own token, never a browser cookie or supplied UID.
    if request.headers().contains_key("origin") {
        return (StatusCode::FORBIDDEN, "远程 MCP 不接受浏览器跨源请求").into_response();
    }
    let headers = request.headers().clone();
    let Some(uid) = app.remote.user(&headers) else {
        return (StatusCode::UNAUTHORIZED, [("www-authenticate", "Bearer realm=\"SAG Wiki\"")], "访问密钥无效或已撤销").into_response();
    };
    let Ok(_permit) = app.remote.requests.try_acquire() else {
        return (StatusCode::TOO_MANY_REQUESTS, "远程请求过多，请稍后重试").into_response();
    };
    let service = match app.service(uid).await {
        Ok(service) => service,
        Err(error) => return ApiError(error).into_response(),
    };
    if !service.cli.access().enabled {
        return (StatusCode::FORBIDDEN, "CLI 接入已关闭").into_response();
    }
    request.extensions_mut().insert(RemoteUser(uid));
    let response = next.run(request).await;
    // Revocation also applies to searches that were already in progress.
    if app.remote.user(&headers) != Some(uid) || !service.cli.access().enabled {
        return (StatusCode::UNAUTHORIZED, "远程接入已撤销").into_response();
    }
    response
}

#[derive(Clone)]
struct Tools { app: App, client: reqwest::Client }

impl ServerHandler for Tools {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::default();
        info.server_info = Implementation::new("SAG Wiki fnOS", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(format!("这里是飞牛 NAS 上的知识库，返回的路径属于 NAS。使用本组工具读取原文，不要在电脑本地打开这些路径。{}", sag_core::sag_mcp::INSTRUCTIONS));
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info
    }

    async fn list_tools(&self, _: Option<PaginatedRequestParams>, _: RequestContext<RoleServer>) -> std::result::Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult { tools: sag_core::sag_mcp::definitions(), ..Default::default() })
    }

    async fn call_tool(&self, request: CallToolRequestParams, context: RequestContext<RoleServer>) -> std::result::Result<CallToolResponse, ErrorData> {
        let parts = context.extensions.get::<axum::http::request::Parts>().unwrap();
        let uid = parts.extensions.get::<RemoteUser>().unwrap().0;
        let headers = parts.headers.clone();
        let result: Result<Value> = async {
            let service = self.app.service(uid).await?;
            self.app.check_paths(uid, &serde_json::to_value(service.store.files()?)?).await?;
            let tools = sag_core::sag_mcp::SagTools {
                store: service.store.clone(), client: self.client.clone(), data_dir: service.data_dir.clone(),
                settings: service.store.settings()?, scope: vec![], references: vec![],
                enabled: sag_core::sag_mcp::NAMES.iter().map(|name| (*name).into()).collect(),
                cancel: context.ct.clone(), progress: Arc::new(|_, _| {}),
            };
            let output = serde_json::to_value(tools.execute(&request.name, Value::Object(request.arguments.unwrap_or_default())).await?)?;
            self.app.check_paths(uid, &output).await?;
            if self.app.remote.user(&headers) != Some(uid) || !service.cli.access().enabled { bail!("远程接入已撤销"); }
            Ok(output)
        }.await;
        Ok(match result {
            Ok(value) => CallToolResult::structured(value),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(format!("{error:#}"))]),
        }.into())
    }
}

pub fn router(app: App, stop: CancellationToken) -> Router {
    let tools = Tools { app: app.clone(), client: reqwest::Client::new() };
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false).with_json_response(true)
        // Token authentication and rejection of every browser Origin are enforced
        // above, including reverse proxies with different internal Host values.
        .disable_allowed_hosts().enforce_origin_validation()
        .with_max_request_body_bytes(32 * 1024).with_cancellation_token(stop);
    let service = StreamableHttpService::new(move || Ok(tools.clone()), Arc::new(LocalSessionManager::default()), config);
    Router::new().route_service("/mcp", service)
        .layer(middleware::from_fn_with_state(app, authorize))
}
