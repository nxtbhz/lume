# Project: Lume V1.0 Hardening & Production Readiness

## Architecture
Lume is a high-performance terminal code editor written in Rust, architected as a workspace of three primary crates:
1. `crates/lume-core`: Headless editor engine. Manages rope buffers (`RopeBuffer`), multi-cursor navigation (`CursorSet`), undo/redo history (`History`), disk persistence & atomic saves (`save.rs`), binary journal crash recovery (`journal.rs`), smart indentation (`indent.rs`), search/replace, diffing (`diff.rs`), and git HEAD integration (`git.rs`).
2. `crates/lume-syntax`: Asynchronous tree-sitter & regex tokenization. Offloads parsing and syntax highlighting from the UI thread to a background worker (`SyntaxWorker`) using `crossbeam-channel`.
3. `crates/lume-tui`: Terminal user interface powered by `crossterm`. Manages raw mode, alternate screen, buffered ANSI rendering, split view geometry (`SplitState`), file explorer, quick open, command palette, modal input prompts, and event dispatching (`AppEvent`).

### Background Worker Architecture
- **JournalWorker**: Singleton background thread in `lume-core` handling append, snapshot, and remove disk operations. Runs `file.sync_data()` off the UI thread and coalesces snapshot requests per path.
- **SyntaxWorker**: Thread in `lume-syntax` receiving `Rope` from the UI thread ($O(1)$ Arc clone). Highlighters keyed by `(buffer_id, Language)` maintain incremental tree-sitter ASTs using `old_tree`. Byte offsets in cached tokens are shifted immediately on keystrokes using `ByteDelta`.
- **Event Loop & Latency**: `App::handle_mouse` returns `io::Result<bool>` (true only when application state changes), suppressing idle pointer moves and non-dragging drag events from triggering terminal redraws.

---

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | `safe_file_open` | Reject files with NUL bytes (\0 in first 8 KiB) or invalid UTF-8 with `io::ErrorKind::InvalidData` before mutating tab state | M1 | spec §1.1 |
| 2 | `untitled_tab_on_failure` | On open failure in `App::open_targets`, keep tab untitled (`file_path: None`, `is_dirty: false`) and report error in status bar | M1 | spec §1.2 |
| 3 | `opt_in_autosave` | `AutoSaveConfig::default().enabled = false` (1000ms debounce); `FocusLost` saves only if enabled | M1 | spec §1.3 |
| 4 | `journal_mtime_recovery` | Stale journal deleted if disk mtime > journal mtime; else load clean disk and prompt `[R]ecover, [D]iscard` (`ConfirmRecoverJournal`) | M1 | spec §1.4 |
| 5 | `discard_journal_cleanup` | Discard on close/quit (`ConfirmCloseTab -> D`, `ConfirmQuit -> D`) deletes discarded tabs' `.journal` files via `discard_tab_changes` / `discard_all_journals` | M1 | spec §1.5 |
| 6 | `tab_close_dirty_guard` | Clicking [x] on dirty tab in tab bar opens `InputPrompt::confirm_close_tab` instead of closing immediately | M1 | spec §1.6 |
| 7 | `journal_path_lifecycle` | Clear `tab.journal_path` on folder deletion and update on tab file rename | M1 | spec §1.7 |
| 8 | `background_journal_worker` | Background `JournalWorker` over crossbeam channel; coalesced snapshots, off-UI `sync_data()`, FIFO `flush_journal` barrier | M2 | spec §2.1 |
| 9 | `rope_syntax_worker` | `SyntaxRequest` accepts `ropey::Rope` ($O(1)$ Arc clone on UI thread) | M2 | spec §2.2 |
| 10 | `buffer_highlighter_cache` | Key highlighters by `(buffer_id, Language)`; pass `old_tree` to tree-sitter for incremental parse | M2 | spec §2.3 |
| 11 | `highlight_token_shifting` | Shift cached `HighlightToken` byte offsets on active-tab edits via `ByteDelta` in `token.rs` | M2 | spec §2.4 |
| 12 | `syntax_buffer_eviction` | Evict closed tab caches in worker via `SyntaxCommand::DropBuffer` and clean TUI caches | M2 | spec §2.5 |
| 13 | `idle_mouse_suppression` | `App::handle_mouse` returns `io::Result<bool>`, suppressing idle `Moved` and non-drag events from redrawing | M2 | spec §2.6 |
| 14 | `line_ending_detection` | Detect `LineEnding::Lf` vs `LineEnding::CrLf` from line 0 on file open | M3 | spec §3.1 |
| 15 | `line_ending_insert` | `insert_newline` emits `\n` or `\r\n` based on tab line ending | M3 | spec §3.2 |
| 16 | `atomic_crlf_deletion` | `delete_backwards` and `delete_forwards` delete `\r\n` as an atomic 2-byte unit | M3 | spec §3.3 |
| 17 | `status_bar_line_ending` | Display `"LF"` or `"CRLF"` in the status bar | M3 | spec §3.4 |
| 18 | `undo_typing_coalescing` | Coalesce consecutive single-character word typing within 400ms unless `break_undo_group()` called | M4 | spec §4.1 |
| 19 | `exact_saved_depth_tracking` | Track `saved_depth: Option<usize>`; undo/redo restores `is_dirty = false` at exact saved state; invalidate on branch edits | M4 | spec §4.2 |
| 20 | `git_head_blob_reader` | Read committed Git HEAD blob via `read_git_head_blob` in `git.rs` | M5 | spec §5.1 |
| 21 | `persistent_gutter_baseline` | Cache `git_baseline` in `Tab`; diff buffer against baseline regardless of `is_dirty`, keeping gutter diffs visible after Ctrl+S | M5 | spec §5.2 |
| 22 | `verification_benchmarks_ci` | All workspace tests pass, `./scripts/ci.sh` clean, zero benchmark regressions in `cargo bench` and `./benchmark.sh` | M6 | spec §6.1 |

