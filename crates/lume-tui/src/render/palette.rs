use std::io::Write;

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

use crate::app::App;

pub fn render_command_palette<W: Write>(app: &App, writer: &mut W) -> std::io::Result<()> {
    let box_w = (app.terminal_width as usize).clamp(36, 68);
    let box_h = 11;
    let start_x = (app.terminal_width as usize).saturating_sub(box_w) / 2;
    let start_y = (app.terminal_height as usize).saturating_sub(box_h) / 3;

    let commands = app.filtered_commands();
    let visible_rows = box_h.saturating_sub(3);
    let max_selected = commands.len().saturating_sub(1);
    let selected = app.palette_selected.min(max_selected);
    let scroll_offset = selected.saturating_sub(visible_rows.saturating_sub(1));

    // Background box
    for y in 0..box_h {
        queue!(
            writer,
            MoveTo(start_x as u16, (start_y + y) as u16),
            SetBackgroundColor(Color::Rgb {
                r: 20,
                g: 25,
                b: 35
            }),
            SetForegroundColor(Color::White)
        )?;

        if y == 0 {
            let title = " Command Palette (Esc to close) ";
            let title_fit: String = title.chars().take(box_w).collect();
            let fill = box_w.saturating_sub(title_fit.chars().count());
            queue!(
                writer,
                SetForegroundColor(Color::Cyan),
                Print(format!("{}{:fill$}", title_fit, "", fill = fill))
            )?;
        } else if y == 1 {
            let prompt = format!(" > {}█", app.palette_query);
            let prompt_fit: String = prompt.chars().take(box_w).collect();
            let fill = box_w.saturating_sub(prompt_fit.chars().count());
            queue!(
                writer,
                SetForegroundColor(Color::White),
                Print(format!("{}{:fill$}", prompt_fit, "", fill = fill))
            )?;
        } else if y == 2 {
            queue!(
                writer,
                SetForegroundColor(Color::DarkGrey),
                Print("─".repeat(box_w))
            )?;
        } else {
            let item_idx = scroll_offset + (y - 3);
            if let Some(item) = commands.get(item_idx) {
                let is_sel = item_idx == selected;
                let prefix = if is_sel { " > " } else { "   " };
                let title = item.title;
                let shortcut = item.shortcut;

                let prefix_chars = prefix.chars().count();
                let shortcut_chars = shortcut.chars().count();
                let avail = box_w.saturating_sub(prefix_chars + shortcut_chars + 2);
                let title_fit: String = title.chars().take(avail).collect();
                let title_chars = title_fit.chars().count();
                let spaces = box_w.saturating_sub(prefix_chars + title_chars + shortcut_chars + 1);

                if is_sel {
                    queue!(
                        writer,
                        SetBackgroundColor(Color::DarkBlue),
                        SetForegroundColor(Color::Yellow)
                    )?;
                } else {
                    queue!(
                        writer,
                        SetBackgroundColor(Color::Rgb {
                            r: 20,
                            g: 25,
                            b: 35
                        }),
                        SetForegroundColor(Color::White)
                    )?;
                }

                queue!(
                    writer,
                    Print(format!(
                        "{}{}{:spaces$}{} ",
                        prefix,
                        title_fit,
                        "",
                        shortcut,
                        spaces = spaces
                    ))
                )?;
            } else {
                queue!(writer, Print(" ".repeat(box_w)))?;
            }
        }
    }

    queue!(writer, ResetColor)?;
    Ok(())
}

pub fn render_which_key<W: Write>(app: &App, writer: &mut W) -> std::io::Result<()> {
    let box_w = (app.terminal_width as usize).clamp(44, 80);
    let box_h = 10;
    let start_x = (app.terminal_width as usize).saturating_sub(box_w) / 2;
    let start_y = (app.terminal_height as usize).saturating_sub(box_h) / 2;

    let lines = [
        " LUME SHORTCUTS CHEAT SHEET (Press any key or Esc to close) ",
        " [File]      Ctrl+O: Open   │ Ctrl+S: Save      │ Ctrl+Shift+S: Save As ",
        " [Tabs]      Ctrl+T: New    │ Ctrl+W: Close     │ Ctrl+Tab / Alt+Left/Right: Switch ",
        " [Split]     Ctrl+K \\: V-Split │ Ctrl+K -: H-Split │ Ctrl+K W: Switch │ Ctrl+K Q: Close ",
        " [Explorer]  Ctrl+B/F2: Toggle │ a: New File │ n: New Folder │ m/F2: Rename │ d: Del │ r: Refresh ",
        " [Edit]      Ctrl+A: All │ Ctrl+X: Cut │ Ctrl+C: Copy │ Ctrl+V: Paste │ Ctrl+Z/Y: Undo/Redo ",
        " [Cursor]    Arrows: Move   │ Shift+Arrows: Select │ Home / End ",
        " [Mouse]     Click: Position/Open │ 2xClick: Word │ 3xClick: Line │ Drag: Sel ",
        " [Features]  Ctrl+P: Quick Open │ Ctrl+G: Go to Line │ Ctrl+Shift+P / Alt+P: Palette ",
        " [Quit]      Ctrl+Q: Exit Lume ",
    ];

    for (i, line) in lines.iter().enumerate() {
        queue!(
            writer,
            MoveTo(start_x as u16, (start_y + i) as u16),
            SetBackgroundColor(Color::Rgb {
                r: 25,
                g: 30,
                b: 45
            }),
            SetForegroundColor(if i == 0 { Color::Yellow } else { Color::White })
        )?;
        let line_fit: String = line.chars().take(box_w).collect();
        let fill = box_w.saturating_sub(line_fit.chars().count());
        queue!(
            writer,
            Print(format!("{}{:fill$}", line_fit, "", fill = fill))
        )?;
    }

    queue!(writer, ResetColor)?;
    Ok(())
}
