use crate::{protocol::*, sag_mcp::SagTools, store, text};
use anyhow::{bail, Context, Result};
use regex::Regex;
use std::{ops::Range, path::PathBuf};

struct Pattern {
    first: Regex,
    following: Option<Regex>,
}
struct Matcher {
    patterns: Vec<Pattern>,
    literals: Option<regex::RegexSet>,
}
impl Matcher {
    fn new(request: &MatchRequest) -> Result<Self> {
        if request.patterns.iter().any(|p| p.chars().count() > 2000)
            || request.patterns.iter().map(String::len).sum::<usize>() > 16 * 1024
        {
            bail!("单个匹配条件最多 2000 字符，条件总大小最多 16 KiB")
        }
        let mut patterns = vec![];
        let mut seen = std::collections::HashSet::new();
        for pattern in &request.patterns {
            if !seen.insert(pattern) {
                continue;
            }
            if pattern.is_empty() && request.options.word_regexp && !request.options.line_regexp {
                continue;
            }
            let source = match request.mode {
                MatchMode::Regex => pattern.clone(),
                MatchMode::Fixed => regex::escape(pattern),
            };
            // Normalize flags and comments before adding boundaries around the user's expression.
            let source = regex_syntax::ParserBuilder::new()
                .case_insensitive(request.options.ignore_case)
                .build()
                .parse(&source)
                .context("正则表达式无效")?
                .to_string();
            let compile = |s: &str| Regex::new(s).context("正则表达式无效");
            let (first, following) = if request.options.line_regexp {
                (compile(&format!("^({source})$"))?, None)
            } else if request.options.word_regexp {
                // Capture only the pattern; boundaries must also work for punctuation patterns such as @.
                (
                    compile(&format!(
                        r"(?:^|[^\p{{L}}\p{{N}}_])({source})(?:$|[^\p{{L}}\p{{N}}_])"
                    ))?,
                    Some(compile(&format!(
                        r"[^\p{{L}}\p{{N}}_]({source})(?:$|[^\p{{L}}\p{{N}}_])"
                    ))?),
                )
            } else {
                (compile(&format!("({source})"))?, None)
            };
            patterns.push(Pattern { first, following });
        }
        let literals = if matches!(request.mode, MatchMode::Fixed) {
            Some(regex::RegexSetBuilder::new(request.patterns.iter().map(|p| regex::escape(p)))
                .case_insensitive(request.options.ignore_case).build()?)
        } else { None };
        Ok(Self { patterns, literals })
    }
    fn find(&self, line: &str, from: usize) -> Option<Range<usize>> {
        self.patterns
            .iter()
            .filter_map(|p| {
                let (regex, start) = if from > 0 {
                    if let Some(following) = &p.following {
                        (
                            following,
                            line[..from].char_indices().next_back().unwrap().0,
                        )
                    } else {
                        (&p.first, from)
                    }
                } else {
                    (&p.first, 0)
                };
                regex.captures_at(line, start)?.get(1).map(|m| m.range())
            })
            .min_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)))
    }
}

fn reserve(value: &impl serde::Serialize, used: &mut usize) -> Result<bool> {
    let size = serde_json::to_vec(value)?.len() + 1;
    if *used + size > MAX_RESPONSE as usize - 1024 {
        return Ok(false);
    }
    *used += size;
    Ok(true)
}

