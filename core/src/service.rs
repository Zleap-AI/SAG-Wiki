use crate::indexer::IndexRequest;
use crate::store::Store;
use crate::{
    agent, cli_service, explore, indexer, mcp, models, retrieval, sag_mcp, store, text, types::*,
};
use notify::{RecursiveMode, Watcher};
use sag_protocol::{EventEnvelope, Query, QueryOutput, ServiceEvent};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
type CommandResult<T> = Result<T, String>;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Clone)]
pub struct EventSink {
    pub owner: String,
    pub request_id: Option<String>,
    pub cancel: CancellationToken,
    pub bus: broadcast::Sender<(Option<String>, EventEnvelope)>,
}
fn emit(app: &EventSink, event: &str, payload: impl serde::Serialize + Clone) {
    let value = serde_json::to_value(payload).unwrap();
    let event = match event {
        "data-changed" => ServiceEvent::DataChanged,
        "settings-changed" => ServiceEvent::SettingsChanged,
        "history-changed" => ServiceEvent::HistoryChanged,
        "index-progress" => ServiceEvent::IndexProgress(serde_json::from_value(value).unwrap()),
        "app-error" => ServiceEvent::AppError(serde_json::from_value(value).unwrap()),
        "query-progress" => ServiceEvent::QueryProgress(serde_json::from_value(value).unwrap()),
        "tool-approval" => ServiceEvent::ToolApproval(serde_json::from_value(value).unwrap()),
        _ => unreachable!("unknown service event"),
    };
    let owner = matches!(
        event,
        ServiceEvent::QueryProgress(_) | ServiceEvent::ToolApproval(_)
    )
    .then(|| app.owner.clone());
    let _ = app.bus.send((
        owner,
        EventEnvelope {
            request_id: app.request_id.clone(),
            event,
        },
    ));
}
pub struct Service {
    pub store: Arc<Store>,
    explorer: Arc<explore::Explorer>,
    client: reqwest::Client,
    tasks: Mutex<HashMap<(String, String), CancellationToken>>,
    sessions: Mutex<HashMap<String, std::sync::Weak<tokio::sync::Mutex<()>>>>,
    conversation_access: tokio::sync::RwLock<()>,
    approvals: Mutex<HashMap<(String, String), tokio::sync::oneshot::Sender<bool>>>,
    scan: mpsc::Sender<IndexRequest>,
    watcher: Arc<Mutex<notify::RecommendedWatcher>>,
    pub data_dir: PathBuf,
    pub events: broadcast::Sender<(Option<String>, EventEnvelope)>,
    pub stop: CancellationToken,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    pub cli: Arc<cli_service::CliService>,
}
impl Service {
    fn session_lock(&self, id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = sessions.get(id).and_then(std::sync::Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        sessions.insert(id.into(), Arc::downgrade(&lock));
        lock
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
fn list_roots(state: &Arc<Service>) -> CommandResult<Vec<Root>> {
    state.store.roots().map_err(error)
}
async fn list_entries(path: String, state: &Arc<Service>) -> CommandResult<Vec<Entry>> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let settings = state.store.settings().map_err(error)?;
        state
            .store
            .entries(Path::new(&path), settings.show_hidden)
            .map_err(error)
    })
    .await
    .map_err(error)?
}
fn list_files(state: &Arc<Service>) -> CommandResult<Vec<FileInfo>> {
    state.store.files().map_err(error)
}
async fn read_document(path: String, state: &Arc<Service>) -> CommandResult<Document> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || state.store.document(Path::new(&path)).map_err(error))
        .await
        .map_err(error)?
}
async fn explore_overview(
    scope: String,
    query: String,
    visible: Vec<i64>,
    state: &Arc<Service>,
) -> CommandResult<explore::ExploreOverview> {
    let explorer = state.explorer.clone();
    tokio::task::spawn_blocking(move || explorer.overview(&scope, &query, &visible).map_err(error))
        .await
        .map_err(error)?
}
async fn explore_graph(
    scope: String,
    request_id: String,
    state: &Arc<Service>,
    app: &EventSink,
) -> CommandResult<explore::ExploreGraph> {
    let state = state.clone();
    let cancel = app.cancel.child_token();
    state
        .tasks
        .lock()
        .unwrap()
        .insert((app.owner.clone(), request_id.clone()), cancel.clone());
    let explorer = state.explorer.clone();
    let result =
        tokio::task::spawn_blocking(move || explorer.graph(&scope, &cancel).map_err(error)).await;
    state
        .tasks
        .lock()
        .unwrap()
        .remove(&(app.owner.clone(), request_id));
    result.map_err(error)?
}
async fn explore_neighbors(
    scope: String,
    center_id: i64,
    snapshot: Option<String>,
    offset: usize,
    request_id: String,
    state: &Arc<Service>,
    app: &EventSink,
) -> CommandResult<explore::ExploreNeighbors> {
    let state = state.clone();
    let cancel = app.cancel.child_token();
    state
        .tasks
        .lock()
        .unwrap()
        .insert((app.owner.clone(), request_id.clone()), cancel.clone());
    let explorer = state.explorer.clone();
    let result = tokio::task::spawn_blocking(move || {
        explorer
            .neighbors(&scope, center_id, snapshot.as_deref(), offset, &cancel)
            .map_err(error)
    })
    .await;
    state
        .tasks
        .lock()
        .unwrap()
        .remove(&(app.owner.clone(), request_id));
    result.map_err(error)?
}
async fn explore_evidence(
    scope: String,
    snapshot: String,
    source: i64,
    target: i64,
    state: &Arc<Service>,
) -> CommandResult<explore::ExploreEvidence> {
    let explorer = state.explorer.clone();
    tokio::task::spawn_blocking(move || {
        explorer
            .evidence(&scope, &snapshot, source, target)
            .map_err(error)
    })
    .await
    .map_err(error)?
}
fn add_root(path: String, state: &Arc<Service>) -> CommandResult<String> {
    indexer::add_root(&state.store, &state.watcher, &state.scan, Path::new(&path)).map_err(error)
}
fn list_removed_directories(state: &Arc<Service>) -> CommandResult<Vec<String>> {
    state.store.excluded_directories().map_err(error)
}
fn remove_root(path: String, state: &Arc<Service>) -> CommandResult<()> {
    state.store.remove_path(Path::new(&path)).map_err(error)?;
    Ok(())
}
fn refresh_root(path: String, state: &Arc<Service>) -> CommandResult<()> {
    let path = state.store.authorize(Path::new(&path)).map_err(error)?;
    let root = state
        .store
        .roots()
        .map_err(error)?
        .into_iter()
        .find(|r| path.starts_with(&r.path))
        .ok_or("目录未加入")?;
    state
        .scan
        .send(IndexRequest::Scan {
            root: root.path,
            verify: true,
        })
        .map_err(error)
}
fn reindex_all(state: &Arc<Service>) -> CommandResult<Vec<Root>> {
    for root in state.store.prepare_reindex().map_err(error)? {
        state
            .scan
            .send(IndexRequest::Scan { root, verify: true })
            .map_err(error)?;
    }
    state.store.roots().map_err(error)
}
fn save_document(
    path: String,
    body: String,
    expected_hash: Option<String>,
    state: &Arc<Service>,
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
    state
        .scan
        .send(IndexRequest::Paths {
            root: root.path,
            paths: vec![PathBuf::from(&document.path)],
        })
        .map_err(error)?;
    Ok(document)
}
fn put_draft(draft: Draft, state: &Arc<Service>) -> CommandResult<()> {
    state.store.put_draft(&draft).map_err(error)
}
fn list_drafts(state: &Arc<Service>) -> CommandResult<Vec<Draft>> {
    state.store.drafts().map_err(error)
}
fn delete_draft(path: String, state: &Arc<Service>) -> CommandResult<()> {
    state.store.delete_draft(&path).map_err(error)
}
fn get_settings(state: &Arc<Service>) -> CommandResult<Settings> {
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
fn save_settings(
    settings: Settings,
    answer_key: String,
    jev_key: String,
    mcp_credentials: Option<std::collections::BTreeMap<String, McpCredentials>>,
    state: &Arc<Service>,
    _app: &EventSink,
) -> CommandResult<()> {
    let old = state.store.settings().map_err(error)?;
    Store::validate_settings(&settings).map_err(error)?;
    if settings.convert_other_formats
        && (!old.convert_other_formats || old.conversion_formats != settings.conversion_formats)
    {
        state.store.docling.require_formats(&settings.conversion_formats).map_err(error)?;
    }
    let mcp_credentials = mcp_credentials.unwrap_or_default();
    for (kind, previous, next, key) in [
        ("answer", &old.answer, &settings.answer, &answer_key),
        ("jev", &old.jev, &settings.jev, &jev_key),
    ] {
        if kind == "jev" && next.auth_mode == ModelAuthMode::None { continue; }
        models::check_key_provider(
            previous,
            next,
            key,
            models::has_secret(&state.data_dir, kind).map_err(error)?,
        )
        .map_err(error)?;
    }
    if mcp_credentials
        .keys()
        .any(|id| !settings.mcp_servers.iter().any(|server| &server.id == id))
    {
        return Err("MCP 服务不存在".into());
    }
    // Never persist a new Jev address while its previous provider's key is active.
    if settings.jev.auth_mode == ModelAuthMode::None
        || reqwest::Url::parse(&old.jev.url).map_err(error)?.origin()
            != reqwest::Url::parse(&settings.jev.url).map_err(error)?.origin()
    {
        models::clear_secret(&state.data_dir, "jev").map_err(error)?;
    }
    state.store.save_settings(&settings).map_err(error)?;
    models::save_secret(&state.data_dir, "answer", &answer_key).map_err(error)?;
    if settings.jev.auth_mode == ModelAuthMode::Bearer {
        models::save_secret(&state.data_dir, "jev", &jev_key).map_err(error)?;
    }
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
            state
                .scan
                .send(IndexRequest::Scan {
                    root: root.path,
                    verify: false,
                })
                .map_err(error)?
        }
    } else if old.file_types != settings.file_types || old.convert_other_formats != settings.convert_other_formats || old.conversion_formats != settings.conversion_formats || !old.watch && settings.watch {
        for root in state.store.roots().map_err(error)? {
            state
                .scan
                .send(IndexRequest::Scan {
                    root: root.path,
                    verify: !old.watch && settings.watch,
                })
                .map_err(error)?;
        }
    }
    Ok(())
}
fn clear_model_key(kind: String, state: &Arc<Service>) -> CommandResult<()> {
    models::clear_secret(&state.data_dir, &kind).map_err(error)
}
async fn test_mcp(
    server: McpServer,
    credentials: Option<McpCredentials>,
    state: &Arc<Service>,
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
fn resolve_tool_approval(
    id: String,
    allow: bool,
    state: &Arc<Service>,
    app: &EventSink,
) -> CommandResult<()> {
    let sender = state
        .approvals
        .lock()
        .unwrap()
        .remove(&(app.owner.clone(), id))
        .ok_or("此工具请求已结束")?;
    sender.send(allow).map_err(|_| "此工具请求已结束".into())
}
async fn request_tool_approval(
    state: &Service,
    app: &EventSink,
    query_id: &str,
    tool: mcp::ExternalTool,
    arguments: serde_json::Value,
    cancel: &CancellationToken,
) -> anyhow::Result<bool> {
    let id = uuid::Uuid::new_v4().to_string();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    state
        .approvals
        .lock()
        .unwrap()
        .insert((app.owner.clone(), id.clone()), sender);
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
    state
        .approvals
        .lock()
        .unwrap()
        .remove(&(app.owner.clone(), id));
    result
}
async fn test_model(
    kind: String,
    config: ModelConfig,
    key: String,
    state: &Arc<Service>,
) -> CommandResult<String> {
    if kind != "answer" && kind != "jev" {
        return Err("模型类型不正确".into());
    }
    if kind == "answer" && config.auth_mode != ModelAuthMode::Bearer {
        return Err("问答模型需要 API Key 鉴权；无需鉴权选项仅适用于 Jev".into());
    }
    let key = if kind == "jev" && config.auth_mode == ModelAuthMode::None {
        String::new()
    } else if key.trim().is_empty() {
        let saved = state.store.settings().map_err(error)?;
        let previous = if kind == "answer" {
            &saved.answer
        } else {
            &saved.jev
        };
        models::check_key_provider(
            previous,
            &config,
            &key,
            models::has_secret(&state.data_dir, &kind).map_err(error)?,
        )
        .map_err(error)?;
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
fn list_conversations(state: &Arc<Service>) -> CommandResult<Vec<Conversation>> {
    state.store.conversations().map_err(error)
}
async fn clear_conversations(state: &Arc<Service>) -> CommandResult<()> {
    let _access = state.conversation_access.try_write()
        .map_err(|_| "请先停止正在进行的问答，再清空所有对话".to_string())?;
    let state = state.clone();
    tokio::task::spawn_blocking(move || state.store.clear_conversations().map_err(error))
        .await.map_err(error)?
}
fn edit_conversation(
    id: String,
    title: Option<String>,
    deleted: Option<bool>,
    state: &Arc<Service>,
) -> CommandResult<()> {
    state
        .store
        .edit_conversation(&id, title.as_deref(), deleted)
        .map_err(error)
}
async fn replace_messages(
    id: String,
    messages: Vec<Message>,
    state: &Arc<Service>,
    app: &EventSink,
) -> CommandResult<Conversation> {
    let _access = state.conversation_access.read().await;
    let lock = state.session_lock(&id);
    let _query = tokio::select! {
        _ = app.cancel.cancelled() => return Err("已停止".into()),
        guard = lock.lock() => guard,
    };
    state.store.replace_messages(&id, messages).map_err(error)
}
fn cancel_query(id: String, state: &Arc<Service>, app: &EventSink) {
    if let Some(token) = state.tasks.lock().unwrap().get(&(app.owner.clone(), id)) {
        token.cancel()
    }
}
async fn summarize_search(
    id: String,
    question: String,
    results: Vec<Candidate>,
    state: &Arc<Service>,
    app: &EventSink,
) -> CommandResult<sag_protocol::SearchSummary> {
    let cancel = app.cancel.child_token();
    state.tasks.lock().unwrap().insert((app.owner.clone(), id.clone()), cancel.clone());
    let result = async {
        let settings = state.store.settings()?;
        anyhow::ensure!(settings.search_summary_enabled, "自动总结已关闭");
        anyhow::ensure!(!question.trim().is_empty(), "请输入问题或关键词");
        anyhow::ensure!(!results.is_empty(), "总结需要搜索结果");
        let results = &results[..results.len().min(settings.search_summary_sources)];
        for candidate in results {
            state.store.authorize(Path::new(&candidate.path))?;
        }
        let mut config = settings.answer.clone();
        config.reasoning_effort = settings.model_profiles.get(&config.model)
            .and_then(|profile| profile.reasoning_effort);
        let key = models::secret(&state.data_dir, "answer")?;
        let plan = models::prepare_search_summary(
            &config, &question, results, settings.answer_context_tokens,
            settings.answer_output_tokens.min(settings.answer_context_tokens / 4),
        )?;
        let answer = models::answer(&state.client, &config, &key, &plan,
            settings.timeout_secs, &cancel, |delta| {
                emit(app, "query-progress", Progress {
                    id: id.clone(), stage: "正在总结搜索结果".into(),
                    delta: Some(delta.into()), reasoning_delta: None, reasoning_block: None,
                    replace: false, count: plan.evidence.len(), tool: None,
                });
            }).await?;
        Ok(sag_protocol::SearchSummary { text: answer.text, evidence: plan.evidence })
    }.await;
    state.tasks.lock().unwrap().remove(&(app.owner.clone(), id));
    result.map_err(|e: anyhow::Error| e.to_string())
}
async fn run_query(
    query: Query,
    state: &Arc<Service>,
    app: &EventSink,
) -> CommandResult<QueryOutput> {
    let _access = if query.kind == "ask" { Some(state.conversation_access.read().await) } else { None };
    let state = state.clone();
    let cancel = app.cancel.child_token();
    state
        .tasks
        .lock()
        .unwrap()
        .insert((app.owner.clone(), query.id.clone()), cancel.clone());
    let session = query.conversation_id.as_deref().unwrap_or(&query.id);
    let lock = state.session_lock(session);
    let result = async {
        let _query = tokio::select! {
            _ = cancel.cancelled() => return Err(anyhow::anyhow!("已停止")),
            guard = lock.lock() => guard,
        };
        execute_query(&query, &state, &app, &cancel).await
    }
    .await;
    state
        .tasks
        .lock()
        .unwrap()
        .remove(&(app.owner.clone(), query.id));
    result.map_err(error)
}
async fn execute_query(
    q: &Query,
    state: &Arc<Service>,
    app: &EventSink,
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
        if page == 0 {
            bail!("页码必须大于 0")
        }
        let (results, pagination) =
            state
                .store
                .search_files_page(&q.question, &q.scope, page, 20)?;
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
        bail!("请先在设置中添加这个问答模型")
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
            created_at: Some(store::now()),
            question: q.question.clone(),
            answer: String::new(),
            reasoning: String::new(),
            reasoning_blocks: vec![],
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
                        reasoning_block: None,
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
                let enabled = profile
                    .mcp_tools
                    .get(sag_mcp::ID)
                    .cloned()
                    .unwrap_or_else(|| sag_mcp::NAMES.iter().map(|name| (*name).into()).collect());
                if !enabled.is_empty() {
                    let app = (*app).clone();
                    let query_id = q.id.clone();
                    mcp.attach_sag(sag_mcp::SagTools {
                        store: state.store.clone(),
                        client: state.client.clone(),
                        data_dir: state.data_dir.clone(),
                        settings: settings.clone(),
                        scope: q.scope.clone(),
                        references: q.references.clone(),
                        enabled,
                        cancel: cancel.clone(),
                        progress: Arc::new(move |stage, count| {
                            emit(
                                &app,
                                "query-progress",
                                Progress {
                                    id: query_id.clone(),
                                    stage: stage.into(),
                                    delta: None,
                                    reasoning_delta: None,
                        reasoning_block: None,
                                    replace: false,
                                    count,
                                    tool: None,
                                },
                            )
                        }),
                    })
                    .await?;
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
                        reasoning_block: None,
                            replace: true,
                            count,
                            tool: None,
                        },
                    ),
                    Some(agent::AnswerDelta::Reasoning(text, block)) => emit(
                        app,
                        "query-progress",
                        Progress {
                            id: q.id.clone(),
                            stage: stage.into(),
                            delta: None,
                            reasoning_delta: Some(text.into()),
                            reasoning_block: Some(block.clone()),
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
                            agent::AnswerDelta::Reasoning(_, _) => unreachable!(),
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
            let jev_key = models::jev_secret(&state.data_dir, &settings.jev)?;
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
fn load_image(path: String, state: &Arc<Service>) -> CommandResult<String> {
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

impl Service {
    pub fn open(data: PathBuf) -> anyhow::Result<Arc<Self>> {
        std::fs::create_dir_all(&data)?;
        let store = Arc::new(Store::open(&data.join("sag.sqlite"))?);
        let stop = CancellationToken::new();
        let (events, _) = broadcast::channel(1024);
        let event_sink = EventSink {
            owner: String::new(),
            request_id: None,
            cancel: stop.clone(),
            bus: events.clone(),
        };
        let (tx, rx) = mpsc::channel::<IndexRequest>();
        let watcher_store = store.clone();
        let watcher_tx = tx.clone();
        let watcher_app = event_sink.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
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
            })?;
        for root in store.roots()? {
            if Path::new(&root.path).exists() {
                watcher.watch(Path::new(&root.path), RecursiveMode::Recursive)?;
            }
            tx.send(IndexRequest::Scan {
                root: root.path,
                verify: false,
            })?;
        }
        let worker_store = store.clone();
        let handle = event_sink.clone();
        let worker_stop = stop.clone();
        let worker = std::thread::spawn(move || loop {
            if worker_stop.is_cancelled() {
                break;
            }
            let first = match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(first) => first,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            for request in indexer::receive_batch(&rx, first) {
                let root = match &request {
                    IndexRequest::Scan { root, .. } | IndexRequest::Paths { root, .. } => {
                        root.clone()
                    }
                };
                let last_update = std::cell::Cell::new(std::time::Instant::now());
                if let Err(e) = indexer::apply_request(&worker_store, request, |progress| {
                    emit(
                        &handle,
                        "index-progress",
                        serde_json::json!({"path":root,"count":progress.count,"total":progress.total,"stage":worker_store.indexing.task(&root).stage}),
                    );
                    if progress.count > 0 && last_update.get().elapsed() >= std::time::Duration::from_secs(1) {
                        last_update.set(std::time::Instant::now());
                        emit(&handle, "data-changed", ());
                    }
                }) {
                    emit(&handle, "app-error", format!("{}：{}", root, e));
                }
                emit(&handle, "data-changed", ());
            }
        });
        let client = reqwest::Client::builder().build()?;

        let watcher = Arc::new(Mutex::new(watcher));
        let add_store = store.clone();
        let add_watcher = watcher.clone();
        let add_tx = tx.clone();
        let add_events = event_sink.clone();
        let add_root = Arc::new(move |path: &Path| {
            let root = indexer::add_root(&add_store, &add_watcher, &add_tx, path)?;
            emit(&add_events, "data-changed", ());
            Ok(root)
        });
        let remove_store = store.clone();
        let remove_events = event_sink.clone();
        let remove_root = Arc::new(move |path: &Path| {
            remove_store.remove_path(path)?;
            emit(&remove_events, "data-changed", ());
            Ok(())
        });
        let cli = cli_service::CliService::shared(
            store.clone(),
            client.clone(),
            data.clone(),
            add_root,
            remove_root,
        )?;
        Ok(Arc::new(Self {
            explorer: Arc::new(explore::Explorer::new(store.clone())),
            store,
            client,
            tasks: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            conversation_access: tokio::sync::RwLock::new(()),
            approvals: Mutex::new(HashMap::new()),
            scan: tx,
            watcher,
            data_dir: data,
            events,
            stop,
            cli,
            worker: Mutex::new(Some(worker)),
        }))
    }
    pub async fn finish(&self) -> anyhow::Result<()> {
        self.stop.cancel();
        self.store.indexing.stop();
        self.store.docling.cancel();
        let worker = self.worker.lock().unwrap().take();
        if let Some(worker) = worker {
            tokio::task::spawn_blocking(move || {
                worker
                    .join()
                    .map_err(|_| anyhow::anyhow!("索引线程退出失败"))
            })
            .await??;
        }
        Ok(())
    }
    pub fn info(&self) -> anyhow::Result<sag_protocol::ServiceInfo> {
        Ok(sag_protocol::ServiceInfo {
            version: Some(sag_protocol::BUILD_VERSION.into()),
            protocol_version: sag_protocol::VERSION,
            build_id: sag_protocol::BUILD_ID.into(),
            channel: sag_protocol::BUILD_CHANNEL.into(),
            pid: std::process::id(),
            executable: std::env::current_exe()?.display().to_string(),
            data_directory: self.data_dir.display().to_string(),
        })
    }
    pub async fn handle(
        self: &Arc<Self>,
        operation: sag_protocol::Operation,
        app: &EventSink,
    ) -> CommandResult<sag_protocol::Output> {
        use sag_protocol::{Operation as Op, Output as Out};
        if app.cancel.is_cancelled() {
            return Err("已停止".into());
        }
        let modified = matches!(
            &operation,
            Op::AddRoot { .. }
                | Op::AddRootWithOptions { .. }
                | Op::SaveFolderIndexOptions { .. }
                | Op::RemoveRoot { .. }
                | Op::SaveDocument { .. }
                | Op::ReindexAll
                | Op::SetIndexPaused { .. }
                | Op::UninstallDocling
                | Op::ClearIndexHistory
                | Op::SetConversionWorkers { .. }
                | Op::RemoveAllRoots
                | Op::ClearAllData
                | Op::SaveSettings { .. }
        );
        let history_changed = matches!(
            &operation,
            Op::EditConversation { .. } | Op::ReplaceMessages { .. } | Op::ClearConversations | Op::ClearAllData
        );
        let settings_changed = matches!(
            &operation,
            Op::SaveSettings { .. }
                | Op::SetConversionWorkers { .. }
                | Op::UninstallDocling
                | Op::ClearAllData
                | Op::ClearModelKey { .. }
                | Op::Cli {
                    command: sag_protocol::Command::Configure { .. },
                    ..
                }
        );
        let result = match operation {
            Op::Info => Out::Info(self.info().map_err(error)?),
            Op::Snapshot => Out::Snapshot(sag_protocol::Snapshot {
                roots: list_roots(self)?,
                files: list_files(self)?,
                settings: get_settings(self)?,
                conversations: list_conversations(self)?,
                drafts: list_drafts(self)?,
            }),
            Op::Cli { path, command } => Out::Cli(
                self.cli
                    .handle(path, command, app.cancel.clone())
                    .await
                    .map_err(error)?,
            ),
            Op::GetAccess => Out::Access(self.cli.access()),
            Op::SetAccess { access } => {
                self.cli.configure(access).map_err(error)?;
                Out::Unit
            }
            Op::ListRoots => Out::Roots(list_roots(self)?),
            Op::ListEntries { path } => Out::Entries(list_entries(path, self).await?),
            Op::ListFiles => Out::Files(list_files(self)?),
            Op::ReadDocument { path } => Out::Document(read_document(path, self).await?),
            Op::SaveDocument {
                path,
                body,
                expected_hash,
            } => Out::Document(save_document(path, body, expected_hash, self)?),
            Op::AddRoot { path } => Out::Text(add_root(path, self)?),
            Op::AddRootWithOptions { path, options } => {
                let directory = Path::new(&path).canonicalize().map_err(error)?;
                if !directory.is_dir() { return Err("请选择文件夹".into()); }
                if self.store.roots().map_err(error)?.iter().any(|root| directory.starts_with(&root.path)) {
                    return Err("此文件夹已被添加，请使用文件夹菜单中的索引设置".into());
                }
                if options.convert_other_formats {
                    self.store.docling.require_formats(&options.conversion_formats).map_err(error)?;
                }
                crate::folder_index::write(&directory, &options).map_err(error)?;
                Out::Text(add_root(path, self)?)
            }
            Op::GetFolderIndexOptions { path } => {
                let path = self.store.authorize(Path::new(&path)).map_err(error)?;
                Out::FolderIndexOptions(self.store.folder_index_options(&path).map_err(error)?)
            }
            Op::SaveFolderIndexOptions { path, options, rebuild } => {
                let root = self.store.save_folder_index_options(Path::new(&path), &options, rebuild).map_err(error)?;
                let path = self.store.authorize(Path::new(&path)).map_err(error)?;
                self.scan.send(IndexRequest::Paths { root, paths: vec![path.join(".sagignore")] }).map_err(error)?;
                Out::Unit
            }
            Op::RemoveRoot { path } => {
                remove_root(path, self)?;
                Out::Unit
            }
            Op::RemovedDirectories => Out::Strings(list_removed_directories(self)?),
            Op::RefreshRoot { path } => {
                refresh_root(path, self)?;
                Out::Unit
            }
            Op::ReindexAll => Out::Roots(reindex_all(self)?),
            Op::GetIndexStatus => Out::IndexStatus(self.store.indexing.status()),
            Op::GetIndexHistory => Out::IndexHistory(self.store.index_history().map_err(error)?),
            Op::SetConversionWorkers { workers } => {
                let mut settings = self.store.settings().map_err(error)?;
                settings.conversion_workers = workers;
                self.store.save_settings(&settings).map_err(error)?;
                Out::Unit
            }
            Op::GetIndexHistoryLog { id } => Out::IndexHistoryLog(self.store.index_history_log(id).map_err(error)?),
            Op::AuthorizeIndexHistoryFile { id, path } => Out::Text(
                self.store.authorize_index_history_file(id, &path).map_err(error)?.to_string_lossy().into_owned(),
            ),
            Op::ClearIndexHistory => {
                self.store.clear_index_history().map_err(error)?;
                Out::Unit
            }
            Op::SetIndexPaused { paused } => Out::IndexStatus(self.store.indexing.set_paused(paused)),
            Op::RemoveAllRoots => {
                let store = self.store.clone();
                tokio::task::spawn_blocking(move || store.remove_all_roots().map_err(error))
                    .await.map_err(error)??;
                Out::Unit
            }
            Op::PutDraft { draft } => {
                put_draft(draft, self)?;
                Out::Unit
            }
            Op::ListDrafts => Out::Drafts(list_drafts(self)?),
            Op::DeleteDraft { path } => {
                delete_draft(path, self)?;
                Out::Unit
            }
            Op::GetSettings => Out::Settings(get_settings(self)?),
            Op::RegisterDoclingComponent { path } => {
                self.store.docling.set_component_source(std::path::PathBuf::from(path)).map_err(error)?;
                Out::Unit
            }
            Op::DoclingStatus => Out::Docling(self.store.docling.status()),
            Op::InstallDocling => {
                self.store.docling.start().map_err(error)?;
                Out::Docling(self.store.docling.status())
            }
            Op::InstallDoclingProfiles { profiles } => {
                self.store.docling.start_profiles(profiles).map_err(error)?;
                Out::Docling(self.store.docling.status())
            }
            Op::CancelDoclingInstall => {
                self.store.docling.cancel();
                Out::Unit
            }
            Op::UninstallDocling => {
                let store = self.store.clone();
                tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                    store.docling.uninstall()?;
                    let mut settings = store.settings()?;
                    settings.convert_other_formats = false;
                    store.save_settings(&settings)?;
                    Ok(())
                }).await.map_err(error)?.map_err(error)?;
                Out::Docling(self.store.docling.status())
            }
            Op::SaveSettings {
                settings,
                answer_key,
                jev_key,
                mcp_credentials,
            } => {
                save_settings(settings, answer_key, jev_key, mcp_credentials, self, app)?;
                Out::Unit
            }
            Op::ClearModelKey { kind } => {
                clear_model_key(kind, self)?;
                Out::Unit
            }
            Op::TestModel { kind, config, key } => {
                Out::Text(test_model(kind, config, key, self).await?)
            }
            Op::TestMcp {
                server,
                credentials,
            } => Out::McpTools(test_mcp(server, credentials, self).await?),
            Op::ListConversations => Out::Conversations(list_conversations(self)?),
            Op::ClearConversations => {
                clear_conversations(self).await?;
                Out::Unit
            }
            Op::ClearAllData => {
                let _access = self.conversation_access.try_write()
                    .map_err(|_| "请先停止正在进行的问答，再清除所有数据".to_string())?;
                let store = self.store.clone();
                tokio::task::spawn_blocking(move || store.clear_all_data().map_err(error))
                    .await.map_err(error)??;
                models::clear_secrets(&self.data_dir).map_err(error)?;
                self.cli.configure(sag_protocol::Access::default()).map_err(error)?;
                Out::Unit
            }
            Op::EditConversation { id, title, deleted } => {
                edit_conversation(id, title, deleted, self)?;
                Out::Unit
            }
            Op::ReplaceMessages { id, messages } => {
                Out::Conversation(replace_messages(id, messages, self, app).await?)
            }
            Op::RunQuery { query } => Out::Query(run_query(query, self, app).await?),
            Op::SummarizeSearch { id, question, results } =>
                Out::SearchSummary(summarize_search(id, question, results, self, app).await?),
            Op::CancelQuery { id } => {
                cancel_query(id, self, app);
                Out::Unit
            }
            Op::ResolveToolApproval { id, allow } => {
                resolve_tool_approval(id, allow, self, app)?;
                Out::Unit
            }
            Op::ExploreOverview {
                scope,
                query,
                visible,
            } => Out::Overview(explore_overview(scope, query, visible, self).await?),
            Op::ExploreGraph { scope, request_id } => {
                Out::Graph(explore_graph(scope, request_id, self, app).await?)
            }
            Op::ExploreNeighbors {
                scope,
                center_id,
                snapshot,
                offset,
                request_id,
            } => Out::Neighbors(
                explore_neighbors(scope, center_id, snapshot, offset, request_id, self, app)
                    .await?,
            ),
            Op::ExploreEvidence {
                scope,
                snapshot,
                source,
                target,
            } => Out::Evidence(explore_evidence(scope, snapshot, source, target, self).await?),
            Op::AuthorizePath { path } => Out::Text(
                self.store
                    .authorize(Path::new(&path))
                    .map_err(error)?
                    .display()
                    .to_string(),
            ),
            Op::LoadImage { path } => Out::Text(load_image(path, self)?),
            Op::Shutdown => Out::Unit,
        };
        if modified {
            emit(app, "data-changed", ());
        }
        if settings_changed {
            emit(app, "settings-changed", ());
        }
        if history_changed {
            emit(app, "history-changed", ());
        }
        Ok(result)
    }
}
