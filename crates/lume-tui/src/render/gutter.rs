use std::io::Write;

use crossterm::{
    queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
};
use lume_core::diff::LineDiffStatus;

pub fn render_gutter<W: Write>(
    writer: &mut W,
    buffer_line: usize,
    total_lines: usize,
    gutter_w: usize,
    diff_status: Option<LineDiffStatus>,
) -> std::io::Result<()> {
    let num_width = gutter_w.saturating_sub(3);
    if buffer_line < total_lines {
        queue!(
            writer,
            SetForegroundColor(Color::DarkGrey),
            Print(format!("{:>width$}", buffer_line + 1, width = num_width)),
            ResetColor
        )?;

        match diff_status {
            Some(LineDiffStatus::Added) => {
                queue!(
                    writer,
                    SetForegroundColor(Color::Green),
                    Print("+"),
                    ResetColor
                )?;
            }
            Some(LineDiffStatus::Modified) => {
                queue!(
                    writer,
                    SetForegroundColor(Color::Yellow),
                    Print("~"),
                    ResetColor
                )?;
            }
            Some(LineDiffStatus::DeletedAbove) => {
                queue!(
                    writer,
                    SetForegroundColor(Color::Red),
                    Print("-"),
                    ResetColor
                )?;
            }
            _ => {
                queue!(writer, Print(" "))?;
            }
        }

        queue!(
            writer,
            SetForegroundColor(Color::DarkGrey),
            Print("│ "),
            ResetColor
        )?;
    } else {
        queue!(
            writer,
            SetForegroundColor(Color::DarkGrey),
            Print(format!("{:>width$} ~ ", "~", width = num_width)),
            ResetColor
        )?;
    }
    Ok(())
}
