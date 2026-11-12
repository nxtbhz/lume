//! Challenger Milestone 1 Empirical Stress Tests (Core)
//!
//! Tests:
//! 1. NUL byte rejection at byte offsets 0, 100, 8191, 8192
//! 2. Invalid UTF-8 rejection (truncated, invalid bytes, overlong, surrogates, deep offsets)
//! 3. Tab state non-mutation on open failure (buffer, dirty, cursors, file_path, history)

use lume_core::editor::Editor;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

fn temp_file(prefix: &str, ext: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "{}_{}_{}.{}",
        prefix,
        std::process::id(),
        nanos,
        ext
    ))
}

// -----------------------------------------------------------------------------
// 1. NUL Byte Rejection Stress Tests
// -----------------------------------------------------------------------------

#[test]
fn test_nul_byte_at_offset_0_rejected() {
    let path = temp_file("chal_nul_0", "bin");
    let mut data = vec![0x00];
    data.extend_from_slice(b"hello world text");
    fs::write(&path, &data).unwrap();

    let mut ed = Editor::new();
    let res = ed.open_in_current_tab(&path);
    let _ = fs::remove_file(&path);

    assert!(res.is_err(), "Offset 0 must be rejected");
    assert_eq!(res.unwrap_err().kind(), ErrorKind::InvalidData);
}

#[test]
fn test_nul_byte_at_offset_100_rejected() {
    let path = temp_file("chal_nul_100", "bin");
    let mut data = vec![b'a'; 100];
    data.push(0x00);
    data.extend_from_slice(b"trailing text");
    fs::write(&path, &data).unwrap();

    let mut ed = Editor::new();
    let res = ed.open_in_current_tab(&path);
    let _ = fs::remove_file(&path);

    assert!(res.is_err(), "Offset 100 must be rejected");
    assert_eq!(res.unwrap_err().kind(), ErrorKind::InvalidData);
}

#[test]
fn test_nul_byte_at_offset_8191_rejected() {
    let path = temp_file("chal_nul_8191", "bin");
    let mut data = vec![b'a'; 8191];
    data.push(0x00); // 8192nd byte (last byte of first 8 KiB)
    data.extend_from_slice(b"trailing text");
    fs::write(&path, &data).unwrap();

    let mut ed = Editor::new();
    let res = ed.open_in_current_tab(&path);
    let _ = fs::remove_file(&path);

    assert!(
        res.is_err(),
        "Offset 8191 (last byte of 8 KiB) must be rejected"
    );
    assert_eq!(res.unwrap_err().kind(), ErrorKind::InvalidData);
}

#[test]
fn test_nul_byte_at_offset_8192_behavior() {
    let path = temp_file("chal_nul_8192", "bin");
    let mut data = vec![b'a'; 8192];
    data.push(0x00); // 8193rd byte (1st byte beyond first 8 KiB)
    data.extend_from_slice(b"trailing text");
    fs::write(&path, &data).unwrap();

    let mut ed = Editor::new();
    let res = ed.open_in_current_tab(&path);
    let _ = fs::remove_file(&path);

    // Document empirical behavior: offset 8192 is byte 8193, which is outside the 8 KiB probe window.
    // Since 0x00 is valid UTF-8 (U+0000), RopeBuffer::from_reader succeeds.
    // Thus open_in_current_tab succeeds per the "first 8 KiB" specification limit.
    assert!(
        res.is_ok(),
        "Offset 8192 is beyond first 8 KiB and UTF-8 valid U+0000; succeeds per spec"
    );
}

// -----------------------------------------------------------------------------
// 2. Invalid UTF-8 Rejection Generator & Adversarial Tests
// -----------------------------------------------------------------------------

#[test]
fn test_invalid_utf8_raw_ff_fe_fd_rejected() {
    let path = temp_file("chal_bad_ff", "bin");
    fs::write(&path, [0xFF, 0xFE, 0xFD]).unwrap();

    let mut ed = Editor::new();
    let res = ed.open_in_current_tab(&path);
    let _ = fs::remove_file(&path);

    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), ErrorKind::InvalidData);
}

