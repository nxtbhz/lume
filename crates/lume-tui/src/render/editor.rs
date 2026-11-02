use std::io::Write;

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{Clear, ClearType},
};

use crate::app::App;
use crate::render::gutter::render_gutter;
use crate::split::{PaneRect, SplitDirection};
use lume_core::{CursorSet, TextBuffer};
use lume_syntax::TokenType;

#[allow(clippy::too_many_arguments)]
fn render_pane_viewport<W: Write>(
    app: &mut App,
    writer: &mut W,
    rect: PaneRect,
    tab_idx: usize,
    scroll_row: usize,
    scroll_col: usize,
    cursors: &CursorSet,
    is_split: bool,
    is_active_pane: bool,
) -> std::io::Result<()> {
    if rect.width == 0 || rect.height == 0 || app.editor.tabs().is_empty() {
        return Ok(());
    }

    let tab_idx = tab_idx.min(app.editor.tabs().len().saturating_sub(1));
    let tokens = app.update_syntax_highlights_for_tab(tab_idx);
    let diff_markers = app.editor.tabs_mut()[tab_idx].diff_markers();

    let raw_gutter_w = app.gutter_width_for_tab(tab_idx);
    let gutter_w = raw_gutter_w.min(rect.width);
    let text_w = rect.width.saturating_sub(gutter_w);

    let total_lines = app.editor.tabs()[tab_idx].buffer.len_lines();
    let max_bytes = app.editor.tabs()[tab_idx].buffer.len_bytes();

    let mut primary_cursor = *cursors.primary();
    primary_cursor.head = primary_cursor.head.min(max_bytes);
    primary_cursor.anchor = primary_cursor.anchor.min(max_bytes);
    let selection_range = if primary_cursor.is_selection() {
        Some(primary_cursor.min()..primary_cursor.max())
    } else {
        None
    };

    let bracket_pair =
        if selection_range.is_none() && is_active_pane && tab_idx == app.editor.active_tab_idx() {
            let min_visible_char = app.editor.tabs()[tab_idx]
                .buffer
                .rope()
                .line_to_char(scroll_row.min(total_lines));
            let max_visible_line = (scroll_row + rect.height).min(total_lines);
            let max_visible_char = app.editor.tabs()[tab_idx]
                .buffer
                .rope()
                .line_to_char(max_visible_line);
            app.editor
                .matching_bracket_for_cursor(min_visible_char, max_visible_char)
        } else {
            None
        };

    for screen_row in 0..rect.height {
        let buffer_line = scroll_row + screen_row;
        let term_row = (rect.top + screen_row) as u16;
        queue!(writer, MoveTo(rect.left as u16, term_row))?;

        let mut printed_cols = 0usize;

        if buffer_line < total_lines {
            if app.show_line_numbers && gutter_w > 0 {
                let diff_status = diff_markers
                    .as_ref()
                    .and_then(|m| m.get(buffer_line).copied());
                render_gutter(writer, buffer_line, total_lines, gutter_w, diff_status)?;
                printed_cols += gutter_w;
            }

            if text_w > 0 {
                let tab_buffer = &app.editor.tabs()[tab_idx].buffer;
                if let Some(line) = tab_buffer.line(buffer_line) {
                    let line_start_byte = tab_buffer.line_to_byte(buffer_line);
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

                    let line_matches: Vec<(usize, usize)> = if let Some(ref search) =
                        app.search_state
                    {
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
                    let mut run_text = String::new();
                    let mut run_style: Option<(Option<Color>, Color)> = None;

                    for (col_idx, &c) in chars.iter().enumerate() {
                        let char_len = c.len_utf8();
                        let char_byte = current_byte;
                        current_byte += char_len;

                        if col_idx < scroll_col || col_idx >= scroll_col + text_w {
                            continue;
                        }

                        let is_selected = selection_range
                            .as_ref()
                            .is_some_and(|r| r.contains(&char_byte));
                        let search_match = line_matches
                            .iter()
                            .find(|(s, e)| *s <= char_byte && char_byte < *e);
                        let is_bracket_match =
                            bracket_pair.is_some_and(|(b1, b2)| char_byte == b1 || char_byte == b2);

                        let style: (Option<Color>, Color) =
                            if let (Some(ref _search), Some(&(ms, me))) =
                                (&app.search_state, search_match)
                            {
                                let is_active = primary_cursor.range() == (ms..me);
                                if is_active {
                                    (
                                        Some(Color::Rgb {
                                            r: 215,
                                            g: 155,
                                            b: 0,
                                        }),
                                        Color::Black,
                                    )
                                } else {
                                    (
                                        Some(Color::Rgb {
                                            r: 120,
                                            g: 90,
                                            b: 20,
                                        }),
                                        Color::White,
                                    )
                                }
                            } else if is_selected {
                                (Some(Color::DarkBlue), Color::White)
                            } else if is_bracket_match {
                                (
                                    Some(Color::Rgb {
                                        r: 70,
                                        g: 70,
                                        b: 110,
                                    }),
                                    Color::White,
                                )
                            } else {
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
                                (None, fg)
                            };

                        if run_style == Some(style) {
                            run_text.push(c);
                        } else {
                            if let Some((prev_bg, prev_fg)) = run_style {
                                if let Some(bg) = prev_bg {
                                    queue!(writer, SetBackgroundColor(bg))?;
                                }
                                queue!(
                                    writer,
                                    SetForegroundColor(prev_fg),
                                    Print(&run_text),
                                    ResetColor
                                )?;
                                run_text.clear();
                            }
                            run_style = Some(style);
                            run_text.push(c);
                        }
                        printed_cols += 1;
                    }

                    if let Some((prev_bg, prev_fg)) = run_style {
                        if let Some(bg) = prev_bg {
                            queue!(writer, SetBackgroundColor(bg))?;
                        }
                        queue!(
                            writer,
                            SetForegroundColor(prev_fg),
                            Print(&run_text),
                            ResetColor
                        )?;
                    }
                }
            }
        } else if app.show_line_numbers && gutter_w > 0 {
            render_gutter(writer, buffer_line, total_lines, gutter_w, None)?;
            printed_cols += gutter_w;
        }

        if is_split {
            let pad = rect.width.saturating_sub(printed_cols);
            if pad > 0 {
                queue!(writer, Print(" ".repeat(pad)))?;
            }
        } else {
            queue!(writer, Clear(ClearType::UntilNewLine))?;
        }
    }

    Ok(())
}

pub fn render_editor_viewport<W: Write>(
    app: &mut App,
    writer: &mut W,
    text_left: usize,
    text_h: usize,
    _text_w: usize,
    _gutter_w: usize,
    _total_lines: usize,
) -> std::io::Result<()> {
    let text_top = app.text_area_top();
    let area_w = (app.terminal_width as usize).saturating_sub(text_left);

    if !app.splits.is_split {
        let rect = PaneRect {
            left: text_left,
            top: text_top,
            width: area_w,
            height: text_h,
        };
        let active_tab_idx = app.editor.active_tab_idx();
        let scroll_row = app.scroll_row;
        let scroll_col = app.scroll_col;
        let cursors = app.editor.cursors().clone();
        return render_pane_viewport(
            app,
            writer,
            rect,
            active_tab_idx,
            scroll_row,
            scroll_col,
            &cursors,
            false,
            true,
        );
    }

    app.sync_active_pane();
    let (p0_rect, sep_rect, p1_rect) = app.splits.layout(text_left, text_top, area_w, text_h);
    let p0 = app.splits.panes[0].clone();
    let p1 = app.splits.panes[1].clone();
    let direction = app.splits.direction;
    let active_pane = app.splits.active_pane;

    render_pane_viewport(
        app,
        writer,
        p0_rect,
        p0.tab_idx,
        p0.scroll_row,
        p0.scroll_col,
        &p0.cursors,
        true,
        active_pane == 0,
    )?;

    if let Some(sep) = sep_rect {
        match direction {
            SplitDirection::Vertical => {
                if sep.width > 0 {
                    for r in 0..sep.height {
                        queue!(
                            writer,
                            MoveTo(sep.left as u16, (sep.top + r) as u16),
                            SetForegroundColor(Color::DarkGrey),
                            Print("│"),
                            ResetColor
                        )?;
                    }
                }
            }
            SplitDirection::Horizontal => {
                if sep.height > 0 && sep.width > 0 {
                    queue!(
                        writer,
                        MoveTo(sep.left as u16, sep.top as u16),
                        SetForegroundColor(Color::DarkGrey),
                        Print("─".repeat(sep.width)),
                        ResetColor
                    )?;
                }
            }
        }
    }

    if let Some(p1_rect) = p1_rect {
        render_pane_viewport(
            app,
            writer,
            p1_rect,
            p1.tab_idx,
            p1.scroll_row,
            p1.scroll_col,
            &p1.cursors,
            true,
            active_pane == 1,
        )?;
    }

    Ok(())
}
