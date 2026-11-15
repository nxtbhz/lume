#[cfg(unix)]
mod unix_explorer_security_tests {
    use std::os::unix::fs::symlink;

    use crate::explorer::FileExplorer;
    use crate::tests::TestDir;

    fn assert_rejected(err: std::io::Error) {
        assert!(
            matches!(
                err.kind(),
                std::io::ErrorKind::InvalidInput | std::io::ErrorKind::PermissionDenied
            ),
            "expected InvalidInput or PermissionDenied, got {:?}",
            err
        );
    }

    #[test]
    fn test_explorer_security_blocks_symlink_escape_in_create_file_folder_and_rename() {
        let ws_dir = TestDir::new("lume_sec_ws");
        let outside_dir = TestDir::new("lume_sec_outside");
        let ws = ws_dir.path();
        let outside = outside_dir.path();

        let escape_link = ws.join("escape");
        symlink(outside, &escape_link).unwrap();

        let inside_file = ws.join("inside.txt");
        std::fs::write(&inside_file, "safe").unwrap();

        let mut explorer = FileExplorer::new(ws.to_path_buf());

        // 1. Direct symlink base_dir in create_file
        let err_cf = explorer.create_file(&escape_link, "pwn.txt").unwrap_err();
        assert_rejected(err_cf);
        assert!(!outside.join("pwn.txt").exists());

        // 2. Non-existent subdir under symlink base_dir in create_file (TOCTOU/ancestry gap)
        let err_cf_nested = explorer
            .create_file(&escape_link.join("nested"), "pwn2.txt")
            .unwrap_err();
        assert_rejected(err_cf_nested);
        assert!(!outside.join("nested").exists());

        // 3. Intermediate symlink component inside rel_path in create_file
        let err_cf_rel = explorer.create_file(ws, "escape/sub/pwn3.txt").unwrap_err();
        assert_rejected(err_cf_rel);
        assert!(!outside.join("sub").exists());

        // 4. Symlink base_dir in create_folder
        let err_cd = explorer.create_folder(&escape_link, "dir").unwrap_err();
        assert_rejected(err_cd);
        assert!(!outside.join("dir").exists());

        // 5. Intermediate symlink component in create_folder
        let err_cd_rel = explorer.create_folder(ws, "escape/dir2").unwrap_err();
        assert_rejected(err_cd_rel);
        assert!(!outside.join("dir2").exists());

        // 6. Symlink target in rename_item via .. or relative symlink component
        let err_rn_dotdot = explorer
            .rename_item(&inside_file, "../escape/pwn")
            .unwrap_err();
        assert_rejected(err_rn_dotdot);
        assert!(inside_file.exists());
        assert!(!outside.join("pwn").exists());

        let err_rn_sym = explorer
            .rename_item(&inside_file, "escape/pwn")
            .unwrap_err();
        assert_rejected(err_rn_sym);
        assert!(inside_file.exists());
        assert!(!outside.join("pwn").exists());
    }

    #[test]
    fn test_explorer_security_blocks_dangling_symlink_target() {
        let ws_dir = TestDir::new("lume_sec_dangling_ws");
        let outside_dir = TestDir::new("lume_sec_dangling_out");
        let ws = ws_dir.path();
        let outside_missing = outside_dir.path().join("does_not_exist_yet");

        let dangling = ws.join("dangling_out");
        symlink(&outside_missing, &dangling).unwrap();

        let mut explorer = FileExplorer::new(ws.to_path_buf());

        let err_cf = explorer.create_file(ws, "dangling_out").unwrap_err();
        assert!(
            matches!(
                err_cf.kind(),
                std::io::ErrorKind::InvalidInput
                    | std::io::ErrorKind::PermissionDenied
                    | std::io::ErrorKind::AlreadyExists
            ),
            "unexpected error for dangling symlink create_file: {:?}",
            err_cf
        );
        assert!(!outside_missing.exists());

        let err_cf_sub = explorer
            .create_file(&dangling.join("sub"), "pwn.txt")
            .unwrap_err();
        assert_rejected(err_cf_sub);
        assert!(!outside_missing.exists());
    }

    #[cfg(unix)]
    #[test]
    fn test_explorer_rejects_symlink_loop_curdir_and_symlink_delete_folder() {
        let base = std::env::temp_dir().join(format!("lume_exp_sec_reg_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ws = base.join("ws");
        let outside = base.join("outside");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), "do not delete").unwrap();

        // 1. Symlink loop `ws/loop -> ws` combined with `/.` must NOT bypass ancestor check
        let loop_link = ws.join("loop");
        std::os::unix::fs::symlink(&ws, &loop_link).unwrap();
        let mut explorer = crate::explorer::FileExplorer::new(ws.clone());
        let err_loop = explorer
            .create_file(&loop_link.join("."), "pwn.txt")
            .unwrap_err();
        assert_eq!(err_loop.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(!ws.join("pwn.txt").exists());

        // 2. `delete_folder` must reject symlinks pointing outside workspace without deleting target
        let del_link = ws.join("del_link");
        std::os::unix::fs::symlink(&outside, &del_link).unwrap();
        let err_del = explorer.delete_folder(&del_link).unwrap_err();
        assert_eq!(err_del.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(outside.join("secret.txt").exists());

        let _ = std::fs::remove_dir_all(&base);
    }
}
