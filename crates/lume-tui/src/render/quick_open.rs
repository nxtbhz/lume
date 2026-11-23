use std::io::Write;

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

use crate::app::App;
use crate::config::KeymapContext;
use crate::quick_open::{self, QuickOpenState, HEADER_ROWS};
use crate::render::text::fit_text;
use lume_core::buffer::visual;

/// Renders the Quick Open modal floating box.
pub fn render_quick_open<W: Write>(
    app: &App,
    quick_open: &QuickOpenState,
    writer: &mut W,
) -> std::io::Result<()> {
    let rect = quick_open::layout(app.terminal_width as usize, app.terminal_height as usize);
    let box_w = rect.width;
    if box_w == 0 || rect.height == 0 {
        return Ok(());
    }

    let bg_color = Color::Rgb {
        r: 20,
        g: 25,
        b: 35,
    };

    let scroll_offset = quick_open.visible_offset(rect.height);

    for y in 0..rect.height {
        queue!(
            writer,
            MoveTo(rect.left as u16, (rect.top + y) as u16),
            SetBackgroundColor(bg_color),
            SetForegroundColor(Color::White)
        )?;

        if y == 0 {
            // Title row
            let hints = app.config.keymaps.hints(
                KeymapContext::QuickOpen,
                &[("list.accept", "open"), ("list.cancel", "cancel")],
            );
            let title = if hints.is_empty() {
                String::from(" Quick Open File ")
            } else {
                format!(" Quick Open File ({hints}) ")
            };
            let count_str = format!(
                "[{}/{}] ",
                quick_open.filtered.len(),
                quick_open.files.len()
            );
            queue!(writer, SetForegroundColor(Color::Cyan))?;
            if count_str.len() < box_w {
                queue!(
                    writer,
                    Print(fit_text(&title, box_w - count_str.len())),
                    SetForegroundColor(Color::Yellow),
                    Print(count_str)
                )?;
            } else {
                queue!(writer, Print(fit_text(&title, box_w)))?;
            }
        } else if y == 1 {
            let prompt = format!(" > {}█", quick_open.query);
            queue!(writer, Print(fit_text(&prompt, box_w)))?;
        } else if y == 2 {
            // Divider row
            queue!(
                writer,
                SetForegroundColor(Color::DarkGrey),
                Print("─".repeat(box_w))
            )?;
        } else {
            // List item row
            let match_idx = scroll_offset + y - HEADER_ROWS;

            if match_idx < quick_open.filtered.len() {
                let m = &quick_open.filtered[match_idx];
                let item = &quick_open.files[m.item_idx];
                let is_sel = match_idx == quick_open.selected_idx;

                let row_bg = if is_sel { Color::DarkBlue } else { bg_color };

                let full_prefix = if is_sel { " > " } else { "   " };
                let prefix = &full_prefix[..box_w.min(full_prefix.len())];
                let indicator = " [Tab]";
                let tab_indicator = if item.is_open_tab && box_w > prefix.len() + indicator.len() {
                    indicator
                } else {
                    ""
                };

                queue!(
                    writer,
                    SetBackgroundColor(row_bg),
                    SetForegroundColor(if is_sel {
                        Color::Yellow
                    } else {
                        Color::DarkGrey
                    }),
                    Print(prefix)
                )?;

                let avail_width = box_w.saturating_sub(prefix.len() + tab_indicator.len());

                // Find filename boundary in item.relative_path
                let filename_start = item
                    .relative_path
                    .rfind('/')
                    .map(|pos| item.relative_path[..pos].chars().count() + 1)
                    .unwrap_or(0);

                let mut columns_printed = 0;
                let mut char_idx = 0;
                for grapheme in visual::graphemes(&item.relative_path) {
                    if grapheme.column + grapheme.width > avail_width {
                        break;
                    }
                    let end_char = char_idx + grapheme.text.chars().count();
                    if grapheme.width > 0 {
                        let is_matched = m
                            .match_indices
                            .iter()
                            .any(|idx| (char_idx..end_char).contains(idx));
                        let color = if is_matched {
                            Color::Green
                        } else if is_sel || char_idx >= filename_start {
                            Color::White
                        } else {
                            Color::DarkGrey
                        };
                        queue!(
                            writer,
                            SetForegroundColor(color),
                            Print(grapheme.display_text())
                        )?;
                        columns_printed += grapheme.width;
                    }
                    char_idx = end_char;
                }

                // If space remains, fill before tab indicator
                let spaces = avail_width - columns_printed;
                if spaces > 0 {
                    queue!(writer, Print(" ".repeat(spaces)))?;
                }

                if !tab_indicator.is_empty() {
                    queue!(
                        writer,
                        SetForegroundColor(Color::Magenta),
                        Print(tab_indicator)
                    )?;
                }
            } else {
                queue!(
                    writer,
                    SetBackgroundColor(bg_color),
                    Print(" ".repeat(box_w))
                )?;
            }
        }
    }

    queue!(writer, ResetColor)?;
    Ok(())
}
