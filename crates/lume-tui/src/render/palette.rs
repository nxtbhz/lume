use std::collections::HashSet;
use std::io::Write;

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

use crate::app::App;
use crate::config::{format_key_sequence, KeymapContext};
use crate::render::text::{display_width, fit_text};
use crate::vim::VimMode;

pub fn render_command_palette<W: Write>(app: &App, writer: &mut W) -> std::io::Result<()> {
    let box_w = (app.terminal_width as usize).min(68);
    let box_h = 11.min(app.terminal_height as usize);
    if box_w == 0 || box_h == 0 {
        return Ok(());
    }
    let start_x = (app.terminal_width as usize).saturating_sub(box_w) / 2;
    let start_y = (app.terminal_height as usize).saturating_sub(box_h) / 3;

    let commands = app.filtered_commands();
    let visible_rows = box_h.saturating_sub(3);
    let max_selected = commands.len().saturating_sub(1);
    let selected = app.palette_selected.min(max_selected);
    let scroll_offset = selected.saturating_sub(visible_rows.saturating_sub(1));

    // Background box
    for y in 0..box_h {
        queue!(
            writer,
            MoveTo(start_x as u16, (start_y + y) as u16),
            SetBackgroundColor(Color::Rgb {
                r: 20,
                g: 25,
                b: 35
            }),
            SetForegroundColor(Color::White)
        )?;

        if y == 0 {
            let title = " Command Palette (Esc to close) ";
            queue!(
                writer,
                SetForegroundColor(Color::Cyan),
                Print(fit_text(title, box_w))
            )?;
        } else if y == 1 {
            let prompt = format!(" > {}█", app.palette_query);
            queue!(
                writer,
                SetForegroundColor(Color::White),
                Print(fit_text(&prompt, box_w))
            )?;
        } else if y == 2 {
            queue!(
                writer,
                SetForegroundColor(Color::DarkGrey),
                Print("─".repeat(box_w))
            )?;
        } else {
            let item_idx = scroll_offset + (y - 3);
            if let Some(item) = commands.get(item_idx) {
                let is_sel = item_idx == selected;
                let prefix = if is_sel { " > " } else { "   " };
                let title = item.title;
                let shortcut = app.shortcut_label_for_command(item.id);

                if is_sel {
                    queue!(
                        writer,
                        SetBackgroundColor(Color::DarkBlue),
                        SetForegroundColor(Color::Yellow)
                    )?;
                } else {
                    queue!(
                        writer,
                        SetBackgroundColor(Color::Rgb {
                            r: 20,
                            g: 25,
                            b: 35
                        }),
                        SetForegroundColor(Color::White)
                    )?;
                }

                let label = format!("{prefix}{title}");
                let shortcut_width = display_width(&shortcut) + 2;
                if shortcut_width + prefix.len() < box_w && !shortcut.is_empty() {
                    queue!(
                        writer,
                        Print(fit_text(&label, box_w - shortcut_width)),
                        Print(" "),
                        Print(&shortcut),
                        Print(" ")
                    )?;
                } else {
                    queue!(writer, Print(fit_text(&label, box_w)))?;
                }
            } else {
                queue!(writer, Print(" ".repeat(box_w)))?;
            }
        }
    }

    queue!(writer, ResetColor)?;
    Ok(())
}

/// Builds the dynamic Which-Key overlay lines reflecting the active preset and overrides.
pub fn build_which_key_lines(app: &App) -> Vec<String> {
    let mut lines = vec![format!(
        " LUME KEYMAPS [preset: {}] (Press any key or Esc to close) ",
        app.config.preset.as_str()
    )];

    let max_line_width = 78;

    // 1. Global bindings
    let global_items = collect_context_items(app, KeymapContext::Global);
    push_wrapped_section(&mut lines, "[Global]", &global_items, max_line_width);

    // 2. Active editing mode bindings
    let (mode_ctx, mode_tag) = if app.modal_enabled {
        match app.vim_state.mode {
            VimMode::Normal => (KeymapContext::Normal, "[Normal]"),
            VimMode::Visual => (KeymapContext::Visual, "[Visual]"),
            VimMode::Insert => (KeymapContext::Insert, "[Insert]"),
        }
    } else {
        (KeymapContext::Editor, "[Editor]")
    };
    let mode_items = collect_context_items(app, mode_ctx);
    push_wrapped_section(&mut lines, mode_tag, &mode_items, max_line_width);

    // 3. Explorer bindings
    let explorer_items = collect_context_items(app, KeymapContext::Explorer);
    push_wrapped_section(&mut lines, "[Explorer]", &explorer_items, max_line_width);

    lines
}

