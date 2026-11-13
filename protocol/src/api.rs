use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ApiAction {
    Status,
    WebStart { host: Option<String>, port: Option<u16> },
    WebStop,
    WebCode,
    WebRotateCode,
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
    CreateKey {
        name: String,
    },
    RevokeKey {
        id: String,
    },
    Autostart {
        enabled: Option<bool>,
    },
}
