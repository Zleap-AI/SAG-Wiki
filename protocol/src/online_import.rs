use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Feishu,
    Lark,
    Notion,
    Yuque,
    Rss,
}

impl Platform {
    pub fn supports_account_login(self) -> bool {
        matches!(self, Self::Feishu | Self::Lark)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Schedule {
    #[default]
    Manual,
    HalfHourly,
    Hourly,
    EverySixHours,
    EveryTwelveHours,
    Daily,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Connection {
    pub platform: Platform,
    pub drive: bool,
    #[serde(default)]
    pub user_access: bool,
    pub app_id: String,
    pub address: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub connection: Connection,
    pub resources: Vec<String>,
    pub directory: String,
    pub schedule: Schedule,
    pub daily_time: String,
    pub paused: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    #[serde(alias = "feishu_login")]
    AccountLogin {
        #[serde(default)]
        platform: Option<Platform>,
        app_id: String,
        app_secret: String,
        drive: bool,
    },
    #[serde(alias = "feishu_login_status")]
    AccountLoginStatus {
        session: String,
    },
    #[serde(alias = "cancel_feishu_login")]
    CancelAccountLogin {
        session: String,
    },
    List {
        platform: Platform,
    },
    Browse {
        id: Option<String>,
        connection: Connection,
        secret: Option<String>,
        auth_session: Option<String>,
        parent: Option<String>,
        cursor: Option<String>,
    },
    Save {
        source: Source,
        secret: Option<String>,
        auth_session: Option<String>,
    },
    Run {
        id: String,
        retry: bool,
    },
    Cancel {
        id: String,
    },
    Remove {
        id: String,
    },
    History {
        id: String,
    },
}
