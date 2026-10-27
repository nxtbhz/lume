use crate::buffer::TextBuffer;
use crate::editor::Editor;

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
}
