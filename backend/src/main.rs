mod file_permissions;
#[cfg(target_os = "linux")]
mod document_publish;
mod cli;
mod fnos;
mod remote;

use anyhow::{bail, Context, Result};
use axum::{extract::{DefaultBodyLimit, Query, State}, http::{HeaderMap, StatusCode}, response::{sse::{Event, KeepAlive, Sse}, IntoResponse, Response}, routing::{get, post}, Json, Router};
use fnos::{Fnos, PREFIX};
use sag_core::service::{EventSink, Service};
use sag_protocol::Operation;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::{HashMap, HashSet}, convert::Infallible, path::{Path, PathBuf}, sync::Arc};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
struct App {
    fnos: Fnos,
    data: PathBuf,
    users: Arc<Mutex<HashMap<u32, Arc<Service>>>>,
    worker_socket: PathBuf,
    remote: Arc<remote::Access>,
}

struct ApiError(anyhow::Error);
impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(value: E) -> Self { Self(value.into()) }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, Json(json!({"error": self.0.to_string()}))).into_response()
    }
}

// Browser RPC is only served on the fnOS gateway socket; remote MCP uses its own token.
fn user(headers: &HeaderMap) -> Result<u32> {
    headers.get("x-trim-userid").context("请从飞牛桌面打开应用，缺少网关用户身份")?
        .to_str()?.parse().context("飞牛网关用户身份无效")
}

fn same_origin(headers: &HeaderMap) -> Result<()> {
    // Fetch Metadata describes the browser-facing origin, even when the fnOS
    // gateway replaces Host while proxying to our Unix socket. Page scripts
    // cannot set Sec-* headers. Keep the Origin check for older clients.
    if let Some(site) = headers.get("sec-fetch-site") {
        if site != "same-origin" { bail!("请求来源与应用地址不一致"); }
        return Ok(());
    }
    // A cross-origin script must preflight this non-simple header. This router
    // never grants CORS access, so it cannot be attached by a foreign web page.
    // Unlike forwarded Host headers, it also works through the fnOS HTTP gateway.
    if headers.get("x-sag-request").is_some_and(|value| value == "1") {
        if let Some(origin) = headers.get("origin") {
            let origin = reqwest::Url::parse(origin.to_str()?)?;
            if !matches!(origin.scheme(), "http" | "https") { bail!("请求来源与应用地址不一致"); }
        }
        return Ok(());
    }
    if let Some(origin) = headers.get("origin") {
        let origin = reqwest::Url::parse(origin.to_str()?)?;
        let host = headers.get("host").context("缺少 Host")?.to_str()?;
        let expected = reqwest::Url::parse(&format!("{}://{host}", origin.scheme()))?;
        if !matches!(origin.scheme(), "http" | "https") || origin.origin() != expected.origin() {
            bail!("请求来源与应用地址不一致");
        }
    }
    Ok(())
}

impl App {
    async fn service(&self, uid: u32) -> Result<Arc<Service>> {
        let mut users = self.users.lock().await;
        if let Some(service) = users.get(&uid) { return Ok(service.clone()); }
        let data = self.data.join("users").join(uid.to_string());
        std::fs::create_dir_all(&data)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700))?;
        let native = self.fnos.clone();
        let runtime = tokio::runtime::Handle::current();
        let policy: sag_core::store::FileAccess = Arc::new(move |path, write| {
            let check = async {
                if write == sag_core::store::FileAccessMode::Publish {
                    // Source access is still checked separately; only our conversion
                    // output and directories can gain a named-user ACL.
                    let root = path.ancestors().find(|ancestor| ancestor.file_name().is_some_and(|name| name == ".sag-converted"))
                        .context("转换目录无效")?.parent().context("转换目录缺少父目录")?;
                    if !native.allowed(uid, root, false).await? { return Ok(false); }
                    file_permissions::share_conversion(path, uid, native.allowed(uid, root, true).await?)?;
                    Ok(true)
                } else { native.allowed_action(uid, path, write).await }
            };
            if tokio::runtime::Handle::try_current().is_ok() {
                tokio::task::block_in_place(|| runtime.block_on(check))
            } else { runtime.block_on(check) }
        });
        let runtime = sag_core::mcp::StdioRuntime::Fnos { socket: self.worker_socket.clone(), uid };
        let service = tokio::task::spawn_blocking(move || Service::open_with_runtime(data, Some(policy), runtime)).await??;
        #[cfg(target_os = "linux")]
        {
            let socket = self.worker_socket.clone();
            service.store.set_document_publisher(Arc::new(move |temporary, target, expected_hash| {
                document_publish::request(&socket, &document_publish::Request {
                    uid, temporary: temporary.into(), target: target.into(), expected_hash: expected_hash.map(str::to_owned),
                })
            }));
        }
        users.insert(uid, service.clone());
        Ok(service)
    }

    async fn check_paths(&self, uid: u32, value: &Value) -> Result<()> {
        let mut paths = HashSet::new();
        collect_paths(value, &mut paths);
        for path in paths { self.fnos.require(uid, &path, false).await?; }
        Ok(())
    }
}