#[test]
fn test_invalid_utf8_truncated_sequences_rejected() {
    // 2-byte start without continuation
    let path2 = temp_file("chal_bad_trunc2", "txt");
    fs::write(&path2, b"hello \xc3 world").unwrap();
    let mut ed = Editor::new();
    let res2 = ed.open_in_current_tab(&path2);
    let _ = fs::remove_file(&path2);
    assert_eq!(res2.unwrap_err().kind(), ErrorKind::InvalidData);

    // 3-byte start with invalid continuation
    let path3 = temp_file("chal_bad_trunc3", "txt");
    fs::write(&path3, b"hello \xe2\x28\xa1 world").unwrap();
    let mut ed = Editor::new();
    let res3 = ed.open_in_current_tab(&path3);
    let _ = fs::remove_file(&path3);
    assert_eq!(res3.unwrap_err().kind(), ErrorKind::InvalidData);

    // 4-byte start with invalid continuation
    let path4 = temp_file("chal_bad_trunc4", "txt");
    fs::write(&path4, b"hello \xf0\x28\x80\x80 world").unwrap();
    let mut ed = Editor::new();
    let res4 = ed.open_in_current_tab(&path4);
    let _ = fs::remove_file(&path4);
    assert_eq!(res4.unwrap_err().kind(), ErrorKind::InvalidData);
}

#[test]
fn test_invalid_utf8_overlong_and_surrogates_rejected() {
    // Overlong encoding of ASCII '/' (0x2F -> 0xC0, 0xAF)
    let path_overlong = temp_file("chal_overlong", "txt");
    fs::write(&path_overlong, [0xC0, 0xAF]).unwrap();
    let mut ed = Editor::new();
    let res_o = ed.open_in_current_tab(&path_overlong);
    let _ = fs::remove_file(&path_overlong);
    assert_eq!(res_o.unwrap_err().kind(), ErrorKind::InvalidData);

    // UTF-16 surrogate half (U+D800: 0xED, 0xA0, 0x80)
    let path_surrogate = temp_file("chal_surrogate", "txt");
    fs::write(&path_surrogate, [0xED, 0xA0, 0x80]).unwrap();
    let mut ed = Editor::new();
    let res_s = ed.open_in_current_tab(&path_surrogate);
    let _ = fs::remove_file(&path_surrogate);
    assert_eq!(res_s.unwrap_err().kind(), ErrorKind::InvalidData);
}

#[test]
fn test_invalid_utf8_deep_in_file_past_8k_rejected() {
    // 16 KiB file: first 8 KiB is clean ASCII, but byte 10,000 has invalid UTF-8
    let path = temp_file("chal_bad_deep", "txt");
    let mut data = vec![b'x'; 10_000];
    data.extend_from_slice(&[0xFF, 0xFE]); // Invalid UTF-8 far past 8 KiB probe
    data.extend_from_slice(&[b'y'; 6_000]);
    fs::write(&path, &data).unwrap();

    let mut ed = Editor::new();
    let res = ed.open_in_current_tab(&path);
    let _ = fs::remove_file(&path);

    assert!(
        res.is_err(),
        "Invalid UTF-8 deep past 8 KiB must still be rejected"
    );
    assert_eq!(res.unwrap_err().kind(), ErrorKind::InvalidData);
}

// -----------------------------------------------------------------------------
// 3. Tab State Non-Mutation on Open Failure
// -----------------------------------------------------------------------------

