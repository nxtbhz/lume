use crate::buffer::{BufferError, TextBuffer};
use crate::editor::Editor;
use crate::history::HistoryManager;

impl Editor {
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let tab = &self.tabs[self.active_tab];
        let head = tab.cursors.primary().head;
        let line = tab.buffer.byte_to_line(head);
        let line_start_char = tab.buffer.rope().line_to_char(line);
        let cursor_char = tab.buffer.byte_to_char(head);
        let col = cursor_char.saturating_sub(line_start_char);
        (line, col)
    }

    pub fn set_cursor_from_line_col(&mut self, line: usize, col: usize, select: bool) {
        let tab = &mut self.tabs[self.active_tab];
        let total_lines = tab.buffer.len_lines();
        let target_line = line.min(total_lines.saturating_sub(1));
        let line_start_char = tab.buffer.rope().line_to_char(target_line);
        let line_len_chars = tab.buffer.line_len_chars(target_line);
        let content_len = if line_len_chars > 0
            && tab.buffer.rope().char(line_start_char + line_len_chars - 1) == '\n'
        {
            line_len_chars - 1
        } else {
            line_len_chars
        };

        let target_char = line_start_char + col.min(content_len);
        let target_byte = tab.buffer.char_to_byte(target_char);

        let primary = tab.cursors.primary_mut();
        if select {
            primary.select_to(target_byte);
        } else {
            primary.move_to(target_byte);
        }
    }

    pub fn select_word_at_cursor(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        let head = tab.cursors.primary().head;
        let len = tab.buffer.len_bytes();
        if len == 0 {
            return;
        }

        let char_idx = tab.buffer.byte_to_char(head);
        let total_chars = tab.buffer.len_chars();

        // Scan backwards
        let mut start_char = char_idx;
        while start_char > 0 {
            let prev_c = tab.buffer.rope().char(start_char - 1);
            if prev_c.is_alphanumeric() || prev_c == '_' {
                start_char -= 1;
            } else {
                break;
            }
        }

        // Scan forwards
        let mut end_char = char_idx;
        while end_char < total_chars {
            let c = tab.buffer.rope().char(end_char);
            if c.is_alphanumeric() || c == '_' {
                end_char += 1;
            } else {
                break;
            }
        }

        let start_byte = tab.buffer.char_to_byte(start_char);
        let end_byte = tab.buffer.char_to_byte(end_char);

        let primary = tab.cursors.primary_mut();
        primary.anchor = start_byte;
        primary.head = end_byte;
    }

    pub fn select_line_at_cursor(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        let head = tab.cursors.primary().head;
        let line = tab.buffer.byte_to_line(head);
        let start_byte = tab.buffer.line_to_byte(line);
        let next_line = line + 1;
        let end_byte = if next_line < tab.buffer.len_lines() {
            tab.buffer.line_to_byte(next_line)
        } else {
            tab.buffer.len_bytes()
        };

        let primary = tab.cursors.primary_mut();
        primary.anchor = start_byte;
        primary.head = end_byte;
    }

    pub fn move_left(&mut self, select: bool) {
        let tab = &mut self.tabs[self.active_tab];
        let primary = tab.cursors.primary_mut();
        if !select && primary.is_selection() {
            let target = primary.min();
            primary.move_to(target);
            return;
        }

        if primary.head > 0 {
            let char_idx = tab.buffer.byte_to_char(primary.head);
            let prev_byte = tab.buffer.char_to_byte(char_idx - 1);
            if select {
                primary.select_to(prev_byte);
            } else {
                primary.move_to(prev_byte);
            }
        }
    }

    pub fn move_right(&mut self, select: bool) {
        let tab = &mut self.tabs[self.active_tab];
        let primary = tab.cursors.primary_mut();
        if !select && primary.is_selection() {
            let target = primary.max();
            primary.move_to(target);
            return;
        }

        if primary.head < tab.buffer.len_bytes() {
            let char_idx = tab.buffer.byte_to_char(primary.head);
            let next_byte = tab.buffer.char_to_byte(char_idx + 1);
            if select {
                primary.select_to(next_byte);
            } else {
                primary.move_to(next_byte);
            }
        }
    }

    pub fn move_up(&mut self, select: bool) {
        let tab = &mut self.tabs[self.active_tab];
        let head = tab.cursors.primary().head;
        let line = tab.buffer.byte_to_line(head);
        if line == 0 {
            if !select {
                tab.cursors.primary_mut().move_to(0);
            } else {
                tab.cursors.primary_mut().select_to(0);
            }
            return;
        }

        let line_start_char = tab.buffer.rope().line_to_char(line);
        let cursor_char = tab.buffer.byte_to_char(head);
        let col = cursor_char.saturating_sub(line_start_char);

        let prev_line = line - 1;
        let prev_line_start_char = tab.buffer.rope().line_to_char(prev_line);
        let prev_line_len = tab.buffer.line_len_chars(prev_line);
        let prev_line_content_len = if prev_line_len > 0
            && tab
                .buffer
                .rope()
                .char(prev_line_start_char + prev_line_len - 1)
                == '\n'
        {
            prev_line_len - 1
        } else {
            prev_line_len
        };

        let target_char = prev_line_start_char + col.min(prev_line_content_len);
        let target_byte = tab.buffer.char_to_byte(target_char);

        let primary = tab.cursors.primary_mut();
        if select {
            primary.select_to(target_byte);
        } else {
            primary.move_to(target_byte);
        }
    }

    pub fn move_down(&mut self, select: bool) {
        let tab = &mut self.tabs[self.active_tab];
        let head = tab.cursors.primary().head;
        let line = tab.buffer.byte_to_line(head);
        let total_lines = tab.buffer.len_lines();
        if line + 1 >= total_lines {
            let end_byte = tab.buffer.len_bytes();
            if !select {
                tab.cursors.primary_mut().move_to(end_byte);
            } else {
                tab.cursors.primary_mut().select_to(end_byte);
            }
            return;
        }

        let line_start_char = tab.buffer.rope().line_to_char(line);
        let cursor_char = tab.buffer.byte_to_char(head);
        let col = cursor_char.saturating_sub(line_start_char);

        let next_line = line + 1;
        let next_line_start_char = tab.buffer.rope().line_to_char(next_line);
        let next_line_len = tab.buffer.line_len_chars(next_line);
        let next_line_content_len = if next_line_len > 0
            && tab
                .buffer
                .rope()
                .char(next_line_start_char + next_line_len - 1)
                == '\n'
        {
            next_line_len - 1
        } else {
            next_line_len
        };

        let target_char = next_line_start_char + col.min(next_line_content_len);
        let target_byte = tab.buffer.char_to_byte(target_char);

        let primary = tab.cursors.primary_mut();
        if select {
            primary.select_to(target_byte);
        } else {
            primary.move_to(target_byte);
        }
    }

    pub fn move_home(&mut self, select: bool) {
        let tab = &mut self.tabs[self.active_tab];
        let head = tab.cursors.primary().head;
        let line = tab.buffer.byte_to_line(head);
        let start_byte = tab.buffer.line_to_byte(line);

        let primary = tab.cursors.primary_mut();
        if select {
            primary.select_to(start_byte);
        } else {
            primary.move_to(start_byte);
        }
    }

    pub fn move_end(&mut self, select: bool) {
        let tab = &mut self.tabs[self.active_tab];
        let head = tab.cursors.primary().head;
        let line = tab.buffer.byte_to_line(head);
        let line_start_char = tab.buffer.rope().line_to_char(line);
        let line_len = tab.buffer.line_len_chars(line);
        let content_len =
            if line_len > 0 && tab.buffer.rope().char(line_start_char + line_len - 1) == '\n' {
                line_len - 1
            } else {
                line_len
            };

        let target_char = line_start_char + content_len;
        let target_byte = tab.buffer.char_to_byte(target_char);

        let primary = tab.cursors.primary_mut();
        if select {
            primary.select_to(target_byte);
        } else {
            primary.move_to(target_byte);
        }
    }

    pub fn move_lines(&mut self, delta: isize, select: bool) {
        if delta == 0 {
            return;
        }
        let tab = &mut self.tabs[self.active_tab];
        let total_lines = tab.buffer.len_lines();
        if total_lines == 0 {
            return;
        }

        let head = tab.cursors.primary().head;
        let line = tab.buffer.byte_to_line(head);

        let target_line = if delta < 0 {
            line.saturating_sub(delta.unsigned_abs())
        } else {
            (line + delta as usize).min(total_lines.saturating_sub(1))
        };

        let line_start_char = tab.buffer.rope().line_to_char(line);
        let cursor_char = tab.buffer.byte_to_char(head);
        let col = cursor_char.saturating_sub(line_start_char);

        let target_line_start_char = tab.buffer.rope().line_to_char(target_line);
        let target_line_len = tab.buffer.line_len_chars(target_line);
        let target_content_len = if target_line_len > 0
            && tab
                .buffer
                .rope()
                .char(target_line_start_char + target_line_len - 1)
                == '\n'
        {
            target_line_len - 1
        } else {
            target_line_len
        };

        let target_char = target_line_start_char + col.min(target_content_len);
        let target_byte = tab.buffer.char_to_byte(target_char);

        let primary = tab.cursors.primary_mut();
        if select {
            primary.select_to(target_byte);
        } else {
            primary.move_to(target_byte);
        }
    }

    pub fn page_up(&mut self, lines: usize, select: bool) {
        self.move_lines(-(lines as isize), select);
    }

    pub fn page_down(&mut self, lines: usize, select: bool) {
        self.move_lines(lines as isize, select);
    }

    pub fn move_to_start(&mut self, select: bool) {
        let tab = &mut self.tabs[self.active_tab];
        let primary = tab.cursors.primary_mut();
        if select {
            primary.select_to(0);
        } else {
            primary.move_to(0);
        }
    }

    pub fn move_to_end(&mut self, select: bool) {
        let tab = &mut self.tabs[self.active_tab];
        let end_byte = tab.buffer.len_bytes();
        let primary = tab.cursors.primary_mut();
        if select {
            primary.select_to(end_byte);
        } else {
            primary.move_to(end_byte);
        }
    }

    pub fn move_word_forward(&mut self, select: bool) {
        let tab = &mut self.tabs[self.active_tab];
        let total_chars = tab.buffer.len_chars();
        if total_chars == 0 {
            return;
        }

        let head = tab.cursors.primary().head;
        let mut char_idx = tab.buffer.byte_to_char(head);
        if char_idx >= total_chars {
            return;
        }

        let rope = tab.buffer.rope();
        let current_c = rope.char(char_idx);

        fn is_word_char(c: char) -> bool {
            c.is_alphanumeric() || c == '_'
        }

        if is_word_char(current_c) {
            while char_idx < total_chars && is_word_char(rope.char(char_idx)) {
                char_idx += 1;
            }
        } else if !current_c.is_whitespace() {
            while char_idx < total_chars {
                let c = rope.char(char_idx);
                if !is_word_char(c) && !c.is_whitespace() {
                    char_idx += 1;
                } else {
                    break;
                }
            }
        }

        while char_idx < total_chars && rope.char(char_idx).is_whitespace() {
            char_idx += 1;
        }

        let target_byte = tab.buffer.char_to_byte(char_idx.min(total_chars));
        let primary = tab.cursors.primary_mut();
        if select {
            primary.select_to(target_byte);
        } else {
            primary.move_to(target_byte);
        }
    }

    pub fn move_word_backward(&mut self, select: bool) {
        let tab = &mut self.tabs[self.active_tab];
        let total_chars = tab.buffer.len_chars();
        if total_chars == 0 {
            return;
        }

        let head = tab.cursors.primary().head;
        let mut char_idx = tab.buffer.byte_to_char(head);
        if char_idx == 0 {
            let primary = tab.cursors.primary_mut();
            if select {
                primary.select_to(0);
            } else {
                primary.move_to(0);
            }
            return;
        }

        let rope = tab.buffer.rope();

        fn is_word_char(c: char) -> bool {
            c.is_alphanumeric() || c == '_'
        }

        char_idx -= 1;
        while char_idx > 0 && rope.char(char_idx).is_whitespace() {
            char_idx -= 1;
        }

        if char_idx == 0 && rope.char(0).is_whitespace() {
            let primary = tab.cursors.primary_mut();
            if select {
                primary.select_to(0);
            } else {
                primary.move_to(0);
            }
            return;
        }

        let current_c = rope.char(char_idx);
        if is_word_char(current_c) {
            while char_idx > 0 && is_word_char(rope.char(char_idx - 1)) {
                char_idx -= 1;
            }
        } else if !current_c.is_whitespace() {
            while char_idx > 0 {
                let prev_c = rope.char(char_idx - 1);
                if !is_word_char(prev_c) && !prev_c.is_whitespace() {
                    char_idx -= 1;
                } else {
                    break;
                }
            }
        }

        let target_byte = tab.buffer.char_to_byte(char_idx);
        let primary = tab.cursors.primary_mut();
        if select {
            primary.select_to(target_byte);
        } else {
            primary.move_to(target_byte);
        }
    }

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