---

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Safe Open & Recovery | Safe open, stale journal deletion, explicit recovery prompt, discard cleanup, tab [x] guard, opt-in autosave | None | DONE |
| M2 | Worker Optimization & Latency | Background JournalWorker, incremental Rope SyntaxWorker, token shifting, idle mouse suppression | M1 | DONE |
| M3 | Line Ending Fidelity | LineEnding enum (LF/CRLF), atomic \r\n insertion and deletion, status bar indicator | None | DEFERRED_PER_USER_DIRECTIVE |
| M4 | Undo & Clean State | 400ms word typing undo coalescing, break_undo_group, exact saved_depth tracking and branch invalidation | None | DEFERRED_PER_USER_DIRECTIVE |
| M5 | Git HEAD Gutter Baseline | read_git_head_blob, git_baseline caching, persistent gutter diff markers across saves | None | DEFERRED_PER_USER_DIRECTIVE |
| M6 | Final Milestone & Hardening | 100% E2E test suite pass (Tiers 1-4), Tier 5 white-box adversarial coverage hardening, CI & benchmarks clean | M1, M2, M3, M4, M5, TEST_READY | DEFERRED_PER_USER_DIRECTIVE |

---

## Interface Contracts

### M1 ↔ Core Tab & TUI Prompts
- `Editor::open_in_current_tab(&mut self, path: &Path) -> io::Result<()>`:
  Checks first 8 KiB for NUL bytes or invalid UTF-8. On error, returns `io::Error::new(io::ErrorKind::InvalidData, ...)` without modifying tab state.
- `Tab.pending_recovery: Option<String>`: Holds recovered text if fresh journal exists.
- `Editor::recover_tab_journal(&mut self, tab_idx: usize)`: Applies pending recovery, records delta, marks dirty.
- `Editor::discard_tab_changes(&mut self, tab_idx: usize)`: Cleans pending recovery, deletes `.journal` from disk.
- `Editor::discard_all_journals(&mut self)`: Cleans all pending recoveries, deletes all `.journal` files for open tabs.
- `PromptAction::ConfirmRecoverJournal { tab_idx: usize }`: Prompt modal supporting `[R]ecover` and `[D]iscard`.
- `AutoSaveConfig::default().enabled`: `false`.

### M2 ↔ Background Workers
- `JournalOp`:
  `AppendDeltas { path: PathBuf, journal_path: PathBuf, deltas: Vec<TextDelta> }`,
  `Snapshot { path: PathBuf, journal_path: PathBuf, text: String }`,
  `Remove { journal_path: PathBuf }`,
  `Flush { responder: crossbeam_channel::Sender<()> }`.
