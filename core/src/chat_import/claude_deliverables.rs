use super::{clean_text, field, root, transcript, visit};
use anyhow::{ensure, Result};
use percent_encoding::percent_decode_str;
use pulldown_cmark::{Event, Parser, Tag};
use regex::Regex;
use reqwest::Url;
use sag_protocol::{ChatImportIssue, ChatSource};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::LazyLock,
};
use tokio_util::sync::CancellationToken;
use walkdir::WalkDir;

static DELIVERY_HEADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:#{1,6}[ \t]+)?(?:\*\*)?(?:最终交付文件|最终产物|交付文件|交付产物|最终交付物|final deliverables|delivered files|final outputs|deliverables)(?:\*\*)?(?:[ \t]*[:：](?:\*\*)?[ \t]*(.*))?$").unwrap()
});

#[derive(Default)]
struct Reply {
    id: String,
    text: String,
    cwd: PathBuf,
    completed: bool,
    used_tool: bool,
}

impl Reply {
    fn collect(self, paths: &mut BTreeSet<PathBuf>) -> Result<()> {
        if self.completed && !self.used_tool && !self.text.is_empty() {
            delivered_paths(&clean_text(&self.text, "assistant")?, &self.cwd, paths);
        }
        Ok(())
    }
}

fn replies(path: &Path, stop: &CancellationToken, paths: &mut BTreeSet<PathBuf>) -> Result<()> {
    let mut reply = Reply::default();
    let result = visit(path, |event| {
        ensure!(!stop.is_cancelled(), "已停止");
        if event["isSidechain"] == true || event["isMeta"] == true {
            return Ok(true);
        }
        match field(&event, "type") {
            "user" => {
                std::mem::take(&mut reply).collect(paths)?;
                return Ok(true);
            }
            "assistant" if event["isSynthetic"] != true && event["isApiErrorMessage"] != true => {}
            _ => return Ok(true),
        }
        let message = &event["message"];
        let id = field(message, "id");
        if id.is_empty() || reply.id != id {
            std::mem::take(&mut reply).collect(paths)?;
            reply.id = id.to_owned();
        }
        if let Some(cwd) = event["cwd"].as_str() {
            reply.cwd = PathBuf::from(cwd);
        }
        match field(message, "stop_reason") {
            "end_turn" | "stop_sequence" => reply.completed = true,
            "tool_use" => reply.used_tool = true,
            _ => {}
        }
        for block in message["content"].as_array().into_iter().flatten() {
            match field(block, "type") {
                "text" => {
                    if !reply.text.is_empty() {
                        reply.text.push('\n');
                    }
                    reply.text.push_str(field(block, "text"));
                }
                "tool_use" | "server_tool_use" => reply.used_tool = true,
                _ => {}
            }
        }
        if id.is_empty() {
            std::mem::take(&mut reply).collect(paths)?;
        }
        Ok(true)
    });
    reply.collect(paths)?;
    result
}

fn list_entry(line: &str) -> Option<&str> {
    for marker in ["- ", "* ", "+ "] {
        if let Some(value) = line.strip_prefix(marker) {
            return Some(value.trim_start());
        }
    }
    let suffix = line.trim_start_matches(|ch: char| ch.is_ascii_digit());
    if suffix.len() == line.len() {
        return None;
    }
    suffix
        .strip_prefix(". ")
        .or_else(|| suffix.strip_prefix(") "))
        .map(str::trim_start)
}

fn delivered_paths(text: &str, cwd: &Path, paths: &mut BTreeSet<PathBuf>) {
    let mut delivery_section = false;
    let mut fence: Option<(u8, usize)> = None;
    for line in text.lines() {
        if line.starts_with("    ") || line.starts_with('\t') {
            continue;
        }
        let line = line.trim();
        if let Some(&marker @ (b'`' | b'~')) = line.as_bytes().first() {
            let length = line.bytes().take_while(|&byte| byte == marker).count();
            if length >= 3 {
                if let Some((opening, minimum)) = fence {
                    if marker == opening && length >= minimum && line[length..].trim().is_empty() {
                        fence = None;
                    }
                } else {
                    fence = Some((marker, length));
                }
                continue;
            }
        }
        if fence.is_some() || line.is_empty() {
            continue;
        }
        let entry = list_entry(line);
        let value = entry.unwrap_or(line);
        if let Some(heading) = DELIVERY_HEADING.captures(value) {
            delivery_section = true;
            if let Some(inline) = heading.get(1) {
                add_entry(inline.as_str(), cwd, paths);
            }
        } else if delivery_section && !add_entry(value, cwd, paths) && entry.is_none() {
            delivery_section = false;
        }
    }
}

