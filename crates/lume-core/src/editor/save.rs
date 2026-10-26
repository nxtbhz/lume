use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::buffer::RopeBuffer;
use crate::editor::tab::{AsyncSaveEvent, TabSyncState};
use crate::editor::Editor;
use crate::journal::Journal;

impl Editor {
    pub fn save(&mut self) -> Result<(), std::io::Error> {
        let path = self.tabs[self.active_tab].file_path.clone();
        if let Some(path) = path {
            self.save_as(&path)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No file path specified",
            ))
        }
    }

    pub fn write_buffer_atomically(path: &Path, buffer: &RopeBuffer) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("file");
        let temp_filename = format!(".{}.lume_tmp", file_name);
        let temp_path = match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join(temp_filename),
            _ => PathBuf::from(temp_filename),
        };

        let write_res = (|| -> Result<(), std::io::Error> {
            let file = std::fs::File::create(&temp_path)?;
            let mut writer = std::io::BufWriter::new(file);
            buffer.write_to(&mut writer)?;
            writer.get_ref().sync_all()?;
            Ok(())
        })();

        if let Err(e) = write_res {
            let _ = std::fs::remove_file(&temp_path);
            return Err(e);
        }

        if let Err(e) = std::fs::rename(&temp_path, path) {
            let _ = std::fs::remove_file(&temp_path);
            return Err(e);
        }

        Ok(())
    }

    pub fn save_buffer_atomically(path: &Path, buffer: &RopeBuffer) -> Result<(), std::io::Error> {
        Self::write_buffer_atomically(path, buffer)?;
        let _ = Journal::remove(path);
        Ok(())
    }

    pub fn save_as<P: AsRef<Path>>(&mut self, path: P) -> Result<(), std::io::Error> {
        let path = path.as_ref();
        Self::save_buffer_atomically(path, &self.tabs[self.active_tab].buffer)?;

        let tab = &mut self.tabs[self.active_tab];
        tab.file_path = Some(path.to_path_buf());
        tab.is_dirty = false;
        tab.recovered_from_journal = false;
        if let Some(ref sync) = tab.sync_state {
            sync.is_dirty.store(false, Ordering::SeqCst);
        }
        self.autosave.mark_saved();

        Ok(())
    }

    /// Asynchronous save offloading disk write, fsync and rename to a background thread
    pub fn save_async(&mut self) -> Result<(), std::io::Error> {
        let path = match self.tabs[self.active_tab].file_path {
            Some(ref p) => p.clone(),
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "No file path specified",
                ));
            }
        };

        let tab = &mut self.tabs[self.active_tab];
        let tab_id = tab.id;
        let save_revision = tab.revision;
        // O(1) buffer clone (Arc-based rope)
        let buffer_clone = tab.buffer.clone();

        let sync = tab
            .sync_state
            .get_or_insert_with(|| {
                Arc::new(TabSyncState {
                    revision: AtomicU64::new(save_revision),
                    is_dirty: AtomicBool::new(tab.is_dirty),
                    journal_lock: Mutex::new(()),
                })
            })
            .clone();
        sync.revision.store(save_revision, Ordering::SeqCst);
        sync.is_dirty.store(true, Ordering::SeqCst);

        let (tx, _) = self
            .save_channel
            .get_or_insert_with(crossbeam_channel::unbounded);
        let tx = tx.clone();

        self.autosave.mark_saved();

        std::thread::spawn(move || {
            let result = match Self::write_buffer_atomically(&path, &buffer_clone) {
                Ok(()) => {
                    let _guard = sync.journal_lock.lock().unwrap_or_else(|e| e.into_inner());
                    if sync.revision.load(Ordering::SeqCst) == save_revision {
                        let _ = Journal::remove(&path);
                        sync.is_dirty.store(false, Ordering::SeqCst);
                    }
                    Ok(())
                }
                Err(e) => Err(e.to_string()),
            };

            let _ = tx.send(AsyncSaveEvent {
                tab_id,
                path,
                save_revision,
                result,
            });
        });

        Ok(())
    }

    pub fn poll_async_saves(&mut self) -> Vec<AsyncSaveEvent> {
        let mut events = Vec::new();
        if let Some((_, ref rx)) = self.save_channel {
            while let Ok(ev) = rx.try_recv() {
                events.push(ev);
            }
        }

        for ev in &events {
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == ev.tab_id) {
                match ev.result {
                    Ok(()) => {
                        if tab.revision == ev.save_revision {
                            tab.is_dirty = false;
                            tab.recovered_from_journal = false;
                            if let Some(ref sync) = tab.sync_state {
                                let _guard =
                                    sync.journal_lock.lock().unwrap_or_else(|e| e.into_inner());
                                if sync.revision.load(Ordering::SeqCst) == ev.save_revision {
                                    sync.is_dirty.store(false, Ordering::SeqCst);
                                    let _ = Journal::remove(&ev.path);
                                }
                            } else {
                                let _ = Journal::remove(&ev.path);
                            }
                        }
                    }
                    Err(_) => {
                        tab.is_dirty = true;
                        if let Some(ref sync) = tab.sync_state {
                            sync.is_dirty.store(true, Ordering::SeqCst);
                        }
                    }
                }
            }
        }

        events
    }

    pub fn check_autosave(&mut self) -> bool {
        if self.autosave.should_autosave(self.is_dirty()) && self.file_path().is_some() {
            self.save_async().is_ok()
        } else {
            false
        }
    }
}
