use crate::buffer::{TextBuffer, TextDelta};
use crate::cursor::Cursor;
use std::collections::VecDeque;

pub const DEFAULT_MAX_UNDO_ENTRIES: usize = 5_000;
pub const DEFAULT_MAX_HISTORY_BYTES: usize = 50 * 1024 * 1024; // 50 MB

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub deltas: Vec<TextDelta>,
    pub cursors_before: Vec<Cursor>,
    pub cursors_after: Vec<Cursor>,
}

fn entry_bytes(entry: &HistoryEntry) -> usize {
    entry
        .deltas
        .iter()
        .map(|d| d.old_text.len() + d.new_text.len())
        .sum()
}

pub trait HistoryManager {
    fn record_edit(
        &mut self,
        delta: TextDelta,
        cursors_before: Vec<Cursor>,
        cursors_after: Vec<Cursor>,
    ) {
        self.record_edits(vec![delta], cursors_before, cursors_after);
    }
    fn record_edits(
        &mut self,
        deltas: Vec<TextDelta>,
        cursors_before: Vec<Cursor>,
        cursors_after: Vec<Cursor>,
    );
    fn undo(&mut self, current_buffer: &mut dyn TextBuffer) -> Option<Vec<Cursor>>;
    fn redo(&mut self, current_buffer: &mut dyn TextBuffer) -> Option<Vec<Cursor>>;
}

#[derive(Clone, Debug)]
pub struct History {
    undo_stack: VecDeque<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    max_entries: usize,
    max_bytes: usize,
    current_bytes: usize,
    undo_group_open: bool,
    saved_depth: Option<usize>,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    pub fn new() -> Self {
        Self::with_limits(DEFAULT_MAX_UNDO_ENTRIES, DEFAULT_MAX_HISTORY_BYTES)
    }

    pub fn with_limits(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: Vec::new(),
            max_entries: max_entries.max(1),
            max_bytes: max_bytes.max(1),
            current_bytes: 0,
            undo_group_open: false,
            saved_depth: Some(0),
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn break_group(&mut self) {
        self.undo_group_open = false;
    }

    pub fn mark_saved(&mut self) {
        self.saved_depth = Some(self.undo_stack.len());
        self.undo_group_open = false;
    }

    pub fn invalidate_saved_state(&mut self) {
        self.saved_depth = None;
    }

    pub fn is_at_saved_state(&self) -> bool {
        self.saved_depth == Some(self.undo_stack.len())
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.current_bytes = 0;
        self.undo_group_open = false;
        self.saved_depth = Some(0);
    }

    fn invalidate_branch_saved_depth(&mut self) {
        if !self.redo_stack.is_empty() {
            if let Some(depth) = self.saved_depth {
                if depth > self.undo_stack.len() {
                    self.saved_depth = None;
                }
            }
        }
    }

    pub fn record_coalesced_insert(
        &mut self,
        delta: TextDelta,
        cursors_before: Vec<Cursor>,
        cursors_after: Vec<Cursor>,
        can_coalesce: bool,
    ) {
        let at_saved_depth = self.saved_depth == Some(self.undo_stack.len());
        if can_coalesce
            && self.undo_group_open
            && !at_saved_depth
            && self.redo_stack.is_empty()
            && delta.old_text.is_empty()
            && !delta.new_text.is_empty()
            && cursors_before.len() == 1
            && cursors_after.len() == 1
            && self.current_bytes + delta.new_text.len() <= self.max_bytes
        {
            if let Some(last_entry) = self.undo_stack.back_mut() {
                if last_entry.deltas.len() == 1 {
                    let last_delta = &mut last_entry.deltas[0];
                    if last_delta.old_text.is_empty()
                        && last_delta.start_byte + last_delta.new_text.len() == delta.start_byte
                    {
                        last_delta.new_text.push_str(&delta.new_text);
                        last_entry.cursors_after = cursors_after;
                        self.current_bytes += delta.new_text.len();
                        self.undo_group_open = true;
                        return;
                    }
                }
            }
        }

        self.invalidate_branch_saved_depth();
        self.redo_stack.clear();
        self.push_undo(HistoryEntry {
            deltas: vec![delta],
            cursors_before,
            cursors_after,
        });
        self.undo_group_open = can_coalesce;
    }

    fn push_undo(&mut self, entry: HistoryEntry) {
        let bytes = entry_bytes(&entry);
        self.current_bytes += bytes;
        self.undo_stack.push_back(entry);

        while self.undo_stack.len() > self.max_entries
            || (self.current_bytes > self.max_bytes && self.undo_stack.len() > 1)
        {
            if let Some(oldest) = self.undo_stack.pop_front() {
                self.current_bytes = self.current_bytes.saturating_sub(entry_bytes(&oldest));
                self.saved_depth = match self.saved_depth {
                    Some(d) if d > 0 => Some(d - 1),
                    _ => None,
                };
            }
        }
    }
}

impl HistoryManager for History {
    fn record_edits(
        &mut self,
        deltas: Vec<TextDelta>,
        cursors_before: Vec<Cursor>,
        cursors_after: Vec<Cursor>,
    ) {
        self.invalidate_branch_saved_depth();
        self.redo_stack.clear();
        self.push_undo(HistoryEntry {
            deltas,
            cursors_before,
            cursors_after,
        });
        self.undo_group_open = false;
    }

