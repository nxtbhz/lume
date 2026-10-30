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
                if p == path
                    || (p.exists()
                        && path.exists()
                        && p.canonicalize().ok() == path.canonicalize().ok())
                {
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
                if p == path
                    || (p.exists()
                        && path.exists()
                        && p.canonicalize().ok() == path.canonicalize().ok())
                {
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
}

#[cfg(test)]
mod tests;
