# Lume benchmarks and performance caveats

This report records deterministic build artifacts, Criterion microbenchmarks, and exploratory startup/memory observations measured on commit `a8df968` on `2026-11-16`. Every runtime measurement carries its exact command, host state, and validity limits so readers do not confuse headless or microbenchmark numbers with full interactive terminal latency.

## Measurement environment and confidence tiers

All measurements ran on Ubuntu 26.04.1 LTS (`Linux 7.0.0-43-generic`, `x86_64`) using an AMD Ryzen 5 3400G (`8` logical CPUs) and `13 GiB` RAM (`free -h`). The host ran with the `schedutil` CPU frequency governor active and a non-isolated desktop 1-minute load average of `2.08`. Builds used `rustc 1.100.0 (0ef1bc386 2026-11-09)` and `cargo 1.100.0 (3acc79c0c 2026-11-05)` with `--locked` dependencies from `Cargo.lock`.

Deterministic and CI-verified evidence covers exact release binary byte counts, ELF section sizes, `ldd` outputs, `scripts/ci.sh` test totals, and Criterion 95% confidence intervals produced by `crates/lume-core/benches/latency.rs`. Those numbers are directly reproducible from commit `a8df968`.

Exploratory evidence covers `hyperfine` startup timings, `/usr/bin/time -v` Peak RSS readings, and single-run Python PTY probes. Because those runs used small sample counts under `schedutil` frequency scaling and measured non-PTY headless paths, they do not support statistically powered cross-editor claims.

## Release binary footprint and test counts

Building `cargo build --release --locked --bin lume` applies `[profile.release]` (`opt-level = 3`, `lto = "thin"`, `codegen-units = 1`, `strip = true`, `Cargo.toml:23-27`). That build embeds all six Tree-sitter C parsers into one ELF executable with zero external runtime directory, linking dynamically only against standard C/GCC runtime libraries (`libc.so.6`, `libgcc_s.so.1`, `linux-vdso.so.1`, and `ld-linux-x86-64.so.2`). Running `./scripts/ci.sh` verifies formatting, Clippy (`-D warnings`), benchmark compilation, rustdoc (`-D warnings`), and both feature matrices.

| Artifact or check | Exact command | Measured result (commit `a8df968`) | Notes |
| :--- | :--- | :--- | :--- |
| Binary file size | `stat -c%s target/release/lume` | `8,164,200` bytes (`7.79 MiB`) | Stripped ELF `x86_64` executable |
| ELF section sizes | `size target/release/lume` | `text = 8,068,881`, `data = 91,848`, `bss = 11,755` | Total `8,172,484` bytes |
| Dynamic libraries | `ldd target/release/lume` | 4 entries (`linux-vdso.so.1`, `libgcc_s.so.1`, `libc.so.6`, `/lib64/ld-linux-x86-64.so.2`) | `libm` is not linked |
| Default test suite | `cargo test --workspace --locked` | `529` passed, `0` failed, `0` ignored | `309` `lume-core`, `33` `lume-syntax`, `187` `lume-tui` |
| All-features test suite | `cargo test --workspace --all-features --locked` | `598` passed, `0` failed, `0` ignored | `368` `lume-core`, `43` `lume-syntax`, `187` `lume-tui` |

## Headless core microbenchmarks (Criterion)

Running `cargo bench -p lume-core --bench latency -- --warm-up-time 1 --measurement-time 3` (`criterion 0.5.1`) collects `100` samples per benchmark after a `1.0 s` warmup across three groups in `crates/lume-core/benches/latency.rs`. Local Criterion delta percentages are omitted because they compare against whatever prior cache existed in `target/criterion`.

