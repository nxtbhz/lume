use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitHead {
    Branch(String),
    Detached(String), // 7-char short commit hash or tag
}

impl GitHead {
    pub fn display_name(&self) -> &str {
        match self {
            GitHead::Branch(b) => b.as_str(),
            GitHead::Detached(c) => c.as_str(),
        }
    }
}

/// Parses the contents of a `.git/HEAD` file.
pub fn parse_git_head_content(content: &str) -> Option<GitHead> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(branch) = trimmed.strip_prefix("ref: refs/heads/") {
        let branch = branch.trim();
        if !branch.is_empty() {
            return Some(GitHead::Branch(branch.to_string()));
        }
    }

    if let Some(tag) = trimmed.strip_prefix("ref: refs/tags/") {
        let tag = tag.trim();
        if !tag.is_empty() {
            return Some(GitHead::Detached(tag.to_string()));
        }
    }

    // Check for detached commit (40-character hex hash)
    if trimmed.len() >= 40 && trimmed.chars().take(40).all(|c| c.is_ascii_hexdigit()) {
        return Some(GitHead::Detached(trimmed[..7].to_string()));
    }

    None
}

fn normalize_lexical(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut normalized = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !matches!(
                    normalized.components().next_back(),
                    None | Some(Component::RootDir | Component::Prefix(_) | Component::ParentDir)
                ) {
                    normalized.pop();
                } else if normalized.as_os_str().is_empty() {
                    normalized.push(comp);
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(comp);
            }
        }
    }
    normalized
}

fn find_git_root_and_dir(start_path: &Path) -> Option<(PathBuf, PathBuf)> {
    if start_path.as_os_str().is_empty() {
        return None;
    }

    let anchored = if start_path.is_relative() {
        normalize_lexical(&std::env::current_dir().ok()?.join(start_path))
    } else {
        start_path.to_path_buf()
    };

    let mut current = if anchored.is_file() {
        anchored.parent()?.to_path_buf()
    } else {
        anchored
    };

    let mut depth = 0;
    while depth < 32 {
        let candidate = current.join(".git");
        if candidate.is_dir() {
            return Some((current, candidate));
        } else if candidate.is_file() {
            // Worktree or submodule: `.git` file contains `gitdir: <path>`
            if let Ok(content) = fs::read_to_string(&candidate) {
                if let Some(gitdir_str) = content.trim().strip_prefix("gitdir:") {
                    let target_raw = gitdir_str.trim();
                    let target_path = Path::new(target_raw);
                    let resolved = if target_path.is_absolute() {
                        target_path.to_path_buf()
                    } else {
                        current.join(target_path)
                    };
                    if resolved.exists() {
                        return Some((current, resolved));
                    }
                }
            }
        }

        match current.parent() {
            Some(parent) if parent != current => {
                current = parent.to_path_buf();
                depth += 1;
            }
            _ => break,
        }
    }

    None
}

/// Searches upwards from `start_path` (up to 32 directory levels) for a `.git`
/// directory or a `.git` worktree pointer file. Returns the path to the resolved `.git` dir.
pub fn find_git_dir(start_path: &Path) -> Option<PathBuf> {
    find_git_root_and_dir(start_path).map(|(_, git_dir)| git_dir)
}

/// Reads the `HEAD` file inside the resolved `git_path`.
pub fn read_git_head_from_dir(git_path: &Path) -> Option<GitHead> {
    let head_file = git_path.join("HEAD");
    if let Ok(content) = fs::read_to_string(head_file) {
        parse_git_head_content(&content)
    } else {
        None
    }
}

/// High-level zero-cost lookup: finds the git directory from `start_path` and resolves `HEAD`.
pub fn get_git_head_for_path(start_path: &Path) -> Option<GitHead> {
    let git_dir = find_git_dir(start_path)?;
    read_git_head_from_dir(&git_dir)
}

