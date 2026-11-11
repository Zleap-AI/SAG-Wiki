use jieba_rs::Jieba;
use regex::Regex;
use std::{collections::HashSet, sync::{Arc, LazyLock, Mutex}};
use indexmap::IndexMap;
use unicode_normalization::UnicodeNormalization;

/// A resource rejection is a file issue, not permission to publish partial text.
#[derive(Debug)]
pub(crate) struct ResourceLimit(pub String);
impl std::fmt::Display for ResourceLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "索引文本超过资源上限：{}；原文件未修改，本次文件索引未写入", self.0)
    }
}
impl std::error::Error for ResourceLimit {}

pub(crate) const MAX_INDEX_LINE_BYTES: usize = 256 * 1024;
pub(crate) const MAX_INDEX_STRUCTURE_ITEMS: usize = 131_072;

fn charge(total: &mut usize, bytes: usize, limit: usize) -> Result<(), ResourceLimit> {
    *total = total.checked_add(bytes).filter(|total| *total <= limit)
        .ok_or_else(|| ResourceLimit(format!("分块及元数据预算为 {} MiB", limit / 1024 / 1024)))?;
    Ok(())
}

static JIEBA: LazyLock<Jieba> = LazyLock::new(Jieba::new);
static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\p{L}\p{N}]").unwrap());
static STOPS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    "的 了 是 在 和 与 或 哪 什么 哪个 哪些 如何 怎么 为什么 是否 这 那 一个 一种 请 我 你 他 它 吗 呢 吧 地 得 the a an of in on at to for and or is are was were be by with from which what who how when where does do did that this it as".split_whitespace().collect()
});
const TOKEN_CACHE_MAX_BYTES: usize = 8 * 1024 * 1024;
const TOKEN_CACHE_MAX_INPUT_BYTES: usize = 8192;
#[derive(Default)]
struct TokenCache {
    entries: IndexMap<String, Vec<String>>,
    bytes: usize,
}
impl TokenCache {
    fn entry_bytes(source: &str, tokens: &[String]) -> usize {
        source.len() + std::mem::size_of_val(tokens) + tokens.iter().map(String::len).sum::<usize>()
    }
    fn insert(&mut self, source: &str, tokens: &[String]) {
        let bytes = Self::entry_bytes(source, tokens);
        if bytes > TOKEN_CACHE_MAX_BYTES { return; }
        if let Some(previous) = self.entries.swap_remove(source) {
            self.bytes -= Self::entry_bytes(source, &previous);
        }
        while self.bytes + bytes > TOKEN_CACHE_MAX_BYTES || self.entries.len() >= 8192 {
            let (source, tokens) = self.entries.swap_remove_index(0).unwrap();
            self.bytes -= Self::entry_bytes(&source, &tokens);
        }
        self.entries.insert(source.to_owned(), tokens.to_vec());
        self.bytes += bytes;
    }
}
static TOKEN_CACHE: LazyLock<Mutex<TokenCache>> = LazyLock::new(|| Mutex::new(TokenCache::default()));
pub fn tokenize(text: &str) -> Vec<String> {
    // Large source lines (for example embedded images) must not stay resident.
    if text.len() > TOKEN_CACHE_MAX_INPUT_BYTES { return tokenize_uncached(text); }
    {
        let mut cache = TOKEN_CACHE.lock().expect("token cache mutex poisoned");
        // Cache eviction order does not affect tokens; avoid shifting thousands of entries.
        if let Some(tokens) = cache.entries.swap_remove(text) {
            cache.entries.insert(text.to_owned(), tokens.clone());
            return tokens;
        }
    }
    let tokens = tokenize_uncached(text);
    let mut cache = TOKEN_CACHE.lock().expect("token cache mutex poisoned");
    cache.insert(text, &tokens);
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
    explore_summary_bounded(path, chunks, usize::MAX).expect("unlimited explore summary")
}
pub(crate) fn explore_summary_bounded<'a>(path: &std::path::Path, chunks: impl Iterator<Item = (&'a str, &'a str)>, byte_limit: usize) -> Result<Vec<crate::types::DocumentTerm>, ResourceLimit> {
    let code = matches!(file_kind(path), Some(FileKind::Code | FileKind::Config));
    let title: HashSet<_> = tokenize(path.file_stem().and_then(|s| s.to_str()).unwrap_or("")).into_iter().collect();
    let mut headings = HashSet::new();
    let mut counts = std::collections::HashMap::<String, usize>::new();
    let mut seen_passages = HashSet::new();
    let mut bytes = 0usize;
    for (heading, content) in chunks {
        headings.extend(tokenize(heading));
        if !seen_passages.insert(content.trim()) { continue; }
        charge(&mut bytes, 64, byte_limit)?;
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
            if let Some(count) = counts.get_mut(term) { *count += 1; }
            else {
                charge(&mut bytes, 64 + term.len() * 2, byte_limit)?;
                counts.insert(term.to_owned(), 1);
            }
        }
    }
    let mut terms: Vec<_> = counts.into_iter().map(|(term, count)| {
        let boost = 1.0 + if title.contains(&term) { 1.5 } else { 0.0 } + if headings.contains(&term) { 0.5 } else { 0.0 };
        crate::types::DocumentTerm { term, count, boost }
    }).collect();
    terms.sort_by(|a,b| (b.boost * (1.0 + (b.count as f64).ln())).total_cmp(&(a.boost * (1.0 + (a.count as f64).ln()))).then(a.term.cmp(&b.term)));
    terms.truncate(64);
    Ok(terms)
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
pub(crate) fn indexable_ranges(source: &str, path: &std::path::Path) -> Vec<(usize, usize)> {
    if !path.extension().is_some_and(|extension| ["html", "htm", "md", "markdown"].iter().any(|kind| extension.eq_ignore_ascii_case(kind))) {
        return vec![(0, source.len())];
    }
    static RESOURCE_DATA: LazyLock<Regex> = LazyLock::new(|| Regex::new(
        r#"(?i)data:(?:image|font|audio|video|application)/[a-z0-9.+-]+(?:;[^;,\s"'<>]+)*,"#
    ).unwrap());
    static SVG_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<(/?)svg\b[^>]*>").unwrap());
    let bytes = source.as_bytes();
    let mut excluded = vec![];
    // Keep descriptions and their original source offsets, but never index
    // image destinations (including opaque hashes and signed URL parameters).
    static IMAGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
        r"!\[((?:\\.|\[[^\]\n]*\]|[^\]\\])*)\]\((?:\\.|[^()\n]|\([^()\n]*\))*\)"
    ).unwrap());
    for image in IMAGE.captures_iter(source) {
        let whole = image.get(0).unwrap();
        let alt = image.get(1).unwrap();
        excluded.push((whole.start(), alt.start()));
        excluded.push((alt.end(), whole.end()));
    }
    static REFERENCE_IMAGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
        r"!\[([^\]\n]*)\](?:\[([^\]\n]*)\])?"
    ).unwrap());
    static DEFINITION: LazyLock<Regex> = LazyLock::new(|| Regex::new(
        r"(?m)^ {0,3}\[([^\]\n]+)\]:[ \t]*(?:\r?\n[ \t]+)?[^\r\n]*"
    ).unwrap());
    let normalize_label = |label: &str| label.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let references: Vec<_> = REFERENCE_IMAGE.captures_iter(source).filter_map(|image| {
        let whole = image.get(0).unwrap();
        if source[whole.end()..].starts_with('(') { return None; }
        let alt = image.get(1).unwrap();
        let label = image.get(2).filter(|m| !m.as_str().is_empty()).unwrap_or(alt);
        excluded.push((whole.start(), alt.start()));
        excluded.push((alt.end(), whole.end()));
        Some(normalize_label(label.as_str()))
    }).collect();
    for definition in DEFINITION.captures_iter(source) {
        if references.contains(&normalize_label(&definition[1])) {
            let whole = definition.get(0).unwrap();
            excluded.push((whole.start(), whole.end()));
        }
    }
    static HTML_IMAGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?is)<img\b(?:"[^"]*"|'[^']*'|[^'">])*>"#).unwrap());
    static ALT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?is)\balt\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#).unwrap());
    for image in HTML_IMAGE.find_iter(source) {
        if let Some(alt) = ALT.captures(image.as_str()).and_then(|c| c.get(1).or_else(|| c.get(2)).or_else(|| c.get(3))) {
            excluded.push((image.start(), image.start() + alt.start()));
            excluded.push((image.start() + alt.end(), image.end()));
        } else {
            excluded.push((image.start(), image.end()));
        }
    }
    for resource in RESOURCE_DATA.find_iter(source) {
        let quoted = resource.start() > 0 && matches!(bytes[resource.start() - 1], b'\'' | b'"');
        let base64 = resource.as_str().to_ascii_lowercase().contains(";base64,");
        let mut end = resource.end();
        while end < bytes.len() {
            let byte = bytes[end];
            let allowed = if base64 {
                byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=')
                    || quoted && byte.is_ascii_whitespace()
            } else {
                !byte.is_ascii_whitespace() && !matches!(byte, b'\'' | b'"' | b'<' | b'>' | b')')
            };
            if !allowed { break; }
            end += 1;
        }
        excluded.push((resource.start(), end));
    }
    let (mut svg_depth, mut svg_start) = (0usize, 0usize);
    for tag in SVG_TAG.captures_iter(source) {
        let whole = tag.get(0).unwrap();
        if &tag[1] == "/" {
            if svg_depth > 0 {
                svg_depth -= 1;
                if svg_depth == 0 { excluded.push((svg_start, whole.end())); }
            }
        } else if whole.as_str().ends_with("/>") {
            if svg_depth == 0 { excluded.push((whole.start(), whole.end())); }
        } else {
            if svg_depth == 0 { svg_start = whole.start(); }
            svg_depth += 1;
        }
    }
    if svg_depth > 0 { excluded.push((svg_start, source.len())); }
    excluded.sort_unstable();
    let mut ranges = vec![];
    let mut cursor = 0;
    for (start, end) in excluded {
        if cursor < start { ranges.push((cursor, start)); }
        cursor = cursor.max(end);
    }
    if cursor < source.len() { ranges.push((cursor, source.len())); }
    ranges
}
pub(crate) fn indexable_text<'a>(source: &'a str, path: &std::path::Path) -> std::borrow::Cow<'a, str> {
    let ranges = indexable_ranges(source, path);
    if ranges == [(0, source.len())] { return std::borrow::Cow::Borrowed(source); }
    let mut text = String::new();
    for (start, end) in ranges {
        if !text.is_empty() { text.push('\n'); }
        text.push_str(&source[start..end]);
    }
    std::borrow::Cow::Owned(text)
}
enum FileKind { Markdown, Text, Code, Config }
fn file_kind(path: &std::path::Path) -> Option<FileKind> {
    if path.file_name().and_then(|name| name.to_str()).is_some_and(|name| {
        matches!(name, "Dockerfile" | "Containerfile" | "Makefile" | "CMakeLists.txt")
    }) {
        return Some(FileKind::Code);
    }
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if MARKDOWN_EXTENSIONS.contains(&extension.as_str()) { Some(FileKind::Markdown) }
    else if TEXT_EXTENSIONS.contains(&extension.as_str()) { Some(FileKind::Text) }
    else if CODE_EXTENSIONS.contains(&extension.as_str()) { Some(FileKind::Code) }
    else if CONFIG_EXTENSIONS.contains(&extension.as_str()) { Some(FileKind::Config) }
    else { None }
}
const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown"];
const TEXT_EXTENSIONS: &[&str] = &["txt", "rst", "rtf"];
const CODE_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "jsx", "ts", "tsx", "mjs", "cjs", "go", "java", "c", "h", "cpp", "hpp", "cc", "cs",
    "rb", "php", "swift", "kt", "kts", "scala", "sh", "bash", "zsh", "ps1", "sql", "html", "htm", "css",
    "scss", "vue", "svelte", "r", "lua", "dart",
];
const CONFIG_EXTENSIONS: &[&str] = &["json", "jsonc", "yaml", "yml", "toml", "ini", "cfg", "conf", "xml"];
pub fn enabled_extensions(types: &crate::types::FileTypes) -> Vec<String> {
    [(types.markdown, MARKDOWN_EXTENSIONS), (types.text, TEXT_EXTENSIONS),
     (types.text, crate::anydoc::EXTENSIONS),
     (types.code, CODE_EXTENSIONS), (types.config, CONFIG_EXTENSIONS)]
        .into_iter().filter(|(enabled, _)| *enabled)
        .flat_map(|(_, extensions)| extensions.iter().map(|extension| (*extension).into())).collect()
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
// Sentence/line boundaries retain exact source offsets, including closing quotes.
fn sentence_ranges(source: &str, start: usize, end: usize, code: bool) -> impl Iterator<Item = (usize, usize)> + '_ {
    let mut cursor = start;
    let mut chars = source[start..end].char_indices().peekable();
    std::iter::from_fn(move || {
        while let Some((offset, ch)) = chars.next() {
            let boundary = if code {
                ch == '\n'
            } else {
                "。！？!?".contains(ch)
                    || ch == '.' && chars.peek().is_none_or(|(_, next)| next.is_whitespace() || "\"'”’）)]}".contains(*next))
            };
            if !boundary { continue; }
            let mut cut = start + offset + ch.len_utf8();
            if !code {
                while let Some(&(offset, ch)) = chars.peek() {
                    if !(ch.is_whitespace() || "。！？!?\"'”’）)]}".contains(ch)) { break; }
                    cut = start + offset + ch.len_utf8();
                    chars.next();
                }
            }
            let part = (cursor, cut);
            cursor = cut;
            return Some(part);
        }
        if cursor < end {
            let part = (cursor, end);
            cursor = end;
            Some(part)
        } else { None }
    })
}

