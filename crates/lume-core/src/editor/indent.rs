use std::sync::atomic::Ordering;

use crate::buffer::{BufferError, TextBuffer, TextDelta};
use crate::cursor::Cursor;
use crate::editor::Editor;
use crate::history::HistoryManager;
use crate::journal::Journal;

fn matching_closing_pair(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        '\'' => Some('\''),
        _ => None,
    }
}

fn is_closing_pair(c: char) -> bool {
    matches!(c, ')' | ']' | '}' | '"' | '\'')
}

impl Editor {
    pub(crate) fn record_edit_internal(&mut self, deltas: &[TextDelta]) {
        let tab = &mut self.tabs[self.active_tab];
        tab.is_dirty = true;
        tab.revision += 1;
        let sync_clone = tab.sync_state.clone();
        if let Some(ref sync) = sync_clone {
            sync.revision.store(tab.revision, Ordering::SeqCst);
            sync.is_dirty.store(true, Ordering::SeqCst);
        }
        self.autosave.notify_change();

        // Write append-only binary deltas to crash recovery journal using cached journal path
        if let Some(ref path) = self.tabs[self.active_tab].file_path {
            let jpath = self.tabs[self.active_tab]
                .journal_path
                .clone()
                .unwrap_or_else(|| Journal::journal_path(path));
            if deltas.is_empty() {
                Journal::enqueue_snapshot(
                    path.clone(),
                    jpath,
                    self.tabs[self.active_tab].buffer.rope().clone(),
                );
            } else {
                Journal::enqueue_deltas(path.clone(), jpath, deltas.to_vec());
            }
        }
    }

    pub fn insert_char(&mut self, c: char) -> Result<(), BufferError> {
        let can_coalesce = !c.is_whitespace();
        if !self.config.auto_pairs {
            let mut text = [0u8; 4];
            let s = c.encode_utf8(&mut text);
            return self.insert_str_with_coalescing(s, can_coalesce);
        }

        let tab = &self.tabs[self.active_tab];

        // Check if overtype applies to all cursors (cursor head right before matching closing char)
        let can_overtype = tab.cursors.cursors().iter().all(|cur| {
            if cur.is_selection() || cur.head >= tab.buffer.len_bytes() {
                false
            } else {
                let next_char = tab.buffer.rope().char(tab.buffer.byte_to_char(cur.head));
                is_closing_pair(c) && next_char == c
            }
        });

        if can_overtype && !tab.cursors.cursors().is_empty() {
            let tab_mut = &mut self.tabs[self.active_tab];
            let mut new_cursors = Vec::with_capacity(tab_mut.cursors.cursors().len());
            for cur in tab_mut.cursors.cursors() {
                let next_byte = cur.head + c.len_utf8();
                new_cursors.push(Cursor::new(next_byte));
            }
            tab_mut.cursors.set_cursors(new_cursors);
            tab_mut.history.break_group();
            return Ok(());
        }

        // Check if auto-pairing applies for opening delimiter
        if let Some(close_char) = matching_closing_pair(c) {
            let should_pair = tab.cursors.cursors().iter().all(|cur| {
                if cur.is_selection() {
                    false
                } else if cur.head >= tab.buffer.len_bytes() {
                    true // EOF
                } else {
                    let next_char = tab.buffer.rope().char(tab.buffer.byte_to_char(cur.head));
                    next_char == ' ' || next_char == '\t' || next_char == '\n' || next_char == '\r'
                }
            });

            if should_pair && !tab.cursors.cursors().is_empty() {
                let tab_mut = &mut self.tabs[self.active_tab];
                let cursors_before = tab_mut.cursors.cursors().to_vec();

                let mut sorted_cursors = cursors_before.clone();
                sorted_cursors.sort_by_key(|cur| (cur.min(), cur.max()));

                let pair_str = format!("{}{}", c, close_char);
                let mut deltas: Vec<TextDelta> = Vec::new();
                let mut new_cursors: Vec<Cursor> = Vec::new();
                let mut delta_offset: isize = 0;

                for cur in sorted_cursors {
                    let start = (cur.min() as isize + delta_offset).max(0) as usize;
                    let ins_delta = tab_mut.buffer.insert(start, &pair_str)?;
                    // Cursor placed in the middle of pair
                    let new_pos = start + c.len_utf8();
                    new_cursors.push(Cursor::new(new_pos));

                    deltas.push(TextDelta::new(start, String::new(), ins_delta.new_text));
                    delta_offset += pair_str.len() as isize;
                }

                tab_mut.cursors.set_cursors(new_cursors);
                let cursors_after = tab_mut.cursors.cursors().to_vec();

                tab_mut
                    .history
                    .record_edits(deltas.clone(), cursors_before, cursors_after);
                self.record_edit_internal(&deltas);
                return Ok(());
            }
        }

        // Standard character insertion fallback
        let mut text = [0u8; 4];
        let s = c.encode_utf8(&mut text);
        self.insert_str_with_coalescing(s, can_coalesce)
    }