fn collect_paths(value: &Value, paths: &mut HashSet<String>) {
    match value {
        Value::Array(items) => for item in items { collect_paths(item, paths); },
        Value::Object(object) => for (key, value) in object {
            if matches!(key.as_str(), "path" | "scope" | "references") {
                match value {
                    Value::String(path) if !path.is_empty() => {
                        let path = if object.get("is_new").and_then(Value::as_bool) == Some(true) {
                            Path::new(path).parent().unwrap_or(Path::new(path)).to_string_lossy().into_owned()
                        } else { path.clone() };
                        paths.insert(path);
                    }
                    Value::Array(items) => for item in items {
                        if let Some(path) = item.as_str() { if !path.is_empty() { paths.insert(path.to_owned()); } }
                    },
                    _ => {},
                }
            }
            collect_paths(value, paths);
        },
        _ => {},
    }
}

#[derive(Deserialize)]
struct Rpc { command: String, #[serde(default)] args: serde_json::Map<String, Value>, session: uuid::Uuid }

fn operation(request: &Rpc) -> Result<Operation> {
    let command = match request.command.as_str() {
        "get_service_info" => "info",
        "get_docling_status" => "docling_status",
        "list_removed_directories" => "removed_directories",
        "list_roots" | "list_entries" | "list_files" | "read_document" | "save_document" |
        "add_root" | "add_root_with_options" | "get_folder_index_options" | "save_folder_index_options" |
        "remove_root" | "refresh_root" | "reindex_all" | "get_index_status" | "get_index_history" |
        "install_docling" | "cancel_docling_install" | "uninstall_docling" | "get_index_history_log" | "clear_index_history" | "set_conversion_workers" | "set_index_paused" |
        "remove_all_roots" | "put_draft" | "list_drafts" | "delete_draft" | "get_settings" | "save_settings" |
        "clear_model_key" | "test_model" | "test_mcp" | "list_conversations" | "clear_conversations" |
        "clear_all_data" | "edit_conversation" | "replace_messages" | "run_query" | "summarize_search" |
        "cancel_query" | "resolve_tool_approval" | "explore_overview" | "explore_graph" |
        "get_access" | "set_access" | "explore_neighbors" | "explore_evidence" | "load_image" | "authorize_path" => &request.command,
        _ => bail!("飞牛版不支持此操作：{}", request.command),
    };
    let args: serde_json::Map<String, Value> = request.args.iter().map(|(key, value)| {
        let mut normalized = String::new();
        for ch in key.chars() { if ch.is_uppercase() { normalized.push('_'); normalized.extend(ch.to_lowercase()); } else { normalized.push(ch); } }
        (normalized, value.clone())
    }).collect();
    let value = if args.is_empty() { json!({"operation":command}) } else { json!({"operation":command,"args":args}) };
    Ok(serde_json::from_value(value).context("请求参数无效")?)
}

async fn rpc(State(app): State<App>, headers: HeaderMap, Json(request): Json<Rpc>) -> Result<Json<Value>, ApiError> {
    let uid = user(&headers)?;
    same_origin(&headers)?;
    if request.command == "fnos_remote_status" {
        return Ok(Json(json!({"enabled":app.remote.enabled(uid),"port":remote::address()?.port()})));
    }
    if request.command == "fnos_remote_rotate" {
        if !app.service(uid).await?.cli.access().enabled { return Err(anyhow::anyhow!("请先启用 CLI 接入").into()); }
        return Ok(Json(json!({"token":app.remote.rotate(uid)?})));
    }
    if request.command == "fnos_remote_revoke" {
        app.remote.revoke(uid)?;
        return Ok(Json(Value::Null));
    }
    if request.command == "fnos_cli_run" {
        return Ok(Json(cli::run_command(&app, uid, &request.args).await?));
    }
    if request.command == "fnos_accessible_folders" {
        let paths = app.fnos.folders(uid).await?;
        let mut visible = Vec::new();
        for path in paths { if app.fnos.allowed(uid, Path::new(&path), false).await? { visible.push(path); } }
        return Ok(Json(json!(visible)));
    }
    if request.command == "fnos_convert_paths" {
        let paths: Vec<String> = serde_json::from_value(request.args.get("paths").cloned().context("缺少 paths")?)?;
        for path in &paths { app.fnos.require(uid, path, false).await?; }
        let language = request.args.get("language").and_then(Value::as_str).context("缺少 language")?;
        return Ok(Json(json!({"status":0,"result":app.fnos.convert_paths(&paths, language).await?})));
    }
    let operation = operation(&request)?;
    let service = app.service(uid).await?;
    // Cached search/history can contain file contents, so recheck their sources.
    if matches!(&operation, Operation::RunQuery { .. } | Operation::SummarizeSearch { .. } |
        Operation::ListConversations | Operation::ReplaceMessages { .. } | Operation::ExploreOverview { .. } |
        Operation::ExploreGraph { .. } | Operation::ExploreNeighbors { .. } | Operation::ExploreEvidence { .. }) {
        app.check_paths(uid, &serde_json::to_value(service.store.files()?)?).await?;
        app.check_paths(uid, &serde_json::to_value(service.store.conversations()?)?).await?;
    }
    match &operation {
        Operation::SaveDocument { path, .. } => app.fnos.require(uid, path, true).await?,
        Operation::PutDraft { draft } => app.fnos.require(uid, if draft.is_new {
            Path::new(&draft.path).parent().context("草稿缺少父目录")?.to_str().context("草稿路径无效")?
        } else { &draft.path }, true).await?,
        Operation::AddRootWithOptions { path, .. } | Operation::SaveFolderIndexOptions { path, .. } => {
            app.fnos.require(uid, path, false).await?;
            app.fnos.require(uid, &Path::new(path).join(".sagignore").to_string_lossy(), true).await?;
        }
        Operation::RemoveRoot { .. } | Operation::RemoveAllRoots | Operation::ClearAllData |
        Operation::DeleteDraft { .. } | Operation::ClearConversations | Operation::EditConversation { .. } => {},
        _ => app.check_paths(uid, &Value::Object(request.args.clone())).await?,
    }
    let sink = EventSink { owner: request.session.to_string(), request_id: Some(uuid::Uuid::new_v4().to_string()), cancel: CancellationToken::new(), bus: service.events.clone() };
    let output = service.handle(operation, &sink).await.map_err(anyhow::Error::msg)?;
    let envelope = serde_json::to_value(output)?;
    let mut data = envelope.get("data").cloned().unwrap_or(Value::Null);
    if request.command == "list_drafts" {
        let mut visible = Vec::new();
        for draft in data.as_array().context("草稿列表格式无效")? {
            let path = Path::new(draft["path"].as_str().context("草稿路径无效")?);
            let access_path = if draft["is_new"] == true { path.parent().context("草稿缺少父目录")? } else { path };
            if app.fnos.allowed(uid, access_path, false).await? { visible.push(draft.clone()); }
        }
        data = Value::Array(visible);
    }
    app.check_paths(uid, &data).await?;
    Ok(Json(data))
}


#[derive(Deserialize)] struct Events { session: uuid::Uuid }
async fn events(State(app): State<App>, headers: HeaderMap, Query(query): Query<Events>) -> Result<impl IntoResponse, ApiError> {
    let uid = user(&headers)?;
    same_origin(&headers)?;
    let service = app.service(uid).await?;
    let mut receiver = service.events.subscribe();
    let owner = query.session.to_string();
    let stream = async_stream::stream! {
        yield Ok::<Event, Infallible>(Event::default().event("ready").data("{}"));
        loop {
            let received = tokio::select! {
                _ = service.stop.cancelled() => break,
                event = receiver.recv() => event,
            };
            match received {
                Ok((target, envelope)) if target.as_ref().is_none_or(|target| target == &owner) => {
                    let value = serde_json::to_value(envelope.event).expect("service events are serializable");
                    match app.check_paths(uid, &value).await {
                        Ok(()) => yield Ok(Event::default().data(value.to_string())),
                        Err(error) => yield Ok(Event::default().data(json!({"event":"app-error","payload":error.to_string()}).to_string())),
                    }
                }
                Ok(_) => {},
                Err(error) => {
                    yield Ok(Event::default().data(json!({"event":"app-error","payload":format!("事件连接中断，请刷新页面：{error}")}).to_string()));
                    break;
                }
            }
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn router(app: App, assets: PathBuf) -> Router {
    let index = assets.join("index.html");
    let page = get(move || {
        let index = index.clone();
        async move { tokio::fs::read_to_string(index).await.map(axum::response::Html).map_err(ApiError::from) }
    });
    Router::new().route(&format!("{PREFIX}/"), page.clone()).nest(PREFIX, Router::new()
        .route("/", page)
        .route("/api/rpc", post(rpc))
        .route("/api/events", get(events))
        .route("/api/health", get(|headers: HeaderMap| async move {
            user(&headers).map(|uid| Json(json!({"status":"ok","uid":uid,"version":env!("CARGO_PKG_VERSION")}))).map_err(ApiError::from)
        }))
        .fallback_service(ServeDir::new(&assets).not_found_service(ServeFile::new(assets.join("index.html")))))
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024)).with_state(app)
}

#[tokio::main]
async fn main() -> Result<()> {
    let data = PathBuf::from(std::env::var("TRIM_PKGVAR").context("缺少 TRIM_PKGVAR")?);
    let destination = PathBuf::from(std::env::var("TRIM_APPDEST").context("缺少 TRIM_APPDEST")?);
    let socket = destination.join("app.sock");
    let native = Fnos::new(std::env::var_os("SAG_FNOS_API_SOCKET").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/run/trim_open_gateway_apiscope.socket")))?;
    let remote = Arc::new(remote::Access::open(&data)?);
    let app = App { fnos: native, data, remote, worker_socket: destination.join("worker.sock"), users: Arc::new(Mutex::new(HashMap::new())) };
    let assets = destination.join("www");
    if !assets.join("index.html").is_file() { bail!("缺少前端构建文件 www/index.html"); }
    // The lifecycle script removes only this application's stale socket before start.
    let listener = tokio::net::UnixListener::bind(&socket)?;
    let stop = CancellationToken::new();
    let remote_listener = tokio::net::TcpListener::bind(remote::address()?).await.context("远程 MCP 端口无法监听")?;
    let remote_app = app.clone();
    let remote_stop = stop.clone();
    let remote_task = tokio::spawn(async move {
        axum::serve(remote_listener, remote::router(remote_app, remote_stop.clone()))
            .with_graceful_shutdown(remote_stop.cancelled_owned()).await
    });
    let cli_app = app.clone();
    let cli_stop = stop.clone();
    let cli_task = tokio::spawn(cli::run(cli_app, destination.join("cli.sock"), cli_stop));
    let shutdown_app = app.clone();
    axum::serve(listener, router(app, assets)).with_graceful_shutdown(async move {
        let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("register SIGTERM");
        tokio::select! { _ = terminate.recv() => {}, _ = tokio::signal::ctrl_c() => {} }
        stop.cancel();
        for service in shutdown_app.users.lock().await.values() {
            if let Err(error) = service.finish().await { eprintln!("停止索引失败：{error}"); }
        }
    }).await?;
    cli_task.await??;
    remote_task.await??;
    std::fs::remove_file(socket)?;
    Ok(())
}

#[cfg(test)] mod tests;