fn collect_context_items(app: &App, ctx: KeymapContext) -> Vec<String> {
    let mut grouped: Vec<(&str, Vec<String>)> = Vec::new();
    let mut seen_lower: HashSet<(&str, String)> = HashSet::new();

    for (seq, action) in app.config.keymaps.entries_for(ctx) {
        // Keep Editor/Insert overview concise by hiding repetitive Shift+arrow selection motions
        // unless custom-bound; standard motions are still covered by non-select entries.
        if matches!(ctx, KeymapContext::Editor | KeymapContext::Insert)
            && action.starts_with("motion.select_")
        {
            continue;
        }

        let formatted = format_key_sequence(seq);
        let dedup_key = formatted.replace("Shift+", "").to_ascii_lowercase();
        if !seen_lower.insert((action, dedup_key)) {
            continue;
        }

        if let Some((_, keys)) = grouped.iter_mut().find(|(a, _)| *a == action) {
            if !keys.contains(&formatted) {
                keys.push(formatted);
            }
        } else {
            grouped.push((action, vec![formatted]));
        }
    }

    grouped
        .into_iter()
        .map(|(action, keys)| format!("{}: {}", keys.join(" / "), short_action_label(action)))
        .collect()
}

fn push_wrapped_section(lines: &mut Vec<String>, tag: &str, items: &[String], max_width: usize) {
    if items.is_empty() {
        return;
    }
    let prefix = format!(" {tag:<10} ");
    let continuation = " ".repeat(prefix.len());
    let mut current = prefix.clone();
    let mut has_item = false;

    for item in items {
        let sep = if has_item { " │ " } else { "" };
        if has_item
            && display_width(&current) + display_width(sep) + display_width(item) > max_width
        {
            lines.push(current);
            current = format!("{continuation}{item}");
        } else {
            current.push_str(sep);
            current.push_str(item);
            has_item = true;
        }
    }
    if has_item {
        lines.push(current);
    }
}

fn short_action_label(action: &str) -> &str {
    match action {
        "view.command_palette" => "Palette",
        "file.quick_open" => "Quick Open",
        "file.open" => "Open",
        "file.save" => "Save",
        "file.save_as" => "Save As",
        "file.new_tab" => "New Tab",
        "file.close_tab" => "Close Tab",
        "file.next_tab" => "Next Tab",
        "file.prev_tab" => "Prev Tab",
        "view.toggle_explorer" => "Explorer",
        "view.toggle_word_wrap" => "Word Wrap",
        "view.split_vertical" => "V-Split",
        "view.split_horizontal" => "H-Split",
        "view.focus_next_split" => "Switch Split",
        "view.close_split" => "Close Split",
        "language.set_mode" => "Language",
        "edit.undo" => "Undo",
        "edit.redo" => "Redo",
        "edit.cut" => "Cut",
        "edit.copy" => "Copy",
        "edit.paste" => "Paste",
        "edit.select_all" => "Select All",
        "edit.find" => "Find",
        "edit.replace" => "Replace",
        "search.find_next" => "Next Match",
        "search.find_prev" => "Prev Match",
        "navigation.match_bracket" => "Bracket",
        "navigation.goto_line" => "Go to Line",
        "navigation.goto_definition" => "Go to Def",
        "settings.toggle_modal" => "Modal",
        "view.which_key" => "Help",
        "config.open" => "Open Config",
        "config.reload" => "Reload Config",
        "app.quit" => "Quit",
        "explorer.new_file" => "New File",
        "explorer.new_folder" => "New Folder",
        "explorer.rename" => "Rename",
        "explorer.delete_folder" => "Delete",
        "explorer.refresh" => "Refresh",
        "explorer.activate" => "Open/Toggle",
        "explorer.focus_editor" => "Editor",
        "explorer.move_up" => "Up",
        "explorer.move_down" => "Down",
        "explorer.page_up" => "PageUp",
        "explorer.page_down" => "PageDown",
        "explorer.collapse" => "Collapse",
        "explorer.expand" => "Expand",
        "vim.normal_mode" => "Normal",
        "vim.insert_mode" => "Insert",
        "vim.append_mode" => "Append",
        "vim.open_line_below" => "Line Below",
        "vim.open_line_above" => "Line Above",
        "vim.visual_mode" => "Visual",
        "vim.delete_char" => "Del Char",
        "vim.delete_line" => "Del Line",
        "vim.yank_line" => "Yank Line",
        "vim.paste" => "Put",
        "vim.visual_delete" => "Delete",
        "vim.visual_yank" => "Yank",
        other => other.split('.').next_back().unwrap_or(other),
    }
}

pub fn render_which_key<W: Write>(app: &App, writer: &mut W) -> std::io::Result<()> {
    let lines = build_which_key_lines(app);
    let box_w = (app.terminal_width as usize).clamp(44, 80);
    let box_h = lines.len().min(app.terminal_height as usize);
    if box_w == 0 || box_h == 0 {
        return Ok(());
    }
    let start_x = (app.terminal_width as usize).saturating_sub(box_w) / 2;
    let start_y = (app.terminal_height as usize).saturating_sub(box_h) / 2;

    for (i, line) in lines.iter().take(box_h).enumerate() {
        queue!(
            writer,
            MoveTo(start_x as u16, (start_y + i) as u16),
            SetBackgroundColor(Color::Rgb {
                r: 25,
                g: 30,
                b: 45
            }),
            SetForegroundColor(if i == 0 { Color::Yellow } else { Color::White })
        )?;
        queue!(writer, Print(fit_text(line, box_w)))?;
    }

    queue!(writer, ResetColor)?;
    Ok(())
}
