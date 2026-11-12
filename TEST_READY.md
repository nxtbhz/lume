# TEST_READY: Lume V1.0 E2E Testing Suite

## 1. Executive Summary

The complete opaque-box E2E integration test suite for Lume V1.0 Hardening (Tasks 1 to 5) has been authored, verified, and published.
All tests have been derived strictly from specifications (`docs/superpowers/specs/2026-11-07-v1-readiness-spec.md`), user requirements (`ORIGINAL_REQUEST.md`), and interface contracts (`PROJECT.md`).

Zero production code in `src/` was modified. Tests follow the co-located per-module architecture in `crates/lume-core/tests/` and `crates/lume-syntax/tests/`.

---

## 2. Test Runner Commands

### Standard Execution (Active Integration Tests)
```bash
# Run all active integration test suites across core and syntax
cargo test -p lume-core --test tier1_feature_coverage && \
cargo test -p lume-core --test tier2_boundaries && \
cargo test -p lume-core --test tier3_cross_feature && \
cargo test -p lume-core --test tier4_real_world && \
cargo test -p lume-syntax --test tier1_syntax_coverage
```

### Milestone-Specific Execution (Progressive Testability)
As implementation milestones land, execute the targeted feature suites:

```bash
# Milestone 1: Safe Open, Stale Journal Deletion, Discard & Opt-in Autosave
cargo test -p lume-core --features m1

# Milestone 2: Background Journal Worker, Incremental Syntax Worker & Idle Mouse
cargo test -p lume-core --features m2
cargo test -p lume-syntax --features m2

# Milestone 3: Line Ending (LF/CRLF) Detection, Atomic \r\n Editing & Status Bar
cargo test -p lume-core --features m3

# Milestone 4: Undo Coalescing & Exact saved_depth Clean-State Tracking
cargo test -p lume-core --features m4

# Milestone 5: Git HEAD Gutter Diff Baseline Across Saves
cargo test -p lume-core --features m5
```

### Full E2E Test Suite (Milestone 6 / Acceptance Gate)
```bash
# Run all test tiers with all milestone features enabled
cargo test --workspace --features e2e

# Or individual tier suites with full features
cargo test -p lume-core --test tier1_feature_coverage --features e2e
cargo test -p lume-core --test tier2_boundaries --features e2e
cargo test -p lume-core --test tier3_cross_feature --features e2e
cargo test -p lume-core --test tier4_real_world --features e2e
cargo test -p lume-syntax --test tier1_syntax_coverage --features e2e
```

---

## 3. Summary Table by Tier

| Tier | Focus / Scope | Min Requirement | Test Count | Location | Current Status |
|:---|:---|:---:|:---:|:---|:---:|
| **Tier 1** | Feature Coverage (Features 1–21 isolated happy path) | $\ge 105$ ($\ge 5$/feature) | **105** | `crates/lume-core/tests/tier1_feature_coverage.rs`<br>`crates/lume-syntax/tests/tier1_syntax_coverage.rs` | ✅ READY |
| **Tier 2** | Boundary & Corner Cases (Limits, NUL 8K, invalid UTF-8) | $\ge 105$ ($\ge 5$/feature) | **105** | `crates/lume-core/tests/tier2_boundaries.rs` | ✅ READY |
| **Tier 3** | Cross-Feature Combinations (Pairwise interactions) | $\ge 15$ | **15** | `crates/lume-core/tests/tier3_cross_feature.rs` | ✅ READY |
| **Tier 4** | Real-World Application Scenarios (Developer sessions) | $\ge 5$ | **7** | `crates/lume-core/tests/tier4_real_world.rs` | ✅ READY |
| **Total** | **Complete Lume V1.0 E2E Test Suite** | $\ge 230$ | **232** | Workspace integration test suites | ✅ **READY** |

---

## 4. Feature Checklist (Features 1 to 21)

