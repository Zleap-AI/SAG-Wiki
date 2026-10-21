use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize)]
pub struct McpProcessRequest {
    pub uid: u32,
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize)]
pub struct McpProcessReply {
    pub pid: Option<u32>,
    pub stderr_path: Option<String>,
    pub error: Option<String>,
}
