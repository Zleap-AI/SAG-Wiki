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
    data_dir_from_config(&data_directory_config()?)
}

/// This locator stays outside the movable application data directory.
pub fn data_directory_config() -> Result<PathBuf> {
    Ok(dirs::config_dir()
        .context("无法获取用户配置目录")?
        .join("com.sag.desktop-location.json"))
}

fn data_dir_from_config(config: &std::path::Path) -> Result<PathBuf> {
    if let Some(path) = read_data_directory_location(config)? {
        anyhow::ensure!(path.is_absolute() && path.join("sag.sqlite").is_file(), "应用数据目录不可用：{}", path.display());
        return Ok(path);
    }
    Ok(dirs::data_dir()
        .context("无法获取用户数据目录")?
        .join("com.sag.desktop"))
}

pub fn read_data_directory_location(config: &std::path::Path) -> Result<Option<PathBuf>> {
    match std::fs::read(config) {
        Ok(bytes) => serde_json::from_slice(&bytes).context("应用数据位置配置无效").map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("无法读取应用数据位置配置"),
    }
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
        #[serde(default)]
        auth_mode: Option<crate::types::ModelAuthMode>,
    },
    Add,
    AddWithTypes { extensions: Vec<String> },
    Remove,
    Search {
        #[serde(default)]
        exclude: Vec<String>,
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

#[cfg(test)]
mod data_directory_tests {
    use super::*;
    #[test]
    fn saved_location_is_shared_and_invalid_locations_fail() {
        let temp = std::env::temp_dir().join(format!("sag-location-test-{}", std::process::id()));
        std::fs::create_dir(&temp).unwrap();
        let config = temp.join("location.json");
        std::fs::write(temp.join("sag.sqlite"), b"database").unwrap();
        std::fs::write(&config, serde_json::to_vec(&temp).unwrap()).unwrap();
        assert_eq!(data_dir_from_config(&config).unwrap(), temp);
        std::fs::write(&config, b"invalid json").unwrap();
        assert!(data_dir_from_config(&config).is_err());
        std::fs::write(&config, br#""relative-folder""#).unwrap();
        assert!(data_dir_from_config(&config).is_err());
        std::fs::write(&config, serde_json::to_vec(&temp.join("missing")).unwrap()).unwrap();
        assert!(data_dir_from_config(&config).is_err());
        std::fs::create_dir(temp.join("missing")).unwrap();
        assert!(data_dir_from_config(&config).is_err());
        std::fs::remove_dir_all(temp).unwrap();
    }
    #[test]
    fn missing_locator_uses_the_existing_default() {
        let config = std::env::temp_dir().join(format!("sag-missing-location-{}.json", std::process::id()));
        assert_eq!(data_dir_from_config(&config).unwrap(), dirs::data_dir().unwrap().join("com.sag.desktop"));
    }
}
