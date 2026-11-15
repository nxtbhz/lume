use std::sync::atomic::Ordering;

use crate::buffer::{BufferError, TextBuffer, TextDelta};
use crate::cursor::Cursor;
use crate::editor::Editor;
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
        let cursors_len = tab.cursors.cursors().len();

        // Check if overtype applies to all cursors (cursor head right before matching closing char)
        let can_overtype = is_closing_pair(c)
            && cursors_len > 0
            && tab.cursors.cursors().iter().all(|cur| {
                !cur.is_selection()
                    && cur.head < tab.buffer.len_bytes()
                    && Self::ensure_char_boundary(&tab.buffer, cur.head)
                        .is_ok_and(|ch| tab.buffer.rope().char(ch) == c)
            });

        if can_overtype {
            let tab_mut = &mut self.tabs[self.active_tab];
            let primary_before = tab_mut.cursors.primary_idx();
            let new_cursors = tab_mut
                .cursors
                .cursors()
                .iter()
                .map(|cur| Cursor::new(cur.head + c.len_utf8()))
                .collect();
            tab_mut
                .cursors
                .set_cursors_with_primary(new_cursors, primary_before);
            tab_mut.history.break_group();
            return Ok(());
        }

        // Check if auto-pairing applies for opening delimiter
        if let Some(close_char) = matching_closing_pair(c) {
            let should_pair = cursors_len > 0
                && tab.cursors.cursors().iter().all(|cur| {
                    if cur.is_selection() {
                        false
                    } else if cur.head >= tab.buffer.len_bytes() {
                        cur.head == tab.buffer.len_bytes()
                    } else {
                        Self::ensure_char_boundary(&tab.buffer, cur.head).is_ok_and(|ch| {
                            matches!(tab.buffer.rope().char(ch), ' ' | '\t' | '\n' | '\r')
                        })
                    }
                });

            if should_pair {
                let tab_mut = &mut self.tabs[self.active_tab];
                let cursors_before = tab_mut.cursors.cursors().to_vec();
                let primary_before = tab_mut.cursors.primary_idx();
                let (sorted_cursors, sorted_primary) =
                    Self::sorted_cursors_with_primary(&cursors_before, primary_before);
                Self::validate_cursor_boundaries(&tab_mut.buffer, &sorted_cursors)?;

                let pair_str = format!("{}{}", c, close_char);
                let mut deltas: Vec<TextDelta> = Vec::new();
                let mut new_cursors: Vec<Cursor> = Vec::with_capacity(sorted_cursors.len());
                let mut delta_offset: isize = 0;

                for cur in sorted_cursors {
                    let start = (cur.min() as isize + delta_offset).max(0) as usize;
                    let (ins_len, _) = Self::replace_range_or_rollback(
                        &mut tab_mut.buffer,
                        &mut deltas,
                        start,
                        start,
                        false,
                        &pair_str,
                    )?;
                    // Cursor placed in the middle of pair
                    new_cursors.push(Cursor::new(start + c.len_utf8()));
                    delta_offset += ins_len as isize;
                }

                self.commit_edit_batch(
                    deltas,
                    new_cursors,
                    sorted_primary,
                    cursors_before,
                    primary_before,
                );
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
        let primary_before = tab.cursors.primary_idx();
        let (sorted_cursors, sorted_primary) =
            Self::sorted_cursors_with_primary(&cursors_before, primary_before);
        Self::validate_start_boundaries(&tab.buffer, &sorted_cursors)?;

        let mut deltas: Vec<TextDelta> = Vec::new();
        let mut new_cursors: Vec<Cursor> = Vec::with_capacity(sorted_cursors.len());
        let mut delta_offset: isize = 0;

        for cursor in sorted_cursors {
            let start = (cursor.min() as isize + delta_offset).max(0) as usize;
            let end = (cursor.max() as isize + delta_offset).max(0) as usize;

            let (ins_len, del_len) = Self::replace_range_or_rollback(
                &mut tab.buffer,
                &mut deltas,
                start,
                end,
                cursor.is_selection(),
                s,
            )?;
            new_cursors.push(Cursor::new(start + ins_len));
            delta_offset += ins_len as isize - del_len as isize;
        }

        tab.cursors
            .set_cursors_with_primary(new_cursors, sorted_primary);
        let cursors_after = tab.cursors.cursors().to_vec();
        let primary_after = tab.cursors.primary_idx();

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
            tab.history.record_edits_with_primary(
                deltas.clone(),
                cursors_before,
                primary_before,
                cursors_after,
                primary_after,
            );
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
        let primary_before = tab.cursors.primary_idx();
        let (sorted_cursors, sorted_primary) =
            Self::sorted_cursors_with_primary(&cursors_before, primary_before);
        Self::validate_start_boundaries(&tab.buffer, &sorted_cursors)?;

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
        let mut new_cursors: Vec<Cursor> = Vec::with_capacity(sorted_cursors.len());
        let mut delta_offset: isize = 0;

        for (cursor, s) in sorted_cursors.into_iter().zip(insert_strings) {
            let start = (cursor.min() as isize + delta_offset).max(0) as usize;
            let end = (cursor.max() as isize + delta_offset).max(0) as usize;

            let (ins_len, del_len) = Self::replace_range_or_rollback(
                &mut tab.buffer,
                &mut deltas,
                start,
                end,
                cursor.is_selection(),
                &s,
            )?;
            new_cursors.push(Cursor::new(start + ins_len));
            delta_offset += ins_len as isize - del_len as isize;
        }

        self.commit_edit_batch(
            deltas,
            new_cursors,
            sorted_primary,
            cursors_before,
            primary_before,
        );
        Ok(())
    }

    pub(crate) fn sorted_cursors_with_primary(
        cursors: &[Cursor],
        primary_idx: usize,
    ) -> (Vec<Cursor>, usize) {
        if cursors.len() <= 1 {
            return (cursors.to_vec(), 0);
        }
        let orig_primary = primary_idx.min(cursors.len() - 1);
        let mut tagged: Vec<(usize, Cursor)> = cursors.iter().copied().enumerate().collect();
        tagged.sort_by_key(|(orig_idx, c)| (c.min(), c.max(), *orig_idx));
        let mut new_primary = 0;
        let sorted = tagged
            .into_iter()
            .enumerate()
            .map(|(new_idx, (orig_idx, c))| {
                if orig_idx == orig_primary {
                    new_primary = new_idx;
                }
                c
            })
            .collect();
        (sorted, new_primary)
    }

    pub(crate) fn ensure_char_boundary(
        buffer: &crate::buffer::RopeBuffer,
        byte_idx: usize,
    ) -> Result<usize, BufferError> {
        let len = buffer.len_bytes();
        if byte_idx > len {
            return Err(BufferError::OutOfBounds(byte_idx, len));
        }
        let char_idx = buffer.try_byte_to_char(byte_idx)?;
        if buffer.try_char_to_byte(char_idx)? != byte_idx {
            return Err(BufferError::NotCharBoundary(byte_idx));
        }
        Ok(char_idx)
    }

    pub(crate) fn validate_start_boundaries(
        buffer: &crate::buffer::RopeBuffer,
        cursors: &[Cursor],
    ) -> Result<(), BufferError> {
        for cur in cursors {
            Self::ensure_char_boundary(buffer, cur.min())?;
        }
        Ok(())
    }

    pub(crate) fn validate_cursor_boundaries(
        buffer: &crate::buffer::RopeBuffer,
        cursors: &[Cursor],
    ) -> Result<(), BufferError> {
        for cur in cursors {
            Self::ensure_char_boundary(buffer, cur.min())?;
            if cur.is_selection() {
                Self::ensure_char_boundary(buffer, cur.max())?;
            }
        }
        Ok(())
    }

    fn ensure_char_boundary_or_rollback(
        buffer: &mut crate::buffer::RopeBuffer,
        deltas: &mut Vec<TextDelta>,
        byte_idx: usize,
    ) -> Result<usize, BufferError> {
        match Self::ensure_char_boundary(buffer, byte_idx) {
            Ok(char_idx) => Ok(char_idx),
            Err(e) => {
                let to_rollback = std::mem::take(deltas);
                Self::rollback_deltas(buffer, to_rollback);
                Err(e)
            }
        }
    }

    pub(crate) fn rollback_deltas(buffer: &mut crate::buffer::RopeBuffer, deltas: Vec<TextDelta>) {
        for delta in deltas.into_iter().rev() {
            if !delta.new_text.is_empty() {
                let _ = buffer.delete(delta.start_byte, delta.start_byte + delta.new_text.len());
            }
            if !delta.old_text.is_empty() {
                let _ = buffer.insert(delta.start_byte, &delta.old_text);
            }
        }
    }

    fn replace_range_or_rollback(
        buffer: &mut crate::buffer::RopeBuffer,
        deltas: &mut Vec<TextDelta>,
        start: usize,
        end: usize,
        is_selection: bool,
        text: &str,
    ) -> Result<(usize, usize), BufferError> {
        let mut old_text = String::new();
        let del_len = if is_selection && start < end && end <= buffer.len_bytes() {
            let del_delta = match buffer.delete(start, end) {
                Ok(delta) => delta,
                Err(e) => {
                    let to_rollback = std::mem::take(deltas);
                    Self::rollback_deltas(buffer, to_rollback);
                    return Err(e);
                }
            };
            let removed = del_delta.old_text.len();
            old_text = del_delta.old_text;
            removed
        } else {
            0
        };

        match buffer.insert(start, text) {
            Ok(ins_delta) => {
                let ins_len = ins_delta.new_text.len();
                deltas.push(TextDelta::new(start, old_text, ins_delta.new_text));
                Ok((ins_len, del_len))
            }
            Err(e) => {
                if !old_text.is_empty() {
                    let _ = buffer.insert(start, &old_text);
                }
                let to_rollback = std::mem::take(deltas);
                Self::rollback_deltas(buffer, to_rollback);
                Err(e)
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

    fn commit_edit_batch(
        &mut self,
        deltas: Vec<TextDelta>,
        new_cursors: Vec<Cursor>,
        sorted_primary: usize,
        cursors_before: Vec<Cursor>,
        primary_before: usize,
    ) {
        let tab = &mut self.tabs[self.active_tab];
        tab.cursors
            .set_cursors_with_primary(new_cursors, sorted_primary);
        let cursors_after = tab.cursors.cursors().to_vec();
        let primary_after = tab.cursors.primary_idx();
        tab.history.record_edits_with_primary(
            deltas.clone(),
            cursors_before,
            primary_before,
            cursors_after,
            primary_after,
        );
        self.record_edit_internal(&deltas);
    }

    fn finish_delete_batch(
        &mut self,
        deltas: Vec<TextDelta>,
        new_cursors: Vec<Cursor>,
        sorted_primary: usize,
        cursors_before: Vec<Cursor>,
        primary_before: usize,
    ) {
        if !deltas.is_empty() {
            self.commit_edit_batch(
                deltas,
                new_cursors,
                sorted_primary,
                cursors_before,
                primary_before,
            );
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
        let primary_before = tab.cursors.primary_idx();
        let (sorted_cursors, sorted_primary) =
            Self::sorted_cursors_with_primary(&cursors_before, primary_before);
        Self::validate_cursor_boundaries(&tab.buffer, &sorted_cursors)?;

        let mut deltas: Vec<TextDelta> = Vec::new();
        let mut new_cursors: Vec<Cursor> = Vec::with_capacity(sorted_cursors.len());
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
            } else if start > 0 {
                let cursor_char =
                    Self::ensure_char_boundary_or_rollback(&mut tab.buffer, &mut deltas, start)?;
                let line = tab.buffer.byte_to_line(start);
                let line_start_char = tab.buffer.rope().line_to_char(line);
                let line_start_byte = tab.buffer.char_to_byte(line_start_char);
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

        self.finish_delete_batch(
            deltas,
            new_cursors,
            sorted_primary,
            cursors_before,
            primary_before,
        );
        Ok(())
    }

    pub fn delete_forwards(&mut self) -> Result<(), BufferError> {
        let tab = &mut self.tabs[self.active_tab];
        let cursors_before = tab.cursors.cursors().to_vec();
        let primary_before = tab.cursors.primary_idx();
        let (sorted_cursors, sorted_primary) =
            Self::sorted_cursors_with_primary(&cursors_before, primary_before);
        Self::validate_cursor_boundaries(&tab.buffer, &sorted_cursors)?;

        let mut deltas: Vec<TextDelta> = Vec::new();
        let mut new_cursors: Vec<Cursor> = Vec::with_capacity(sorted_cursors.len());
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
            } else if start < tab.buffer.len_bytes() {
                let char_idx =
                    Self::ensure_char_boundary_or_rollback(&mut tab.buffer, &mut deltas, start)?;
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

        self.finish_delete_batch(
            deltas,
            new_cursors,
            sorted_primary,
            cursors_before,
            primary_before,
        );
        Ok(())
    }
}