/// Reads the UTF-8 contents of `path` at Git `HEAD`, or `None` if unavailable.
pub fn read_git_head_blob(path: &Path) -> Option<String> {
    if path.as_os_str().is_empty() || path.is_dir() {
        return None;
    }

    let abs_raw = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    let abs_path = normalize_lexical(&abs_raw);
    if abs_path.is_dir() {
        return None;
    }

    let (worktree_root, _) = find_git_root_and_dir(&abs_path)?;
    let rel_path = abs_path.strip_prefix(&worktree_root).ok()?;

    let mut rel_posix = String::new();
    for comp in rel_path.components() {
        if let std::path::Component::Normal(part) = comp {
            if !rel_posix.is_empty() {
                rel_posix.push('/');
            }
            rel_posix.push_str(part.to_str()?);
        }
    }
    if rel_posix.is_empty() {
        return None;
    }

    let spec = format!("HEAD:{}", rel_posix);
    let output = std::process::Command::new("git")
        .arg("show")
        .arg(spec)
        .current_dir(&worktree_root)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::SystemTime;

    #[test]
    fn test_parse_git_head_branch() {
        assert_eq!(
            parse_git_head_content("ref: refs/heads/main\n"),
            Some(GitHead::Branch("main".to_string()))
        );
        assert_eq!(
            parse_git_head_content("ref: refs/heads/feature/zero-cost-git\r\n"),
            Some(GitHead::Branch("feature/zero-cost-git".to_string()))
        );
        assert_eq!(GitHead::Branch("main".to_string()).display_name(), "main");
    }

    #[test]
    fn test_parse_git_head_detached_commit() {
        assert_eq!(
            parse_git_head_content("0123456789abcdef0123456789abcdef01234567\n"),
            Some(GitHead::Detached("0123456".to_string()))
        );
        assert_eq!(
            GitHead::Detached("0123456".to_string()).display_name(),
            "0123456"
        );
    }

    #[test]
    fn test_parse_git_head_detached_tag() {
        assert_eq!(
            parse_git_head_content("ref: refs/tags/v1.0.0\n"),
            Some(GitHead::Detached("v1.0.0".to_string()))
        );
    }

    #[test]
    fn test_parse_git_head_empty_and_invalid() {
        assert_eq!(parse_git_head_content(""), None);
        assert_eq!(parse_git_head_content("   \n"), None);
        assert_eq!(parse_git_head_content("invalid content"), None);
    }

    #[test]
    fn test_find_git_dir_and_read_worktree() {
        let unique_id = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("lume_git_test_{}", unique_id));
        let worktree_dir = temp_dir.join("worktree");
        let fake_common_git = temp_dir.join("main_repo").join(".git");
        let wt_git_dir = fake_common_git.join("worktrees").join("feature-wt");

        fs::create_dir_all(&wt_git_dir).unwrap();
        fs::create_dir_all(&worktree_dir).unwrap();

        fs::write(wt_git_dir.join("HEAD"), "ref: refs/heads/feature-wt\n").unwrap();
        fs::write(
            worktree_dir.join(".git"),
            format!("gitdir: {}\n", wt_git_dir.display()),
        )
        .unwrap();

        let sub_file = worktree_dir.join("src").join("main.rs");
        fs::create_dir_all(sub_file.parent().unwrap()).unwrap();
        fs::write(&sub_file, "fn main() {}\n").unwrap();

        let head = get_git_head_for_path(&sub_file).expect("should resolve git worktree HEAD");
        assert_eq!(head, GitHead::Branch("feature-wt".to_string()));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_non_git_path_returns_none() {
        let temp_dir = std::env::temp_dir().join("lume_non_git_dir");
        fs::create_dir_all(&temp_dir).unwrap();
        assert_eq!(get_git_head_for_path(&temp_dir), None);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_git_head_blob_in_linked_worktree() {
        let unique_id = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root_dir = std::env::temp_dir().join(format!(
            "lume_git_wt_blob_{}_{}",
            std::process::id(),
            unique_id
        ));
        let main_dir = root_dir.join("main");
        let wt_dir = root_dir.join("wt");
        let _ = fs::remove_dir_all(&root_dir);
        fs::create_dir_all(&main_dir).unwrap();

        let run_git = |dir: &Path, args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&out.stderr)
            );
        };

        run_git(&main_dir, &["init"]);
        run_git(&main_dir, &["config", "user.email", "test@lume.local"]);
        run_git(&main_dir, &["config", "user.name", "Lume Test"]);
        fs::create_dir_all(main_dir.join("src")).unwrap();
        fs::write(main_dir.join("src/main.rs"), "fn main() {}\n").unwrap();
        run_git(&main_dir, &["add", "src/main.rs"]);
        run_git(&main_dir, &["commit", "-m", "init"]);
        run_git(
            &main_dir,
            &[
                "worktree",
                "add",
                wt_dir.to_str().unwrap(),
                "-b",
                "wt-branch",
            ],
        );

        let wt_file = wt_dir.join("src/main.rs");
        fs::write(&wt_file, "fn main() { println!(\"dirty\"); }\n").unwrap();
        assert_eq!(
            read_git_head_blob(&wt_file).as_deref(),
            Some("fn main() {}\n")
        );

        let _ = fs::remove_dir_all(&root_dir);
    }

    #[test]
    fn test_find_git_dir_single_component_relative_path() {
        // Cargo executes lume-core unit tests with cwd = crates/lume-core, where "Cargo.toml"
        // is a single-component relative path whose parent() is "".
        let found = find_git_dir(Path::new("Cargo.toml"));
        assert!(
            found.is_some(),
            "find_git_dir(Path::new(\"Cargo.toml\")) should find ancestor .git directory"
        );
    }
}
