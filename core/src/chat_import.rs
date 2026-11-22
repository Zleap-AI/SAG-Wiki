//! Local transcript import. Event names and Codex compressed-twin discovery
//! adapted from claude-code-history-viewer 67979b4 (MIT); see third-party notice.
mod claude_desktop;
pub(crate) mod claude_deliverables;
pub(crate) mod schedule;

use crate::{
    indexer::IndexRequest,
    store::{hash, Store},
};
use anyhow::{bail, ensure, Context, Result};
use base64::Engine;
use rusqlite::{params, OptionalExtension};
use sag_protocol::{
    ChatCatalog, ChatListQuery, ChatImportIssue, ChatImportItem, ChatPreview, ChatSession, ChatSource,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    sync::mpsc,
};

fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}
pub(crate) fn label(source: ChatSource) -> &'static str {
    match source {
        ChatSource::Claude => "Claude Code",
        ChatSource::ClaudeDesktop => "Claude Desktop",
        ChatSource::Codex => "Codex",
        ChatSource::WorkBuddy => "WorkBuddy",
    }
}
fn source_key(source: ChatSource) -> &'static str {
    match source {
        ChatSource::Claude => "claude",
        ChatSource::ClaudeDesktop => "claude-desktop",
        ChatSource::Codex => "codex",
        ChatSource::WorkBuddy => "workbuddy",
    }
}

fn visit(path: &Path, mut accept: impl FnMut(Value) -> Result<bool>) -> Result<()> {
    let file =
        fs::File::open(path).with_context(|| format!("无法读取聊天记录：{}", path.display()))?;
    let reader: Box<dyn Read> = if path.extension().is_some_and(|ext| ext == "zst") {
        Box::new(zstd::stream::read::Decoder::new(file)?)
    } else {
        Box::new(file)
    };
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    let mut number = 0;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        number += 1;
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str(&line)
            .with_context(|| format!("聊天记录第 {number} 行无效或尚未写完：{}", path.display()))?;
        if !accept(value)? { break; }
    }
    Ok(())
}

