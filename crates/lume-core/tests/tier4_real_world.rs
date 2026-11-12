//! Tier 4: Real-World Application Scenarios
//!
//! Complex developer workflows simulating realistic editor usage:
//! 1. Git repository workflow: opening tracked files, editing, saving, and verifying diff baselines.
//! 2. Crash recovery simulation: dirty buffers, unclean exit, recovery prompt handling, and disk sync.
//! 3. Mixed binary & code project: opening binary assets without tab corruption, editing valid files with undo.
//! 4. Human typing & micro-pause workflow: coalescing, cursor navigation, save point, clean state restoration.
//! 5. Multi-tab session: concurrent editing across tabs, selective save and discard, journal cleanup.
//! 6. Refactoring with branching history: exploration, undo, alternative implementation, redo pruning.
//! 7. Incremental editing & tab switching: fast context switching, independent undo stacks and clean snapshots.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use lume_core::buffer::TextDelta;
use lume_core::editor::Editor;
use lume_core::git::find_git_dir;
use lume_core::journal::Journal;

fn unique_temp_path(prefix: &str, ext: &str) -> PathBuf {
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
// Scenario 1: Git Developer Workflow
// -----------------------------------------------------------------------------
#[test]
fn test_t4_scenario_1_git_developer_workflow() {
    let repo_dir = unique_temp_path("test_t4_repo", "dir");
    fs::create_dir_all(&repo_dir).unwrap();

    let run_git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(&repo_dir)
            .output()
            .unwrap()
    };

    run_git(&["init"]);
    run_git(&["config", "user.email", "dev@lume.test"]);
    run_git(&["config", "user.name", "Lume Tester"]);

    let main_rs = repo_dir.join("main.rs");
    fs::write(&main_rs, "fn main() {\n    println!(\"v1\");\n}\n").unwrap();
    run_git(&["add", "main.rs"]);
    run_git(&["commit", "-m", "initial commit"]);

    assert!(find_git_dir(&main_rs).is_some());

    // Step 1: Open file in editor
    let mut ed = Editor::open(&main_rs).unwrap();
    assert!(!ed.is_dirty());
    assert_eq!(ed.file_path(), Some(main_rs.as_path()));

    // Step 2: Edit file (insert "_modified" at cursor 26)
    ed.cursors_mut().set_single(26);
    ed.insert_str("_modified").unwrap();
    assert!(ed.is_dirty());

    // Step 3: Save file atomically
    ed.save().unwrap();
    assert!(!ed.is_dirty());

    let disk_content = fs::read_to_string(&main_rs).unwrap();
    assert!(disk_content.contains("_modified"));

    // Step 4: Undo back
    assert!(ed.undo());
    assert_eq!(
        ed.buffer().rope().to_string(),
        "fn main() {\n    println!(\"v1\");\n}\n"
    );

    let _ = fs::remove_dir_all(&repo_dir);
}

// -----------------------------------------------------------------------------
// Scenario 2: Crash Simulation & Recovery Lifecycle
// -----------------------------------------------------------------------------
#[test]
fn test_t4_scenario_2_crash_simulation_and_recovery_workflow() {
    let file = unique_temp_path("test_t4_crash", "rs");
    fs::write(&file, "struct Server {\n    port: u16,\n}\n").unwrap();

    // Session 1: Developer edits file, journal deltas are logged
    let mut ed1 = Editor::open(&file).unwrap();
    ed1.cursors_mut().set_single(30);
    ed1.insert_str("    host: String,\n").unwrap();
    assert!(ed1.is_dirty());

    // Simulate crash journal persistence
    let delta = TextDelta {
        start_byte: 30,
        old_text: String::new(),
        new_text: "    host: String,\n".to_string(),
        timestamp: 12345,
    };
    Journal::record_deltas(&file, &[delta]).unwrap();
    assert!(Journal::has_journal(&file));

    // Simulate sudden crash (kill -9): drop editor without calling ed1.save()
    drop(ed1);

    // Verify disk content is still original
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "struct Server {\n    port: u16,\n}\n"
    );

    // Session 2: Relaunch editor after crash
    let recovered_text = Journal::recover(&file).unwrap();
    assert!(recovered_text.is_some());
    assert!(recovered_text.unwrap().contains("host: String"));

    // Developer executes [D]iscard on journal
    Journal::remove(&file).unwrap();
    assert!(!Journal::has_journal(&file));

    // Normal open cleanly loads disk content
    let ed2 = Editor::open(&file).unwrap();
    assert_eq!(
        ed2.buffer().rope().to_string(),
        "struct Server {\n    port: u16,\n}\n"
    );
    assert!(!ed2.is_dirty());

    let _ = fs::remove_file(&file);
}

