//! Settings for the two supported hosted MinerU services.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MineruProvider {
    #[default]
    #[serde(rename = "302")]
    Ai302,
    #[serde(rename = "official")]
    Official,
}
impl MineruProvider {
    pub fn credential_kind(self) -> &'static str {
        match self {
            Self::Ai302 => "mineru:302",
            Self::Official => "mineru:official",
        }
    }
    pub fn base_url(self) -> &'static str {
        match self {
            Self::Ai302 => "https://api.302.ai",
            Self::Official => "https://mineru.net/api/v4",
        }
    }
    pub fn upload_limit_bytes(self) -> u64 {
        match self {
            Self::Ai302 => 50 * 1024 * 1024,
            Self::Official => 200 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MineruModel {
    #[default]
    Vlm,
    Pipeline,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct MineruConfig {
    pub enabled: bool,
    pub provider: MineruProvider,
    pub has_key: bool,
    pub request_timeout_secs: u64,
    pub task_timeout_secs: u64,
    pub poll_interval_secs: u64,
    pub concurrency: usize,
    pub model_version: MineruModel,
    pub ocr: bool,
    pub table: bool,
    pub formula: bool,
}
impl Default for MineruConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: MineruProvider::Ai302,
            has_key: false,
            request_timeout_secs: 120,
            task_timeout_secs: 1800,
            poll_interval_secs: 2,
            concurrency: 2,
            model_version: MineruModel::Vlm,
            ocr: true,
            table: true,
            formula: true,
        }
    }
}
impl MineruConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(5..=3600).contains(&self.request_timeout_secs) {
            return Err("MinerU 请求超时应为 5–3600 秒".into());
        }
        if !(10..=86400).contains(&self.task_timeout_secs) {
            return Err("MinerU 任务超时应为 10–86400 秒".into());
        }
        if !(1..=60).contains(&self.poll_interval_secs) {
            return Err("MinerU 查询间隔应为 1–60 秒".into());
        }
        if !(1..=16).contains(&self.concurrency) {
            return Err("MinerU 并行数应为 1–16".into());
        }
        Ok(())
    }
    /// Settings which change the document content, not transport scheduling.
    pub fn content_identity(&self) -> String {
        format!(
            "mineru-v1:{:?}:{:?}:{}:{}:{}",
            self.provider, self.model_version, self.ocr, self.table, self.formula
        )
    }
}
