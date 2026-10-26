use crate::buffer::RopeBuffer;
use crate::cursor::CursorSet;
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
            sync_state: None,
        }
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
