use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Agent {
    ClaudeCode,
    Codex,
    ClaudeDesktop,
    #[serde(rename = "workbuddy", alias = "work-buddy")]
    WorkBuddy,
}

impl Agent {
    pub const ALL: [Self; 4] = [
        Self::ClaudeCode,
        Self::Codex,
        Self::ClaudeDesktop,
        Self::WorkBuddy,
    ];

    pub fn folder(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::ClaudeDesktop => "claude-desktop",
            Self::WorkBuddy => "workbuddy",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Html,
    Slides,
    Word,
    Spreadsheet,
    Pdf,
    Markdown,
    Image,
}

pub const DEFAULT_FORMATS: [Format; 7] = [
    Format::Html,
    Format::Slides,
    Format::Word,
    Format::Spreadsheet,
    Format::Pdf,
    Format::Markdown,
    Format::Image,
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Source {
    pub agent: Agent,
    pub enabled: bool,
    pub directory: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Options {
    pub sources: Vec<Source>,
    pub formats: Vec<Format>,
    pub destination: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Status {
        agent: Agent,
    },
    Scan {
        options: Options,
    },
    Page {
        scan: String,
        page: usize,
    },
    Start {
        scan: String,
        exclude_all: bool,
        exceptions: Vec<String>,
        #[serde(default)]
        auto_sync: bool,
    },
    Stop {
        agent: Agent,
    },
}
