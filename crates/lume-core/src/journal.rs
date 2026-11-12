use crate::buffer::{RopeBuffer, TextBuffer, TextDelta};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Message operations processed asynchronously by the background JournalWorker.
#[derive(Debug)]
pub enum JournalOp {
    AppendDeltas {
        path: PathBuf,
        journal_path: PathBuf,
        deltas: Vec<TextDelta>,
    },
    Snapshot {
        path: PathBuf,
        journal_path: PathBuf,
        rope: ropey::Rope,
    },
    Remove {
        journal_path: PathBuf,
    },
    Flush {
        responder: Sender<()>,
    },
}

static JOURNAL_SENDER: OnceLock<Sender<JournalOp>> = OnceLock::new();

/// Singleton background worker processing journal disk I/O off the UI thread.
pub struct JournalWorker;

impl JournalWorker {
    pub fn sender() -> &'static Sender<JournalOp> {
        JOURNAL_SENDER.get_or_init(|| {
            let (tx, rx) = unbounded::<JournalOp>();
            std::thread::Builder::new()
                .name("lume-journal-worker".to_string())
                .spawn(move || {
                    journal_worker_loop(rx);
                })
                .expect("failed to spawn lume-journal-worker thread");
            tx
        })
    }
}

/// Coalesce queued snapshots per journal path, keeping only the latest snapshot
/// before any subsequent append, remove, or flush barrier.
pub fn coalesce_ops(ops: Vec<JournalOp>) -> Vec<JournalOp> {
    if ops.len() <= 1 {
        return ops;
    }

    if !ops
        .iter()
        .any(|op| matches!(op, JournalOp::Snapshot { .. }))
    {
        return ops;
    }

    let mut keep = vec![true; ops.len()];
    let mut pending_snapshot_paths: HashSet<PathBuf> = HashSet::new();

    for (i, op) in ops.iter().enumerate().rev() {
        match op {
            JournalOp::Flush { .. } => {
                pending_snapshot_paths.clear();
            }
            JournalOp::Remove { journal_path } => {
                pending_snapshot_paths.remove(journal_path);
            }
            JournalOp::AppendDeltas { journal_path, .. } => {
                pending_snapshot_paths.remove(journal_path);
            }
            JournalOp::Snapshot { journal_path, .. } => {
                if pending_snapshot_paths.contains(journal_path) {
                    keep[i] = false;
                } else {
                    pending_snapshot_paths.insert(journal_path.clone());
                }
            }
        }
    }

    ops.into_iter()
        .zip(keep)
        .filter_map(|(op, should_keep)| if should_keep { Some(op) } else { None })
        .collect()
}

fn journal_worker_loop(rx: Receiver<JournalOp>) {
    while let Ok(first_op) = rx.recv() {
        let mut batch = Vec::new();
        batch.push(first_op);
        while let Ok(op) = rx.try_recv() {
            batch.push(op);
        }

        let coalesced = coalesce_ops(batch);
        for op in coalesced {
            match op {
                JournalOp::AppendDeltas {
                    path,
                    journal_path,
                    deltas,
                } => {
                    let _ = record_deltas_internal(&path, &journal_path, &deltas);
                }
                JournalOp::Snapshot {
                    path: _,
                    journal_path,
                    rope,
                } => {
                    let _ = record_snapshot_internal(&journal_path, &rope);
                }
                JournalOp::Remove { journal_path } => {
                    if journal_path.exists() {
                        let _ = fs::remove_file(&journal_path);
                    }
                }
                JournalOp::Flush { responder } => {
                    let _ = responder.send(());
                }
            }
        }
    }
}

/// Crash recovery journal: records unsaved buffer deltas/snapshots to disk.
/// If a crash (e.g. kill -9) occurs, the file's unsaved edits can be fully restored.
pub struct Journal;

const MAGIC: &[u8; 5] = b"LUMJ\x01";
const MAGIC_V2: &[u8; 5] = b"LUMJ\x02";

