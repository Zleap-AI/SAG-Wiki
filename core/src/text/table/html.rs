use super::Table;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::{fmt::Write, ops::Range, sync::LazyLock};

pub(super) struct TableBody {
    cells: Vec<Cell>,
    rows: Vec<Vec<usize>>,
    header_rows: usize,
}

struct Cell {
    column: usize,
    columns: usize,
    rows: Range<usize>,
    header: bool,
    markup: String,
    text: String,
    width: usize,
}

impl TableBody {
    pub(super) fn content(&self, rows: Range<usize>, markup: bool) -> String {
        let mut result = String::new();
        if markup {
            result.push_str("<table>");
        }
        for (tag, section) in [
            ("thead", 0..self.header_rows),
            (
                "tbody",
                self.header_rows + rows.start..self.header_rows + rows.end,
            ),
        ] {
            if section.is_empty() {
                continue;
            }
            if markup {
                write!(result, "<{tag}>").unwrap();
            }
            for row in section.clone() {
                if markup {
                    result.push_str("<tr>");
                }
                for &index in &self.rows[row] {
                    let cell = &self.cells[index];
                    if markup {
                        if row != cell.rows.start.max(section.start) {
                            continue;
                        }
                        let element = if cell.header { "th" } else { "td" };
                        write!(result, "<{element}").unwrap();
                        if cell.columns > 1 {
                            write!(result, " colspan=\"{}\"", cell.columns).unwrap();
                        }
                        let height = cell.rows.end.min(section.end) - row;
                        if height > 1 {
                            write!(result, " rowspan=\"{height}\"").unwrap();
                        }
                        write!(result, ">{}</{element}>", cell.markup).unwrap();
                    } else {
                        result.push_str(&cell.text);
                        result.push_str(" | ");
                    }
                }
                if markup {
                    result.push_str("</tr>");
                }
                result.push('\n');
            }
            if markup {
                write!(result, "</{tag}>").unwrap();
            }
        }
        if markup {
            result.push_str("</table>");
        }
        result
    }

    pub(super) fn header_width(&self) -> usize {
        self.rows[..self.header_rows]
            .iter()
            .map(|row| self.width(row))
            .sum()
    }

    pub(super) fn widths(&self) -> Vec<usize> {
        self.rows[self.header_rows..]
            .iter()
            .map(|row| self.width(row))
            .collect()
    }

    fn width(&self, row: &[usize]) -> usize {
        row.iter().map(|&index| self.cells[index].width + 3).sum()
    }
}

fn element_text(element: ElementRef<'_>) -> String {
    element
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn from_rows(source: &str, range: Range<usize>, mut offsets: Vec<Range<usize>>) -> Option<Table> {
    static TABLE: LazyLock<Selector> = LazyLock::new(|| Selector::parse("table").unwrap());
    static ROW: LazyLock<Selector> = LazyLock::new(|| Selector::parse("tr").unwrap());
    let fragment = Html::parse_fragment(&source[range.clone()]);
    let element = fragment.select(&TABLE).next()?;
    let rows: Vec<_> = element
        .select(&ROW)
        .filter(|row| {
            row.ancestors()
                .filter_map(ElementRef::wrap)
                .find(|parent| parent.value().name() == "table")
                == Some(element)
        })
        .collect();
    if rows.len() != offsets.len() {
        return None;
    }
    let header_rows = rows
        .iter()
        .take_while(|row| {
            let parent = row.parent().and_then(ElementRef::wrap).unwrap();
            let mut cells = row.child_elements();
            parent.value().name() == "thead"
                || (cells.next().is_some_and(|cell| cell.value().name() == "th")
                    && cells.all(|cell| cell.value().name() != "td"))
        })
        .count();
    if header_rows == rows.len() {
        return None;
    }
    let caption = element
        .child_elements()
        .find(|child| child.value().name() == "caption")
        .map(|caption| element_text(caption).chars().take(240).collect::<String>())
        .filter(|caption| !caption.is_empty());
    let mut body = TableBody {
        cells: vec![],
        rows: Vec::with_capacity(rows.len()),
        header_rows,
    };
    let mut active: Vec<usize> = vec![];
    let mut group_end = 0;
    let mut placements = 0;
    for (index, row) in rows.iter().enumerate() {
        if index == group_end {
            let parent = row.parent();
            group_end = index
                + rows[index..]
                    .iter()
                    .take_while(|row| row.parent() == parent)
                    .count();
        }
        active.retain(|&cell| body.cells[cell].rows.end > index);
        let mut column = 0;
        let mut row_cells = active.clone();
        let mut occupied = active.iter().peekable();
        for cell in row
            .child_elements()
            .filter(|cell| matches!(cell.value().name(), "th" | "td"))
        {
            let columns = cell
                .value()
                .attr("colspan")
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, 1000);
            while let Some(&&blocking) = occupied.peek() {
                let other = &body.cells[blocking];
                if column + columns <= other.column {
                    break;
                }
                column = column.max(other.column + other.columns);
                occupied.next();
            }
            let height = cell
                .value()
                .attr("rowspan")
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(1);
            let end = if height == 0 {
                group_end
            } else {
                (index + height.min(65_534)).min(group_end)
            };
            let text = element_text(cell);
            row_cells.push(body.cells.len());
            body.cells.push(Cell {
                column,
                columns,
                rows: index..end,
                header: cell.value().name() == "th",
                markup: cell.inner_html(),
                width: text.encode_utf16().count(),
                text,
            });
            column += columns;
        }
        row_cells.sort_unstable_by_key(|&cell| body.cells[cell].column);
        placements += row_cells.len();
        if placements > super::super::MAX_INDEX_STRUCTURE_ITEMS {
            return None;
        }
        active = row_cells
            .iter()
            .copied()
            .filter(|&cell| body.cells[cell].rows.end > index + 1)
            .collect();
        body.rows.push(row_cells);
    }
    offsets.drain(..header_rows);
    Some(Table {
        start: range.start,
        end: range.end,
        rows: offsets,
        caption,
        header: String::new(),
        group_labels: vec![],
        html: Some(body),
    })
}

