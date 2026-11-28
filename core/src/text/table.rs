use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::{ops::Range, sync::LazyLock};

mod html;

pub(crate) struct Table {
    pub start: usize,
    pub end: usize,
    pub rows: Vec<Range<usize>>,
    pub caption: Option<String>,
    header: String,
    group_labels: Vec<Option<Range<usize>>>,
    html: Option<html::TableBody>,
}

impl Table {
    pub fn find(tables: &[Self], start: usize) -> Option<&Self> {
        let table = tables.get(tables.partition_point(|t| t.start <= start).checked_sub(1)?)?;
        (start < table.rows.last()?.end).then_some(table)
    }

    pub fn exact_rows(&self, start: usize, end: usize) -> Option<Range<usize>> {
        let first = self.rows.binary_search_by_key(&start, |row| row.start).ok()?;
        let last = self.rows.binary_search_by_key(&end, |row| row.end).ok()?;
        (first <= last).then_some(first..last + 1)
    }

    pub fn row_at(&self, start: usize, end: usize) -> Option<usize> {
        let index = self.rows.partition_point(|row| row.end <= start);
        let row = self.rows.get(index)?;
        (row.start <= start && end <= row.end).then_some(index)
    }

    pub fn content(&self, source: &str, rows: Range<usize>) -> String {
        if let Some(html) = &self.html {
            return html.content(rows, true);
        }
        let mut content = self.header.clone();
        for i in rows {
            content.push('\n');
            let row = &source[self.rows[i].clone()];
            if let Some(label) = &self.group_labels[i] {
                let cell = cells(row).next().unwrap();
                let offset = cell.as_ptr() as usize - row.as_ptr() as usize;
                content.push_str(&row[..offset]);
                content.push_str(&source[label.clone()]);
                content.push_str(&row[offset + cell.len()..]);
            } else {
                content.push_str(row);
            }
        }
        content
    }

    pub fn index_content(&self, source: &str, rows: Range<usize>) -> String {
        self.html.as_ref().map_or_else(|| self.content(source, rows.clone()), |html| html.content(rows.clone(), false))
    }

    pub fn batches(&self, source: &str, budget: usize) -> impl Iterator<Item = Range<usize>> + '_ {
        let budget = if self.rows.len() <= 4 { budget.max(800) } else { budget };
        let header = self.html.as_ref().map_or_else(|| self.header.encode_utf16().count(), |html| html.header_width());
        let widths = self.html.as_ref().map_or_else(|| self.rows.iter().zip(&self.group_labels).map(|(row, label)| {
            source[row.clone()].encode_utf16().count()
                + label.as_ref().map_or(0, |label| source[label.clone()].encode_utf16().count()) + 1
        }).collect::<Vec<_>>(), |html| html.widths());
        let mut first = 0;
        std::iter::from_fn(move || {
            if first == widths.len() { return None; }
            let mut end = first + 1;
            let mut size = header + widths[first];
            while end < widths.len() && size + widths[end] <= budget {
                size += widths[end];
                end += 1;
            }
            let rows = first..end;
            first = end;
            Some(rows)
        })
    }
}

fn cells(line: &str) -> impl Iterator<Item = &str> {
    let line = line.trim();
    let line = line.strip_prefix('|').unwrap_or(line);
    let line = line.strip_suffix('|').unwrap_or(line);
    let mut start = 0;
    let mut escape = false;
    line.char_indices().filter_map(move |(index, ch)| {
        let split = ch == '|' && !escape;
        escape = ch == '\\' && !escape;
        if split {
            let cell = &line[start..index];
            start = index + 1;
            Some(cell)
        } else { None }
    }).chain(std::iter::once_with(move || {
        // The final cell is after the last unescaped divider, if any.
        let mut end = 0;
        let mut escape = false;
        for (index, ch) in line.char_indices() {
            if ch == '|' && !escape { end = index + 1; }
            escape = ch == '\\' && !escape;
        }
        &line[end..]
    }))
}

fn normalized_header(line: &str) -> String {
    static MONTH_PAIR: LazyLock<Regex> = LazyLock::new(|| Regex::new(
        r"(?i)^((?:jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)\s*-\s*\d{2,4})\s+((?:jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)\s*-\s*\d{2,4})$"
    ).unwrap());
    let mut labels: Vec<_> = cells(line).map(str::trim).collect();
    let mut changed = false;
    for i in 1..labels.len() {
        if labels[i].is_empty() {
            if let Some(parts) = MONTH_PAIR.captures(labels[i - 1]) {
                labels[i - 1] = parts.get(1).unwrap().as_str();
                labels[i] = parts.get(2).unwrap().as_str();
                changed = true;
            }
        }
    }
    if changed { format!("| {} |", labels.join(" | ")) } else { line.to_owned() }
}

