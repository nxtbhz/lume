# Lume V1.0 Testing Infrastructure (`TEST_INFRA.md`)

## 1. Overview & Testing Philosophy

The Lume V1.0 hardening test suite validates the stability, data safety, POSIX durability, and user interface contracts of the Lume editor engine.
The test architecture strictly enforces:
- **Opaque-Box Verification**: Tests are derived strictly from formal specifications (`2026-11-07-v1-readiness-spec.md`), user requirements (`ORIGINAL_REQUEST.md`), and public interface contracts (`PROJECT.md`). Tests exercise behavior through public APIs without coupling to private implementation details.
- **Progressive Testability**: Tests are partitioned by milestone (M1 through M5) with explicit dependency isolation. Each milestone test suite is verifiable independently as that milestone lands.
- **Zero-Flake & Deterministic**: Tests create isolated temporary directories, clean up after themselves, avoid wall-clock sleeps where possible, and use monotonic ordering.

---

## 2. 4-Tier Test Architecture

The test suite is structured into four distinct verification tiers:

### Tier 1 — Feature Coverage (Isolated Happy Path)
- **Target**: Every feature in `PROJECT.md § Feature Inventory` (Features 1 to 21).
- **Requirement**: $\ge 5$ distinct test cases per feature (105+ tests total).
- **Objective**: Verify that the primary path and public contract of each feature behaves exactly as specified under standard operational conditions.

### Tier 2 — Boundary & Corner Cases (Adversarial Stress)
- **Target**: Boundary conditions, format limits, and hostile inputs across all features.
- **Requirement**: $\ge 5$ distinct boundary cases per feature (105+ tests total).
- **Edge Conditions Covered**:
  - Empty files (0 bytes) vs huge files.
  - NUL byte `\0` located at exact byte boundaries (offset 0, offset 8191, offset 8192).
  - Malformed UTF-8 sequences (truncated multi-byte codepoints).
  - Zero-delay vs rapid debounced autosave.
  - Line ending transitions (CRLF $\leftrightarrow$ LF, stray `\r` without `\n`, orphan `\n`).
  - Undo stack limits (5,000 entries / 50 MB quota exhaustion).
  - Git repository states (detached HEAD, unborn branch, submodules, non-git directories).
  - Journal staleness, missing journals, and corruption recovery.

### Tier 3 — Cross-Feature Combinations (Pairwise Interaction)
- **Target**: Concurrency, state synchronization, and pairwise interaction between independent subsystems.
- **Requirement**: Comprehensive pairwise tests ($\ge 15$ scenarios).
- **Key Interactions**:
  1. Safe file open with CRLF line ending detection.
  2. Opt-in autosave disabled interacting with crash journal recovery.
  3. Undo typing coalescing interacting with atomic CRLF backspace/delete.
  4. Git HEAD diff baseline interacting with atomic save and undo back to `saved_depth`.
  5. Background `JournalWorker` asynchronous queuing interacting with discard cleanup.
  6. Split view shared buffer editing interacting with `SyntaxWorker` token shifting and tab close.
  7. Multi-tab lifecycle with file rename updating journal paths across active buffers.

### Tier 4 — Real-World Application Scenarios
- **Target**: Realistic end-to-end developer workflows simulating full sessions.
- **Requirement**: $\ge 5$ complex multi-step scenarios.
- **Scenarios**:
  1. *Git Developer Workflow*: Editing tracked files with CRLF, saving, verifying gutter diff markers persist across saves, undoing back to clean state.
  2. *Crash Recovery Simulation*: Creating a dirty buffer, simulating unclean exit with active journal, reopening editor, asserting prompt modal, executing `[R]ecover` vs `[D]iscard`.
  3. *Mixed Binary & Code Project*: Navigating workspaces containing both binary files (`.bin`, `.png`) and valid code, verifying graceful rejection without tab corruption, followed by normal code editing.
  4. *Rapid Typing & Micro-Pause*: Simulating human keystrokes (<400ms coalescing, >400ms breaking groups, cursor motions breaking groups, save establishing clean baseline).
  5. *Multi-Tab Background Worker Session*: Managing multiple dirty tabs, async journal snapshots, closing tabs via `[x]` with confirmation guards, and selective saving/discarding.

---

## 3. Directory & File Organization

The integration test suite is located in crate-level integration test directories adhering to Cargo and project conventions:

```
crates/lume-core/tests/
├── tier1_feature_coverage.rs   # Tier 1: Features 1-8, 14-21 (>= 80 tests)
├── tier2_boundaries.rs         # Tier 2: Boundary & stress tests (>= 105 tests)
├── tier3_cross_feature.rs      # Tier 3: Pairwise cross-feature interactions (>= 15 tests)
└── tier4_real_world.rs         # Tier 4: Real-world developer workflows (>= 7 scenarios)

crates/lume-syntax/tests/
└── tier1_syntax_coverage.rs    # Tier 1: Syntax features 9-12 (>= 20 tests)
```

---

## 4. Test Runner Commands

### Standard CI Validation (All Passing Checks)
```bash
# Run standard workspace tests without milestone flags (must pass in CI)
cargo test --workspace --locked

# Full CI gate
./scripts/ci.sh
```

### Milestone-Specific Execution (Progressive Testability)
As implementation milestones land, execute the target test suite using feature flags:

```bash
# Milestone 1: Safe Open, Stale Journal Deletion, Discard & Autosave
cargo test -p lume-core --features m1

# Milestone 2: Background Workers (Journal & Syntax) & Idle Mouse
cargo test -p lume-core --features m2
cargo test -p lume-syntax --features m2

# Milestone 3: Line Ending Detection, Atomic \r\n Editing & Status Bar
cargo test -p lume-core --features m3

# Milestone 4: Undo Coalescing & Exact saved_depth Clean-State Tracking
cargo test -p lume-core --features m4

# Milestone 5: Git HEAD Gutter Diff Baseline Across Saves
cargo test -p lume-core --features m5
```

### Full E2E Test Suite Execution (Milestone 6 / Acceptance)
```bash
# Run all E2E integration test suites across all crates
cargo test --workspace --features e2e

# Run specific tier across all features
cargo test -p lume-core --test tier1_feature_coverage --features e2e
cargo test -p lume-syntax --test tier1_syntax_coverage --features e2e
cargo test -p lume-core --test tier2_boundaries --features e2e
cargo test -p lume-core --test tier3_cross_feature --features e2e
cargo test -p lume-core --test tier4_real_world --features e2e
```

---

## 5. Summary Table by Tier

| Tier | Focus | Scope | Min Required | Implemented | Status |
|:---|:---|:---|:---:|:---:|:---:|
| **Tier 1** | Feature Coverage | Features 1–21 (isolated happy path) | $\ge 105$ | 105 | Ready |
| **Tier 2** | Boundary & Corner | Features 1–21 (adversarial edge cases) | $\ge 105$ | 105 | Ready |
| **Tier 3** | Cross-Feature | Pairwise subsystem interactions | $\ge 15$ | 15 | Ready |
| **Tier 4** | Real-World Workflows | Complex multi-step session simulations | $\ge 5$ | 7 | Ready |
| **Total** | Full E2E Suite | Complete Lume V1.0 Hardening | $\ge 230$ | **232** | Ready |
