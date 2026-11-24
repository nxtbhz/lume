pub mod layout;
#[cfg(test)]
mod tests;

use layout::{layout_markdown, RenderedMarkdown};
use lume_core::buffer::LINE_BREAKS;
use ropey::Rope;
use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    sync::Arc,
};

#[derive(Clone, Copy)]
pub enum ColumnAlign {
    Left,
    Center,
    Right,
}
#[derive(Clone, Copy, Debug)]
pub enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}
impl AlertKind {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "[!NOTE]" => Self::Note,
            "[!TIP]" => Self::Tip,
            "[!IMPORTANT]" => Self::Important,
            "[!WARNING]" => Self::Warning,
            "[!CAUTION]" => Self::Caution,
            _ => return None,
        })
    }
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct InlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: bool,
}
#[derive(Clone)]
pub struct InlineSpan {
    pub text: String,
    pub style: InlineStyle,
}
type Cell = Vec<InlineSpan>;
pub enum ListMarker {
    Bullet,
    Ordered(u64),
}
pub struct ListItem {
    pub source_line: usize,
    pub indent_level: usize,
    pub marker: ListMarker,
    pub task: Option<bool>,
    pub spans: Cell,
}
pub enum MarkdownBlock {
    Heading(usize, usize, Cell),
    Paragraph(usize, Cell),
    CodeBlock {
        line: usize,
        language: String,
        lines: Vec<String>,
    },
    BlockQuote(usize, Option<AlertKind>, Cell),
    List(ListItem),
    Table {
        line: usize,
        headers: Vec<Cell>,
        align: Vec<ColumnAlign>,
        rows: Vec<Vec<Cell>>,
    },
    Rule(usize),
}
pub struct MarkdownDocument {
    pub blocks: Vec<MarkdownBlock>,
}

fn push(spans: &mut Cell, text: &str, style: InlineStyle) {
    if text.is_empty() {
        return;
    }
    if let Some(s) = spans.last_mut().filter(|s| s.style == style) {
        s.text.push_str(text);
    } else {
        spans.push(InlineSpan {
            text: text.to_owned(),
            style,
        });
    }
}

fn inline(mut s: &str) -> Cell {
    let mut out = Vec::new();
    let mut style = InlineStyle::default();
    while !s.is_empty() {
        if let Some(rest) = s.strip_prefix('\\') {
            if let Some(c) = rest.chars().next().filter(char::is_ascii_punctuation) {
                push(&mut out, &rest[..c.len_utf8()], style);
                s = &rest[c.len_utf8()..];
                continue;
            }
        }
        if s.starts_with('`') {
            let ticks = s.bytes().take_while(|&b| b == b'`').count();
            let marker = &s[..ticks];
            if let Some((code, rest)) = s[ticks..].split_once(marker) {
                push(
                    &mut out,
                    code,
                    InlineStyle {
                        code: true,
                        ..style
                    },
                );
                s = rest;
                continue;
            }
        }
        let image = s.starts_with("![");
        if image || s.starts_with('[') {
            let label = &s[if image { 2 } else { 1 }..];
            if let Some((label, tail)) = label.split_once("](") {
                if let Some((_, rest)) = tail.split_once(')') {
                    if image {
                        push(&mut out, &format!("[img: {label}]"), style);
                    } else {
                        push(
                            &mut out,
                            label,
                            InlineStyle {
                                link: true,
                                ..style
                            },
                        );
                    }
                    s = rest;
                    continue;
                }
            }
        }
        let mut consumed = false;
        for (marker, kind) in [("**", 0), ("~~", 1), ("*", 2)] {
            if let Some(rest) = s.strip_prefix(marker) {
                let flag = match kind {
                    0 => &mut style.bold,
                    1 => &mut style.strike,
                    _ => &mut style.italic,
                };
                if *flag || (!rest.starts_with(char::is_whitespace) && rest.contains(marker)) {
                    *flag = !*flag;
                    s = rest;
                    consumed = true;
                    break;
                }
            }
        }
        if consumed {
            continue;
        }
        let len = s.chars().next().unwrap().len_utf8();
        let end = len
            + s[len..]
                .find(['\\', '`', '!', '[', '*', '~'])
                .unwrap_or(s.len() - len);
        push(&mut out, &s[..end], style);
        s = &s[end..];
    }
    out
}