// Exceptionally long sentences/tables are split at word boundaries when possible.
// Original byte ranges, Unicode columns and CRLF pairs remain intact.
fn split_oversized_range(source: &str, mut start: usize, end: usize, limit: usize) -> Vec<(usize, usize)> {
    let limit = limit.min(MAX_INDEX_LINE_BYTES / 4);
    let mut parts = vec![];
    while start < end {
        let mut length = 0;
        let mut whitespace = None;
        let mut cut = end;
        for (offset, ch) in source[start..end].char_indices() {
            if length + ch.len_utf16() > limit {
                cut = whitespace.unwrap_or(start + offset);
                if source.as_bytes()[cut - 1] == b'\r' && source.as_bytes().get(cut) == Some(&b'\n') { cut -= 1; }
                break;
            }
            length += ch.len_utf16();
            if ch.is_whitespace() { whitespace = Some(start + offset + ch.len_utf8()); }
        }
        parts.push((start, cut));
        start = cut;
    }
    parts
}

// 512 is a target; only resource-sized sentences/lines need the bounded fallback.
fn split_block(
    source: &str,
    start: usize,
    end: usize,
    limit: usize,
    code: bool,
    bounded: bool,
) -> Vec<(usize, usize)> {
    let mut parts = vec![];
    let mut cursor = start;
    let mut length = 0;
    let sentences = sentence_ranges(source, start, end, code).flat_map(|(s, e)| {
        if bounded && e - s > MAX_INDEX_LINE_BYTES { split_oversized_range(source, s, e, limit) }
        else { vec![(s, e)] }
    });
    for (s, e) in sentences {
        let size = source[s..e].encode_utf16().count();
        if length > 0 && length + size > limit {
            parts.push((cursor, s));
            cursor = s;
            length = 0;
        }
        length += size;
    }
    if cursor < end { parts.push((cursor, end)); }
    parts
}
#[cfg(test)]
pub fn chunks(content: &str, max_chars: usize) -> Vec<Chunk> {
    chunks_for_file(content, max_chars, std::path::Path::new("document.md"))
}
pub fn chunks_for_file(content: &str, max_chars: usize, path: &std::path::Path) -> Vec<Chunk> {
    build_chunks(content, max_chars, path, None).expect("unlimited chunk construction")
}

