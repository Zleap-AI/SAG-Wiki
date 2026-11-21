use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Feishu,
    Lark,
    Notion,
    Yuque,
    Github,
    Gitlab,
    Dingtalk,
    Ima,
    Rss,
    Apify,
    Api,
}

impl Platform {
    pub fn requires_secret(self) -> bool {
        !matches!(self, Self::Github | Self::Gitlab | Self::Rss | Self::Api)
    }

    pub fn is_repository(self) -> bool {
        matches!(self, Self::Github | Self::Gitlab)
    }

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<ApiConfig>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ApiMethod {
    Get,
    Post,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiRequest {
    pub method: ApiMethod,
    pub url: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub secret_header: String,
    pub body: Option<String>,
    pub allow_lan: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiCleanup {
    pub model: String,
    pub prompt: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiConfig {
    pub request: ApiRequest,
    pub records_path: String,
    pub fields: Vec<String>,
    pub dedup_field: Option<String>,
    pub cleanup: Option<ApiCleanup>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    Apify {
        token: String,
        #[serde(default)]
        connection: Option<String>,
        request: ApifyAction,
    },
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
    ApiSample {
        id: Option<String>,
        request: ApiRequest,
        secret: Option<String>,
    },
    ApiPreview {
        config: Box<ApiConfig>,
        response: serde_json::Value,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ApifyAction {
    Store {
        search: String,
        offset: usize,
    },
    GetConnection,
    SaveConnection {
        name: String,
        actor: String,
        reference: String,
        input: serde_json::Value,
        directory: String,
        #[serde(default)]
        schedule: Schedule,
        #[serde(default)]
        daily_time: String,
        #[serde(default)]
        paused: bool,
    },
    RemoveConnection,
    Describe { actor: String },
    Start { actor: String, input: serde_json::Value },
    Status { run: String },
    Cancel { run: String },
    Preview { run: String },
    Import {
        run: String,
        directory: String,
        offset: usize,
        preset: bool,
        mapping: ApifyMapping,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApifyMapping {
    pub title: Option<String>,
    pub url: Option<String>,
    pub id: Option<String>,
    pub content: Vec<String>,
}
