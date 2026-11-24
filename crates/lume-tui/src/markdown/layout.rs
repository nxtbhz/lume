use super::*;
use crossterm::style::Color;
use lume_core::buffer::visual;
use lume_syntax::{Language, SyntaxHighlighter, TokenType};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StyledCellRun {
    pub fg: Color,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub underlined: bool,
    pub crossed_out: bool,
}
impl Default for StyledCellRun {
    fn default() -> Self {
        Self {
            fg: Color::Reset,
            bg: None,
            bold: false,
            italic: false,
            underlined: false,
            crossed_out: false,
        }
    }
}
#[derive(Clone)]
pub struct PreviewSpan {
    pub text: String,
    pub style: StyledCellRun,
}
pub struct PreviewRow {
    pub source_line: usize,
    pub spans: Vec<PreviewSpan>,
    pub width: usize,
}
pub struct RenderedMarkdown {
    pub rows: Vec<PreviewRow>,
    pub width: usize,
    pub total_source_lines: usize,
}
impl RenderedMarkdown {
    pub fn preview_row_for_source_line(&self, line: usize) -> usize {
        self.rows
            .partition_point(|r| {
                r.source_line < line.min(self.total_source_lines.saturating_sub(1))
            })
            .min(self.rows.len().saturating_sub(1))
    }
    pub fn source_line_for_preview_row(&self, row: usize) -> usize {
        self.rows
            .get(row.min(self.rows.len().saturating_sub(1)))
            .map_or(0, |r| r.source_line)
    }
}
fn color(fg: Color) -> StyledCellRun {
    StyledCellRun {
        fg,
        ..Default::default()
    }
}
impl PreviewRow {
    fn new(source_line: usize) -> Self {
        Self {
            source_line,
            spans: Vec::new(),
            width: 0,
        }
    }
    fn push(&mut self, s: &str, width: usize, style: StyledCellRun) {
        if s.is_empty() {
            return;
        }
        self.width += width;
        if let Some(last) = self.spans.last_mut().filter(|p| p.style == style) {
            last.text.push_str(s);
        } else {
            self.spans.push(PreviewSpan {
                text: s.to_owned(),
                style,
            });
        }
    }
    fn append(&mut self, other: Self) {
        self.width += other.width;
        self.spans.extend(other.spans);
    }
    fn pad(&mut self, n: usize) {
        self.push(&" ".repeat(n), n, StyledCellRun::default());
    }
}

fn styled(cell: &Cell, base: StyledCellRun) -> Vec<PreviewSpan> {
    cell.iter()
        .map(|s| {
            let style = s.style;
            PreviewSpan {
                text: s.text.clone(),
                style: StyledCellRun {
                    fg: if style.link { Color::Cyan } else { base.fg },
                    bg: if style.code {
                        Some(Color::DarkGrey)
                    } else {
                        base.bg
                    },
                    bold: base.bold || style.bold,
                    italic: base.italic || style.italic,
                    underlined: style.link,
                    crossed_out: style.strike,
                },
            }
        })
        .collect()
}
fn natural(s: &[PreviewSpan]) -> usize {
    let text: String = s.iter().map(|s| s.text.as_str()).collect();
    visual::graphemes(&text).map(|g| g.width).sum()
}
/// Wrap the visible text as one stream, including words that span style boundaries.
fn wrap(line: usize, spans: &[PreviewSpan], width: usize) -> Vec<PreviewRow> {
    let mut text = String::new();
    let mut styles = Vec::new();
    for s in spans {
        text.push_str(&s.text);
        styles.push((text.len(), s.style));
    }
    let mut index = 0;
    visual::wrap_segments(&text, width)
        .map(|seg| {
            let mut row = PreviewRow::new(line);
            for g in visual::graphemes(&text[seg.start_byte..seg.end_byte]) {
                let byte = seg.start_byte + g.byte_offset;
                while styles[index].0 <= byte {
                    index += 1;
                }
                let style = styles[index].1;
                if g.width <= width {
                    row.push(g.display_text(), g.width, style);
                } else if width > 0 {
                    row.push("�", 1, style);
                }
            }
            row
        })
        .collect()
}

