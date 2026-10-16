use jieba_rs::Jieba;
use regex::Regex;
use std::{collections::HashSet, sync::{LazyLock, Mutex}};
use indexmap::IndexMap;
use unicode_normalization::UnicodeNormalization;

static JIEBA: LazyLock<Jieba> = LazyLock::new(Jieba::new);
static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\p{L}\p{N}]").unwrap());
static STOPS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    "的 了 是 在 和 与 或 哪 什么 哪个 哪些 如何 怎么 为什么 是否 这 那 一个 一种 请 我 你 他 它 吗 呢 吧 地 得 the a an of in on at to for and or is are was were be by with from which what who how when where does do did that this it as".split_whitespace().collect()
});
static TOKEN_CACHE: LazyLock<Mutex<IndexMap<String, Vec<String>>>> = LazyLock::new(|| Mutex::new(IndexMap::new()));
pub fn tokenize(text: &str) -> Vec<String> {
    {
        let mut cache = TOKEN_CACHE.lock().expect("token cache mutex poisoned");
        // Cache eviction order does not affect tokens; avoid shifting thousands of entries.
        if let Some(tokens) = cache.swap_remove(text) {
            cache.insert(text.to_owned(), tokens.clone());
            return tokens;
        }
    }
    let tokens = tokenize_uncached(text);
    let mut cache = TOKEN_CACHE.lock().expect("token cache mutex poisoned");
    cache.insert(text.to_owned(), tokens.clone());
    if cache.len() > 8192 { cache.swap_remove_index(0); }
    tokens
}
fn tokenize_uncached(text: &str) -> Vec<String> {
    let s = text.nfkc().collect::<String>().to_lowercase();
    JIEBA
        .cut_for_search(&s, true)
        .into_iter()
        .map(|token| token.word)
        .filter(|s| WORD.is_match(s) && !STOPS.contains(s))
        .map(str::to_owned)
        .collect()
}
pub fn index_text(text: &str) -> String {
    tokenize(text).join(" ")
}
pub const EXPLORE_SUMMARY_VERSION: i64 = 2;
pub fn explore_summary<'a>(path: &std::path::Path, chunks: impl Iterator<Item = (&'a str, &'a str)>) -> Vec<crate::types::DocumentTerm> {
    let code = matches!(file_kind(path), Some(FileKind::Code | FileKind::Config));
    let title: HashSet<_> = tokenize(path.file_stem().and_then(|s| s.to_str()).unwrap_or("")).into_iter().collect();
    let mut headings = HashSet::new();
    let mut counts = std::collections::HashMap::<String, usize>::new();
    let mut seen_passages = HashSet::new();
    for (heading, content) in chunks {
        headings.extend(tokenize(heading));
        if !seen_passages.insert(content.trim()) { continue; }
        let mut importing = false;
        let content = content.lines().filter(|line| {
            let line = line.trim();
            let label = line.trim_start_matches(['/', '*', '#', ' ']).to_lowercase();
            if ["copyright ", "©", "all rights reserved", "版权所有", "免责声明"].iter().any(|prefix| label.starts_with(prefix)) { return false; }
            if !code { return true; }
            if importing {
                importing = !(line.contains(';') || line.contains("from ") || line.ends_with(')'));
                return false;
            }
            if ["import ", "from ", "use ", "#include "].iter().any(|prefix| line.starts_with(prefix)) {
                importing = line.ends_with('{') || line.ends_with('(');
                return false;
            }
            true
        }).collect::<Vec<_>>().join("\n");
        for term in tokenize(&content) {
            let term = term.as_str();
            if term.chars().count() < 2 || term.chars().all(|c| c.is_numeric()) { continue; }
            if matches!(term, "copyright" | "reserved" | "免责声明" | "版权所有" | "以及" | "可以" | "一个" | "进行" | "通过" | "需要" | "相关" | "使用" | "内容" | "本文") { continue; }
            if code && matches!(term, "import" | "from" | "export" | "default" | "const" | "let" | "var" | "return" | "function" | "class" | "true" | "false" | "null" | "undefined" | "public" | "private" | "static" | "void" | "string" | "number" | "async" | "await" | "self" | "this" | "else" | "new" | "pub" | "use" | "mut" | "impl" | "std" | "def" | "int" | "bool") { continue; }
            *counts.entry(term.to_owned()).or_default() += 1;
        }
    }
    let mut terms: Vec<_> = counts.into_iter().map(|(term, count)| {
        let boost = 1.0 + if title.contains(&term) { 1.5 } else { 0.0 } + if headings.contains(&term) { 0.5 } else { 0.0 };
        crate::types::DocumentTerm { term, count, boost }
    }).collect();
    terms.sort_by(|a,b| (b.boost * (1.0 + (b.count as f64).ln())).total_cmp(&(a.boost * (1.0 + (a.count as f64).ln()))).then(a.term.cmp(&b.term)));
    terms.truncate(64);
    terms
}
pub fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}
pub fn search_terms(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    tokenize(text)
        .into_iter()
        .filter(|s| seen.insert(s.clone()))
        .take(80)
        .collect()
}
pub fn match_query(text: &str) -> String {
    search_terms(text)
        .iter()
        .map(|s| quote(s))
        .collect::<Vec<_>>()
        .join(" OR ")
}

