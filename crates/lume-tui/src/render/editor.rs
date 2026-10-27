use std::io::Write;

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{Clear, ClearType},
};

use crate::app::App;
use crate::render::gutter::render_gutter;
use lume_core::TextBuffer;
use lume_syntax::TokenType;

pub fn render_editor_viewport<W: Write>(
    app: &mut App,
    writer: &mut W,
    text_left: usize,
    text_h: usize,
    text_w: usize,
    gutter_w: usize,
    total_lines: usize,
) -> std::io::Result<()> {
    // 3. Syntax tokens from background worker cache (re-parsed asynchronously off UI thread)
    let tokens = app.update_syntax_highlights();
    let primary_cursor = *app.editor.cursors().primary();
    let selection_range = if primary_cursor.is_selection() {
        Some(primary_cursor.min()..primary_cursor.max())
    } else {
        None
    };

    let min_visible_char = app
        .editor
        .buffer()
        .rope()
        .line_to_char(app.scroll_row.min(total_lines));
    let max_visible_line = (app.scroll_row + text_h).min(total_lines);
    let max_visible_char = app.editor.buffer().rope().line_to_char(max_visible_line);

    let bracket_pair = if selection_range.is_none() {
        app.editor
            .matching_bracket_for_cursor(min_visible_char, max_visible_char)
    } else {
        None
    };

    // 4. Render Text Area (Rows 1..1+text_h)
    for screen_row in 0..text_h {
        let buffer_line = app.scroll_row + screen_row;
        let term_row = (app.text_area_top() + screen_row) as u16;
        queue!(writer, MoveTo(text_left as u16, term_row))?;

        if buffer_line < total_lines {
            // Line numbers gutter
            if app.show_line_numbers {
                render_gutter(writer, buffer_line, total_lines, gutter_w)?;
            }

            // Render characters with syntax and selection styling
            if let Some(line) = app.editor.buffer().line(buffer_line) {
                let line_start_byte = app.editor.buffer().line_to_byte(buffer_line);
                let mut line_str = line.to_string();
                if line_str.ends_with('\n') {
                    line_str.pop();
                    if line_str.ends_with('\r') {
                        line_str.pop();
                    }
                }

                let line_end_byte = line_start_byte + line_str.len();
                let start_tok = tokens.partition_point(|t| t.end_byte <= line_start_byte);
                let rest_tokens = &tokens[start_tok..];
                let end_tok = rest_tokens.partition_point(|t| t.start_byte < line_end_byte);
                let line_tokens = &rest_tokens[..end_tok];

                let line_matches: Vec<(usize, usize)> = if let Some(ref search) = app.search_state {
                    if !search.query.is_empty() {
                        lume_core::search::find_matches_in_str(&line_str, &search.query, false)
                            .into_iter()
                            .map(|(s, e)| (line_start_byte + s, line_start_byte + e))
                            .collect()
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };

                let chars: Vec<char> = line_str.chars().collect();
                let mut current_byte = line_start_byte;

                for (col_idx, &c) in chars.iter().enumerate() {
                    let char_len = c.len_utf8();
                    let char_byte = current_byte;
                    current_byte += char_len;

                    if col_idx < app.scroll_col || col_idx >= app.scroll_col + text_w {
                        continue;
                    }

                    // Determine selection background
                    let is_selected = selection_range
                        .as_ref()
                        .is_some_and(|r| r.contains(&char_byte));
                    let search_match = line_matches
                        .iter()
                        .find(|(s, e)| *s <= char_byte && char_byte < *e);
                    let is_bracket_match =
                        bracket_pair.is_some_and(|(b1, b2)| char_byte == b1 || char_byte == b2);

                    if let (Some(ref _search), Some(&(ms, me))) = (&app.search_state, search_match)
                    {
                        let is_active = primary_cursor.range() == (ms..me);
                        if is_active {
                            queue!(
                                writer,
                                SetBackgroundColor(Color::Rgb {
                                    r: 215,
                                    g: 155,
                                    b: 0
                                }),
                                SetForegroundColor(Color::Black)
                            )?;
                        } else {
                            queue!(
                                writer,
                                SetBackgroundColor(Color::Rgb {
                                    r: 120,
                                    g: 90,
                                    b: 20
                                }),
                                SetForegroundColor(Color::White)
                            )?;
                        }
                    } else if is_selected {
                        queue!(
                            writer,
                            SetBackgroundColor(Color::DarkBlue),
                            SetForegroundColor(Color::White)
                        )?;
                    } else if is_bracket_match {
                        queue!(
                            writer,
                            SetBackgroundColor(Color::Rgb {
                                r: 70,
                                g: 70,
                                b: 110
                            }),
                            SetForegroundColor(Color::White)
                        )?;
                    } else {
                        // Syntax color via binary search on current line's tokens
                        let token_type = line_tokens
                            .binary_search_by(|t| {
                                if char_byte < t.start_byte {
                                    std::cmp::Ordering::Greater
                                } else if char_byte >= t.end_byte {
                                    std::cmp::Ordering::Less
                                } else {
                                    std::cmp::Ordering::Equal
                                }
                            })
                            .ok()
                            .map(|idx| line_tokens[idx].token_type);

                        let fg = match token_type {
                            Some(TokenType::Keyword) => Color::Cyan,
                            Some(TokenType::Function) => Color::Blue,
                            Some(TokenType::Type) => Color::Yellow,
                            Some(TokenType::String) => Color::Green,
                            Some(TokenType::Number) => Color::Magenta,
                            Some(TokenType::Comment) => Color::DarkGrey,
                            Some(TokenType::Macro) => Color::Red,
                            _ => Color::Reset,
                        };
                        queue!(writer, SetForegroundColor(fg))?;
                    }

                    queue!(writer, Print(c), ResetColor)?;
                }
            }
        } else if app.show_line_numbers {
            render_gutter(writer, buffer_line, total_lines, gutter_w)?;
        }

        queue!(writer, Clear(ClearType::UntilNewLine))?;
    }

    Ok(())
}
