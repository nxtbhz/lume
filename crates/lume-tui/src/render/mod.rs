pub mod editor;
pub mod explorer;
pub mod gutter;
pub mod palette;
pub mod status_bar;
pub mod tab_bar;

pub use editor::render_editor_viewport;
pub use palette::{render_command_palette, render_which_key};
pub use status_bar::render_status_bar;
pub use tab_bar::render_tab_bar;

use std::io::Write;

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    queue,
};

use crate::app::{ActiveFocus, App};
use lume_core::TextBuffer;

impl App {
    pub fn render<W: Write>(&mut self, writer: &mut W) -> std::io::Result<()> {
        let gutter_w = self.gutter_width();
        let text_h = self.text_area_height();
        let text_w = self.text_area_width();
        let text_left = self.text_area_left();
        let total_lines = self.editor.buffer().len_lines();
        let active_tab_idx = self.editor.active_tab_idx();

        // Begin synchronized frame update to eliminate all screen tearing and flickering
        writer.write_all(b"\x1b[?2026h")?;
        if let Some(osc) = self.pending_osc52.take() {
            writer.write_all(osc.as_bytes())?;
        }
        // Keep terminal cursor hidden during entire painting phase to avoid cursor teleporting across cells
        queue!(writer, Hide, MoveTo(0, 0))?;

        // 1. Render File Explorer (if visible)
        if self.explorer.is_visible {
            let is_exp_focused = self.focus == ActiveFocus::Explorer;
            self.explorer.render(
                writer,
                0,
                self.terminal_height.saturating_sub(1) as usize,
                is_exp_focused,
            )?;
        }

        // 2. Render Tab Bar
        render_tab_bar(self, writer, text_left, active_tab_idx)?;

        // 3 & 4. Render Text Area (Gutter + Editor viewport)
        render_editor_viewport(
            self,
            writer,
            text_left,
            text_h,
            text_w,
            gutter_w,
            total_lines,
        )?;

        // 5. Render Status Bar or Footer Search Bar (Last Row)
        render_status_bar(self, writer, active_tab_idx)?;

        // 6. Overlays
        if let Some(ref prompt) = self.prompt {
            prompt.render(writer, self.terminal_width, self.terminal_height)?;
        } else if self.palette_open {
            render_command_palette(self, writer)?;
        } else if self.which_key_open {
            render_which_key(self, writer)?;
        }

        // 7. Position terminal cursor
        if self.search_state.is_none()
            && self.prompt.is_none()
            && !self.palette_open
            && !self.which_key_open
        {
            if self.focus == ActiveFocus::Editor {
                let (cursor_row, cursor_col) = self.editor.cursor_line_col();
                let in_view = cursor_row >= self.scroll_row
                    && cursor_row < self.scroll_row + text_h
                    && cursor_col >= self.scroll_col
                    && cursor_col < self.scroll_col + text_w;

                if in_view {
                    let term_cursor_row =
                        (self.text_area_top() + cursor_row - self.scroll_row) as u16;
                    let term_cursor_col =
                        (text_left + gutter_w + cursor_col - self.scroll_col) as u16;
                    queue!(writer, MoveTo(term_cursor_col, term_cursor_row), Show)?;
                } else {
                    queue!(writer, Hide)?;
                }
            } else {
                // Focus in explorer: hide terminal cursor, row selection handles highlight
                queue!(writer, Hide)?;
            }
        }

        // End synchronized frame update
        writer.write_all(b"\x1b[?2026l")?;
        writer.flush()?;
        Ok(())
    }
}
