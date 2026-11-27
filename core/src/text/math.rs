use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};
use regex::Regex;
use std::{collections::HashMap, ops::Range, sync::LazyLock};

pub(super) fn blocks(source: &str) -> Vec<Range<usize>> {
    if !source.contains("$$") && !source.contains("\\[") && !source.contains("\\begin{") {
        return vec![];
    }
    // A formula may close on its opening line, as in `$$ y = f(x) $$ (1)`. Otherwise
    // it runs to the first later line that ends with the closing delimiter.
    static DOLLARS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?m)^ {0,3}\$\$(?:((?:[^\n$]|\$[^$\n])*?)\$\$|((?:[^\n$]|\$[^$\n])*\n[\s\S]*?)\$\$[ \t]*\r?$)").unwrap()
    });
    static BRACKETS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?m)^ {0,3}\\\[(?:((?:[^\n\\]|\\[^\]\n])*?)\\\]|((?:[^\n\\]|\\[^\]\n])*\n[\s\S]*?)\\\][ \t]*\r?$)").unwrap()
    });
    static BRACKET_NUMBER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[0-9\s,.–—-]+$").unwrap());
    static ENVIRONMENT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
        r"(?m)^ {0,3}\\begin\{((?:equation|align|alignat|gather|multline)\*?)\}|\\end\{((?:equation|align|alignat|gather|multline)\*?)\}([ \t]*\r?$)?"
    ).unwrap()
    });
    let options = Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS;
    let mut indented_code = vec![];
    let protected = Parser::new_ext(source, options)
        .into_offset_iter()
        .filter_map(|(event, span)| match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) => {
                indented_code.push(span);
                None
            }
            Event::Start(
                Tag::CodeBlock(CodeBlockKind::Fenced(_)) | Tag::HtmlBlock | Tag::MetadataBlock(_),
            ) => Some(span),
            _ => None,
        })
        .chain(std::iter::once(source.len()..source.len()));
    let mut cursor = 0;
    let mut matches = vec![];
    for span in protected {
        if cursor < span.start {
            let part = &source[cursor..span.start];
            let mut push = |range: Range<usize>| {
                let end = cursor + range.start + part[range.clone()].trim_end_matches('\r').len();
                matches.push(cursor + range.start..end);
            };
            let body = |formula: &regex::Captures| {
                formula.get(1).or_else(|| formula.get(2)).map_or("", |body| body.as_str()).to_owned()
            };
            for formula in DOLLARS.captures_iter(part) {
                if !body(&formula).trim().is_empty() {
                    push(formula.get(0).unwrap().range());
                }
            }
            for formula in BRACKETS.captures_iter(part) {
                let inner = body(&formula);
                if !inner.trim().is_empty() && !BRACKET_NUMBER.is_match(&inner) {
                    push(formula.get(0).unwrap().range());
                }
            }
            let mut opened = HashMap::new();
            for delimiter in ENVIRONMENT.captures_iter(part) {
                let range = delimiter.get(0).unwrap().range();
                if let Some(name) = delimiter.get(1) {
                    opened.entry(name.as_str()).or_insert(range.start);
                } else if let Some(&start) = opened.get(&delimiter[2]) {
                    // Close at a line end, or anywhere on the opening line itself.
                    if delimiter.get(3).is_some() || !part[start..range.start].contains('\n') {
                        opened.remove(&delimiter[2]);
                        push(start..range.end);
                    }
                }
            }
        }
        cursor = cursor.max(span.end);
    }
    matches.sort_unstable_by_key(|span| (span.start, std::cmp::Reverse(span.end)));
    let mut end = 0;
    matches.retain(|span| {
        if span.start < end {
            return false;
        }
        let i = indented_code.partition_point(|code| code.end <= span.start);
        if indented_code
            .get(i)
            .is_some_and(|code| code.start <= span.start)
        {
            return false;
        }
        end = span.end;
        true
    });
    matches
}
