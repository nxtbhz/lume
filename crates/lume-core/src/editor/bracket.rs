use crate::buffer::TextBuffer;
use crate::editor::Editor;

fn matching_bracket_char(c: char) -> Option<(char, bool)> {
    match c {
        '(' => Some((')', true)),
        '[' => Some((']', true)),
        '{' => Some(('}', true)),
        ')' => Some(('(', false)),
        ']' => Some(('[', false)),
        '}' => Some(('{', false)),
        _ => None,
    }
}

impl Editor {
    pub fn find_matching_bracket_in_range(
        &self,
        pos: usize,
        min_char: usize,
        max_char: usize,
    ) -> Option<usize> {
        let tab = self.active_tab();
        let len_bytes = tab.buffer.len_bytes();
        if pos >= len_bytes {
            return None;
        }

        let char_idx = tab.buffer.byte_to_char(pos);
        // Ensure pos lands on a character boundary
        if tab.buffer.char_to_byte(char_idx) != pos {
            return None;
        }

        // Ensure starting bracket is strictly inside the bounded char range
        if char_idx < min_char || char_idx >= max_char {
            return None;
        }

        let start_char = tab.buffer.rope().char(char_idx);
        let (target_char, is_opening) = matching_bracket_char(start_char)?;

        let mut depth: usize = 1;

        if is_opening {
            for (current_char_idx, ch) in
                (char_idx + 1..).zip(tab.buffer.rope().chars_at(char_idx + 1))
            {
                if current_char_idx >= max_char {
                    break;
                }
                if ch == start_char {
                    depth += 1;
                } else if ch == target_char {
                    depth -= 1;
                    if depth == 0 {
                        return Some(tab.buffer.char_to_byte(current_char_idx));
                    }
                }
            }
        } else {
            if char_idx == 0 || char_idx <= min_char {
                return None;
            }
            let mut current_char_idx = char_idx;
            for ch in tab.buffer.rope().chars_at(char_idx).reversed() {
                current_char_idx = current_char_idx.saturating_sub(1);
                if current_char_idx < min_char {
                    break;
                }
                if ch == start_char {
                    depth += 1;
                } else if ch == target_char {
                    depth -= 1;
                    if depth == 0 {
                        return Some(tab.buffer.char_to_byte(current_char_idx));
                    }
                }
                if current_char_idx == 0 {
                    break;
                }
            }
        }

        None
    }

    pub fn find_matching_bracket(&self, pos: usize) -> Option<usize> {
        let total_chars = self.active_tab().buffer.len_chars();
        self.find_matching_bracket_in_range(pos, 0, total_chars)
    }

    pub fn jump_to_matching_bracket(&mut self) -> bool {
        let tab = &self.tabs[self.active_tab];
        let head = tab.cursors.primary().head;
        let len = tab.buffer.len_bytes();

        // 1. Check char under cursor
        if head < len {
            if let Some(target) = self.find_matching_bracket(head) {
                self.tabs[self.active_tab]
                    .cursors
                    .primary_mut()
                    .move_to(target);
                return true;
            }
        }

        // 2. Check char immediately before cursor (e.g. right after a closing bracket)
        if head > 0 {
            let prev_char_idx = tab.buffer.byte_to_char(head).saturating_sub(1);
            let prev_byte = tab.buffer.char_to_byte(prev_char_idx);
            let prev_char = tab.buffer.rope().char(prev_char_idx);
            if matches!(prev_char, ')' | ']' | '}') {
                if let Some(target) = self.find_matching_bracket(prev_byte) {
                    self.tabs[self.active_tab]
                        .cursors
                        .primary_mut()
                        .move_to(target);
                    return true;
                }
            }
        }

        // 3. Vim % motion: search forward on current line for first bracket
        let current_line = tab.buffer.byte_to_line(head);
        let line_start_char = tab.buffer.rope().line_to_char(current_line);
        let line_len_chars = tab.buffer.line_len_chars(current_line);
        let head_char = tab.buffer.byte_to_char(head);
        let start_char_idx = head_char.saturating_add(1);
        let end_char_idx = line_start_char + line_len_chars;

        for ch_idx in start_char_idx..end_char_idx {
            let ch = tab.buffer.rope().char(ch_idx);
            if matches!(ch, '(' | ')' | '[' | ']' | '{' | '}') {
                let b_idx = tab.buffer.char_to_byte(ch_idx);
                if let Some(target) = self.find_matching_bracket(b_idx) {
                    self.tabs[self.active_tab]
                        .cursors
                        .primary_mut()
                        .move_to(target);
                    return true;
                }
            }
        }

        false
    }

    pub fn matching_bracket_for_cursor(
        &self,
        min_char: usize,
        max_char: usize,
    ) -> Option<(usize, usize)> {
        let tab = self.active_tab();
        let primary = tab.cursors.primary();
        if primary.is_selection() {
            return None;
        }

        let head = primary.head;
        let len = tab.buffer.len_bytes();
        let head_char = tab.buffer.byte_to_char(head);

        // 1. Check char under cursor
        if head < len {
            if let Some(match_pos) = self.find_matching_bracket_in_range(head, min_char, max_char) {
                return Some((head, match_pos));
            }
        }

        // 2. Check char immediately before cursor if at closing bracket
        if head > 0 {
            let prev_char_idx = head_char.saturating_sub(1);
            let prev_byte = tab.buffer.char_to_byte(prev_char_idx);
            let prev_char = tab.buffer.rope().char(prev_char_idx);
            if matches!(prev_char, ')' | ']' | '}') {
                if let Some(match_pos) =
                    self.find_matching_bracket_in_range(prev_byte, min_char, max_char)
                {
                    return Some((prev_byte, match_pos));
                }
            }
        }

        None
    }
}
