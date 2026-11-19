# Lume

Lume is a terminal text editor written in Rust 2021 across three workspace crates: `lume-core` for rope buffers and crash journaling, `lume-syntax` for background highlighting, and `lume-tui` for the `crossterm` terminal frontend. The editor pairs modeless GUI shortcuts with an optional modal Vim mode, a workspace-confined file explorer, and two-pane splits inside one stripped binary with embedded grammars.

Build measurements, shared library linkage, test suite totals, Criterion microbenchmark intervals, and startup caveats at commit `a8df968` live in [docs/benchmarks.md](docs/benchmarks.md).

## Quickstart

### Prerequisites

Compiling the workspace requires Rust `1.88.0` or newer because `lume-core` calls `slice::as_chunks` (`crates/lume-core/src/search.rs:13`) and `usize::is_multiple_of` (`crates/lume-core/src/editor/indent.rs:557`). Root `Cargo.toml` does not set `rust-version`, so Cargo accepts older toolchains until compilation fails.

Interactive sessions require a terminal emulator supporting ANSI 24-bit color, the alternate screen, and SGR mouse reporting. Keyboard enhancements are optional. If the terminal does not answer the capability probe, Lume uses standard terminal input after the 2-second probe timeout.

### Build and run

Clone the repository, compile the release binary with locked dependencies, and launch the editor on a file, directory, or wildcard glob:

```bash
git clone https://github.com/nxtbhz/lume.git
cd lume
cargo build --release --locked --bin lume
./target/release/lume [PATH...]
```

Running `lume .` opens the working directory in the sidebar tree alongside an untitled tab. Passing multiple files opens one tab per file, and paths containing `*` or `?` expand internally (`crates/lume-tui/src/args.rs:60`).

Use `Ctrl+C` and `Ctrl+V` to copy and paste within Lume, including across tabs. See the [terminal keymap](docs/reference.md#modeless-editor-keybindings) for system clipboard support.

### Verification and benchmarks

The repository includes a local verification script for formatting, Clippy (`-D warnings`), benchmark compilation, rustdoc warnings, and both feature matrices (`default` and `--all-features`):

```bash
./scripts/ci.sh
cargo bench -p lume-core --bench latency
```

Read [docs/benchmarks.md](docs/benchmarks.md) before quoting results from `cargo bench` or running `benchmark.sh`. That report documents the test host environment, Criterion sampling conditions, and why `benchmark.sh` mixes headless and PTY measurements across editors.

## Workspace layout

The root workspace (`Cargo.toml`) separates core text structures, syntax tokenization, and terminal rendering across three crates:

```text
lume/
├── Cargo.toml                  # Workspace manifest and release profile (opt-level=3, thin LTO, strip)
├── benchmark.sh                # Automated comparison script (see caveats in docs/benchmarks.md)
├── scripts/
│   ├── ci.sh                   # Local CI script (fmt, clippy, benches check, doc, tests)
│   ├── bench.c                 # PTY wrapper source (identical to hx-bench.c)
│   └── hx-bench.c              # PTY harness used by benchmark.sh for Helix
├── docs/
│   ├── architecture.md         # Crate internals, concurrency, persistence, security, and known limits
│   ├── reference.md            # Exhaustive CLI, shortcuts, commands, mouse, and language reference
│   └── benchmarks.md           # Footprint, test totals, Criterion intervals, and startup caveats
└── crates/
    ├── lume-core/              # Headless editor: rope buffer, cursors, history, autosave, journal, diff, git
    ├── lume-syntax/            # 6 Tree-sitter grammars + 8 custom lexers + background worker thread
    └── lume-tui/               # Crossterm TUI binary: event loop, explorer, quick open, splits, modal mode
```

Integration tests sit alongside unit tests inside each crate: `crates/lume-core/tests/` exercises boundary, cross-feature, and crash-recovery scenarios; `crates/lume-syntax/tests/` covers grammar and worker behavior; and `crates/lume-tui/src/tests/` covers key routing, mouse hit-testing, splits, and explorer symlink guards.

## Implemented capabilities and boundaries

`lume-core` implements a `ropey`-backed `RopeBuffer` and transactional multi-cursor edits with rollback on boundary errors. It includes a `5,000`-entry / `50 MiB` `History` stack with save-depth tracking, atomic temp-file saves (`0o600` on Unix), an async background save path (`save_async`), and an append-only crash journal (`Journal`). It also ships SWAR ASCII and Unicode search, fuzzy path matching, and Git `HEAD` line diffs.

`lume-syntax` runs incremental `Tree::edit` parsing on a dedicated worker thread for six compiled Tree-sitter grammars and routes eight additional formats through custom lexers. `lume-tui` builds on both crates with multi-tab editing, a collapsible 26-column workspace explorer, and a `50,000`-file Quick Open overlay. The frontend also supplies a 37-item Command Palette, a Which-Key sheet, two-pane splits, find-and-replace, OSC 52 clipboard output, and optional Vim modal bindings.

Several subsystems implemented in `lume-core` or sketched in earlier plans remain incomplete or unwired in `lume-tui` at commit `a8df968`:

- Pressing `Ctrl+S` invokes synchronous `Editor::save()` on the UI thread (`crates/lume-tui/src/commands.rs:231`), while `AutoSaveController` (`enabled: false` by default) and `FocusLost` call `save_async()`.
- Runtime settings such as `tab_size` (`4`), theme colors, and `autosave.enabled` have no config file, CLI flag, or Command Palette toggle.
- Multi-cursor batches work in `lume-core`, yet `lume-tui` provides no binding to add secondary cursors and renders only the primary selection (`crates/lume-tui/src/render/editor.rs:44`).
- `Language::TypeScript` reuses `tree_sitter_javascript::LANGUAGE` (`crates/lume-syntax/src/language.rs:132`), and `Language::Css` routes to `lex_html_css` (`crates/lume-syntax/src/highlighter.rs:167`), which only highlights `<...>` tags.
- `Journal` snapshots and V1-to-V2 upgrades open the live journal file with `.truncate(true)` instead of staging a temp rename, records lack CRC checksums, and unrecognized headers fall back to `String::from_utf8_lossy` (`crates/lume-core/src/journal.rs:362`, `398`, `476`).
- Lume has no LSP client, debugger (DAP), embedded terminal, workspace-wide text search, plugin runtime, or `-` standard input pipe reader.

## Documentation

Each guide covers one topic without duplicating raw reference tables:

- [docs/architecture.md](docs/architecture.md) covers crate boundaries, threading, persistence protocols, symlink security checks, and implementation trade-offs.
- [docs/reference.md](docs/reference.md) lists all CLI options, modeless and modal shortcuts, key chords, explorer keys, mouse gestures, Command Palette entries, and language rules.
- [docs/benchmarks.md](docs/benchmarks.md) reports measured binary footprint, test totals, `lume-core` Criterion intervals, exploratory startup and RSS observations, and benchmark caveats.

## License

Dual-licensed under either the [MIT License](LICENSE-MIT) or the [Apache License, Version 2.0](LICENSE-APACHE), at your option; see [LICENSE](LICENSE).