pub(crate) fn run(tools: SagTools, path: PathBuf, request: MatchRequest) -> Result<MatchOutput> {
    let matcher = Matcher::new(&request)?;
    let filters = request
        .options
        .filters
        .iter()
        .map(|filter| {
            Ok((
                filter.include,
                glob::Pattern::new(&filter.pattern).context("文件过滤通配符无效")?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let mode = request.options.output;
    if mode == MatchOutputMode::OnlyMatching && request.options.invert_match {
        bail!("-o 不能与 -v 同时使用")
    }
    if mode != MatchOutputMode::Lines
        && (request.options.before_context > 0 || request.options.after_context > 0)
    {
        bail!("上下文参数只能用于匹配行输出")
    }
    let paths = tools.scope.iter().map(PathBuf::from).collect();
    let mut output = MatchOutput {
        protocol_version: VERSION,
        scope: store::path_text(&path)?,
        results: vec![],
        files: vec![],
        matched: false,
        truncated: false,
    };
    let mut used = serde_json::to_vec(&output)?.len();
    let mut selected_count = 0;
    tools.visit(paths, true, |file, _, metadata| {
        if !metadata.is_file() || !text::is_indexable(file, &tools.settings.file_types) {
            return Ok(true);
        }
        let mut included = filters.first().is_none_or(|(include, _)| !include);
        for (include, pattern) in &filters {
            if pattern.matches_path(std::path::Path::new(file.file_name().unwrap())) {
                included = *include;
            }
        }
        if !included {
            return Ok(true);
        }
        let Some(document) = tools.store.matching_document(file, |body| {
            mode == MatchOutputMode::Count || request.options.invert_match || matcher.literals.as_ref().is_none_or(|set| set.is_match(body))
        })? else { return Ok(true); };
        let relative = format!("./{}", file.strip_prefix(&path)?.display());
        let spans: Vec<_> = text::Lines::new(&document.body)
            .spans
            .into_iter()
            .take_while(|(start, _)| *start < document.body.len())
            .collect();
        let mut selected = vec![];
        let mut count = 0;
        for (index, &(start, end)) in spans.iter().enumerate() {
            if tools.cancel.is_cancelled() {
                bail!("已停止")
            }
            let line = &document.body[start..end];
            if matcher.find(line, 0).is_some() == request.options.invert_match {
                continue;
            }
            output.matched = true;
            count += 1;
            selected_count += 1;
            if mode == MatchOutputMode::Quiet {
                return Ok(false);
            }
            if matches!(mode, MatchOutputMode::Lines | MatchOutputMode::OnlyMatching) {
                selected.push(index);
            }
            if mode == MatchOutputMode::Files || request.max_results == Some(selected_count) {
                break;
            }
        }
        if mode == MatchOutputMode::Count || mode == MatchOutputMode::Files && count > 0 {
            let found = MatchFile {
                path: relative.clone(),
                absolute_path: document.path.clone(),
                count: (mode == MatchOutputMode::Count).then_some(count),
            };
            if !reserve(&found, &mut used)? {
                output.truncated = true;
                return Ok(false);
            }
            output.files.push(found);
        }
        if mode == MatchOutputMode::OnlyMatching {
            for index in selected {
                let line = &document.body[spans[index].0..spans[index].1];
                let mut from = 0;
                while let Some(found) = matcher.find(line, from) {
                    if tools.cancel.is_cancelled() {
                        bail!("已停止")
                    }
                    if !found.is_empty() {
                        let result = SearchResult {
                            heading: String::new(),
                            path: relative.clone(),
                            absolute_path: document.path.clone(),
                            start_line: index + 1,
                            end_line: index + 1,
                            content: Some(line[found.clone()].to_owned()),
                            matched_lines: vec![],
                        };
                        if !reserve(&result, &mut used)? {
                            output.truncated = true;
                            return Ok(false);
                        }
                        output.results.push(result);
                    }
                    if found.end > from {
                        from = found.end;
                    } else if found.end < line.len() {
                        from = found.end + line[found.end..].chars().next().unwrap().len_utf8();
                    } else {
                        break;
                    }
                }
            }
        } else if mode == MatchOutputMode::Lines {
            let mut groups: Vec<(usize, usize, Vec<usize>)> = vec![];
            for index in selected {
                let start = index.saturating_sub(request.options.before_context);
                let end = (index + request.options.after_context).min(spans.len() - 1);
                if let Some(last) = groups.last_mut().filter(|last| {
                    start <= last.1 + 1
                        && (request.options.before_context > 0 || request.options.after_context > 0)
                }) {
                    last.1 = end;
                    last.2.push(index + 1);
                } else {
                    groups.push((start, end, vec![index + 1]));
                }
            }
            for (start, end, matched_lines) in groups {
                let result = SearchResult {
                            heading: String::new(),
                    path: relative.clone(),
                    absolute_path: document.path.clone(),
                    start_line: start + 1,
                    end_line: end + 1,
                    content: request
                        .content
                        .then(|| document.body[spans[start].0..spans[end].1].to_owned()),
                    matched_lines: if request.options.before_context > 0
                        || request.options.after_context > 0
                    {
                        matched_lines
                    } else {
                        vec![]
                    },
                };
                if !reserve(&result, &mut used)? {
                    output.truncated = true;
                    return Ok(false);
                }
                output.results.push(result);
            }
        }
        Ok(request.max_results != Some(selected_count))
    })?;
    Ok(output)
}