pub fn solve_table_column_widths(natural: &[usize], width: usize) -> Option<Vec<usize>> {
    let n = natural.len();
    if n == 0 || width < 4 * n + 1 {
        return None;
    }
    let mut remaining = width - (3 * n + 1);
    let mut order: Vec<_> = (0..n).collect();
    order.sort_unstable_by_key(|&i| (natural[i], i));
    let mut widths = vec![1; n];
    for (k, i) in order.into_iter().enumerate() {
        widths[i] = natural[i].max(1).min(remaining / (n - k));
        remaining -= widths[i];
    }
    Some(widths)
}
fn border(line: usize, widths: &[usize], edges: [char; 3]) -> PreviewRow {
    let mut s = edges[0].to_string();
    for (i, &w) in widths.iter().enumerate() {
        s.push_str(&"─".repeat(w + 2));
        s.push(if i + 1 == widths.len() {
            edges[2]
        } else {
            edges[1]
        });
    }
    let mut row = PreviewRow::new(line);
    row.push(&s, s.chars().count(), color(Color::DarkCyan));
    row
}
fn table(
    out: &mut Vec<PreviewRow>,
    line: usize,
    width: usize,
    headers: &[Cell],
    align: &[ColumnAlign],
    rows: &[Vec<Cell>],
) {
    let normal = StyledCellRun::default();
    let edge = color(Color::DarkCyan);
    let data: Vec<_> = std::iter::once(headers)
        .chain(rows.iter().map(Vec::as_slice))
        .enumerate()
        .map(|(i, cells)| {
            cells
                .iter()
                .map(|c| {
                    styled(
                        c,
                        StyledCellRun {
                            bold: i == 0,
                            ..normal
                        },
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let natural_widths: Vec<_> = (0..headers.len())
        .map(|col| data.iter().map(|r| natural(&r[col])).max().unwrap_or(1))
        .collect();
    let Some(widths) = solve_table_column_widths(&natural_widths, width) else {
        if rows.is_empty() {
            for cell in &data[0] {
                out.extend(wrap(line, cell, width));
            }
        }
        for (i, r) in data.iter().skip(1).enumerate() {
            for (head, value) in data[0].iter().zip(r) {
                let mut spans = head.clone();
                spans.push(PreviewSpan {
                    text: ": ".into(),
                    style: normal,
                });
                spans.extend_from_slice(value);
                out.extend(wrap(line + i + 2, &spans, width));
            }
        }
        return;
    };
    out.push(border(line, &widths, ['┌', '┬', '┐']));
    for (i, cells) in data.iter().enumerate() {
        let source = if i == 0 { line } else { line + i + 1 };
        let wrapped: Vec<_> = cells
            .iter()
            .zip(&widths)
            .map(|(c, &w)| wrap(source, c, w))
            .collect();
        let height = wrapped.iter().map(Vec::len).max().unwrap_or(1);
        let mut cols: Vec<_> = wrapped.into_iter().map(Vec::into_iter).collect();
        for _ in 0..height {
            let mut row = PreviewRow::new(source);
            row.push("│", 1, edge);
            for ((col, &w), align) in cols.iter_mut().zip(&widths).zip(align) {
                let cell = col.next().unwrap_or_else(|| PreviewRow::new(source));
                let gap = w - cell.width;
                let left = match align {
                    ColumnAlign::Left => 0,
                    ColumnAlign::Center => gap / 2,
                    ColumnAlign::Right => gap,
                };
                row.pad(left + 1);
                row.append(cell);
                row.pad(gap - left + 1);
                row.push("│", 1, edge);
            }
            out.push(row);
        }
        if i == 0 {
            out.push(border(line + 1, &widths, ['├', '┼', '┤']));
        }
    }
    out.push(border(line + 1 + rows.len(), &widths, ['└', '┴', '┘']));
}

fn prefixed(
    out: &mut Vec<PreviewRow>,
    line: usize,
    width: usize,
    prefix: &str,
    repeat: bool,
    style: StyledCellRun,
    spans: &[PreviewSpan],
) {
    let prefix: String = prefix.chars().take(width.saturating_sub(1)).collect();
    let n = prefix.chars().count();
    for (i, body) in wrap(line, spans, width - n).into_iter().enumerate() {
        let mut row = PreviewRow::new(line);
        if i == 0 || repeat {
            row.push(&prefix, n, style);
        } else {
            row.pad(n);
        }
        row.append(body);
        out.push(row);
    }
}

fn code_block(out: &mut Vec<PreviewRow>, line: usize, width: usize, lang: &str, lines: &[String]) {
    let header = PreviewSpan {
        text: lang.to_owned(),
        style: color(Color::DarkCyan),
    };
    out.extend(wrap(line, &[header], width));
    let text = lines.join("\n");
    let tokens = Language::from_id(lang)
        .and_then(|lang| SyntaxHighlighter::new(lang).ok())
        .map(|mut h| {
            h.update(&text);
            h.highlight_range(0, text.len(), &text)
        })
        .unwrap_or_default();
    let mut token = 0;
    let mut offset = 0;
    for (i, body) in lines.iter().enumerate() {
        let end = offset + body.len();
        let mut pos = offset;
        let mut spans = Vec::new();
        while pos < end {
            while tokens.get(token).is_some_and(|t| t.end_byte <= pos) {
                token += 1;
            }
            let (stop, fg) = match tokens.get(token) {
                Some(t) if t.start_byte <= pos => (t.end_byte.min(end), token_color(t.token_type)),
                Some(t) => (end.min(t.start_byte), Color::Reset),
                None => (end, Color::Reset),
            };
            spans.push(PreviewSpan {
                text: text[pos..stop].to_owned(),
                style: color(fg),
            });
            pos = stop;
        }
        out.extend(wrap(line + i + 1, &spans, width));
        offset = end + 1;
    }
}
pub(crate) fn token_color(t: TokenType) -> Color {
    match t {
        TokenType::Keyword => Color::Cyan,
        TokenType::Function => Color::Blue,
        TokenType::Type => Color::Yellow,
        TokenType::String => Color::Green,
        TokenType::Number => Color::Magenta,
        TokenType::Comment => Color::DarkGrey,
        TokenType::Macro => Color::Red,
        _ => Color::Reset,
    }
}
pub fn layout_markdown(
    doc: &MarkdownDocument,
    width: usize,
    total_source_lines: usize,
) -> RenderedMarkdown {
    let mut rows = Vec::new();
    let normal = StyledCellRun::default();
    for block in &doc.blocks {
        match block {
            MarkdownBlock::Table {
                line,
                headers,
                align,
                rows: data,
            } => table(&mut rows, *line, width, headers, align, data),
            MarkdownBlock::Paragraph(line, cell) => {
                rows.extend(wrap(*line, &styled(cell, normal), width))
            }
            MarkdownBlock::Heading(line, level, cell) => rows.extend(wrap(
                *line,
                &styled(
                    cell,
                    StyledCellRun {
                        bold: true,
                        ..color(if *level <= 2 {
                            Color::Cyan
                        } else {
                            Color::Yellow
                        })
                    },
                ),
                width,
            )),
            MarkdownBlock::Rule(line) => {
                let mut row = PreviewRow::new(*line);
                row.push(&"─".repeat(width), width, color(Color::DarkCyan));
                rows.push(row);
            }
            MarkdownBlock::List(item) => {
                let marker = match (item.task, &item.marker) {
                    (Some(true), _) => "☑".into(),
                    (Some(false), _) => "☐".into(),
                    (None, ListMarker::Bullet) => "•".into(),
                    (None, ListMarker::Ordered(n)) => format!("{n}."),
                };
                let prefix = format!("{}{marker} ", " ".repeat(item.indent_level.min(width)));
                prefixed(
                    &mut rows,
                    item.source_line,
                    width,
                    &prefix,
                    false,
                    color(Color::Cyan),
                    &styled(&item.spans, normal),
                );
            }
            MarkdownBlock::BlockQuote(line, kind, cell) => {
                let (style, body) = if let Some(kind) = kind {
                    let fg = match kind {
                        AlertKind::Warning => Color::Yellow,
                        AlertKind::Caution => Color::Red,
                        AlertKind::Tip => Color::Green,
                        AlertKind::Important => Color::Magenta,
                        AlertKind::Note => Color::Cyan,
                    };
                    let style = StyledCellRun {
                        bold: true,
                        ..color(fg)
                    };
                    (
                        style,
                        vec![PreviewSpan {
                            text: format!("{kind:?}").to_uppercase(),
                            style,
                        }],
                    )
                } else {
                    (color(Color::Cyan), styled(cell, normal))
                };
                prefixed(&mut rows, *line, width, "│ ", true, style, &body);
            }
            MarkdownBlock::CodeBlock {
                line,
                language,
                lines,
            } => code_block(&mut rows, *line, width, language, lines),
        }
    }
    RenderedMarkdown {
        rows,
        width,
        total_source_lines,
    }
}
