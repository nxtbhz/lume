use crate::buffer::{RopeBuffer, TextBuffer, TextDelta};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Crash recovery journal: records unsaved buffer deltas/snapshots to disk.
/// If a crash (e.g. kill -9) occurs, the file's unsaved edits can be fully restored.
pub struct Journal;

const MAGIC: &[u8; 5] = b"LUMJ\x01";
const MAGIC_V2: &[u8; 5] = b"LUMJ\x02";

impl Journal {
    pub fn journal_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("LUME_JOURNAL_DIR") {
            PathBuf::from(dir)
        } else if let Ok(cache) = std::env::var("XDG_CACHE_HOME") {
            PathBuf::from(cache).join("lume/journal")
        } else {
            std::env::temp_dir().join("lume_journal")
        }
    }

    fn journal_path(file_path: &Path) -> PathBuf {
        let dir = Self::journal_dir();
        // Generate stable filename based on path hash
        let hash = path_hash(file_path);
        let name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed");
        dir.join(format!("{:016x}_{}.journal", hash, name))
    }

    /// Record append-only binary deltas to the crash recovery journal
    pub fn record_deltas(file_path: &Path, deltas: &[TextDelta]) -> io::Result<()> {
        if deltas.is_empty() {
            return Ok(());
        }
        let path = Self::journal_path(file_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let has_v2_header = fs::File::open(&path)
            .and_then(|mut f| {
                use std::io::Read;
                let mut hdr = [0u8; 5];
                f.read_exact(&mut hdr)?;
                Ok(&hdr == MAGIC_V2)
            })
            .unwrap_or(false);

        let mut file = if !has_v2_header {
            let base_bytes = if path.exists() {
                Self::recover(file_path)
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
                .open(&path)?;
            f.write_all(MAGIC_V2)?;
            f.write_all(&(base_bytes.len() as u64).to_le_bytes())?;
            f.write_all(&base_bytes)?;
            f
        } else {
            OpenOptions::new().create(true).append(true).open(&path)?
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

        Ok(())
    }

    /// Record full content snapshot directly with MAGIC_V2 header
    pub fn record(file_path: &Path, content: &str) -> io::Result<()> {
        let path = Self::journal_path(file_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = content.as_bytes();
        let mut buf = Vec::with_capacity(MAGIC_V2.len() + 8 + bytes.len());
        buf.extend_from_slice(MAGIC_V2);
        buf.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        buf.extend_from_slice(bytes);
        fs::write(&path, buf)
    }

    pub fn has_journal(file_path: &Path) -> bool {
        let path = Self::journal_path(file_path);
        path.exists()
    }

    /// Recover content from journal, applying binary deltas if present
    pub fn recover(file_path: &Path) -> io::Result<Option<String>> {
        let path = Self::journal_path(file_path);
        if !path.exists() {
            return Ok(None);
        }

        let bytes = fs::read(&path)?;
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
                let _timestamp =
                    u64::from_le_bytes(cursor[new_len..new_len + 8].try_into().unwrap());
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

    pub fn remove(file_path: &Path) -> io::Result<()> {
        let path = Self::journal_path(file_path);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
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

fn path_hash(path: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let resolved = resolve_path(path);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    resolved.to_string_lossy().hash(&mut hasher);
    hasher.finish()
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
}