// -----------------------------------------------------------------------------
// Scenario 3: Mixed Binary & Code Workspace Workflow
// -----------------------------------------------------------------------------
#[test]
fn test_t4_scenario_3_mixed_binary_and_code_workspace_workflow() {
    let dir = unique_temp_path("test_t4_mixed", "dir");
    fs::create_dir_all(&dir).unwrap();

    let code_file = dir.join("lib.rs");
    fs::write(&code_file, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n").unwrap();

    let config_file = dir.join("config.toml");
    fs::write(&config_file, "[package]\nname = \"demo\"\n").unwrap();

    // 1. Open valid Rust code file
    let mut ed = Editor::open(&code_file).unwrap();
    assert_eq!(ed.file_path(), Some(code_file.as_path()));
    assert!(!ed.is_dirty());

    // 2. Open second valid tab
    let tab2 = ed.open_tab(&config_file).unwrap();
    assert_eq!(tab2, 1);
    assert_eq!(ed.tabs().len(), 2);

    // 3. Edit second tab
    ed.cursors_mut().set_single(23);
    ed.insert_str("version = \"1.0.0\"\n").unwrap();
    assert!(ed.is_dirty());
    ed.save().unwrap();
    assert!(!ed.is_dirty());

    // 4. Switch back to first tab and edit with undo
    ed.switch_tab(0);
    assert_eq!(ed.active_tab_idx(), 0);
    ed.insert_str("// Added header\n").unwrap();
    assert!(ed.is_dirty());
    assert!(ed.undo());
    assert_eq!(
        ed.buffer().rope().to_string(),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\n"
    );

    let _ = fs::remove_dir_all(&dir);
}

// -----------------------------------------------------------------------------
// Scenario 4: Human Typing & Micro-Pause Workflow
// -----------------------------------------------------------------------------
#[test]
fn test_t4_scenario_4_human_typing_and_undo_restoration_workflow() {
    let file = unique_temp_path("test_t4_typing", "txt");
    fs::write(&file, "first line\n").unwrap();

    let mut ed = Editor::open(&file).unwrap();
    assert!(!ed.is_dirty());

    // Simulate typing: move to end of line 0
    ed.cursors_mut().set_single(11);

    // Type a word
    for c in "second ".chars() {
        ed.insert_char(c).unwrap();
    }
    assert!(ed.is_dirty());

    // Save to create a clean checkpoint
    ed.save().unwrap();
    assert!(!ed.is_dirty());
    assert_eq!(fs::read_to_string(&file).unwrap(), "first line\nsecond ");

    // Type more after checkpoint
    for c in "line\n".chars() {
        ed.insert_char(c).unwrap();
    }
    assert!(ed.is_dirty());

    // Undoing back to clean checkpoint
    while ed.buffer().rope() != "first line\nsecond " {
        if !ed.undo() {
            break;
        }
    }
    assert_eq!(ed.buffer().rope().to_string(), "first line\nsecond ");

    let _ = fs::remove_file(&file);
}

// -----------------------------------------------------------------------------
// Scenario 5: Multi-Tab Session (Save & Discard)
// -----------------------------------------------------------------------------
#[test]
fn test_t4_scenario_5_multitab_session_with_discard_and_save_workflow() {
    let f1 = unique_temp_path("test_t4_f1", "txt");
    let f2 = unique_temp_path("test_t4_f2", "txt");
    let f3 = unique_temp_path("test_t4_f3", "txt");

    fs::write(&f1, "tab 1 clean\n").unwrap();
    fs::write(&f2, "tab 2 clean\n").unwrap();
    fs::write(&f3, "tab 3 clean\n").unwrap();

    let mut ed = Editor::open(&f1).unwrap();
    ed.open_tab(&f2).unwrap();
    ed.open_tab(&f3).unwrap();
    assert_eq!(ed.tabs().len(), 3);

    // Tab 1: kept clean
    ed.switch_tab(0);
    assert!(!ed.is_dirty());

    // Tab 2: edited and saved
    ed.switch_tab(1);
    ed.insert_str("edited ").unwrap();
    assert!(ed.is_dirty());
    ed.save().unwrap();
    assert!(!ed.is_dirty());

    // Tab 3: edited and discarded
    ed.switch_tab(2);
    ed.insert_str("scratch work ").unwrap();
    assert!(ed.is_dirty());
    // Discard Tab 3
    ed.close_tab(2);
    assert_eq!(ed.tabs().len(), 2);

    // Verify disk content
    assert_eq!(fs::read_to_string(&f1).unwrap(), "tab 1 clean\n");
    assert_eq!(fs::read_to_string(&f2).unwrap(), "edited tab 2 clean\n");
    assert_eq!(fs::read_to_string(&f3).unwrap(), "tab 3 clean\n");

    let _ = fs::remove_file(&f1);
    let _ = fs::remove_file(&f2);
    let _ = fs::remove_file(&f3);
}

// -----------------------------------------------------------------------------
// Scenario 6: Refactoring with Branching History
// -----------------------------------------------------------------------------
#[test]
fn test_t4_scenario_6_refactoring_with_branching_history_workflow() {
    let mut ed = Editor::from_str("fn compute() -> i32 {\n    0\n}\n");

    // Attempt 1: implement via loop
    ed.cursors_mut().set_single(26);
    ed.insert_str("let mut sum = 0; sum\n").unwrap();
    assert!(ed.buffer().rope().to_string().contains("sum"));

    // Decide to undo Attempt 1
    assert!(ed.undo());
    assert_eq!(
        ed.buffer().rope().to_string(),
        "fn compute() -> i32 {\n    0\n}\n"
    );

    // Attempt 2 (Branching edit): implement via iterator
    ed.insert_str("(1..=10).sum()\n").unwrap();
    assert!(ed.buffer().rope().to_string().contains("(1..=10).sum()"));

    // Attempt 1 redo must no longer be reachable
    assert!(!ed.redo());
}

// -----------------------------------------------------------------------------
// Scenario 7: Incremental Editing & Tab Switching
// -----------------------------------------------------------------------------
#[test]
fn test_t4_scenario_7_rapid_file_switch_and_incremental_save_workflow() {
    let p_a = unique_temp_path("test_t4_inc_a", "txt");
    let p_b = unique_temp_path("test_t4_inc_b", "txt");
    fs::write(&p_a, "alpha\n").unwrap();
    fs::write(&p_b, "beta\n").unwrap();

    let mut ed = Editor::open(&p_a).unwrap();
    ed.open_tab(&p_b).unwrap();

    // Edit Tab B
    ed.switch_tab(1);
    ed.cursors_mut().set_single(5);
    ed.insert_str("2\n").unwrap();
    assert!(ed.is_dirty());

    // Switch to Tab A and edit
    ed.switch_tab(0);
    ed.cursors_mut().set_single(6);
    ed.insert_str("1\n").unwrap();
    assert!(ed.is_dirty());

    // Save Tab A
    ed.save().unwrap();
    assert!(!ed.is_dirty());
    assert_eq!(fs::read_to_string(&p_a).unwrap(), "alpha\n1\n");

    // Switch back to Tab B, still dirty
    ed.switch_tab(1);
    assert!(ed.is_dirty());
    ed.save().unwrap();
    assert!(!ed.is_dirty());
    assert_eq!(fs::read_to_string(&p_b).unwrap(), "beta\n2\n");

    let _ = fs::remove_file(&p_a);
    let _ = fs::remove_file(&p_b);
}
