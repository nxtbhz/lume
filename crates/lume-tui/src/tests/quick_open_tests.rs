use crate::quick_open::{scan_workspace_files, QuickOpenItem, QuickOpenState};
use crate::tests::TestDir;
use std::fs;
use std::path::PathBuf;

#[test]
fn test_scan_workspace_files_excludes_artifacts_and_binaries() {
    let test_dir = TestDir::new("lume_test_scan_workspace");
    let root = test_dir.path();

    // Create directories and files
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("target/debug")).unwrap();
    fs::create_dir_all(root.join(".git/objects")).unwrap();
    fs::create_dir_all(root.join("node_modules/pkg")).unwrap();

    fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
    fs::write(root.join("src/lib.rs"), "pub mod lib;").unwrap();
    fs::write(root.join("target/debug/app.exe"), "binary").unwrap();
    fs::write(root.join("src/icon.png"), "image data").unwrap();
    fs::write(root.join(".git/config"), "git config").unwrap();
    fs::write(root.join("node_modules/pkg/index.js"), "module").unwrap();

    let open_tabs: Vec<PathBuf> = vec![root.join("src/main.rs")];
    let items = scan_workspace_files(root, &open_tabs, 1000);

    let rel_paths: Vec<&str> = items.iter().map(|it| it.relative_path.as_str()).collect();
    assert!(rel_paths.contains(&"src/main.rs"));
    assert!(rel_paths.contains(&"src/lib.rs"));

    // Excluded items
    assert!(!rel_paths.iter().any(|p| p.contains("target")));
    assert!(!rel_paths.iter().any(|p| p.contains(".git")));
    assert!(!rel_paths.iter().any(|p| p.contains("node_modules")));
    assert!(!rel_paths.iter().any(|p| p.ends_with(".png")));
    assert!(!rel_paths.iter().any(|p| p.ends_with(".exe")));

    // Open tab flag
    let main_item = items
        .iter()
        .find(|it| it.relative_path == "src/main.rs")
        .unwrap();
    assert!(main_item.is_open_tab);
}

#[test]
fn test_quick_open_state_filtering_and_navigation() {
    let items = vec![
        QuickOpenItem {
            relative_path: "crates/lume-tui/src/main.rs".to_string(),
            file_name: "main.rs".to_string(),
            full_path: PathBuf::from("/workspace/crates/lume-tui/src/main.rs"),
            is_open_tab: false,
        },
        QuickOpenItem {
            relative_path: "crates/lume-core/src/lib.rs".to_string(),
            file_name: "lib.rs".to_string(),
            full_path: PathBuf::from("/workspace/crates/lume-core/src/lib.rs"),
            is_open_tab: true,
        },
    ];

    let mut state = QuickOpenState::new(items);
    assert_eq!(state.filtered.len(), 2);
    // When query is empty, open tab is ranked first
    assert_eq!(state.filtered[0].item_idx, 1);

    // Type query "main"
    for c in "main".chars() {
        state.handle_char(c);
    }
    assert_eq!(state.query, "main");
    assert_eq!(state.filtered.len(), 1);
    assert_eq!(state.selected_item().unwrap().file_name, "main.rs");

    // Backspace removes characters and restores items
    for _ in 0..4 {
        state.handle_backspace();
    }
    assert_eq!(state.query, "");
    assert_eq!(state.filtered.len(), 2);
}

#[test]
fn test_quick_open_state_paste_and_multibyte_utf8() {
    let items = vec![QuickOpenItem {
        relative_path: "documentos/ações/música_final.txt".to_string(),
        file_name: "música_final.txt".to_string(),
        full_path: PathBuf::from("/workspace/documentos/ações/música_final.txt"),
        is_open_tab: false,
    }];

    let mut state = QuickOpenState::new(items);
    state.handle_paste("música\nsecond line ignored");
    assert_eq!(state.query, "música");
    assert_eq!(state.filtered.len(), 1);

    // Backspace multibyte char
    state.handle_backspace();
    assert_eq!(state.query, "músic");
    assert_eq!(state.filtered.len(), 1);
}

