use crate::buffer::{TextBuffer, TextDelta};
use crate::cursor::Cursor;
use std::collections::VecDeque;

pub const DEFAULT_MAX_UNDO_ENTRIES: usize = 5_000;
pub const DEFAULT_MAX_HISTORY_BYTES: usize = 50 * 1024 * 1024; // 50 MB

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub deltas: Vec<TextDelta>,
    pub cursors_before: Vec<Cursor>,
    pub primary_before: usize,
    pub cursors_after: Vec<Cursor>,
    pub primary_after: usize,
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
            primary_before: 0,
            cursors_after,
            primary_after: 0,
        });
        self.undo_group_open = can_coalesce;
    }

    pub fn record_edits_with_primary(
        &mut self,
        deltas: Vec<TextDelta>,
        cursors_before: Vec<Cursor>,
        primary_before: usize,
        cursors_after: Vec<Cursor>,
        primary_after: usize,
    ) {
        self.invalidate_branch_saved_depth();
        self.redo_stack.clear();
        self.push_undo(HistoryEntry {
            deltas,
            cursors_before,
            primary_before,
            cursors_after,
            primary_after,
        });
        self.undo_group_open = false;
    }

    pub fn undo_with_primary(
        &mut self,
        current_buffer: &mut dyn TextBuffer,
    ) -> Option<(Vec<Cursor>, usize)> {
        self.undo_group_open = false;
        let entry = self.undo_stack.pop_back()?;
        let bytes = entry_bytes(&entry);
        self.current_bytes = self.current_bytes.saturating_sub(bytes);

        let applied = apply_delta_sequence(
            current_buffer,
            entry
                .deltas
                .iter()
                .rev()
                .map(|d| (d.start_byte, d.new_text.as_str(), d.old_text.as_str())),
        );
        if !applied {
            self.current_bytes += bytes;
            self.undo_stack.push_back(entry);
            return None;
        }

        let cursors = (entry.cursors_before.clone(), entry.primary_before);
        self.redo_stack.push(entry);
        Some(cursors)
    }

    pub fn redo_with_primary(
        &mut self,
        current_buffer: &mut dyn TextBuffer,
    ) -> Option<(Vec<Cursor>, usize)> {
        self.undo_group_open = false;
        let entry = self.redo_stack.pop()?;

        let applied = apply_delta_sequence(
            current_buffer,
            entry
                .deltas
                .iter()
                .map(|d| (d.start_byte, d.old_text.as_str(), d.new_text.as_str())),
        );
        if !applied {
            self.redo_stack.push(entry);
            return None;
        }

        let cursors = (entry.cursors_after.clone(), entry.primary_after);
        self.push_undo(entry);
        Some(cursors)
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

enum RollbackOp<'a> {
    Reinsert { start: usize, text: &'a str },
    Redelete { start: usize, len: usize },
}

fn rollback_buffer_ops(buf: &mut dyn TextBuffer, ops: Vec<RollbackOp<'_>>) {
    for op in ops.into_iter().rev() {
        match op {
            RollbackOp::Reinsert { start, text } => {
                let _ = buf.insert(start, text);
            }
            RollbackOp::Redelete { start, len } => {
                let _ = buf.delete(start, start + len);
            }
        }
    }
}

fn apply_delta_sequence<'a>(
    buf: &mut dyn TextBuffer,
    steps: impl Iterator<Item = (usize, &'a str, &'a str)>,
) -> bool {
    let mut undo_log = Vec::new();
    for (start, to_delete, to_insert) in steps {
        if !to_delete.is_empty() {
            if buf.delete(start, start + to_delete.len()).is_err() {
                rollback_buffer_ops(buf, undo_log);
                return false;
            }
            undo_log.push(RollbackOp::Reinsert {
                start,
                text: to_delete,
            });
        }
        if !to_insert.is_empty() {
            if buf.insert(start, to_insert).is_err() {
                rollback_buffer_ops(buf, undo_log);
                return false;
            }
            undo_log.push(RollbackOp::Redelete {
                start,
                len: to_insert.len(),
            });
        }
    }
    true
}

impl HistoryManager for History {
    fn record_edits(
        &mut self,
        deltas: Vec<TextDelta>,
        cursors_before: Vec<Cursor>,
        cursors_after: Vec<Cursor>,
    ) {
        self.record_edits_with_primary(deltas, cursors_before, 0, cursors_after, 0);
    }