pub(super) fn parse(
    source: &str,
    markdown_html: Option<&[Range<usize>]>,
    allowed: &[(usize, usize)],
) -> Vec<Table> {
    // Capture source offsets before parsing the DOM, which normalizes tags and inserts tbody.
    static TAG: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?is)<!--.*?(?:-->|\z)|<(/?)([a-z][a-z0-9:.-]*)(?:\s(?:[^'">]|"[^"]*"|'[^']*')*)?/?>"#).unwrap()
    });
    static RAW_END: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)</(script|style|textarea|title|xmp|iframe)\s*>").unwrap()
    });
    if markdown_html.is_some_and(|spans| spans.is_empty())
        || !source
            .as_bytes()
            .windows(6)
            .any(|tag| tag.eq_ignore_ascii_case(b"<table"))
    {
        return vec![];
    }
    let mut tables = vec![];
    let mut active: Option<(usize, Vec<Range<usize>>)> = None;
    let mut depth = 0;
    let mut row = None;
    let mut cursor = 0;
    while let Some(tag) = TAG.captures_at(source, cursor) {
        let token = tag.get(0).unwrap();
        cursor = token.end();
        let Some(name) = tag.get(2) else { continue };
        let name = name.as_str().to_ascii_lowercase();
        let closing = !tag[1].is_empty();
        if active.is_none()
            && markdown_html.is_some_and(|spans| {
                let index = spans.partition_point(|span| span.end <= token.start());
                spans
                    .get(index)
                    .is_none_or(|span| span.start > token.start())
            })
        {
            continue;
        }
        if !closing
            && matches!(
                name.as_str(),
                "script" | "style" | "textarea" | "title" | "xmp" | "iframe"
            )
        {
            let Some(end) = RAW_END
                .captures_iter(&source[cursor..])
                .find(|end| end[1].eq_ignore_ascii_case(&name))
            else {
                break;
            };
            cursor += end.get(0).unwrap().end();
            continue;
        }
        let closes_row = depth == 1
            && (name == "tr"
                || (closing && matches!(name.as_str(), "table" | "thead" | "tbody" | "tfoot")));
        if closes_row {
            if let Some(start) = row.take() {
                let end = if closing && name == "tr" {
                    token.end()
                } else {
                    token.start()
                };
                active.as_mut().unwrap().1.push(start..end);
            }
        }
        match (name.as_str(), closing) {
            ("table", false) => {
                if depth == 0 {
                    active = Some((token.start(), vec![]));
                }
                depth += 1;
            }
            ("table", true) if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    let (start, rows) = active.take().unwrap();
                    let index = allowed.partition_point(|&(_, end)| end <= start);
                    if allowed
                        .get(index)
                        .is_some_and(|&(from, to)| from <= start && token.end() <= to)
                    {
                        tables.extend(from_rows(source, start..token.end(), rows));
                    }
                }
            }
            ("tr", false) if depth == 1 => row = Some(token.start()),
            _ => {}
        }
    }
    tables
}