#[test]
fn test_open_in_current_tab_preserves_dirty_buffer_on_nul_error() {
    let mut ed = Editor::from_str("unsaved precious work");
    ed.insert_str("!").unwrap();
    assert!(ed.is_dirty());
    let orig_text = ed.buffer().rope().to_string();
    let orig_cursors = ed.active_tab().cursors.clone();

    let bad_path = temp_file("chal_dirty_fail_nul", "bin");
    fs::write(&bad_path, [0x00, b'a', b'b']).unwrap();

    let res = ed.open_in_current_tab(&bad_path);
    let _ = fs::remove_file(&bad_path);

    assert!(res.is_err());
    assert!(
        ed.is_dirty(),
        "Tab must still be marked dirty after failed open"
    );
    assert_eq!(
        ed.buffer().rope().to_string(),
        orig_text,
        "Buffer content must not change"
    );
    assert_eq!(
        ed.active_tab().cursors,
        orig_cursors,
        "Cursors must not change"
    );
    assert_eq!(
        ed.file_path(),
        None,
        "file_path must not be set to bad path"
    );
}

#[test]
fn test_open_in_current_tab_preserves_existing_file_path_on_utf8_error() {
    let good_path = temp_file("chal_existing_file", "txt");
    fs::write(&good_path, "good file content").unwrap();

    let mut ed = Editor::open(&good_path).unwrap();
    assert_eq!(ed.file_path(), Some(good_path.as_path()));
    assert!(!ed.is_dirty());

    let bad_path = temp_file("chal_bad_file", "txt");
    fs::write(&bad_path, [0xFF, 0xFE]).unwrap();

    let res = ed.open_in_current_tab(&bad_path);
    let _ = fs::remove_file(&bad_path);
    let _ = fs::remove_file(&good_path);

    assert!(res.is_err());
    assert_eq!(
        ed.file_path(),
        Some(good_path.as_path()),
        "file_path must remain original good file"
    );
    assert_eq!(ed.buffer().rope().to_string(), "good file content");
    assert!(!ed.is_dirty());
}

#[test]
fn test_open_in_current_tab_preserves_undo_history_on_failure() {
    let mut ed = Editor::new();
    ed.insert_str("step 1").unwrap();
    ed.insert_str(" step 2").unwrap();
    assert_eq!(ed.buffer().rope().to_string(), "step 1 step 2");

    let bad_path = temp_file("chal_bad_undo", "bin");
    fs::write(&bad_path, [0x00]).unwrap();

    let res = ed.open_in_current_tab(&bad_path);
    let _ = fs::remove_file(&bad_path);

    assert!(res.is_err());
    // Undo should still work because history was not cleared!
    assert!(ed.undo(), "Undo must succeed after failed open");
    assert_eq!(ed.buffer().rope().to_string(), "step 1");
}

#[test]
fn test_open_tab_does_not_leak_tab_on_failure() {
    let mut ed = Editor::new();
    let initial_tab_count = ed.tabs().len();
    assert_eq!(initial_tab_count, 1);

    let bad_path = temp_file("chal_leak_nul", "bin");
    fs::write(&bad_path, [0x00]).unwrap();

    let res = ed.open_tab(&bad_path);
    let _ = fs::remove_file(&bad_path);

    assert!(res.is_err());
    assert_eq!(
        ed.tabs().len(),
        initial_tab_count,
        "Failed open_tab must not leave a zombie tab"
    );
    assert_eq!(ed.active_tab_idx(), 0);
}

#[test]
fn test_open_or_load_does_not_leak_tab_on_failure() {
    let mut ed = Editor::from_str("non empty buffer");
    let initial_tab_count = ed.tabs().len();
    assert_eq!(initial_tab_count, 1);

    let bad_path = temp_file("chal_or_load_nul", "bin");
    fs::write(&bad_path, [0x00]).unwrap();

    let res = ed.open_or_load(&bad_path);
    let _ = fs::remove_file(&bad_path);

    assert!(res.is_err());
    assert_eq!(
        ed.tabs().len(),
        initial_tab_count,
        "Failed open_or_load must not leave a zombie tab"
    );
    assert_eq!(ed.active_tab_idx(), 0);
    assert_eq!(ed.buffer().rope().to_string(), "non empty buffer");
}
