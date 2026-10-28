use crossterm::style::Color;

use crate::explorer::FileExplorer;
use crate::render::explorer::{
    char_display_width, display_width, get_file_color, get_file_icon, truncate_to_width,
};
use crate::tests::TestDir;

#[test]
fn test_explorer_creation_and_refresh() {
    let current = std::env::current_dir().unwrap();
    let explorer = FileExplorer::new(current);
    assert!(!explorer.items.is_empty());
    assert!(explorer.is_visible);
}

#[test]
fn test_explorer_unicode_icon_border_alignment() {
    let content_w: usize = 25;
    let test_names = [
        ("src", true, true),
        ("crates", true, false),
        ("main.rs", false, false),
        ("Cargo.toml", false, false),
        ("README.md", false, false),
        ("boot.s", false, false),
        ("header.h", false, false),
    ];

    for (name, is_dir, is_expanded) in test_names {
        let icon = if is_dir {
            if is_expanded {
                "▼ "
            } else {
                "▶ "
            }
        } else {
            get_file_icon(name)
        };
        let prefix = format!("  {}", icon);
        let avail_name = content_w.saturating_sub(display_width(&prefix));
        let name_truncated = truncate_to_width(name, avail_name);
        let line_text = format!("{}{}", prefix, name_truncated);
        let line_fill = content_w.saturating_sub(display_width(&line_text));
        let formatted = format!("{}{:fill$}│", line_text, "", fill = line_fill);
        assert_eq!(
            display_width(&formatted),
            content_w + 1,
            "Row for '{}' had misaligned border",
            name
        );
    }
}

#[test]
fn test_create_single_and_nested_folder() {
    let test_dir = TestDir::new("lume_test_mkdir");
    let tmp_dir = test_dir.path();

    let mut explorer = FileExplorer::new(tmp_dir.to_path_buf());
    let root_canon = std::fs::canonicalize(tmp_dir).unwrap();

    // 1. Create a single folder in root
    let created_single = explorer.create_folder(&root_canon, "alpha").unwrap();
    assert!(created_single.exists());
    assert!(created_single.is_dir());
    assert!(explorer
        .items
        .iter()
        .any(|item| item.name == "alpha" && item.is_dir));

    // 2. Create nested folders inside alpha: alpha/beta/gamma
    let created_nested = explorer
        .create_folder(&created_single, "beta/gamma")
        .unwrap();
    assert!(created_nested.exists());
    assert!(created_nested.is_dir());

    // Both beta and gamma's parents must be expanded
    assert!(explorer.expanded_dirs.contains(&created_single));
    let beta_path = created_single.join("beta");
    assert!(explorer.expanded_dirs.contains(&beta_path));

    // Selection should point to the new directory
    let sel_item = explorer.selected_item().expect("Must have selected item");
    assert_eq!(sel_item.name, "gamma");
}

#[test]
fn test_create_folder_prevents_escaping_root() {
    let test_dir = TestDir::new("lume_test_escape");
    let tmp_dir = test_dir.path();

    let mut explorer = FileExplorer::new(tmp_dir.to_path_buf());
    let root_canon = std::fs::canonicalize(tmp_dir).unwrap();

    let res = explorer.create_folder(&root_canon, "../outside_secret");
    assert!(res.is_err(), "Must reject path traversal escaping root");
}

#[test]
fn test_delete_folder_recursive_and_prunes_expanded() {
    let test_dir = TestDir::new("lume_test_rmdir");
    let tmp_dir = test_dir.path();

    let mut explorer = FileExplorer::new(tmp_dir.to_path_buf());
    let root_canon = std::fs::canonicalize(tmp_dir).unwrap();

    // Create folder with contents: parent/child/grandchild
    let grandchild = explorer
        .create_folder(&root_canon, "parent/child/grandchild")
        .unwrap();
    let file_path = grandchild.join("nested.txt");
    std::fs::write(&file_path, "hello nested").unwrap();
    explorer.refresh();

    let parent_path = root_canon.join("parent");
    let child_path = parent_path.join("child");
    assert!(explorer.expanded_dirs.contains(&parent_path));
    assert!(explorer.expanded_dirs.contains(&child_path));

    // Delete parent folder
    let deleted = explorer.delete_folder(&parent_path).unwrap();
    assert_eq!(deleted, parent_path);
    assert!(!parent_path.exists());

    // Expanded dirs must have pruned parent, child, grandchild
    assert!(!explorer.expanded_dirs.contains(&parent_path));
    assert!(!explorer.expanded_dirs.contains(&child_path));
    assert!(!explorer.expanded_dirs.contains(&grandchild));

    // Items should be empty or clamped
    assert!(!explorer
        .items
        .iter()
        .any(|it| it.path.starts_with(&parent_path)));
}

