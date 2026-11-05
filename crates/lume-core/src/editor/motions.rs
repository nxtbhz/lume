use crate::buffer::TextBuffer;
use crate::editor::Editor;

impl Editor {
    pub(crate) fn line_content_len_chars(rope: &ropey::Rope, line: usize) -> usize {
        let line_start_char = rope.line_to_char(line);
        let line_len = rope.line(line).len_chars();
        if line_len > 0 && rope.char(line_start_char + line_len - 1) == '\n' {
            if line_len > 1 && rope.char(line_start_char + line_len - 2) == '\r' {
                line_len - 2
            } else {
                line_len - 1
            }
        } else if line_len > 0 && rope.char(line_start_char + line_len - 1) == '\r' {
            line_len - 1
        } else {
            line_len
        }
    }

    pub fn cursor_line_col(&self) -> (usize, usize) {
        self.tabs[self.active_tab].cursor_line_col()
    }

    pub fn cursor_visual_line_col(&self) -> (usize, usize) {
        self.tabs[self.active_tab].cursor_visual_line_col()
    }

    pub fn set_cursor_from_line_col(&mut self, line: usize, col: usize, select: bool) {
        let tab = &mut self.tabs[self.active_tab];
        let total_lines = tab.buffer.len_lines();
        let target_line = line.min(total_lines.saturating_sub(1));
        let line_start_char = tab.buffer.rope().line_to_char(target_line);
        let content_len = Self::line_content_len_chars(tab.buffer.rope(), target_line);

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

    pub fn select_all(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        let end_byte = tab.buffer.len_bytes();
        let primary = tab.cursors.primary_mut();
        primary.anchor = 0;
        primary.head = end_byte;
        tab.cursors.normalize();
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
            let rope = tab.buffer.rope();
            let target_char = if char_idx >= 2
                && rope.char(char_idx - 1) == '\n'
                && rope.char(char_idx - 2) == '\r'
            {
                char_idx - 2
            } else {
                char_idx - 1
            };
            let prev_byte = tab.buffer.char_to_byte(target_char);
            let primary = tab.cursors.primary_mut();
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
            let total_chars = tab.buffer.len_chars();
            let rope = tab.buffer.rope();
            let target_char = if char_idx + 1 < total_chars
                && rope.char(char_idx) == '\r'
                && rope.char(char_idx + 1) == '\n'
            {
                char_idx + 2
            } else {
                char_idx + 1
            };
            let next_byte = tab.buffer.char_to_byte(target_char);
            let primary = tab.cursors.primary_mut();
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
        let prev_line_content_len = Self::line_content_len_chars(tab.buffer.rope(), prev_line);

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
        let next_line_content_len = Self::line_content_len_chars(tab.buffer.rope(), next_line);

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
        let content_len = Self::line_content_len_chars(tab.buffer.rope(), line);

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
        let target_content_len = Self::line_content_len_chars(tab.buffer.rope(), target_line);

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