    pub fn insert_str(&mut self, s: &str) -> Result<(), BufferError> {
        self.insert_str_with_coalescing(s, false)
    }

    fn insert_str_with_coalescing(
        &mut self,
        s: &str,
        can_coalesce: bool,
    ) -> Result<(), BufferError> {
        let tab = &mut self.tabs[self.active_tab];
        let cursors_before = tab.cursors.cursors().to_vec();

        let mut sorted_cursors = cursors_before.clone();
        sorted_cursors.sort_by_key(|c| (c.min(), c.max()));

        let mut deltas: Vec<TextDelta> = Vec::new();
        let mut new_cursors: Vec<Cursor> = Vec::new();
        let mut delta_offset: isize = 0;

        for cursor in sorted_cursors {
            let start = (cursor.min() as isize + delta_offset).max(0) as usize;
            let end = (cursor.max() as isize + delta_offset).max(0) as usize;

            let mut old_text = String::new();
            let del_len = if cursor.is_selection() && start < end && end <= tab.buffer.len_bytes() {
                let del_delta = tab.buffer.delete(start, end)?;
                let len = del_delta.old_text.len();
                old_text = del_delta.old_text;
                len
            } else {
                0
            };

            let ins_delta = tab.buffer.insert(start, s)?;
            let new_pos = start + s.len();
            new_cursors.push(Cursor::new(new_pos));

            deltas.push(TextDelta::new(start, old_text, ins_delta.new_text));
            delta_offset += s.len() as isize - del_len as isize;
        }

        tab.cursors.set_cursors(new_cursors);
        let cursors_after = tab.cursors.cursors().to_vec();

        if deltas.len() == 1
            && cursors_before.len() == 1
            && cursors_after.len() == 1
            && deltas[0].old_text.is_empty()
        {
            tab.history.record_coalesced_insert(
                deltas[0].clone(),
                cursors_before,
                cursors_after,
                can_coalesce,
            );
        } else {
            tab.history
                .record_edits(deltas.clone(), cursors_before, cursors_after);
        }
        self.record_edit_internal(&deltas);
        Ok(())
    }

    pub fn insert_newline(&mut self) -> Result<(), BufferError> {
        if !self.config.smart_indent {
            let nl = self.tabs[self.active_tab].line_ending.as_str();
            return self.insert_str(nl);
        }

        let tab = &mut self.tabs[self.active_tab];
        let nl = tab.line_ending.as_str();
        let cursors_before = tab.cursors.cursors().to_vec();

        let mut sorted_cursors = cursors_before.clone();
        sorted_cursors.sort_by_key(|c| (c.min(), c.max()));

        // Pre-calculate newline and indentation string for each cursor before buffer mutations
        let mut insert_strings: Vec<String> = Vec::with_capacity(sorted_cursors.len());
        for cursor in &sorted_cursors {
            let target_byte = cursor.min();
            let line = tab.buffer.byte_to_line(target_byte);
            let line_start_char = tab.buffer.rope().line_to_char(line);
            let line_start_byte = tab.buffer.char_to_byte(line_start_char);

            let prefix = tab.buffer.slice_to_cow(line_start_byte, target_byte);
            let leading: String = prefix
                .chars()
                .take_while(|&c| c == ' ' || c == '\t')
                .collect();

            let trimmed = prefix.trim_end();
            let extra_indent = if trimmed.ends_with('{')
                || trimmed.ends_with('(')
                || trimmed.ends_with('[')
                || (trimmed.ends_with(':') && !trimmed.ends_with("::"))
            {
                " ".repeat(self.config.tab_size)
            } else {
                String::new()
            };

            insert_strings.push(format!("{}{}{}", nl, leading, extra_indent));
        }

        let mut deltas: Vec<TextDelta> = Vec::new();
        let mut new_cursors: Vec<Cursor> = Vec::new();
        let mut delta_offset: isize = 0;

        for (cursor, s) in sorted_cursors.into_iter().zip(insert_strings) {
            let start = (cursor.min() as isize + delta_offset).max(0) as usize;
            let end = (cursor.max() as isize + delta_offset).max(0) as usize;

            let mut old_text = String::new();
            let del_len = if cursor.is_selection() && start < end && end <= tab.buffer.len_bytes() {
                let del_delta = tab.buffer.delete(start, end)?;
                let len = del_delta.old_text.len();
                old_text = del_delta.old_text;
                len
            } else {
                0
            };

            let ins_delta = tab.buffer.insert(start, &s)?;
            let new_pos = start + s.len();
            new_cursors.push(Cursor::new(new_pos));

            deltas.push(TextDelta::new(start, old_text, ins_delta.new_text));
            delta_offset += s.len() as isize - del_len as isize;
        }

        tab.cursors.set_cursors(new_cursors);
        let cursors_after = tab.cursors.cursors().to_vec();

        tab.history
            .record_edits(deltas.clone(), cursors_before, cursors_after);
        self.record_edit_internal(&deltas);
        Ok(())
    }

