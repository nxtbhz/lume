use std::io::Write;

use crossterm::{
    queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
};

pub fn render_gutter<W: Write>(
    writer: &mut W,
    buffer_line: usize,
    total_lines: usize,
    gutter_w: usize,
) -> std::io::Result<()> {
    if buffer_line < total_lines {
        queue!(
            writer,
            SetForegroundColor(Color::DarkGrey),
            Print(format!(
                "{:>width$} │ ",
                buffer_line + 1,
                width = gutter_w - 3
            )),
            ResetColor
        )?;
    } else {
        queue!(
            writer,
            SetForegroundColor(Color::DarkGrey),
            Print(format!("{:>width$} ~ ", "~", width = gutter_w - 3)),
            ResetColor
        )?;
    }
    Ok(())
}
