use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const VERSION: u32 = 2;
pub const MAX_REQUEST: u64 = 32 * 1024;
pub const MAX_RESPONSE: u64 = 512 * 1024;
pub const DEFAULT_TIMEOUT: u64 = 600;

pub fn data_dir() -> Result<PathBuf> {
    if let Some(path) =
        std::env::var_os("SAG_DATA_DIR").or_else(|| std::env::var_os("SAG_DESKTOP_DATA_DIR"))
    {
        return Ok(path.into());
    }
    Ok(dirs::data_dir()
        .context("无法获取用户数据目录")?
        .join("com.sag.desktop"))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub protocol_version: u32,
    pub port: u16,
    pub pid: u32,
    pub token: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol_version: u32,
    pub token: String,
    pub path: PathBuf,
    pub command: Command,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    Status,
    List,
    Check,
    Configure {
        #[serde(default)]
        enable: bool,
        url: Option<String>,
        model: Option<String>,
        key: Option<String>,
    },
    Add,
    AddWithTypes { extensions: Vec<String> },
    Remove,
    Search {
        query: String,
        max_results: usize,
        content: bool,
        timeout: u64,
    },
    Match(MatchRequest),
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    Regex,
    Fixed,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchRequest {
    pub patterns: Vec<String>,
    pub mode: MatchMode,
    pub max_results: Option<usize>,
    pub content: bool,
    pub timeout: u64,
    pub options: MatchOptions,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchFilter {
    pub include: bool,
    pub pattern: String,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchOutputMode {
    #[default]
    Lines,
    OnlyMatching,
    Files,
    Count,
    Quiet,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchFile {
    pub path: String,
    pub absolute_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchOutput {
    pub protocol_version: u32,
    pub scope: String,
    pub results: Vec<SearchResult>,
    pub files: Vec<MatchFile>,
    pub matched: bool,
    pub truncated: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResult {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub heading: String,
    pub path: String,
    pub absolute_path: String,
    pub start_line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_lines: Vec<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchOutput {
    pub protocol_version: u32,
    pub scope: String,
    pub results: Vec<SearchResult>,
    pub truncated: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddOutput {
    pub path: String,
    pub index_status: String,
    pub cli_authorized: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Status {
    pub protocol_version: u32,
    pub connected: bool,
    pub enabled: bool,
    pub jev_configured: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexCoverage {
    Included,
    Partial,
    Excluded,
    None,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<String>>,
    pub path: String,
    pub coverage: IndexCoverage,
    pub index_ready: bool,
    pub enabled: bool,
    pub indexed_files: usize,
    pub roots: Vec<crate::types::Root>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Response {
    Status(Status),
    Roots(Vec<crate::types::Root>),
    Checked(CheckOutput),
    Configured(crate::types::ModelConfig),
    Added(AddOutput),
    Removed { path: String },
    Search(SearchOutput),
    Matches(MatchOutput),
    Error(String),
}
