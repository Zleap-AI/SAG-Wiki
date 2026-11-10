//! Settings for the two supported hosted MinerU services.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MineruProvider {
    #[default]
    #[serde(rename = "302", alias = "302_opensource")]
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
            Self::Ai302 => "https://api.302ai.cn",
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
    pub version: String,
    pub parse_method: String,
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
            request_timeout_secs: 60,
            task_timeout_secs: 1800,
            poll_interval_secs: 2,
            concurrency: 2,
            model_version: MineruModel::Vlm,
            version: "2.5".into(),
            parse_method: "auto".into(),
            ocr: true,
            table: true,
            formula: true,
        }
    }
}
impl MineruConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !["2.0", "2.5"].contains(&self.version.as_str()) || !["auto", "txt", "ocr"].contains(&self.parse_method.as_str()) {
            return Err("MinerU 版本或解析方式无效".into());
        }
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
            "mineru-v2:{}:{:?}:{}:{}:{}:{}:{}",
            match self.provider {MineruProvider::Ai302=>"Ai302OpenSource",MineruProvider::Official=>"Official"}, self.model_version, self.ocr, self.table, self.formula, self.version, self.parse_method
        )
    }
}

#[cfg(test)]
mod endpoint_tests {
    use super::*;
    #[test]
    fn hosted_endpoints_match_provider_configuration() {
        for name in ["302","302_opensource"] {let config:MineruConfig=serde_json::from_value(serde_json::json!({"provider":name})).unwrap();assert_eq!(config.provider,MineruProvider::Ai302);assert_eq!(serde_json::to_value(&config).unwrap()["provider"],"302");}
        let mut config=MineruConfig::default();assert_eq!(config.provider,MineruProvider::Ai302);assert_eq!(config.version,"2.5");assert_eq!(config.parse_method,"auto");
        config.version="invalid".into();assert!(config.validate().is_err());
        assert_eq!(MineruProvider::Ai302.base_url(), "https://api.302ai.cn");
        assert_eq!(MineruProvider::Official.base_url(), "https://mineru.net/api/v4");
    }
}
