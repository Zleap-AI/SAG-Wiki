mod build;
mod cli;
pub mod explore;
mod retrieval;
pub mod types;
pub use build::*;
pub use cli::*;
use explore::*;
pub use retrieval::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use types::*;

pub const RETRIEVAL_VERSION: u8 = 10;
pub const BUILD_ID: &str = env!("SAG_BUILD_ID");
pub const BUILD_CHANNEL: &str = env!("SAG_BUILD_CHANNEL");
pub const MAX_FRAME: usize = 128 * 1024;
pub const MAX_MESSAGE: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceInfo {
    #[serde(default)]
    pub version: Option<String>,
    pub protocol_version: u32,
    pub build_id: String,
    pub channel: String,
    pub pid: u32,
    pub executable: String,
    pub data_directory: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Access {
    pub enabled: bool,
}
impl Default for Access {
    fn default() -> Self {
        Self { enabled: true }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Query {
    pub id: String,
    pub kind: String,
    pub question: String,
    pub scope: Vec<String>,
    pub references: Vec<String>,
    pub model: String,
    pub conversation_id: Option<String>,
    pub replace_from: Option<usize>,
    pub page: Option<usize>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct QueryOutput {
    pub terms: Vec<String>,
    pub elapsed_ms: u64,
    pub results: Vec<Candidate>,
    pub trace: Vec<Trace>,
    pub conversation: Option<Conversation>,
    pub pagination: Option<SearchPagination>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct SearchSummary {
    pub text: String,
    pub evidence: Vec<Candidate>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexEvent {
    pub path: String,
    pub count: usize,
    pub total: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct IndexStatus {
    pub paused: bool,
    pub tasks: Vec<IndexTask>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexTask {
    pub path: String,
    pub stage: String,
    pub completed: usize,
    pub total: usize,
    pub files: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexHistory {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub started_at: i64,
    pub finished_at: i64,
    pub completed: usize,
    pub total: usize,
    pub status: String,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexFileLog {
    pub time: i64,
    pub path: String,
    pub stage: String,
    pub status: String,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexHistoryLog {
    pub raw: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub roots: Vec<Root>,
    pub files: Vec<FileInfo>,
    pub settings: Settings,
    pub conversations: Vec<Conversation>,
    pub drafts: Vec<Draft>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", content = "payload", rename_all = "kebab-case")]
pub enum ServiceEvent {
    ServiceDisconnected(String),
    DataChanged,
    SettingsChanged,
    HistoryChanged,
    IndexProgress(IndexEvent),
    AppError(String),
    QueryProgress(Progress),
    ToolApproval(ToolApproval),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub request_id: Option<String>,
    pub event: ServiceEvent,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", content = "args", rename_all = "snake_case")]
pub enum Operation {
    DoclingStatus,
    InstallDocling,
    CancelDoclingInstall,
    UninstallDocling,
    Info,
    Snapshot,
    Cli {
        path: std::path::PathBuf,
        command: Command,
    },
    GetAccess,
    SetAccess {
        access: Access,
    },
    ListRoots,
    ListEntries {
        path: String,
    },
    ListFiles,
    ReadDocument {
        path: String,
    },
    SaveDocument {
        path: String,
        body: String,
        expected_hash: Option<String>,
    },
    AddRoot {
        path: String,
    },
    AddRootWithOptions {
        path: String,
        options: FolderIndexOptions,
    },
    GetFolderIndexOptions {
        path: String,
    },
    SaveFolderIndexOptions {
        path: String,
        options: FolderIndexOptions,
        rebuild: bool,
    },
    RemoveRoot {
        path: String,
    },
    RemovedDirectories,
    RefreshRoot {
        path: String,
    },
    ReindexAll,
    GetIndexStatus,
    GetIndexHistory,
    GetIndexHistoryLog { id: i64 },
    SetConversionWorkers { workers: usize },
    AuthorizeIndexHistoryFile { id: i64, path: String },
    ClearIndexHistory,
    SetIndexPaused { paused: bool },
    RemoveAllRoots,
    PutDraft {
        draft: Draft,
    },
    ListDrafts,
    DeleteDraft {
        path: String,
    },
    GetSettings,
    SaveSettings {
        settings: Settings,
        answer_key: String,
        jev_key: String,
        mcp_credentials: Option<BTreeMap<String, McpCredentials>>,
    },
    ClearModelKey {
        kind: String,
    },
    TestModel {
        kind: String,
        config: ModelConfig,
        key: String,
    },
    TestMcp {
        server: McpServer,
        credentials: Option<McpCredentials>,
    },
    ListConversations,
    ClearConversations,
    ClearAllData,
    EditConversation {
        id: String,
        title: Option<String>,
        deleted: Option<bool>,
    },
    ReplaceMessages {
        id: String,
        messages: Vec<Message>,
    },
    RunQuery {
        query: Query,
    },
    SummarizeSearch {
        id: String,
        question: String,
        results: Vec<Candidate>,
    },
    CancelQuery {
        id: String,
    },
    ResolveToolApproval {
        id: String,
        allow: bool,
    },
    ExploreOverview {
        scope: String,
        query: String,
        visible: Vec<i64>,
    },
    ExploreGraph {
        scope: String,
        request_id: String,
    },
    ExploreNeighbors {
        scope: String,
        center_id: i64,
        snapshot: Option<String>,
        offset: usize,
        request_id: String,
    },
    ExploreEvidence {
        scope: String,
        snapshot: String,
        source: i64,
        target: i64,
    },
    AuthorizePath {
        path: String,
    },
    LoadImage {
        path: String,
    },
    Shutdown,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Output {
    Docling(DoclingStatus),
    IndexStatus(IndexStatus),
    IndexHistory(Vec<IndexHistory>),
    IndexHistoryLog(IndexHistoryLog),
    FolderIndexOptions(FolderIndexOptions),
    Unit,
    Info(ServiceInfo),
    Snapshot(Snapshot),
    Cli(Response),
    Access(Access),
    Roots(Vec<Root>),
    Entries(Vec<Entry>),
    Files(Vec<FileInfo>),
    Document(Document),
    Strings(Vec<String>),
    Text(String),
    Drafts(Vec<Draft>),
    Settings(Settings),
    McpTools(Vec<McpToolInfo>),
    Conversations(Vec<Conversation>),
    Conversation(Conversation),
    Query(QueryOutput),
    SearchSummary(SearchSummary),
    Overview(ExploreOverview),
    Graph(ExploreGraph),
    Neighbors(ExploreNeighbors),
    Evidence(ExploreEvidence),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DoclingStatus {
    pub installed: bool,
    pub running: bool,
    pub stage: String,
    pub message: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    #[serde(default)]
    pub percent: Option<f64>,
    pub error: Option<String>,
    pub formats: Vec<(String, Vec<String>)>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "frame", rename_all = "snake_case")]
pub enum Frame {
    Hello {
        protocol_version: u32,
        token: String,
    },
    Welcome {
        info: ServiceInfo,
    },
    Request {
        id: String,
        operation: Operation,
    },
    Cancel {
        id: String,
    },
    Response {
        id: String,
        result: Result<Output, String>,
    },
    Event {
        envelope: EventEnvelope,
    },
}