| Group / benchmark | Criterion 95% CI `[low, est, high]` | Outliers (`100` samples) | What the benchmark actually executes |
| :--- | :--- | :--- | :--- |
| `buffer_100k_latency/insert_char_middle` | `[1.9340 µs, 1.9829 µs, 2.0361 µs]` | `10` (`3` mild, `7` severe) | Clones shared `100k`-line `Rope`, inserts `"x"` at line `50,000`, drops mutated `Rope` and `TextDelta` inside timer |
| `buffer_100k_latency/insert_char_end` | `[1.4176 µs, 1.4322 µs, 1.4484 µs]` | `7` (`7` mild) | Clones shared `100k`-line `Rope`, inserts `"x"` at EOF, drops inside timer |
| `buffer_100k_latency/insert_char_beginning` | `[1.5207 µs, 1.5292 µs, 1.5392 µs]` | `9` (`6` mild, `3` severe) | Clones shared `100k`-line `Rope`, inserts `"x"` at byte `0`, drops inside timer |
| `buffer_100k_latency/delete_char_middle` | `[1.4059 µs, 1.4088 µs, 1.4119 µs]` | `10` (`5` mild, `5` severe) | Clones shared `100k`-line `Rope`, deletes 1 byte at line `50,000`, drops inside timer |
| `startup_latency/editor_new_empty` | `[173.19 ns, 177.36 ns, 183.59 ns]` | `12` (`7` mild, `5` severe) | Allocates an empty in-memory `Editor` and one `Tab`; no I/O or terminal setup |
| `startup_latency/editor_from_str_small` | `[657.97 ns, 660.58 ns, 663.51 ns]` | `11` (`4` mild, `7` severe) | Allocates an in-memory `Editor` from a 3-line `&str` |
| `indexing_100k_latency/line_to_byte_middle` | `[55.439 ns, 55.664 ns, 55.930 ns]` | `12` (`6` mild, `6` severe) | Repeatedly queries fixed line `50,000` on a hot cached `RopeBuffer` |
| `indexing_100k_latency/byte_to_line_middle` | `[50.693 ns, 50.966 ns, 51.460 ns]` | `9` (`5` mild, `4` severe) | Repeatedly queries fixed byte offset of line `50,000` on a hot cached `RopeBuffer` |

Three methodological constraints apply to `crates/lume-core/benches/latency.rs`:

1. `buffer_100k_latency` uses `b.iter_batched(|| base_buffer.clone(), |mut buf| { ... }, BatchSize::SmallInput)` without returning `buf` (`crates/lume-core/benches/latency.rs:23-61`), so every timing sample includes Copy-on-Write tree-node cloning (`Arc`-shared `Rope`) plus deallocating the modified tree and `TextDelta`.
2. `indexing_100k_latency` calls `line_to_byte(50_000)` and `byte_to_line` at one fixed offset (`crates/lume-core/benches/latency.rs:93-100`), keeping the exact tree path warm in L1/L2 cache and branch predictors.
3. No Criterion benchmarks currently exist in the repository for `fuzzy_match` ($O(m \cdot n^2)$ DP), `compute_rope_diffs` (LCS table), `search`, `SyntaxHighlighter`, or `render_pane_viewport`.

## Exploratory non-PTY startup and memory observations

Exploratory `hyperfine 1.19.0` and `/usr/bin/time -v` runs on `a8df968` compared `lume --bench-startup`, `vim --clean -e -s -c q` (`VIM 9.1`), and `nvim --clean --headless -c q` (`NVIM v0.11.6`). Helix (`hx`) is absent from the test host, so this table includes no Helix rows.

`lume --bench-startup` opens target files, initializes `FileExplorer` and `SyntaxWorker`, and renders one frame to `std::io::sink()` without running `TerminalGuard::init()` (`crates/lume-tui/src/main.rs:73-81`). Because `vim -e -s` and `nvim --headless` also skip TTY setup, these numbers reflect headless load-and-exit cost rather than interactive terminal startup.