#[test]
fn test_delete_root_folder_prevented() {
    let test_dir = TestDir::new("lume_test_root_del");
    let tmp_dir = test_dir.path();

    let mut explorer = FileExplorer::new(tmp_dir.to_path_buf());
    let root_canon = std::fs::canonicalize(tmp_dir).unwrap();

    let res = explorer.delete_folder(&root_canon);
    assert!(res.is_err());
    assert_eq!(
        res.unwrap_err().kind(),
        std::io::ErrorKind::PermissionDenied
    );
    assert!(tmp_dir.exists());
}

#[test]
fn test_create_folder_prevents_absolute_and_prefix_paths() {
    let test_dir = TestDir::new("lume_test_abs");
    let tmp_dir = test_dir.path();

    let mut explorer = FileExplorer::new(tmp_dir.to_path_buf());
    let root_canon = std::fs::canonicalize(tmp_dir).unwrap();

    let res = explorer.create_folder(&root_canon, "/absolute_outside");
    assert!(res.is_err(), "Must reject absolute paths");
}

#[test]
fn test_explorer_delete_preserves_scroll_offset_no_blank_sidebar() {
    let test_dir = TestDir::new("lume_test_scroll");
    let tmp_dir = test_dir.path();

    let mut explorer = FileExplorer::new(tmp_dir.to_path_buf());
    let root_canon = std::fs::canonicalize(tmp_dir).unwrap();

    // Create 10 directories
    for i in 0..10 {
        explorer
            .create_folder(&root_canon, format!("dir_{:02}", i))
            .unwrap();
    }
    assert_eq!(explorer.items.len(), 10);

    // Simulate scrolled viewport
    explorer.scroll_offset = 8;
    explorer.selected_idx = 9;

    // Delete multiple directories to shrink items to 3
    for i in 3..10 {
        let p = root_canon.join(format!("dir_{:02}", i));
        explorer.delete_folder(&p).unwrap();
    }
    assert_eq!(explorer.items.len(), 3);

    // scroll_offset must be clamped so it does not render a blank sidebar
    assert!(
        explorer.scroll_offset < explorer.items.len(),
        "scroll_offset was not clamped!"
    );
    assert!(
        explorer.scroll_offset <= explorer.selected_idx,
        "scroll_offset must not exceed selected_idx"
    );
}

#[test]
fn test_explorer_file_colors_and_char_width() {
    assert_ne!(get_file_color("main.rs"), Color::White);
    assert_ne!(get_file_color("main.c"), Color::White);
    assert_ne!(get_file_color("main.cpp"), Color::White);
    assert_ne!(get_file_color("script.py"), Color::White);
    assert_ne!(get_file_color("App.java"), Color::White);
    assert_ne!(get_file_color("index.js"), Color::White);
    assert_ne!(get_file_color("index.ts"), Color::White);
    assert_ne!(get_file_color("boot.asm"), Color::White);
    assert_ne!(get_file_color("boot.s"), Color::White);
    assert_ne!(get_file_color("Cargo.toml"), Color::White);
    assert_ne!(get_file_color("data.json"), Color::White);
    assert_ne!(get_file_color("config.yaml"), Color::White);
    assert_ne!(get_file_color("README.md"), Color::White);
    assert_eq!(get_file_color("Makefile"), Color::White);
    assert_eq!(get_file_color("no_ext"), Color::White);

    assert_eq!(char_display_width('a'), 1);
    assert_eq!(char_display_width('\0'), 0);
    assert_eq!(char_display_width('\u{0080}'), 0);
}