    fn rollback_deltas(buffer: &mut crate::buffer::RopeBuffer, deltas: Vec<TextDelta>) {
        for delta in deltas.into_iter().rev() {
            if !delta.new_text.is_empty() {
                let _ = buffer.delete(delta.start_byte, delta.start_byte + delta.new_text.len());
            }
            if !delta.old_text.is_empty() {
                let _ = buffer.insert(delta.start_byte, &delta.old_text);
            }
        }
    }

    fn delete_range_or_rollback(
        buffer: &mut crate::buffer::RopeBuffer,
        deltas: &mut Vec<TextDelta>,
        start: usize,
        end: usize,
    ) -> Result<usize, BufferError> {
        match buffer.delete(start, end) {
            Ok(delta) => {
                deltas.push(delta);
                Ok(end - start)
            }
            Err(e) => {
                let to_rollback = std::mem::take(deltas);
                Self::rollback_deltas(buffer, to_rollback);
                Err(e)
            }
        }
    }

    fn finish_delete_batch(
        &mut self,
        deltas: Vec<TextDelta>,
        new_cursors: Vec<Cursor>,
        cursors_before: Vec<Cursor>,
    ) {
        if !deltas.is_empty() {
            let tab = &mut self.tabs[self.active_tab];
            tab.cursors.set_cursors(new_cursors);
            let cursors_after = tab.cursors.cursors().to_vec();
            tab.history
                .record_edits(deltas.clone(), cursors_before, cursors_after);
            self.record_edit_internal(&deltas);
        }
    }

    fn delete_selection_or_rollback(
        buffer: &mut crate::buffer::RopeBuffer,
        deltas: &mut Vec<TextDelta>,
        new_cursors: &mut Vec<Cursor>,
        delta_offset: &mut isize,
        start: usize,
        end: usize,
    ) -> Result<(), BufferError> {
        let len = buffer.len_bytes();
        if end > len {
            let to_rollback = std::mem::take(deltas);
            Self::rollback_deltas(buffer, to_rollback);
            return Err(BufferError::OutOfBounds(end, len));
        }
        if start < end {
            let removed = Self::delete_range_or_rollback(buffer, deltas, start, end)?;
            *delta_offset -= removed as isize;
        }
        new_cursors.push(Cursor::new(start));
        Ok(())
    }

