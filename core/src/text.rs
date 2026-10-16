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
    let mut headings = vec![];
    let mut start = None;
    let mut fence: Option<String> = None;
    let mut front = markdown
        && lines
            .spans
            .first()
            .is_some_and(|(s, e)| content[*s..*e].trim() == "---");
    let mut blocks: Vec<(String, usize, usize, bool)> = vec![];
    let flush = |last: usize,
                 start: &mut Option<usize>,
                 headings: &Vec<String>,
                 code: bool,
                 blocks: &mut Vec<(String, usize, usize, bool)>| {
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
                ));
            }
        }
    };
    if !markdown {
        if !content.trim().is_empty() {
            blocks.push((String::new(), 0, content.len(), true));
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
    let mut result: Vec<Chunk> = vec![];
    let mut length = 0;
    for (heading, start, end, code) in blocks {
        for (s, e) in split_block(content, start, end, max_chars, code) {
            let part = &content[s..e];
            let n = part.encode_utf16().count();
            let span = lines.span(content, s, e);
            let last_line = if e > 0 && content.as_bytes()[e - 1] == b'\n' {
                span.end_line.saturating_sub(1).max(span.start_line)
            } else {
                span.end_line
            };
            if result.last().is_none_or(|c| c.heading != heading) || length + 2 + n > max_chars {
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
    #[test]
    fn very_long_single_line_keeps_unicode_columns_exact() {
        let source = "一😀a".repeat(350_000);
        let chunks = chunks_for_file(&source, 2200, false);
        let mut column = 1;
        for chunk in &chunks {
            let span = &chunk.spans[0];
            assert_eq!((span.start_line, span.end_line), (1, 1));
            assert_eq!(span.start_column, column);
            column += chunk.content.chars().count();
            assert_eq!(span.end_column, column);
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
            c.iter().map(|c| c.content.as_str()).collect::<String>(),
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
            c.iter().map(|c| c.content.as_str()).collect::<String>(),
            &source[10..]
        );
    }
    #[test]
    fn code_files_do_not_parse_hashes_or_frontmatter() {
        let source = "---\r\n# not a title\r\n".repeat(100);
        let c = chunks_for_file(&source, 400, false);
        verify(&source, &c, 400);
        assert_eq!(
            c.iter().map(|c| c.content.as_str()).collect::<String>(),
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
        assert_eq!(c[0].content, "a".repeat(399));
        assert!(c[1].content.starts_with("\r\n"));
        assert_eq!(
            c.iter().map(|p| p.content.as_str()).collect::<String>(),
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