- `JournalWorker`: Singleton background worker thread managing `JournalOp` channel.
- `SyntaxRequest`:
  `pub text: ropey::Rope` ($O(1)$ Arc clone).
- `SyntaxCommand::DropBuffer(usize)`: Sent to `SyntaxWorker` when tab is closed to evict parser and AST.
- `SyntaxWorker`: Keys highlighters by `(usize, Language)` where `usize` is `buffer_id`.
- `token::shift_tokens_for_deltas(tokens: &mut [HighlightToken], deltas: &[ByteDelta])`: Shifting cached token byte ranges.
- `App::handle_mouse(&mut self, mouse: MouseEvent) -> io::Result<bool>`: Returns `Ok(true)` only when state changed requiring render.

### M3 ↔ Line Endings
- `LineEnding`: `enum LineEnding { Lf, CrLf }` with `as_str() -> &'static str`.
- `Tab.line_ending: LineEnding`: Inferred from first line ending in buffer, defaulting to `Lf`.
- `Editor::insert_newline(&mut self)`: Inserts `tab.line_ending.as_str()`.
- `Editor::delete_backwards(&mut self)` / `delete_forwards(&mut self)`: Checks if cursor is preceded/followed by `\r\n` and removes both bytes atomically.
- Status bar rendering: displays `LF` or `CRLF`.

### M4 ↔ History
- `History::can_coalesce(&self, now: Instant) -> bool`: Checks if last edit is within 400ms and edit type is single-character word insertion.
- `History::break_undo_group(&mut self)`: Resets coalescing timer/state.
- `History.saved_depth: Option<usize>`: Set to `Some(undo_stack.len())` on save. Restores `is_dirty = false` when undo/redo lands at `saved_depth`. Invalided to `None` if edits truncate past `saved_depth`.

### M5 ↔ Git Baseline
- `git::read_git_head_blob(file_path: &Path) -> Option<String>`: Finds git worktree root, runs `git --no-pager show HEAD:<rel_path>`.
- `Tab.git_baseline: Option<ropey::Rope>`: Stores committed content.
- `Tab::diff_markers(&self) -> Option<Vec<DiffMarker>>`: Diffs current buffer against `git_baseline.as_ref().or(self.clean_snapshot.as_ref())`, regardless of `is_dirty`.

---

## Code Layout
- `crates/lume-core/src/editor/tab.rs`: `Tab` struct, line ending detection, git baseline storage, pending recovery.
- `crates/lume-core/src/editor/mod.rs`: `Editor` API, safe open, recovery/discard methods, atomic newline & delete.
- `crates/lume-core/src/editor/save.rs`: Atomic save, git baseline refresh, journal removal.
- `crates/lume-core/src/editor/indent.rs`: Indent newline emission, auto-pairs.
- `crates/lume-core/src/editor/lines.rs`: Atomic CRLF deletion and motion.
- `crates/lume-core/src/journal.rs`: `Journal`, `JournalWorker`, `JournalOp`, background thread, mtime freshness.
- `crates/lume-core/src/autosave.rs`: `AutoSaveConfig::default().enabled = false`.
- `crates/lume-core/src/history/mod.rs`: Undo coalescing, `break_undo_group`, `saved_depth`.
- `crates/lume-core/src/git.rs`: `read_git_head_blob`.
- `crates/lume-syntax/src/worker.rs`: `SyntaxRequest` with `Rope`, `(buffer_id, Language)` keyed highlighters, `DropBuffer`.
- `crates/lume-syntax/src/highlighter.rs`: Incremental `parser.parse(text, old_tree)`.
- `crates/lume-syntax/src/token.rs`: `ByteDelta`, `shift_tokens_for_deltas`.
- `crates/lume-tui/src/app.rs`: Mouse event handling (`io::Result<bool>`), tab open failure recovery, syntax cache eviction.
- `crates/lume-tui/src/input/mouse.rs`: Mouse routing, tab [x] dirty prompt guard.
- `crates/lume-tui/src/input/overlays.rs`: Modal prompt handling (`ConfirmRecoverJournal`, Discard cleanup).
- `crates/lume-tui/src/prompt.rs`: `ConfirmRecoverJournal` prompt definitions.
- `crates/lume-tui/src/render/status_bar.rs`: LF/CRLF indicator display.
- `crates/lume-tui/src/render/gutter.rs`: Persistent diff marker rendering.
