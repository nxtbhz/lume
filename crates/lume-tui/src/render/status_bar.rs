use std::io::Write;
use std::time::Duration;

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

use crate::app::{ActiveFocus, App};
use crate::render::text::{display_width, fit_text};
use crate::vim::VimMode;

pub fn render_status_bar<W: Write>(
    app: &App,
    writer: &mut W,
    active_tab_idx: usize,
) -> std::io::Result<()> {
    let (cursor_row, cursor_col) = app.editor.cursor_line_col();
    let status_row = app.terminal_height.saturating_sub(1);
    if let Some(ref search) = app.search_state {
        search.render(writer, app.terminal_width, status_row)?;
    } else {
        queue!(
            writer,
            MoveTo(0, status_row),
            SetBackgroundColor(Color::DarkBlue),
            SetForegroundColor(Color::White)
        )?;

        let file_name = app.editor.active_tab().name();
        let dirty_flag = if app.editor.is_dirty() { " [*]" } else { "" };
        let lang_str = app
            .current_language()
            .map(|l| l.name())
            .unwrap_or("Plain Text");
        let eol_str = app.editor.active_tab().line_ending.label();

        let exp_indicator = if app.explorer.is_visible {
            "[Explorer ON] "
        } else {
            "[Explorer OFF] "
        };

        let right_status = if let Some((ref msg, time)) = app.status_message {
            if time.elapsed() < Duration::from_secs(4) {
                format!("{} ", msg)
            } else {
                "F2: Explorer │ Ctrl+O: Open │ Ctrl+S: Save │ Ctrl+P: Quick Open │ F1: Help "
                    .to_string()
            }
        } else {
            "F2: Explorer │ Ctrl+O: Open │ Ctrl+S: Save │ Ctrl+P: Quick Open │ F1: Help "
                .to_string()
        };

        let width = app.terminal_width as usize;

        let git_str = if let Some(ref head) = app.git_head {
            format!("⎇ {} │ ", head.display_name())
        } else {
            String::new()
        };
        let core_info = format!(
            "{}{}{} │ Tab {}/{} │ {}{} │ Ln {}, Col {} │ {} │ {}",
            git_str,
            exp_indicator,
            if app.focus == ActiveFocus::Explorer {
                "(Explorer Active)"
            } else {
                "(Editor)"
            },
            active_tab_idx + 1,
            app.editor.tabs().len(),
            file_name,
            dirty_flag,
            cursor_row + 1,
            cursor_col + 1,
            lang_str,
            eol_str
        );

        if app.modal_enabled {
            let (badge_fg, badge_bg) = match app.vim_state.mode {
                VimMode::Normal => (
                    Color::White,
                    Color::Rgb {
                        r: 30,
                        g: 100,
                        b: 220,
                    },
                ),
                VimMode::Insert => (
                    Color::Black,
                    Color::Rgb {
                        r: 50,
                        g: 205,
                        b: 50,
                    },
                ),
                VimMode::Visual => (
                    Color::Black,
                    Color::Rgb {
                        r: 255,
                        g: 140,
                        b: 0,
                    },
                ),
            };
            let badge_str = format!("[{}]", app.vim_state.mode.name());
            let prefix = " LUME │ ";
            let suffix_badge = " │";
            let rest_left = format!(" {}", core_info);
            let badge_prefix_len =
                prefix.chars().count() + badge_str.chars().count() + suffix_badge.chars().count();
            let left_len = badge_prefix_len + display_width(&rest_left);
            let spaces = width.saturating_sub(left_len + display_width(&right_status));
            let tail = format!(
                "{}{:spaces$}{}",
                rest_left,
                "",
                right_status,
                spaces = spaces
            );
            let tail_cap = width.saturating_sub(badge_prefix_len);
            if width < badge_prefix_len {
                let full_badge = format!("{}{}{}", prefix, badge_str, suffix_badge);
                queue!(writer, Print(fit_text(&full_badge, width)), ResetColor)?;
            } else {
                queue!(
                    writer,
                    Print(prefix),
                    SetBackgroundColor(badge_bg),
                    SetForegroundColor(badge_fg),
                    Print(&badge_str),
                    SetBackgroundColor(Color::DarkBlue),
                    SetForegroundColor(Color::White),
                    Print(suffix_badge),
                    Print(fit_text(&tail, tail_cap)),
                    ResetColor
                )?;
            }
        } else {
            let left_status = format!(" LUME │ {}", core_info);

            let spaces =
                width.saturating_sub(display_width(&left_status) + display_width(&right_status));
            let bar = format!(
                "{}{:width$}{}",
                left_status,
                "",
                right_status,
                width = spaces
            );
            queue!(writer, Print(fit_text(&bar, width)), ResetColor)?;
        }
    }
    Ok(())
}