#[test]
fn test_explorer_navigation_click_and_render() {
    let test_dir = TestDir::new("lume_test_nav");
    let tmp_dir = test_dir.path();
    std::fs::create_dir_all(tmp_dir.join("sub_dir")).unwrap();
    std::fs::write(tmp_dir.join("sub_dir/inner.txt"), "hello").unwrap();
    std::fs::write(tmp_dir.join("root.txt"), "world").unwrap();

    let mut explorer = FileExplorer::new(tmp_dir.to_path_buf());
    assert!(!explorer.items.is_empty());

    // Navigation
    explorer.move_down(10);
    assert!(explorer.selected_idx > 0 || explorer.items.len() == 1);
    explorer.move_up();

    // Selection queries
    assert!(explorer.selected_item().is_some());
    let _target = explorer.target_folder_for_creation();

    // Activate directory to expand, then activate again to collapse
    assert!(explorer.activate_selected().is_none());
    assert!(explorer.activate_selected().is_none());

    // Move to file and activate
    explorer.move_down(2);
    let _activated_file = explorer.activate_selected();

    // Expansion / Collapse
    explorer.expand_selected();
    explorer.collapse_selected();

    // Test collapse on nested item to jump to parent
    explorer.expand_selected();
    explorer.move_down(1);
    explorer.collapse_selected();

    // Empty explorer edge cases
    let mut empty_explorer = FileExplorer::new(tmp_dir.join("non_existent"));
    empty_explorer.items.clear();
    empty_explorer.move_up();
    empty_explorer.move_down(5);
    assert!(empty_explorer.activate_selected().is_none());
    empty_explorer.collapse_selected();
    empty_explorer.expand_selected();

    // Click at row
    let _ = explorer.click_at(0);
    let out_of_bounds = explorer.click_at(999);
    assert!(out_of_bounds.is_none());

    // Render explorer
    let mut buf = Vec::new();
    assert!(explorer.render(&mut buf, 0, 24, true).is_ok());
    assert!(!buf.is_empty());

    buf.clear();
    assert!(explorer.render(&mut buf, 0, 24, false).is_ok());

    // Invisible render
    explorer.is_visible = false;
    buf.clear();
    assert!(explorer.render(&mut buf, 0, 24, false).is_ok());
    assert!(buf.is_empty());

    // Zero height render
    explorer.is_visible = true;
    buf.clear();
    assert!(explorer.render(&mut buf, 0, 0, false).is_ok());
    assert!(buf.is_empty());
}

#[test]
fn test_create_file_in_root_and_nested() {
    let test_dir = TestDir::new("lume_test_create_file");
    let mut explorer = FileExplorer::new(test_dir.path().to_path_buf());

    // Create file in root
    let created = explorer.create_file(test_dir.path(), "hello.txt").unwrap();
    assert!(created.exists());
    assert_eq!(created.file_name().unwrap(), "hello.txt");
    assert!(explorer
        .items
        .iter()
        .any(|it| it.name == "hello.txt" && !it.is_dir));

    // Create nested file with auto-created subfolders
    let nested = explorer
        .create_file(test_dir.path(), "sub/nested/file.rs")
        .unwrap();
    assert!(nested.exists());
    assert!(explorer
        .expanded_dirs
        .contains(&test_dir.path().join("sub")));
    assert!(explorer
        .expanded_dirs
        .contains(&test_dir.path().join("sub").join("nested")));
}

#[test]
fn test_create_file_already_exists_fails_without_overwrite() {
    let test_dir = TestDir::new("lume_test_create_file_exists");
    let existing_path = test_dir.path().join("data.txt");
    std::fs::write(&existing_path, "original content").unwrap();

    let mut explorer = FileExplorer::new(test_dir.path().to_path_buf());
    let err = explorer
        .create_file(test_dir.path(), "data.txt")
        .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(
        std::fs::read_to_string(&existing_path).unwrap(),
        "original content"
    );
}