impl Journal {
    pub fn journal_dir() -> PathBuf {
        let dir = if let Ok(dir) = std::env::var("LUME_JOURNAL_DIR") {
            PathBuf::from(dir)
        } else if let Ok(cache) = std::env::var("XDG_CACHE_HOME") {
            PathBuf::from(cache).join("lume/journal")
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".cache/lume/journal")
        } else {
            std::env::temp_dir().join(format!("lume_journal_{}", std::process::id()))
        };

        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            builder.mode(0o700);
            let _ = builder.create(&dir);
        }
        #[cfg(not(unix))]
        {
            let _ = fs::create_dir_all(&dir);
        }

        dir
    }

    pub fn journal_path(file_path: &Path) -> PathBuf {
        let dir = Self::journal_dir();
        // Generate stable filename based on deterministic FNV-1a path hash
        let hash = path_hash(file_path);
        let name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed");
        dir.join(format!("{:016x}_{}.journal", hash, name))
    }

    /// Asynchronously enqueues binary deltas to be recorded by the background JournalWorker.
    pub fn enqueue_deltas(path: PathBuf, journal_path: PathBuf, deltas: Vec<TextDelta>) {
        if deltas.is_empty() {
            return;
        }
        let _ = JournalWorker::sender().send(JournalOp::AppendDeltas {
            path,
            journal_path,
            deltas,
        });
    }

    /// Asynchronously enqueues a full buffer snapshot to be recorded by the background JournalWorker.
    pub fn enqueue_snapshot(path: PathBuf, journal_path: PathBuf, rope: ropey::Rope) {
        let _ = JournalWorker::sender().send(JournalOp::Snapshot {
            path,
            journal_path,
            rope,
        });
    }

    /// Asynchronously enqueues removal of a journal file by the background JournalWorker.
    pub fn enqueue_remove(journal_path: PathBuf) {
        let _ = JournalWorker::sender().send(JournalOp::Remove { journal_path });
    }

    /// FIFO flush barrier: blocks until all previously enqueued journal operations
    /// have been completely written to disk and synced via sync_data().
    pub fn flush() {
        if let Some(tx) = JOURNAL_SENDER.get() {
            let (resp_tx, resp_rx) = bounded(1);
            if tx.send(JournalOp::Flush { responder: resp_tx }).is_ok() {
                let _ = resp_rx.recv();
            }
        }
    }

    /// Record append-only binary deltas to the crash recovery journal
    pub fn record_deltas(file_path: &Path, deltas: &[TextDelta]) -> io::Result<()> {
        let path = Self::journal_path(file_path);
        Self::enqueue_deltas(file_path.to_path_buf(), path, deltas.to_vec());
        Self::flush();
        Ok(())
    }

    /// Fast-path: record append-only binary deltas using a pre-computed journal path
    pub fn record_deltas_with_journal_path(
        file_path: &Path,
        journal_path: &Path,
        deltas: &[TextDelta],
    ) -> io::Result<()> {
        record_deltas_internal(file_path, journal_path, deltas)
    }

    /// Record full content snapshot directly with MAGIC_V2 header
    pub fn record(file_path: &Path, content: &str) -> io::Result<()> {
        let path = Self::journal_path(file_path);
        Self::enqueue_snapshot(
            file_path.to_path_buf(),
            path,
            ropey::Rope::from_str(content),
        );
        Self::flush();
        Ok(())
    }

    /// Fast-path: record full content snapshot with pre-computed journal path
    pub fn record_with_journal_path(journal_path: &Path, content: &str) -> io::Result<()> {
        record_snapshot_internal(journal_path, &ropey::Rope::from_str(content))
    }

    pub fn has_journal(file_path: &Path) -> bool {
        Self::flush();
        let path = Self::journal_path(file_path);
        path.exists()
    }

    /// Check whether a crash recovery journal exists and is fresh relative to the disk file.
    /// Returns `false` if the journal is missing or if the disk file `mtime` is strictly newer
    /// than the journal `mtime` (indicating changes occurred on disk after the journal was written).
    pub fn is_fresh_for(file_path: &Path) -> bool {
        Self::flush();
        let journal_path = Self::journal_path(file_path);
        Self::is_fresh_with_journal_path(file_path, &journal_path).unwrap_or(false)
    }

    /// Check journal freshness with pre-computed journal path, returning `io::Result<bool>`.
    pub fn is_fresh_with_journal_path(file_path: &Path, journal_path: &Path) -> io::Result<bool> {
        Self::flush();
        if !journal_path.exists() {
            return Ok(false);
        }
        if !file_path.exists() {
            return Ok(true);
        }

        let disk_mtime = fs::metadata(file_path)?.modified()?;
        let journal_mtime = fs::metadata(journal_path)?.modified()?;

        Ok(journal_mtime >= disk_mtime)
    }

    /// Alias taking `(path, journal_path)` to satisfy both 1-arg and 2-arg caller conventions.
    pub fn is_fresh_for_paths(path: &Path, journal_path: &Path) -> io::Result<bool> {
        Self::is_fresh_with_journal_path(path, journal_path)
    }

    /// Recover content from journal, applying binary deltas if present
    pub fn recover(file_path: &Path) -> io::Result<Option<String>> {
        Self::flush();
        let path = Self::journal_path(file_path);
        recover_internal(file_path, &path)
    }

    pub fn remove(file_path: &Path) -> io::Result<()> {
        let path = Self::journal_path(file_path);
        Self::enqueue_remove(path);
        Self::flush();
        Ok(())
    }
}

