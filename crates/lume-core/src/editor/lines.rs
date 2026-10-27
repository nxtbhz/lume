use crate::buffer::{BufferError, TextBuffer};
use crate::editor::Editor;
use crate::history::HistoryManager;

impl Editor {
    pub fn delete_line(&mut self) -> Result<String, BufferError> {
        let tab = &mut self.tabs[self.active_tab];
        let total_lines = tab.buffer.len_lines();
        let total_bytes = tab.buffer.len_bytes();
        if total_bytes == 0 {
            return Ok(String::new());
        }

        let head = tab.cursors.primary().head;
        let line = tab.buffer.byte_to_line(head);
        let line_start_byte = tab.buffer.line_to_byte(line);

        let (delete_start, delete_end) = if line_start_byte == total_bytes && line > 0 {
            let prev_line = line - 1;
            let prev_line_start = tab.buffer.line_to_byte(prev_line);
            (prev_line_start, total_bytes)
        } else if line + 1 >= total_lines && line > 0 && line_start_byte > 0 {
            let last_char = tab
                .buffer
                .rope()
                .char(tab.buffer.len_chars().saturating_sub(1));
            if last_char != '\n' {
                let prev_line_end = tab.buffer.line_to_byte(line);
                let prev_newline_byte = tab
                    .buffer
                    .char_to_byte(tab.buffer.byte_to_char(prev_line_end).saturating_sub(1));
                (prev_newline_byte, total_bytes)
            } else {
                (line_start_byte, total_bytes)
            }
        } else {
            let line_end_byte = if line + 1 < total_lines {
                tab.buffer.line_to_byte(line + 1)
            } else {
                total_bytes
            };
            (line_start_byte, line_end_byte)
        };

        let cursors_before = tab.cursors.cursors().to_vec();
        let mut deleted_slice = tab
            .buffer
            .slice_to_cow(delete_start, delete_end)
            .into_owned();

        if deleted_slice.starts_with('\n') {
            deleted_slice = deleted_slice[1..].to_string();
        }
        if !deleted_slice.ends_with('\n') {
            deleted_slice.push('\n');
        }

        let delta = tab.buffer.delete(delete_start, delete_end)?;
        let deltas = vec![delta];

        let new_head = delete_start.min(tab.buffer.len_bytes());
        tab.cursors.set_single(new_head);
        let cursors_after = tab.cursors.cursors().to_vec();

        tab.history
            .record_edits(deltas.clone(), cursors_before, cursors_after);
        self.record_edit_internal(&deltas);

        Ok(deleted_slice)
    }

    pub fn yank_line(&self) -> String {
        let tab = self.active_tab();
        let total_lines = tab.buffer.len_lines();
        let total_bytes = tab.buffer.len_bytes();
        if total_bytes == 0 {
            return String::new();
        }

        let head = tab.cursors.primary().head;
        let line = tab.buffer.byte_to_line(head);
        let line_start_byte = tab.buffer.line_to_byte(line);
        let line_end_byte = if line + 1 < total_lines {
            tab.buffer.line_to_byte(line + 1)
        } else {
            total_bytes
        };

        let mut yanked = tab
            .buffer
            .slice_to_cow(line_start_byte, line_end_byte)
            .into_owned();
        if !yanked.ends_with('\n') {
            yanked.push('\n');
        }
        yanked
    }

    pub fn paste_yank(&mut self, text: &str, linewise: bool) -> Result<(), BufferError> {
        if text.is_empty() {
            return Ok(());
        }

        let tab = &mut self.tabs[self.active_tab];
        let cursors_before = tab.cursors.cursors().to_vec();

        let (insert_byte, new_cursor_pos) = if linewise {
            let total_lines = tab.buffer.len_lines();
            let total_bytes = tab.buffer.len_bytes();
            if total_bytes == 0 {
                (0, 0)
            } else {
                let head = tab.cursors.primary().head;
                let line = tab.buffer.byte_to_line(head);
                if line + 1 < total_lines {
                    let next_line_start = tab.buffer.line_to_byte(line + 1);
                    (next_line_start, next_line_start)
                } else {
                    (total_bytes, total_bytes)
                }
            }
        } else {
            let primary = tab.cursors.primary();
            if primary.is_selection() {
                (primary.min(), primary.min())
            } else {
                let head = primary.head;
                let total_bytes = tab.buffer.len_bytes();
                if head < total_bytes {
                    let char_idx = tab.buffer.byte_to_char(head);
                    let ch = tab.buffer.rope().char(char_idx);
                    if ch == '\n' {
                        (head, head)
                    } else {
                        let next_byte = tab.buffer.char_to_byte(char_idx + 1);
                        (next_byte, next_byte)
                    }
                } else {
                    (total_bytes, total_bytes)
                }
            }
        };

        let mut deltas = Vec::new();
        let text_to_insert = if linewise && insert_byte == tab.buffer.len_bytes() && insert_byte > 0
        {
            let last_c = tab.buffer.rope().char(tab.buffer.len_chars() - 1);
            if last_c != '\n' {
                let mut s = String::from("\n");
                s.push_str(text);
                s
            } else {
                text.to_string()
            }
        } else {
            text.to_string()
        };

        let delta = tab.buffer.insert(insert_byte, &text_to_insert)?;
        deltas.push(delta);

        tab.cursors.set_single(new_cursor_pos);
        let cursors_after = tab.cursors.cursors().to_vec();

        tab.history
            .record_edits(deltas.clone(), cursors_before, cursors_after);
        self.record_edit_internal(&deltas);

        Ok(())
    }
}
