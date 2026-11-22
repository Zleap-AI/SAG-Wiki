use super::{clean_text, fallback_title, field, text, title_text, transcript, visit};
use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use sag_protocol::{ChatCatalog, ChatImportIssue, ChatSession, ChatSource};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub(super) fn session_id(path: &Path) -> Option<&str> {
    let name = path.parent()?.file_name()?.to_str()?;
    // Cowork originally used an eight-character session folder name.
    (name.len() == 8 && name.bytes().all(|byte| byte.is_ascii_hexdigit())
        || name
            .strip_prefix("local_")
            .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok()))
    .then_some(name)
}

pub(super) fn primary_message(event: &Value) -> bool {
    let role = field(event, "type");
    matches!(role, "user" | "assistant")
        && field(&event["message"], "role") == role
        && event["parent_tool_use_id"].is_null()
        && event["isReplay"] != true
        && event["isSynthetic"] != true
        && event["isMeta"] != true
        && event["isSidechain"] != true
        && event["isCompactSummary"] != true
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Metadata {
    title: String,
    cwd: String,
    user_selected_folders: Vec<String>,
    last_activity_at: Option<i64>,
}

pub(super) struct Details {
    pub(super) title: String,
    pub(super) project: String,
    updated: String,
}

pub(super) fn details(path: &Path, root: &Path) -> Result<Details> {
    let metadata_path = path.parent().unwrap().with_extension("json");
    let metadata: Metadata = if metadata_path.starts_with(root) {
        match fs::symlink_metadata(&metadata_path) {
            Ok(info) if info.is_file() => serde_json::from_slice(&fs::read(&metadata_path)?)
                .with_context(|| {
                    format!("Claude Desktop 会话信息无效：{}", metadata_path.display())
                })?,
            Ok(_) => Metadata::default(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Metadata::default(),
            Err(error) => return Err(error.into()),
        }
    } else {
        Metadata::default()
    };
    let updated = match metadata
        .last_activity_at
        .and_then(DateTime::from_timestamp_millis)
    {
        Some(time) => time,
        None => DateTime::<Utc>::from(fs::metadata(path)?.modified()?),
    };
    let project = metadata
        .user_selected_folders
        .into_iter()
        .next()
        .unwrap_or(metadata.cwd);
    Ok(Details {
        title: title_text(&metadata.title)?,
        project,
        updated: updated.to_rfc3339_opts(SecondsFormat::Millis, true),
    })
}

fn session(path: &Path, root: &Path) -> Result<Option<ChatSession>> {
    let mut question = String::new();
    let mut has_user_message = false;
    visit(path, |event| {
        if !primary_message(&event) || field(&event, "type") != "user" {
            return Ok(true);
        }
        let content = &event["message"]["content"];
        question = title_text(&clean_text(&text(content), "user")?)?;
        has_user_message = !question.is_empty()
            || content
                .as_array()
                .is_some_and(|blocks| blocks.iter().any(|block| field(block, "type") == "image"));
        Ok(!has_user_message)
    })?;
    if !has_user_message {
        return Ok(None);
    }
    let details = details(path, root)?;
    let title = if details.title.is_empty() {
        fallback_title(&question, &details.updated)
    } else {
        details.title
    };
    Ok(Some(ChatSession {
        id: session_id(path).unwrap().to_owned(),
        title,
        project: details.project,
        updated: details.updated,
        paths: vec![path.to_string_lossy().into_owned()],
    }))
}

pub(super) fn catalog(root: &Path, catalog: &mut ChatCatalog) {
    let locations: Vec<PathBuf> = ["Claude", "Claude-3p"]
        .into_iter()
        .map(|app| root.join(app).join("local-agent-mode-sessions"))
        .filter_map(|path| path.canonicalize().ok())
        .filter(|path| path.starts_with(root) && path.is_dir())
        .collect();
    let is_application_data = dirs::data_dir()
        .and_then(|path| path.canonicalize().ok())
        .as_deref()
        == Some(root);
    let locations = if !is_application_data && locations.is_empty() {
        vec![root.to_owned()]
    } else {
        locations
    };
    for location in locations {
        for entry in WalkDir::new(location)
            .follow_root_links(false)
            .max_depth(4)
            .into_iter()
            .filter_entry(|entry| {
                !matches!(
                    entry.file_name().to_str(),
                    Some("outputs" | "uploads" | "subagents")
                )
            })
        {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    catalog.issues.push(ChatImportIssue {
                        path: error.path().unwrap_or(root).display().to_string(),
                        error: error.to_string(),
                    });
                    continue;
                }
            };
            let path = entry.path();
            if !entry.file_type().is_file() || !transcript(path, ChatSource::ClaudeDesktop) {
                continue;
            }
            match session(path, root) {
                Ok(Some(item)) => catalog.sessions.push(item),
                Ok(None) => {}
                Err(error) => catalog.issues.push(ChatImportIssue {
                    path: path.display().to_string(),
                    error: error.to_string(),
                }),
            }
        }
    }
    catalog
        .sessions
        .sort_by(|a, b| b.updated.cmp(&a.updated).then(a.id.cmp(&b.id)));
    let mut seen = HashSet::new();
    catalog.sessions.retain(|item| seen.insert(item.id.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{chat_import, store::Store};
    use serde_json::json;
    use std::sync::mpsc;

    #[test]
    fn imports_current_and_legacy_cowork_logs_without_internal_messages() {
        let temporary = tempfile::tempdir().unwrap();
        let dir = temporary.path();
        let source = dir.join("profile");
        let known_session = "local_c56cca0c-9dcc-4d2a-9b30-e61f37f53d65";
        let session = source
            .join("Claude-3p/local-agent-mode-sessions/account/workspace")
            .join(known_session);
        let legacy = source.join("Claude/local-agent-mode-sessions/account/workspace/abcdef12");
        let unrelated = source.join("Other App/account/12345678");
        for path in [&session, &legacy, &unrelated] {
            fs::create_dir_all(path).unwrap();
        }
        let timestamp = "2026-11-22T02:00:00.000Z";
        let rows = [
            json!({"type":"system","message":{"content":"hidden-system"}}),
            json!({"type":"user","isSynthetic":true,"message":{"role":"user","content":"hidden-synthetic"}}),
            json!({"type":"user","isReplay":true,"message":{"role":"user","content":"hidden-replay"}}),
            json!({"type":"user","timestamp":timestamp,"message":{"role":"user","content":"<uploaded_files>hidden-upload</uploaded_files>\n实际提问"}}),
            json!({"type":"assistant","parent_tool_use_id":"child","message":{"role":"assistant","content":"hidden-child"}}),
            json!({"type":"assistant","timestamp":timestamp,"message":{"role":"assistant","content":[
                {"type":"thinking","thinking":"hidden-thinking"},
                {"type":"text","text":"实际回答"},
                {"type":"tool_use","name":"Read","input":{"file":"tool-marker"}},
            ]}}),
        ];
        let body = rows
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        for path in [&session, &legacy, &unrelated] {
            fs::write(path.join("audit.jsonl"), &body).unwrap();
        }
        fs::write(session.with_extension("json"), json!({
            "title":"已保存标题","systemPrompt":"hidden-settings","userSelectedFolders":["/example"],
            "lastActivityAt":1_700_000_000_000_i64
        }).to_string()).unwrap();

        let source = source.to_str().unwrap();
        let catalog = chat_import::list(ChatSource::ClaudeDesktop, Some(source)).unwrap();
        assert!(catalog.issues.is_empty());
        assert_eq!(catalog.total, 2);
        let selected = catalog
            .sessions
            .iter()
            .find(|item| item.id == known_session)
            .unwrap();
        assert_eq!(selected.title, "已保存标题");
        assert_eq!(selected.project, "/example");
        let legacy = catalog
            .sessions
            .iter()
            .find(|item| item.id == "abcdef12")
            .unwrap();
        assert_eq!(legacy.title, "实际提问");
        for tools in [false, true] {
            let preview =
                chat_import::preview(ChatSource::ClaudeDesktop, source, &selected.paths, tools)
                    .unwrap();
            assert_eq!(preview.title, selected.title);
            assert!(preview.body.contains("实际提问") && preview.body.contains("实际回答"));
            assert_eq!(preview.body.contains("tool-marker"), tools);
            assert!(!preview.body.contains("hidden-"));
        }
        let store = Store::open(&dir.join("data/db.sqlite")).unwrap();
        let (tx, _rx) = mpsc::channel();
        let archive = chat_import::destination(
            &store,
            &tx,
            &dir.join("Documents"),
            &store.settings().unwrap(),
        )
        .unwrap();
        let imported = chat_import::import(
            &store,
            &tx,
            ChatSource::ClaudeDesktop,
            source,
            selected,
            archive.to_str().unwrap(),
            false,
        )
        .unwrap();
        assert_eq!(
            Path::new(&imported.path).parent().unwrap(),
            archive.join("claude desktop")
        );
        assert!(fs::read_to_string(imported.path)
            .unwrap()
            .contains("实际回答"));
    }
}