| Workload | Editor command | Mean ± stddev (ms) | Median (ms) | Min..max (ms) | Peak RSS (KiB) | Warmup / runs |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| Empty buffer | `lume --bench-startup` | `1.14 ± 0.26` | `1.04` | `0.94..1.96` | `3,556` (`3.47 MiB`) | `5` / `20` |
| Empty buffer | `vim --clean -e -s -c q` | `10.64 ± 2.03` | `10.12` | `8.61..18.00` | `11,456` (`11.19 MiB`) | `5` / `20` |
| Empty buffer | `nvim --clean --headless -c q` | `12.30 ± 3.03` | `11.13` | `10.40..21.21` | `11,584` (`11.31 MiB`) | `5` / `20` |
| `crates/lume-tui/src/app.rs` (`32,508` B, `898` lines) | `lume --bench-startup` | `5.02 ± 0.90` | `4.79` | `4.04..7.84` | `5,236` (`5.11 MiB`) | `5` / `20` |
| `crates/lume-tui/src/app.rs` (`32,508` B, `898` lines) | `vim --clean -e -s` | `18.73 ± 4.72` | `16.50` | `14.43..31.83` | `12,312` (`12.02 MiB`) | `5` / `20` |
| `crates/lume-tui/src/app.rs` (`32,508` B, `898` lines) | `nvim --clean --headless` | `25.68 ± 5.06` | `23.90` | `19.93..32.53` | `12,376` (`12.09 MiB`) | `5` / `20` |
| Synthetic C `100,000` lines (`5,277,780` B) | `lume --bench-startup` | `13.33 ± 3.30` | `12.29` | `10.59..22.44` | `9,612` (`9.39 MiB`) | `3` / `15` |
| Synthetic C `100,000` lines (`5,277,780` B) | `vim --clean -e -s` | `34.63 ± 5.16` | `32.25` | `28.96..45.18` | `18,012` (`17.59 MiB`) | `3` / `15` |
| Synthetic C `100,000` lines (`5,277,780` B) | `nvim --clean --headless` | `41.90 ± 6.38` | `39.03` | `35.24..57.39` | `18,616` (`18.18 MiB`) | `3` / `15` |
| Synthetic C `500,000` lines (`27,277,780` B) | `lume --bench-startup` | `58.38 ± 5.32` | `58.22` | `51.09..69.03` | `34,308` (`33.50 MiB`) | `2` / `10` |
| Synthetic C `500,000` lines (`27,277,780` B) | `vim --clean -e -s` | `100.73 ± 5.50` | `99.11` | `93.86..109.94` | `42,412` (`41.42 MiB`) | `2` / `10` |
| Synthetic C `500,000` lines (`27,277,780` B) | `nvim --clean --headless` | `93.10 ± 3.26` | `92.51` | `89.52..100.20` | `43,336` (`42.32 MiB`) | `2` / `10` |

## Interactive PTY probe and benchmark.sh caveats

Real interactive startup executes `TerminalGuard::init` (`crates/lume-tui/src/terminal.rs:59-125`), which queries Kitty keyboard support via `crossterm::terminal::supports_keyboard_enhancement()` (`\x1b[?u\x1b[c`). In a single-shot Python `pty.openpty()` probe against `target/release/lume` on `a8df968`, answering that query immediately with `\x1b[?0u\x1b[?62;c` reached `\x1b[?2026l` in `6.08 ms` and exited on `Ctrl+Q` in `6.65 ms`. Omitting a terminal reply stalled `TerminalGuard::init` until `2,005.39 ms` (`crossterm`'s 2-second timeout) before rendering the first frame.

Reports generated by `benchmark.sh` (`docs/Benchmark.md`) are not equivalent across editors for three verified reasons in `benchmark.sh` and `scripts/hx-bench.c`. First, `benchmark.sh:166-195` pairs headless runs of `lume --bench-startup`, `vim -e -s`, and `nvim --headless` against `scripts/hx-bench.c`, which launches `hx` inside a real PTY and polls `waitpid` with `usleep(500)`. Second, `benchmark.sh:33-38` exits if `hx` is absent, and `benchmark.sh:395`, `399` labels `crates/lume-tui/src/main.rs` (`92` lines at `a8df968`) as `1.580` lines. Third, `benchmark.sh:166-195` hardcodes `runs` between `8` and `30` without pilot variance checks or CPU frequency pinning.
