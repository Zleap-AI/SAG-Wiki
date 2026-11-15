use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatSource {
    Claude,
    Codex,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub title: String,
    pub project: String,
    pub updated: String,
    pub paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatImportIssue {
    pub path: String,
    pub error: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatListQuery {
    pub page: usize,
    pub search: String,
    pub project: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatCatalog {
    pub source_root: String,
    pub sessions: Vec<ChatSession>,
    pub issues: Vec<ChatImportIssue>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub projects: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatPreview {
    pub title: String,
    pub body: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatImportItem {
    pub path: String,
    pub status: String,
    pub error: Option<String>,
}