pub(crate) fn chunks_for_index(content: &str, max_chars: usize, path: &std::path::Path, byte_limit: usize) -> Result<Vec<Chunk>, ResourceLimit> {
    build_chunks(content, max_chars, path, Some(byte_limit))
}

fn build_chunks(content: &str, max_chars: usize, path: &std::path::Path, byte_limit: Option<usize>) -> Result<Vec<Chunk>, ResourceLimit> {
    let markdown = is_markdown(path);
    let code = matches!(file_kind(path), Some(FileKind::Code | FileKind::Config));
    assert!(max_chars >= 2);
    let ranges = indexable_ranges(content, path);
    if byte_limit.is_some() {
        if content.bytes().enumerate().filter(|&(i, byte)| byte == b'\n' || byte == b'\r' && content.as_bytes().get(i + 1) != Some(&b'\n')).count() > MAX_INDEX_STRUCTURE_ITEMS {
            return Err(ResourceLimit(format!("换行数量不得超过 {MAX_INDEX_STRUCTURE_ITEMS}")));
        }
        let mut boundaries = 0usize;
        for &(start, end) in &ranges {
            boundaries += sentence_ranges(content, start, end, code).take(MAX_INDEX_STRUCTURE_ITEMS + 1).count();
            if boundaries > MAX_INDEX_STRUCTURE_ITEMS {
                return Err(ResourceLimit(format!("句子/行边界数量不得超过 {MAX_INDEX_STRUCTURE_ITEMS}")));
            }
        }
    }
    let lines = Lines::new(content);
    let heading_re = Regex::new(r"^\s{0,3}(#{1,6})\s+(.+?)\s*#*\s*$").unwrap();
    let fence_re = Regex::new(r"^\s{0,3}(`{3,}|~{3,})").unwrap();
    let mut headings = vec![];
    let mut current_heading: Arc<str> = Arc::from("");
    let mut start = None;
    let mut fence: Option<String> = None;
    let mut front = markdown
        && lines
            .spans
            .first()
            .is_some_and(|(s, e)| content[*s..*e].trim() == "---");
    let mut blocks: Vec<(Arc<str>, usize, usize, bool)> = vec![];
    let mut block_bytes = 0usize;
    let flush = |last: usize,
                 start: &mut Option<usize>,
                 heading: &Arc<str>,
                 code: bool,
                 blocks: &mut Vec<(Arc<str>, usize, usize, bool)>,
                 block_bytes: &mut usize| -> Result<(), ResourceLimit> {
        if let Some(first) = start.take() {
            let s = lines.spans[first].0;
            let e = lines.spans[last].1;
            if !content[s..e].trim().is_empty() {
                if let Some(limit) = byte_limit {
                    charge(block_bytes, std::mem::size_of::<(Arc<str>, usize, usize, bool)>(), limit)?;
                }
                blocks.push((heading.clone(), s, e, code));
            }
        }
        Ok(())
    };
    if !markdown {
        if !content.trim().is_empty() {
            blocks.push((current_heading.clone(), 0, content.len(), code));
        }
    } else {
        for (i, (s, e)) in lines.spans.iter().enumerate() {
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
                        flush(i - 1, &mut start, &current_heading, false, &mut blocks, &mut block_bytes)?;
                    }
                    start = Some(i);
                    fence = Some(f);
                } else if fence
                    .as_ref()
                    .is_some_and(|old| old.starts_with(&f[..1]) && f.len() >= old.len())
                {
                    fence = None;
                    flush(i, &mut start, &current_heading, true, &mut blocks, &mut block_bytes)?;
                }
                continue;
            }
            if fence.is_some() {
                continue;
            }
            if let Some(h) = heading_re.captures(line) {
                if i > 0 {
                    flush(i - 1, &mut start, &current_heading, false, &mut blocks, &mut block_bytes)?;
                }
                let level = h[1].len();
                if byte_limit.is_some() && h[2].len() > 16 * 1024 {
                    return Err(ResourceLimit("Markdown 标题不得超过 16 KiB".into()));
                }
                headings.resize(level, String::new());
                headings.truncate(level);
                headings[level - 1] = h[2].to_owned();
                current_heading = headings.iter().filter(|heading| !heading.is_empty())
                    .cloned().collect::<Vec<_>>().join(" / ").into();
                if let Some(limit) = byte_limit {
                    charge(&mut block_bytes, current_heading.len() + 2 * std::mem::size_of::<usize>(), limit)?;
                }
                continue;
            }
            if line.trim().is_empty() {
                if i > 0 {
                    flush(i - 1, &mut start, &current_heading, false, &mut blocks, &mut block_bytes)?;
                }
            } else if start.is_none() {
                start = Some(i);
            }
        }
        flush(
            lines.spans.len() - 1,
            &mut start,
            &current_heading,
            fence.is_some(),
            &mut blocks,
            &mut block_bytes,
        )?;
    }
    let overlap = if max_chars >= 160 { 80 } else { 0 };
    let body_limit = max_chars - if overlap > 0 { overlap + 2 } else { 0 };
    let mut result: Vec<Chunk> = vec![];
    let mut sentences: Vec<Vec<(usize, usize)>> = vec![];
    let mut length = 0;
    let mut result_bytes = 0usize;
    // Cut out embedded resource data before splitting, retaining original offsets
    // so search snippets and citations can still be read from the source file.
    let blocks = blocks.into_iter().flat_map(|(heading, start, end, code)| {
        ranges.iter().filter_map(move |&(allowed_start, allowed_end)| {
            let start = start.max(allowed_start);
            let end = end.min(allowed_end);
            (start < end).then(|| (heading.clone(), start, end, code))
        })
    });
    for (heading, start, end, code) in blocks {
        for (s, e) in split_block(content, start, end, body_limit, code, byte_limit.is_some()) {
            let part = &content[s..e];
            if byte_limit.is_some() && part.len() > MAX_INDEX_LINE_BYTES {
                return Err(ResourceLimit("单个完整句子/代码行不得超过 256 KiB".into()));
            }
            let sentence_parts: Vec<_> = sentence_ranges(content, s, e, code).collect();
            let n = part.encode_utf16().count();
            let new_chunk = result.last().is_none_or(|c| c.heading != heading.as_ref()) || length + 2 + n > body_limit;
            if let Some(limit) = byte_limit {
                let metadata = std::mem::size_of::<crate::types::SourceSpan>()
                    + std::mem::size_of::<(usize, usize)>() * (1 + sentence_parts.len());
                // Merged paragraphs share the final chunk's heading and container metadata.
                let overhead = if new_chunk {
                    heading.len() + std::mem::size_of::<Chunk>() + std::mem::size_of::<Vec<(usize, usize)>>()
                } else { 2 };
                charge(&mut result_bytes, part.len() + metadata + overhead, limit)?;
            }
            let span = lines.span(content, s, e);
            let last_line = if e > 0 && content.as_bytes()[e - 1] == b'\n' {
                span.end_line.saturating_sub(1).max(span.start_line)
            } else {
                span.end_line
            };
            if new_chunk {
                result.push(Chunk {
                    heading: heading.to_string(),
                    content: part.into(),
                    ranges: vec![(span.start_line, last_line)],
                    spans: vec![span],
                });
                sentences.push(sentence_parts);
                length = n;
            } else {
                let c = result.last_mut().unwrap();
                c.content.push_str("\n\n");
                c.content.push_str(part);
                c.ranges.push((span.start_line, last_line));
                c.spans.push(span);
                sentences.last_mut().unwrap().extend(sentence_parts);
                length += 2 + n;
            }
        }
    }
    // Work backwards so every prefix comes from the original, non-overlapping chunk.
    for i in (1..result.len()).rev() {
        if overlap == 0 || result[i - 1].heading != result[i].heading { continue; }
        let mut used = 0;
        let mut prefix = vec![];
        for &(start, end) in sentences[i - 1].iter().rev() {
            let size = content[start..end].encode_utf16().count();
            // Keep at least the last complete sentence, even when it exceeds 80.
            if !prefix.is_empty() && used + size > overlap { break; }
            prefix.push((start, end));
            used += size;
            if used >= overlap { break; }
        }
        prefix.reverse();
        if let Some(limit) = byte_limit {
            let extra = prefix.iter().map(|(start, end)| end - start + 2 + std::mem::size_of::<crate::types::SourceSpan>() + std::mem::size_of::<(usize, usize)>()).sum();
            charge(&mut result_bytes, extra, limit)?;
        }
        prefix.extend(result[i].spans.iter().map(|span| (span.start_byte, span.end_byte)));
        result[i] = chunk_from_ranges(content, &lines, result[i].heading.clone(), &prefix);
    }
    Ok(result)
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
    #[test]
    fn bounded_index_counts_crlf_as_one_line_break() {
        let source = format!("first marker.\r\n{}last marker.", "\r\n".repeat(70_000));
        let chunks = chunks_for_index(&source, 512, std::path::Path::new("policy.md"), 16 * 1024 * 1024).unwrap();
        assert!(chunks.first().unwrap().content.contains("first marker."));
        assert!(chunks.last().unwrap().content.contains("last marker."));
        assert_eq!(chunks.last().unwrap().spans.last().unwrap().end_line, 70_002);
    }

    #[test]
    fn image_destinations_are_not_indexed_and_descriptions_keep_source_spans() {
        let source = "正文。\n![光谱响应](images/opaquehash.png)\n图 3 灵敏度。\n<img src='https://example.com/privatehash?signature=secret' alt='曲线说明'>\n[普通链接](https://example.com/reference)";
        let path = std::path::Path::new("report.pdf.md");
        let text = indexable_text(source, path);
        for marker in ["光谱响应", "图 3 灵敏度", "曲线说明", "https://example.com/reference"] {
            assert!(text.contains(marker), "lost {marker}");
        }
        let chunks = chunks_for_file(source, 160, path);
        for chunk in chunks {
            for marker in ["opaquehash", "privatehash", "signature", "secret"] {
                assert!(!chunk.content.contains(marker), "indexed {marker}");
            }
            for span in chunk.spans {
                assert!(chunk.content.contains(&source[span.start_byte..span.end_byte]));
            }
        }
        assert!(indexable_text("sha256 opaquehash", path).contains("opaquehash"));
        let reference = "![曲线][figure]\n\n[figure]: images/referencehash.png\n\n正文";
        let indexed = indexable_text(reference, path);
        assert!(indexed.contains("曲线") && indexed.contains("正文"));
        assert!(!indexed.contains("referencehash"));
        let variants = "![curve [red]](images/nestedhash.png)\n<img alt=\"a > b\" src=\"images/htmlhash.png\">\n![figure][f]\n[f]:\n  images/multilinehash.png";
        let filtered = indexable_text(variants, path);
        for hash in ["nestedhash", "htmlhash", "multilinehash"] { assert!(!filtered.contains(hash)); }
        for caption in ["curve", "a > b", "figure"] { assert!(filtered.contains(caption)); }
    }

    #[test]
    fn bounded_index_does_not_count_decimal_points_and_dot_leaders_as_sentences() {
        let source = "wind-treating therapy......................................................3.14\r\n\r\n".repeat(5000);
        let chunks = chunks_for_index(&source, 512, std::path::Path::new("policy.md"), 16 * 1024 * 1024).unwrap();
        assert!(!chunks.is_empty());
        assert!(chunks.last().unwrap().content.contains("3.14"));
    }

    #[test]
    fn bounded_index_splits_oversized_table_lines_without_losing_source() {
        let source = format!("车型名称 纯电动续驶里程 {}尾部车型标记", "汽车企业 CA5310 😀 续驶里程225 ".repeat(10_000));
        let chunks = chunks_for_index(&source, 512, std::path::Path::new("policy.md"), 16 * 1024 * 1024).unwrap();
        assert!(chunks.len() > 1);
        let mut covered = 0;
        for chunk in &chunks {
            assert!(chunk.content.len() <= 4096, "oversized chunks still reach the tokenizer");
            for span in &chunk.spans {
                assert_eq!((span.start_line, span.end_line), (1, 1));
                assert!(span.start_byte <= covered);
                assert_eq!(span.start_column, source[..span.start_byte].chars().count() + 1);
                assert_eq!(span.end_column, source[..span.end_byte].chars().count() + 1);
                assert_eq!(source_content(&source, &chunk.spans), chunk.content);
                covered = covered.max(span.end_byte);
            }
        }
        assert_eq!(covered, source.len());
        assert!(chunks.last().unwrap().content.contains("尾部车型标记"));
    }

    #[test]
    fn bounded_chunks_count_a_long_heading_once_per_merged_chunk() {
        let source = format!("# {}\n\n{}", "heading".repeat(100), "Evidence sentence.\n\n".repeat(200));
        let expected = chunks(&source, 512);
        let actual = chunks_for_index(&source, 512, std::path::Path::new("test.md"), 32 * 1024).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(unique_content(&source, &actual), "Evidence sentence.".repeat(200));
    }

    #[test]
    fn bounded_chunks_reject_heading_amplification_and_metadata_without_truncation() {
        let source = format!("# {}\n\n{}", "heading".repeat(100), "Evidence sentence.\n\n".repeat(200));
        assert!(chunks_for_index(&source, 512, std::path::Path::new("test.md"), 4096).is_err());
        let normal = chunks_for_index("# Policy\n\nComplete evidence.", 512, std::path::Path::new("test.md"), 4096).unwrap();
        assert_eq!(normal.len(), 1);
        assert_eq!(normal[0].content, "Complete evidence.");
    }

    #[test]
    fn bounded_chunks_still_skip_large_embedded_resource_payloads() {
        let source = format!("before <img src='data:image/png;base64,{}'> after", "a".repeat(MAX_INDEX_LINE_BYTES + 1));
        let chunks = chunks_for_index(&source, 512, std::path::Path::new("test.md"), 4096).unwrap();
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.contains("before"));
        assert!(chunks[0].content.contains("after"));
        assert!(!chunks[0].content.contains("data:image"));
    }

    #[test]
    fn bounded_summary_rejects_excess_unique_terms_instead_of_dropping_evidence() {
        assert!(explore_summary_bounded(std::path::Path::new("test.txt"), [("", "alpha beta gamma")].into_iter(), 64).is_err());
        let terms = explore_summary_bounded(std::path::Path::new("test.txt"), [("", "alpha alpha")].into_iter(), 256).unwrap();
        assert_eq!(terms[0].term, "alpha");
        assert_eq!(terms[0].count, 2);
    }
    #[test]
    fn embedded_resources_are_excluded_without_removing_ordinary_scripts() {
        let source = concat!(
            "<p>beforemarker</p><svg><text>svgpayloadmarker</text><svg><path d='nestedmarker'/></svg></svg>",
            "<img src='data:image/svg+xml,%3Csvg%3Eencodedsvgmarker%3C/svg%3E'>",
            "<style>@font-face{src:url(data:font/woff2;base64,fontpayloadmarker)}</style>",
            "<audio src='data:audio/mpeg;base64,audiopayloadmarker'></audio>",
            "<video src='data:video/mp4;base64,videopayloadmarker'></video>",
            "<a download='file.pdf' href='data:application/pdf;base64,attachmentpayloadmarker'>附件说明</a>",
            "<script>ordinaryscriptmarker(); //# sourceMappingURL=data:application/json;base64,sourcemapmarker</script>",
            "<style>.ordinarystylemarker{color:red}</style><p>aftermarker</p>"
        );
        for extension in ["html", "md", "MD"] {
            let chunks = chunks_for_file(source, 512, std::path::Path::new(&format!("resources.{extension}")));
            let content = chunks.iter().map(|chunk| chunk.content.as_str()).collect::<String>();
            for marker in ["svgpayloadmarker", "nestedmarker", "encodedsvgmarker", "fontpayloadmarker", "audiopayloadmarker", "videopayloadmarker", "attachmentpayloadmarker", "sourcemapmarker"] {
                assert!(!content.contains(marker), "{extension} retained {marker}");
            }
            for marker in ["beforemarker", "aftermarker", "附件说明", "ordinaryscriptmarker", "ordinarystylemarker"] {
                assert!(content.contains(marker), "{extension} removed {marker}");
            }
            for chunk in chunks { assert_eq!(chunk.content, source_content(source, &chunk.spans)); }
        }
    }

    #[test]
    fn html_chunks_exclude_image_data_and_preserve_source_spans() {
        let source = concat!(
            "<h1>正文标题</h1>\r\n<p>beforemarker</p><img alt='图像说明' src='data:image/png;base64,aW1hZ2VwYXlsb2Fk'>",
            "<p>aftermarker</p>\r\n<style>.logo{background:url(\"DATA:IMAGE/SVG+XML;charset=utf-8;BASE64,c2Vjb25k\r\n  cGF5bG9hZA==\")}</style>",
            "<img src=data:image/jpeg;base64,dGhpcmRwYXlsb2Fk alt=endmarker><p>结尾正文</p>",
            "<img src='https://example.com/image.png'><img src='data:image/svg+xml,%3Csvg%3E'>"
        );
        for extension in ["html", "HTM"] {
            let chunks = chunks_for_file(source, 512, std::path::Path::new(&format!("page.{extension}")));
            let result = chunks.iter().map(|chunk| chunk.content.as_str()).collect::<String>();
            for marker in ["正文标题", "beforemarker", "图像说明", "aftermarker", "endmarker", "结尾正文"] {
                assert!(result.contains(marker), "missing {marker}");
            }
            for payload in ["aW1hZ2VwYXlsb2Fk", "c2Vjb25k", "cGF5bG9hZA", "dGhpcmRwYXlsb2Fk", "%3Csvg%3E", "https://example.com/image.png"] {
                assert!(!result.contains(payload), "image payload remained: {payload}");
            }
            for chunk in &chunks {
                assert_eq!(chunk.content, source_content(source, &chunk.spans));
                for span in &chunk.spans {
                    assert_eq!(span.start_line, source[..span.start_byte].bytes().filter(|byte| *byte == b'\n').count() + 1);
                    let line_start = source[..span.start_byte].rfind('\n').map_or(0, |offset| offset + 1);
                    assert_eq!(span.start_column, source[line_start..span.start_byte].chars().count() + 1);
                }
            }
        }
        let literal = chunks_for_file(source, 512, std::path::Path::new("example.txt"));
        assert!(literal.iter().any(|chunk| chunk.content.contains("aW1hZ2VwYXlsb2Fk")));
    }

    #[test]
    fn sentence_boundaries_survive_chunking_and_overlap() {
        let first = format!("{}。", "完整条件".repeat(100));
        let long = format!("{}。", "超长句子".repeat(150));
        let last = "只有满足上述条件才能发布。";
        let source = format!("# 发布指南\n## 生产环境\n{first}{long}{last}");
        let chunks = chunks_for_file(&source, 512, std::path::Path::new("document.md"));
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| chunk.heading == "发布指南 / 生产环境"));
        assert!(chunks.iter().any(|chunk| chunk.content.contains(&long)));
        for chunk in &chunks {
            for span in &chunk.spans {
                let prefix = &source[..span.start_byte];
                assert!(prefix.ends_with('\n') || prefix.ends_with('。'));
                assert!(source[..span.end_byte].ends_with('。'));
            }
        }
        assert_eq!(unique_content(&source, &chunks), format!("{first}{long}{last}"));
    }

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
        let chunks = chunks_for_file(&source, 512, std::path::Path::new("document.md"));
        assert!(chunks.len() > 2);
        verify(&source, &chunks, 512);
        for pair in chunks.windows(2) {
            let previous = pair[0].spans.last().unwrap();
            let next = &pair[1].spans[0];
            assert!(next.start_byte < previous.end_byte, "adjacent chunks lost context");
            assert!(source[next.start_byte..previous.end_byte].encode_utf16().count() <= 80);
        }
        let separate = chunks_for_file("# Test\nfirst policy\n# Production\nsecond policy", 512, std::path::Path::new("document.md"));
        assert!(!separate[1].content.contains("first policy"));
    }

    #[test]
    fn tables_use_the_normal_budget_without_repeating_headers() {
        let header = "| Model | Score | Notes |\r\n| --- | ---: | --- |";
        let row = format!("| long-row | 1 | {} |", "x".repeat(1400));
        let body = format!("{header}\r\n{row}");
        let source = format!("# Benchmark\r\n{body}");
        let chunks = chunks_for_file(&source, 512, std::path::Path::new("document.md"));
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, body);
        assert_eq!(chunks.iter().filter(|chunk| chunk.content.starts_with(header)).count(), 1);
        assert_eq!(unique_content(&source, &chunks), body);
        assert!(chunks.iter().all(|chunk| chunk.heading == "Benchmark"));
    }

    #[test]
    fn pipe_characters_do_not_change_section_boundaries() {
        let source = "# Before\n| A | B |\n| --- | --- |\n| one | two |\n# After | Details\nFollowing paragraph.";
        let chunks = chunks_for_file(source, 512, std::path::Path::new("document.md"));
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].heading, "Before");
        assert_eq!(chunks[1].heading, "After | Details");
        assert_eq!(chunks[1].content, "Following paragraph.");
    }

    #[test]
    fn very_long_single_line_keeps_unicode_columns_exact() {
        let source = "一😀a".repeat(350_000);
        let chunks = chunks_for_file(&source, 2200, std::path::Path::new("source.rs"));
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
    fn long_single_sentence_stays_intact() {
        let source = "很长的一行😀".repeat(1600);
        let c = chunks(&source, 2200);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].content, source);
        assert_eq!(
            unique_content(&source, &c),
            source
        );
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
        let c = chunks_for_file(&source, 400, std::path::Path::new("source.rs"));
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
        assert_eq!(c[0].content, "😀中文甲乙");
        assert_eq!((c[0].spans[0].start_byte, c[0].spans[0].end_byte), (0, 16));
        assert_eq!(
            (c[0].spans[0].start_column, c[0].spans[0].end_column),
            (1, 6)
        );
        let source = format!("{}\r\nnext", "a".repeat(399));
        let c = chunks_for_file(&source, 400, std::path::Path::new("source.rs"));
        assert_eq!(c[0].content, format!("{}\r\n", "a".repeat(399)));
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
    fn large_inputs_are_not_retained_in_token_cache() {
        let source = format!("large-cache-regression {}", "中文 document payload ".repeat(1000));
        assert_eq!(tokenize(&source), tokenize_uncached(&source));
        assert!(!TOKEN_CACHE.lock().unwrap().entries.contains_key(&source));
    }

    #[test]
    fn token_cache_enforces_total_memory_budget() {
        let mut cache = TokenCache::default();
        let tokens = vec!["word".to_owned(); 1000];
        for index in 0..1000 {
            cache.insert(&format!("document-{index}"), &tokens);
            assert!(cache.bytes <= TOKEN_CACHE_MAX_BYTES);
        }
        assert!(cache.entries.len() < 1000);
        assert_eq!(cache.bytes, cache.entries.iter().map(|(source, tokens)| TokenCache::entry_bytes(source, tokens)).sum::<usize>());
        let bytes = cache.bytes;
        cache.insert("document-999", &tokens);
        assert_eq!(cache.bytes, bytes);
    }

    #[test]
    fn cache_eviction_preserves_tokenization() {
        for index in 0..8_300 {
            let source = format!("文档标识符 marker_{index} mixedCase");
            let expected = tokenize_uncached(&source);
            assert_eq!(tokenize(&source), expected);
            assert_eq!(tokenize(&source), expected);
        }
        let cache = TOKEN_CACHE.lock().unwrap();
        assert!(cache.entries.len() <= 8192);
        assert!(cache.bytes <= TOKEN_CACHE_MAX_BYTES);
    }

}
