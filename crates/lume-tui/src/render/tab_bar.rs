use std::io::Write;

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

use crate::app::App;

pub fn render_tab_bar<W: Write>(
    app: &App,
    writer: &mut W,
    text_left: usize,
    active_tab_idx: usize,
) -> std::io::Result<()> {
    queue!(
        writer,
        MoveTo(text_left as u16, 0),
        SetBackgroundColor(Color::Rgb {
            r: 30,
            g: 30,
            b: 30
        }),
        SetForegroundColor(Color::White)
    )?;

    let mut tab_header = String::new();
    for (i, tab) in app.editor.tabs().iter().enumerate() {
        let dirty = if tab.is_dirty { "*" } else { "" };
        let is_active = i == active_tab_idx;
        if is_active {
            tab_header.push_str(&format!(" [ {}: {}{} x ] ", i + 1, tab.name(), dirty));
        } else {
            tab_header.push_str(&format!("  {}: {}{} x   ", i + 1, tab.name(), dirty));
        }
    }
    tab_header.push_str(" [ + ]");

    let tab_bar_w = (app.terminal_width as usize).saturating_sub(text_left);
    let tab_bar_fill = format!("{:width$}", tab_header, width = tab_bar_w);
    let tab_bar_trimmed: String = tab_bar_fill.chars().take(tab_bar_w).collect();
    queue!(writer, Print(tab_bar_trimmed), ResetColor)?;
    Ok(())
}
