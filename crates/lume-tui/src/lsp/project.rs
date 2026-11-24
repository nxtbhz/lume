use std::path::{Component, Path, PathBuf};

use lume_syntax::Language;

use crate::lsp::{LspError, ServerKind};

/// Unique key identifying a language server session by server kind and canonical project root.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProjectKey {
    pub server: ServerKind,
    pub root: PathBuf,
}

/// Maps a file path and its Lume syntax [`Language`] to the responsible server and LSP language ID.
///
/// Only C and C++ files are routed to `clangd`. CUDA (`.cu`/`.cuh`) files are explicitly
/// excluded even though syntax highlighting classifies them as C++.
pub fn server_for(path: &Path, language: Language) -> Option<(ServerKind, &'static str)> {
    if is_cuda_path(path) {
        return None;
    }
    match language {
        Language::C => Some((ServerKind::Clangd, "c")),
        Language::Cpp => Some((ServerKind::Clangd, "cpp")),
        _ => None,
    }
}

fn is_cuda_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cu") || ext.eq_ignore_ascii_case("cuh"))
}

/// Resolves a file path to an absolute, canonicalized path when possible, falling back
/// to canonicalizing the longest existing ancestor and joining any remaining components.
pub(crate) fn canonicalize_best_effort(path: &Path, base_dir: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    };
    if let Ok(canon) = std::fs::canonicalize(&abs) {
        return canon;
    }
    let normalized = normalize_lexical(&abs);
    let mut existing = normalized.as_path();
    let mut tail = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name() else {
            return normalized;
        };
        tail.push(name.to_os_string());
        let Some(parent) = existing.parent() else {
            return normalized;
        };
        existing = parent;
    }
    let mut resolved = std::fs::canonicalize(existing).unwrap_or_else(|_| existing.to_path_buf());
    for part in tail.into_iter().rev() {
        resolved.push(part);
    }
    resolved
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Resolves the canonical project root for `source` under `kind`.
///
/// For `clangd`, searches ancestors of `source` in priority order:
/// 1. Nearest ancestor containing `compile_commands.json`, `build/compile_commands.json`,
///    or `compile_flags.txt`.
/// 2. `explorer_root`, if it contains `source`.
/// 3. Nearest ancestor containing `.git`.
/// 4. Parent directory of `source`.
pub fn project_for(
    source: &Path,
    kind: ServerKind,
    explorer_root: &Path,
) -> Result<ProjectKey, LspError> {
    let canon_explorer =
        std::fs::canonicalize(explorer_root).unwrap_or_else(|_| normalize_lexical(explorer_root));
    let canon_source = canonicalize_best_effort(source, &canon_explorer);
    let source_dir = canon_source.parent().ok_or_else(|| {
        LspError::Project(format!(
            "cannot determine parent directory for '{}'",
            source.display()
        ))
    })?;

    let root = match kind {
        ServerKind::Clangd => resolve_clangd_root(&canon_source, source_dir, &canon_explorer),
    };

    let canon_root = std::fs::canonicalize(&root).unwrap_or(root);
    Ok(ProjectKey {
        server: kind,
        root: canon_root,
    })
}

