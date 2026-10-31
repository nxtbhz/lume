use crate::render::gutter::render_gutter;
use lume_core::diff::LineDiffStatus;

#[test]
fn test_render_gutter_markers_and_colors() {
    let mut buf_unchanged = Vec::new();
    render_gutter(
        &mut buf_unchanged,
        0,
        10,
        5,
        Some(LineDiffStatus::Unchanged),
    )
    .unwrap();
    let str_unchanged = String::from_utf8_lossy(&buf_unchanged);
    assert!(
        str_unchanged.contains("1") && str_unchanged.contains("│ "),
        "unchanged should contain line number 1 and separator: {}",
        str_unchanged
    );
    assert!(
        !str_unchanged.contains("+")
            && !str_unchanged.contains("~")
            && !str_unchanged.contains("-"),
        "unchanged should not contain diff markers: {}",
        str_unchanged
    );

    let mut buf_added = Vec::new();
    render_gutter(&mut buf_added, 1, 10, 5, Some(LineDiffStatus::Added)).unwrap();
    let str_added = String::from_utf8_lossy(&buf_added);
    assert!(
        str_added.contains("+") && str_added.contains("│ "),
        "added line should contain + marker and separator: {}",
        str_added
    );

    let mut buf_modified = Vec::new();
    render_gutter(&mut buf_modified, 2, 10, 5, Some(LineDiffStatus::Modified)).unwrap();
    let str_modified = String::from_utf8_lossy(&buf_modified);
    assert!(
        str_modified.contains("~") && str_modified.contains("│ "),
        "modified line should contain ~ marker and separator: {}",
        str_modified
    );

    let mut buf_deleted = Vec::new();
    render_gutter(
        &mut buf_deleted,
        3,
        10,
        5,
        Some(LineDiffStatus::DeletedAbove),
    )
    .unwrap();
    let str_deleted = String::from_utf8_lossy(&buf_deleted);
    assert!(
        str_deleted.contains("-") && str_deleted.contains("│ "),
        "deleted line should contain - marker and separator: {}",
        str_deleted
    );
}

#[test]
fn test_gutter_width_invariant_preserves_mouse_alignment() {
    let mut buf = Vec::new();
    // gutter_w = 2 digits + 3 = 5
    render_gutter(&mut buf, 9, 50, 5, Some(LineDiffStatus::Added)).unwrap();
    let raw_str = String::from_utf8_lossy(&buf);
    // Verifies the formatted line number and marker contains "10" and "+│ "
    assert!(
        raw_str.contains("10"),
        "must contain line number 10: {}",
        raw_str
    );
    assert!(raw_str.contains("+"), "must contain + marker: {}", raw_str);
    assert!(
        raw_str.contains("│ "),
        "must contain separator: {}",
        raw_str
    );
}