    fn undo(&mut self, current_buffer: &mut dyn TextBuffer) -> Option<Vec<Cursor>> {
        self.undo_with_primary(current_buffer)
            .map(|(cursors, _)| cursors)
    }

    fn redo(&mut self, current_buffer: &mut dyn TextBuffer) -> Option<Vec<Cursor>> {
        self.redo_with_primary(current_buffer)
            .map(|(cursors, _)| cursors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::{BufferError, RopeBuffer};
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

    #[test]
    fn test_history_undo_failure_does_not_drop_entry_silently() {
        struct FailAfterOpsBuffer {
            inner: RopeBuffer,
            ops_until_fail: Option<usize>,
        }

        impl TextBuffer for FailAfterOpsBuffer {
            fn len_bytes(&self) -> usize {
                self.inner.len_bytes()
            }
            fn len_lines(&self) -> usize {
                self.inner.len_lines()
            }
            fn byte_to_line(&self, byte_idx: usize) -> usize {
                self.inner.byte_to_line(byte_idx)
            }
            fn line_to_byte(&self, line_idx: usize) -> usize {
                self.inner.line_to_byte(line_idx)
            }
            fn slice_to_cow(&self, start: usize, end: usize) -> std::borrow::Cow<'_, str> {
                self.inner.slice_to_cow(start, end)
            }
            fn insert(&mut self, byte_idx: usize, text: &str) -> Result<TextDelta, BufferError> {
                if let Some(ref mut left) = self.ops_until_fail {
                    if *left == 0 {
                        self.ops_until_fail = None;
                        return Err(BufferError::OutOfBounds(byte_idx, self.inner.len_bytes()));
                    }
                    *left -= 1;
                }
                self.inner.insert(byte_idx, text)
            }
            fn delete(
                &mut self,
                start_byte: usize,
                end_byte: usize,
            ) -> Result<TextDelta, BufferError> {
                if let Some(ref mut left) = self.ops_until_fail {
                    if *left == 0 {
                        self.ops_until_fail = None;
                        return Err(BufferError::InvalidRange(
                            start_byte,
                            end_byte,
                            self.inner.len_bytes(),
                        ));
                    }
                    *left -= 1;
                }
                self.inner.delete(start_byte, end_byte)
            }
        }

        let mut buf = FailAfterOpsBuffer {
            inner: RopeBuffer::from_str("hello world"),
            ops_until_fail: None,
        };
        let mut history = History::new();

        // Single history entry containing 2 deltas: replace "hello" with "HELLO", and "world" with "WORLD"
        let d1_del = buf.delete(0, 5).unwrap();
        let d1_ins = buf.insert(0, "HELLO").unwrap();
        let d2_del = buf.delete(6, 11).unwrap();
        let d2_ins = buf.insert(6, "WORLD").unwrap();
        history.record_edits(
            vec![
                TextDelta::new(0, d1_del.old_text, d1_ins.new_text),
                TextDelta::new(6, d2_del.old_text, d2_ins.new_text),
            ],
            vec![Cursor::new(0)],
            vec![Cursor::new(11)],
        );
        assert_eq!(buf.slice_to_cow(0, buf.len_bytes()), "HELLO WORLD");

        // Fail on the 3rd buffer op during undo (after reverting d2 completely, when deleting "HELLO")
        buf.ops_until_fail = Some(2);
        let undo_res = history.undo(&mut buf);
        assert!(undo_res.is_none(), "undo failure must return None");
        assert!(
            history.can_undo(),
            "failed undo must restore entry to undo_stack"
        );
        assert!(
            !history.can_redo(),
            "failed undo must not push entry to redo_stack"
        );
        buf.ops_until_fail = None;
        assert_eq!(
            buf.slice_to_cow(0, buf.len_bytes()),
            "HELLO WORLD",
            "partial undo must roll back completely"
        );

        // Now allow undo to succeed, then verify redo failure rolls back symmetrically
        assert!(history.undo(&mut buf).is_some());
        assert_eq!(buf.slice_to_cow(0, buf.len_bytes()), "hello world");

        buf.ops_until_fail = Some(2);
        let redo_res = history.redo(&mut buf);
        assert!(redo_res.is_none(), "redo failure must return None");
        assert!(
            history.can_redo(),
            "failed redo must restore entry to redo_stack"
        );
        assert_eq!(
            buf.slice_to_cow(0, buf.len_bytes()),
            "hello world",
            "partial redo must roll back completely"
        );
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