/// Split only at unescaped pipes outside a matching backtick run.
fn split_table_row(line: &str) -> Vec<&str> {
    let s = line.trim();
    let mut cells = Vec::new();
    let mut start = 0;
    let mut ticks = 0;
    let mut i = 0;
    while i < s.len() {
        match s.as_bytes()[i] {
            b'\\' => {
                i += 1;
                if i < s.len() {
                    i += s[i..].chars().next().unwrap().len_utf8();
                }
            }
            b'`' => {
                let n = s.as_bytes()[i..].iter().take_while(|&&b| b == b'`').count();
                if ticks == 0 {
                    ticks = n;
                } else if n == ticks {
                    ticks = 0;
                }
                i += n;
            }
            b'|' if ticks == 0 => {
                if i > 0 {
                    cells.push(s[start..i].trim());
                }
                start = i + 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    if start < s.len() {
        cells.push(s[start..].trim());
    }
    cells
}
fn delimiter(s: &str) -> Option<Vec<ColumnAlign>> {
    let cells = split_table_row(s);
    if cells.is_empty() {
        return None;
    }
    cells
        .into_iter()
        .map(|c| {
            let middle = c.strip_prefix(':').unwrap_or(c);
            let middle = middle.strip_suffix(':').unwrap_or(middle);
            if middle.is_empty() || !middle.bytes().all(|b| b == b'-') {
                return None;
            }
            Some(match (c.starts_with(':'), c.ends_with(':')) {
                (true, true) => ColumnAlign::Center,
                (_, true) => ColumnAlign::Right,
                _ => ColumnAlign::Left,
            })
        })
        .collect()
}
fn list_item(line: usize, s: &str) -> Option<ListItem> {
    let trim = s.trim_start();
    let indent = s.len() - trim.len();
    let (marker, mut text) =
        if let Some(t) = ["- ", "+ ", "* "].iter().find_map(|p| trim.strip_prefix(p)) {
            (ListMarker::Bullet, t)
        } else {
            let (num, t) = trim.split_once(". ")?;
            (ListMarker::Ordered(num.parse().ok()?), t)
        };
    let task = if let Some(t) = text.strip_prefix("[ ] ") {
        text = t;
        Some(false)
    } else if let Some(t) = text
        .strip_prefix("[x] ")
        .or_else(|| text.strip_prefix("[X] "))
    {
        text = t;
        Some(true)
    } else {
        None
    };
    Some(ListItem {
        source_line: line,
        indent_level: indent,
        marker,
        task,
        spans: inline(text),
    })
}

fn parse_lines<'a>(lines: impl Iterator<Item = Cow<'a, str>>) -> MarkdownDocument {
    let mut input = lines.enumerate().peekable();
    let mut blocks = Vec::new();
    while let Some((line, raw)) = input.next() {
        let s = raw.trim_end_matches(LINE_BREAKS);
        let trim = s.trim();
        let block = if trim.starts_with("```") || trim.starts_with("~~~") {
            let ch = trim.as_bytes()[0];
            let n = trim.bytes().take_while(|&b| b == ch).count();
            let language = trim[n..].split_whitespace().next().unwrap_or("").to_owned();
            let mut lines = Vec::new();
            for (_, raw) in input.by_ref() {
                let t = raw.trim();
                if t.len() >= n && t.bytes().all(|b| b == ch) {
                    break;
                }
                lines.push(raw.trim_end_matches(LINE_BREAKS).to_owned());
            }
            MarkdownBlock::CodeBlock {
                line,
                language,
                lines,
            }
        } else if let Some(align) = input
            .peek()
            .filter(|_| s.contains('|'))
            .and_then(|(_, v)| delimiter(v))
            .filter(|a| a.len() == split_table_row(s).len())
        {
            input.next();
            let headers = split_table_row(s).into_iter().map(inline).collect();
            let mut rows = Vec::new();
            while input.peek().is_some_and(|(_, v)| v.contains('|')) {
                let (_, raw) = input.next().unwrap();
                let mut cells: Vec<_> = split_table_row(&raw)
                    .into_iter()
                    .take(align.len())
                    .map(inline)
                    .collect();
                cells.resize_with(align.len(), Vec::new);
                rows.push(cells);
            }
            MarkdownBlock::Table {
                line,
                headers,
                align,
                rows,
            }
        } else if trim.len() >= 3 && b"-*_".iter().any(|&ch| trim.bytes().all(|b| b == ch)) {
            MarkdownBlock::Rule(line)
        } else if let Some(t) = trim.strip_prefix('>') {
            let t = t.trim_start();
            let alert = AlertKind::parse(t);
            MarkdownBlock::BlockQuote(
                line,
                alert,
                if alert.is_some() {
                    Vec::new()
                } else {
                    inline(t)
                },
            )
        } else if let Some(item) = list_item(line, s) {
            MarkdownBlock::List(item)
        } else {
            let level = trim.bytes().take_while(|&b| b == b'#').count();
            if (1..=6).contains(&level) && (trim.len() == level || trim[level..].starts_with(' ')) {
                MarkdownBlock::Heading(line, level, inline(trim[level..].trim()))
            } else {
                MarkdownBlock::Paragraph(line, inline(s))
            }
        };
        blocks.push(block);
    }
    MarkdownDocument { blocks }
}
#[cfg(test)]
pub fn parse_markdown(s: &str) -> MarkdownDocument {
    parse_markdown_rope(&Rope::from_str(s))
}
pub fn parse_markdown_rope(rope: &Rope) -> MarkdownDocument {
    parse_lines(rope.lines().map(Cow::from))
}

struct CachedTab {
    revision: u64,
    doc: Arc<MarkdownDocument>,
    layouts: VecDeque<(usize, Arc<RenderedMarkdown>)>,
}
#[derive(Default)]
pub struct MarkdownPreviewCache {
    tabs: HashMap<usize, CachedTab>,
}
impl MarkdownPreviewCache {
    pub fn get_or_compute(
        &mut self,
        id: usize,
        revision: u64,
        rope: &Rope,
        width: usize,
    ) -> Arc<RenderedMarkdown> {
        let create = || CachedTab {
            revision,
            doc: Arc::new(parse_markdown_rope(rope)),
            layouts: VecDeque::new(),
        };
        let tab = self.tabs.entry(id).or_insert_with(create);
        if tab.revision != revision {
            *tab = create();
        }
        if let Some((_, layout)) = tab.layouts.iter().find(|(w, _)| *w == width) {
            return Arc::clone(layout);
        }
        let layout = Arc::new(layout_markdown(&tab.doc, width, rope.len_lines()));
        if tab.layouts.len() == 2 {
            tab.layouts.pop_front();
        }
        tab.layouts.push_back((width, Arc::clone(&layout)));
        layout
    }
    pub fn invalidate_tab(&mut self, id: usize) {
        self.tabs.remove(&id);
    }
}
