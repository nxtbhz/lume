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

/// Searches upwards from `start_path` (up to 32 directory levels) for a `.git`
/// directory or a `.git` worktree pointer file. Returns the path to the resolved `.git` dir.
pub fn find_git_dir(start_path: &Path) -> Option<PathBuf> {
    let mut current = if start_path.is_file() {
        start_path.parent()?
    } else {
        start_path
    };

    let mut depth = 0;
    while depth < 32 {
        let candidate = current.join(".git");
        if candidate.is_dir() {
            return Some(candidate);
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
                        return Some(resolved);
                    }
                }
            }
        }

        match current.parent() {
            Some(parent) if parent != current => {
                current = parent;
                depth += 1;
            }
            _ => break,
        }
    }

    None
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
        assert_eq!(
            GitHead::Branch("main".to_string()).display_name(),
            "main"
        );
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
}
