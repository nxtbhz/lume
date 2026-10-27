use std::io::Write;
use std::path::Path;

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

use crate::explorer::FileExplorer;

impl FileExplorer {
    pub fn render<W: Write>(
        &self,
        writer: &mut W,
        top_row: u16,
        height: usize,
        is_focused: bool,
    ) -> std::io::Result<()> {
        if !self.is_visible || height == 0 {
            return Ok(());
        }

        let width = self.width;
        let content_w = width.saturating_sub(1); // last column is the vertical border

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
        let header_raw = format!(" 📁 {} [F2]", root_name);
        let header_text = truncate_to_width(&header_raw, content_w);
        let fill = content_w.saturating_sub(display_width(&header_text));
        queue!(
            writer,
            Print(format!("{}{:fill$}", header_text, "", fill = fill)),
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

                let prefix = format!("{}{}", indent, icon);
                let avail_name = content_w.saturating_sub(display_width(&prefix));
                let name_truncated = truncate_to_width(&item.name, avail_name);
                let line_text = format!("{}{}", prefix, name_truncated);
                let line_fill = content_w.saturating_sub(display_width(&line_text));

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
                    Print(format!("{}{:fill$}", line_text, "", fill = line_fill)),
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

pub(crate) fn char_display_width(c: char) -> usize {
    if c < ' '
        || ('\u{007f}'..='\u{009f}').contains(&c)
        || ('\u{0300}'..='\u{036f}').contains(&c)
        || ('\u{fe00}'..='\u{fe0f}').contains(&c)
    {
        0
    } else if ('\u{1100}'..='\u{115f}').contains(&c)
        || ('\u{2e80}'..='\u{303e}').contains(&c)
        || ('\u{3040}'..='\u{a4cf}').contains(&c)
        || ('\u{ac00}'..='\u{d7a3}').contains(&c)
        || ('\u{f900}'..='\u{faff}').contains(&c)
        || ('\u{fe10}'..='\u{fe6f}').contains(&c)
        || ('\u{ff00}'..='\u{ff60}').contains(&c)
        || ('\u{ffe0}'..='\u{ffe6}').contains(&c)
        || c == '\u{2615}'
        || c == '\u{2795}'
        || ('\u{1f300}'..='\u{1f9ff}').contains(&c)
        || ('\u{1fa00}'..='\u{1faff}').contains(&c)
        || ('\u{20000}'..='\u{2fffd}').contains(&c)
        || ('\u{30000}'..='\u{3fffd}').contains(&c)
    {
        2
    } else {
        1
    }
}

pub(crate) fn display_width(s: &str) -> usize {
    s.chars().map(char_display_width).sum()
}

pub(crate) fn truncate_to_width(s: &str, max_width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = char_display_width(c);
        if used + w > max_width {
            break;
        }
        out.push(c);
        used += w;
    }
    out
}