fn root(source: ChatSource, supplied: Option<&str>) -> Result<PathBuf> {
    let path = match supplied.filter(|s| !s.trim().is_empty()) {
        Some(path) => PathBuf::from(path),
        None => match source {
            ChatSource::Claude => std::env::var_os("CLAUDE_CONFIG_DIR")
                .map(PathBuf::from)
                .unwrap_or(
                    dirs::home_dir()
                        .context("无法获取用户目录")?
                        .join(".claude"),
                )
                .join("projects"),
            ChatSource::ClaudeDesktop => dirs::data_dir().context("无法获取用户目录")?,
            ChatSource::Codex => std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .unwrap_or(dirs::home_dir().context("无法获取用户目录")?.join(".codex")),
            ChatSource::WorkBuddy => dirs::home_dir()
                .context("无法获取用户目录")?
                .join(".workbuddy/projects"),
        },
    };
    ensure!(path.is_absolute(), "请选择绝对路径的聊天记录文件夹");
    Ok(path)
}
fn transcript(path: &Path, source: ChatSource) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    match source {
        ChatSource::Claude => {
            name.ends_with(".jsonl") && !name.starts_with("agent-") && name != "history.jsonl"
        }
        ChatSource::ClaudeDesktop => name == "audit.jsonl" && claude_desktop::session_id(path).is_some(),
        ChatSource::Codex => {
            name.starts_with("rollout-")
                && (name.ends_with(".jsonl") || name.ends_with(".jsonl.zst"))
                && !(name.ends_with(".zst") && path.with_extension("").exists())
        }
        ChatSource::WorkBuddy => name
            .strip_suffix(".jsonl")
            .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok()),
    }
}
fn workbuddy_timestamp(value: &Value) -> String {
    value["timestamp"].as_i64()
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map_or_else(|| field(value, "timestamp").to_owned(), |time| time.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}
fn checked_paths(source: ChatSource, source_root: &str, paths: &[String]) -> Result<Vec<PathBuf>> {
    ensure!(!paths.is_empty(), "请选择聊天记录");
    let root = root(source, Some(source_root))?.canonicalize()?;
    paths
        .iter()
        .map(|value| {
            let p = Path::new(value);
            ensure!(
                !fs::symlink_metadata(p)?.file_type().is_symlink(),
                "聊天记录不能是符号链接"
            );
            let p = p.canonicalize()?;
            ensure!(
                p.starts_with(&root) && p.is_file() && transcript(&p, source),
                "聊天记录不在所选文件夹内或格式不受支持"
            );
            Ok(p)
        })
        .collect()
}
fn text(value: &Value) -> String {
    if let Some(s) = value.as_str() {
        return s.to_owned();
    }
    if let Some(items) = value.as_array() {
        return items
            .iter()
            .filter_map(|v| v["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
    }
    String::new()
}
// Runtime envelopes can share a message with a real request. Only strip known
// envelopes outside Markdown fences; examples quoted by the user stay intact.
fn clean_text(value: &str, role: &str) -> Result<String> {
    let s = value.trim_start();
    if role == "user" && [
        "<INSTRUCTIONS>", "# AGENTS.md instructions for",
        "This session is being continued from a previous conversation",
    ].iter().any(|prefix| s.starts_with(prefix)) {
        return Ok(String::new());
    }
    if role == "user" && s.starts_with("Side conversation boundary.")
        && s.contains("Everything before this boundary is inherited history from the parent thread.") {
        return Ok(String::new());
    }
    let value = if role == "user" && s.starts_with("# Files mentioned by the user:") {
        s.split_once("\n## My request:\n").map_or("", |(_, request)| request)
    } else { value };
    let mut remaining = value;
    let mut output = String::new();
    let mut fence: Option<(char, usize)> = None;
    while !remaining.is_empty() {
        let trimmed = remaining.trim_start_matches([' ', '\t']);
        let marker = trimmed.chars().next().filter(|c| *c == '`' || *c == '~');
        if let Some(marker) = marker {
            let count = trimmed.chars().take_while(|c| *c == marker).count();
            if count >= 3 {
                match fence {
                    None => fence = Some((marker, count)),
                    Some((open, length)) if open == marker && count >= length => fence = None,
                    _ => {},
                }
            }
        } else if fence.is_none() && trimmed.starts_with('<') {
            let tag = trimmed[1..].split(['>', ' ', '\t', '\n']).next().unwrap();
            let internal = match tag {
                "thinking" => role == "assistant",
                "heartbeat" => true,
                "environment_context" | "permissions" | "skills_instructions" |
                "turn_aborted" | "local-command-caveat" | "command-name" |
                "command-message" | "command-args" | "local-command-stdout" |
                "local-command-stderr" | "task-notification" | "subagent_notification" |
                "cross-session-message" | "system-reminder" | "in-app-browser-context" |
                "external_codex_apps_open_page" | "uploaded_files" => role == "user",
                "skill" => role == "user" && trimmed.starts_with("<skill>")
                    && trimmed.contains("<name>") && trimmed.contains("<path>"),
                _ => role == "user" && tag.starts_with("external_codex_apps_selected_page_part_"),
            };
            if internal || (role == "user" && tag == "send_user_message_question_reply") {
                let close = format!("</{tag}>");
                if let Some(end) = trimmed.find(&close) {
                    if tag == "send_user_message_question_reply" {
                        let start = trimmed.find('>').unwrap() + 1;
                        let replies: Vec<Value> = serde_json::from_str(&trimmed[start..end])
                            .context("聊天中的用户回复格式无效")?;
                        for reply in replies {
                            let question = reply["question"].as_str().context("聊天中的用户回复缺少问题")?;
                            let answer = reply["answer"].as_str().context("聊天中的用户回复缺少回答")?;
                            output.push_str(&format!("{question}\n\n{answer}\n\n"));
                        }
                    }
                    remaining = &trimmed[end + close.len()..];
                    continue;
                }
                // Older Claude transcripts also contain unfinished thinking-only blocks.
                if tag == "thinking" {
                    break;
                }
            }
        }
        let end = remaining.find('\n').map_or(remaining.len(), |i| i + 1);
        output.push_str(&remaining[..end]);
        remaining = &remaining[end..];
    }
    Ok(output.trim().to_owned())
}

#[cfg(test)]
fn list(source: ChatSource, supplied: Option<&str>) -> Result<ChatCatalog> {
    list_page(source, supplied, &ChatListQuery::default())
}

fn title_text(value: &str) -> Result<String> {
    let title = clean_text(value, "user")?;
    if title.starts_with("![Image](") || title == "[Image unavailable]" {
        return Ok(String::new());
    }
    Ok(title.lines().next().unwrap_or("").trim().to_owned())
}

fn fallback_title(question: &str, timestamp: &str) -> String {
    if !question.is_empty() { return question.to_owned(); }
    format!("图片对话 · {}", timestamp.get(..10).unwrap_or(timestamp))
}

fn paginate(mut catalog: ChatCatalog, query: &ChatListQuery) -> ChatCatalog {
    catalog.projects = catalog.sessions.iter().map(|s| s.project.clone()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    let search = query.search.to_lowercase();
    catalog.sessions.retain(|s| (query.project.is_empty() || s.project == query.project)
        && format!("{} {}", s.title, s.project).to_lowercase().contains(&search));
    catalog.total = catalog.sessions.len();
    catalog.page = query.page.min(catalog.total.saturating_sub(1) / catalog.page_size);
    catalog.sessions = catalog.sessions.into_iter().skip(catalog.page * catalog.page_size).take(catalog.page_size).collect();
    catalog
}

// Use Codex metadata for titles; inspect only unnamed chats with user messages.
// Filenames identify rollover fragments; all matching fragments remain available to import.
fn codex_catalog(root: &Path, session_id: Option<&str>) -> Result<Option<Vec<ChatSession>>> {
    let mut databases = fs::read_dir(root)?.collect::<std::io::Result<Vec<_>>>()?.into_iter().map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("state_") && n.ends_with(".sqlite")))
        .collect::<Vec<_>>();
    databases.sort_by_key(|p| p.file_stem().and_then(|n| n.to_str()).and_then(|n| n.strip_prefix("state_")).and_then(|n| n.parse::<u32>().ok()).unwrap_or(0));
    let Some(database) = databases.last() else { return Ok(None) };
    let db = rusqlite::Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let columns = db.prepare("PRAGMA table_info(threads)")?.query_map([], |row| row.get::<_, String>(1))?.collect::<rusqlite::Result<HashSet<_>>>()?;
    if !["id", "title", "cwd", "updated_at"].iter().all(|key| columns.contains(*key)) { return Ok(None) }
    let mut groups = BTreeMap::<String, ChatSession>::new();
    let name = if columns.contains("name") { "name" } else { "NULL" };
    let user_event = if columns.contains("has_user_event") { "has_user_event" } else { "1" };
    let mut no_user_event = HashSet::new();
    let mut rows = db.prepare(&format!("SELECT id,title,cwd,updated_at,{name},{user_event} FROM threads WHERE ?1 IS NULL OR id = ?1 ORDER BY updated_at DESC,id"))?;
    for row in rows.query_map([session_id], |row| {
        let id: String = row.get(0)?;
        let title: String = row.get::<_, Option<String>>(4)?.filter(|s| !s.trim().is_empty()).unwrap_or(row.get(1)?);
        let updated: i64 = row.get(3)?;
        Ok((id, title, row.get::<_, String>(2)?, updated, row.get::<_, bool>(5)?))
    })? {
        let (id, title, project, updated, has_user_event) = row?;
        if !has_user_event { no_user_event.insert(id.clone()); }
        let updated = chrono::DateTime::from_timestamp(updated, 0).context("Codex 会话时间无效")?.to_rfc3339();
        groups.insert(id.clone(), ChatSession { title, id, project, updated, paths: vec![] });
    }
    for directory in [root.join("sessions"), root.join("archived_sessions")] {
        if !directory.exists() { continue }
        for entry in walkdir::WalkDir::new(directory).follow_links(false) {
            let entry = entry?;
            if !entry.file_type().is_file() || !transcript(entry.path(), ChatSource::Codex) { continue }
            let name = entry.file_name().to_string_lossy();
            let stem = name.strip_suffix(".jsonl.zst").or_else(|| name.strip_suffix(".jsonl")).unwrap();
            if let Some(id) = stem.get(stem.len().saturating_sub(36)..) {
                if let Some(session) = groups.get_mut(id) { session.paths.push(entry.path().to_string_lossy().into_owned()); }
            }
        }
    }
    let mut sessions = groups.into_values().filter(|s| !s.paths.is_empty()).collect::<Vec<_>>();
    for session in &mut sessions {
        session.paths.sort();
        session.title = title_text(&session.title)?;
        if session.title == session.id { session.title.clear(); }
        if session.title.is_empty() && !no_user_event.contains(&session.id) {
            for path in &session.paths {
                visit(Path::new(path), |v| {
                    let p = &v["payload"];
                    let candidate = if field(&v, "type") == "response_item" && field(p, "role") == "user" {
                        text(&p["content"])
                    } else if field(&v, "type") == "event_msg" && field(p, "type") == "user_message" {
                        field(p, "message").to_owned()
                    } else { return Ok(true); };
                    session.title = title_text(&candidate)?;
                    Ok(session.title.is_empty())
                })?;
                if !session.title.is_empty() { break; }
            }
        }
        session.title = fallback_title(&session.title, &session.updated);
    }
    sessions.sort_by(|a,b| b.updated.cmp(&a.updated).then(a.id.cmp(&b.id)));
    Ok(Some(sessions))
}

pub fn list_page(source: ChatSource, supplied: Option<&str>, query: &ChatListQuery) -> Result<ChatCatalog> {
    Ok(paginate(catalog(source, supplied)?, query))
}

fn catalog(source: ChatSource, supplied: Option<&str>) -> Result<ChatCatalog> {
    let root = root(source, supplied)?;
    let mut catalog = ChatCatalog {
        source_root: root.to_string_lossy().into(),
        sessions: vec![],
        issues: vec![],
        total: 0, page: 0, page_size: 50, projects: vec![],
    };
    if !root.exists() && supplied.is_none() {
        return Ok(catalog);
    }
    ensure!(root.is_dir(), "聊天记录文件夹不存在");
    let root = root.canonicalize()?;
    catalog.source_root = root.to_string_lossy().into();
    if source == ChatSource::ClaudeDesktop {
        claude_desktop::catalog(&root, &mut catalog);
        return Ok(catalog);
    }
    if source == ChatSource::Codex {
        if let Some(sessions) = codex_catalog(&root, None)? {
            catalog.sessions = sessions;
            return Ok(catalog);
        }
    }
    let mut groups: BTreeMap<String, ChatSession> = BTreeMap::new();
    for entry in walkdir::WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.file_name() != "subagents")
    {
        let entry = match entry {
            Ok(v) => v,
            Err(e) => {
                catalog.issues.push(ChatImportIssue {
                    path: e.path().unwrap_or(&root).display().to_string(),
                    error: e.to_string(),
                });
                continue;
            }
        };
        if !entry.file_type().is_file() || !transcript(entry.path(), source) {
            continue;
        }
        let path = entry.path();
        let mut id = String::new();
        let mut title = String::new();
        let mut project = String::new();
        let mut updated = String::new();
        let mut started = String::new();
        let mut workbuddy_title = String::new();
        let mut workbuddy_has_messages = false;
        let result = visit(path, |v| {
            let workbuddy_time;
            let ts = if source == ChatSource::WorkBuddy {
                workbuddy_time = workbuddy_timestamp(&v);
                &workbuddy_time
            } else {
                field(&v, "timestamp")
            };
            if started.is_empty() && !ts.is_empty() {
                started = ts.into();
            }
            if ts > updated.as_str() {
                updated = ts.into();
            }
            match source {
                ChatSource::Claude | ChatSource::ClaudeDesktop => {
                    if id.is_empty() {
                        id = field(&v, "sessionId").into();
                    }
                    if project.is_empty() {
                        project = field(&v, "cwd").into();
                    }
                    if field(&v, "type") == "user" && title.is_empty() && v["isMeta"] != true && v["isSidechain"] != true && v["isCompactSummary"] != true {
                        title = title_text(&text(&v["message"]["content"]))?;
                    }
                    if field(&v, "type") == "custom-title" {
                        title = title_text(field(&v, "customTitle"))?;
                    }
                }
                ChatSource::Codex => {
                    if field(&v, "type") == "session_meta" && id.is_empty() {
                        id = field(&v["payload"], "id").into();
                        project = field(&v["payload"], "cwd").into();
                    }
                    if title.is_empty()
                        && field(&v, "type") == "response_item"
                        && field(&v["payload"], "role") == "user"
                    {
                        title = title_text(&text(&v["payload"]["content"]))?;
                    }
                    if title.is_empty() && field(&v["payload"], "type") == "user_message" {
                        title = title_text(field(&v["payload"], "message"))?;
                    }
                }
                ChatSource::WorkBuddy => {
                    let kind = field(&v, "type");
                    let role = field(&v, "role");
                    if id.is_empty() {
                        id = field(&v, "sessionId").into();
                    }
                    if project.is_empty() {
                        project = field(&v, "cwd").into();
                    }
                    if kind == "ai-title" {
                        workbuddy_title = title_text(field(&v, "aiTitle"))?;
                    } else if kind == "message" && matches!(role, "user" | "assistant") {
                        let body = clean_text(&text(&v["content"]), role)?;
                        workbuddy_has_messages |= !body.is_empty();
                        if role == "user" && title.is_empty() {
                            title = title_text(&body)?;
                        }
                    }
                }
            }
            Ok(true)
        });
        if let Err(e) = result {
            catalog.issues.push(ChatImportIssue {
                path: path.display().to_string(),
                error: e.to_string(),
            });
            continue;
        }
        if id.is_empty() || (source == ChatSource::WorkBuddy && !workbuddy_has_messages) {
            continue;
        }
        if !workbuddy_title.is_empty() {
            title = workbuddy_title;
        }
        let file = path.to_string_lossy().into_owned();
        let session = groups.entry(id.clone()).or_insert_with(|| ChatSession {
            id,
            title: String::new(),
            project: project.clone(),
            updated: String::new(),
            paths: vec![],
        });
        if session.title.is_empty() {
            session.title = title;
        }
        if session.project.is_empty() {
            session.project = project;
        }
        if updated > session.updated {
            session.updated = updated;
        }
        // Keep separate rollover files in timestamp order without assuming their names encode order.
        session.paths.push(format!("{started}\n{file}"));
    }
    for mut session in groups.into_values() {
        session.paths.sort();
        session.paths = session
            .paths
            .into_iter()
            .map(|s| s.split_once('\n').unwrap().1.to_owned())
            .collect();
        if session.title.is_empty() {
            session.title = fallback_title("", &session.updated);
        }
        catalog.sessions.push(session);
    }
    catalog.sessions.sort_by(|a, b| b.updated.cmp(&a.updated));
    Ok(catalog)
}

#[derive(Clone)]
struct Message {
    id: String,
    role: String,
    body: String,
    timestamp: String,
}
struct Parsed {
    id: String,
    title: String,
    project: String,
    messages: Vec<Message>,
    parent: Option<String>,
    positions: HashMap<String, usize>,
    assets: BTreeMap<String, Vec<u8>>,
}
fn fenced(value: &str) -> String {
    let n = value
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or(0)
        .max(2)
        + 1;
    let fence = "`".repeat(n);
    format!("{fence}\n{value}\n{fence}")
}
fn blocks(
    value: &Value,
    role: &str,
    include_tools: bool,
    assets: &mut BTreeMap<String, Vec<u8>>,
) -> Result<String> {
    if let Some(s) = value.as_str() {
        return clean_text(s, role);
    }
    let Some(items) = value.as_array() else {
        return Ok(String::new());
    };
    let mut output = Vec::new();
    for item in items {
        let block = match field(item, "type") {
            "text" | "input_text" | "output_text" => Some(clean_text(field(item, "text"), role)?),
            "image" | "input_image" => {
                let source = &item["source"];
                let url = if field(source, "type") == "base64" {
                    format!(
                        "data:{};base64,{}",
                        field(source, "media_type"),
                        field(source, "data")
                    )
                } else {
                    item["image_url"]
                        .as_str()
                        .or_else(|| source["url"].as_str())
                        .unwrap_or("")
                        .to_owned()
                };
                if let Some((header, data)) = url.split_once(",").filter(|(header, _)| {
                    header.starts_with("data:image/") && header.ends_with(";base64")
                }) {
                    let extension = match header {
                        "data:image/png;base64" => "png",
                        "data:image/jpeg;base64" => "jpg",
                        "data:image/webp;base64" => "webp",
                        "data:image/gif;base64" => "gif",
                        _ => bail!("不支持的聊天图片格式"),
                    };
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(data)
                        .context("聊天图片数据无效")?;
                    let name = format!("{}.{}", hash(&bytes), extension);
                    assets.insert(name.clone(), bytes);
                    Some(format!("![Image](chat-assets/{name})"))
                } else if url.starts_with("https://") || url.starts_with("http://") {
                    Some(format!(
                        "![Image](<{}>)",
                        url.replace('>', "%3E").replace('\n', "")
                    ))
                } else {
                    Some("[Image unavailable]".into())
                }
            }
            "tool_use" | "server_tool_use" | "mcp_tool_use" if include_tools => Some(format!(
                "### Tool: {}\n\n{}",
                field(item, "name"),
                fenced(&serde_json::to_string_pretty(&item["input"]).unwrap())
            )),
            "tool_result" | "mcp_tool_result" if include_tools => Some(format!(
                "### Tool result\n\n{}",
                fenced(&if item["content"].is_string() {
                    text(&item["content"])
                } else {
                    serde_json::to_string_pretty(&item["content"]).unwrap()
                })
            )),
            _ => None,
        };
        if let Some(block) = block.filter(|b| !b.is_empty()) {
            output.push(block);
        }
    }
    Ok(output.join("\n\n"))
}
fn parse(source: ChatSource, path: &Path, include_tools: bool) -> Result<Parsed> {
    let desktop = source == ChatSource::ClaudeDesktop;
    let mut out = Parsed {
        id: if desktop {
            claude_desktop::session_id(path).context("聊天记录缺少会话标识")?.to_owned()
        } else {
            String::new()
        },
        title: String::new(),
        project: String::new(),
        messages: vec![],
        parent: None,
        positions: HashMap::new(),
        assets: BTreeMap::new(),
    };
    let mut events = vec![];
    visit(path, |v| {
        let timestamp = if source == ChatSource::WorkBuddy {
            workbuddy_timestamp(&v)
        } else {
            field(&v, "timestamp").to_owned()
        };
        match source {
            ChatSource::Claude | ChatSource::ClaudeDesktop => {
                if desktop && !claude_desktop::primary_message(&v) {
                    return Ok(true);
                }
                let id = field(&v, "uuid").to_owned();
                if !id.is_empty() {
                    out.positions.insert(id.clone(), out.messages.len());
                }
                if out.id.is_empty() {
                    out.id = field(&v, "sessionId").into();
                }
                if out.project.is_empty() {
                    out.project = field(&v, "cwd").into();
                }
                if field(&v, "subtype") == "compact_boundary"
                    && !field(&v, "logicalParentUuid").is_empty()
                {
                    out.parent = Some(field(&v, "logicalParentUuid").into());
                }
                if field(&v, "type") == "custom-title" {
                    out.title = title_text(field(&v, "customTitle"))?;
                }
                let role = field(&v, "type");
                if matches!(role, "user" | "assistant")
                    && v["isMeta"] != true
                    && v["isSidechain"] != true
                    && v["isCompactSummary"] != true
                {
                    let content = v
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .or_else(|| v.get("content"))
                        .unwrap_or(&Value::Null);
                    let body = blocks(content, role, include_tools, &mut out.assets)?;
                    if !body.trim().is_empty() {
                        out.messages.push(Message {
                            id,
                            role: role.into(),
                            body,
                            timestamp,
                        });
                    }
                }
            }
            ChatSource::Codex => {
                let p = &v["payload"];
                match field(&v, "type") {
                    "session_meta" if out.id.is_empty() => {
                        out.id = field(p, "id").into();
                        out.project = field(p, "cwd").into();
                    }
                    "response_item" => {
                        let kind = field(p, "type");
                        let role = field(p, "role");
                        if kind == "message" && matches!(role, "user" | "assistant") {
                            let body = blocks(&p["content"], role, include_tools, &mut out.assets)?;
                            if !body.trim().is_empty() {
                                out.messages.push(Message {
                                    id: field(p, "id").into(),
                                    role: role.into(),
                                    body,
                                    timestamp,
                                });
                            }
                        } else if include_tools
                            && matches!(
                                kind,
                                "function_call"
                                    | "custom_tool_call"
                                    | "local_shell_call"
                                    | "function_call_output"
                                    | "custom_tool_call_output"
                                    | "web_search_call"
                            )
                        {
                            out.messages.push(Message {
                                id: field(p, "id").into(),
                                role: "tool".into(),
                                body: format!(
                                    "### {} {}\n\n{}",
                                    kind,
                                    field(p, "name"),
                                    fenced(&serde_json::to_string_pretty(p)?)
                                ),
                                timestamp,
                            });
                        }
                    }
                    "event_msg" => {
                        let (role, body, id) = match field(p, "type") {
                            "user_message" => {
                                ("user", field(p, "message").to_owned(), String::new())
                            }
                            "agent_message" => {
                                ("assistant", field(p, "message").to_owned(), String::new())
                            }
                            "item_completed" => {
                                let item = &p["item"];
                                let role = match field(item, "type") {
                                    "UserMessage" => "user",
                                    "AgentMessage" => "assistant",
                                    _ => "",
                                };
                                (
                                    role,
                                    if role == "assistant" {
                                        field(item, "text").to_owned()
                                    } else {
                                        text(&item["content"])
                                    },
                                    field(item, "id").to_owned(),
                                )
                            }
                            _ => ("", String::new(), String::new()),
                        };
                        let body = clean_text(&body, role)?;
                        if !role.is_empty() && !body.trim().is_empty() {
                            events.push(Message {
                                id,
                                role: role.into(),
                                body,
                                timestamp,
                            });
                        }
                    }
                    "thread_item" => bail!("暂不支持此聊天记录版本：thread_item"),
                    _ => {}
                }
            }
            ChatSource::WorkBuddy => {
                if out.id.is_empty() {
                    out.id = field(&v, "sessionId").into();
                }
                if out.project.is_empty() {
                    out.project = field(&v, "cwd").into();
                }
                let kind = field(&v, "type");
                let role = field(&v, "role");
                let (role, body) = match kind {
                    "ai-title" => {
                        out.title = title_text(field(&v, "aiTitle"))?;
                        return Ok(true);
                    }
                    "message" if matches!(role, "user" | "assistant") => {
                        (role, blocks(&v["content"], role, include_tools, &mut out.assets)?)
                    }
                    "function_call" | "function_call_result" if include_tools => {
                        let value = &v[if kind == "function_call" { "arguments" } else { "output" }];
                        let body = match value.as_str() {
                            Some(text) => text.to_owned(),
                            None => serde_json::to_string_pretty(value)?,
                        };
                        ("tool", format!("### {} {}\n\n{}", kind, field(&v, "name"), fenced(&body)))
                    }
                    _ => return Ok(true),
                };
                if !body.is_empty() {
                    out.messages.push(Message {
                        id: field(&v, "id").to_owned(),
                        role: role.into(),
                        body,
                        timestamp,
                    });
                }
            }
        }
        Ok(true)
    })?;
    if source == ChatSource::Codex {
        // Match mirror events one-for-one; repeated genuine prompts must survive.
        let mut mirrors: HashMap<(String, String), usize> = HashMap::new();
        for m in &out.messages {
            *mirrors.entry((m.role.clone(), m.body.clone())).or_default() += 1;
        }
        for e in events {
            let remaining = mirrors.entry((e.role.clone(), e.body.clone())).or_default();
            if *remaining > 0 {
                *remaining -= 1;
            } else {
                out.messages.push(e);
            }
        }
        out.messages.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    }
    Ok(out)
}

fn load(source: ChatSource, root: &str, paths: &[String], include_tools: bool) -> Result<Parsed> {
    let paths = checked_paths(source, root, paths)?;
    let mut combined = Parsed {
        id: String::new(),
        title: String::new(),
        project: String::new(),
        messages: vec![],
        parent: None,
        positions: HashMap::new(),
        assets: BTreeMap::new(),
    };
    let mut seen_files = HashSet::new();
    let mut seen_messages = HashSet::new();
    for path in paths {
        let mut parsed = parse(source, &path, include_tools)?;
        if source == ChatSource::ClaudeDesktop {
            let details = claude_desktop::details(&path, &Path::new(root).canonicalize()?)?;
            parsed.title = details.title;
            if !details.project.is_empty() {
                parsed.project = details.project;
            }
        }
        let mut chain = vec![(path.clone(), parsed)];
        seen_files.insert(path.clone());
        if source == ChatSource::Claude {
            loop {
                let (_, current) = chain.last().unwrap();
                let Some(parent) = current
                    .parent
                    .clone()
                    .filter(|p| !current.positions.contains_key(p))
                else {
                    break;
                };
                let directory = path.parent().unwrap();
                let mut predecessor = None;
                for entry in fs::read_dir(directory)? {
                    let p = entry?.path();
                    if !transcript(&p, source) || seen_files.contains(&p) {
                        continue;
                    }
                    let p = checked_paths(source, root, &[p.to_string_lossy().into()])?.remove(0);
                    let mut parsed = parse(source, &p, include_tools)?;
                    if let Some(&boundary) = parsed.positions.get(&parent) {
                        let keep = boundary
                            + usize::from(
                                parsed
                                    .messages
                                    .get(boundary)
                                    .is_some_and(|message| message.id == parent),
                            );
                        parsed.messages.truncate(keep);
                        seen_files.insert(p.clone());
                        predecessor = Some((p, parsed));
                        break;
                    }
                }
                let Some(previous) = predecessor else {
                    bail!("聊天记录缺少续接文件：{parent}");
                };
                chain.push(previous);
            }
        }
        for (_, parsed) in chain.into_iter().rev() {
            if combined.id.is_empty() {
                combined.id = parsed.id.clone();
                combined.project = parsed.project.clone();
            }
            // The leaf's session identity is used for continuation chains.
            if source == ChatSource::Claude {
                combined.id = parsed.id.clone();
            } else {
                ensure!(combined.id == parsed.id, "选择的文件不属于同一会话");
            }
            if !parsed.title.is_empty() { combined.title = parsed.title.clone(); }
            combined.assets.extend(parsed.assets);
            for message in parsed.messages {
                let key = if message.id.is_empty() {
                    format!("{}\n{}\n{}", message.timestamp, message.role, message.body)
                } else {
                    message.id.clone()
                };
                if seen_messages.insert(key) {
                    combined.messages.push(message);
                }
            }
        }
    }
    ensure!(!combined.id.is_empty(), "聊天记录缺少会话标识");
    ensure!(
        !combined.messages.is_empty(),
        "聊天记录没有可导入的问答内容"
    );
    if source == ChatSource::Codex {
        if let Some(catalog) = codex_catalog(Path::new(root), Some(&combined.id))? {
            if let Some(session) = catalog.into_iter().find(|s| s.id == combined.id) {
                combined.title = session.title;
            }
        }
    }
    Ok(combined)
}
fn render(source: ChatSource, parsed: &Parsed) -> ChatPreview {
    let first = parsed.messages.iter().find(|m| m.role == "user");
    let question = first.map(|m| title_text(&m.body).expect("parsed text is already cleaned")).unwrap_or_default();
    let title = if parsed.title.is_empty() {
        fallback_title(&question, first.map_or("", |m| m.timestamp.as_str()))
    } else { parsed.title.clone() };
    let mut body = format!("# {title}\n\n{} · {}\n\n", label(source), parsed.project);
    for m in &parsed.messages {
        let role = match m.role.as_str() {
            "user" => "User",
            "tool" => "Tool",
            _ => "Assistant",
        };
        body.push_str(&format!("## {role} · {}\n\n{}\n\n", m.timestamp, m.body));
    }
    ChatPreview { title, body }
}
pub fn preview(
    source: ChatSource,
    root: &str,
    paths: &[String],
    include_tools: bool,
) -> Result<ChatPreview> {
    Ok(render(source, &load(source, root, paths, include_tools)?))
}

pub fn destination_path(documents: &Path, settings: &sag_protocol::types::Settings) -> PathBuf {
    settings.chat_import_folder.as_ref().map(PathBuf::from).unwrap_or_else(|| documents.join("sag-wiki").join("agent-history"))
}

pub fn set_destination(store: &Store, path: &Path) -> Result<String> {
    let path = path.canonicalize().context("无法打开这个文件夹")?;
    ensure!(path.is_dir(), "请选择文件夹");
    let path = path.to_string_lossy().into_owned();
    let mut settings = store.settings()?;
    settings.chat_import_folder = Some(path.clone());
    store.save_settings(&settings)?;
    Ok(path)
}

pub fn destination(store: &Store, scan: &mpsc::Sender<IndexRequest>, documents: &Path, settings: &sag_protocol::types::Settings) -> Result<PathBuf> {
    let folder = destination_path(documents, settings);
    fs::create_dir_all(&folder)?;
    let folder = folder.canonicalize()?;
    if !store.roots()?.iter().any(|root| folder.starts_with(&root.path)) {
        crate::indexer::add_root(store, scan, &folder)?;
    }
    Ok(folder)
}

pub fn import(
    store: &Store,
    scan: &mpsc::Sender<IndexRequest>,
    source: ChatSource,
    root: &str,
    session: &ChatSession,
    folder: &str,
    include_tools: bool,
) -> Result<ChatImportItem> {
    let parsed = load(source, root, &session.paths, include_tools)?;
    ensure!(parsed.id == session.id, "会话标识已变化，请重新加载列表");
    let folder = store.authorize(Path::new(folder))?;
    let identity = hash(
        format!(
            "{}\n{root}\n{}\n{}",
            source_key(source),
            parsed.id,
            folder.display()
        )
        .as_bytes(),
    );
    let previous: Option<(String, String)> = store
        .connection()
        .query_row(
            "SELECT path,output_hash FROM chat_imports WHERE identity=?",
            [&identity],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let preview = render(source, &parsed);
    let name: String = preview
        .title
        .chars()
        .filter(|c| !c.is_control() && !r#"/\:*?"<>|"#.contains(*c))
        .take(64)
        .collect();
    let path = previous
        .as_ref()
        .map(|(path, _)| PathBuf::from(path))
        .unwrap_or_else(|| {
            folder.join(match source {
                ChatSource::Claude => "claude code",
                ChatSource::ClaudeDesktop => "claude desktop",
                ChatSource::Codex => "codex",
                ChatSource::WorkBuddy => "workbuddy",
            }).join(format!(
                "{}-{}-{}.md",
                source_key(source),
                name.trim(),
                &identity[..12]
            ))
        });
    let path_text = path.to_string_lossy().into_owned();
    let expected = previous.as_ref().map(|(_, hash)| hash.clone());
    let output_hash = hash(preview.body.as_bytes());
    if path.exists() {
        let actual = hash(&fs::read(&path)?);
        ensure!(
            expected.as_deref() == Some(actual.as_str()),
            "导入文件已被修改，已保留现有文件"
        );
        if actual == output_hash {
            return Ok(ChatImportItem {
                path: path_text,
                status: "skipped".into(),
                error: None,
            });
        }
    }
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent)?;
    let parent = store.authorize(parent)?;
    ensure!(parent.starts_with(&folder), "导入目录不在保存位置内");
    if !parsed.assets.is_empty() {
        let directory = parent.join("chat-assets");
        if directory.exists() {
            ensure!(
                !directory.symlink_metadata()?.file_type().is_symlink(),
                "图片目录不能是符号链接"
            );
        }
        fs::create_dir_all(&directory)?;
        for (name, bytes) in &parsed.assets {
            let target = directory.join(name);
            if target.exists() {
                ensure!(fs::read(&target)? == *bytes, "聊天图片文件与原始内容不一致");
            } else {
                use std::io::Write;
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(target)?;
                file.write_all(bytes)?;
            }
        }
    }
    let document = store.save_document(
        &path,
        &preview.body,
        if path.exists() {
            expected.as_deref()
        } else {
            None
        },
    )?;
    store.connection().execute("INSERT INTO chat_imports(identity,path,output_hash) VALUES(?1,?2,?3) ON CONFLICT(identity) DO UPDATE SET output_hash=excluded.output_hash",params![identity,path_text,document.hash])?;
    let roots = store.roots()?;
    let registered = roots
        .iter()
        .find(|r| path.starts_with(&r.path))
        .context("目录未加入知识库")?;
    scan.send(IndexRequest::Paths {
        root: registered.path.clone(),
        paths: vec![path.clone()],
    })?;
    Ok(ChatImportItem {
        path: path_text,
        status: if expected.is_some() {
            "updated"
        } else {
            "imported"
        }
        .into(),
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn write(path: &Path, records: &[Value]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            records.iter().map(|v| format!("{v}\n")).collect::<String>(),
        )
        .unwrap();
    }
    fn claude(id: &str, uuid: &str, role: &str, content: Value) -> Value {
        json!({"type":role,"uuid":uuid,"sessionId":id,"cwd":"/demo/project","timestamp":"2026-11-15T01:00:00Z","message":{"role":role,"content":content}})
    }
    fn codex_meta() -> Value {
        json!({"type":"session_meta","timestamp":"2026-11-15T01:00:00Z","payload":{"id":"codex-session","cwd":"/demo/project"}})
    }
    fn codex(role: &str, id: &str, body: &str) -> Value {
        json!({"type":"response_item","timestamp":"2026-11-15T01:00:00Z","payload":{"type":"message","id":id,"role":role,"content":[{"type":"input_text","text":body}]}})
    }

    #[test]
    fn scheduled_imports_use_separate_times_and_preserve_edited_markdown() {
        use chrono::{Local, TimeZone};
        use sag_protocol::ChatImportSchedule;
        use tokio_util::sync::CancellationToken;

        let dir = tempfile::tempdir().unwrap();
        let claude_root = dir.path().join("claude");
        let codex_root = dir.path().join("codex");
        let claude_path = claude_root.join("session.jsonl");
        let codex_path = codex_root.join("rollout-demo.jsonl");
        let folder = dir.path().join("library");
        fs::create_dir(&folder).unwrap();
        let question = claude("scheduled-claude", "first", "user", json!("Claude question"));
        let answer = claude("scheduled-claude", "second", "assistant", json!([
            {"type":"text","text":"Claude response"},
            {"type":"tool_use","name":"Read","input":{"path":"claude-tool-marker"}}
        ]));
        write(&claude_path, &[question.clone(), answer.clone()]);
        write(&codex_path, &[codex_meta(), codex("user", "first", "Codex question"),
            json!({"type":"response_item","payload":{"type":"function_call","name":"Read","arguments":"codex-tool-marker"}})]);
        let database = dir.path().join("data/db.sqlite");
        let store = Store::open(&database).unwrap();
        set_destination(&store, &folder).unwrap();
        let (scan, _requests) = mpsc::channel();
        let stop = CancellationToken::new();
        let today = Local::now().date_naive().succ_opt().unwrap();
        let at = |day: chrono::NaiveDate, hour| Local.from_local_datetime(&day.and_hms_opt(hour, 0, 0).unwrap()).unwrap();
        let configure = |source, enabled, time: &str, root: &Path, include_tools| {
            schedule::save(&store, source, ChatImportSchedule {
                enabled, time: time.into(), interval_minutes: None, source_root: Some(root.display().to_string()), include_tools,
            })
        };
        assert!(!schedule::status(&store, ChatSource::Claude).unwrap().schedule.enabled);
        assert!(configure(ChatSource::Claude, true, "25:00", &claude_root, false).is_err());
        configure(ChatSource::Claude, true, "09:00", &claude_root, false).unwrap();
        configure(ChatSource::Codex, true, "10:00", &codex_root, true).unwrap();
        assert!(schedule::run_due(&store, &scan, ChatSource::Claude, at(today, 8), &stop).unwrap().is_none());
        assert!(schedule::run_due(&store, &scan, ChatSource::Codex, at(today, 9), &stop).unwrap().is_none());
        assert!(!folder.join("codex").exists());
        for (source, hour, subfolder, tool_marker, expected) in [
            (ChatSource::Claude, 9, "claude code", "claude-tool-marker", false),
            (ChatSource::Codex, 10, "codex", "codex-tool-marker", true),
        ] {
            let result = schedule::run_due(&store, &scan, source, at(today, hour), &stop).unwrap().unwrap();
            assert_eq!((result.changed, result.failed), (1, 0), "{:?}", result.error);
            let mut files = fs::read_dir(folder.join(subfolder)).unwrap();
            assert_eq!(fs::read_to_string(files.next().unwrap().unwrap().path()).unwrap().contains(tool_marker), expected);
            assert!(files.next().is_none());
        }
        let saved = schedule::status(&store, ChatSource::Claude).unwrap();
        assert_eq!(saved.last_run_at, Some(at(today, 9).timestamp_millis()));
        assert_eq!(saved.last_run.unwrap().changed, 1);
        drop(store);
        let store = Store::open(&database).unwrap();
        assert!(schedule::run_due(&store, &scan, ChatSource::Claude, at(today, 11), &stop).unwrap().is_none());
        let next = today.succ_opt().unwrap();
        let result = schedule::run_due(&store, &scan, ChatSource::Claude, at(next, 11), &stop).unwrap().unwrap();
        assert_eq!((result.changed, result.failed), (0, 0));
        let file = fs::read_dir(folder.join("claude code")).unwrap().next().unwrap().unwrap().path();
        write(&claude_path, &[question, answer, claude("scheduled-claude", "third", "assistant", json!("Later response"))]);
        let result = schedule::run_due(&store, &scan, ChatSource::Claude, at(next.succ_opt().unwrap(), 11), &stop).unwrap().unwrap();
        assert_eq!((result.changed, result.failed), (1, 0));
        assert!(fs::read_to_string(&file).unwrap().contains("Later response"));
        fs::write(&file, "My edits").unwrap();
        let result = schedule::run_due(&store, &scan, ChatSource::Claude, at(next.succ_opt().unwrap().succ_opt().unwrap(), 11), &stop).unwrap().unwrap();
        assert_eq!((result.changed, result.failed), (0, 1));
        assert_eq!(fs::read_to_string(file).unwrap(), "My edits");
    }

    #[test]
    fn claude_default_filters_tools_both_roles_and_preserves_long_text_code_and_images() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chat.jsonl");
        let long = format!(
            "{}\n```python\nprint('code-marker')\n```",
            "完整中文正文".repeat(1000)
        );
        write(
            &path,
            &[
                claude(
                    "c1",
                    "q1",
                    "user",
                    json!([{"type":"text","text":"用户问题"},{"type":"tool_result","content":"TOOL_RESULT"}]),
                ),
                claude(
                    "c1",
                    "a1",
                    "assistant",
                    json!([{"type":"text","text":long},{"type":"tool_use","name":"Read","input":{"path":"TOOL_INPUT"}},{"type":"thinking","thinking":"THINKING"}]),
                ),
                claude(
                    "c1",
                    "q2",
                    "user",
                    json!([{"type":"image","source":{"type":"base64","media_type":"image/png","data":"aW1hZ2U="}}]),
                ),
                json!({"type":"progress","sessionId":"c1","content":"PROGRESS"}),
            ],
        );
        let root = dir.path().to_str().unwrap();
        let paths = vec![path.display().to_string()];
        let plain = preview(ChatSource::Claude, root, &paths, false).unwrap();
        assert!(plain.body.contains(&long));
        assert!(plain.body.contains("用户问题"));
        assert!(plain.body.contains("chat-assets/"));
        for hidden in ["TOOL_RESULT", "TOOL_INPUT", "THINKING", "PROGRESS"] {
            assert!(!plain.body.contains(hidden));
        }
        let full = preview(ChatSource::Claude, root, &paths, true).unwrap();
        assert!(full.body.contains("TOOL_RESULT") && full.body.contains("TOOL_INPUT"));
    }
    #[test]
    fn claude_cleans_runtime_context_without_losing_conversation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claude-cleanup.jsonl");
        let mut summary = claude("c1", "summary", "user", json!("This session is being continued from a previous conversation. HIDDEN_SUMMARY"));
        summary["isCompactSummary"] = json!(true);
        let mut meta = claude("c1", "meta", "user", json!("HIDDEN_META"));
        meta["isMeta"] = json!(true);
        let mut sidechain = claude("c1", "side", "assistant", json!("HIDDEN_SIDECHAIN"));
        sidechain["isSidechain"] = json!(true);
        write(&path, &[
            claude("c1", "system", "system", json!("HIDDEN_SYSTEM")), summary, meta, sidechain,
            claude("c1", "stdout", "user", json!("<local-command-stdout>HIDDEN_STDOUT</local-command-stdout>")),
            claude("c1", "command", "user", json!("<command-message>HIDDEN_COMMAND</command-message>\n<command-name>/clear</command-name>")),
            claude("c1", "notification", "user", json!("<cross-session-message from=\"agent\">HIDDEN_NOTIFICATION</cross-session-message>")),
            claude("c1", "q1", "user", json!([
                {"type":"text","text":"<environment_context>HIDDEN_ENV</environment_context>"},
                {"type":"text","text":"保留用户问题"},
                {"type":"text","text":"<system-reminder>HIDDEN_REMINDER</system-reminder>"}
            ])),
            claude("c1", "a0", "assistant", json!("<thinking>HIDDEN_UNFINISHED_THINKING")),
            claude("c1", "a1", "assistant", json!("<thinking>HIDDEN_THINKING</thinking>\n保留助手回答")),
            claude("c1", "q2", "user", json!("请解释 system-reminder 标签\n```xml\n<system-reminder>保留代码示例</system-reminder>\n```")),
        ]);
        for tools in [false, true] {
            let result = preview(ChatSource::Claude, dir.path().to_str().unwrap(), &[path.display().to_string()], tools).unwrap();
            assert!(!result.body.contains("HIDDEN_"), "{}", result.body);
            for kept in ["保留用户问题", "保留助手回答", "保留代码示例"] { assert!(result.body.contains(kept)); }
            assert_eq!(result.title, "保留用户问题");
        }
        let catalog = list(ChatSource::Claude, dir.path().to_str()).unwrap();
        assert_eq!(catalog.sessions[0].title, "保留用户问题");
    }

    #[test]
    fn codex_cleans_runtime_context_and_preserves_user_replies() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout-cleanup.jsonl");
        write(&path, &[
            codex_meta(),
            codex("system", "system", "HIDDEN_SYSTEM"),
            codex("developer", "developer", "HIDDEN_DEVELOPER"),
            codex("user", "browser", "<in-app-browser-context source=\"ambient-ui-state\">HIDDEN_BROWSER</in-app-browser-context>"),
            codex("user", "page", "<external_codex_apps_open_page>HIDDEN_PAGE</external_codex_apps_open_page>"),
            codex("user", "selected", "<external_codex_apps_selected_page_part_1_of_1>HIDDEN_SELECTED</external_codex_apps_selected_page_part_1_of_1>"),
            codex("user", "notification", "<subagent_notification>HIDDEN_NOTIFICATION</subagent_notification>"),
            codex("user", "heartbeat", "<heartbeat>HIDDEN_HEARTBEAT</heartbeat>"),
            codex("assistant", "heartbeat-answer", "<heartbeat>HIDDEN_HEARTBEAT_REPLY</heartbeat>"),
            json!({"type":"turn_context","payload":{"cwd":"HIDDEN_CWD","instructions":"HIDDEN_INSTRUCTIONS"}}),
            json!({"type":"compacted","payload":{"message":"HIDDEN_SUMMARY"}}),
            json!({"type":"response_item","payload":{"type":"reasoning","summary":[{"type":"summary_text","text":"HIDDEN_REASONING"}]}}),
            json!({"type":"response_item","payload":{"type":"message","role":"user","content":[
                {"type":"input_text","text":"<environment_context>HIDDEN_ENV</environment_context>\n保留实际请求"},
                {"type":"input_text","text":"<skill>\n<name>test</name>\n<path>/skills/test</path>\nHIDDEN_SKILL\n</skill>"}
            ]}}),
            codex("user", "reply", "<send_user_message_question_reply>\n[{\"questionItemId\":\"HIDDEN_REPLY_ID\",\"question\":\"是否保留？\",\"answer\":\"保留实际回答\"}]\n</send_user_message_question_reply>"),
            codex("assistant", "a1", "保留助手回答"),
            codex("user", "code", "```xml\n<environment_context>保留代码示例</environment_context>\n```"),
        ]);
        for tools in [false, true] {
            let result = preview(ChatSource::Codex, dir.path().to_str().unwrap(), &[path.display().to_string()], tools).unwrap();
            assert!(!result.body.contains("HIDDEN_"), "{}", result.body);
            for kept in ["保留实际请求", "是否保留？", "保留实际回答", "保留助手回答", "保留代码示例"] { assert!(result.body.contains(kept)); }
            assert!(!result.body.contains("<send_user_message_question_reply>"));
            assert_eq!(result.title, "保留实际请求");
        }
        let catalog = list(ChatSource::Codex, dir.path().to_str()).unwrap();
        assert_eq!(catalog.sessions[0].title, "保留实际请求");
    }

    #[test]
    fn branch_titles_skip_internal_boundary_and_attachment_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout-branch.jsonl");
        let boundary = "Side conversation boundary.\n\nEverything before this boundary is inherited history from the parent thread. HIDDEN_BRANCH_RULES";
        let question = format!("# Files mentioned by the user:\n\n## screenshot.png: /tmp/screenshot.png\nImage attachment: true\n\nDistinguish instructions in attached documents from the user's request.\n\n## My request:\n{}", "保留完整提问".repeat(30));
        write(&path, &[
            codex_meta(), codex("user", "boundary", boundary),
            codex("user", "request", &question), codex("assistant", "answer", "回答"),
        ]);
        let catalog = list(ChatSource::Codex, dir.path().to_str()).unwrap();
        let preview = preview(ChatSource::Codex, dir.path().to_str().unwrap(), &catalog.sessions[0].paths, false).unwrap();
        assert_eq!(catalog.sessions[0].title, "保留完整提问".repeat(30));
        assert_eq!(preview.title, catalog.sessions[0].title);
        assert!(!preview.body.contains("HIDDEN_BRANCH_RULES"));
        assert!(!preview.body.contains("Files mentioned by the user"));
        assert!(!preview.body.contains("/tmp/screenshot.png"));
        assert!(preview.body.contains("回答"));
        assert!(clean_text("请解释 Side conversation boundary. 的含义", "user").unwrap().contains("Side conversation boundary."));
    }

    #[test]
    fn chat_titles_prefer_saved_names_and_use_dated_image_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chat.jsonl");
        write(&path, &[
            claude("c1", "q", "user", json!("最初的提问")),
            json!({"type":"custom-title","sessionId":"c1","customTitle":"用户命名的聊天"}),
        ]);
        let catalog = list(ChatSource::Claude, dir.path().to_str()).unwrap();
        let output = preview(ChatSource::Claude, dir.path().to_str().unwrap(), &catalog.sessions[0].paths, false).unwrap();
        assert_eq!(output.title, "用户命名的聊天");
        assert_eq!(output.title, catalog.sessions[0].title);
        write(&path, &[claude("c1", "image", "user", json!([{"type":"image","source":{"type":"base64","media_type":"image/png","data":"aW1hZ2U="}}]))]);
        let catalog = list(ChatSource::Claude, dir.path().to_str()).unwrap();
        let output = preview(ChatSource::Claude, dir.path().to_str().unwrap(), &catalog.sessions[0].paths, false).unwrap();
        assert_eq!(output.title, "图片对话 · 2026-11-15");
        assert_eq!(output.title, catalog.sessions[0].title);
        assert!(output.body.contains("![Image](chat-assets/"));
    }

    #[test]
    fn codex_filters_injected_skill_but_keeps_user_requests_and_references() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout-skill.jsonl");
        let skill = "<skill>\n<name>unslop</name>\n<path>/skills/unslop/SKILL.md</path>\n---\nname: unslop\n---\nSKILL_INJECTED_BODY\n</skill>";
        let request = "用 [$unslop](/skills/unslop/SKILL.md) 优化导入窗口";
        write(&path, &[
            codex_meta(), codex("user", "u1", request),
            codex("user", "skill", skill),
            json!({"type":"event_msg","payload":{"type":"user_message","message":skill}}),
            codex("assistant", "a1", "已优化窗口"),
            codex("user", "u2", "请解释 <skill> 标签的作用"),
        ]);
        for include_tools in [false, true] {
            let output = preview(ChatSource::Codex, dir.path().to_str().unwrap(), &[path.display().to_string()], include_tools).unwrap();
            assert!(!output.body.contains("SKILL_INJECTED_BODY"));
            assert!(output.body.contains(request));
            assert!(output.body.contains("已优化窗口"));
            assert!(output.body.contains("请解释 <skill> 标签的作用"));
        }
    }
    #[test]
    fn codex_merges_rollovers_decompresses_and_removes_only_mirror_events() {
        let dir = tempfile::tempdir().unwrap();
        let one = dir.path().join("sessions/rollout-one.jsonl");
        let two = dir.path().join("archived_sessions/rollout-two.jsonl");
        write(
            &one,
            &[
                codex_meta(),
                codex("developer", "d", "HIDDEN_INSTRUCTION"),
                codex(
                    "user",
                    "env",
                    "<environment_context>HIDDEN_ENV</environment_context>",
                ),
                codex("user", "u1", "重复问题"),
                codex("user", "u2", "重复问题"),
                json!({"type":"event_msg","timestamp":"2026-11-15T01:00:00Z","payload":{"type":"user_message","message":"重复问题"}}),
                codex("assistant", "a1", "第一答复"),
            ],
        );
        write(
            &two,
            &[
                codex_meta(),
                codex("assistant", "a2", "第二答复"),
                json!({"type":"response_item","payload":{"type":"function_call","id":"tool1","name":"exec","arguments":"FULL_TOOL_CALL"}}),
            ],
        );
        let compressed = two.with_extension("jsonl.zst");
        fs::write(
            &compressed,
            zstd::encode_all(fs::read(&two).unwrap().as_slice(), 1).unwrap(),
        )
        .unwrap();
        let root = dir.path().to_str().unwrap();
        assert_eq!(
            list(ChatSource::Codex, Some(root)).unwrap().sessions[0]
                .paths
                .len(),
            2
        );
        fs::remove_file(two).unwrap();
        let catalog = list(ChatSource::Codex, Some(root)).unwrap();
        assert_eq!(catalog.sessions.len(), 1);
        let output = preview(ChatSource::Codex, root, &catalog.sessions[0].paths, false).unwrap();
        assert!(output.body.contains("第一答复") && output.body.contains("第二答复"));
        assert_eq!(output.body.matches("重复问题").count(), 3); // title plus two real prompts
        assert!(!output.body.contains("HIDDEN") && !output.body.contains("FULL_TOOL_CALL"));
        assert!(
            preview(ChatSource::Codex, root, &catalog.sessions[0].paths, true)
                .unwrap()
                .body
                .contains("FULL_TOOL_CALL")
        );
    }
    #[test]
    fn claude_continuation_stops_at_parent_instead_of_merging_later_branch() {
        let dir = tempfile::tempdir().unwrap();
        let one = dir.path().join("one.jsonl");
        let two = dir.path().join("two.jsonl");
        write(
            &one,
            &[
                claude("one", "parent", "user", json!("旧问题")),
                claude("one", "other", "assistant", json!("其他分支")),
            ],
        );
        write(
            &two,
            &[
                json!({"type":"system","subtype":"compact_boundary","logicalParentUuid":"parent","uuid":"boundary","sessionId":"two"}),
                claude("two", "next", "assistant", json!("续接答案")),
            ],
        );
        let output = preview(
            ChatSource::Claude,
            dir.path().to_str().unwrap(),
            &[two.display().to_string()],
            false,
        )
        .unwrap();
        assert!(output.body.contains("旧问题") && output.body.contains("续接答案"));
        assert!(!output.body.contains("其他分支"));
    }
    #[test]
    fn invalid_lines_and_files_outside_selected_root_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.jsonl");
        fs::write(&path, "{broken").unwrap();
        let root = dir.path().to_str().unwrap();
        let catalog = list(ChatSource::Claude, Some(root)).unwrap();
        assert_eq!(catalog.issues.len(), 1);
        assert!(preview(
            ChatSource::Claude,
            root,
            &[path.display().to_string()],
            false
        )
        .unwrap_err()
        .to_string()
        .contains("第 1 行"));
        let other = tempfile::tempdir().unwrap();
        assert!(preview(
            ChatSource::Claude,
            other.path().to_str().unwrap(),
            &[path.display().to_string()],
            false
        )
        .is_err());
    }
    #[test]
    fn codex_saved_names_and_attachment_titles_match_preview() {
        let dir = tempfile::tempdir().unwrap();
        let database = rusqlite::Connection::open(dir.path().join("state_5.sqlite")).unwrap();
        database.execute_batch("CREATE TABLE threads(id TEXT, title TEXT, cwd TEXT, updated_at INTEGER, name TEXT, has_user_event INTEGER)").unwrap();
        let id = "00000000-0000-4000-8000-000000000001";
        let path = dir.path().join(format!("sessions/rollout-{id}.jsonl"));
        let mut meta = codex_meta();
        meta["payload"]["id"] = json!(id);
        let wrapped = "# Files mentioned by the user:\n\n## image.png: /tmp/image.png\n\n## My request:\n实际提问";
        write(&path, &[meta, codex("user", "q1", wrapped)]);
        database.execute("INSERT INTO threads VALUES(?1,?2,'/demo/project',1794704400,'自定义聊天名称',1)", params![id, wrapped]).unwrap();
        let root = dir.path().to_str().unwrap();
        let catalog = list(ChatSource::Codex, Some(root)).unwrap();
        assert_eq!(catalog.sessions[0].title, "自定义聊天名称");
        assert_eq!(preview(ChatSource::Codex, root, &catalog.sessions[0].paths, false).unwrap().title, "自定义聊天名称");
        database.execute("UPDATE threads SET name=NULL", []).unwrap();
        assert_eq!(list(ChatSource::Codex, Some(root)).unwrap().sessions[0].title, "实际提问");
        database.execute("UPDATE threads SET title=''", []).unwrap();
        assert_eq!(list(ChatSource::Codex, Some(root)).unwrap().sessions[0].title, "实际提问");
    }

    #[test]
    fn codex_catalog_uses_metadata_without_reading_transcript_bodies() {
        let dir = tempfile::tempdir().unwrap();
        let database = rusqlite::Connection::open(dir.path().join("state_5.sqlite")).unwrap();
        database.execute_batch("CREATE TABLE threads(id TEXT, title TEXT, cwd TEXT, created_at INTEGER, updated_at INTEGER)").unwrap();
        for i in 0..61 {
            let id = format!("00000000-0000-4000-8000-{i:012}");
            database.execute("INSERT INTO threads VALUES(?1,?2,'/demo/project',100,?3)", params![id, format!("Chat {i}"), 100 + i]).unwrap();
            let file = dir.path().join(format!("sessions/rollout-2026-11-15-{id}.jsonl"));
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            // Listing must not parse bodies; validation belongs to preview/import.
            fs::write(file, "invalid transcript body").unwrap();
        }
        let catalog = list(ChatSource::Codex, Some(dir.path().to_str().unwrap())).unwrap();
        assert_eq!(catalog.sessions.len(), 50);
        assert_eq!(catalog.sessions[0].title, "Chat 60");
        assert_eq!(catalog.total, 61);
        let next = list_page(ChatSource::Codex, Some(dir.path().to_str().unwrap()), &ChatListQuery { page: 1, ..Default::default() }).unwrap();
        assert_eq!(next.sessions.len(), 11);
        assert_eq!(next.sessions[0].title, "Chat 10");
        let filtered = list_page(ChatSource::Codex, Some(dir.path().to_str().unwrap()), &ChatListQuery { search: "Chat 0".into(), project: "/demo/project".into(), ..Default::default() }).unwrap();
        assert_eq!(filtered.total, 1);
        assert_eq!(filtered.sessions[0].title, "Chat 0");
    }
    #[test]
    fn custom_destination_is_remembered_and_registered_on_import() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("data/db.sqlite");
        let folder = dir.path().join("Custom Chats");
        fs::create_dir(&folder).unwrap();
        let store = Store::open(&database).unwrap();
        let saved = set_destination(&store, &folder).unwrap();
        assert!(store.roots().unwrap().is_empty());
        drop(store);
        let store = Store::open(&database).unwrap();
        let settings = store.settings().unwrap();
        assert_eq!(settings.chat_import_folder.as_deref(), Some(saved.as_str()));
        let (tx, _rx) = mpsc::channel();
        let actual = destination(&store, &tx, &dir.path().join("Documents"), &settings).unwrap();
        assert_eq!(actual.to_string_lossy(), saved);
        assert_eq!(store.roots().unwrap().len(), 1);
        assert!(set_destination(&store, &database).is_err());
    }
    #[test]
    fn english_destination_creates_and_registers_agent_history() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("data/db.sqlite")).unwrap();
        let (tx, _rx) = mpsc::channel();
        let documents = dir.path().join("Documents");
        let folder = destination(&store, &tx, &documents, &sag_protocol::types::Settings { language: "en".into(), ..Default::default() }).unwrap();
        assert_eq!(folder, documents.join("sag-wiki").join("agent-history").canonicalize().unwrap());
        assert!(folder.is_dir());
        assert_eq!(store.roots().unwrap()[0].path, folder.to_string_lossy());
    }
    #[test]
    fn imports_use_source_subfolders_and_keep_existing_file_locations() {
        let dir = tempfile::tempdir().unwrap();
        let transcripts = dir.path().join("source");
        let store = Store::open(&dir.path().join("data/db.sqlite")).unwrap();
        let (tx, _rx) = mpsc::channel();
        let custom = dir.path().join("Custom Chats");
        fs::create_dir(&custom).unwrap();
        set_destination(&store, &custom).unwrap();
        let folder = destination(&store, &tx, &dir.path().join("Documents"), &store.settings().unwrap()).unwrap();
        for (source, filename, subfolder, records) in [
            (ChatSource::Claude, "chat.jsonl", "claude code", vec![claude("c1", "u1", "user", json!("来源分组"))]),
            (ChatSource::Codex, "rollout-test.jsonl", "codex", vec![codex_meta(), codex("user", "u1", "来源分组")]),
        ] {
            write(&transcripts.join(subfolder).join(filename), &records);
            let catalog = list(source, Some(transcripts.join(subfolder).to_str().unwrap())).unwrap();
            let session = &catalog.sessions[0];
            let first = import(&store, &tx, source, &catalog.source_root, session, folder.to_str().unwrap(), false).unwrap();
            assert_eq!(Path::new(&first.path).parent().unwrap(), folder.join(subfolder));
            let repeated = import(&store, &tx, source, &catalog.source_root, session, folder.to_str().unwrap(), false).unwrap();
            assert_eq!(repeated.path, first.path);
            assert_eq!(repeated.status, "skipped");
            // Previously imported files continue updating at their recorded location.
            let legacy = folder.join(format!("legacy-{subfolder}.md"));
            fs::rename(&first.path, &legacy).unwrap();
            store.connection().execute("UPDATE chat_imports SET path=?1 WHERE path=?2", params![legacy.to_str().unwrap(), first.path]).unwrap();
            let repeated = import(&store, &tx, source, &catalog.source_root, session, folder.to_str().unwrap(), false).unwrap();
            assert_eq!(Path::new(&repeated.path), legacy);
            assert_eq!(repeated.status, "skipped");
        }
    }

    #[test]
    fn import_indexes_updates_without_duplicates_and_preserves_user_edits() {
        let dir = tempfile::tempdir().unwrap();
        let transcripts = dir.path().join("source");
        let documents = dir.path().join("Documents");
        let source = transcripts.join("chat.jsonl");
        write(
            &source,
            &[
                claude("c1", "u1", "user", json!("检索导入marker")),
                claude(
                    "c1",
                    "a1",
                    "assistant",
                    json!([{"type":"text","text":"导入正文答案"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"aW1hZ2U="}}]),
                ),
            ],
        );
        let store = Store::open(&dir.path().join("data/db.sqlite")).unwrap();
        let (tx, rx) = mpsc::channel();
        let library = destination(&store, &tx, &documents, &store.settings().unwrap()).unwrap();
        assert_eq!(library, documents.join("sag-wiki").join("agent-history").canonicalize().unwrap());
        assert!(library.is_dir());
        assert_eq!(store.roots().unwrap().len(), 1);
        assert_eq!(destination(&store, &tx, &documents, &store.settings().unwrap()).unwrap(), library);
        assert_eq!(store.roots().unwrap().len(), 1);
        while rx.try_recv().is_ok() {}
        let root = library.to_string_lossy().into_owned();
        let catalog = list(ChatSource::Claude, Some(transcripts.to_str().unwrap())).unwrap();
        let session = &catalog.sessions[0];
        let first = import(
            &store,
            &tx,
            ChatSource::Claude,
            &catalog.source_root,
            session,
            &root,
            false,
        )
        .unwrap();
        assert_eq!(first.status, "imported");
        assert_eq!(Path::new(&first.path).parent().unwrap(), library.join("claude code"));
        let request = rx.recv().unwrap();
        let IndexRequest::Paths {
            root: ref indexed_root,
            ref paths,
        } = request
        else {
            panic!("expected indexing request")
        };
        store.index_file(indexed_root, &paths[0], 2000).unwrap();
        assert_eq!(store.files().unwrap().len(), 1);
        assert!(!store
            .candidate_query("marker", None, &[], &[], &[], 10)
            .unwrap()
            .is_empty());
        let images = fs::read_dir(library.join("claude code/chat-assets")).unwrap().count();
        assert_eq!(images, 1);
        assert_eq!(
            import(
                &store,
                &tx,
                ChatSource::Claude,
                &catalog.source_root,
                session,
                &root,
                false
            )
            .unwrap()
            .status,
            "skipped"
        );
        let mut content = fs::read_to_string(&source).unwrap();
        content.push_str(&format!(
            "{}\n",
            claude("c1", "a2", "assistant", json!("继续聊天"))
        ));
        fs::write(&source, content).unwrap();
        assert_eq!(
            import(
                &store,
                &tx,
                ChatSource::Claude,
                &catalog.source_root,
                session,
                &root,
                false
            )
            .unwrap()
            .status,
            "updated"
        );
        fs::write(&first.path, "用户手动编辑").unwrap();
        assert!(import(
            &store,
            &tx,
            ChatSource::Claude,
            &catalog.source_root,
            session,
            &root,
            false
        )
        .is_err());
        assert_eq!(fs::read_to_string(&first.path).unwrap(), "用户手动编辑");
    }
}
    #[test]
    fn workbuddy_exports_messages_and_only_includes_tools_when_requested() {
        let temporary = tempfile::tempdir().unwrap();
        let project = temporary.path().join("project");
        fs::create_dir(&project).unwrap();
        let session = "2c3eae79-dc96-496e-92f2-2895da3b2c08";
        let file = project.join(format!("{session}.jsonl"));
        let mut rows = [
            serde_json::json!({"type":"session-meta","meta":{"secret":"hidden-settings"}}),
            serde_json::json!({"type":"message","id":"system","role":"system","content":"hidden-system"}),
            serde_json::json!({"type":"message","id":"question","role":"user","content":[{"type":"input_text","text":"<system-reminder>hidden-reminder</system-reminder>\n实际提问"}]}),
            serde_json::json!({"type":"reasoning","content":"hidden-reasoning"}),
            serde_json::json!({"type":"function_call","id":"call","name":"present_files","arguments":"tool-input","providerData":"hidden-provider-data"}),
            serde_json::json!({"type":"function_call_result","id":"result","name":"present_files","output":[{"text":"tool-output"}]}),
            serde_json::json!({"type":"message","id":"answer","role":"assistant","content":[{"type":"output_text","text":"实际回答"}]}),
            serde_json::json!({"type":"ai-title","aiTitle":"任务名称"}),
        ];
        for row in &mut rows {
            row["sessionId"] = serde_json::json!(session);
            row["timestamp"] = serde_json::json!(1_700_000_000_000_i64);
            row["cwd"] = serde_json::json!("/demo/project");
        }
        fs::write(&file, rows.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")).unwrap();
        let root = temporary.path().to_str().unwrap();
        let catalog = list(ChatSource::WorkBuddy, Some(root)).unwrap();
        assert_eq!(catalog.total, 1);
        let chat = &catalog.sessions[0];
        assert_eq!(chat.title, "任务名称");
        assert_eq!(chat.updated, "2023-11-14T22:13:20.000Z");
        for include_tools in [false, true] {
            let preview = preview(ChatSource::WorkBuddy, root, &chat.paths, include_tools).unwrap();
            assert_eq!(preview.title, chat.title);
            assert!(preview.body.contains("实际提问") && preview.body.contains("实际回答"));
            assert_eq!(preview.body.contains("tool-input"), include_tools);
            assert_eq!(preview.body.contains("tool-output"), include_tools);
            assert!(!preview.body.contains("hidden-"));
        }
    }
