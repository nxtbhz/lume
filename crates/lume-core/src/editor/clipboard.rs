use crate::buffer::{TextBuffer, TextDelta};
use crate::cursor::Cursor;
use crate::editor::Editor;
use crate::history::HistoryManager;

impl Editor {
    /// Returns the current clipboard contents, if any.
    pub fn clipboard(&self) -> Option<&str> {
        self.clipboard.as_deref()
    }

    /// Sets the clipboard contents directly.
    pub fn set_clipboard(&mut self, text: impl Into<String>) {
        self.clipboard = Some(text.into());
    }

    /// Copies selected text across all cursors, or the entire current line if no selection exists.
    /// Stores the copied text in `self.clipboard` and returns it.
    pub fn copy_selection(&mut self) -> Option<String> {
        let tab = &self.tabs[self.active_tab];
        let total_bytes = tab.buffer.len_bytes();
        if total_bytes == 0 {
            return None;
        }

        let has_selection = tab.cursors.cursors().iter().any(|c| c.is_selection());
        let text = if has_selection {
            let mut sorted_cursors = tab.cursors.cursors().to_vec();
            sorted_cursors.sort_by_key(|c| (c.min(), c.max()));
            let mut pieces = Vec::new();
            for c in sorted_cursors {
                if c.is_selection() {
                    let min = c.min().min(total_bytes);
                    let max = c.max().min(total_bytes);
                    if min < max {
                        pieces.push(tab.buffer.slice_to_cow(min, max).to_string());
                    }
                }
            }
            if pieces.is_empty() {
                return None;
            }
            pieces.join("\n")
        } else {
            let head = tab.cursors.primary().head.min(total_bytes);
            let line = tab.buffer.byte_to_line(head);
            let line_start_char = tab.buffer.rope().line_to_char(line);
            let line_start_byte = tab.buffer.char_to_byte(line_start_char);
            let next_line = line + 1;
            let line_end_byte = if next_line < tab.buffer.len_lines() {
                let next_char = tab.buffer.rope().line_to_char(next_line);
                tab.buffer.char_to_byte(next_char)
            } else {
                total_bytes
            };
            if line_start_byte >= line_end_byte {
                return None;
            }
            tab.buffer
                .slice_to_cow(line_start_byte, line_end_byte)
                .to_string()
        };

        self.clipboard = Some(text.clone());
        Some(text)
    }

    /// Cuts selected text across all cursors (or the current line if no selection exists) atomically.
    /// Stores the cut text in `self.clipboard` and returns it.
    pub fn cut_selection(&mut self) -> Option<String> {
        let tab = &self.tabs[self.active_tab];
        let total_bytes = tab.buffer.len_bytes();
        if total_bytes == 0 {
            return None;
        }

        let has_selection = tab.cursors.cursors().iter().any(|c| c.is_selection());
        let tab = &mut self.tabs[self.active_tab];
        let cursors_before = tab.cursors.cursors().to_vec();

        if has_selection {
            let mut sorted_cursors = cursors_before.clone();
            sorted_cursors.sort_by_key(|c| (c.min(), c.max()));

            let mut pieces = Vec::new();
            for c in &sorted_cursors {
                if c.is_selection() {
                    let min = c.min().min(total_bytes);
                    let max = c.max().min(total_bytes);
                    if min < max {
                        pieces.push(tab.buffer.slice_to_cow(min, max).to_string());
                    }
                }
            }
            if pieces.is_empty() {
                return None;
            }
            let cut_text = pieces.join("\n");

            let mut deltas: Vec<TextDelta> = Vec::new();
            let mut new_cursors: Vec<Cursor> = Vec::new();
            let mut delta_offset: isize = 0;

            for c in sorted_cursors {
                if c.is_selection() {
                    let start = (c.min() as isize + delta_offset).max(0) as usize;
                    let end = (c.max() as isize + delta_offset).max(0) as usize;
                    if start < end && end <= tab.buffer.len_bytes() {
                        if let Ok(delta) = tab.buffer.delete(start, end) {
                            deltas.push(delta);
                            new_cursors.push(Cursor::new(start));
                            delta_offset -= (end - start) as isize;
                            continue;
                        }
                    }
                    new_cursors.push(Cursor::new(start));
                } else {
                    let pos = (c.head as isize + delta_offset).max(0) as usize;
                    new_cursors.push(Cursor::new(pos));
                }
            }

            if !deltas.is_empty() {
                tab.cursors.set_cursors(new_cursors);
                let cursors_after = tab.cursors.cursors().to_vec();
                tab.history
                    .record_edits(deltas.clone(), cursors_before, cursors_after);
                self.record_edit_internal(&deltas);
            }

            self.clipboard = Some(cut_text.clone());
            Some(cut_text)
        } else {
            let head = tab.cursors.primary().head.min(total_bytes);
            let line = tab.buffer.byte_to_line(head);
            let line_start_char = tab.buffer.rope().line_to_char(line);
            let line_start_byte = tab.buffer.char_to_byte(line_start_char);
            let next_line = line + 1;
            let line_end_byte = if next_line < tab.buffer.len_lines() {
                let next_char = tab.buffer.rope().line_to_char(next_line);
                tab.buffer.char_to_byte(next_char)
            } else {
                total_bytes
            };
            if line_start_byte >= line_end_byte {
                return None;
            }
            let cut_text = tab
                .buffer
                .slice_to_cow(line_start_byte, line_end_byte)
                .to_string();

            if let Ok(delta) = tab.buffer.delete(line_start_byte, line_end_byte) {
                let new_pos = line_start_byte.min(tab.buffer.len_bytes());
                tab.cursors.set_single(new_pos);
                let cursors_after = tab.cursors.cursors().to_vec();
                let deltas = vec![delta];
                tab.history
                    .record_edits(deltas.clone(), cursors_before, cursors_after);
                self.record_edit_internal(&deltas);
            }

            self.clipboard = Some(cut_text.clone());
            Some(cut_text)
        }
    }

    /// Pastes the clipboard contents at active cursor positions (replacing selections).
    /// Returns true if pasted, or false if clipboard was empty.
    pub fn paste(&mut self) -> bool {
        if let Some(text) = self.clipboard.clone() {
            self.insert_str(&text).is_ok()
        } else {
            false
        }
    }
}
