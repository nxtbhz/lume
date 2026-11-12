pub mod bracket;
pub mod clipboard;
pub mod config;
pub mod indent;
pub mod lines;
pub mod motions;
pub mod save;
pub mod search;
pub mod tab;

pub use config::EditorConfig;
pub use tab::{AsyncSaveEvent, Tab, TabSyncState};

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crossbeam_channel::{Receiver, Sender};

use crate::autosave::AutoSaveController;
use crate::buffer::{RopeBuffer, TextBuffer};
use crate::cursor::CursorSet;
use crate::history::HistoryManager;
use crate::journal::Journal;

#[derive(Clone, Debug)]
pub struct Editor {
    pub(crate) tabs: Vec<Tab>,
    pub(crate) active_tab: usize,
    pub(crate) next_tab_id: usize,
    pub(crate) autosave: AutoSaveController,
    pub(crate) config: EditorConfig,
    pub(crate) save_channel: Option<(Sender<AsyncSaveEvent>, Receiver<AsyncSaveEvent>)>,
    pub(crate) clipboard: Option<String>,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
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
            clipboard: None,
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
            clipboard: None,
        }
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let mut ed = Self::new();
        ed.open_in_current_tab(path)?;
        Ok(ed)
    }

    pub fn open_in_current_tab<P: AsRef<Path>>(&mut self, path: P) -> Result<(), std::io::Error> {
        let path = path.as_ref();

        // 1. Open file and probe first 8 KiB for NUL bytes (\0)
        let mut file = std::fs::File::open(path)?;
        let mut probe = [0u8; 8192];
        let n = file.read(&mut probe)?;
        if probe[..n].contains(&0x00) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Binary file detected (NUL byte in first 8 KiB)",
            ));
        }

        // 2. Rewind file and read into RopeBuffer, validating UTF-8 encoding
        file.seek(SeekFrom::Start(0))?;
        let buffer = RopeBuffer::from_reader(file).map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid UTF-8 encoding")
            } else {
                e
            }
        })?;

        // 3. Check for crash recovery journal:
        //    - If stale (disk mtime > journal mtime): remove stale journal and load cleanly
        //    - If fresh and content differs: stage in pending_recovery without marking dirty
        //    - If fresh and content identical: remove redundant journal
        let journal_path = Journal::journal_path(path);
        let mut pending_recovery = None;

        if Journal::has_journal(path) {
            if !Journal::is_fresh_for(path) {
                let _ = Journal::remove(path);
            } else if let Ok(Some(recovered_text)) = Journal::recover(path) {
                let disk_content = buffer.slice_to_cow(0, buffer.len_bytes());
                if recovered_text == disk_content {
                    let _ = Journal::remove(path);
                } else {
                    pending_recovery = Some(recovered_text);
                }
            }
        }

        // 4. All checks passed — safe to mutate active tab state
        let tab = &mut self.tabs[self.active_tab];
        tab.buffer = buffer;
        tab.cursors.set_single(0);
        tab.history.clear();
        tab.file_path = Some(path.to_path_buf());
        tab.journal_path = Some(journal_path);
        tab.is_dirty = false;
        tab.recovered_from_journal = false;
        tab.pending_recovery = pending_recovery;
        tab.update_clean_snapshot();
        if let Some(ref sync) = tab.sync_state {
            sync.is_dirty.store(false, Ordering::SeqCst);
            sync.revision.store(tab.revision, Ordering::SeqCst);
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
                let is_same_file = match (p.canonicalize(), path.canonicalize()) {
                    (Ok(c1), Ok(c2)) => c1 == c2,
                    _ => false,
                };
                if p == path || is_same_file {
                    self.active_tab = i;
                    return Ok(i);
                }
            }
        }

        let idx = self.new_tab();
        if let Err(e) = self.open_in_current_tab(path) {
            self.tabs.pop();
            if self.active_tab >= self.tabs.len() && !self.tabs.is_empty() {
                self.active_tab = self.tabs.len() - 1;
            }
            return Err(e);
        }
        Ok(idx)
    }

    pub fn open_or_load<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error> {
        let path = path.as_ref();
        // If already open, switch to it
        for (i, tab) in self.tabs.iter().enumerate() {
            if let Some(ref p) = tab.file_path {
                let is_same_file = match (p.canonicalize(), path.canonicalize()) {
                    (Ok(c1), Ok(c2)) => c1 == c2,
                    _ => false,
                };
                if p == path || is_same_file {
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
        if let Err(e) = self.open_in_current_tab(path) {
            self.tabs.pop();
            if self.active_tab >= self.tabs.len() && !self.tabs.is_empty() {
                self.active_tab = self.tabs.len() - 1;
            }
            return Err(e);
        }
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
        self.tabs[self.active_tab].journal_path = path
            .as_ref()
            .map(|p| crate::journal::Journal::journal_path(p));
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

    pub fn is_any_dirty(&self) -> bool {
        self.tabs.iter().any(|tab| {
            if let Some(ref sync) = tab.sync_state {
                tab.is_dirty && sync.is_dirty.load(Ordering::SeqCst)
            } else {
                tab.is_dirty
            }
        })
    }

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

    pub fn revision(&self) -> u64 {
        self.tabs[self.active_tab].revision
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

    /// Restores pending recovery text into the tab buffer, marks the buffer dirty,
    /// updates revision, and notifies autosave.
    pub fn recover_tab_journal(&mut self, tab_idx: usize) -> bool {
        if tab_idx >= self.tabs.len() {
            return false;
        }
        let tab = &mut self.tabs[tab_idx];
        if let Some(text) = tab.pending_recovery.take() {
            tab.buffer = RopeBuffer::from_str(&text);
            tab.cursors.set_single(0);
            tab.history.clear();
            tab.revision += 1;
            tab.is_dirty = true;
            tab.recovered_from_journal = true;
            tab.cached_diffs = None;
            if let Some(ref sync) = tab.sync_state {
                sync.revision.store(tab.revision, Ordering::SeqCst);
                sync.is_dirty.store(true, Ordering::SeqCst);
            }
            self.autosave.notify_change();
            true
        } else {
            false
        }
    }

    /// Discards pending recovery and unlinks the `.journal` crash recovery file for the tab.
    pub fn discard_tab_changes(&mut self, tab_idx: usize) -> bool {
        if tab_idx >= self.tabs.len() {
            return false;
        }
        let tab = &mut self.tabs[tab_idx];
        tab.pending_recovery = None;
        let jpath = tab
            .journal_path
            .clone()
            .or_else(|| tab.file_path.as_ref().map(|p| Journal::journal_path(p)));
        if let Some(jp) = jpath {
            Journal::enqueue_remove(jp.clone());
            Journal::flush();
        }
        true
    }

    /// Discards pending recovery and unlinks all `.journal` files for all currently open tabs.
    pub fn discard_all_journals(&mut self) {
        for i in 0..self.tabs.len() {
            self.discard_tab_changes(i);
        }
    }

    /// Returns pending recovery text for the active tab, if present.
    pub fn pending_recovery(&self) -> Option<&str> {
        self.tabs[self.active_tab].pending_recovery.as_deref()
    }

    /// Flushes all pending background journal operations and blocks until durability is achieved.
    pub fn flush_journal(&self) {
        Journal::flush();
    }
}

#[cfg(test)]
mod tests;