fn from_rows(source: &str, range: Range<usize>, mut rows: Vec<Range<usize>>) -> Option<Table> {
    if rows.is_empty() { return None; }
    let mut prefix = source[range.start..rows[0].start].lines();
    let mut labels = prefix.next()?;
    let delimiter = prefix.next()?;
    let mut start = range.start;
    if cells(labels).all(|cell| cell.trim().is_empty()) {
        // Spreadsheet converters can put a title and explanatory rows before the labels.
        if let Some(i) = rows.iter().take(16).position(|row| {
            let cells: Vec<_> = cells(&source[row.clone()]).map(str::trim).collect();
            let populated = cells.iter().filter(|cell| !cell.is_empty()).count();
            populated >= 2 && populated * 2 >= cells.len() && cells[0].chars().next().is_some_and(char::is_alphabetic)
        }) {
            start = rows[i].start;
            labels = &source[rows[i].clone()];
            rows.drain(..=i);
        }
    }
    if rows.is_empty() { return None; }

    let first_label = cells(labels).next().unwrap().trim().to_ascii_lowercase();
    let grouped = matches!(first_label.as_str(), "measure" | "category" | "procedure" | "appeal procedure");
    let mut group = None;
    let group_labels = rows.iter().map(|range| {
        if !grouped { return None; }
        let raw = &source[range.clone()];
        let cell = cells(raw).next().unwrap().trim();
        if cell.is_empty() { return group.clone(); }
        let start = range.start + (cell.as_ptr() as usize - raw.as_ptr() as usize);
        group = Some(start..start + cell.len());
        None
    }).collect();

    let caption = source[..range.start].rsplit('\n').take(10).find_map(|line| {
        let line = line.trim().trim_matches(['*', '#', ' ']);
        let (label, _) = line.split_once(':')?;
        let (name, number) = label.split_once(' ')?;
        (name.eq_ignore_ascii_case("table") && !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
            .then(|| line.chars().take(240).collect())
    });
    Some(Table { start, end: rows.last().unwrap().end, rows, caption, header: format!("{}\n{delimiter}", normalized_header(labels)), group_labels, html: None })
}

pub(crate) fn supports(path: &std::path::Path) -> bool {
    path.extension().is_some_and(|ext| ["md", "markdown", "html", "htm"].iter().any(|name| ext.eq_ignore_ascii_case(name)))
}

pub(crate) fn parse(source: &str, path: &std::path::Path) -> Vec<Table> {
    if !supports(path) || !source.contains(['|', '<']) { return vec![]; }
    parse_in_ranges(source, path, &super::indexable_ranges(source, path))
}

pub(super) fn parse_in_ranges(source: &str, path: &std::path::Path, allowed: &[(usize, usize)]) -> Vec<Table> {
    if !supports(path) { return vec![]; }
    if !super::is_markdown(path) {
        return html::parse(source, None, allowed);
    }
    if !source.contains(['|', '<']) { return vec![]; }
    let mut depth = 0;
    let mut active = None;
    let mut tables = vec![];
    let mut html_offsets = vec![];
    let options = Options::ENABLE_TABLES | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS;
    for (event, span) in Parser::new_ext(source, options).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if depth == 0 && matches!(tag, Tag::Table(_)) {
                    active = Some((span.clone(), Vec::new()));
                }
                if let (Tag::TableRow, Some((_, rows))) = (tag, &mut active) {
                    let end = span.start + source[span.clone()].trim_end_matches(['\r', '\n']).len();
                    rows.push(span.start..end);
                }
                depth += 1;
            }
            Event::End(tag) => {
                depth -= 1;
                if tag == TagEnd::Table {
                    if let Some((range, rows)) = active.take() {
                        tables.extend(from_rows(source, range, rows));
                    }
                }
            }
            Event::Html(_) | Event::InlineHtml(_) => html_offsets.push(span),
            _ => {}
        }
    }
    let mut html_tables = html::parse(source, Some(&html_offsets), allowed);
    tables.retain(|table| {
        let i = html_tables.partition_point(|html| html.end <= table.start);
        html_tables.get(i).is_none_or(|html| html.start >= table.end)
    });
    tables.append(&mut html_tables);
    tables.sort_unstable_by_key(|table| table.start);
    tables
}