    fn undo(&mut self, current_buffer: &mut dyn TextBuffer) -> Option<Vec<Cursor>> {
        self.undo_group_open = false;
        let entry = self.undo_stack.pop_back()?;
        self.current_bytes = self.current_bytes.saturating_sub(entry_bytes(&entry));

        // Revert edits in reverse order
        for delta in entry.deltas.iter().rev() {
            if !delta.new_text.is_empty() {
                let _ = current_buffer
                    .delete(delta.start_byte, delta.start_byte + delta.new_text.len());
            }
            if !delta.old_text.is_empty() {
                let _ = current_buffer.insert(delta.start_byte, &delta.old_text);
            }
        }

        let cursors = entry.cursors_before.clone();
        self.redo_stack.push(entry);
        Some(cursors)
    }

    fn redo(&mut self, current_buffer: &mut dyn TextBuffer) -> Option<Vec<Cursor>> {
        self.undo_group_open = false;
        let entry = self.redo_stack.pop()?;

        // Re-apply edits in forward order
        for delta in &entry.deltas {
            if !delta.old_text.is_empty() {
                let _ = current_buffer
                    .delete(delta.start_byte, delta.start_byte + delta.old_text.len());
            }
            if !delta.new_text.is_empty() {
                let _ = current_buffer.insert(delta.start_byte, &delta.new_text);
            }
        }

        let cursors = entry.cursors_after.clone();
        self.push_undo(entry);
        Some(cursors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::RopeBuffer;
    use proptest::prelude::*;

    #[test]
    fn test_undo_redo_symmetry() {
        let mut buf = RopeBuffer::from_str("Hello World");
        let mut history = History::new();

        // 1. Insert ", Brave" at byte 5
        let delta = buf.insert(5, ", Brave").unwrap();
        history.record_edit(delta, vec![Cursor::new(5)], vec![Cursor::new(13)]);

        assert_eq!(buf.slice_to_cow(0, buf.len_bytes()), "Hello, Brave World");
        assert!(history.can_undo());
        assert!(!history.can_redo());

        // 2. Undo
        let cursors = history.undo(&mut buf).unwrap();
        assert_eq!(cursors[0].head, 5);
        assert_eq!(buf.slice_to_cow(0, buf.len_bytes()), "Hello World");
        assert!(history.can_redo());

        // 3. Redo
        let cursors = history.redo(&mut buf).unwrap();
        assert_eq!(cursors[0].head, 13);
        assert_eq!(buf.slice_to_cow(0, buf.len_bytes()), "Hello, Brave World");
    }

    #[test]
    fn test_history_limits_retention() {
        let mut buf = RopeBuffer::from_str("Initial");
        let mut history = History::with_limits(3, 100);

        for i in 1..=5 {
            let s = format!(" {}", i);
            let delta = buf.insert(buf.len_bytes(), &s).unwrap();
            history.record_edit(
                delta,
                vec![Cursor::new(0)],
                vec![Cursor::new(buf.len_bytes())],
            );
        }

        // Stack size must not exceed 3
        assert_eq!(history.undo_stack.len(), 3);
    }

    #[test]
    fn test_coalescing_and_saved_depth_eviction() {
        let mut buf = RopeBuffer::from_str("");
        let mut history = History::with_limits(2, 1024);
        assert!(history.is_at_saved_state());

        let d1 = buf.insert(0, "a").unwrap();
        history.record_coalesced_insert(d1, vec![Cursor::new(0)], vec![Cursor::new(1)], true);
        let d2 = buf.insert(1, "b").unwrap();
        history.record_coalesced_insert(d2, vec![Cursor::new(1)], vec![Cursor::new(2)], true);
        assert_eq!(history.undo_stack.len(), 1);
        assert!(!history.is_at_saved_state());

        // Mark saved at depth 1 ("ab")
        history.mark_saved();
        assert!(history.is_at_saved_state());

        // Typing right after save must NOT mutate saved entry
        let d3 = buf.insert(2, "c").unwrap();
        history.record_coalesced_insert(d3, vec![Cursor::new(2)], vec![Cursor::new(3)], true);
        assert_eq!(history.undo_stack.len(), 2);
        assert!(!history.is_at_saved_state());

        // Push a 3rd entry so oldest ("ab") is evicted; saved_depth shifts 1 -> 0
        history.break_group();
        let d4 = buf.insert(3, "d").unwrap();
        history.record_coalesced_insert(d4, vec![Cursor::new(3)], vec![Cursor::new(4)], true);
        assert_eq!(history.undo_stack.len(), 2);
        assert!(!history.is_at_saved_state());

        // Undo "d" and "c" -> back to depth 0 (which is the state after evicted "ab")
        assert!(history.undo(&mut buf).is_some());
        assert!(!history.is_at_saved_state());
        assert!(history.undo(&mut buf).is_some());
        assert_eq!(buf.slice_to_cow(0, buf.len_bytes()), "ab");
        assert!(history.is_at_saved_state());
    }

    proptest! {
        #[test]
        fn prop_undo_redo_invariance(initial in "\\PC*", edits in prop::collection::vec("[a-zA-Z0-9 ]+", 1..10)) {
            let mut buf = RopeBuffer::from_str(&initial);
            let mut history = History::new();

            for edit in &edits {
                let len = buf.len_bytes();
                let insert_pos = if len == 0 { 0 } else { len / 2 };
                // ensure insert_pos is on valid char boundary
                let char_idx = buf.byte_to_char(insert_pos);
                let byte_pos = buf.char_to_byte(char_idx);

                if let Ok(delta) = buf.insert(byte_pos, edit) {
                    history.record_edit(delta, vec![Cursor::new(byte_pos)], vec![Cursor::new(byte_pos + edit.len())]);
                }
            }

            // Undo all edits
            while history.can_undo() {
                history.undo(&mut buf);
            }

            // Invariant: buffer must match initial string exactly
            prop_assert_eq!(buf.slice_to_cow(0, buf.len_bytes()), initial);
        }
    }
}
