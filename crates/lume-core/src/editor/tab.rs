use crate::buffer::{RopeBuffer, TextBuffer};
use crate::cursor::CursorSet;
use crate::diff::LineDiffStatus;
use crate::editor::line_ending::LineEnding;
use crate::history::History;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct TabSyncState {
    pub revision: AtomicU64,
    pub is_dirty: AtomicBool,
    pub journal_lock: Mutex<()>,
    pub save_lock: Mutex<()>,
}

#[derive(Clone, Debug)]
pub struct AsyncSaveEvent {
    pub tab_id: usize,
    pub path: PathBuf,
    pub save_revision: u64,
    pub result: Result<(), String>,
}

#[derive(Clone, Debug)]
pub struct Tab {
    pub id: usize,
    pub revision: u64,
    pub buffer: RopeBuffer,
    pub cursors: CursorSet,
    pub history: History,
    pub file_path: Option<PathBuf>,
    pub language_override: Option<String>,
    pub line_ending: LineEnding,
    pub is_dirty: bool,
    pub recovered_from_journal: bool,
    pub pending_recovery: Option<String>,
    pub clean_snapshot: Option<ropey::Rope>,
    pub(crate) git_baseline: Option<ropey::Rope>,
    pub journal_path: Option<PathBuf>,
    pub(crate) cached_diffs: Option<(u64, Arc<Vec<LineDiffStatus>>)>,
    pub(crate) sync_state: Option<Arc<TabSyncState>>,
}

impl Tab {
    fn from_buffer(id: usize, buffer: RopeBuffer, line_ending: LineEnding) -> Self {
        Self {
            id,
            revision: 0,
            buffer,
            cursors: CursorSet::new(),
            history: History::new(),
            file_path: None,
            journal_path: None,
            language_override: None,
            line_ending,
            is_dirty: false,
            recovered_from_journal: false,
            pending_recovery: None,
            clean_snapshot: None,
            git_baseline: None,
            cached_diffs: None,
            sync_state: None,
        }
    }

    pub fn new(id: usize) -> Self {
        Self::from_buffer(id, RopeBuffer::new(), LineEnding::Lf)
    }

    pub fn from_str(id: usize, text: &str) -> Self {
        let buffer = RopeBuffer::from_str(text);
        let line_ending = LineEnding::detect(buffer.rope());
        Self::from_buffer(id, buffer, line_ending)
    }

    pub fn from_rope(id: usize, rope: ropey::Rope) -> Self {
        let buffer = RopeBuffer::from_rope(rope);
        let line_ending = LineEnding::detect(buffer.rope());
        Self::from_buffer(id, buffer, line_ending)
    }

    pub fn update_clean_snapshot(&mut self) {
        self.clean_snapshot = Some(self.buffer.rope().clone());
        self.cached_diffs = None;
    }

    pub fn diff_markers(&mut self) -> Option<Arc<Vec<LineDiffStatus>>> {
        let base = if let Some(ref git_base) = self.git_baseline {
            git_base
        } else if self.is_dirty {
            self.clean_snapshot.as_ref()?
        } else {
            return None;
        };

        let cached_valid = self
            .cached_diffs
            .as_ref()
            .map(|(rev, _)| *rev == self.revision)
            .unwrap_or(false);

        if !cached_valid {
            let diffs = crate::diff::compute_rope_diffs(base, self.buffer.rope());
            self.cached_diffs = Some((self.revision, Arc::new(diffs)));
        }

        self.cached_diffs
            .as_ref()
            .map(|(_, diffs)| Arc::clone(diffs))
    }

    pub fn name(&self) -> String {
        if let Some(ref p) = self.file_path {
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("[No Name]")
                .to_string()
        } else {
            format!("[Tab {}]", self.id)
        }
    }

    pub fn cursor_line_col(&self) -> (usize, usize) {
        self.cursor_line_col_for(&self.cursors)
    }

    pub fn cursor_line_col_for(&self, cursors: &CursorSet) -> (usize, usize) {
        let head = cursors.primary().head.min(self.buffer.len_bytes());
        let line = self.buffer.byte_to_line(head);
        let line_start_char = self.buffer.rope().line_to_char(line);
        let cursor_char = self.buffer.byte_to_char(head);
        let col = cursor_char.saturating_sub(line_start_char);
        (line, col)
    }

    pub fn cursor_visual_line_col(&self) -> (usize, usize) {
        self.cursor_visual_line_col_for(&self.cursors)
    }

    pub fn cursor_visual_line_col_for(&self, cursors: &CursorSet) -> (usize, usize) {
        use unicode_width::UnicodeWidthChar;
        let head = cursors.primary().head.min(self.buffer.len_bytes());
        let line = self.buffer.byte_to_line(head);
        let line_start_byte = self.buffer.line_to_byte(line);
        let line_prefix = self.buffer.slice_to_cow(line_start_byte, head);
        let mut visual_col = 0;
        for c in line_prefix.chars() {
            if c == '\t' {
                visual_col += 4 - (visual_col % 4);
            } else {
                visual_col += c.width().unwrap_or(0);
            }
        }
        (line, visual_col)
    }
}
