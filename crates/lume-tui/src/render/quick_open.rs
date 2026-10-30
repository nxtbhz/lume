use std::io::Write;

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

use crate::app::App;
use crate::quick_open::QuickOpenState;

/// Renders the Quick Open modal floating box.
pub fn render_quick_open<W: Write>(
    app: &App,
    quick_open: &QuickOpenState,
    writer: &mut W,
) -> std::io::Result<()> {
    let term_w = app.terminal_width as usize;
    let term_h = app.terminal_height as usize;

    let box_w = (term_w * 3 / 4).clamp(50, 84).min(term_w);
    let box_h = 14.min(term_h.saturating_sub(4)).max(6);
    let start_x = term_w.saturating_sub(box_w) / 2;
    let start_y = 3.min(term_h.saturating_sub(box_h) / 2);

    let bg_color = Color::Rgb {
        r: 20,
        g: 25,
        b: 35,
    };

    let _list_height = box_h.saturating_sub(3);

    for y in 0..box_h {
        queue!(
            writer,
            MoveTo(start_x as u16, (start_y + y) as u16),
            SetBackgroundColor(bg_color),
            SetForegroundColor(Color::White)
        )?;

        if y == 0 {
            // Title row
            let title = " Quick Open File (Ctrl+P, Esc: cancel) ";
            let count_str = format!(
                "[{}/{}] ",
                quick_open.filtered.len(),
                quick_open.files.len()
            );
            let title_avail = box_w.saturating_sub(count_str.chars().count() + 1);
            let display_title: String = title.chars().take(title_avail).collect();
            let fill =
                box_w.saturating_sub(display_title.chars().count() + count_str.chars().count());

            queue!(
                writer,
                SetForegroundColor(Color::Cyan),
                Print(&display_title),
                SetForegroundColor(Color::DarkGrey),
                Print(" ".repeat(fill)),
                SetForegroundColor(Color::Yellow),
                Print(&count_str)
            )?;
        } else if y == 1 {
            // Query input row
            let prompt_prefix = " > ";
            let cursor = "█";
            let avail = box_w.saturating_sub(prompt_prefix.chars().count() + 1);
            let display_query: String = quick_open.query.chars().take(avail).collect();
            let fill = box_w
                .saturating_sub(prompt_prefix.chars().count() + display_query.chars().count() + 1);

            queue!(
                writer,
                SetForegroundColor(Color::Yellow),
                Print(prompt_prefix),
                SetForegroundColor(Color::White),
                Print(&display_query),
                SetForegroundColor(Color::Cyan),
                Print(cursor),
                Print(" ".repeat(fill))
            )?;
        } else if y == 2 {
            // Divider row
            queue!(
                writer,
                SetForegroundColor(Color::DarkGrey),
                Print("─".repeat(box_w))
            )?;
        } else {
            // List item row
            let row_idx = y - 3;
            let match_idx = quick_open.scroll_offset + row_idx;

            if match_idx < quick_open.filtered.len() {
                let m = &quick_open.filtered[match_idx];
                let item = &quick_open.files[m.item_idx];
                let is_sel = match_idx == quick_open.selected_idx;

                let row_bg = if is_sel { Color::DarkBlue } else { bg_color };

                let prefix = if is_sel { " > " } else { "   " };
                let tab_indicator = if item.is_open_tab { " [Tab]" } else { "" };

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

                let mut chars_printed = 0;
                for (c_idx, ch) in item.relative_path.chars().enumerate() {
                    if chars_printed >= avail_width {
                        break;
                    }

                    let is_matched = m.match_indices.contains(&c_idx);
                    if is_matched {
                        queue!(writer, SetForegroundColor(Color::Green), Print(ch))?;
                    } else if is_sel {
                        queue!(writer, SetForegroundColor(Color::White), Print(ch))?;
                    } else if c_idx >= filename_start {
                        queue!(writer, SetForegroundColor(Color::White), Print(ch))?;
                    } else {
                        queue!(writer, SetForegroundColor(Color::DarkGrey), Print(ch))?;
                    }
                    chars_printed += 1;
                }

                // If space remains, fill before tab indicator
                let spaces = avail_width.saturating_sub(chars_printed);
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
