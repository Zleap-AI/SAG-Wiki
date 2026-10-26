//! Notes: answers saved as Markdown whose citations are anchored to source
//! files by hash, byte range and the cited passage. Anchors live in the
//! frontmatter, which the indexer skips, so a note never duplicates its
//! sources in search results. Verification is local and never calls a model.
pub use sag_protocol::types::{NoteSource, NoteSourceState, NoteSourceStatus};
use crate::{text, types::Candidate};
use anyhow::{bail, Context, Result};
use regex::Regex;
use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    sync::LazyLock,
};

pub const FORMAT_VERSION: u32 = 1;
pub const DEFAULT_FOLDER: &str = "notes";
/// Below this word-level similarity a changed passage counts as missing.
pub const CHANGED_THRESHOLD: f64 = 0.6;
const SEED_LINES: usize = 3;
const SEED_MIN_CHARS: usize = 12;
const SEED_MAX_HITS: usize = 50;
const MAX_TOKENS: usize = 4000;

/// Parsed frontmatter of a note.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteHeader {
    pub question: String,
    pub asked_at: Option<i64>,
    pub model: String,
    pub scope: Vec<String>,
    pub sources: Vec<NoteSource>,
}

pub fn is_note(body: &str) -> bool {
    let body = body.strip_prefix('\u{feff}').unwrap_or(body);
    body.starts_with("---\n") && body[4..].starts_with("sag_note:")
        || body.starts_with("---\r\n") && body[5..].starts_with("sag_note:")
}

// ---------------------------------------------------------------------------
// Citations