fn add_entry(entry: &str, cwd: &Path, paths: &mut BTreeSet<PathBuf>) -> bool {
    if entry.starts_with(['[', '<', '`']) {
        for token in Parser::new(entry) {
            let path = match token {
                Event::Start(Tag::Link { dest_url, .. }) => local_path(&dest_url, cwd, true),
                Event::Code(code) => local_path(&code, cwd, false),
                Event::Start(Tag::Paragraph) => continue,
                _ => return false,
            };
            if let Some(path) = path.filter(|path| path.extension().is_some()) {
                paths.insert(path);
                return true;
            }
            return false;
        }
        false
    } else if let Some(path) =
        local_path(entry, cwd, false).filter(|path| path.extension().is_some())
    {
        paths.insert(path);
        true
    } else {
        false
    }
}

fn local_path(value: &str, cwd: &Path, link: bool) -> Option<PathBuf> {
    let value = value.trim();
    if value.is_empty() || value.starts_with("//") || value.starts_with("\\\\") {
        return None;
    }
    if value.starts_with("file://") {
        let url = Url::parse(value).ok()?;
        return if url.host_str().is_none_or(|host| host == "localhost") {
            url.to_file_path().ok()
        } else {
            None
        };
    }
    let value = if link {
        percent_decode_str(value.split(['#', '?']).next()?)
            .decode_utf8()
            .ok()?
    } else {
        std::borrow::Cow::Borrowed(value)
    };
    let path = Path::new(value.as_ref());
    if let Some(home_relative) = value.strip_prefix("~/") {
        return Some(dirs::home_dir()?.join(home_relative));
    }
    if path.is_absolute() {
        return Some(path.to_owned());
    }
    if value.contains(':') || !cwd.is_absolute() {
        return None;
    }
    Some(cwd.join(path))
}

pub(crate) fn find(stop: &CancellationToken) -> Result<(BTreeSet<PathBuf>, Vec<ChatImportIssue>)> {
    let root = root(ChatSource::Claude, None)?;
    find_in(&root, stop)
}

fn find_in(
    root: &Path,
    stop: &CancellationToken,
) -> Result<(BTreeSet<PathBuf>, Vec<ChatImportIssue>)> {
    let mut paths = BTreeSet::new();
    let mut issues = Vec::new();
    if !root.try_exists()? {
        return Ok((paths, issues));
    }
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| entry.file_name() != "subagents")
    {
        ensure!(!stop.is_cancelled(), "已停止");
        match entry {
            Ok(entry)
                if entry.file_type().is_file() && transcript(entry.path(), ChatSource::Claude) =>
            {
                if let Err(error) = replies(entry.path(), stop, &mut paths) {
                    issues.push(ChatImportIssue {
                        path: entry.path().display().to_string(),
                        error: error.to_string(),
                    });
                }
            }
            Ok(_) => {}
            Err(error) => issues.push(ChatImportIssue {
                path: error.path().unwrap_or(root).display().to_string(),
                error: error.to_string(),
            }),
        }
    }
    ensure!(!stop.is_cancelled(), "已停止");
    Ok((paths, issues))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[test]
    fn reads_only_explicit_deliveries_from_completed_main_replies() {
        let temporary = tempfile::tempdir().unwrap();
        let history = temporary.path().join("history");
        let project = temporary.path().join("workspace");
        fs::create_dir_all(history.join("project/subagents")).unwrap();
        let final_text = r#"### 最终产物

1. [交付报告](reports/final%20report.pdf)（不含 [草稿](draft.pdf)）
2. notes.md
3. `exports/answer.html`
4. [远程文件](https://example.com/download.pdf)

## 补充

- [参考文件](reference.pdf)

```text
最终产物：sample.pdf
```"#;
        let reply = |id, stop_reason, text| {
            json!({
                "type":"assistant", "cwd":project.to_str().unwrap(),
                "message":{"id":id,"stop_reason":stop_reason,"content":[{"type":"text","text":text}]},
            })
        };
        let mut sidechain = reply("sub", "end_turn", "最终产物：child.pdf");
        sidechain["isSidechain"] = json!(true);
        let entries = [
            json!({"type":"user", "message":{"role":"user","content":"最终产物：input.pdf"}}),
            reply("work", "tool_use", "最终产物：intermediate.pdf"),
            reply("interrupted", "", "最终产物：partial.pdf"),
            reply("loose", "end_turn", "请参考 [输入文件](loose.pdf)"),
            sidechain,
            reply("done", "", final_text),
            reply("done", "end_turn", "**交付文件：** `summary.md`"),
            reply("another", "stop_sequence", "Delivered files: release.docx"),
        ];
        let encoded = entries
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(history.join("project/session.jsonl"), encoded).unwrap();
        fs::write(
            history.join("project/subagents/session.jsonl"),
            reply("child", "end_turn", "最终产物：nested.pdf").to_string(),
        )
        .unwrap();
        let (files, issues) = find_in(&history, &CancellationToken::new()).unwrap();
        assert!(issues.is_empty());
        assert_eq!(
            files,
            [
                "reports/final report.pdf",
                "notes.md",
                "exports/answer.html",
                "release.docx",
                "summary.md"
            ]
            .into_iter()
            .map(|name| project.join(name))
            .collect()
        );
    }
}
