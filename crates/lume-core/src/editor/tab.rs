use crate::buffer::RopeBuffer;
use crate::cursor::CursorSet;
use crate::diff::LineDiffStatus;
use crate::history::History;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct TabSyncState {
    pub revision: AtomicU64,
    pub is_dirty: AtomicBool,
    pub journal_lock: Mutex<()>,
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
    pub is_dirty: bool,
    pub recovered_from_journal: bool,
    pub clean_snapshot: Option<ropey::Rope>,
    pub(crate) cached_diffs: Option<(u64, Vec<LineDiffStatus>)>,
    pub(crate) sync_state: Option<Arc<TabSyncState>>,
}

impl Tab {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            revision: 0,
            buffer: RopeBuffer::new(),
            cursors: CursorSet::new(),
            history: History::new(),
            file_path: None,
            language_override: None,
            is_dirty: false,
            recovered_from_journal: false,
            clean_snapshot: None,
            cached_diffs: None,
            sync_state: None,
        }
    }

    pub fn from_str(id: usize, text: &str) -> Self {
        Self {
            id,
            revision: 0,
            buffer: RopeBuffer::from_str(text),
            cursors: CursorSet::new(),
            history: History::new(),
            file_path: None,
            language_override: None,
            is_dirty: false,
            recovered_from_journal: false,
            clean_snapshot: None,
            cached_diffs: None,
            sync_state: None,
        }
    }

    pub fn update_clean_snapshot(&mut self) {
        self.clean_snapshot = Some(self.buffer.rope().clone());
        self.cached_diffs = None;
    }

    pub fn diff_markers(&mut self) -> Option<&[LineDiffStatus]> {
        if !self.is_dirty {
            return None;
        }
        let snapshot = self.clean_snapshot.as_ref()?;

        let cached_valid = self
            .cached_diffs
            .as_ref()
            .map(|(rev, _)| *rev == self.revision)
            .unwrap_or(false);

        if !cached_valid {
            let base_lines_owned: Vec<String> = snapshot
                .lines()
                .map(|l| {
                    let mut s = l.to_string();
                    if s.ends_with('\n') {
                        s.pop();
                        if s.ends_with('\r') {
                            s.pop();
                        }
                    }
                    s
                })
                .collect();

            let curr_lines_owned: Vec<String> = self
                .buffer
                .rope()
                .lines()
                .map(|l| {
                    let mut s = l.to_string();
                    if s.ends_with('\n') {
                        s.pop();
                        if s.ends_with('\r') {
                            s.pop();
                        }
                    }
                    s
                })
                .collect();

            let base_refs: Vec<&str> = base_lines_owned.iter().map(|s| s.as_str()).collect();
            let curr_refs: Vec<&str> = curr_lines_owned.iter().map(|s| s.as_str()).collect();

            let diffs = crate::diff::compute_line_diffs(&base_refs, &curr_refs);
            self.cached_diffs = Some((self.revision, diffs));
        }

        self.cached_diffs.as_ref().map(|(_, diffs)| diffs.as_slice())
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
}