static LABELS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[(\d+(?:[ ,，]+\d+)*)\]").unwrap());
static CODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)```.*?(?:```|$)|`[^`\n]*`").unwrap());
static NUMBERS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());

/// Keeps only the evidence that the prose cites and renumbers it by first use.
/// Returns the rewritten answer and the kept evidence in label order.
pub fn renumber_citations<'a>(answer: &str, evidence: &'a [Candidate]) -> (String, Vec<&'a Candidate>) {
    // Code spans never carry citations; mask them so labels inside stay untouched.
    let mut protected = vec![];
    for m in CODE.find_iter(answer) {
        protected.push((m.start(), m.end()));
    }
    let in_code = |at: usize| protected.iter().any(|(s, e)| at >= *s && at < *e);
    let mut order: Vec<usize> = vec![];
    let mut out = String::with_capacity(answer.len());
    let mut last = 0;
    for m in LABELS.captures_iter(answer) {
        let whole = m.get(0).unwrap();
        let tail = &answer[whole.end()..];
        if in_code(whole.start()) || tail.starts_with(['(', ':']) {
            continue;
        }
        let mut labels = vec![];
        for n in NUMBERS.find_iter(&m[1]) {
            let Ok(number) = n.as_str().parse::<usize>() else { continue };
            if number == 0 || number > evidence.len() {
                continue;
            }
            let index = number - 1;
            let position = match order.iter().position(|&i| i == index) {
                Some(p) => p,
                None => {
                    order.push(index);
                    order.len() - 1
                }
            };
            if !labels.contains(&(position + 1)) {
                labels.push(position + 1);
            }
        }
        if labels.is_empty() {
            continue;
        }
        out.push_str(&answer[last..whole.start()]);
        for label in labels {
            out.push('[');
            out.push_str(&label.to_string());
            out.push(']');
        }
        last = whole.end();
    }
    out.push_str(&answer[last..]);
    (out, order.into_iter().map(|i| &evidence[i]).collect())
}

/// Builds an anchor from a retrieval candidate. Spans are merged into one
/// range so the saved text can be matched as a whole when the file changes.
pub fn source_from_candidate(id: usize, candidate: &Candidate, display_path: Option<String>) -> NoteSource {
    let (start_byte, end_byte) = candidate
        .source_spans
        .iter()
        .fold((usize::MAX, 0), |(s, e), span| (s.min(span.start_byte), e.max(span.end_byte)));
    let (start_byte, end_byte) = if start_byte == usize::MAX {
        candidate.source_ranges.iter().fold((usize::MAX, 0), |(s, e), (a, b)| (s.min(*a), e.max(*b)))
    } else {
        (start_byte, end_byte)
    };
    let (start_byte, end_byte) = if start_byte == usize::MAX { (0, 0) } else { (start_byte, end_byte) };
    NoteSource {
        id,
        path: candidate.path.clone(),
        display_path,
        hash: candidate.hash.clone(),
        start_byte,
        end_byte,
        start_line: candidate.start_line,
        end_line: candidate.end_line,
        text: candidate.content.clone(),
    }
}

// ---------------------------------------------------------------------------
// Paths

/// Lexically removes `.` and `..` without touching the file system.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}
/// Relative path from `from_dir` to `to`, or the absolute path when they
/// share no prefix (different drives) or the result would be unreadable.
pub fn relative_path(from_dir: &Path, to: &Path) -> String {
    let from: Vec<_> = from_dir.components().collect();
    let target: Vec<_> = to.components().collect();
    let common = from.iter().zip(&target).take_while(|(a, b)| a == b).count();
    if common == 0 || !matches!(from.first(), Some(Component::RootDir | Component::Prefix(_))) || common < 2 && cfg!(windows) {
        return to.to_string_lossy().replace('\\', "/");
    }
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for part in &target[common..] {
        out.push(part.as_os_str());
    }
    if out.as_os_str().is_empty() {
        ".".into()
    } else {
        out.to_string_lossy().replace('\\', "/")
    }
}
fn resolve(note_dir: &Path, value: &str) -> String {
    let path = Path::new(value);
    let joined = if path.is_absolute() { path.to_path_buf() } else { note_dir.join(path) };
    normalize(&joined).to_string_lossy().into_owned()
}

// ---------------------------------------------------------------------------
// Frontmatter format (a small YAML subset: JSON-quoted scalars, lists, `|` blocks)

fn quoted(value: &str) -> String {
    serde_json::to_string(value).unwrap()
}
fn timestamp(ms: i64) -> String {
    use chrono::{Local, SecondsFormat, TimeZone};
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_else(|| ms.to_string())
}
fn parse_timestamp(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|t| t.timestamp_millis())
        .ok()
        .or_else(|| value.parse().ok())
}

/// Renders a complete note: frontmatter, answer, and a readable source list.
pub fn render(header: &NoteHeader, note_dir: &Path, answer: &str, english: bool) -> String {
    let mut out = String::new();
    out.push_str("---\nsag_note: ");
    out.push_str(&FORMAT_VERSION.to_string());
    out.push_str("\nquestion: ");
    out.push_str(&quoted(&header.question));
    if let Some(at) = header.asked_at {
        out.push_str("\nasked_at: ");
        out.push_str(&timestamp(at));
    }
    if !header.model.is_empty() {
        out.push_str("\nmodel: ");
        out.push_str(&quoted(&header.model));
    }
    if !header.scope.is_empty() {
        out.push_str("\nscope:");
        for scope in &header.scope {
            out.push_str("\n  - ");
            out.push_str(&quoted(&relative_path(note_dir, Path::new(scope))));
        }
    }
    out.push_str("\nsources:");
    if header.sources.is_empty() {
        out.push_str(" []");
    }
    for source in &header.sources {
        out.push_str(&format!("\n  - id: {}", source.id));
        out.push_str("\n    path: ");
        out.push_str(&quoted(&relative_path(note_dir, Path::new(&source.path))));
        if let Some(display) = &source.display_path {
            out.push_str("\n    display_path: ");
            out.push_str(&quoted(&relative_path(note_dir, Path::new(display))));
        }
        out.push_str("\n    hash: ");
        out.push_str(&source.hash);
        out.push_str(&format!("\n    bytes: [{}, {}]", source.start_byte, source.end_byte));
        out.push_str(&format!("\n    lines: [{}, {}]", source.start_line, source.end_line));
        out.push_str("\n    text: |");
        for line in source.text.replace("\r\n", "\n").replace('\r', "\n").lines() {
            out.push_str("\n      ");
            out.push_str(line);
        }
    }
    out.push_str("\n---\n\n");
    out.push_str(answer.trim_end());
    out.push('\n');
    if !header.sources.is_empty() {
        out.push_str(if english { "\n## Sources\n\n" } else { "\n## 依据\n\n" });
        for source in &header.sources {
            let shown = source.display_path.as_deref().unwrap_or(&source.path);
            let name = relative_path(note_dir, Path::new(shown));
            if english {
                out.push_str(&format!("{}. `{}` lines {}–{}\n", source.id, name, source.start_line, source.end_line));
            } else {
                out.push_str(&format!("{}. `{}` 第 {}–{} 行\n", source.id, name, source.start_line, source.end_line));
            }
        }
    }
    out
}

/// Splits the frontmatter block from the body, honoring CRLF and a BOM.
pub fn split_frontmatter(body: &str) -> Option<(&str, &str)> {
    let body = body.strip_prefix('\u{feff}').unwrap_or(body);
    let rest = body.strip_prefix("---\n").or_else(|| body.strip_prefix("---\r\n"))?;
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if offset > 0 || trimmed != "---" {
            if matches!(trimmed, "---" | "...") {
                return Some((&rest[..offset], &rest[offset + line.len()..]));
            }
        }
        offset += line.len();
    }
    None
}

fn unquote(value: &str) -> Result<String> {
    let value = value.trim();
    if value.starts_with('"') {
        serde_json::from_str::<String>(value).context("字符串格式无效")
    } else if let Some(inner) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        Ok(inner.replace("''", "'"))
    } else {
        Ok(value.to_owned())
    }
}
fn parse_pair(value: &str) -> Result<(usize, usize)> {
    let inner = value.trim().strip_prefix('[').and_then(|v| v.strip_suffix(']')).context("范围格式无效")?;
    let mut parts = inner.split(',').map(|p| p.trim().parse::<usize>());
    let a = parts.next().context("范围缺少起点")??;
    let b = parts.next().context("范围缺少终点")??;
    Ok((a, b))
}

/// Parses a note. Relative paths resolve against `note_dir`.
pub fn parse(body: &str, note_dir: &Path) -> Result<NoteHeader> {
    let (front, _) = split_frontmatter(body).context("缺少 frontmatter")?;
    let lines: Vec<&str> = front.lines().collect();
    let mut header = NoteHeader { question: String::new(), asked_at: None, model: String::new(), scope: vec![], sources: vec![] };
    let mut version = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if line.starts_with([' ', '\t']) {
            bail!("frontmatter 缩进无效：{line}");
        }
        let (key, value) = line.split_once(':').context("frontmatter 行缺少键")?;
        let value = value.trim();
        match key.trim() {
            "sag_note" => version = Some(value.parse::<u32>().context("sag_note 版本无效")?),
            "question" => header.question = unquote(value)?,
            "asked_at" => header.asked_at = parse_timestamp(value),
            "model" => header.model = unquote(value)?,
            "scope" => {
                if value == "[]" { continue; }
                while i < lines.len() && lines[i].trim_start().starts_with("- ") && lines[i].starts_with(' ') {
                    header.scope.push(resolve(note_dir, &unquote(lines[i].trim_start()[2..].trim())?));
                    i += 1;
                }
            }
            "sources" => {
                if value == "[]" { continue; }
                while i < lines.len() && lines[i].starts_with("  - ") {
                    let mut fields: HashMap<&str, String> = HashMap::new();
                    let mut text: Option<String> = None;
                    let first = &lines[i][4..];
                    i += 1;
                    let mut pending = vec![first];
                    while i < lines.len() && lines[i].starts_with("    ") && !lines[i].starts_with("  - ") {
                        pending.push(lines[i].trim_start_matches(' '));
                        i += 1;
                    }
                    let mut j = 0;
                    while j < pending.len() {
                        let item = pending[j];
                        j += 1;
                        let (k, v) = item.split_once(':').context("来源字段无效")?;
                        if k.trim() == "text" {
                            // Block scalar: the following lines belong to the text until a field appears.
                            let mut collected = vec![];
                            if v.trim() == "|" || v.trim() == "|-" {
                                // Text lines were trimmed of leading spaces above; re-read them raw.
                                // Raw lines start at index i - (pending.len() - j).
                                let raw_start = i - (pending.len() - j);
                                let mut r = raw_start;
                                while r < i {
                                    let raw = lines[r];
                                    if raw.starts_with("      ") { collected.push(&raw[6..]); }
                                    else if raw.trim().is_empty() { collected.push(""); }
                                    else { break; }
                                    r += 1;
                                }
                                j += r - raw_start;
                                while collected.last().is_some_and(|l| l.is_empty()) { collected.pop(); }
                                text = Some(collected.join("\n"));
                            } else {
                                text = Some(unquote(v)?);
                            }
                        } else {
                            fields.insert(k.trim(), v.trim().to_owned());
                        }
                    }
                    let get = |k: &str| fields.get(k).cloned().with_context(|| format!("来源缺少 {k}"));
                    let (start_byte, end_byte) = parse_pair(&get("bytes")?)?;
                    let (start_line, end_line) = parse_pair(&get("lines")?)?;
                    let id: usize = get("id")?.parse().context("来源 id 无效")?;
                    header.sources.push(NoteSource {
                        id,
                        path: resolve(note_dir, &unquote(&get("path")?)?),
                        display_path: fields.get("display_path").map(|v| unquote(v)).transpose()?.map(|v| resolve(note_dir, &v)),
                        hash: unquote(&get("hash")?)?,
                        start_byte,
                        end_byte,
                        start_line,
                        end_line,
                        text: text.unwrap_or_default(),
                    });
                }
            }
            _ => {}
        }
    }
    match version {
        Some(v) if v <= FORMAT_VERSION => {}
        Some(v) => bail!("笔记格式版本 {v} 高于当前支持的 {FORMAT_VERSION}"),
        None => bail!("不是 SAG 笔记：缺少 sag_note"),
    }
    Ok(header)
}

// ---------------------------------------------------------------------------
// Relocation

/// Normalized text (LF, no trailing whitespace) with a map back to source bytes.
struct Normalized {
    text: String,
    origin: Vec<usize>,
}
fn normalized(source: &str) -> Normalized {
    let mut text = String::with_capacity(source.len());
    let mut origin = Vec::with_capacity(source.len() + 1);
    let lines = text::Lines::new(source);
    for (index, (start, end)) in lines.spans.iter().enumerate() {
        let line = &source[*start..*end];
        let trimmed = line.trim_end();
        for (offset, ch) in trimmed.char_indices() {
            for _ in 0..ch.len_utf8() {
                origin.push(start + offset);
            }
            text.push(ch);
        }
        if index + 1 < lines.spans.len() {
            origin.push(*end);
            text.push('\n');
        }
    }
    origin.push(source.len());
    Normalized { text, origin }
}
impl Normalized {
    fn span(&self, start: usize, end: usize) -> (usize, usize) {
        (self.origin[start], self.origin[end.min(self.origin.len() - 1)])
    }
}

fn tokens(text: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut start = None;
    for (i, ch) in text.char_indices() {
        let word = ch.is_alphanumeric() && !is_cjk(ch);
        if word {
            if start.is_none() { start = Some(i); }
        } else {
            if let Some(s) = start.take() { out.push(&text[s..i]); }
            if is_cjk(ch) { out.push(&text[i..i + ch.len_utf8()]); }
        }
        if out.len() >= MAX_TOKENS { break; }
    }
    if let Some(s) = start { out.push(&text[s..]); }
    out
}
fn is_cjk(ch: char) -> bool {
    matches!(ch as u32, 0x3040..=0x30ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff)
}
/// Word-level similarity in [0, 1]: twice the LCS over the total token count.
pub fn similarity(a: &str, b: &str) -> f64 {
    let a = tokens(a);
    let b = tokens(b);
    if a.is_empty() && b.is_empty() { return 1.; }
    if a.is_empty() || b.is_empty() { return 0.; }
    let mut previous = vec![0u32; b.len() + 1];
    let mut current = vec![0u32; b.len() + 1];
    for x in &a {
        for (j, y) in b.iter().enumerate() {
            current[j + 1] = if x.eq_ignore_ascii_case(y) { previous[j] + 1 } else { previous[j + 1].max(current[j]) };
        }
        std::mem::swap(&mut previous, &mut current);
    }
    2. * previous[b.len()] as f64 / (a.len() + b.len()) as f64
}

pub struct Relocation {
    pub state: NoteSourceState,
    pub start_byte: usize,
    pub end_byte: usize,
    pub similarity: Option<f64>,
}

/// Finds the cited passage in the current content of a file whose hash changed.
pub fn relocate(source: &NoteSource, current: &str) -> Relocation {
    let wanted = normalized(&source.text);
    let wanted_text = wanted.text.trim_matches('\n');
    let have = normalized(current);
    if wanted_text.is_empty() {
        return Relocation { state: NoteSourceState::Missing, start_byte: 0, end_byte: 0, similarity: None };
    }
    // Exact match: moved. Several hits pick the one nearest the original position.
    let mut best: Option<(usize, usize)> = None;
    let mut from = 0;
    while let Some(at) = have.text[from..].find(wanted_text) {
        let position = from + at;
        let distance = position.abs_diff(source.start_byte);
        if best.is_none_or(|(_, d)| distance < d) { best = Some((position, distance)); }
        // Advance by one character so the next slice starts on a UTF-8 boundary.
        from = position + have.text[position..].chars().next().map_or(1, char::len_utf8);
        if from >= have.text.len() { break; }
    }
    if let Some((position, _)) = best {
        let (s, e) = have.span(position, position + wanted_text.len());
        return Relocation { state: NoteSourceState::Moved, start_byte: s, end_byte: e, similarity: Some(1.) };
    }
    // Fuzzy: seed lines locate candidate windows; the best window decides.
    let wanted_lines: Vec<&str> = wanted_text.lines().collect();
    let line_count = wanted_lines.len().max(1);
    let mut seeds: Vec<&str> = wanted_lines
        .iter()
        .copied()
        .filter(|l| l.chars().filter(|c| !c.is_whitespace()).count() >= SEED_MIN_CHARS)
        .collect();
    seeds.sort_by_key(|l| std::cmp::Reverse(l.len()));
    seeds.truncate(SEED_LINES);
    if seeds.is_empty() {
        seeds = wanted_lines.iter().copied().filter(|l| !l.trim().is_empty()).take(SEED_LINES).collect();
    }
    let have_lines: Vec<(usize, &str)> = {
        let mut out = vec![];
        let mut offset = 0;
        for line in have.text.split_inclusive('\n') {
            out.push((offset, line.trim_end_matches('\n')));
            offset += line.len();
        }
        out
    };
    let mut candidates: Vec<usize> = vec![];
    for seed in seeds {
        let seed = seed.trim();
        if seed.is_empty() { continue; }
        let mut hits = 0;
        for (index, (_, line)) in have_lines.iter().enumerate() {
            if line.contains(seed) {
                hits += 1;
                let pad = line_count / 4 + 1;
                for start in index.saturating_sub(line_count + pad)..=index {
                    if !candidates.contains(&start) { candidates.push(start); }
                }
                if hits >= SEED_MAX_HITS { break; }
            }
        }
    }
    if candidates.is_empty() {
        // No line survived intact (typical for a one-line edit): fall back to lines
        // that still share most of the cited words.
        // Compare line against line: a multi-line passage edited in several places
        // still has lines that each keep most of their own words.
        let wanted_line_tokens: Vec<std::collections::HashSet<String>> = wanted_lines
            .iter()
            .map(|line| tokens(line).iter().map(|t| t.to_lowercase()).collect::<std::collections::HashSet<_>>())
            .filter(|set| !set.is_empty())
            .collect();
        if !wanted_line_tokens.is_empty() {
            let mut scored: Vec<(usize, usize)> = have_lines
                .iter()
                .enumerate()
                .filter_map(|(index, (_, line))| {
                    let line_tokens: std::collections::HashSet<String> = tokens(line).iter().map(|t| t.to_lowercase()).collect();
                    let shared = wanted_line_tokens
                        .iter()
                        .filter(|wanted| wanted.intersection(&line_tokens).count() * 2 >= wanted.len())
                        .map(|wanted| wanted.intersection(&line_tokens).count())
                        .max()?;
                    Some((shared, index))
                })
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            for (_, index) in scored.into_iter().take(SEED_MAX_HITS) {
                let pad = line_count / 4 + 1;
                for start in index.saturating_sub(line_count + pad)..=index {
                    if !candidates.contains(&start) { candidates.push(start); }
                }
            }
        }
    }
    let mut best: Option<(f64, usize, usize)> = None;
    let lengths = [line_count, line_count + line_count / 4 + 1, line_count.saturating_sub(line_count / 4).max(1)];
    for start in candidates {
        for &length in &lengths {
            let end = (start + length).min(have_lines.len());
            if end <= start { continue; }
            let (s, _) = have_lines[start];
            let e = have_lines[end - 1].0 + have_lines[end - 1].1.len();
            let window = &have.text[s..e];
            let score = similarity(wanted_text, window);
            if best.is_none_or(|(b, _, _)| score > b) { best = Some((score, s, e)); }
        }
    }
    match best {
        Some((score, s, e)) if score >= CHANGED_THRESHOLD => {
            let (start_byte, end_byte) = have.span(s, e);
            Relocation { state: NoteSourceState::Changed, start_byte, end_byte, similarity: Some(score) }
        }
        Some((score, _, _)) => Relocation { state: NoteSourceState::Missing, start_byte: 0, end_byte: 0, similarity: Some(score) },
        None => Relocation { state: NoteSourceState::Missing, start_byte: 0, end_byte: 0, similarity: None },
    }
}

/// Line span for a byte range of `content`.
pub fn span_of(content: &str, start: usize, end: usize) -> crate::types::SourceSpan {
    let lines = text::Lines::new(content);
    lines.span(content, start.min(content.len()), end.min(content.len()))
}

/// File name derived from the question, valid on every platform.
pub fn file_name(question: &str) -> String {
    let cleaned: String = question
        .chars()
        .map(|c| if c.is_control() || "/\\:?*<>|\"".contains(c) { ' ' } else { c })
        .collect();
    let mut name: String = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    const TRAILING: &[char] = &['.', ' ', '?', '!', ',', ';', ':', '？', '！', '。', '，', '、', '；', '：'];
    name = name.trim_matches(['.', ' ']).chars().take(60).collect::<String>().trim_end_matches(TRAILING).to_owned();
    if name.is_empty() { name = "note".into(); }
    format!("{name}.md")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SourceSpan;

    fn candidate(path: &str, content: &str, start: usize, hash: &str) -> Candidate {
        Candidate {
            id: 1, document_id: 1, path: path.into(), title: "t".into(), heading: String::new(),
            content: content.into(), start_line: 3, end_line: 4,
            source_ranges: vec![(start, start + content.len())],
            source_spans: vec![SourceSpan { start_byte: start, end_byte: start + content.len(), start_line: 3, start_column: 1, end_line: 4, end_column: 1 }],
            bm25: 0., score: 0., previous_score: None, order: 0, hash: hash.into(),
        }
    }

    #[test]
    fn citations_are_renumbered_by_first_use_and_only_cited_sources_kept() {
        let evidence: Vec<Candidate> = (0..4).map(|i| candidate(&format!("/k/{i}.md"), "x", 0, "h")).collect();
        let (text, kept) = renumber_citations("见 [3]，还有 [1, 3] 与 `[2]`，无效 [9]。[4](link) 也不算。", &evidence);
        assert_eq!(text, "见 [1]，还有 [2][1] 与 `[2]`，无效 [9]。[4](link) 也不算。");
        assert_eq!(kept.iter().map(|c| c.path.as_str()).collect::<Vec<_>>(), ["/k/2.md", "/k/0.md"]);
    }

    #[test]
    fn note_round_trips_through_render_and_parse() {
        let home = if cfg!(windows) { Path::new(r"C:\Users\test") } else { Path::new("/home/me") };
        let dir = home.join("Documents").join("sag-wiki").join("notes");
        let scope = home.join("project").join("src");
        let source = scope.join("auth").join("guard.rs");
        let header = NoteHeader {
            question: "权限在哪里校验？\"引号\"".into(),
            asked_at: Some(1_761_370_800_000),
            model: "m".into(),
            scope: vec![scope.to_string_lossy().into_owned()],
            sources: vec![NoteSource {
                id: 1, path: source.to_string_lossy().into_owned(), display_path: None, hash: "abc".into(),
                start_byte: 10, end_byte: 40, start_line: 2, end_line: 3,
                text: "fn check()\n    // 第二行 : with colon\n".into(),
            }],
        };
        let body = render(&header, &dir, "答案 [1]。", false);
        assert!(is_note(&body));
        assert!(body.contains("path: \"../../../project/src/auth/guard.rs\""));
        assert!(body.contains("## 依据"));
        let parsed = parse(&body, &dir).unwrap();
        assert_eq!(parsed.question, header.question);
        assert_eq!(parsed.asked_at, header.asked_at);
        assert_eq!(parsed.scope, header.scope);
        let mut expected = header.sources.clone();
        expected[0].text = "fn check()\n    // 第二行 : with colon".into();
        assert_eq!(parsed.sources, expected);
        // CRLF files parse too.
        let parsed = parse(&body.replace('\n', "\r\n"), &dir).unwrap();
        assert_eq!(parsed.sources[0].text, expected[0].text);
    }

    #[test]
    fn parse_rejects_non_notes_and_broken_frontmatter() {
        assert!(parse("# just markdown", Path::new("/")).is_err());
        assert!(parse("---\ntitle: x\n---\n", Path::new("/")).is_err());
        assert!(parse("---\nsag_note: 1\nsources:\n  - id: 1\n    path: a\n---\n", Path::new("/")).is_err());
    }

    fn source(text: &str, start: usize) -> NoteSource {
        NoteSource { id: 1, path: "/f".into(), display_path: None, hash: "old".into(), start_byte: start, end_byte: start + text.len(), start_line: 1, end_line: 1, text: text.into() }
    }

    #[test]
    fn relocation_finds_moved_changed_and_missing_passages() {
        let passage = "fn check_permission(user: &User) -> bool {\n    user.roles.contains(&Role::Admin)\n}";
        let moved = format!("// twenty new lines\n{}\n{passage}\n", "x\n".repeat(20));
        let r = relocate(&source(passage, 0), &moved);
        assert_eq!(r.state, NoteSourceState::Moved);
        assert_eq!(&moved[r.start_byte..r.end_byte], passage);

        let changed = "fn check_permission(user: &User, action: Action) -> bool {\n    user.roles.contains(&Role::Admin) || action.is_public()\n}\n";
        let r = relocate(&source(passage, 0), changed);
        assert_eq!(r.state, NoteSourceState::Changed);
        assert!(r.similarity.unwrap() >= CHANGED_THRESHOLD);
        assert!(changed[r.start_byte..r.end_byte].contains("check_permission"));

        let gone = "completely different file\nabout cooking recipes\n";
        assert_eq!(relocate(&source(passage, 0), gone).state, NoteSourceState::Missing);
    }

    #[test]
    fn relocation_prefers_the_hit_nearest_the_original_position_and_ignores_trailing_space() {
        let passage = "same boilerplate line\nsecond";
        let current = format!("{passage}\n\nfiller\n\n{passage}   \n");
        let second = current.rfind("same").unwrap();
        let r = relocate(&source(passage, second - 3), &current);
        assert_eq!(r.state, NoteSourceState::Moved);
        assert_eq!(r.start_byte, second);
        let crlf = current.replace('\n', "\r\n");
        let r = relocate(&source(passage, 0), &crlf);
        assert_eq!(r.state, NoteSourceState::Moved);
        assert_eq!(r.start_byte, 0);
    }

    #[test]
    fn relocation_handles_multibyte_text_and_single_line_edits() {
        // A hit followed by a multi-byte character must not slice mid-codepoint.
        let r = relocate(&source("发布流程", 0), "新增\n发布流程\n");
        assert_eq!(r.state, NoteSourceState::Moved);
        let r = relocate(&source("发布流程需要审批", 0), "其他\n发布流程需要审批\n发布流程需要审批\n");
        assert_eq!(r.state, NoteSourceState::Moved);
        // A one-line passage with one word replaced is changed, not missing.
        let r = relocate(&source("Release requires owner approval before deployment.", 0), "intro\nRelease requires security approval before deployment.\noutro\n");
        assert_eq!(r.state, NoteSourceState::Changed);
        assert!(r.similarity.unwrap() > 0.8);
        // Every line of a multi-line passage edited by one word: still changed.
        let passage = "Release requires owner approval before deployment.\nThe owner signs the manifest file.\nDeployment happens after the owner confirms.";
        let edited = "intro\nRelease requires security approval before deployment.\nThe lead signs the manifest file.\nDeployment happens after the lead confirms.\noutro\n";
        let r = relocate(&source(passage, 0), edited);
        assert_eq!(r.state, NoteSourceState::Changed);
        assert!(r.similarity.unwrap() >= 0.7);
        assert!(edited[r.start_byte..r.end_byte].starts_with("Release requires security"));
    }

    #[test]
    fn file_names_are_safe_and_bounded() {
        assert_eq!(file_name("  登录后/权限:在哪里?  "), "登录后 权限 在哪里.md");
        assert_eq!(file_name("登录后权限在哪里校验？"), "登录后权限在哪里校验.md");
        assert_eq!(file_name("..."), "note.md");
        assert_eq!(file_name(&"a".repeat(100)).len(), 63);
    }

    #[test]
    fn relative_paths_cross_folders() {
        assert_eq!(relative_path(Path::new("/a/b/notes"), Path::new("/a/c/x.md")), "../../c/x.md");
        assert_eq!(relative_path(Path::new("/a/b"), Path::new("/a/b/x.md")), "x.md");
        assert_eq!(normalize(Path::new("/a/b/notes/../../c/x.md")), PathBuf::from("/a/c/x.md"));
    }
}
