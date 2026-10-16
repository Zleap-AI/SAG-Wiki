use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const VERSION: u32 = 1;
pub const MAX_REQUEST: u64 = 32 * 1024;
pub const MAX_RESPONSE: u64 = 512 * 1024;
pub const DEFAULT_TIMEOUT: u64 = 600;

pub fn data_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SAG_DATA_DIR").or_else(|| std::env::var_os("SAG_DESKTOP_DATA_DIR")) {
        return Ok(path.into());
    }
    Ok(dirs::data_dir()
        .context("无法获取用户数据目录")?
        .join("com.sag.desktop"))
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub protocol_version: u32,
    pub port: u16,
    pub pid: u32,
    pub token: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol_version: u32,
    pub token: String,
    pub path: PathBuf,
    pub command: Command,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    Status,
    Configure {
        #[serde(default)]
        enable: bool,
        url: Option<String>,
        model: Option<String>,
        key: Option<String>,
    },
    Add,
    Remove,
    Search {
        query: String,
        max_results: usize,
        content: bool,
        timeout: u64,
    },
    Match(MatchRequest),
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    Regex,
    Fixed,
}
#[derive(Serialize, Deserialize)]
pub struct MatchRequest {
    pub patterns: Vec<String>,
    pub mode: MatchMode,
    pub max_results: Option<usize>,
    pub content: bool,
    pub timeout: u64,
    pub options: MatchOptions,
}
#[derive(Default, Serialize, Deserialize)]
pub struct MatchOptions {
    pub ignore_case: bool,
    pub word_regexp: bool,
    pub line_regexp: bool,
    pub invert_match: bool,
    pub before_context: usize,
    pub after_context: usize,
    pub output: MatchOutputMode,
    pub filters: Vec<MatchFilter>,
}
#[derive(Serialize, Deserialize)]
pub struct MatchFilter { pub include: bool, pub pattern: String }
#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchOutputMode { #[default] Lines, OnlyMatching, Files, Count, Quiet }
#[derive(Debug, Serialize, Deserialize)]
pub struct MatchFile {
    pub path: String,
    pub absolute_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct MatchOutput {
    pub protocol_version: u32,
    pub scope: String,
    pub results: Vec<SearchResult>,
    pub files: Vec<MatchFile>,
    pub matched: bool,
    pub truncated: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub path: String,
    pub absolute_path: String,
    pub start_line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_lines: Vec<usize>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchOutput {
    pub protocol_version: u32,
    pub scope: String,
    pub results: Vec<SearchResult>,
    pub truncated: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct AddOutput {
    pub path: String,
    pub index_status: String,
    pub cli_authorized: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Status {
    pub protocol_version: u32,
    pub connected: bool,
    pub enabled: bool,
    pub allowed: bool,
    pub index_ready: bool,
    pub jev_configured: bool,
    pub path: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Response {
    Status(Status),
    Configured(crate::types::ModelConfig),
    Added(AddOutput),
    Removed { path: String },
    Search(SearchOutput),
    Matches(MatchOutput),
    Error(String),
}
