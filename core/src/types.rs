use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    None,
    Low,
    Medium,
    High,
    Max,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelProfile {
    pub system_prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
    pub sag2_enabled: bool,
    pub mcp_servers: Vec<String>,
    pub mcp_tools: BTreeMap<String, Vec<String>>,
}
impl Default for ModelProfile {
    fn default() -> Self {
        Self {
            system_prompt: String::new(),
            reasoning_effort: None,
            sag2_enabled: true,
            mcp_servers: vec![],
            mcp_tools: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub url: String,
    pub command: String,
    pub args: Vec<String>,
    pub require_confirmation: bool,
    pub has_credentials: bool,
    pub tools: Option<Vec<McpToolInfo>>,
}
impl Default for McpServer {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            transport: "http".into(),
            url: String::new(),
            command: String::new(),
            args: vec![],
            require_confirmation: true,
            has_credentials: false,
            tools: None,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct McpCredentials {
    pub env: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct ToolApproval {
    pub id: String,
    pub query_id: String,
    pub server_name: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Root {
    pub path: String,
    pub name: String,
    pub status: String,
    pub count: usize,
    pub error: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct IndexProgress {
    pub count: usize,
    pub total: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    pub name: String,
    pub directory: bool,
    pub symlink: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub body: String,
    pub hash: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileInfo {
    pub id: i64,
    pub path: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentTerm {
    pub term: String,
    pub count: usize,
    pub boost: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub id: i64,
    pub document_id: i64,
    pub path: String,
    pub title: String,
    pub heading: String,
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
    pub source_ranges: Vec<(usize, usize)>,
    #[serde(default)]
    pub source_spans: Vec<SourceSpan>,
    pub bm25: f64,
    pub score: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_score: Option<f64>,
    pub order: usize,
    pub hash: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchPagination {
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelConfig {
    pub url: String,
    pub model: String,
    pub models: Vec<String>,
    pub has_key: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
}
impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            url: "https://api.302ai.cn/v1".into(),
            model: "qwen3.6-flash".into(),
            models: vec!["qwen3.6-flash".into()],
            has_key: false,
            reasoning_effort: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Retrieval {
    pub candidate_k: usize,
    pub seed_k: usize,
    pub expansion_term_limit: usize,
    pub max_hops: usize,
    pub top_k: usize,
    pub chunk_chars: usize,
}
impl Default for Retrieval {
    fn default() -> Self {
        Self {
            candidate_k: 100,
            seed_k: 10,
            expansion_term_limit: 20,
            max_hops: 2,
            top_k: 10,
            chunk_chars: 2200,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileTypes {
    pub markdown: bool,
    pub text: bool,
    pub code: bool,
    pub config: bool,
}
impl Default for FileTypes {
    fn default() -> Self {
        Self { markdown: true, text: true, code: true, config: true }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(default)]
    pub retrieval_version: u8,
    pub credential_error: Option<String>,
    pub answer: ModelConfig,
    pub jev: ModelConfig,
    pub retrieval: Retrieval,
    pub theme: String,
    pub language: String,
    pub start_page: String,
    pub editor_size: usize,
    pub show_hidden: bool,
    pub watch: bool,
    pub index_workers: usize,
    pub excluded: Vec<String>,
    pub file_types: FileTypes,
    pub timeout_secs: u64,
    pub answer_context_tokens: usize,
    pub answer_output_tokens: usize,
    pub max_tool_calls: usize,
    pub model_profiles: BTreeMap<String, ModelProfile>,
    pub mcp_servers: Vec<McpServer>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            retrieval_version: crate::sag2_v10::VERSION,
            credential_error: None,
            answer: ModelConfig::default(),
            jev: ModelConfig {
                url: "https://api.302ai.cn/typesafeai/v1".into(),
                model: "jev-latest".into(),
                models: vec![],
                has_key: false,
                reasoning_effort: None,
            },
            retrieval: Retrieval::default(),
            theme: "light".into(),
            language: "zh-CN".into(),
            start_page: "ask".into(),
            editor_size: 15,
            show_hidden: true,
            watch: true,
            index_workers: 8,
            file_types: FileTypes::default(),
            excluded: [
                ".git",
                "node_modules",
                "dist",
                "target",
                ".venv",
                "venv",
                "__pycache__",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            timeout_secs: 120,
            answer_context_tokens: 65536,
            answer_output_tokens: 2048,
            max_tool_calls: 10,
            model_profiles: BTreeMap::new(),
            mcp_servers: vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnswerContext {
    pub limit: usize,
    pub reserved_output: usize,
    pub estimated_input: usize,
    pub omitted_sources: usize,
    pub omitted_history: usize,
    pub input_tokens: Option<usize>,
    pub output_tokens: Option<usize>,
    #[serde(default)]
    pub elapsed_ms: Option<u64>,
    #[serde(default)]
    pub generation_ms: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub id: String,
    pub query: String,
    pub source_count: usize,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub server_name: Option<String>,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub denied: bool,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub question: String,
    pub answer: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reasoning: String,
    pub evidence: Vec<Candidate>,
    pub model: String,
    pub scope: Vec<String>,
    pub references: Vec<String>,
    pub status: String,
    pub error: Option<String>,
    pub retrieval_question: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCallRecord>,
    #[serde(default)]
    pub context: Option<AnswerContext>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub messages: Vec<Message>,
    pub updated: i64,
    pub deleted: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Draft {
    pub path: String,
    pub name: String,
    pub body: String,
    pub hash: String,
    pub is_new: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolProgress {
    pub call: ToolCallRecord,
    pub finished: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Progress {
    pub replace: bool,
    pub id: String,
    pub stage: String,
    pub delta: Option<String>,
    pub reasoning_delta: Option<String>,
    pub count: usize,
    pub tool: Option<ToolProgress>,
}

#[cfg(test)]
mod default_tests {
    use super::*;
    #[test]
    fn sag2_stays_enabled_for_new_and_existing_model_profiles() {
        assert!(ModelProfile::default().sag2_enabled);
        assert!(
            serde_json::from_str::<ModelProfile>(
                r#"{"system_prompt":"原有提示词","mcp_servers":["wiki"]}"#
            )
            .unwrap()
            .sag2_enabled
        );
        assert!(
            !serde_json::from_str::<ModelProfile>(r#"{"sag2_enabled":false}"#)
                .unwrap()
                .sag2_enabled
        );
    }
    #[test]
    fn answer_context_defaults_to_64k() {
        assert_eq!(Settings::default().answer_context_tokens, 65_536);
        assert_eq!(
            serde_json::from_str::<Settings>("{}")
                .unwrap()
                .answer_context_tokens,
            65_536
        );
    }
    #[test]
    fn retrieval_defaults_to_v10() {
        assert_eq!(Settings::default().retrieval.chunk_chars, 2200);
        assert_eq!(serde_json::from_str::<Settings>("{}").unwrap().retrieval.chunk_chars, 2200);
        assert_eq!(Settings::default().retrieval.candidate_k, 100);
        assert_eq!(
            serde_json::from_str::<Settings>("{}")
                .unwrap()
                .retrieval
                .candidate_k,
            100
        );
        assert_eq!(Settings::default().retrieval.max_hops, 2);
        assert_eq!(
            serde_json::from_str::<Settings>("{}")
                .unwrap()
                .retrieval
                .max_hops,
            2
        );
    }
}