fn record_deltas_internal(
    file_path: &Path,
    journal_path: &Path,
    deltas: &[TextDelta],
) -> io::Result<()> {
    if deltas.is_empty() {
        return Ok(());
    }
    if let Some(parent) = journal_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let has_v2_header = fs::File::open(journal_path)
        .and_then(|mut f| {
            use std::io::Read;
            let mut hdr = [0u8; 5];
            f.read_exact(&mut hdr)?;
            Ok(&hdr == MAGIC_V2)
        })
        .unwrap_or(false);

    let mut file = if !has_v2_header {
        let base_bytes = if journal_path.exists() {
            recover_internal(file_path, journal_path)
                .ok()
                .flatten()
                .map(|s| s.into_bytes())
                .unwrap_or_else(|| {
                    if file_path.exists() {
                        fs::read(file_path).unwrap_or_default()
                    } else {
                        Vec::new()
                    }
                })
        } else if file_path.exists() {
            fs::read(file_path).unwrap_or_default()
        } else {
            Vec::new()
        };

        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(journal_path)?;
        f.write_all(MAGIC_V2)?;
        f.write_all(&(base_bytes.len() as u64).to_le_bytes())?;
        f.write_all(&base_bytes)?;
        f
    } else {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(journal_path)?
    };

    for delta in deltas {
        let old_bytes = delta.old_text.as_bytes();
        let new_bytes = delta.new_text.as_bytes();

        file.write_all(&(delta.start_byte as u64).to_le_bytes())?;
        file.write_all(&(old_bytes.len() as u32).to_le_bytes())?;
        file.write_all(old_bytes)?;
        file.write_all(&(new_bytes.len() as u32).to_le_bytes())?;
        file.write_all(new_bytes)?;
        file.write_all(&delta.timestamp.to_le_bytes())?;
    }

    file.sync_data()?;
    Ok(())
}

fn record_snapshot_internal(journal_path: &Path, rope: &ropey::Rope) -> io::Result<()> {
    if let Some(parent) = journal_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(journal_path)?;
    file.write_all(MAGIC_V2)?;
    file.write_all(&(rope.len_bytes() as u64).to_le_bytes())?;
    for chunk in rope.chunks() {
        file.write_all(chunk.as_bytes())?;
    }
    file.sync_data()?;
    Ok(())
}

