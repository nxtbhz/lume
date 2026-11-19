use std::io::Write;

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

use crate::app::App;
use crate::render::text::fit_text;
use lume_core::Tab;

pub(crate) fn tab_label(tab: &Tab, idx: usize, active: bool) -> String {
    let dirty = if tab.is_dirty { "*" } else { "" };
    let name = tab.name();
    if active {
        format!(" [ {}: {name}{dirty} x ] ", idx + 1)
    } else {
        format!("  {}: {name}{dirty} x   ", idx + 1)
    }
}

pub fn render_tab_bar<W: Write>(
    app: &App,
    writer: &mut W,
    text_left: usize,
    active_tab_idx: usize,
) -> std::io::Result<()> {
    let width = (app.terminal_width as usize).saturating_sub(text_left);
    if width == 0 {
        return Ok(());
    }
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
        tab_header.push_str(&tab_label(tab, i, i == active_tab_idx));
    }
    tab_header.push_str(" [ + ]");

    queue!(writer, Print(fit_text(&tab_header, width)), ResetColor)?;
    Ok(())
}
