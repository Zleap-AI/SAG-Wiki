use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const VERSION: u32 = 1;
pub const MAX_REQUEST: u64 = 32 * 1024;
pub const MAX_RESPONSE: u64 = 512 * 1024;
pub const DEFAULT_TIMEOUT: u64 = 600;

pub fn data_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SAG_DESKTOP_DATA_DIR") {
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
    Add,
    Search {
        query: String,
        max_results: usize,
        content: bool,
        timeout: u64,
    },
}
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub path: String,
    pub absolute_path: String,
    pub start_line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
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
    Added(AddOutput),
    Search(SearchOutput),
    Error(String),
}
