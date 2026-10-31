use crate::app::App;
use crate::render::render_status_bar;
use lume_core::git::GitHead;

#[test]
fn test_status_bar_displays_git_branch() {
    let mut app = App::new(None).unwrap();
    app.terminal_width = 100;
    app.terminal_height = 24;
    app.git_head = Some(GitHead::Branch("main".to_string()));

    let mut buf = Vec::new();
    render_status_bar(&app, &mut buf, 0).expect("status bar render should succeed");
    let output = String::from_utf8_lossy(&buf);

    assert!(
        output.contains("⎇ main"),
        "output should contain git branch indicator: {}",
        output
    );
}

#[test]
fn test_status_bar_displays_detached_commit() {
    let mut app = App::new(None).unwrap();
    app.terminal_width = 100;
    app.terminal_height = 24;
    app.git_head = Some(GitHead::Detached("7f3a9b1".to_string()));

    let mut buf = Vec::new();
    render_status_bar(&app, &mut buf, 0).expect("status bar render should succeed");
    let output = String::from_utf8_lossy(&buf);

    assert!(
        output.contains("⎇ 7f3a9b1"),
        "output should contain detached commit indicator: {}",
        output
    );
}

#[test]
fn test_status_bar_without_git_has_no_broken_separators() {
    let mut app = App::new(None).unwrap();
    app.terminal_width = 100;
    app.terminal_height = 24;
    app.git_head = None;

    let mut buf = Vec::new();
    render_status_bar(&app, &mut buf, 0).expect("status bar render should succeed");
    let output = String::from_utf8_lossy(&buf);

    assert!(
        !output.contains("⎇"),
        "output should not contain git icon when non-git"
    );
    assert!(
        !output.contains("│ │"),
        "output should not contain double consecutive separators"
    );
}

#[test]
fn test_status_bar_narrow_width_truncation() {
    let mut app = App::new(None).unwrap();
    app.terminal_width = 30;
    app.terminal_height = 24;
    app.git_head = Some(GitHead::Branch(
        "feature/very-long-branch-name-overflow".to_string(),
    ));

    let mut buf = Vec::new();
    render_status_bar(&app, &mut buf, 0).expect("narrow status bar render should succeed");
    assert!(!buf.is_empty());
}

#[test]
fn test_status_bar_narrow_width_modal_truncation() {
    let mut app = App::new(None).unwrap();
    app.terminal_width = 30;
    app.terminal_height = 24;
    app.modal_enabled = true;
    app.git_head = Some(GitHead::Branch(
        "feature/very-long-branch-name-overflow".to_string(),
    ));

    let mut buf = Vec::new();
    render_status_bar(&app, &mut buf, 0).expect("narrow modal status bar render should succeed");
    assert!(!buf.is_empty());
}
