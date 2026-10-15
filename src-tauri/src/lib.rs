mod agent;
mod cli_service;
mod cli_commands;
#[path = "../../cli/src/install.rs"]
mod cli_install;
#[path = "../../cli/src/protocol.rs"]
mod cli_protocol;
mod document_text;
mod explore;
mod indexer;
mod mcp;
mod models;
mod retrieval;
mod sag2_v10;
mod sag_mcp;
mod store;
mod text;
mod types;
use notify::{RecursiveMode, Watcher};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
};
use store::Store;
use indexer::IndexRequest;
use tauri::{Emitter, Manager, State, menu::{Menu, MenuItem}, tray::TrayIconBuilder};
use tokio_util::sync::CancellationToken;
use types::*;

type CommandResult<T> = std::result::Result<T, String>;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
struct AppState {
    store: Arc<Store>,
    explorer: Arc<explore::Explorer>,
    client: reqwest::Client,
    tasks: Mutex<HashMap<String, CancellationToken>>,
    query_lock: Arc<tokio::sync::Mutex<()>>,
    cli: Arc<cli_service::CliService>,
    approvals: Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>,
    scan: mpsc::Sender<IndexRequest>,
    watcher: Mutex<notify::RecommendedWatcher>,
    data_dir: PathBuf,
}
fn emit(app: &tauri::AppHandle, event: &str, payload: impl serde::Serialize + Clone) {
    if let Err(e) = app.emit(event, payload) {
        eprintln!("{event}: {e}")
    }
}
fn show_main_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    let window = app.get_webview_window("main").unwrap();
    window.show()?;
    window.unminimize()?;
    window.set_focus()
}
fn tray_menu(app: &tauri::AppHandle, language: &str) -> tauri::Result<Menu<tauri::Wry>> {
    let english = language == "en";
    let open = MenuItem::with_id(app, "sag-open", if english { "Open SAG" } else { "打开 SAG" }, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "sag-quit", if english { "Quit SAG" } else { "退出 SAG" }, true, None::<&str>)?;
    Menu::with_items(app, &[&open, &quit])
}
#[tauri::command]
fn reveal_main_window(app: tauri::AppHandle) -> CommandResult<()> {
    show_main_window(&app).map_err(error)
}
#[tauri::command]
fn quit_application(app: tauri::AppHandle) {
    app.exit(0);
}
#[tauri::command]
fn list_roots(state: State<Arc<AppState>>) -> CommandResult<Vec<Root>> {
    state.store.roots().map_err(error)
}
#[tauri::command]
async fn list_entries(path: String, state: State<'_, Arc<AppState>>) -> CommandResult<Vec<Entry>> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let settings = state.store.settings().map_err(error)?;
        state
            .store
            .entries(Path::new(&path), settings.show_hidden)
            .map_err(error)
    })
    .await
    .map_err(error)?
}
#[tauri::command]
fn list_files(state: State<Arc<AppState>>) -> CommandResult<Vec<FileInfo>> {
    state.store.files().map_err(error)
}
#[tauri::command]
async fn read_document(path: String, state: State<'_, Arc<AppState>>) -> CommandResult<Document> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.store.document(Path::new(&path)).map_err(error)
    })
    .await
    .map_err(error)?
}
#[tauri::command]
async fn explore_overview(scope: String, query: String, visible: Vec<i64>, state: State<'_, Arc<AppState>>) -> CommandResult<explore::ExploreOverview> {
    let explorer = state.explorer.clone();
    tauri::async_runtime::spawn_blocking(move || explorer.overview(&scope, &query, &visible).map_err(error)).await.map_err(error)?
}
#[tauri::command]
async fn explore_neighbors(scope: String, center_id: i64, snapshot: Option<String>, offset: usize, request_id: String, state: State<'_, Arc<AppState>>) -> CommandResult<explore::ExploreNeighbors> {
    let state = state.inner().clone();
    let cancel = CancellationToken::new();
    state.tasks.lock().unwrap().insert(request_id.clone(), cancel.clone());
    let explorer = state.explorer.clone();
    let result = tauri::async_runtime::spawn_blocking(move || explorer.neighbors(&scope, center_id, snapshot.as_deref(), offset, &cancel).map_err(error)).await;
    state.tasks.lock().unwrap().remove(&request_id);
    result.map_err(error)?
}
#[tauri::command]
async fn explore_evidence(scope: String, snapshot: String, source: i64, target: i64, state: State<'_, Arc<AppState>>) -> CommandResult<explore::ExploreEvidence> {
    let explorer = state.explorer.clone();
    tauri::async_runtime::spawn_blocking(move || explorer.evidence(&scope, &snapshot, source, target).map_err(error)).await.map_err(error)?
}
#[tauri::command]
fn add_root(path: String, state: State<Arc<AppState>>) -> CommandResult<String> {
    let root = state.store.add_root(Path::new(&path)).map_err(error)?;
    state
        .watcher
        .lock()
        .unwrap()
        .watch(Path::new(&root), RecursiveMode::Recursive)
        .map_err(error)?;
    state.scan.send(IndexRequest::Scan { root: root.clone(), verify: false }).map_err(error)?;
    Ok(root)
}
#[tauri::command]
fn remove_root(path: String, state: State<Arc<AppState>>) -> CommandResult<()> {
    state.store.remove_root(&path).map_err(error)?;
    state.cli.revoke_root(&path).map_err(error)
}
#[tauri::command]
fn refresh_root(path: String, state: State<Arc<AppState>>) -> CommandResult<()> {
    let path = state.store.authorize(Path::new(&path)).map_err(error)?;
    let root = state
        .store
        .roots()
        .map_err(error)?
        .into_iter()
        .find(|r| path.starts_with(&r.path))
        .ok_or("目录未加入")?;
    state.scan.send(IndexRequest::Scan { root: root.path, verify: true }).map_err(error)
}
#[tauri::command]
fn reindex_all(state: State<Arc<AppState>>) -> CommandResult<Vec<Root>> {
    for root in state.store.prepare_reindex().map_err(error)? {
        state.scan.send(IndexRequest::Scan { root, verify: true }).map_err(error)?;
    }
    state.store.roots().map_err(error)
}
#[tauri::command]
fn save_document(
    path: String,
    body: String,
    expected_hash: Option<String>,
    state: State<Arc<AppState>>,
) -> CommandResult<Document> {
    let document = state
        .store
        .save_document(Path::new(&path), &body, expected_hash.as_deref())
        .map_err(error)?;
    let root = state
        .store
        .roots()
        .map_err(error)?
        .into_iter()
        .find(|r| Path::new(&document.path).starts_with(&r.path))
        .ok_or("目录未加入")?;
    state.scan.send(IndexRequest::Paths { root: root.path, paths: vec![PathBuf::from(&document.path)] }).map_err(error)?;
    Ok(document)
}
#[tauri::command]
fn put_draft(draft: Draft, state: State<Arc<AppState>>) -> CommandResult<()> {
    state.store.put_draft(&draft).map_err(error)
}
#[tauri::command]
fn list_drafts(state: State<Arc<AppState>>) -> CommandResult<Vec<Draft>> {
    state.store.drafts().map_err(error)
}
#[tauri::command]
fn delete_draft(path: String, state: State<Arc<AppState>>) -> CommandResult<()> {
    state.store.delete_draft(&path).map_err(error)
}
#[tauri::command]
fn get_settings(state: State<Arc<AppState>>) -> CommandResult<Settings> {
    let mut s = state.store.settings().map_err(error)?;
    let mut errors = vec![];
    for (kind, config) in [("answer", &mut s.answer), ("jev", &mut s.jev)] {
        match models::has_secret(&state.data_dir, kind) {
            Ok(present) => config.has_key = present,
            Err(e) => {
                config.has_key = false;
                errors.push(format!("{kind}: {e}"));
            }
        }
    }
    s.credential_error = if errors.is_empty() {
        None
    } else {
        Some(format!("无法读取本地密钥：{}", errors.join("；")))
    };
    for server in &mut s.mcp_servers {
        server.has_credentials =
            models::has_secret(&state.data_dir, &format!("mcp:{}", server.id)).map_err(error)?;
    }
    Ok(s)
}
#[tauri::command]
fn save_settings(
    settings: Settings,
    answer_key: String,
    jev_key: String,
    mcp_credentials: Option<std::collections::BTreeMap<String, McpCredentials>>,
    state: State<Arc<AppState>>,
    app: tauri::AppHandle,
) -> CommandResult<()> {
    let old = state.store.settings().map_err(error)?;
    let mcp_credentials = mcp_credentials.unwrap_or_default();
    for (kind, previous, next, key) in [
        ("answer", &old.answer, &settings.answer, &answer_key),
        ("jev", &old.jev, &settings.jev, &jev_key),
    ] {
        models::check_key_provider(previous, next, key, models::has_secret(&state.data_dir, kind).map_err(error)?).map_err(error)?;
    }
    if mcp_credentials
        .keys()
        .any(|id| !settings.mcp_servers.iter().any(|server| &server.id == id))
    {
        return Err("MCP 服务不存在".into());
    }
    state.store.save_settings(&settings).map_err(error)?;
    if old.language != settings.language {
        app.tray_by_id("main").unwrap().set_menu(Some(tray_menu(&app, &settings.language).map_err(error)?)).map_err(error)?;
    }
    models::save_secret(&state.data_dir, "answer", &answer_key).map_err(error)?;
    models::save_secret(&state.data_dir, "jev", &jev_key).map_err(error)?;
    for (id, credentials) in mcp_credentials {
        models::save_mcp_credentials(&state.data_dir, &id, &credentials).map_err(error)?;
    }
    for removed in old.mcp_servers.iter().filter(|old| {
        !settings
            .mcp_servers
            .iter()
            .any(|server| server.id == old.id)
    }) {
        models::save_mcp_credentials(&state.data_dir, &removed.id, &McpCredentials::default())
            .map_err(error)?;
    }
    if old.retrieval.chunk_chars != settings.retrieval.chunk_chars
        || old.excluded != settings.excluded
    {
        state.store.invalidate_index().map_err(error)?;
        for root in state.store.roots().map_err(error)? {
            state.scan.send(IndexRequest::Scan { root: root.path, verify: false }).map_err(error)?
        }
    } else if old.file_types != settings.file_types || !old.watch && settings.watch {
        for root in state.store.roots().map_err(error)? {
            state.scan.send(IndexRequest::Scan { root: root.path, verify: !old.watch && settings.watch }).map_err(error)?;
        }
    }
    Ok(())
}
#[tauri::command]
fn clear_model_key(kind: String, state: State<Arc<AppState>>) -> CommandResult<()> {
    models::clear_secret(&state.data_dir, &kind).map_err(error)
}
#[tauri::command]
async fn test_mcp(
    server: McpServer,
    credentials: Option<McpCredentials>,
    state: State<'_, Arc<AppState>>,
) -> CommandResult<Vec<McpToolInfo>> {
    let credentials = match credentials {
        Some(value) => value,
        None => models::mcp_credentials(&state.data_dir, &server.id).map_err(error)?,
    };
    mcp::test(
        &server,
        &credentials,
        &state.client,
        state.store.settings().map_err(error)?.timeout_secs,
    )
    .await
    .map_err(error)
}
#[tauri::command]
fn resolve_tool_approval(
    id: String,
    allow: bool,
    state: State<Arc<AppState>>,
) -> CommandResult<()> {
    let sender = state
        .approvals
        .lock()
        .unwrap()
        .remove(&id)
        .ok_or("此工具请求已结束")?;
    sender.send(allow).map_err(|_| "此工具请求已结束".into())
}
async fn request_tool_approval(
    state: &AppState,
    app: &tauri::AppHandle,
    query_id: &str,
    tool: mcp::ExternalTool,
    arguments: serde_json::Value,
    cancel: &CancellationToken,
) -> anyhow::Result<bool> {
    let id = uuid::Uuid::new_v4().to_string();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    state.approvals.lock().unwrap().insert(id.clone(), sender);
    emit(
        app,
        "tool-approval",
        ToolApproval {
            id: id.clone(),
            query_id: query_id.into(),
            server_name: tool.server_name,
            tool_name: tool.name,
            arguments,
        },
    );
    let result = tokio::select! {_=cancel.cancelled()=>Err(anyhow::anyhow!("已停止")),result=receiver=>result.map_err(|_|anyhow::anyhow!("工具确认已关闭"))};
    state.approvals.lock().unwrap().remove(&id);
    result
}
#[tauri::command]
async fn test_model(
    kind: String,
    config: ModelConfig,
    key: String,
    state: State<'_, Arc<AppState>>,
) -> CommandResult<String> {
    if kind != "answer" && kind != "jev" {
        return Err("模型类型不正确".into());
    }
    let key = if key.trim().is_empty() {
        let saved = state.store.settings().map_err(error)?;
        let previous = if kind == "answer" { &saved.answer } else { &saved.jev };
        models::check_key_provider(previous, &config, &key, models::has_secret(&state.data_dir, &kind).map_err(error)?).map_err(error)?;
        models::secret(&state.data_dir, &kind).map_err(error)?
    } else {
        key
    };
    models::test_connection(
        &state.client,
        &kind,
        &config,
        &key,
        state.store.settings().map_err(error)?.timeout_secs,
    )
    .await
    .map_err(error)
}
#[tauri::command]
fn list_conversations(state: State<Arc<AppState>>) -> CommandResult<Vec<Conversation>> {
    state.store.conversations().map_err(error)
}
#[tauri::command]
fn edit_conversation(
    id: String,
    title: Option<String>,
    deleted: Option<bool>,
    state: State<Arc<AppState>>,
) -> CommandResult<()> {
    state
        .store
        .edit_conversation(&id, title.as_deref(), deleted)
        .map_err(error)
}
#[tauri::command]
async fn replace_messages(
    id: String,
    messages: Vec<Message>,
    state: State<'_, Arc<AppState>>,
) -> CommandResult<Conversation> {
    let _query = state.query_lock.lock().await;
    state.store.replace_messages(&id, messages).map_err(error)
}
#[tauri::command]
fn cancel_query(id: String, state: State<Arc<AppState>>) {
    if let Some(token) = state.tasks.lock().unwrap().get(&id) {
        token.cancel()
    }
}
#[derive(serde::Deserialize)]
struct Query {
    id: String,
    kind: String,
    question: String,
    scope: Vec<String>,
    references: Vec<String>,
    model: String,
    conversation_id: Option<String>,
    replace_from: Option<usize>,
    page: Option<usize>,
}
#[derive(serde::Serialize)]
struct QueryOutput {
    terms: Vec<String>,
    elapsed_ms: u64,
    results: Vec<Candidate>,
    trace: Vec<retrieval::Trace>,
    conversation: Option<Conversation>,
    pagination: Option<SearchPagination>,
}
#[tauri::command]
async fn run_query(
    query: Query,
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> CommandResult<QueryOutput> {
    let state = state.inner().clone();
    let cancel = CancellationToken::new();
    {
        let mut tasks = state.tasks.lock().unwrap();
        for token in tasks.values() {
            token.cancel()
        }
        tasks.insert(query.id.clone(), cancel.clone());
    }
    let _query = state.query_lock.lock().await;
    let result = if cancel.is_cancelled() {
        Err(anyhow::anyhow!("已停止"))
    } else {
        execute_query(&query, &state, &app, &cancel).await
    };
    state.tasks.lock().unwrap().remove(&query.id);
    result.map_err(error)
}
async fn execute_query(
    q: &Query,
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    cancel: &CancellationToken,
) -> anyhow::Result<QueryOutput> {
    use anyhow::{bail, Context};
    let started = std::time::Instant::now();
    if q.question.trim().is_empty() {
        bail!("请输入问题或关键词")
    };
    if !matches!(q.kind.as_str(), "ask" | "search" | "deep_search") {
        bail!("查询类型不正确")
    };
    for path in q.scope.iter().chain(q.references.iter()) {
        state.store.authorize(Path::new(path))?;
    }
    if q.kind == "search" {
        let page = q.page.unwrap_or(1);
        if page == 0 { bail!("页码必须大于 0") }
        let (results, pagination) = state.store.search_files_page(&q.question, &q.scope, page, 20)?;
        return Ok(QueryOutput {
            terms: text::search_terms(&q.question),
            results,
            trace: vec![],
            conversation: None,
            elapsed_ms: started.elapsed().as_millis() as u64,
            pagination: Some(pagination),
        });
    }
    let mut settings = state.store.settings()?;
    if !settings.answer.models.contains(&q.model) && q.model != settings.answer.model {
        bail!("请先在设置中添加这个回答模型")
    };
    settings.answer.model = q.model.clone();
    settings.answer.reasoning_effort = settings
        .model_profiles
        .get(&q.model)
        .and_then(|profile| profile.reasoning_effort);
    let mut conversation = if q.kind == "ask" {
        Some(match &q.conversation_id {
            Some(id) => state
                .store
                .conversations()?
                .into_iter()
                .find(|c| &c.id == id)
                .context("对话不存在")?,
            None => Conversation {
                id: q.id.clone(),
                title: q.question.chars().take(30).collect(),
                messages: vec![],
                updated: store::now(),
                deleted: false,
            },
        })
    } else {
        None
    };
    if let Some(index) = q.replace_from {
        let c = conversation.as_mut().context("仅问答支持重新生成")?;
        if index >= c.messages.len() {
            bail!("要重新生成的消息不存在")
        }
        c.messages.truncate(index);
    }
    let history = conversation
        .as_ref()
        .map(|c| c.messages.clone())
        .unwrap_or_default();
    let retrieval_question = q.question.clone();
    if let Some(c) = &mut conversation {
        c.messages.push(Message {
            question: q.question.clone(),
            answer: String::new(),
            reasoning: String::new(),
            evidence: vec![],
            model: q.model.clone(),
            scope: q.scope.clone(),
            references: q.references.clone(),
            status: "running".into(),
            error: None,
            retrieval_question: retrieval_question.clone(),
            tool_calls: vec![],
            context: None,
        });
        state.store.put_conversation(c)?;
        emit(app, "history-changed", ());
    }
    let result = async {
        let notify =
            |stage: &str, delta: Option<&str>, count, tool: Option<(&ToolCallRecord, bool)>| {
                emit(
                    app,
                    "query-progress",
                    Progress {
                        replace: false,
                        id: q.id.clone(),
                        stage: stage.into(),
                        delta: delta.map(String::from),
                        reasoning_delta: None,
                        count,
                        tool: tool.map(|(call, finished)| ToolProgress {
                            call: call.clone(),
                            finished,
                        }),
                    },
                );
            };
        if let Some(c) = &mut conversation {
            let answer_key = models::secret(&state.data_dir, "answer")?;
            let profile = settings
                .model_profiles
                .get(&q.model)
                .cloned()
                .unwrap_or_default();
            let servers = profile
                .mcp_servers
                .iter()
                .map(|id| {
                    let server = settings
                        .mcp_servers
                        .iter()
                        .find(|server| &server.id == id)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("绑定的 MCP 服务不存在"))?;
                    let credentials = models::mcp_credentials(&state.data_dir, id)?;
                    Ok((server, credentials))
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            if !servers.is_empty() {
                notify("思考中", None, 0, None);
            }
            let mut mcp = mcp::McpSession::open(
                servers,
                &state.client,
                settings.timeout_secs,
                cancel,
                &profile.mcp_tools,
            )
            .await?;
            if profile.sag2_enabled {
                let enabled = profile.mcp_tools.get(sag_mcp::ID).cloned()
                    .unwrap_or_else(|| sag_mcp::NAMES.iter().map(|name| (*name).into()).collect());
                if !enabled.is_empty() {
                    let app = app.clone();
                    let query_id = q.id.clone();
                    mcp.attach_sag(sag_mcp::SagTools {
                        store: state.store.clone(), client: state.client.clone(),
                        data_dir: state.data_dir.clone(), settings: settings.clone(),
                        scope: q.scope.clone(), references: q.references.clone(), enabled,
                        cancel: cancel.clone(),
                        progress: Arc::new(move |stage, count| emit(&app, "query-progress", Progress {
                            id: query_id.clone(), stage: stage.into(), delta: None,
                            reasoning_delta: None, replace: false, count, tool: None,
                        })),
                    }).await?;
                }
            }
            let message = c.messages.last_mut().unwrap();
            let trace = agent::answer(
                &state.client,
                &settings.answer,
                &answer_key,
                message,
                &history,
                settings.answer_context_tokens,
                settings
                    .answer_output_tokens
                    .min(settings.answer_context_tokens / 4),
                settings.timeout_secs,
                cancel,
                agent::AgentOptions {
                    max_tool_calls: settings.max_tool_calls,
                    sag2_enabled: false,
                    system_prompt: &profile.system_prompt,
                    mcp: Some(&mcp),
                },
                |_| async { anyhow::bail!("本地工具应通过内置 SAG MCP 调用") },
                |tool, arguments| request_tool_approval(state, app, &q.id, tool, arguments, cancel),
                |stage, delta, count, tool| match delta {
                    Some(agent::AnswerDelta::Replace(text)) => emit(
                        app,
                        "query-progress",
                        Progress {
                            id: q.id.clone(),
                            stage: stage.into(),
                            delta: Some(text.into()),
                            reasoning_delta: None,
                            replace: true,
                            count,
                            tool: None,
                        },
                    ),
                    Some(agent::AnswerDelta::Reasoning(text)) => emit(
                        app,
                        "query-progress",
                        Progress {
                            id: q.id.clone(),
                            stage: stage.into(),
                            delta: None,
                            reasoning_delta: Some(text.into()),
                            replace: false,
                            count,
                            tool: None,
                        },
                    ),
                    other => notify(
                        stage,
                        other.map(|d| match d {
                            agent::AnswerDelta::Append(text) => text,
                            agent::AnswerDelta::Replace(_) => unreachable!(),
                            agent::AnswerDelta::Reasoning(_) => unreachable!(),
                        }),
                        count,
                        tool,
                    ),
                },
            )
            .await?;
            message.status = "complete".into();
            message.context.as_mut().unwrap().elapsed_ms =
                Some(started.elapsed().as_millis() as u64);
            let results = message.evidence.clone();
            c.updated = store::now();
            state.store.put_conversation(c)?;
            emit(app, "history-changed", ());
            Ok(retrieval::SearchOutput { results, trace })
        } else {
            let jev_key = models::secret(&state.data_dir, "jev")?;
            retrieval::search(
                &state.store,
                &q.question,
                &q.scope,
                &settings.retrieval,
                cancel,
                |candidates, evidence| {
                    let key = &jev_key;
                    let settings = &settings;
                    async move {
                        models::score(
                            &state.client,
                            &settings.jev,
                            key,
                            &q.question,
                            candidates,
                            &evidence,
                            settings.timeout_secs,
                            cancel,
                        )
                        .await
                    }
                },
                |stage, count| notify(stage, None, count, None),
            )
            .await
        }
    }
    .await;
    match result {
        Ok(output) => Ok(QueryOutput {
            terms: text::search_terms(&q.question),
            elapsed_ms: started.elapsed().as_millis() as u64,
            results: output.results,
            trace: output.trace,
            conversation,
            pagination: None,
        }),
        Err(e) => {
            if let Some(c) = &mut conversation {
                let m = c.messages.last_mut().unwrap();
                m.status = if cancel.is_cancelled() {
                    "stopped"
                } else {
                    "failed"
                }
                .into();
                m.error = Some(e.to_string());
                if let Some(context) = &mut m.context {
                    context.elapsed_ms = Some(started.elapsed().as_millis() as u64);
                }
                c.updated = store::now();
                state.store.put_conversation(c)?;
                emit(app, "history-changed", ());
            }
            Err(e)
        }
    }
}
#[tauri::command]
fn reveal_path(path: String, state: State<Arc<AppState>>) -> CommandResult<()> {
    let path = state.store.authorize(Path::new(&path)).map_err(error)?;
    reveal(&path).map_err(error)
}
fn reveal(path: &Path) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let mut c = std::process::Command::new("open");
        if path.is_file() {
            c.arg("-R");
        }
        if !c.arg(path).status()?.success() {
            anyhow::bail!("Finder 未能打开此位置")
        }
    }
    #[cfg(target_os = "windows")]
    {
        let mut c = std::process::Command::new("explorer.exe");
        if path.is_file() {
            c.arg(format!("/select,{}", path.display()));
        } else {
            c.arg(path);
        }
        c.spawn()?;
    }
    Ok(())
}
#[tauri::command]
fn open_data_directory(state: State<Arc<AppState>>) -> CommandResult<()> {
    reveal(&state.data_dir).map_err(error)
}
#[tauri::command]
fn load_image(path: String, state: State<Arc<AppState>>) -> CommandResult<String> {
    use base64::Engine;
    let p = state.store.authorize(Path::new(&path)).map_err(error)?;
    let ext = p
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return Err("暂不支持此图片格式".into()),
    };
    let bytes = std::fs::read(p).map_err(error)?;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("图片超过 20 MiB".into());
    }
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data = std::env::var_os("SAG_DESKTOP_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or(app.path().app_data_dir()?);
            std::fs::create_dir_all(&data)?;
            let instance = cli_service::lock_instance(&data)?;
            let store = Arc::new(Store::open(&data.join("sag.sqlite"))?);
            let icon = app.default_window_icon().unwrap().clone();
            #[cfg(target_os = "macos")]
            let icon = {
                // macOS template icons use the logo silhouette, without its white background.
                let mut rgba = icon.rgba().to_vec();
                for pixel in rgba.chunks_exact_mut(4) {
                    pixel[3] = (u16::from(pixel[3]) * (255 - u16::from(pixel[0])) / 255) as u8;
                    pixel[..3].fill(0);
                }
                tauri::image::Image::new_owned(rgba, icon.width(), icon.height())
            };
            TrayIconBuilder::with_id("main")
                .icon(icon)
                .icon_as_template(cfg!(target_os = "macos"))
                .tooltip("SAG")
                .menu(&tray_menu(app.handle(), &store.settings()?.language)?)
                .show_menu_on_left_click(!cfg!(target_os = "windows"))
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "sag-open" => if let Err(e) = show_main_window(app) { emit(app, "app-error", e.to_string()); },
                    "sag-quit" => emit(app, "quit-requested", ()),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    #[cfg(target_os = "windows")]
                    if matches!(event, tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, button_state: tauri::tray::MouseButtonState::Up, .. }) {
                        if let Err(e) = show_main_window(tray.app_handle()) { emit(tray.app_handle(), "app-error", e.to_string()); }
                    }
                    #[cfg(not(target_os = "windows"))]
                    let _ = (tray, event);
                })
                .build(app)?;
            let (tx, rx) = mpsc::channel::<IndexRequest>();
            let watcher_store = store.clone();
            let watcher_tx = tx.clone();
            let watcher_app = app.handle().clone();
            let mut watcher = notify::recommended_watcher(
                move |event: notify::Result<notify::Event>| match event {
                    Ok(e) => {
                        if matches!(e.kind, notify::EventKind::Access(_)) {
                            return;
                        }
                        let settings = match watcher_store.settings() {
                            Ok(s) if s.watch => s,
                            Ok(_) => return,
                            Err(err) => {
                                emit(&watcher_app, "app-error", err.to_string());
                                return;
                            }
                        };
                        let roots = match watcher_store.roots() {
                            Ok(r) => r,
                            Err(err) => {
                                emit(&watcher_app, "app-error", err.to_string());
                                return;
                            }
                        };
                        for request in indexer::event_requests(&e, &roots, &settings) {
                            if let Err(err) = watcher_tx.send(request) {
                                emit(&watcher_app, "app-error", err.to_string());
                            }
                        }
                    }
                    Err(err) => emit(&watcher_app, "app-error", format!("文件监听失败：{err}")),
                },
            )?;
            for root in store.roots()? {
                if Path::new(&root.path).exists() {
                    watcher.watch(Path::new(&root.path), RecursiveMode::Recursive)?;
                }
                tx.send(IndexRequest::Scan { root: root.path, verify: false })?;
            }
            let worker_store = store.clone();
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                while let Ok(first) = rx.recv() {
                    for request in indexer::receive_batch(&rx, first) {
                        let root = match &request {
                            IndexRequest::Scan { root, .. } | IndexRequest::Paths { root, .. } => root.clone(),
                        };
                        if let Err(e) = indexer::apply_request(&worker_store, request, |progress| {
                            emit(
                                &handle,
                                "index-progress",
                                serde_json::json!({"path":root,"count":progress.count,"total":progress.total}),
                            )
                        }) {
                            emit(&handle, "app-error", format!("{}：{}", root, e));
                        }
                        emit(&handle, "data-changed", ());
                    }
                }
            });
            let client = reqwest::Client::builder().build()?;
            let query_lock = Arc::new(tokio::sync::Mutex::new(()));
            let cli = cli_service::CliService::start(store.clone(), client.clone(), data.clone(), query_lock.clone(), instance)?;
            app.manage(Arc::new(AppState {
                explorer: Arc::new(explore::Explorer::new(store.clone())),
                store,
                client,
                tasks: Mutex::new(HashMap::new()),
                query_lock,
                cli,
                approvals: Mutex::new(HashMap::new()),
                scan: tx,
                watcher: Mutex::new(watcher),
                data_dir: data,
            }));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Err(e) = window.hide() { emit(window.app_handle(), "app-error", e.to_string()); }
            }
        })
        .invoke_handler(tauri::generate_handler![
            cli_commands::get_cli_settings,
            cli_commands::save_cli_settings,
            cli_commands::install_cli_integration,
            cli_commands::test_cli_connection,
            reveal_main_window,
            quit_application,
            list_roots,
            list_entries,
            list_files,
            read_document,
            explore_overview,
            explore_neighbors,
            explore_evidence,
            add_root,
            remove_root,
            refresh_root,
            reindex_all,
            save_document,
            put_draft,
            list_drafts,
            delete_draft,
            get_settings,
            save_settings,
            test_model,
            test_mcp,
            resolve_tool_approval,
            clear_model_key,
            list_conversations,
            edit_conversation,
            replace_messages,
            cancel_query,
            run_query,
            reveal_path,
            open_data_directory,
            load_image
        ])
        .build(tauri::generate_context!())
        .expect("无法启动 SAG 桌面客户端");
    app.run(|app, event| match event {
        tauri::RunEvent::Exit => app.state::<Arc<AppState>>().cli.shutdown(),
        tauri::RunEvent::ExitRequested { code: None, api, .. } => {
            api.prevent_exit();
            emit(app, "quit-requested", ());
        }
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. } => {
            if let Err(e) = show_main_window(app) { emit(app, "app-error", e.to_string()); }
        }
        _ => {}
    });
}