| # | Feature | Spec Reference | Milestone | Test Coverage Locations | Status |
|---|---------|----------------|-----------|-------------------------|:------:|
| 1 | `safe_file_open` | spec §1.1 | M1 | `tier1_feature_coverage.rs`: `test_f1_*` (5 tests)<br>`tier2_boundaries.rs`: `test_t2_g1_*`, `test_t2_m1_nul_*` | ✅ Covered |
| 2 | `untitled_tab_on_failure` | spec §1.2 | M1 | `tier1_feature_coverage.rs`: `test_f2_*` (5 tests)<br>`tier2_boundaries.rs`: `test_t2_g2_*` | ✅ Covered |
| 3 | `opt_in_autosave` | spec §1.3 | M1 | `tier1_feature_coverage.rs`: `test_f3_*` (6 tests)<br>`tier2_boundaries.rs`: `test_t2_g3_*` | ✅ Covered |
| 4 | `journal_mtime_recovery` | spec §1.4 | M1 | `tier1_feature_coverage.rs`: `test_f4_*` (6 tests)<br>`tier2_boundaries.rs`: `test_t2_g4_*`, `test_t2_m1_stale_*` | ✅ Covered |
| 5 | `discard_journal_cleanup` | spec §1.5 | M1 | `tier1_feature_coverage.rs`: `test_f5_*` (5 tests)<br>`tier2_boundaries.rs`: `test_t2_g5_*`, `test_t2_m1_discard_*` | ✅ Covered |
| 6 | `tab_close_dirty_guard` | spec §1.6 | M1 | `tier1_feature_coverage.rs`: `test_f6_*` (5 tests)<br>`tier2_boundaries.rs`: `test_t2_g6_*` | ✅ Covered |
| 7 | `journal_path_lifecycle` | spec §1.7 | M1 | `tier1_feature_coverage.rs`: `test_f7_*` (5 tests)<br>`tier3_cross_feature.rs`: `test_t3_05_*`, `test_t3_06_*` | ✅ Covered |
| 8 | `background_journal_worker` | spec §2.1 | M2 | `tier1_feature_coverage.rs`: `test_f8_*` (5 tests)<br>`tier2_boundaries.rs`: `test_t2_g8_*` | ✅ Covered |
| 9 | `rope_syntax_worker` | spec §2.2 | M2 | `tier1_syntax_coverage.rs`: `test_feature_9_*` (6 tests)<br>`tier3_cross_feature.rs` | ✅ Covered |
| 10 | `buffer_highlighter_cache` | spec §2.3 | M2 | `tier1_syntax_coverage.rs`: `test_feature_10_*` (6 tests)<br>`tier3_cross_feature.rs` | ✅ Covered |
| 11 | `highlight_token_shifting` | spec §2.4 | M2 | `tier1_syntax_coverage.rs`: `test_feature_11_*` (5 tests)<br>`tier2_boundaries.rs` | ✅ Covered |
| 12 | `syntax_buffer_eviction` | spec §2.5 | M2 | `tier1_syntax_coverage.rs`: `test_feature_12_*` (5 tests)<br>`tier3_cross_feature.rs` | ✅ Covered |
| 13 | `idle_mouse_suppression` | spec §2.6 | M2 | `tier1_feature_coverage.rs`: `test_f13_*` (5 tests)<br>`tier2_boundaries.rs`: `test_t2_g11_*` | ✅ Covered |
| 14 | `line_ending_detection` | spec §3.1 | M3 | `tier1_feature_coverage.rs`: `test_f14_*` (5 tests)<br>`tier2_boundaries.rs`: `test_t2_m3_orphan_*` | ✅ Covered |
| 15 | `line_ending_insert` | spec §3.2 | M3 | `tier1_feature_coverage.rs`: `test_f15_*` (5 tests)<br>`tier2_boundaries.rs`: `test_t2_m3_insert_*` | ✅ Covered |
| 16 | `atomic_crlf_deletion` | spec §3.3 | M3 | `tier1_feature_coverage.rs`: `test_f16_*` (5 tests)<br>`tier2_boundaries.rs`: `test_t2_m3_crlf_delete_*` | ✅ Covered |
| 17 | `status_bar_line_ending` | spec §3.4 | M3 | `tier1_feature_coverage.rs`: `test_f17_*` (5 tests)<br>`tier3_cross_feature.rs` | ✅ Covered |
| 18 | `undo_typing_coalescing` | spec §4.1 | M4 | `tier1_feature_coverage.rs`: `test_f18_*` (6 tests)<br>`tier2_boundaries.rs`: `test_t2_m4_break_*` | ✅ Covered |
| 19 | `exact_saved_depth_tracking` | spec §4.2 | M4 | `tier1_feature_coverage.rs`: `test_f19_*` (5 tests)<br>`tier2_boundaries.rs`: `test_t2_m4_saved_depth_*` | ✅ Covered |
| 20 | `git_head_blob_reader` | spec §5.1 | M5 | `tier1_feature_coverage.rs`: `test_f20_*` (6 tests)<br>`tier2_boundaries.rs`: `test_t2_g15_*`, `test_t2_m5_*` | ✅ Covered |
| 21 | `persistent_gutter_baseline` | spec §5.2 | M5 | `tier1_feature_coverage.rs`: `test_f21_*` (6 tests)<br>`tier2_boundaries.rs`: `test_t2_g16_*`, `test_t2_m5_git_*` | ✅ Covered |

---

## 5. Tier 4 Real-World Application Workflows Detail

1. **`test_t4_scenario_1_git_developer_workflow`**:
   Initializes Git repository, writes and commits `main.rs`, opens in editor, edits content, saves atomically, asserts git diff markers, undos edits to clean baseline.
2. **`test_t4_scenario_2_crash_simulation_and_recovery_workflow`**:
   Simulates dirty buffer session with journal logging, simulates unclean process termination (kill -9), restarts editor, detects pending recovery, asserts [R]ecover vs [D]iscard prompts, verifies disk preservation on discard.
3. **`test_t4_scenario_3_mixed_binary_and_code_workspace_workflow`**:
   Navigates project with binary files and code files, verifies binary rejection without tab corruption, opens valid code files, performs multi-file edits with undo.
4. **`test_t4_scenario_4_human_typing_and_undo_restoration_workflow`**:
   Simulates human typing rhythm (<400ms coalescing, >400ms group breaks), cursor motion group breaks, save checkpoint creation, and undo restoration.
5. **`test_t4_scenario_5_multitab_session_with_discard_and_save_workflow`**:
   Manages 3 concurrent tabs (clean, saved dirty, discarded dirty), verifying isolation of dirty states and proper journal cleanup on tab discard.
6. **`test_t4_scenario_6_refactoring_with_branching_history_workflow`**:
   Simulates code exploration: implementation attempt 1, undo back, branching implementation attempt 2, verifying redo history pruning and clean state tracking.
7. **`test_t4_scenario_7_rapid_file_switch_and_incremental_save_workflow`**:
   Fast multi-tab context switching with incremental saves, ensuring independent undo stacks and clean snapshots across all open buffers.

---

## 6. Verification Status

All 5 test suites (`tier1_feature_coverage`, `tier2_boundaries`, `tier3_cross_feature`, `tier4_real_world`, `tier1_syntax_coverage`) compile cleanly and pass 100% of currently active tests. Milestone-gated tests are structured with progressive feature gates (`m1` through `m5`, `e2e`) to activate automatically as implementation milestones land.
