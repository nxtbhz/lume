use crate::{app::App, split::PaneRect};
use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Attribute, Print, SetAttribute, SetBackgroundColor, SetForegroundColor},
};
use std::io::Write;

pub fn render_markdown_pane<W: Write>(
    app: &mut App,
    writer: &mut W,
    rect: PaneRect,
    tab_idx: usize,
    scroll: usize,
    _split: bool,
) -> std::io::Result<usize> {
    let layout = app.preview_layout(tab_idx, rect.width);
    let top = scroll.min(layout.rows.len().saturating_sub(rect.height.max(1)));
    for y in 0..rect.height {
        queue!(writer, MoveTo(rect.left as u16, (rect.top + y) as u16))?;
        let mut used = 0;
        if let Some(row) = layout.rows.get(top + y) {
            for span in &row.spans {
                let s = span.style;
                queue!(writer, SetForegroundColor(s.fg))?;
                if let Some(bg) = s.bg {
                    queue!(writer, SetBackgroundColor(bg))?;
                }
                for (enabled, a) in [
                    (s.bold, Attribute::Bold),
                    (s.italic, Attribute::Italic),
                    (s.underlined, Attribute::Underlined),
                    (s.crossed_out, Attribute::CrossedOut),
                ] {
                    if enabled {
                        queue!(writer, SetAttribute(a))?;
                    }
                }
                queue!(writer, Print(&span.text), SetAttribute(Attribute::Reset))?;
            }
            used = row.width;
        }
        queue!(writer, Print(" ".repeat(layout.width.saturating_sub(used))))?;
    }
    Ok(top)
}
