use std::io::Write;
use std::path::Path;

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

use crate::explorer::FileExplorer;
use crate::render::text::fit_text;

impl FileExplorer {
    pub fn render<W: Write>(
        &self,
        writer: &mut W,
        top_row: u16,
        height: usize,
        width: usize,
        is_focused: bool,
        shortcut: Option<&str>,
    ) -> std::io::Result<()> {
        if !self.is_visible || height == 0 || width == 0 {
            return Ok(());
        }

        let content_w = width - 1; // last column is the vertical border

        // Header (Row top_row)
        queue!(
            writer,
            MoveTo(0, top_row),
            SetBackgroundColor(Color::Rgb {
                r: 25,
                g: 30,
                b: 40
            }),
            SetForegroundColor(if is_focused {
                Color::Yellow
            } else {
                Color::White
            })
        )?;

        let root_name = self
            .root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| self.root.to_str().unwrap_or("WORKSPACE"));
        let header = if let Some(key) = shortcut {
            format!(" 📁 {root_name} [{key}]")
        } else {
            format!(" 📁 {root_name}")
        };
        queue!(
            writer,
            Print(fit_text(&header, content_w)),
            SetForegroundColor(Color::DarkGrey),
            Print("│"),
            ResetColor
        )?;

        // Rows
        for i in 0..height.saturating_sub(1) {
            let item_idx = self.scroll_offset + i;
            let screen_row = top_row + 1 + i as u16;
            queue!(writer, MoveTo(0, screen_row))?;

            if item_idx < self.items.len() {
                let item = &self.items[item_idx];
                let is_sel = item_idx == self.selected_idx;

                if is_sel {
                    let bg = if is_focused {
                        Color::Rgb {
                            r: 35,
                            g: 55,
                            b: 90,
                        }
                    } else {
                        Color::Rgb {
                            r: 30,
                            g: 35,
                            b: 45,
                        }
                    };
                    queue!(writer, SetBackgroundColor(bg))?;
                } else {
                    queue!(
                        writer,
                        SetBackgroundColor(Color::Rgb {
                            r: 18,
                            g: 20,
                            b: 26
                        })
                    )?;
                }

                let indent = "  ".repeat(item.depth.min(6));
                let icon = if item.is_dir {
                    if item.is_expanded {
                        "▼ "
                    } else {
                        "▶ "
                    }
                } else {
                    get_file_icon(&item.name)
                };

                let line_text = format!("{indent}{icon}{}", item.name);

                let fg = if is_sel {
                    if is_focused {
                        Color::Yellow
                    } else {
                        Color::White
                    }
                } else if item.is_dir {
                    Color::Cyan
                } else {
                    get_file_color(&item.name)
                };

                queue!(
                    writer,
                    SetForegroundColor(fg),
                    Print(fit_text(&line_text, content_w)),
                    SetBackgroundColor(Color::Reset),
                    SetForegroundColor(Color::DarkGrey),
                    Print("│"),
                    ResetColor
                )?;
            } else {
                queue!(
                    writer,
                    SetBackgroundColor(Color::Rgb {
                        r: 18,
                        g: 20,
                        b: 26
                    }),
                    Print(" ".repeat(content_w)),
                    SetBackgroundColor(Color::Reset),
                    SetForegroundColor(Color::DarkGrey),
                    Print("│"),
                    ResetColor
                )?;
            }
        }

        Ok(())
    }
}

pub(crate) fn get_file_icon(name: &str) -> &'static str {
    if let Some(ext) = Path::new(name).extension().and_then(|s| s.to_str()) {
        match ext.to_ascii_lowercase().as_str() {
            "rs" => "🦀 ",
            "c" | "h" => "🅒 ",
            "cpp" | "hpp" | "cc" | "cu" => "➕ ",
            "py" => "🐍 ",
            "java" => "☕ ",
            "js" | "ts" | "jsx" | "tsx" => "🅙 ",
            "s" | "asm" => "⚙ ",
            "toml" | "json" | "yaml" | "yml" => "📋 ",
            "md" => "📝 ",
            _ => "  ",
        }
    } else {
        "  "
    }
}

pub(crate) fn get_file_color(name: &str) -> Color {
    if let Some(ext) = Path::new(name).extension().and_then(|s| s.to_str()) {
        match ext.to_ascii_lowercase().as_str() {
            "rs" => Color::Rgb {
                r: 230,
                g: 120,
                b: 60,
            },
            "c" | "h" | "cpp" | "hpp" | "cc" | "cu" => Color::Rgb {
                r: 90,
                g: 160,
                b: 240,
            },
            "py" => Color::Rgb {
                r: 240,
                g: 210,
                b: 80,
            },
            "java" => Color::Rgb {
                r: 230,
                g: 90,
                b: 80,
            },
            "js" | "ts" => Color::Rgb {
                r: 240,
                g: 220,
                b: 70,
            },
            "s" | "asm" => Color::Rgb {
                r: 180,
                g: 140,
                b: 220,
            },
            "toml" | "json" | "yaml" | "yml" => Color::Rgb {
                r: 100,
                g: 210,
                b: 180,
            },
            "md" => Color::Rgb {
                r: 140,
                g: 200,
                b: 240,
            },
            _ => Color::White,
        }
    } else {
        Color::White
    }
}