fn recover_internal(file_path: &Path, path: &Path) -> io::Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }

    let bytes = fs::read(path)?;
    if bytes.is_empty() {
        return Ok(None);
    }

    if bytes.starts_with(MAGIC_V2) || bytes.starts_with(MAGIC) {
        let (base_text, mut cursor) = if bytes.starts_with(MAGIC_V2) {
            let after_magic = &bytes[MAGIC_V2.len()..];
            if after_magic.len() < 8 {
                return Ok(None);
            }
            let base_len = u64::from_le_bytes(after_magic[..8].try_into().unwrap()) as usize;
            let rest = &after_magic[8..];
            if rest.len() < base_len {
                return Ok(None);
            }
            let base = String::from_utf8_lossy(&rest[..base_len]).to_string();
            (base, &rest[base_len..])
        } else {
            let base = if file_path.exists() {
                fs::read_to_string(file_path).unwrap_or_default()
            } else {
                String::new()
            };
            (base, &bytes[MAGIC.len()..])
        };

        let mut buffer = RopeBuffer::from_str(&base_text);

        while cursor.len() >= 12 {
            let start_byte = u64::from_le_bytes(cursor[..8].try_into().unwrap()) as usize;
            let old_len = u32::from_le_bytes(cursor[8..12].try_into().unwrap()) as usize;
            cursor = &cursor[12..];

            if cursor.len() < old_len + 4 {
                break;
            }
            let _old_bytes = &cursor[..old_len];
            cursor = &cursor[old_len..];

            let new_len = u32::from_le_bytes(cursor[..4].try_into().unwrap()) as usize;
            cursor = &cursor[4..];

            if cursor.len() < new_len + 8 {
                break;
            }
            let new_bytes = &cursor[..new_len];
            let _timestamp = u64::from_le_bytes(cursor[new_len..new_len + 8].try_into().unwrap());
            cursor = &cursor[new_len + 8..];

            if old_len > 0 {
                let _ = buffer.delete(start_byte, start_byte + old_len);
            }
            if new_len > 0 {
                if let Ok(new_str) = std::str::from_utf8(new_bytes) {
                    let _ = buffer.insert(start_byte, new_str);
                }
            }
        }

        let text = buffer.slice_to_cow(0, buffer.len_bytes()).to_string();
        Ok(Some(text))
    } else {
        let content = String::from_utf8_lossy(&bytes).to_string();
        Ok(Some(content))
    }
}