#[test]
fn test_quick_open_navigation_clamping() {
    let items = vec![
        QuickOpenItem {
            relative_path: "a.rs".to_string(),
            file_name: "a.rs".to_string(),
            full_path: PathBuf::from("/workspace/a.rs"),
            is_open_tab: false,
        },
        QuickOpenItem {
            relative_path: "b.rs".to_string(),
            file_name: "b.rs".to_string(),
            full_path: PathBuf::from("/workspace/b.rs"),
            is_open_tab: false,
        },
    ];

    let mut state = QuickOpenState::new(items);
    assert_eq!(state.selected_idx, 0);

    // Moving up at top stays at 0
    state.move_up();
    assert_eq!(state.selected_idx, 0);

    // Move down
    state.move_down(10);
    assert_eq!(state.selected_idx, 1);

    // Move down at bottom stays at 1
    state.move_down(10);
    assert_eq!(state.selected_idx, 1);
}

#[test]
fn test_scan_workspace_files_includes_open_tabs_outside_or_hidden() {
    let test_dir = TestDir::new("lume_test_open_tabs_hidden");
    let root = test_dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    let main_rs = root.join("src/main.rs");
    let hidden_env = root.join(".env");
    let outside_dir = TestDir::new("lume_test_outside");
    let outside_file = outside_dir.path().join("external.rs");

    fs::write(&main_rs, "fn main() {}").unwrap();
    fs::write(&hidden_env, "SECRET=123").unwrap();
    fs::write(&outside_file, "// external").unwrap();

    let open_tabs = vec![hidden_env.clone(), outside_file.clone()];
    let items = scan_workspace_files(root, &open_tabs, 1000);

    let paths: Vec<&str> = items.iter().map(|it| it.relative_path.as_str()).collect();
    assert!(paths.contains(&"src/main.rs"));
    assert!(paths.iter().any(|p| p.ends_with(".env")));
    assert!(paths.iter().any(|p| p.ends_with("external.rs")));

    let env_item = items.iter().find(|it| it.full_path == hidden_env).unwrap();
    assert!(env_item.is_open_tab);

    let ext_item = items
        .iter()
        .find(|it| it.full_path == outside_file)
        .unwrap();
    assert!(ext_item.is_open_tab);
}

#[cfg(unix)]
#[test]
fn test_scan_workspace_files_cycle_protection() {
    let test_dir = TestDir::new("lume_test_symlink_cycle");
    let root = test_dir.path();
    let dir_a = root.join("dir_a");
    fs::create_dir_all(&dir_a).unwrap();
    fs::write(dir_a.join("file.rs"), "// in a").unwrap();

    // Create circular symlink: dir_a/loop -> dir_a
    let link = dir_a.join("loop");
    let _ = std::os::unix::fs::symlink(&dir_a, &link);

    let items = scan_workspace_files(root, &[], 1000);
    assert!(!items.is_empty());
    assert!(items.iter().any(|it| it.file_name == "file.rs"));
}

#[test]
fn test_quick_open_scrolling_and_edge_cases() {
    let items = vec![
        QuickOpenItem {
            relative_path: "1.rs".to_string(),
            file_name: "1.rs".to_string(),
            full_path: PathBuf::from("/workspace/1.rs"),
            is_open_tab: false,
        },
        QuickOpenItem {
            relative_path: "2.rs".to_string(),
            file_name: "2.rs".to_string(),
            full_path: PathBuf::from("/workspace/2.rs"),
            is_open_tab: false,
        },
        QuickOpenItem {
            relative_path: "3.rs".to_string(),
            file_name: "3.rs".to_string(),
            full_path: PathBuf::from("/workspace/3.rs"),
            is_open_tab: false,
        },
    ];

    let mut state = QuickOpenState::new(items);
    // Backspace on empty
    state.handle_backspace();
    assert_eq!(state.query, "");

    // Empty paste
    state.handle_paste("");
    assert_eq!(state.query, "");

    // Move down with max_visible = 1 forces scroll_offset to advance
    state.move_down(1);
    assert_eq!(state.selected_idx, 1);
    assert_eq!(state.scroll_offset, 1);

    state.move_down(1);
    assert_eq!(state.selected_idx, 2);
    assert_eq!(state.scroll_offset, 2);

    // Move up forces scroll_offset to decrease
    state.move_up();
    assert_eq!(state.selected_idx, 1);
    assert_eq!(state.scroll_offset, 1);
}