pub fn is_markdown(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "md" | "markdown"))
}
enum FileKind { Markdown, Text, Code, Config }
fn file_kind(path: &std::path::Path) -> Option<FileKind> {
    if path.file_name().and_then(|name| name.to_str()).is_some_and(|name| {
        matches!(name, "Dockerfile" | "Containerfile" | "Makefile" | "CMakeLists.txt")
    }) {
        return Some(FileKind::Code);
    }
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "md" | "markdown" => Some(FileKind::Markdown),
        "txt" | "rst" | "rtf" => Some(FileKind::Text),
        "json" | "jsonc" | "yaml" | "yml" | "toml" | "ini" | "cfg" | "conf" | "xml" => Some(FileKind::Config),
            "rs"
                | "py"
                | "js"
                | "jsx"
                | "ts"
                | "tsx"
                | "mjs"
                | "cjs"
                | "go"
                | "java"
                | "c"
                | "h"
                | "cpp"
                | "hpp"
                | "cc"
                | "cs"
                | "rb"
                | "php"
                | "swift"
                | "kt"
                | "kts"
                | "scala"
                | "sh"
                | "bash"
                | "zsh"
                | "ps1"
                | "sql"
                | "html"
                | "htm"
                | "css"
                | "scss"
                | "vue"
                | "svelte"
                | "r"
                | "lua"
                | "dart" => Some(FileKind::Code),
        _ => None,
    }
}
pub fn is_supported(path: &std::path::Path) -> bool {
    file_kind(path).is_some()
}
pub fn is_indexable(path: &std::path::Path, types: &crate::types::FileTypes) -> bool {
    match file_kind(path) {
        Some(FileKind::Markdown) => types.markdown,
        Some(FileKind::Text) => types.text,
        Some(FileKind::Code) => types.code,
        Some(FileKind::Config) => types.config,
        None => false,
    }
}
pub fn in_hidden_directory(root: &std::path::Path, path: &std::path::Path, directory: bool) -> bool {
    path.strip_prefix(root).is_ok_and(|relative| {
        let directories = if directory { Some(relative) } else { relative.parent() };
        directories.into_iter().flat_map(std::path::Path::components)
            .any(|part| part.as_os_str().as_encoded_bytes().starts_with(b"."))
    })
}
#[derive(Clone, Debug, PartialEq)]
pub struct Chunk {
    pub heading: String,
    pub content: String,
    pub ranges: Vec<(usize, usize)>,
    pub spans: Vec<crate::types::SourceSpan>,
}
pub(crate) struct Lines {
    pub(crate) spans: Vec<(usize, usize)>,
    char_checkpoints: Vec<(usize, usize)>,
}
impl Lines {
    pub(crate) fn new(source: &str) -> Self {
        let bytes = source.as_bytes();
        let mut spans = vec![];
        let mut start = 0;
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\r' || bytes[i] == b'\n' {
                spans.push((start, i));
                if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                start = i + 1;
            }
            i += 1;
        }
        spans.push((start, bytes.len()));
        let mut char_checkpoints = vec![(0, 0)];
        for (count, (byte, _)) in source.char_indices().enumerate() {
            if byte - char_checkpoints.last().unwrap().0 >= 4096 {
                char_checkpoints.push((byte, count));
            }
        }
        Self { spans, char_checkpoints }
    }
    fn point(&self, source: &str, byte: usize) -> (usize, usize) {
        let line = self.spans.partition_point(|(start, _)| *start <= byte) - 1;
        let (start, end) = self.spans[line];
        let char_count = |byte| {
            let index = self.char_checkpoints.partition_point(|(offset, _)| *offset <= byte) - 1;
            let (offset, count) = self.char_checkpoints[index];
            count + source[offset..byte].chars().count()
        };
        (line + 1, char_count(byte.min(end)) - char_count(start) + 1)
    }
    pub(crate) fn span(&self, source: &str, start: usize, end: usize) -> crate::types::SourceSpan {
        let (start_line, start_column) = self.point(source, start);
        let (end_line, end_column) = self.point(source, end);
        crate::types::SourceSpan {
            start_byte: start,
            end_byte: end,
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }
}
// Keep exact source byte offsets; line/column endpoints are 1-based and end-exclusive.
fn split_block(
    source: &str,
    start: usize,
    end: usize,
    limit: usize,
    code: bool,
) -> Vec<(usize, usize)> {
    let mut parts = vec![];
    let mut cursor = start;
    while cursor < end {
        let mut used = 0;
        let mut cut = cursor;
        let mut line_break = None;
        let mut sentence = None;
        let mut space = None;
        for (offset, ch) in source[cursor..end].char_indices() {
            if used + ch.len_utf16() > limit {
                break;
            }
            used += ch.len_utf16();
            cut = cursor + offset + ch.len_utf8();
            if used >= limit / 2 {
                if ch == '\n' {
                    line_break = Some(cut)
                } else if "。！？；.!?;".contains(ch) {
                    sentence = Some(cut)
                } else if ch.is_whitespace() {
                    space = Some(cut)
                }
            }
        }
        if cut < end {
            cut = line_break
                .or(if code { None } else { sentence })
                .or(space)
                .unwrap_or(cut);
        }
        if cut < end && source.as_bytes()[cut - 1] == b'\r' && source.as_bytes()[cut] == b'\n' {
            cut -= 1;
        }
        parts.push((cursor, cut));
        cursor = cut;
    }
    parts
}
#[cfg(test)]
pub fn chunks(content: &str, max_chars: usize) -> Vec<Chunk> {
    chunks_for_file(content, max_chars, true)
}
pub fn chunks_for_file(content: &str, max_chars: usize, markdown: bool) -> Vec<Chunk> {
    assert!(max_chars >= 2);
    let lines = Lines::new(content);
    let heading_re = Regex::new(r"^\s{0,3}(#{1,6})\s+(.+?)\s*#*\s*$").unwrap();
    let fence_re = Regex::new(r"^\s{0,3}(`{3,}|~{3,})").unwrap();
    let table_interrupt = Regex::new(r"^\s{0,3}(?:>|[-+*][ \t]|[0-9]+[.)][ \t])").unwrap();
    let mut headings = vec![];
    let mut start = None;
    let mut fence: Option<String> = None;
    let mut front = markdown
        && lines
            .spans
            .first()
            .is_some_and(|(s, e)| content[*s..*e].trim() == "---");
    let mut blocks: Vec<(String, usize, usize, bool, bool)> = vec![];
    let mut table_end = None;
    let flush = |last: usize,
                 start: &mut Option<usize>,
                 headings: &Vec<String>,
                 code: bool,
                 blocks: &mut Vec<(String, usize, usize, bool, bool)>| {
        if let Some(first) = start.take() {
            let s = lines.spans[first].0;
            let e = lines.spans[last].1;
            if !content[s..e].trim().is_empty() {
                blocks.push((
                    headings
                        .iter()
                        .filter(|s| !s.is_empty())
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" / "),
                    s,
                    e,
                    code,
                    false,
                ));
            }
        }
    };
    if !markdown {
        if !content.trim().is_empty() {
            blocks.push((String::new(), 0, content.len(), true, false));
        }
    } else {
        for (i, (s, e)) in lines.spans.iter().enumerate() {
            if table_end.is_some_and(|end| i <= end) { continue; }
            let line = &content[*s..*e];
            if front {
                if i > 0 && matches!(line.trim(), "---" | "...") {
                    front = false;
                }
                continue;
            }
            if let Some(m) = fence_re.captures(line) {
                let f = m[1].to_owned();
                if fence.is_none() {
                    if i > 0 {
                        flush(i - 1, &mut start, &headings, false, &mut blocks);
                    }
                    start = Some(i);
                    fence = Some(f);
                } else if fence
                    .as_ref()
                    .is_some_and(|old| old.starts_with(&f[..1]) && f.len() >= old.len())
                {
                    fence = None;
                    flush(i, &mut start, &headings, true, &mut blocks);
                }
                continue;
            }
            if fence.is_some() {
                continue;
            }
            if line.contains('|') && lines.spans.get(i + 1).is_some_and(|&(a, b)| table_separator(&content[a..b])) {
                if i > 0 { flush(i - 1, &mut start, &headings, false, &mut blocks); }
                let mut last = i + 1;
                while last + 1 < lines.spans.len() {
                    let (a, b) = lines.spans[last + 1];
                    let row = &content[a..b];
                    if !row.contains('|') || heading_re.is_match(row) || fence_re.is_match(row) || table_interrupt.is_match(row) { break; }
                    last += 1;
                }
                blocks.push((headings.iter().filter(|h| !h.is_empty()).cloned().collect::<Vec<_>>().join(" / "), *s, lines.spans[last].1, false, true));
                table_end = Some(last);
                continue;
            }
            if let Some(h) = heading_re.captures(line) {
                if i > 0 {
                    flush(i - 1, &mut start, &headings, false, &mut blocks);
                }
                let level = h[1].len();
                headings.resize(level, String::new());
                headings.truncate(level);
                headings[level - 1] = h[2].to_owned();
                continue;
            }
            if line.trim().is_empty() {
                if i > 0 {
                    flush(i - 1, &mut start, &headings, false, &mut blocks);
                }
            } else if start.is_none() {
                start = Some(i);
            }
        }
        flush(
            lines.spans.len() - 1,
            &mut start,
            &headings,
            fence.is_some(),
            &mut blocks,
        );
    }
    let overlap = if max_chars >= 160 { 80 } else { 0 };
    let body_limit = max_chars - if overlap > 0 { overlap + 2 } else { 0 };
    let mut result: Vec<Chunk> = vec![];
    let mut length = 0;
    let mut table_chunks = HashSet::new();
    for (heading, start, end, code, table) in blocks {
        if table {
            let first = lines.spans.partition_point(|&(a, _)| a < start);
            let last = lines.spans.partition_point(|&(a, _)| a <= end) - 1;
            let header_end = if first + 2 <= last { lines.spans[first + 2].0 } else { lines.spans[first + 1].1 };
            let header_size = content[start..header_end].encode_utf16().count();
            let mut row_start = first + 2;
            let mut size = header_size;
            for row in first + 2..=last {
                let (a, b) = lines.spans[row];
                let n = content[a..b].encode_utf16().count() + 2;
                if row > row_start && size + n > max_chars {
                    table_chunks.insert(result.len());
                    result.push(chunk_from_ranges(content, &lines, heading.clone(), &[(start, header_end), (lines.spans[row_start].0, lines.spans[row - 1].1)]));
                    row_start = row;
                    size = header_size;
                }
                size += n;
            }
            let mut parts = vec![(start, header_end)];
            if row_start <= last { parts.push((lines.spans[row_start].0, end)); }
            table_chunks.insert(result.len());
            result.push(chunk_from_ranges(content, &lines, heading, &parts));
            length = 0;
            continue;
        }
        for (s, e) in split_block(content, start, end, body_limit, code) {
            let part = &content[s..e];
            let n = part.encode_utf16().count();
            let span = lines.span(content, s, e);
            let last_line = if e > 0 && content.as_bytes()[e - 1] == b'\n' {
                span.end_line.saturating_sub(1).max(span.start_line)
            } else {
                span.end_line
            };
            if result.last().is_none_or(|c| c.heading != heading) || table_chunks.contains(&result.len().saturating_sub(1)) || length + 2 + n > body_limit {
                result.push(Chunk {
                    heading: heading.clone(),
                    content: part.into(),
                    ranges: vec![(span.start_line, last_line)],
                    spans: vec![span],
                });
                length = n;
            } else {
                let c = result.last_mut().unwrap();
                c.content.push_str("\n\n");
                c.content.push_str(part);
                c.ranges.push((span.start_line, last_line));
                c.spans.push(span);
                length += 2 + n;
            }
        }
    }
    // Work backwards so every prefix comes from the original, non-overlapping chunk.
    for i in (1..result.len()).rev() {
        if overlap == 0 || result[i - 1].heading != result[i].heading || table_chunks.contains(&i) || table_chunks.contains(&(i - 1)) { continue; }
        let mut remaining = overlap;
        let mut prefix = vec![];
        for span in result[i - 1].spans.iter().rev() {
            let mut start = span.end_byte;
            let mut used = 0;
            for (offset, ch) in content[span.start_byte..span.end_byte].char_indices().rev() {
                if used + ch.len_utf16() > remaining { break; }
                start = span.start_byte + offset;
                used += ch.len_utf16();
            }
            if start > span.start_byte {
                // Prefer a complete trailing sentence, then a word/line boundary.
                let tail = &content[start..span.end_byte];
                let boundary = tail.char_indices().find(|(offset, ch)|
                    "。！？；.!?;\n".contains(*ch) && offset + ch.len_utf8() < tail.len())
                    .or_else(|| tail.char_indices().find(|(offset, ch)| ch.is_whitespace() && offset + ch.len_utf8() < tail.len()));
                if let Some((offset, ch)) = boundary { start += offset + ch.len_utf8(); }
            }
            if start > 0 && content.as_bytes().get(start) == Some(&b'\n') && content.as_bytes()[start - 1] == b'\r' { start += 1; }
            if start < span.end_byte { prefix.push((start, span.end_byte)); }
            if start > span.start_byte || used + 2 >= remaining { break; }
            remaining -= used + 2;
        }
        prefix.reverse();
        prefix.extend(result[i].spans.iter().map(|span| (span.start_byte, span.end_byte)));
        result[i] = chunk_from_ranges(content, &lines, result[i].heading.clone(), &prefix);
    }
    result
}
fn table_separator(line: &str) -> bool {
    let line = line.trim();
    line.contains('|') && line.trim_matches('|').split('|').all(|cell| {
        let cell = cell.trim().trim_matches(':');
        !cell.is_empty() && cell.chars().all(|c| c == '-')
    })
}
fn chunk_from_ranges(source: &str, lines: &Lines, heading: String, parts: &[(usize, usize)]) -> Chunk {
    let mut merged: Vec<(usize, usize)> = vec![];
    for &(start, end) in parts {
        if let Some(previous) = merged.last_mut().filter(|previous| start <= previous.1) {
            previous.1 = previous.1.max(end);
        } else { merged.push((start, end)); }
    }
    let parts = &merged;
    let spans: Vec<_> = parts.iter().map(|&(start, end)| lines.span(source, start, end)).collect();
    let ranges = spans.iter().map(|span| {
        let last = if source.as_bytes()[span.end_byte - 1] == b'\n' {
            span.end_line.saturating_sub(1).max(span.start_line)
        } else { span.end_line };
        (span.start_line, last)
    }).collect();
    let content = source_content(source, &spans);
    Chunk { heading, content, ranges, spans }
}
pub(crate) fn source_content(source: &str, spans: &[crate::types::SourceSpan]) -> String {
    let mut result = String::new();
    for span in spans {
        let part = &source[span.start_byte..span.end_byte];
        if !result.is_empty() && !result.ends_with('\n') && !part.starts_with('\n') { result.push_str("\n\n"); }
        result.push_str(part);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    fn verify(source: &str, chunks: &[Chunk], limit: usize) {
        for c in chunks {
            assert!(c.content.encode_utf16().count() <= limit);
            assert_eq!(
                c.content,
                c.spans
                    .iter()
                    .map(|s| &source[s.start_byte..s.end_byte])
                    .collect::<Vec<_>>()
                    .join("\n\n")
            );
        }
    }
    fn unique_content(source: &str, chunks: &[Chunk]) -> String {
        let mut end = 0;
        let mut result = String::new();
        for span in chunks.iter().flat_map(|chunk| &chunk.spans) {
            if span.end_byte > end {
                result.push_str(&source[span.start_byte.max(end)..span.end_byte]);
                end = span.end_byte;
            }
        }
        result
    }
    #[test]
    fn adjacent_chunks_repeat_source_context_within_the_budget() {
        let source = format!("# Rules\n\n{}", "Each request uses the active production policy. ".repeat(60));
        let chunks = chunks_for_file(&source, 512, true);
        assert!(chunks.len() > 2);
        verify(&source, &chunks, 512);
        for pair in chunks.windows(2) {
            let previous = pair[0].spans.last().unwrap();
            let next = &pair[1].spans[0];
            assert!(next.start_byte < previous.end_byte, "adjacent chunks lost context");
            assert!(source[next.start_byte..previous.end_byte].encode_utf16().count() <= 80);
        }
        let separate = chunks_for_file("# Test\nfirst policy\n# Production\nsecond policy", 512, true);
        assert!(!separate[1].content.contains("first policy"));
    }

    #[test]
    fn tables_repeat_headers_keep_rows_and_do_not_absorb_neighboring_prose() {
        let header = "| Model | Score | Notes |\r\n| --- | ---: | --- |";
        let rows: Vec<_> = (0..20).map(|i| format!("| model-{i:02} | {i} | {} |", "detail ".repeat(9))).collect();
        let source = format!("# Benchmark\r\nIntroductory text.\r\n{header}\r\n{}\r\nFollowing paragraph.", rows.join("\r\n"));
        let chunks = chunks_for_file(&source, 512, true);
        let tables: Vec<_> = chunks.iter().filter(|chunk| chunk.content.starts_with(header)).collect();
        assert!(tables.len() > 1);
        assert!(chunks.iter().any(|chunk| chunk.content.contains("Following paragraph.")));
        for row in &rows {
            assert_eq!(tables.iter().filter(|chunk| chunk.content.contains(row)).count(), 1);
        }
        for chunk in tables {
            assert_eq!(chunk.heading, "Benchmark");
            assert!(chunk.content.encode_utf16().count() <= 512);
            assert!(!chunk.content.contains("| --- | ---: | --- |\r\n\n"));
            assert!(!chunk.content.contains("| --- | ---: | --- |\n\n"));
            for span in &chunk.spans { assert!(!source[span.start_byte..span.end_byte].contains("Introductory text.")); }
        }
        let row = format!("| long-row | 1 | {} |", "x".repeat(700));
        let oversized = chunks_for_file(&format!("{header}\r\n{row}"), 512, true);
        assert_eq!(oversized.len(), 1);
        assert!(oversized[0].content.contains(&row));
    }

    #[test]
    fn tables_stop_at_markdown_blocks_even_when_they_contain_pipes() {
        let source = "# Before\n| A | B |\n| --- | --- |\n| one | two |\n# After | Details\nFollowing paragraph.";
        let chunks = chunks_for_file(source, 512, true);
        assert!(chunks.iter().any(|c| c.heading == "After | Details" && c.content == "Following paragraph."));
        assert!(chunks.iter().any(|c| c.content == "| A | B |\n| --- | --- |\n| one | two |"));
        for block in ["```text\ncode | value\n```", "> quoted | value", "- list | value"] {
            let source = format!("| A | B |\n| --- | --- |\n| one | two |\n{block}");
            let chunks = chunks_for_file(&source, 512, true);
            assert!(!chunks[0].content.contains(block));
        }
    }

    #[test]
    fn very_long_single_line_keeps_unicode_columns_exact() {
        let source = "一😀a".repeat(350_000);
        let chunks = chunks_for_file(&source, 2200, false);
        let mut column = 1;
        for chunk in &chunks {
            let span = &chunk.spans[0];
            assert_eq!((span.start_line, span.end_line), (1, 1));
            assert!(span.start_column <= column);
            assert!(span.end_column > column);
            assert_eq!(span.end_column - span.start_column, chunk.content.chars().count());
            column = span.end_column;
            assert_eq!(&source[span.start_byte..span.end_byte], chunk.content);
        }
        assert_eq!(column, source.chars().count() + 1);
    }

    #[test]
    fn keeps_fences_and_ranges() {
        let source = "---\na: 1\n---\n# 标题\n\n第一段。\n\n```rs\n# not a heading\n```\n\n第二段";
        let c = chunks(source, 2200);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].ranges, vec![(6, 6), (8, 10), (12, 12)]);
        assert_eq!(c[0].heading, "标题");
        verify(source, &c, 2200);
    }
    #[test]
    fn long_single_line_is_split_without_loss() {
        let source = "很长的一行😀".repeat(1600);
        let c = chunks(&source, 2200);
        assert!(c.len() > 3);
        verify(&source, &c, 2200);
        assert_eq!(
            unique_content(&source, &c),
            source
        );
        assert!(c[1].spans[0].start_column > 1);
    }
    #[test]
    fn long_code_and_crlf_keep_exact_positions() {
        let source = format!(
            "# Code\r\n\r\n```rust\r\n{}\r\n```",
            "let value = 123;\r\n".repeat(600)
        );
        let c = chunks(&source, 400);
        verify(&source, &c, 400);
        assert_eq!(
            unique_content(&source, &c),
            &source[10..]
        );
    }
    #[test]
    fn code_files_do_not_parse_hashes_or_frontmatter() {
        let source = "---\r\n# not a title\r\n".repeat(100);
        let c = chunks_for_file(&source, 400, false);
        verify(&source, &c, 400);
        assert_eq!(
            unique_content(&source, &c),
            source
        );
    }
    #[test]
    fn utf16_budget_and_headings() {
        assert_eq!(chunks("😀😀\n\na", 4).len(), 2);
        assert_eq!(chunks("# A\none\n# B\ntwo", 999).len(), 2);
    }
    #[test]
    fn query_uses_terms_not_sql() {
        assert!(!match_query("' OR 1=1 --").contains('\''));
        assert_eq!(match_query("the and 是"), "");
        assert_eq!(search_terms("ＯＣＴＸ octx the"), vec!["octx"]);
        assert_eq!(match_query("ＯＣＴＸ octx the"), "\"octx\"");
        assert_eq!(index_text("ＦＩＬＥ"), "file");
    }
    #[test]
    fn unicode_columns_and_crlf_boundaries_are_exact() {
        let c = chunks("😀中文甲乙", 4);
        assert_eq!(c[0].content, "😀中文");
        assert_eq!((c[0].spans[0].start_byte, c[0].spans[0].end_byte), (0, 10));
        assert_eq!(
            (c[0].spans[0].start_column, c[0].spans[0].end_column),
            (1, 4)
        );
        assert_eq!(c[1].spans[0].start_column, 4);
        let source = format!("{}\r\nnext", "a".repeat(399));
        let c = chunks_for_file(&source, 400, false);
        verify(&source, &c, 400);
        for span in c.iter().flat_map(|chunk| &chunk.spans) {
            assert!(!(span.start_byte > 0 && source.as_bytes()[span.start_byte] == b'\n' && source.as_bytes()[span.start_byte - 1] == b'\r'));
            assert!(!(span.end_byte < source.len() && source.as_bytes()[span.end_byte] == b'\n' && source.as_bytes()[span.end_byte - 1] == b'\r'));
        }
        assert_eq!(
            unique_content(&source, &c),
            source
        );
    }
    #[test]
    fn file_types_are_explicit() {
        for name in [
            "x.MD",
            "x.py",
            "x.rs",
            "x.ts",
            "x.txt",
            "x.yaml",
            "Dockerfile",
        ] {
            assert!(is_supported(std::path::Path::new(name)));
        }
        for name in ["x.pdf", "x.png", ".env", "x.exe"] {
            assert!(!is_supported(std::path::Path::new(name)));
        }
    }
    #[test]
    fn cache_eviction_preserves_tokenization() {
        for index in 0..8_300 {
            let source = format!("文档标识符 marker_{index} mixedCase");
            let expected = tokenize_uncached(&source);
            assert_eq!(tokenize(&source), expected);
            assert_eq!(tokenize(&source), expected);
        }
        assert!(TOKEN_CACHE.lock().unwrap().len() <= 8192);
    }

}
