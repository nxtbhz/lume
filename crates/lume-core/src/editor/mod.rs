pub mod config;
pub mod save;
pub mod tab;

pub use config::EditorConfig;
pub use tab::{AsyncSaveEvent, Tab, TabSyncState};

use crate::autosave::AutoSaveController;
use crate::buffer::{BufferError, RopeBuffer, TextBuffer, TextDelta};
use crate::cursor::{Cursor, CursorSet};
use crate::history::HistoryManager;
use crate::journal::Journal;
use crossbeam_channel::{Receiver, Sender};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

#[derive(Clone, Debug)]
pub struct Editor {
    pub(crate) tabs: Vec<Tab>,
    pub(crate) active_tab: usize,
    pub(crate) next_tab_id: usize,
    pub(crate) autosave: AutoSaveController,
    pub(crate) config: EditorConfig,
    pub(crate) save_channel: Option<(Sender<AsyncSaveEvent>, Receiver<AsyncSaveEvent>)>,
}
impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

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
    pub fn new() -> Self {
        Self {
            tabs: vec![Tab::new(1)],
            active_tab: 0,
            next_tab_id: 2,
            autosave: AutoSaveController::default(),
            config: EditorConfig::default(),
            save_channel: None,
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(text: &str) -> Self {
        Self {
            tabs: vec![Tab::from_str(1, text)],
            active_tab: 0,
            next_tab_id: 2,
            autosave: AutoSaveController::default(),
            config: EditorConfig::default(),
            save_channel: None,
        }
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let mut ed = Self::new();
        ed.open_in_current_tab(path)?;
        Ok(ed)
    }

    pub fn open_in_current_tab<P: AsRef<Path>>(&mut self, path: P) -> Result<(), std::io::Error> {
        let path = path.as_ref();

        // Check if crash recovery journal exists for this file
        let (buffer, recovered) = if let Ok(Some(recovered_text)) = Journal::recover(path) {
            (RopeBuffer::from_str(&recovered_text), true)
        } else {
            let file = std::fs::File::open(path)?;
            (RopeBuffer::from_reader(file)?, false)
        };

        let tab = &mut self.tabs[self.active_tab];
        tab.buffer = buffer;
        tab.cursors.set_single(0);
        tab.history.clear();
        tab.file_path = Some(path.to_path_buf());
        tab.is_dirty = recovered;
        tab.recovered_from_journal = recovered;
        if let Some(ref sync) = tab.sync_state {
            sync.is_dirty.store(recovered, Ordering::SeqCst);
        }

        if recovered {
            self.autosave.notify_change();
        }

        Ok(())
    }

    // Tabs management
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn tabs_mut(&mut self) -> &mut [Tab] {
        &mut self.tabs
    }

    pub fn active_tab_idx(&self) -> usize {
        self.active_tab
    }

    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    pub fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_tab]
    }

    pub fn new_tab(&mut self) -> usize {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        self.tabs.push(Tab::new(id));
        self.active_tab = self.tabs.len() - 1;
        self.active_tab
    }

    pub fn open_tab<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error> {
        let path = path.as_ref();
        // If already open, switch to it
        for (i, tab) in self.tabs.iter().enumerate() {
            if let Some(ref p) = tab.file_path {
                if p == path {
                    self.active_tab = i;
                    return Ok(i);
                }
            }
        }

        let idx = self.new_tab();
        self.open_in_current_tab(path)?;
        Ok(idx)
    }

    pub fn open_or_load<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error> {
        let path = path.as_ref();
        // If already open, switch to it
        for (i, tab) in self.tabs.iter().enumerate() {
            if let Some(ref p) = tab.file_path {
                if p == path {
                    self.active_tab = i;
                    return Ok(i);
                }
            }
        }

        // If current tab is untitled, clean, and empty, reuse it
        let current_tab = &self.tabs[self.active_tab];
        if current_tab.file_path.is_none()
            && !current_tab.is_dirty
            && current_tab.buffer.len_bytes() == 0
        {
            self.open_in_current_tab(path)?;
            return Ok(self.active_tab);
        }

        let idx = self.new_tab();
        self.open_in_current_tab(path)?;
        Ok(idx)
    }

    pub fn close_tab(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }

        if !self.tabs[index].is_dirty {
            if let Some(ref p) = self.tabs[index].file_path {
                let _ = Journal::remove(p);
            }
        }

        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.tabs.push(Tab::new(self.next_tab_id));
            self.next_tab_id += 1;
            self.active_tab = 0;
        } else if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        true
    }

    pub fn switch_tab(&mut self, index: usize) -> bool {
        if index < self.tabs.len() {
            self.active_tab = index;
            true
        } else {
            false
        }
    }

    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
        }
    }

    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab = if self.active_tab == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab - 1
            };
        }
    }

    // Active buffer delegation
    pub fn buffer(&self) -> &RopeBuffer {
        &self.tabs[self.active_tab].buffer
    }

    pub fn buffer_mut(&mut self) -> &mut RopeBuffer {
        &mut self.tabs[self.active_tab].buffer
    }

    pub fn cursors(&self) -> &CursorSet {
        &self.tabs[self.active_tab].cursors
    }

    pub fn cursors_mut(&mut self) -> &mut CursorSet {
        &mut self.tabs[self.active_tab].cursors
    }

    pub fn file_path(&self) -> Option<&Path> {
        self.tabs[self.active_tab].file_path.as_deref()
    }

    pub fn set_file_path(&mut self, path: Option<PathBuf>) {
        self.tabs[self.active_tab].file_path = path;
    }

    pub fn language_override(&self) -> Option<&str> {
        self.tabs[self.active_tab].language_override.as_deref()
    }

    pub fn set_language_override(&mut self, lang: Option<String>) {
        self.tabs[self.active_tab].language_override = lang;
    }

    pub fn is_dirty(&self) -> bool {
        let tab = &self.tabs[self.active_tab];
        if let Some(ref sync) = tab.sync_state {
            tab.is_dirty && sync.is_dirty.load(Ordering::SeqCst)
        } else {
            tab.is_dirty
        }
    }

    pub fn find_next(&mut self, query: &str, case_sensitive: bool) -> Option<(usize, usize)> {
        if query.is_empty() {
            return None;
        }
        let tab = &mut self.tabs[self.active_tab];
        let from_byte = if tab.cursors.primary().is_selection() {
            tab.cursors.primary().max()
        } else {
            tab.cursors.primary().head
        };
        if let Some((start, end)) =
            crate::search::find_next(&tab.buffer, query, from_byte, case_sensitive)
        {
            let primary = tab.cursors.primary_mut();
            primary.anchor = start;
            primary.head = end;
            Some((start, end))
        } else {
            None
        }
    }

    pub fn find_prev(&mut self, query: &str, case_sensitive: bool) -> Option<(usize, usize)> {
        if query.is_empty() {
            return None;
        }
        let tab = &mut self.tabs[self.active_tab];
        let from_byte = if tab.cursors.primary().is_selection() {
            tab.cursors.primary().min()
        } else {
            tab.cursors.primary().head
        };
        if let Some((start, end)) =
            crate::search::find_prev(&tab.buffer, query, from_byte, case_sensitive)
        {
            let primary = tab.cursors.primary_mut();
            primary.anchor = start;
            primary.head = end;
            Some((start, end))
        } else {
            None
        }
    }

    pub fn replace_current(
        &mut self,
        query: &str,
        replacement: &str,
        case_sensitive: bool,
    ) -> bool {
        if query.is_empty() {
            return false;
        }

        let tab = &self.tabs[self.active_tab];
        let primary = *tab.cursors.primary();
        let sel_min = primary.min();
        let sel_max = primary.max();

        let is_match_selected = if primary.is_selection() {
            let selected_text = tab.buffer.slice_to_cow(sel_min, sel_max);
            if case_sensitive {
                selected_text == query
            } else {
                selected_text.eq_ignore_ascii_case(query)
                    || selected_text.to_lowercase() == query.to_lowercase()
            }
        } else {
            false
        };

        if !is_match_selected {
            // Preview: select next match if available, without modifying buffer
            return self.find_next(query, case_sensitive).is_some() && false;
        }

        let target_start = sel_min;
        let target_end = sel_max;

        let tab = &mut self.tabs[self.active_tab];
        let cursors_before = tab.cursors.cursors().to_vec();

        let old_text = match tab.buffer.delete(target_start, target_end) {
            Ok(delta) => delta.old_text,
            Err(_) => return false,
        };

        if tab.buffer.insert(target_start, replacement).is_err() {
            return false;
        }

        let new_pos = target_start + replacement.len();
        tab.cursors.set_single(new_pos);

        let delta = TextDelta::new(target_start, old_text, replacement);
        let deltas = vec![delta];
        let cursors_after = tab.cursors.cursors().to_vec();

        tab.history
            .record_edits(deltas.clone(), cursors_before, cursors_after);
        self.record_edit_internal(&deltas);

        // Advance to next match after this replacement
        let _ = self.find_next(query, case_sensitive);

        true
    }

    pub fn replace_all(&mut self, query: &str, replacement: &str, case_sensitive: bool) -> usize {
        if query.is_empty() {
            return 0;
        }

        let tab = &self.tabs[self.active_tab];
        let total_bytes = tab.buffer.len_bytes();
        if total_bytes == 0 {
            return 0;
        }

        let matches = crate::search::find_matches_in_range(
            &tab.buffer,
            query,
            0,
            total_bytes,
            case_sensitive,
        );
        if matches.is_empty() {
            return 0;
        }

        let count = matches.len();
        let tab = &mut self.tabs[self.active_tab];
        let cursors_before = tab.cursors.cursors().to_vec();

        let mut deltas = Vec::with_capacity(count);
        let mut delta_offset: isize = 0;
        let mut last_end = 0;

        for (orig_start, orig_end) in matches {
            let start = (orig_start as isize + delta_offset) as usize;
            let end = (orig_end as isize + delta_offset) as usize;

            if let Ok(del_delta) = tab.buffer.delete(start, end) {
                if let Ok(ins_delta) = tab.buffer.insert(start, replacement) {
                    last_end = start + ins_delta.new_text.len();
                    deltas.push(TextDelta::new(
                        start,
                        del_delta.old_text,
                        ins_delta.new_text,
                    ));
                    delta_offset += replacement.len() as isize - (orig_end - orig_start) as isize;
                }
            }
        }

        if deltas.is_empty() {
            return 0;
        }

        tab.cursors.set_single(last_end);
        let cursors_after = tab.cursors.cursors().to_vec();

        tab.history
            .record_edits(deltas.clone(), cursors_before, cursors_after);
        self.record_edit_internal(&deltas);

        count
    }

    // AutoSave check
    pub fn autosave_controller(&self) -> &AutoSaveController {
        &self.autosave
    }

    pub fn autosave_controller_mut(&mut self) -> &mut AutoSaveController {
        &mut self.autosave
    }

    pub fn config(&self) -> &EditorConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut EditorConfig {
        &mut self.config
    }

    fn record_edit_internal(&mut self, deltas: &[TextDelta]) {
        let tab = &mut self.tabs[self.active_tab];
        tab.is_dirty = true;
        tab.revision += 1;
        let sync_clone = tab.sync_state.clone();
        if let Some(ref sync) = sync_clone {
            sync.revision.store(tab.revision, Ordering::SeqCst);
            sync.is_dirty.store(true, Ordering::SeqCst);
        }
        self.autosave.notify_change();

        // Write append-only binary deltas to crash recovery journal if file path is known
        if let Some(ref path) = self.tabs[self.active_tab].file_path {
            let _guard = sync_clone
                .as_ref()
                .map(|s| s.journal_lock.lock().unwrap_or_else(|e| e.into_inner()));
            if deltas.is_empty() {
                let text = self.tabs[self.active_tab]
                    .buffer
                    .slice_to_cow(0, self.tabs[self.active_tab].buffer.len_bytes());
                let _ = Journal::record(path, &text);
            } else {
                let _ = Journal::record_deltas(path, deltas);
            }
        }
    }

    pub fn revision(&self) -> u64 {
        self.tabs[self.active_tab].revision
    }

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

    pub fn insert_char(&mut self, c: char) -> Result<(), BufferError> {
        if !self.config.auto_pairs {
            let mut text = [0u8; 4];
            let s = c.encode_utf8(&mut text);
            return self.insert_str(s);
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
        self.insert_str(s)
    }

    pub fn insert_str(&mut self, s: &str) -> Result<(), BufferError> {
        let tab = &mut self.tabs[self.active_tab];
        let cursors_before = tab.cursors.cursors().to_vec();

        let mut sorted_cursors = cursors_before.clone();
        sorted_cursors.sort_by_key(|c| (c.min(), c.max()));

        let mut deltas: Vec<TextDelta> = Vec::new();
        let mut new_cursors: Vec<Cursor> = Vec::new();
        let mut delta_offset: isize = 0;

        for cursor in sorted_cursors {
            let start = (cursor.min() as isize + delta_offset) as usize;
            let end = (cursor.max() as isize + delta_offset) as usize;

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

        tab.history
            .record_edits(deltas.clone(), cursors_before, cursors_after);
        self.record_edit_internal(&deltas);
        Ok(())
    }

    pub fn insert_newline(&mut self) -> Result<(), BufferError> {
        if !self.config.smart_indent {
            return self.insert_str("\n");
        }

        let tab = &mut self.tabs[self.active_tab];
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

            insert_strings.push(format!("\n{}{}", leading, extra_indent));
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
                if start < end && end <= tab.buffer.len_bytes() {
                    let delta = tab.buffer.delete(start, end)?;
                    deltas.push(delta);
                    new_cursors.push(Cursor::new(start));
                    delta_offset -= (end - start) as isize;
                } else {
                    new_cursors.push(Cursor::new(start));
                }
            } else if start > 0 && start <= tab.buffer.len_bytes() {
                // Check for smart unindent condition
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
                    let target_char = cursor_char - tab_size;
                    tab.buffer.char_to_byte(target_char)
                } else {
                    let prev_char = tab.buffer.byte_to_char(start) - 1;
                    tab.buffer.char_to_byte(prev_char)
                };

                let delta = tab.buffer.delete(delete_from_byte, start)?;
                deltas.push(delta);
                new_cursors.push(Cursor::new(delete_from_byte));
                delta_offset -= (start - delete_from_byte) as isize;
            } else {
                new_cursors.push(Cursor::new(start));
            }
        }

        if !deltas.is_empty() {
            tab.cursors.set_cursors(new_cursors);
            let cursors_after = tab.cursors.cursors().to_vec();
            tab.history
                .record_edits(deltas.clone(), cursors_before, cursors_after);
            self.record_edit_internal(&deltas);
        }

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
            let start = (cursor.min() as isize + delta_offset) as usize;
            let end = (cursor.max() as isize + delta_offset) as usize;

            if cursor.is_selection() {
                if start < end && end <= tab.buffer.len_bytes() {
                    let delta = tab.buffer.delete(start, end)?;
                    deltas.push(delta);
                    new_cursors.push(Cursor::new(start));
                    delta_offset -= (end - start) as isize;
                } else {
                    new_cursors.push(Cursor::new(start));
                }
            } else if start < tab.buffer.len_bytes() {
                let char_idx = tab.buffer.byte_to_char(start);
                let next_byte = tab.buffer.char_to_byte(char_idx + 1);
                let delta = tab.buffer.delete(start, next_byte)?;
                deltas.push(delta);
                new_cursors.push(Cursor::new(start));
                delta_offset -= (next_byte - start) as isize;
            } else {
                new_cursors.push(Cursor::new(start));
            }
        }

        if !deltas.is_empty() {
            tab.cursors.set_cursors(new_cursors);
            let cursors_after = tab.cursors.cursors().to_vec();
            tab.history
                .record_edits(deltas.clone(), cursors_before, cursors_after);
            self.record_edit_internal(&deltas);
        }

        Ok(())
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

    pub fn undo(&mut self) -> bool {
        let tab = &mut self.tabs[self.active_tab];
        if let Some(cursors) = tab.history.undo(&mut tab.buffer) {
            tab.cursors.set_cursors(cursors);
            self.record_edit_internal(&[]);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        let tab = &mut self.tabs[self.active_tab];
        if let Some(cursors) = tab.history.redo(&mut tab.buffer) {
            tab.cursors.set_cursors(cursors);
            self.record_edit_internal(&[]);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_replace_current_and_undo() {
        let mut ed = Editor::from_str("foo bar foo baz");
        // Find and select first match 0..3
        ed.find_next("foo", true);
        assert_eq!(ed.cursors().primary().range(), 0..3);
        let replaced = ed.replace_current("foo", "qux", true);
        assert!(replaced);
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "qux bar foo baz"
        );
        // Cursor should have jumped to next match
        assert_eq!(ed.cursors().primary().range(), 8..11);

        ed.undo();
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "foo bar foo baz"
        );
    }

    #[test]
    fn test_editor_replace_current_preview_when_unselected() {
        let mut ed = Editor::from_str("foo bar foo baz");
        // Place cursor at offset 4 (on "bar", not selecting "foo")
        ed.cursors_mut().set_single(4);
        let replaced = ed.replace_current("foo", "qux", true);
        // Should NOT destructively replace yet; previews by selecting next match at 8..11
        assert!(!replaced);
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "foo bar foo baz"
        );
        assert_eq!(ed.cursors().primary().range(), 8..11);

        // Subsequent replace_current with match selected replaces it
        let replaced_now = ed.replace_current("foo", "qux", true);
        assert!(replaced_now);
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "foo bar qux baz"
        );
    }

    #[test]
    fn test_editor_replace_all_50_plus_atomic_and_multibyte_asymmetric() {
        let mut initial = String::new();
        for i in 0..60 {
            initial.push_str(&format!("item_ação_{} ", i));
        }
        let mut ed = Editor::from_str(&initial);
        let count = ed.replace_all("item_ação", "🚀_novo_item_longo", true);
        assert_eq!(count, 60);

        // Single atomic undo restores exact original string
        assert!(ed.undo());
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            initial
        );

        // Redo restores all 60
        assert!(ed.redo());
        assert!(!ed
            .buffer()
            .slice_to_cow(0, ed.buffer().len_bytes())
            .contains("item_ação"));
    }

    #[test]
    fn test_editor_replace_all_atomic_undo() {
        let mut ed = Editor::from_str("let x = alpha + alpha;\nlet y = alpha;");
        let count = ed.replace_all("alpha", "beta_val", true);
        assert_eq!(count, 3);
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "let x = beta_val + beta_val;\nlet y = beta_val;"
        );

        // Atomic undo: single undo call must revert all 3 replacements
        assert!(ed.undo());
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "let x = alpha + alpha;\nlet y = alpha;"
        );

        // Redo restores all 3 replacements
        assert!(ed.redo());
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "let x = beta_val + beta_val;\nlet y = beta_val;"
        );
    }

    #[test]
    fn test_editor_typing_and_navigation() {
        let mut ed = Editor::new();
        ed.insert_str("line 1\nline 2").unwrap();

        assert_eq!(ed.cursor_line_col(), (1, 6));

        ed.move_left(false);
        assert_eq!(ed.cursor_line_col(), (1, 5));

        ed.move_up(false);
        assert_eq!(ed.cursor_line_col(), (0, 5));

        ed.move_home(false);
        assert_eq!(ed.cursor_line_col(), (0, 0));

        ed.move_end(false);
        assert_eq!(ed.cursor_line_col(), (0, 6));

        ed.delete_backwards().unwrap();
        assert_eq!(ed.cursor_line_col(), (0, 5));
        assert!(ed
            .buffer()
            .slice_to_cow(0, ed.buffer().len_bytes())
            .starts_with("line \n"));
    }

    #[test]
    fn test_editor_undo_redo() {
        let mut ed = Editor::new();
        ed.insert_str("abc").unwrap();
        assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "abc");

        ed.undo();
        assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "");

        ed.redo();
        assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "abc");
    }

    #[test]
    fn test_editor_atomic_save_and_journal_cleanup() {
        let tmp_dir = std::env::temp_dir();
        let file_path = tmp_dir.join(format!("lume_test_save_{}.txt", std::process::id()));

        let mut ed = Editor::new();
        ed.set_file_path(Some(file_path.clone()));
        ed.insert_str("Atomic save content").unwrap();

        // While dirty, journal exists
        assert!(Journal::has_journal(&file_path));

        ed.save().unwrap();
        assert!(!ed.is_dirty());
        assert!(file_path.exists());
        // Clean save removes journal
        assert!(!Journal::has_journal(&file_path));

        let read_back = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(read_back, "Atomic save content");

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_editor_crash_recovery() {
        let tmp_dir = std::env::temp_dir();
        let file_path = tmp_dir.join(format!("lume_crash_test_{}.txt", std::process::id()));
        std::fs::write(&file_path, "original disk text").unwrap();

        // 1. User opens file and modifies it
        {
            let mut ed = Editor::open(&file_path).unwrap();
            ed.set_cursor_from_line_col(0, 18, false);
            ed.insert_str(" + unsaved edits").unwrap();
            // Journal records unsaved changes!
            assert!(Journal::has_journal(&file_path));
            // Simulate crash: ed is dropped without calling save()!
        }

        // 2. User re-opens file after crash
        let recovered_ed = Editor::open(&file_path).unwrap();
        assert!(recovered_ed.active_tab().recovered_from_journal);
        assert!(recovered_ed.is_dirty());
        let text = recovered_ed
            .buffer()
            .slice_to_cow(0, recovered_ed.buffer().len_bytes());
        assert_eq!(text, "original disk text + unsaved edits");

        // Clean up
        let _ = Journal::remove(&file_path);
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_editor_multi_tabs() {
        let mut ed = Editor::new();
        ed.insert_str("tab 1 content").unwrap();

        let tab2_idx = ed.new_tab();
        assert_eq!(tab2_idx, 1);
        ed.insert_str("tab 2 content").unwrap();

        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "tab 2 content"
        );

        ed.switch_tab(0);
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "tab 1 content"
        );

        ed.close_tab(0);
        assert_eq!(ed.tabs().len(), 1);
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "tab 2 content"
        );
    }

    #[test]
    fn test_editor_mouse_word_and_line_selection() {
        let mut ed = Editor::new();
        ed.insert_str("hello_world and goodbye\nsecond line")
            .unwrap();

        // Click inside "hello_world"
        ed.set_cursor_from_line_col(0, 4, false);
        ed.select_word_at_cursor();
        let primary = ed.cursors().primary();
        assert!(primary.is_selection());
        assert_eq!(primary.min(), 0);
        assert_eq!(primary.max(), 11);

        // Triple click on second line
        ed.set_cursor_from_line_col(1, 3, false);
        ed.select_line_at_cursor();
        let primary = ed.cursors().primary();
        assert!(primary.is_selection());
        assert_eq!(primary.min(), 24);
        assert_eq!(primary.max(), 35);
    }

    #[test]
    fn test_editor_overwrite_selection_single_undo() {
        let mut ed = Editor::new();
        ed.insert_str("Hello World").unwrap();

        // Select "World" (bytes 6..11)
        ed.cursors_mut().primary_mut().anchor = 6;
        ed.cursors_mut().primary_mut().head = 11;
        assert!(ed.cursors().primary().is_selection());

        // Overwrite selection with "Rust"
        ed.insert_str("Rust").unwrap();
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "Hello Rust"
        );

        // A SINGLE undo must restore "Hello World" and original selection!
        ed.undo();
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "Hello World"
        );
        let primary = ed.cursors().primary();
        assert_eq!(primary.min(), 6);
        assert_eq!(primary.max(), 11);

        // Redo restores "Hello Rust"
        ed.redo();
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "Hello Rust"
        );
    }

    #[test]
    fn test_editor_multi_cursor_simultaneous_editing_and_undo() {
        let mut ed = Editor::new();
        ed.insert_str("alpha beta gamma").unwrap();

        // Set 3 cursors at the end of each word: after 'alpha' (5), after 'beta' (10), after 'gamma' (16)
        ed.cursors_mut()
            .set_cursors(vec![Cursor::new(5), Cursor::new(10), Cursor::new(16)]);
        assert_eq!(ed.cursors().cursors().len(), 3);

        // Simultaneous insert "_test" at all 3 cursors
        ed.insert_str("_test").unwrap();
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "alpha_test beta_test gamma_test"
        );

        // All 3 cursors should still exist and be properly positioned
        assert_eq!(ed.cursors().cursors().len(), 3);
        assert_eq!(ed.cursors().cursors()[0].head, 10); // 5 + 5
        assert_eq!(ed.cursors().cursors()[1].head, 20); // 10 + 5 + 5
        assert_eq!(ed.cursors().cursors()[2].head, 31); // 16 + 5 + 5 + 5

        // Simultaneous delete backwards (delete 5 chars of "_test")
        for _ in 0..5 {
            ed.delete_backwards().unwrap();
        }
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "alpha beta gamma"
        );
        assert_eq!(ed.cursors().cursors().len(), 3);
        assert_eq!(ed.cursors().cursors()[0].head, 5);
        assert_eq!(ed.cursors().cursors()[1].head, 10);
        assert_eq!(ed.cursors().cursors()[2].head, 16);

        // Undo 5 deletions, then undo insert
        for _ in 0..5 {
            ed.undo();
        }
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "alpha_test beta_test gamma_test"
        );

        // One more undo reverses the multi-cursor insertion
        ed.undo();
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "alpha beta gamma"
        );
        assert_eq!(ed.cursors().cursors().len(), 3);
        assert_eq!(ed.cursors().cursors()[0].head, 5);
        assert_eq!(ed.cursors().cursors()[1].head, 10);
        assert_eq!(ed.cursors().cursors()[2].head, 16);
    }

    #[test]
    fn test_editor_async_autosave() {
        let tmp_dir = std::env::temp_dir();
        let file_path = tmp_dir.join(format!("lume_test_autosave_{}.txt", std::process::id()));

        let mut ed = Editor::new();
        ed.set_file_path(Some(file_path.clone()));
        ed.autosave_controller_mut().config.debounce = std::time::Duration::from_millis(0);

        ed.insert_str("async autosave content").unwrap();
        assert!(ed.is_dirty());

        let triggered = ed.check_autosave();
        assert!(triggered);

        for _ in 0..50 {
            let _ = ed.poll_async_saves();
            if !ed.is_dirty() && file_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&file_path) {
                    if content == "async autosave content" {
                        break;
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(!ed.is_dirty());
        assert!(file_path.exists());
        let read_back = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(read_back, "async autosave content");

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_async_save_preserves_journal_for_newer_edits() {
        let tmp_dir = std::env::temp_dir();
        let file_path = tmp_dir.join(format!(
            "lume_test_autosave_journal_race_{}.txt",
            std::process::id()
        ));
        std::fs::write(&file_path, "base").unwrap();

        let mut ed = Editor::open(&file_path).unwrap();
        ed.set_cursor_from_line_col(0, 4, false);
        ed.insert_str(" + edit1").unwrap();
        assert_eq!(ed.revision(), 1);
        assert!(Journal::has_journal(&file_path));

        // Trigger async save at revision 1
        ed.save_async().unwrap();

        // User continues typing (revision 2) before/while async save finishes
        ed.insert_str(" + edit2").unwrap();
        assert_eq!(ed.revision(), 2);

        // Wait for async save of revision 1 to complete
        for _ in 0..50 {
            let events = ed.poll_async_saves();
            if !events.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Tab must still be dirty and journal must NOT be deleted because revision 2 > save_revision 1
        assert!(ed.is_dirty());
        assert!(
            Journal::has_journal(&file_path),
            "Journal for newer edits must not be deleted by older background save"
        );

        // Recovering from journal must include edit2
        let recovered = Editor::open(&file_path).unwrap();
        assert_eq!(
            recovered
                .buffer()
                .slice_to_cow(0, recovered.buffer().len_bytes()),
            "base + edit1 + edit2"
        );

        let _ = Journal::remove(&file_path);
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_async_save_failure_keeps_dirty_and_reports_error() {
        // Point to a path inside a regular file (so creating parent directory fails with I/O error)
        let tmp_dir = std::env::temp_dir();
        let blocker_file = tmp_dir.join(format!("lume_blocker_file_{}", std::process::id()));
        std::fs::write(&blocker_file, "not a directory").unwrap();
        let invalid_target = blocker_file.join("subdir").join("cannot_write.txt");

        let mut ed = Editor::new();
        ed.set_file_path(Some(invalid_target.clone()));
        ed.insert_str("important unsaved data").unwrap();
        assert!(ed.is_dirty());

        ed.save_async().unwrap();
        // Immediately after save_async, buffer must remain dirty
        assert!(ed.is_dirty());

        let mut received_err = false;
        for _ in 0..50 {
            let events = ed.poll_async_saves();
            if let Some(ev) = events.first() {
                assert!(ev.result.is_err());
                received_err = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(received_err, "Expected async save I/O error event");
        assert!(ed.is_dirty(), "Tab must remain dirty when async save fails");

        let _ = Journal::remove(&invalid_target);
        let _ = std::fs::remove_file(&blocker_file);
    }

    #[test]
    fn test_close_dirty_tab_preserves_journal() {
        let tmp_dir = std::env::temp_dir();
        let file_path = tmp_dir.join(format!("lume_close_dirty_tab_{}.txt", std::process::id()));
        std::fs::write(&file_path, "initial content").unwrap();

        let mut ed = Editor::open(&file_path).unwrap();
        ed.insert_str("unsaved prefix ").unwrap();
        assert!(ed.is_dirty());
        assert!(Journal::has_journal(&file_path));

        // Closing a dirty tab must NOT delete the crash recovery journal
        ed.close_tab(0);
        assert!(Journal::has_journal(&file_path));

        // Re-opening recovers the unsaved edits
        let recovered = Editor::open(&file_path).unwrap();
        assert_eq!(
            recovered
                .buffer()
                .slice_to_cow(0, recovered.buffer().len_bytes()),
            "unsaved prefix initial content"
        );

        let _ = Journal::remove(&file_path);
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_multi_cursor_out_of_bounds_selection_delta_offset() {
        let mut ed = Editor::new();
        ed.insert_str("0123456789").unwrap();

        ed.cursors_mut()
            .set_cursors(vec![Cursor::new(2), Cursor::new(5)]);
        // Mutate primary cursor selection past the end of the buffer
        ed.cursors_mut().primary_mut().head = 20;

        // Inserting "X" skips out-of-bounds deletion on cursor 1 and must only shift cursor 2 by +1
        ed.insert_str("X").unwrap();
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "01X234X56789"
        );
    }

    #[test]
    fn test_editor_page_up_and_page_down() {
        let mut ed = Editor::new();
        let content = (1..=50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        ed.insert_str(&content).unwrap();

        // Cursor at bottom
        assert_eq!(ed.cursor_line_col().0, 49);

        // Page up by 10 lines
        ed.page_up(10, false);
        assert_eq!(ed.cursor_line_col().0, 39);

        // Page up by 100 lines (clamps to line 0)
        ed.page_up(100, false);
        assert_eq!(ed.cursor_line_col().0, 0);

        // Page down by 15 lines
        ed.page_down(15, false);
        assert_eq!(ed.cursor_line_col().0, 15);

        // Page down past end (clamps to last line 49)
        ed.page_down(100, false);
        assert_eq!(ed.cursor_line_col().0, 49);

        // Selection with shift
        ed.page_up(10, true);
        assert_eq!(ed.cursor_line_col().0, 39);
        assert!(ed.cursors().primary().is_selection());
    }

    #[test]
    fn test_smart_indent_copy_leading_spaces() {
        let mut ed = Editor::new();
        ed.insert_str("    let x = 10;").unwrap();
        ed.insert_newline().unwrap();
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "    let x = 10;\n    "
        );
        assert_eq!(ed.cursor_line_col(), (1, 4));
    }

    #[test]
    fn test_smart_indent_delimiters_add_4_spaces() {
        for delim in &["{", "(", "[", ":"] {
            let mut ed = Editor::new();
            ed.insert_str(&format!("    def test{}", delim)).unwrap();
            ed.insert_newline().unwrap();
            let expected = format!("    def test{}\n        ", delim);
            assert_eq!(
                ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
                expected
            );
            assert_eq!(ed.cursor_line_col(), (1, 8));
        }
    }

    #[test]
    fn test_smart_indent_with_trailing_spaces_after_delimiter() {
        let mut ed = Editor::new();
        ed.insert_str("struct Point {   ").unwrap();
        ed.insert_newline().unwrap();
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "struct Point {   \n    "
        );
        assert_eq!(ed.cursor_line_col(), (1, 4));
    }

    #[test]
    fn test_smart_indent_middle_of_line_split() {
        let mut ed = Editor::new();
        ed.insert_str("    let a = foo(); let b = 2;").unwrap();
        ed.cursors_mut().set_single(18); // Logo após foo();
        ed.insert_newline().unwrap();
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "    let a = foo();\n     let b = 2;"
        );
    }

    #[test]
    fn test_smart_indent_disabled() {
        let mut ed = Editor::new();
        ed.config_mut().smart_indent = false;
        ed.insert_str("    let x = 1;").unwrap();
        ed.insert_newline().unwrap();
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "    let x = 1;\n"
        );
        assert_eq!(ed.cursor_line_col(), (1, 0));
    }

    #[test]
    fn test_smart_indent_undo_atomic() {
        let mut ed = Editor::new();
        ed.insert_str("    fn main() {").unwrap();
        ed.insert_newline().unwrap();
        assert_eq!(ed.cursor_line_col(), (1, 8));
        ed.undo();
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "    fn main() {"
        );
        assert_eq!(ed.cursor_line_col(), (0, 15));
    }

    #[test]
    fn test_smart_unindent_col_4_and_col_8() {
        let mut ed = Editor::new();
        ed.insert_str("        ").unwrap(); // 8 spaces
        assert_eq!(ed.cursor_line_col(), (0, 8));

        // First backspace unindents from col 8 to col 4
        ed.delete_backwards().unwrap();
        assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "    ");
        assert_eq!(ed.cursor_line_col(), (0, 4));

        // Second backspace unindents from col 4 to col 0
        ed.delete_backwards().unwrap();
        assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "");
        assert_eq!(ed.cursor_line_col(), (0, 0));
    }

    #[test]
    fn test_smart_unindent_non_multiple_of_4_deletes_single_space() {
        let mut ed = Editor::new();
        ed.insert_str("      ").unwrap(); // 6 spaces
        assert_eq!(ed.cursor_line_col(), (0, 6));

        // Col 6 is not multiple of 4: deletes 1 space -> 5 spaces
        ed.delete_backwards().unwrap();
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "     "
        );
        assert_eq!(ed.cursor_line_col(), (0, 5));

        // Col 5 -> deletes 1 space -> 4 spaces
        ed.delete_backwards().unwrap();
        assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "    ");
        assert_eq!(ed.cursor_line_col(), (0, 4));

        // Col 4 is multiple of 4 with only spaces -> unindents 4 spaces -> 0 spaces
        ed.delete_backwards().unwrap();
        assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "");
        assert_eq!(ed.cursor_line_col(), (0, 0));
    }

    #[test]
    fn test_smart_unindent_ignores_lines_with_non_space_prefix() {
        let mut ed = Editor::new();
        ed.insert_str("a   ").unwrap(); // 'a' + 3 spaces = col 4
        assert_eq!(ed.cursor_line_col(), (0, 4));

        // Col 4 is multiple of 4, but prefix contains 'a' -> deletes 1 space
        ed.delete_backwards().unwrap();
        assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "a  ");
        assert_eq!(ed.cursor_line_col(), (0, 3));
    }

    #[test]
    fn test_smart_unindent_disabled() {
        let mut ed = Editor::new();
        ed.config_mut().smart_unindent = false;
        ed.insert_str("    ").unwrap();
        ed.delete_backwards().unwrap();
        assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "   ");
        assert_eq!(ed.cursor_line_col(), (0, 3));
    }

    #[test]
    fn test_smart_unindent_with_selection_deletes_selection() {
        let mut ed = Editor::new();
        ed.insert_str("        ").unwrap();
        // Select the last 4 spaces
        ed.cursors_mut().primary_mut().anchor = 4;
        ed.cursors_mut().primary_mut().head = 8;
        ed.delete_backwards().unwrap();
        assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "    ");
        assert_eq!(ed.cursor_line_col(), (0, 4));
    }

    #[test]
    fn test_auto_pairs_triggers_before_whitespace_newline_and_eof() {
        let pairs = [('(', ")"), ('[', "]"), ('{', "}"), ('"', "\""), ('\'', "'")];

        // Case 1: At EOF
        for &(open, close) in &pairs {
            let mut ed = Editor::new();
            ed.insert_char(open).unwrap();
            let expected = format!("{}{}", open, close);
            assert_eq!(
                ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
                expected
            );
            assert_eq!(ed.cursor_line_col(), (0, 1)); // Cursor inside pair
        }

        // Case 2: Before space
        for &(open, close) in &pairs {
            let mut ed = Editor::new();
            ed.insert_str(" ").unwrap();
            ed.cursors_mut().set_single(0);
            ed.insert_char(open).unwrap();
            let expected = format!("{}{} ", open, close);
            assert_eq!(
                ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
                expected
            );
            assert_eq!(ed.cursor_line_col(), (0, 1));
        }

        // Case 3: Before newline
        for &(open, close) in &pairs {
            let mut ed = Editor::new();
            ed.insert_str("\n").unwrap();
            ed.cursors_mut().set_single(0);
            ed.insert_char(open).unwrap();
            let expected = format!("{}{}\n", open, close);
            assert_eq!(
                ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
                expected
            );
            assert_eq!(ed.cursor_line_col(), (0, 1));
        }
    }

    #[test]
    fn test_auto_pairs_does_not_trigger_before_identifier_or_punctuation() {
        let mut ed = Editor::new();
        ed.insert_str("foo;").unwrap();
        ed.cursors_mut().set_single(0);

        // Before 'f': should only insert '('
        ed.insert_char('(').unwrap();
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "(foo;"
        );
        assert_eq!(ed.cursor_line_col(), (0, 1));

        // Move before ';': should only insert '"'
        ed.cursors_mut().set_single(4);
        ed.insert_char('"').unwrap();
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "(foo\";"
        );
    }

    #[test]
    fn test_auto_pairs_overtype_closing_delimiters() {
        let mut ed = Editor::new();
        // Type '(' -> '()' with cursor inside
        ed.insert_char('(').unwrap();
        assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "()");
        assert_eq!(ed.cursor_line_col(), (0, 1));

        // Type ')' -> overtypes, advancing cursor past ')' without inserting duplicate
        ed.insert_char(')').unwrap();
        assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "()");
        assert_eq!(ed.cursor_line_col(), (0, 2));

        // Quotes overtype
        let mut ed2 = Editor::new();
        ed2.insert_char('"').unwrap();
        assert_eq!(ed2.cursor_line_col(), (0, 1));
        ed2.insert_char('"').unwrap();
        assert_eq!(
            ed2.buffer().slice_to_cow(0, ed2.buffer().len_bytes()),
            "\"\""
        );
        assert_eq!(ed2.cursor_line_col(), (0, 2));
    }

    #[test]
    fn test_auto_pairs_disabled() {
        let mut ed = Editor::new();
        ed.config_mut().auto_pairs = false;
        ed.insert_char('(').unwrap();
        assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "(");
        assert_eq!(ed.cursor_line_col(), (0, 1));
    }

    #[test]
    fn test_auto_pairs_undo_atomic() {
        let mut ed = Editor::new();
        ed.insert_char('{').unwrap();
        assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "{}");
        ed.undo();
        assert_eq!(ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()), "");
        assert_eq!(ed.cursor_line_col(), (0, 0));
    }

    #[test]
    fn test_multi_cursor_smart_indent_mixed() {
        let mut ed = Editor::new();
        ed.insert_str("    fn foo() {\n        let x = 1;").unwrap();
        // Cursor 1 at end of line 0 (after '{') -> offset 14
        // Cursor 2 at end of line 1 (after ';') -> offset 33
        ed.cursors_mut()
            .set_cursors(vec![Cursor::new(14), Cursor::new(33)]);

        ed.insert_newline().unwrap();

        let expected = "    fn foo() {\n        \n        let x = 1;\n        ";
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            expected
        );
        assert_eq!(ed.cursors().cursors().len(), 2);
        // Cursor 1 at line 1 col 8
        assert_eq!(ed.cursors().cursors()[0].head, 23);
        // Cursor 2 at line 3 col 8
        assert_eq!(ed.cursors().cursors()[1].head, 51);
    }

    #[test]
    fn test_multi_cursor_smart_unindent() {
        let mut ed = Editor::new();
        ed.insert_str("        \n        ").unwrap();
        // Cursor 1 at end of line 0 (col 8) -> offset 8
        // Cursor 2 at end of line 1 (col 8) -> offset 17
        ed.cursors_mut()
            .set_cursors(vec![Cursor::new(8), Cursor::new(17)]);

        ed.delete_backwards().unwrap();

        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "    \n    "
        );
        assert_eq!(ed.cursors().cursors().len(), 2);
        assert_eq!(ed.cursors().cursors()[0].head, 4);
        assert_eq!(ed.cursors().cursors()[1].head, 9);
    }

    #[test]
    fn test_multi_cursor_auto_pairs() {
        let mut ed = Editor::new();
        ed.insert_str("a \nb ").unwrap();
        // Cursor 1 after 'a ' -> offset 2
        // Cursor 2 after 'b ' -> offset 5
        ed.cursors_mut()
            .set_cursors(vec![Cursor::new(2), Cursor::new(5)]);

        ed.insert_char('(').unwrap();

        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "a ()\nb ()"
        );
        assert_eq!(ed.cursors().cursors().len(), 2);
        assert_eq!(ed.cursors().cursors()[0].head, 3);
        assert_eq!(ed.cursors().cursors()[1].head, 8);
    }

    #[test]
    fn test_smart_unindent_tab_size_zero_does_not_panic() {
        let mut ed = Editor::new();
        ed.config_mut().tab_size = 0;
        ed.insert_str("    ").unwrap();
        // Should not panic with division by zero
        assert!(ed.delete_backwards().is_ok());
    }

    #[test]
    fn test_smart_indent_scope_resolution_operator_does_not_extra_indent() {
        let mut ed = Editor::new();
        ed.insert_str("use std::").unwrap();
        ed.insert_newline().unwrap();
        assert_eq!(
            ed.buffer().slice_to_cow(0, ed.buffer().len_bytes()),
            "use std::\n"
        );
        assert_eq!(ed.cursor_line_col(), (1, 0));
    }

    #[test]
    fn test_bracket_matching_simple_pairs() {
        let ed = Editor::from_str("(hello) [world] {foo}");
        // '(' at 0 matches ')' at 6
        assert_eq!(ed.find_matching_bracket(0), Some(6));
        // ')' at 6 matches '(' at 0
        assert_eq!(ed.find_matching_bracket(6), Some(0));

        // '[' at 8 matches ']' at 14
        assert_eq!(ed.find_matching_bracket(8), Some(14));
        // ']' at 14 matches '[' at 8
        assert_eq!(ed.find_matching_bracket(14), Some(8));

        // '{' at 16 matches '}' at 20
        assert_eq!(ed.find_matching_bracket(16), Some(20));
        // '}' at 20 matches '{' at 16
        assert_eq!(ed.find_matching_bracket(20), Some(16));
    }

    #[test]
    fn test_bracket_matching_nested_and_siblings() {
        // Indices:
        // { [ ( ) ] }
        // 0 2 4 6 8 10
        let ed = Editor::from_str("{ [ ( ) ] }");
        assert_eq!(ed.find_matching_bracket(0), Some(10));
        assert_eq!(ed.find_matching_bracket(2), Some(8));
        assert_eq!(ed.find_matching_bracket(4), Some(6));
        assert_eq!(ed.find_matching_bracket(6), Some(4));
        assert_eq!(ed.find_matching_bracket(8), Some(2));
        assert_eq!(ed.find_matching_bracket(10), Some(0));

        // Siblings: { () () }
        // 0: {, 2: (, 3: ), 5: (, 6: ), 8: }
        let ed_siblings = Editor::from_str("{ () () }");
        assert_eq!(ed_siblings.find_matching_bracket(0), Some(8));
        assert_eq!(ed_siblings.find_matching_bracket(2), Some(3));
        assert_eq!(ed_siblings.find_matching_bracket(3), Some(2));
        assert_eq!(ed_siblings.find_matching_bracket(5), Some(6));
        assert_eq!(ed_siblings.find_matching_bracket(6), Some(5));
        assert_eq!(ed_siblings.find_matching_bracket(8), Some(0));
    }

    #[test]
    fn test_bracket_matching_unmatched_and_non_bracket() {
        let ed = Editor::from_str("fn foo(bar { baz");
        // '(' at 6 is unclosed
        assert_eq!(ed.find_matching_bracket(6), None);
        // '{' at 11 is unclosed
        assert_eq!(ed.find_matching_bracket(11), None);
        // 'f' at 0 is not a bracket
        assert_eq!(ed.find_matching_bracket(0), None);
        // Out of bounds
        assert_eq!(ed.find_matching_bracket(999), None);
    }

    #[test]
    fn test_bracket_matching_multibyte_utf8() {
        // "Olá (Mundo! 🚀) Fim"
        // 'O'=1, 'l'=1, 'á'=2 -> 4 bytes before ' ' (byte 4) and '(' (byte 5)
        // 🚀 is 4 bytes
        let ed = Editor::from_str("Olá (Mundo! 🚀) Fim");
        let open_pos = ed
            .buffer()
            .slice_to_cow(0, ed.buffer().len_bytes())
            .find('(')
            .unwrap();
        let close_pos = ed
            .buffer()
            .slice_to_cow(0, ed.buffer().len_bytes())
            .find(')')
            .unwrap();

        assert_eq!(ed.find_matching_bracket(open_pos), Some(close_pos));
        assert_eq!(ed.find_matching_bracket(close_pos), Some(open_pos));
    }

    #[test]
    fn test_bracket_matching_jump_motion() {
        let mut ed = Editor::from_str("    let x = (1 + 2);");
        // Start at column 0 (before the bracket on this line)
        ed.cursors_mut().set_single(0);
        // Vim % motion: jumping when not on bracket finds first bracket '(' and jumps to ')'
        let jumped = ed.jump_to_matching_bracket();
        assert!(jumped);
        let close_paren_byte = ed
            .buffer()
            .slice_to_cow(0, ed.buffer().len_bytes())
            .find(')')
            .unwrap();
        assert_eq!(ed.cursors().primary().head, close_paren_byte);

        // Jump again while on ')' jumps back to '('
        let jumped_back = ed.jump_to_matching_bracket();
        assert!(jumped_back);
        let open_paren_byte = ed
            .buffer()
            .slice_to_cow(0, ed.buffer().len_bytes())
            .find('(')
            .unwrap();
        assert_eq!(ed.cursors().primary().head, open_paren_byte);

        // When no brackets on line
        let mut ed_none = Editor::from_str("let x = 42;");
        assert!(!ed_none.jump_to_matching_bracket());
    }

    #[test]
    fn test_bracket_matching_bounded_range() {
        let ed = Editor::from_str("line 0\n{\n    line 2\n}\nline 4");
        // '{' is at line 1, '}' is at line 3
        let open_byte = ed
            .buffer()
            .slice_to_cow(0, ed.buffer().len_bytes())
            .find('{')
            .unwrap();
        let close_byte = ed
            .buffer()
            .slice_to_cow(0, ed.buffer().len_bytes())
            .find('}')
            .unwrap();

        let open_char = ed.buffer().byte_to_char(open_byte);
        let close_char = ed.buffer().byte_to_char(close_byte);

        // If range covers both chars: found
        assert_eq!(
            ed.find_matching_bracket_in_range(open_byte, open_char, close_char + 1),
            Some(close_byte)
        );

        // If range excludes close_char: returns None
        assert_eq!(
            ed.find_matching_bracket_in_range(open_byte, open_char, close_char),
            None
        );

        // If range excludes open_char (start bracket outside range): returns None
        assert_eq!(
            ed.find_matching_bracket_in_range(open_byte, open_char + 1, close_char + 1),
            None
        );

        // Backward scan: if range covers both chars: found
        assert_eq!(
            ed.find_matching_bracket_in_range(close_byte, open_char, close_char + 1),
            Some(open_byte)
        );

        // Backward scan: if range excludes open_char: returns None
        assert_eq!(
            ed.find_matching_bracket_in_range(close_byte, open_char + 1, close_char + 1),
            None
        );

        // Backward scan: if range excludes close_char (close_char >= max_char): returns None
        assert_eq!(
            ed.find_matching_bracket_in_range(close_byte, open_char, close_char),
            None
        );
    }

    #[test]
    fn test_editor_delete_forwards_and_multibyte() {
        let mut editor = Editor::new();
        editor.insert_str("abcdef").unwrap();
        editor.cursors_mut().set_single(2); // between b and c

        // Single delete forwards: deletes 'c'
        editor.delete_forwards().unwrap();
        assert_eq!(
            editor.buffer().slice_to_cow(0, editor.buffer().len_bytes()),
            "abdef"
        );

        // Selection delete forwards
        editor
            .cursors_mut()
            .set_cursors(vec![Cursor::with_anchor(1, 3)]); // "bd"
        editor.delete_forwards().unwrap();
        assert_eq!(
            editor.buffer().slice_to_cow(0, editor.buffer().len_bytes()),
            "aef"
        );

        // Delete forwards at EOF
        editor.cursors_mut().set_single(3);
        assert!(editor.delete_forwards().is_ok());
        assert_eq!(editor.buffer().len_bytes(), 3);

        // Delete forwards with multibyte emoji
        let mut ed_utf8 = Editor::new();
        ed_utf8.insert_str("A🚀B").unwrap();
        ed_utf8.cursors_mut().set_single(1); // right before 🚀
        ed_utf8.delete_forwards().unwrap();
        assert_eq!(
            ed_utf8
                .buffer()
                .slice_to_cow(0, ed_utf8.buffer().len_bytes()),
            "AB"
        );
    }

    #[test]
    fn test_editor_selection_movements_and_tab_cycle() {
        let mut editor = Editor::new();
        editor
            .insert_str("line 1\nline 2 with more words\nline 3")
            .unwrap();
        editor.cursors_mut().set_single(0);

        // move_right with selection
        editor.move_right(true);
        editor.move_right(true);
        assert_eq!(editor.cursors().primary().anchor, 0);
        assert_eq!(editor.cursors().primary().head, 2);

        // move_left collapses selection when select: false
        editor.move_left(false);
        assert!(!editor.cursors().primary().is_selection());

        // move_down with selection
        editor.move_down(true);
        assert!(editor.cursors().primary().is_selection());

        // move_up with selection
        editor.move_up(true);

        // move_end and move_home with selection
        editor.move_end(true);
        editor.move_home(true);

        // Boundary movements
        editor.cursors_mut().set_single(0);
        editor.move_up(false); // at line 0
        assert_eq!(editor.cursors().primary().head, 0);

        let end_pos = editor.buffer().len_bytes();
        editor.cursors_mut().set_single(end_pos);
        editor.move_down(false); // at last line
        assert_eq!(editor.cursors().primary().head, end_pos);

        // Tab cycling and management
        let tab2 = editor.new_tab();
        assert_eq!(tab2, 1);
        assert_eq!(editor.tabs().len(), 2);
        assert_eq!(editor.active_tab_idx(), 1);

        editor.next_tab();
        assert_eq!(editor.active_tab_idx(), 0);
        editor.prev_tab();
        assert_eq!(editor.active_tab_idx(), 1);

        assert!(editor.switch_tab(0));
        assert!(!editor.switch_tab(999));

        assert!(editor.close_tab(1));
        assert!(!editor.close_tab(999));
        assert_eq!(editor.tabs().len(), 1);

        // Closing the only remaining tab auto-spawns a new clean tab
        assert!(editor.close_tab(0));
        assert_eq!(editor.tabs().len(), 1);
    }

    #[test]
    fn test_vim_primitives() {
        let mut editor = Editor::new();
        editor
            .insert_str("hello world 123\nline two\nline three\n")
            .unwrap();

        // 1. Navigation: move_to_start & move_to_end
        editor.move_to_end(false);
        assert_eq!(editor.cursors().primary().head, editor.buffer().len_bytes());
        editor.move_to_start(false);
        assert_eq!(editor.cursors().primary().head, 0);

        // 2. Navigation: move_word_forward & move_word_backward
        editor.move_word_forward(false);
        assert_eq!(editor.cursors().primary().head, 6); // start of "world"
        editor.move_word_forward(false);
        assert_eq!(editor.cursors().primary().head, 12); // start of "123"
        editor.move_word_forward(false);
        assert_eq!(editor.cursors().primary().head, 16); // start of "line two"

        editor.move_word_backward(false);
        assert_eq!(editor.cursors().primary().head, 12); // back to "123"
        editor.move_word_backward(false);
        assert_eq!(editor.cursors().primary().head, 6); // back to "world"
        editor.move_word_backward(false);
        assert_eq!(editor.cursors().primary().head, 0); // back to "hello"
        editor.move_word_backward(false);
        assert_eq!(editor.cursors().primary().head, 0); // clamp at 0

        // 3. Selection with word movement
        editor.move_word_forward(true);
        assert!(editor.cursors().primary().is_selection());
        assert_eq!(editor.cursors().primary().anchor, 0);
        assert_eq!(editor.cursors().primary().head, 6);

        // 4. Yank line
        editor.move_to_start(false);
        let yanked = editor.yank_line();
        assert_eq!(yanked, "hello world 123\n");

        // 5. Delete line & atomic undo
        editor.move_word_forward(false); // inside line 1
        let deleted = editor.delete_line().unwrap();
        assert_eq!(deleted, "hello world 123\n");
        assert_eq!(editor.buffer().rope().to_string(), "line two\nline three\n");
        assert_eq!(editor.cursors().primary().head, 0);

        assert!(editor.undo());
        assert_eq!(
            editor.buffer().rope().to_string(),
            "hello world 123\nline two\nline three\n"
        );

        // 6. Paste yank: linewise
        editor.move_to_start(false);
        editor.paste_yank("new second line\n", true).unwrap();
        assert_eq!(
            editor.buffer().rope().to_string(),
            "hello world 123\nnew second line\nline two\nline three\n"
        );
        assert!(editor.undo());
        assert_eq!(
            editor.buffer().rope().to_string(),
            "hello world 123\nline two\nline three\n"
        );

        // 7. Paste yank: characterwise
        editor.move_to_start(false);
        editor.move_right(false); // at 'e'
        editor.paste_yank("XYZ", false).unwrap();
        assert_eq!(
            editor.buffer().rope().to_string(),
            "heXYZllo world 123\nline two\nline three\n"
        );
        assert!(editor.undo());
        assert_eq!(
            editor.buffer().rope().to_string(),
            "hello world 123\nline two\nline three\n"
        );
    }

    #[test]
    fn test_vim_primitives_utf8_and_empty_lines() {
        let mut editor = Editor::new();
        // Empty buffer tests
        assert_eq!(editor.delete_line().unwrap(), "");
        assert_eq!(editor.yank_line(), "");
        editor.paste_yank("first line\n", true).unwrap();
        assert_eq!(editor.buffer().rope().to_string(), "first line\n");

        // UTF-8 Multibyte text
        editor.delete_line().unwrap();
        editor.insert_str("olá mundo_123 çã!\n\né isso\n").unwrap();
        editor.move_to_start(false);

        // Forward word movements across accents and punctuation
        editor.move_word_forward(false); // to "mundo_123"
        editor.move_word_forward(false); // to "çã"
        editor.move_word_forward(false); // to "!"
        editor.move_word_forward(false); // to "\n\n" empty line / "é"
        editor.move_word_forward(false); // to "isso"

        // Backward word movements
        editor.move_word_backward(false); // to "é"
        editor.move_word_backward(false); // to "!"
        editor.move_word_backward(false); // to "çã"
        editor.move_word_backward(false); // to "mundo_123"
        editor.move_word_backward(false); // to "olá"
        assert_eq!(editor.cursors().primary().head, 0);

        // Delete line without trailing newline
        editor.move_to_end(false);
        editor.insert_str("last line without newline").unwrap();
        let deleted_last = editor.delete_line().unwrap();
        assert!(deleted_last.contains("last line without newline"));
    }

    #[test]
    fn test_delete_line_eof_and_paste_normalization() {
        let mut editor = Editor::new();
        editor.insert_str("line 1\nline 2\n").unwrap();
        editor.move_to_end(false);
        let deleted = editor.delete_line().unwrap();
        assert_eq!(deleted, "line 2\n");
        assert_eq!(editor.buffer().rope().to_string(), "line 1\n");

        let mut editor2 = Editor::new();
        editor2.insert_str("line A\nline B").unwrap();
        editor2.move_to_end(false);
        let deleted2 = editor2.delete_line().unwrap();
        assert_eq!(deleted2, "line B\n");
        assert_eq!(editor2.buffer().rope().to_string(), "line A");

        editor2.move_to_start(false);
        editor2.paste_yank(&deleted2, true).unwrap();
        assert_eq!(editor2.buffer().rope().to_string(), "line A\nline B\n");
    }
}