    pub fn delete_backwards(&mut self) -> Result<(), BufferError> {
        let tab = &mut self.tabs[self.active_tab];
        let cursors_before = tab.cursors.cursors().to_vec();

        let mut sorted_cursors = cursors_before.clone();
        sorted_cursors.sort_by_key(|c| (c.min(), c.max()));

        let mut deltas: Vec<TextDelta> = Vec::new();
        let mut new_cursors: Vec<Cursor> = Vec::new();
        let mut delta_offset: isize = 0;

        let tab_size = self.config.tab_size.max(1);
        let smart_unindent = self.config.smart_unindent;

        for cursor in sorted_cursors {
            let start = (cursor.min() as isize + delta_offset).max(0) as usize;
            let end = (cursor.max() as isize + delta_offset).max(0) as usize;

            if cursor.is_selection() {
                Self::delete_selection_or_rollback(
                    &mut tab.buffer,
                    &mut deltas,
                    &mut new_cursors,
                    &mut delta_offset,
                    start,
                    end,
                )?;
            } else if start > tab.buffer.len_bytes() {
                let len = tab.buffer.len_bytes();
                Self::rollback_deltas(&mut tab.buffer, deltas);
                return Err(BufferError::OutOfBounds(start, len));
            } else if start > 0 {
                let line = tab.buffer.byte_to_line(start);
                let line_start_char = tab.buffer.rope().line_to_char(line);
                let line_start_byte = tab.buffer.char_to_byte(line_start_char);
                let cursor_char = tab.buffer.byte_to_char(start);
                let col = cursor_char.saturating_sub(line_start_char);

                let prefix = tab.buffer.slice_to_cow(line_start_byte, start);
                let is_all_spaces = !prefix.is_empty() && prefix.chars().all(|c| c == ' ');
                let can_unindent =
                    smart_unindent && is_all_spaces && (col > 0 && col.is_multiple_of(tab_size));

                let delete_from_byte = if can_unindent {
                    tab.buffer.char_to_byte(cursor_char - tab_size)
                } else {
                    let prev_char = if cursor_char >= 2
                        && tab.buffer.rope().char(cursor_char - 1) == '\n'
                        && tab.buffer.rope().char(cursor_char - 2) == '\r'
                    {
                        cursor_char - 2
                    } else {
                        cursor_char - 1
                    };
                    tab.buffer.char_to_byte(prev_char)
                };

                let removed = Self::delete_range_or_rollback(
                    &mut tab.buffer,
                    &mut deltas,
                    delete_from_byte,
                    start,
                )?;
                new_cursors.push(Cursor::new(delete_from_byte));
                delta_offset -= removed as isize;
            } else {
                new_cursors.push(Cursor::new(start));
            }
        }

        self.finish_delete_batch(deltas, new_cursors, cursors_before);
        Ok(())
    }

    pub fn delete_forwards(&mut self) -> Result<(), BufferError> {
        let tab = &mut self.tabs[self.active_tab];
        let cursors_before = tab.cursors.cursors().to_vec();

        let mut sorted_cursors = cursors_before.clone();
        sorted_cursors.sort_by_key(|c| (c.min(), c.max()));

        let mut deltas: Vec<TextDelta> = Vec::new();
        let mut new_cursors: Vec<Cursor> = Vec::new();
        let mut delta_offset: isize = 0;

        for cursor in sorted_cursors {
            let start = (cursor.min() as isize + delta_offset).max(0) as usize;
            let end = (cursor.max() as isize + delta_offset).max(0) as usize;

            if cursor.is_selection() {
                Self::delete_selection_or_rollback(
                    &mut tab.buffer,
                    &mut deltas,
                    &mut new_cursors,
                    &mut delta_offset,
                    start,
                    end,
                )?;
            } else if start > tab.buffer.len_bytes() {
                let len = tab.buffer.len_bytes();
                Self::rollback_deltas(&mut tab.buffer, deltas);
                return Err(BufferError::OutOfBounds(start, len));
            } else if start < tab.buffer.len_bytes() {
                let char_idx = tab.buffer.byte_to_char(start);
                let total_chars = tab.buffer.len_chars();
                let next_char = if char_idx + 1 < total_chars
                    && tab.buffer.rope().char(char_idx) == '\r'
                    && tab.buffer.rope().char(char_idx + 1) == '\n'
                {
                    char_idx + 2
                } else {
                    char_idx + 1
                };
                let next_byte = tab.buffer.char_to_byte(next_char);
                let removed =
                    Self::delete_range_or_rollback(&mut tab.buffer, &mut deltas, start, next_byte)?;
                new_cursors.push(Cursor::new(start));
                delta_offset -= removed as isize;
            } else {
                new_cursors.push(Cursor::new(start));
            }
        }

        self.finish_delete_batch(deltas, new_cursors, cursors_before);
        Ok(())
    }
}