fn resolve_clangd_root(canon_source: &Path, source_dir: &Path, canon_explorer: &Path) -> PathBuf {
    // 1. Nearest compile database / compile_flags.txt marker
    for dir in source_dir.ancestors() {
        if dir.join("compile_commands.json").is_file()
            || dir.join("build").join("compile_commands.json").is_file()
            || dir.join("compile_flags.txt").is_file()
        {
            return dir.to_path_buf();
        }
    }

    // 2. Explorer root when it contains the source file
    if canon_source.starts_with(canon_explorer) {
        return canon_explorer.to_path_buf();
    }

    // 3. Nearest Git repository root
    for dir in source_dir.ancestors() {
        if dir.join(".git").exists() {
            return dir.to_path_buf();
        }
    }

    // 4. File's immediate parent directory
    source_dir.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::TestDir;

    #[test]
    fn routes_c_cpp_projects_and_rejects_unsupported() {
        // Language mapping for C/C++ with clangd; CUDA, Python, and Rust have no route.
        assert_eq!(
            server_for(Path::new("src/t.c"), Language::C),
            Some((ServerKind::Clangd, "c"))
        );
        assert_eq!(
            server_for(Path::new("include/arch.h"), Language::C),
            Some((ServerKind::Clangd, "c"))
        );
        assert_eq!(
            server_for(Path::new("src/use.cpp"), Language::Cpp),
            Some((ServerKind::Clangd, "cpp"))
        );
        assert_eq!(
            server_for(Path::new("include/arch.hpp"), Language::Cpp),
            Some((ServerKind::Clangd, "cpp"))
        );
        // CUDA files are classified as Language::Cpp by syntax highlighting but excluded from clangd routing.
        assert_eq!(server_for(Path::new("kernels/add.cu"), Language::Cpp), None);
        assert_eq!(
            server_for(Path::new("kernels/add.CUH"), Language::Cpp),
            None
        );
        assert_eq!(server_for(Path::new("script.py"), Language::Python), None);
        assert_eq!(server_for(Path::new("src/main.rs"), Language::Rust), None);

        // Distinct C/C++ roots via compile_commands.json, build/compile_commands.json, compile_flags.txt
        let dir = TestDir::new("lume_lsp_project_roots");
        let root_a = dir.path().join("proj_a");
        let root_b = dir.path().join("proj_b");
        let root_c = dir.path().join("proj_c");
        let root_plain = dir.path().join("plain");
        std::fs::create_dir_all(root_a.join("src")).unwrap();
        std::fs::create_dir_all(root_b.join("build")).unwrap();
        std::fs::create_dir_all(root_b.join("src")).unwrap();
        std::fs::create_dir_all(root_c.join("nested")).unwrap();
        std::fs::create_dir_all(&root_plain).unwrap();

        std::fs::write(root_a.join("compile_commands.json"), "[]").unwrap();
        std::fs::write(root_b.join("build/compile_commands.json"), "[]").unwrap();
        std::fs::write(root_c.join("compile_flags.txt"), "-Iinclude\n").unwrap();

        let file_a = root_a.join("src/t.c");
        let file_b = root_b.join("src/use.cpp");
        let file_c = root_c.join("nested/mod.c");
        let file_plain = root_plain.join("single.c");
        for f in [&file_a, &file_b, &file_c, &file_plain] {
            std::fs::write(f, "int x;\n").unwrap();
        }

        let key_a = project_for(&file_a, ServerKind::Clangd, dir.path()).unwrap();
        let key_b = project_for(&file_b, ServerKind::Clangd, dir.path()).unwrap();
        let key_c = project_for(&file_c, ServerKind::Clangd, dir.path()).unwrap();
        let key_plain = project_for(&file_plain, ServerKind::Clangd, dir.path()).unwrap();

        assert_eq!(key_a.root, root_a);
        assert_eq!(key_b.root, root_b);
        assert_eq!(key_c.root, root_c);
        assert_ne!(key_a, key_b);
        // No marker inside explorer_root -> falls back to explorer_root
        assert_eq!(key_plain.root, dir.path());

        // Outside explorer_root: Git root wins before file's parent directory
        let ext_dir = TestDir::new("lume_lsp_ext_git");
        let git_repo = ext_dir.path().join("repo");
        std::fs::create_dir_all(git_repo.join(".git")).unwrap();
        std::fs::create_dir_all(git_repo.join("src")).unwrap();
        let ext_git_file = git_repo.join("src/ext.c");
        std::fs::write(&ext_git_file, "int y;\n").unwrap();

        let key_git = project_for(&ext_git_file, ServerKind::Clangd, dir.path()).unwrap();
        assert_eq!(key_git.root, git_repo);

        // Outside explorer_root and outside any git repo -> file parent directory
        let loose_dir = ext_dir.path().join("loose");
        std::fs::create_dir_all(&loose_dir).unwrap();
        let loose_file = loose_dir.join("solo.h");
        std::fs::write(&loose_file, "struct S;\n").unwrap();
        let key_loose = project_for(&loose_file, ServerKind::Clangd, dir.path()).unwrap();
        assert_eq!(key_loose.root, loose_dir);
    }
}
