use serde::{Deserialize, Serialize};

/// Public API/MCP access scope. Missing fields retain legacy full-library access.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ApiScope {
    #[default]
    All,
    Folders {
        paths: Vec<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ApiAction {
    Status,
    WebStart {
        host: Option<String>,
        port: Option<u16>,
    },
    WebStop,
    WebCode,
    WebRotateCode,
    WebSetAccessCode {
        required: bool,
    },
    WebSetExternalUrl {
        url: Option<String>,
    },
    Start {
        host: Option<String>,
        port: Option<u16>,
    },
    Stop,
    Configure {
        host: String,
        port: u16,
    },
    ListKeys,
    CopyKey {
        id: Option<String>,
    },
    // Keep this wire shape unchanged for older management clients.
    CreateKey {
        name: String,
    },
    // Old services reject this operation instead of ignoring a scope field.
    CreateScopedKey {
        name: String,
        paths: Vec<String>,
    },
    UpdateKeyScope {
        id: String,
        scope: ApiScope,
    },
    RevokeKey {
        id: String,
    },
    Autostart {
        enabled: Option<bool>,
    },
}
