# Lume reference

This reference lists every CLI flag, keybinding, command ID, mouse action, syntax language, environment variable, and Cargo feature defined in `lume-core`, `lume-syntax`, and `lume-tui` at commit `a8df968`.

## Command-line interface

Invocation syntax is `lume [OPTIONS] [PATH]...` (`crates/lume-tui/src/args.rs:152-230`).

| Flag or argument | Default | Behavior | Source |
| :--- | :--- | :--- | :--- |
| `-h`, `--help` | Off | Print CLI usage text and exit with status `0` | [args.rs](../crates/lume-tui/src/args.rs#L170-L172) |
| `-v`, `--version` | Off | Print `lume 0.1.0 (M1 functional MVP)` and exit with status `0` | [main.rs](../crates/lume-tui/src/main.rs#L69-L72) |
| `--bench-startup` | Off | Open targets, render once into `std::io::sink()`, print elapsed time, and exit | [main.rs](../crates/lume-tui/src/main.rs#L73-L81) |
| `[PATH]...` | Empty workspace | Open directories as workspace root (first dir wins) and files as tabs | [args.rs](../crates/lume-tui/src/args.rs#L193-L208) |

Arguments containing `*` or `?` expand via `expand_glob` (`crates/lume-tui/src/args.rs:60-149`), skipping hidden entries unless the glob segment itself starts with `.`. Passing `-` does not read standard input; `parse_cli_args` passes `"-"` through as a file path (`crates/lume-tui/src/args.rs:163`).

## Modeless editor keybindings

Modeless mode is active on startup (`modal_enabled = false`, `crates/lume-tui/src/app.rs:209`). Global shortcuts with `Ctrl` or `Alt` take precedence before mode-specific editor keys (`crates/lume-tui/src/input/key.rs:73-226`).

| Shortcut | Action | Notes and fallbacks | Source |
| :--- | :--- | :--- | :--- |
| `Ctrl+Q` | Quit application | Prompts `[S]ave / [D]iscard / [C]ancel` if any tab is dirty | [key.rs](../crates/lume-tui/src/input/key.rs#L86-L89) |
| `Ctrl+O` | Open file prompt | Tab autocompletes directory entries | [key.rs](../crates/lume-tui/src/input/key.rs#L102-L105) |
| `Ctrl+P` | Open Quick Open | Fuzzy workspace file finder | [key.rs](../crates/lume-tui/src/input/key.rs#L94-L100) |
| `Ctrl+Shift+P` or `Alt+P` | Open Command Palette | Use `Alt+P` on terminals where `Ctrl+Shift+P` reports as `Ctrl+P` | [key.rs](../crates/lume-tui/src/input/key.rs#L79-L97) |
| `Ctrl+S` | Save active file | Synchronous atomic write; opens Save As if untitled | [commands.rs](../crates/lume-tui/src/commands.rs#L229-L238) |
| `Ctrl+Shift+S` | Save file as | Opens path prompt with suggestions | [key.rs](../crates/lume-tui/src/input/key.rs#L106-L117) |
| `Ctrl+T` | New untitled tab | Switches focus to new tab | [key.rs](../crates/lume-tui/src/input/key.rs#L122-L125) |
| `Ctrl+W` | Close active tab | Prompts if dirty; keeps at least one tab open | [key.rs](../crates/lume-tui/src/input/key.rs#L126-L129) |
| `Ctrl+Tab` or `Alt+Right` | Next tab | Wraps around tabs and triggers recovery check if needed | [key.rs](../crates/lume-tui/src/input/key.rs#L130-L137) |
| `Ctrl+Shift+Tab`, `BackTab`, or `Alt+Left` | Previous tab | Wraps around tabs and triggers recovery check if needed | [key.rs](../crates/lume-tui/src/input/key.rs#L138-L141) |
| `Ctrl+B`, `Ctrl+E`, or `F2` | Toggle File Explorer | `F2` triggers rename when explorer is visible and focused | [key.rs](../crates/lume-tui/src/input/key.rs#L118-L121) |
| `Ctrl+A` | Select all | Selects full active buffer | [key.rs](../crates/lume-tui/src/input/key.rs#L142-L145) |
| `Ctrl+Z` | Undo | Restores cursors and updates dirty/journal state | [key.rs](../crates/lume-tui/src/input/key.rs#L146-L149) |
| `Ctrl+Y` or `Ctrl+R` | Redo | Reapplies undone entry | [key.rs](../crates/lume-tui/src/input/key.rs#L150-L153) |
| `Ctrl+C` | Copy selection or line | Updates internal clipboard and emits OSC 52 | [commands.rs](../crates/lume-tui/src/commands.rs#L347-L352) |
| `Ctrl+X` | Cut selection or line | Updates internal clipboard and emits OSC 52 | [commands.rs](../crates/lume-tui/src/commands.rs#L353-L358) |
| `Ctrl+V` | Paste internal clipboard | External clipboard pastes use terminal bracketed paste (`AppEvent::Paste`) | [key.rs](../crates/lume-tui/src/input/key.rs#L162-L165) |
| `Ctrl+F` | Open Find bar | Case-insensitive substring search in active buffer | [key.rs](../crates/lume-tui/src/input/key.rs#L166-L169) |
| `Ctrl+H` | Open Replace bar | Case-insensitive find and replace | [key.rs](../crates/lume-tui/src/input/key.rs#L170-L173) |
| `F3` / `Shift+F3` | Find next / previous | Reuses last search query | [key.rs](../crates/lume-tui/src/input/key.rs#L206-L216) |
| `Ctrl+M` | Jump to matching bracket | Matches `()`, `[]`, `{}` | [key.rs](../crates/lume-tui/src/input/key.rs#L174-L177) |
| `Ctrl+G` | Open Go to Line prompt | Accepts `line`, `line:col`, `:col`, or `line:` | [key.rs](../crates/lume-tui/src/input/key.rs#L178-L181) |
| `Ctrl+K` | Enter chord state | Waits for second key (`\`, `-`, `w`, `q`, `m`) | [key.rs](../crates/lume-tui/src/input/key.rs#L182-L188) |
| `Ctrl+Alt+V` | Toggle modal Vim mode | Enters Normal mode when enabled | [key.rs](../crates/lume-tui/src/input/key.rs#L74-L77) |
| `F1` | Open Which-Key sheet | Any key or `Esc` dismisses the overlay | [key.rs](../crates/lume-tui/src/input/key.rs#L194-L197) |
| `Arrows`, `Home`, `End`, `PageUp`, `PageDown` | Move cursor | Hold `Shift` to extend selection | [key.rs](../crates/lume-tui/src/input/key.rs#L242-L255) |
| `Ctrl+Left` / `Ctrl+Right` | Word backward / word end forward | Hold `Shift` to extend selection | [key.rs](../crates/lume-tui/src/input/key.rs#L240-L241) |
| `Tab` | Insert four spaces | Inserts `"    "` at cursor | [key.rs](../crates/lume-tui/src/input/key.rs#L298-L312) |

Copy (`Ctrl+C`), cut (`Ctrl+X`), and Vim yank operations build ANSI OSC 52 escape sequences up to `MAX_OSC52_BYTES = 10 MiB` (`crates/lume-core/src/clipboard.rs:41`). When `$TMUX` is set or `$TERM` starts with `screen` or `tmux`, `osc52_sequence_auto` wraps the sequence in tmux DCS passthrough (`\x1bPtmux;...\x1b\\`, `crates/lume-core/src/clipboard.rs:66-72`).

## Key chords and modal Vim mode

Pressing `Ctrl+K` sets `pending_chord = Some('k')` until the next key (`crates/lume-tui/src/input/key.rs:45-71`).

| Second key | Command ID | Effect | Source |
| :--- | :--- | :--- | :--- |
| `\` or `\|` | `view.split_vertical` | Split editor side-by-side or switch split orientation to vertical | [key.rs](../crates/lume-tui/src/input/key.rs#L48-L50) |
| `-` or `_` | `view.split_horizontal` | Split editor top-and-bottom or switch split orientation to horizontal | [key.rs](../crates/lume-tui/src/input/key.rs#L51-L53) |
| `w` or `W` | `view.focus_next_split` | Switch focus between pane `0` and pane `1` | [key.rs](../crates/lume-tui/src/input/key.rs#L54-L56) |
| `q` or `Q` | `view.close_split` | Close active split pane | [key.rs](../crates/lume-tui/src/input/key.rs#L57-L59) |
| `m` or `M` | `language.set_mode` | Open language override selector for active tab | [key.rs](../crates/lume-tui/src/input/key.rs#L60-L62) |
| `Esc` or any other key | None | Cancel pending chord | [key.rs](../crates/lume-tui/src/input/key.rs#L63-L68) |

Modal mode (`Ctrl+Alt+V`) routes non-global editor keys through `handle_modal_key` across `Normal`, `Insert`, and `Visual` states (`crates/lume-tui/src/input/modal.rs:7-304`). Pressing `:` in Normal mode opens the Go to Line prompt (`crates/lume-tui/src/input/modal.rs:294-296`), not an ex command line.

| Mode | Key | Action | Source |
| :--- | :--- | :--- | :--- |
| Normal | `h`, `j`, `k`, `l` or `Arrows` | Move cursor left, down, up, right | [modal.rs](../crates/lume-tui/src/input/modal.rs#L226-L229) |
| Normal | `w` / `b` | Move to next word start / previous word start | [modal.rs](../crates/lume-tui/src/input/modal.rs#L230-L231) |
| Normal | `0` / `$` or `Home` / `End` | Move to line start / line end | [modal.rs](../crates/lume-tui/src/input/modal.rs#L232-L233) |
| Normal | `gg` / `G` | Move to start / end of buffer | [modal.rs](../crates/lume-tui/src/input/modal.rs#L184-L187) |
| Normal | `PageUp` / `PageDown` | Scroll by viewport height | [modal.rs](../crates/lume-tui/src/input/modal.rs#L235-L242) |
| Normal | `x` | Delete character (or selection) into yank register | [modal.rs](../crates/lume-tui/src/input/modal.rs#L243-L263) |
| Normal | `dd` / `yy` | Delete line / yank line into yank register, internal clipboard, and OSC 52 | [modal.rs](../crates/lume-tui/src/input/modal.rs#L188-L205) |
| Normal | `p` | Paste from `yank_register` (characterwise or linewise) | [modal.rs](../crates/lume-tui/src/input/modal.rs#L264-L269) |
| Normal | `u` / `Ctrl+R` | Undo / redo | [modal.rs](../crates/lume-tui/src/input/modal.rs#L270-L272) |
| Normal | `i` / `a` / `o` / `O` | Enter Insert mode at cursor, after cursor, below line, or above line | [modal.rs](../crates/lume-tui/src/input/modal.rs#L273-L290) |
| Normal | `v` | Enter Visual mode | [modal.rs](../crates/lume-tui/src/input/modal.rs#L291-L293) |
| Normal | `:` | Open Go to Line prompt | [modal.rs](../crates/lume-tui/src/input/modal.rs#L294-L296) |
| Normal | `Esc` | Clear pending `g`/`d`/`y` operator and collapse selection | [modal.rs](../crates/lume-tui/src/input/modal.rs#L212-L216) |
| Visual | `h`, `j`, `k`, `l`, `Arrows`, `w`, `b`, `0`, `$`, `gg`, `G` | Extend selection | [modal.rs](../crates/lume-tui/src/input/modal.rs#L117-L142) |
| Visual | `d` or `x` | Delete selection into `yank_register`, clipboard, and OSC 52; return to Normal | [modal.rs](../crates/lume-tui/src/input/modal.rs#L143-L158) |
| Visual | `y` | Yank selection into `yank_register`, clipboard, and OSC 52; return to Normal | [modal.rs](../crates/lume-tui/src/input/modal.rs#L159-L176) |
| Visual | `v` or `Esc` | Collapse selection and return to Normal | [modal.rs](../crates/lume-tui/src/input/modal.rs#L126-L130) |
| Insert | `Esc` | Return to Normal mode | [modal.rs](../crates/lume-tui/src/input/modal.rs#L17-L21) |
| Insert | Typing, `Enter`, `Tab`, `Backspace`, `Delete`, `Arrows`, `Ctrl+Left`/`Right` | Edit and navigate text identically to modeless mode | [modal.rs](../crates/lume-tui/src/input/modal.rs#L22-L113) |

## File explorer, overlays, and mouse actions

When `FileExplorer` is visible (`26` columns wide by default, `crates/lume-tui/src/explorer.rs:36-37`) and focused, keys route to `handle_explorer_key` (`crates/lume-tui/src/input/overlays.rs:646-708`). Hidden entries (except `.gitignore`), `target`, `node_modules`, and `*.lume_tmp` are omitted from the tree (`crates/lume-tui/src/explorer.rs:92-100`).

| Context | Input | Effect | Source |
| :--- | :--- | :--- | :--- |
| Explorer | `Up`, `Down`, `PageUp`, `PageDown` | Move tree selection | [overlays.rs](../crates/lume-tui/src/input/overlays.rs#L651-L666) |
| Explorer | `Left` / `Right` | Collapse folder (or jump to parent) / expand folder | [overlays.rs](../crates/lume-tui/src/input/overlays.rs#L667-L672) |
| Explorer | `Enter` or `Space` | Toggle folder or open selected file | [overlays.rs](../crates/lume-tui/src/input/overlays.rs#L673-L677) |
| Explorer | `Tab` or `Esc` | Move focus back to Editor | [overlays.rs](../crates/lume-tui/src/input/overlays.rs#L678-L680) |
| Explorer | `a` / `n` | Prompt to create file / folder under selected folder | [overlays.rs](../crates/lume-tui/src/input/overlays.rs#L681-L686) |
| Explorer | `m` or `F2` | Prompt to rename selected item (root disallowed) | [overlays.rs](../crates/lume-tui/src/input/overlays.rs#L687-L689) |
| Explorer | `d` or `Delete` | Prompt to delete selected folder (files cannot be deleted here) | [overlays.rs](../crates/lume-tui/src/input/overlays.rs#L690-L695) |
| Explorer | `r` | Refresh directory tree | [overlays.rs](../crates/lume-tui/src/input/overlays.rs#L696-L698) |
| Explorer | Alphanumeric, `.`, or `_` | Jump to next item starting with that character | [overlays.rs](../crates/lume-tui/src/input/overlays.rs#L699-L704) |
| Search / Replace bar | `Enter` / `Shift+Enter` | Next match (or replace current in Replacement field) / previous match | [overlays.rs](../crates/lume-tui/src/input/overlays.rs#L476-L526) |
| Search / Replace bar | `F3` / `Shift+F3` | Jump to next / previous match without modifying buffer | [overlays.rs](../crates/lume-tui/src/input/overlays.rs#L455-L475) |
| Search / Replace bar | `Ctrl+Alt+Enter` | Replace all occurrences (case-insensitive) | [overlays.rs](../crates/lume-tui/src/input/overlays.rs#L381-L396) |
| Search / Replace bar | `Tab`, `Ctrl+F`, `Ctrl+H`, `Ctrl+Z`, `Ctrl+Y`, `Ctrl+V`, `Esc` | Toggle field, switch mode, undo/redo, paste, or restore cursor and close | [overlays.rs](../crates/lume-tui/src/input/overlays.rs#L399-L582) |
| Quick Open (`Ctrl+P`) | `Up`/`Down`, `PageUp`/`PageDown`, `Enter`, `Esc`, `Ctrl+V` | Navigate up to `50,000` indexed paths, open selected file, or cancel | [overlays.rs](../crates/lume-tui/src/input/overlays.rs#L710-L771) |
| Prompts | `Tab` / `Up` / `Down` / `Enter` / `Esc` | Autocomplete file path or language, pick suggestion, submit, or cancel | [overlays.rs](../crates/lume-tui/src/input/overlays.rs#L128-L369) |
| Confirm close / quit | `S` / `D` / `C` or `Esc` | Save dirty tabs, discard changes and unlink journal, or cancel | [overlays.rs](../crates/lume-tui/src/input/overlays.rs#L51-L126) |
| Recovery prompt | `R` / `D` / `C` or `Esc` | Restore `.journal` text into tab, unlink journal, or defer without applying | [overlays.rs](../crates/lume-tui/src/input/overlays.rs#L20-L49) |
| Mouse | Single / double / triple left click in editor | Place cursor / select word / select line (`400 ms` click window) | [mouse.rs](../crates/lume-tui/src/input/mouse.rs#L151-L176) |
| Mouse | Left drag in editor | Extend selection while dragging | [mouse.rs](../crates/lume-tui/src/input/mouse.rs#L182-L215) |
| Mouse | Wheel up / down / left / right | Scroll hovered explorer (`1` item), split pane (`3` lines/cols), or editor | [mouse.rs](../crates/lume-tui/src/input/mouse.rs#L220-L298) |
| Mouse | Click tab header, `x`, or `[ + ]` | Switch tab, close tab (`request_close_tab`), or open untitled tab | [mouse.rs](../crates/lume-tui/src/input/mouse.rs#L304-L338) |
| Mouse | Click status bar | Open Language Mode selection prompt | [mouse.rs](../crates/lume-tui/src/input/mouse.rs#L80-L84) |
| Mouse | Click outside Quick Open box | Close Quick Open overlay | [mouse.rs](../crates/lume-tui/src/input/mouse.rs#L45-L49) |

## Command palette actions

`COMMANDS` in `crates/lume-tui/src/commands.rs:15-201` defines 37 entries shown in the Command Palette (`Ctrl+Shift+P` or `Alt+P`). Programmatic calls to `App::execute_command` also accept `:set modal`, `:set nomodal`, and `:set modal!` (`crates/lume-tui/src/commands.rs:511-530`).

| Command ID | Palette title | Listed shortcut | Target behavior |
| :--- | :--- | :--- | :--- |
| `file.quick_open` | `File: Quick Open File...` | `Ctrl+P` | Scan workspace and open Quick Open |
| `view.command_palette` | `View: Command Palette...` | `Ctrl+Shift+P / Alt+P` | Open Command Palette |
| `file.open` | `File: Open File...` | `Ctrl+O` | Open file prompt |
| `file.save` | `File: Save` | `Ctrl+S` | Synchronous atomic save or Save As prompt |
| `file.save_as` | `File: Save As...` | `Ctrl+Shift+S` | Open Save As prompt |
| `view.toggle_explorer` | `View: Toggle File Explorer` | `Ctrl+B / F2` | Show/hide explorer and shift focus |
| `explorer.new_file` | `Explorer: New File...` | `a in Explorer` | Prompt to create file inside target folder |
| `explorer.new_folder` | `Explorer: New Folder...` | `n in Explorer` | Prompt to create folder inside target folder |
| `explorer.rename` | `Explorer: Rename Selected Item...` | `F2 / m in Explorer` | Prompt to rename selected file or folder |
| `explorer.delete_folder` | `Explorer: Delete Selected Folder...` | `d in Explorer` | Confirm deletion of selected directory |
| `explorer.refresh` | `Explorer: Refresh Tree` | `r in Explorer` | Rescan directory tree |
| `language.set_mode` | `Language: Change Language Mode` | `Ctrl+K M` | Override syntax highlighting language |
| `file.new_tab` | `File: New Tab` | `Ctrl+T` | Create and switch to untitled tab |
| `file.close_tab` | `File: Close Tab` | `Ctrl+W` | Close active tab (confirms if dirty) |
| `file.next_tab` | `File: Next Tab` | `Ctrl+Tab / Alt+Right` | Cycle forward across tabs |
| `file.prev_tab` | `File: Previous Tab` | `Alt+Left` | Cycle backward across tabs |
| `edit.undo` | `Edit: Undo` | `Ctrl+Z` | Undo last history entry |
| `edit.redo` | `Edit: Redo` | `Ctrl+Y` | Redo last undone entry |
| `edit.cut` | `Edit: Cut` | `Ctrl+X` | Cut selection or current line with OSC 52 |
| `edit.copy` | `Edit: Copy` | `Ctrl+C` | Copy selection or current line with OSC 52 |
| `edit.paste` | `Edit: Paste` | `Ctrl+V` | Insert `Editor::clipboard` at cursor |
| `edit.find` | `Edit: Find in Buffer...` | `Ctrl+F` | Open Find footer bar |
| `edit.replace` | `Edit: Replace in Buffer...` | `Ctrl+H` | Open Find & Replace footer bar |
| `navigation.match_bracket` | `Navigation: Jump to Matching Bracket` | `Ctrl+M` | Jump between matching `()`, `[]`, `{}` |
| `navigation.goto_line` | `Navigation: Go to Line...` | `Ctrl+G` | Open `line[:col]` navigation prompt |
| `view.which_key` | `View: Which-Key Shortcuts` | `F1` | Show shortcut summary overlay |
| `view.split_vertical` | `View: Split Editor Vertically` | `Ctrl+K \` | Split viewport into left/right panes |
| `view.split_horizontal` | `View: Split Editor Horizontally` | `Ctrl+K -` | Split viewport into top/bottom panes |
| `view.focus_next_split` | `View: Focus Next Split Pane` | `Ctrl+K W` | Toggle focus between panes `0` and `1` |
| `view.close_split` | `View: Close Active Split Pane` | `Ctrl+K Q` | Collapse split to remaining pane |
| `view.toggle_line_numbers` | `View: Toggle Line Numbers` | `""` | Show or hide line-number/diff gutter |
| `settings.toggle_auto_pairs` | `Settings: Toggle Auto-Closing Pairs` | `""` | Toggle `() [] {} "" ''` auto-closing and overtype |
| `settings.toggle_smart_indent` | `Settings: Toggle Smart Indentation` | `""` | Toggle `Enter` indent after `{`, `(`, `[`, `:` |
| `settings.toggle_smart_unindent` | `Settings: Toggle Smart Unindent` | `""` | Toggle `Backspace` dedent by `tab_size` (`4`) |
| `settings.toggle_modal` | `Settings: Toggle Modal Mode (Vim Motions) [:set modal]` | `Ctrl+Alt+V` | Toggle Vim modal keybindings |
| `edit.select_all` | `Edit: Select All` | `Ctrl+A` | Select entire buffer |
| `app.quit` | `App: Quit` | `Ctrl+Q` | Exit editor (confirms if any tab is dirty) |

## Supported syntax languages

`lume-syntax` defines 16 `Language` variants (`crates/lume-syntax/src/language.rs:4-137`). Six languages use dedicated Tree-sitter grammars, TypeScript reuses the JavaScript grammar, and eight use custom line/byte lexers (`crates/lume-syntax/src/highlighter.rs:152-170`).

| Language | Auto-detected extensions and filenames | `Language::from_id` aliases | Highlighter backend |
| :--- | :--- | :--- | :--- |
| C | `.c`, `.h` | `c` | `tree-sitter-c` |
| C++ | `.cpp`, `.hpp`, `.cc`, `.cxx`, `.hh`, `.hxx`, `.cu`, `.cuh` | `cpp`, `c++`, `cuda`, `cu` | `tree-sitter-cpp` |
| Rust | `.rs` | `rust`, `rs` | `tree-sitter-rust` |
| Python | `.py`, `.pyw` | `python`, `py` | `tree-sitter-python` |
| Java | `.java` | `java` | `tree-sitter-java` |
| JavaScript | `.js`, `.jsx`, `.mjs`, `.cjs` | `javascript`, `js`, `mjs`, `cjs` | `tree-sitter-javascript` |
| TypeScript | `.ts`, `.tsx` | `typescript`, `ts` | `tree-sitter-javascript` (shared JS grammar) |
| Assembly | `.s`, `.asm` | `assembly`, `asm`, `s` | Custom lexer (`lex_assembly`) |
| Go | `.go` | `go` | Custom lexer (`lex_go`) |
| JSON | `.json` | `json` | Custom lexer (`lex_json`) |
| TOML | `.toml`, `Cargo.toml` | `toml` | Custom lexer (`lex_toml`) |
| YAML | `.yaml`, `.yml` | `yaml`, `yml` | Custom lexer (`lex_yaml`) |
| Markdown | `.md`, `.markdown` | `markdown`, `md` | Custom lexer (`lex_markdown`) |
| Shell | `.sh`, `.bash`, `.zsh`, `Makefile`, `GNUmakefile`, `Dockerfile` | `shell`, `bash`, `sh`, `zsh` | Custom lexer (`lex_shell`) |
| HTML | `.html`, `.htm` | `html`, `htm` | Custom lexer (`lex_html_css`, `<...>` tags) |
| CSS | `.css` | `css` | Custom lexer (`lex_html_css`, `<...>` tags only) |

Typing `plain text` in the `Set Language` prompt clears `language_override` (`crates/lume-tui/src/input/overlays.rs:190-193`), which returns highlighting to path-based detection for named files or no highlighting for untitled tabs.

## Environment variables and Cargo features

Runtime behavior reads five environment variables (`Journal::journal_dir` in `crates/lume-core/src/journal.rs:154-163` and `osc52_sequence_auto` in `crates/lume-core/src/clipboard.rs:66-72`). Two extra variables configure the external Helix PTY benchmark wrapper (`scripts/hx-bench.c:27-35`).

| Variable | Component | Default when unset | Effect |
| :--- | :--- | :--- | :--- |
| `LUME_JOURNAL_DIR` | `lume-core` | Falls back to `$XDG_CACHE_HOME/lume/journal` | Overrides directory used for `.journal` crash files |
| `XDG_CACHE_HOME` | `lume-core` | Falls back to `$HOME/.cache/lume/journal` | Base cache directory for `lume/journal` |
| `HOME` | `lume-core` | Falls back to `$TMPDIR/lume_journal_<pid>` | Parent of `.cache/lume/journal` |
| `TMUX` | `lume-core` | Unset | Enables tmux DCS passthrough for OSC 52 sequences |
| `TERM` | `lume-core` | Unset | Enables tmux/screen DCS passthrough when starting with `screen` or `tmux` |
| `HELIX_BIN` | `scripts/hx-bench.c` | `"hx"` | Executable launched by benchmark PTY wrapper |
| `HELIX_BENCH_CONFIG` | `scripts/hx-bench.c` | `"/tmp/lume-bench-helix-config.toml"` | Config file path passed as `-c` to Helix wrapper |

The feature flags `m1`, `m2`, `m3`, `m4`, `m5`, and `e2e` in `crates/lume-core/Cargo.toml:23-30` and `m2`, `e2e` in `crates/lume-syntax/Cargo.toml:19-22` only enable gated integration test modules under `crates/lume-core/tests/` and `crates/lume-syntax/tests/`. Production library and binary builds compile identical code with or without those Cargo features.