fn resolve_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        let abs = std::env::current_dir()
            .map(|d| d.join(path))
            .unwrap_or_else(|_| path.to_path_buf());
        if let (Some(parent), Some(name)) = (abs.parent(), abs.file_name()) {
            if let Ok(canon_parent) = parent.canonicalize() {
                return canon_parent.join(name);
            }
        }
        abs
    })
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn path_hash(path: &Path) -> u64 {
    let resolved = resolve_path(path);
    fnv1a_64(resolved.to_string_lossy().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_journal_record_recover_remove() {
        let target = std::env::temp_dir().join(format!("lume_test_code_{}.c", std::process::id()));
        let _ = Journal::remove(&target);
        assert!(!Journal::has_journal(&target));

        Journal::record(&target, "int main() { return 42; }").unwrap();
        assert!(Journal::has_journal(&target));

        let recovered = Journal::recover(&target).unwrap().unwrap();
        assert_eq!(recovered, "int main() { return 42; }");

        Journal::remove(&target).unwrap();
        assert!(!Journal::has_journal(&target));
    }

    #[test]
    fn test_journal_relative_path_resolution() {
        let rel = Path::new("Cargo.toml");
        let abs = std::env::current_dir().unwrap().join("Cargo.toml");
        assert_eq!(path_hash(rel), path_hash(&abs));
    }

    #[test]
    fn test_journal_record_then_record_deltas_does_not_corrupt() {
        let dir = std::env::temp_dir().join(format!(
            "lume_test_journal_undo_corrupt_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("code.rs");
        fs::write(&file_path, "fn main() {}\n").unwrap();
        let _ = Journal::remove(&file_path);

        // Simulate Editor::undo() calling Journal::record with full snapshot
        Journal::record(&file_path, "fn main() {}\n").unwrap();

        // Simulate subsequent edit calling Journal::record_deltas
        let delta = TextDelta {
            start_byte: 12,
            old_text: String::new(),
            new_text: "// ok".to_string(),
            timestamp: 100,
        };
        Journal::record_deltas(&file_path, &[delta]).unwrap();

        let recovered = Journal::recover(&file_path)
            .unwrap()
            .expect("should recover");
        assert_eq!(recovered, "fn main() {}// ok\n");

        let _ = Journal::remove(&file_path);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_journal_legacy_plain_text_upgraded_on_record_deltas() {
        let dir =
            std::env::temp_dir().join(format!("lume_test_journal_legacy_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("legacy.rs");
        fs::write(&file_path, "disk\n").unwrap();
        let _ = Journal::remove(&file_path);

        // Write raw plain text directly to the journal path (simulating legacy format)
        let jpath = Journal::journal_path(&file_path);
        if let Some(parent) = jpath.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&jpath, "legacy snapshot\n").unwrap();

        let delta = TextDelta {
            start_byte: 15,
            old_text: String::new(),
            new_text: "!".to_string(),
            timestamp: 101,
        };
        Journal::record_deltas(&file_path, &[delta]).unwrap();

        let recovered = Journal::recover(&file_path)
            .unwrap()
            .expect("should recover");
        assert_eq!(recovered, "legacy snapshot!\n");

        let _ = Journal::remove(&file_path);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_coalesce_ops_collapses_consecutive_snapshots_for_same_path() {
        let p1 = PathBuf::from("/tmp/test1.txt");
        let j1 = PathBuf::from("/tmp/test1.journal");
        let p2 = PathBuf::from("/tmp/test2.txt");
        let j2 = PathBuf::from("/tmp/test2.journal");

        let ops = vec![
            JournalOp::Snapshot {
                path: p1.clone(),
                journal_path: j1.clone(),
                rope: ropey::Rope::from_str("v1"),
            },
            JournalOp::Snapshot {
                path: p2.clone(),
                journal_path: j2.clone(),
                rope: ropey::Rope::from_str("w1"),
            },
            JournalOp::Snapshot {
                path: p1.clone(),
                journal_path: j1.clone(),
                rope: ropey::Rope::from_str("v2"),
            },
            JournalOp::Snapshot {
                path: p1.clone(),
                journal_path: j1.clone(),
                rope: ropey::Rope::from_str("v3"),
            },
        ];

        let coalesced = coalesce_ops(ops);
        assert_eq!(coalesced.len(), 2);
        match &coalesced[0] {
            JournalOp::Snapshot {
                journal_path, rope, ..
            } => {
                assert_eq!(journal_path, &j2);
                assert_eq!(rope.to_string(), "w1");
            }
            _ => panic!("expected snapshot"),
        }
        match &coalesced[1] {
            JournalOp::Snapshot {
                journal_path, rope, ..
            } => {
                assert_eq!(journal_path, &j1);
                assert_eq!(rope.to_string(), "v3");
            }
            _ => panic!("expected snapshot"),
        }
    }

    #[test]
    fn test_coalesce_ops_preserves_barriers() {
        let p = PathBuf::from("/tmp/barrier.txt");
        let j = PathBuf::from("/tmp/barrier.journal");
        let (tx, _) = crossbeam_channel::bounded(1);

        let ops = vec![
            JournalOp::Snapshot {
                path: p.clone(),
                journal_path: j.clone(),
                rope: ropey::Rope::from_str("v1"),
            },
            JournalOp::Flush { responder: tx },
            JournalOp::Snapshot {
                path: p.clone(),
                journal_path: j.clone(),
                rope: ropey::Rope::from_str("v2"),
            },
        ];

        let coalesced = coalesce_ops(ops);
        assert_eq!(coalesced.len(), 3);
    }

    #[test]
    fn test_async_worker_flush_guarantees_durability() {
        let dir = std::env::temp_dir().join(format!("lume_worker_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("worker_test.rs");
        let journal_path = Journal::journal_path(&file_path);

        Journal::enqueue_snapshot(
            file_path.clone(),
            journal_path.clone(),
            ropey::Rope::from_str("fn worker() {}\n"),
        );
        Journal::flush();

        assert!(journal_path.exists());
        let recovered = Journal::recover(&file_path)
            .unwrap()
            .expect("recovered text");
        assert_eq!(recovered, "fn worker() {}\n");

        Journal::enqueue_remove(journal_path.clone());
        Journal::flush();
        assert!(!journal_path.exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