#[test]
fn test_create_file_prevents_path_traversal_and_absolute_paths() {
    let test_dir = TestDir::new("lume_test_create_file_traversal");
    let mut explorer = FileExplorer::new(test_dir.path().to_path_buf());

    let err1 = explorer
        .create_file(test_dir.path(), "/tmp/escape.txt")
        .unwrap_err();
    assert_eq!(err1.kind(), std::io::ErrorKind::PermissionDenied);

    let err2 = explorer
        .create_file(test_dir.path(), "../../escape.txt")
        .unwrap_err();
    assert_eq!(err2.kind(), std::io::ErrorKind::PermissionDenied);

    let err3 = explorer.create_file(test_dir.path(), "").unwrap_err();
    assert_eq!(err3.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn test_rename_file_and_folder_success() {
    let test_dir = TestDir::new("lume_test_rename");
    let file_path = test_dir.path().join("old_name.txt");
    std::fs::write(&file_path, "file data").unwrap();

    let folder_path = test_dir.path().join("old_folder");
    std::fs::create_dir(&folder_path).unwrap();

    let mut explorer = FileExplorer::new(test_dir.path().to_path_buf());

    // Rename file
    let new_file = explorer.rename_item(&file_path, "new_name.txt").unwrap();
    assert!(!file_path.exists());
    assert!(new_file.exists());
    assert_eq!(std::fs::read_to_string(&new_file).unwrap(), "file data");

    // Rename folder
    let new_folder = explorer.rename_item(&folder_path, "new_folder").unwrap();
    assert!(!folder_path.exists());
    assert!(new_folder.exists());
}

#[test]
fn test_rename_root_forbidden_and_existing_target_fails() {
    let test_dir = TestDir::new("lume_test_rename_forbidden");
    let f1 = test_dir.path().join("f1.txt");
    let f2 = test_dir.path().join("f2.txt");
    std::fs::write(&f1, "1").unwrap();
    std::fs::write(&f2, "2").unwrap();

    let mut explorer = FileExplorer::new(test_dir.path().to_path_buf());

    // Root rename forbidden
    let err_root = explorer.rename_item(test_dir.path(), "other").unwrap_err();
    assert_eq!(err_root.kind(), std::io::ErrorKind::PermissionDenied);

    // Collision fails
    let err_collision = explorer.rename_item(&f1, "f2.txt").unwrap_err();
    assert_eq!(err_collision.kind(), std::io::ErrorKind::AlreadyExists);
}

#[test]
fn test_jump_to_prefix_wraparound_and_scroll_clamping() {
    let test_dir = TestDir::new("lume_test_jump_prefix");
    std::fs::write(test_dir.path().join("alpha.txt"), "").unwrap();
    std::fs::write(test_dir.path().join("beta.txt"), "").unwrap();
    std::fs::write(test_dir.path().join("charlie.txt"), "").unwrap();
    std::fs::write(test_dir.path().join("delta.txt"), "").unwrap();

    let mut explorer = FileExplorer::new(test_dir.path().to_path_buf());
    assert_eq!(explorer.items[0].name, "alpha.txt");

    // Jump to 'b' -> beta.txt (idx 1)
    assert!(explorer.jump_to_prefix('b', 10));
    assert_eq!(explorer.selected_idx, 1);
    assert_eq!(explorer.items[explorer.selected_idx].name, "beta.txt");

    // Jump to 'd' -> delta.txt (idx 3)
    assert!(explorer.jump_to_prefix('d', 10));
    assert_eq!(explorer.selected_idx, 3);

    // Jump to 'A' (case-insensitive wraparound) -> alpha.txt (idx 0)
    assert!(explorer.jump_to_prefix('A', 10));
    assert_eq!(explorer.selected_idx, 0);

    // Jump to non-existing letter -> returns false, preserves selected_idx
    assert!(!explorer.jump_to_prefix('z', 10));
    assert_eq!(explorer.selected_idx, 0);
}

#[test]
fn test_jump_to_prefix_empty_tree_does_not_panic() {
    let test_dir = TestDir::new("lume_test_jump_empty");
    let mut explorer = FileExplorer::new(test_dir.path().to_path_buf());
    explorer.items.clear();
    assert!(!explorer.jump_to_prefix('a', 10));
}

#[test]
fn test_create_file_ends_with_separator_fails() {
    let test_dir = TestDir::new("lume_test_create_sep");
    let mut explorer = FileExplorer::new(test_dir.path().to_path_buf());
    let err1 = explorer
        .create_file(test_dir.path(), "folder/")
        .unwrap_err();
    assert_eq!(err1.kind(), std::io::ErrorKind::InvalidInput);
    let err2 = explorer
        .create_file(test_dir.path(), "folder\\")
        .unwrap_err();
    assert_eq!(err2.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn test_rename_item_edge_cases_and_expanded_dirs() {
    let test_dir = TestDir::new("lume_test_rename_edges");
    let ws = test_dir.path();
    let f1 = ws.join("file1.txt");
    std::fs::write(&f1, "1").unwrap();
    let parent_dir = ws.join("parent");
    let child_dir = parent_dir.join("child");
    std::fs::create_dir_all(&child_dir).unwrap();

    let mut explorer = FileExplorer::new(ws.to_path_buf());
    explorer.expanded_dirs.insert(parent_dir.clone());
    explorer.expanded_dirs.insert(child_dir.clone());

    // Empty new name
    let err_empty = explorer.rename_item(&f1, "   ").unwrap_err();
    assert_eq!(err_empty.kind(), std::io::ErrorKind::InvalidInput);

    // Absolute new name
    let err_abs = explorer.rename_item(&f1, "/etc/passwd").unwrap_err();
    assert_eq!(err_abs.kind(), std::io::ErrorKind::PermissionDenied);

    // Traversal escaping root
    let err_trav = explorer.rename_item(&f1, "../../escape.txt").unwrap_err();
    assert_eq!(err_trav.kind(), std::io::ErrorKind::PermissionDenied);

    // Source outside root
    let outside_dir = TestDir::new("lume_outside");
    let outside_file = outside_dir.path().join("out.txt");
    std::fs::write(&outside_file, "out").unwrap();
    let err_outside = explorer.rename_item(&outside_file, "new.txt").unwrap_err();
    assert_eq!(err_outside.kind(), std::io::ErrorKind::PermissionDenied);

    // Rename folder updates parent and child in expanded_dirs
    let new_parent = explorer.rename_item(&parent_dir, "parent_renamed").unwrap();
    assert!(explorer.expanded_dirs.contains(&new_parent));
    assert!(explorer.expanded_dirs.contains(&new_parent.join("child")));
    assert!(!explorer.expanded_dirs.contains(&parent_dir));
}
