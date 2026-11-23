//! Zero-dependency TOML subset parser for Lume's `config.toml`.

use std::collections::{BTreeMap, HashSet};

use crate::config::ConfigError;

/// Scalar value supported by Lume's configuration schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TomlValue {
    Bool(bool),
    Integer(u64),
    Str(String),
}

/// A parsed TOML key-value entry together with its 1-based source line number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TomlEntry {
    pub value: TomlValue,
    pub line: usize,
}

/// Key-value entries inside a single TOML table.
pub type TomlTable = BTreeMap<String, TomlEntry>;

/// Parsed TOML document mapping table names (`""` for root) to their entries.
pub type TomlDocument = BTreeMap<String, TomlTable>;

/// Parses a TOML configuration document into tables and scalar entries with line numbers.
///
/// Supports `[section]` / `[section.sub]` headers, bare and quoted keys (`"..."` / `'...'`),
/// booleans, unsigned integers, basic/literal strings, and `#` comments.
pub(crate) fn parse_toml_document(input: &str) -> Result<TomlDocument, ConfigError> {
    let mut doc: TomlDocument = BTreeMap::new();
    let mut seen_tables: HashSet<String> = HashSet::new();
    let mut current_table = String::new();

    for (idx, raw_line) in input.lines().enumerate() {
        let line_no = idx + 1;
        let line = if idx == 0 {
            raw_line.strip_prefix('\u{feff}').unwrap_or(raw_line)
        } else {
            raw_line
        };
        let trimmed = trim_horizontal_ws(line);
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix('[') {
            let table_name = parse_table_header(rest, line_no)?;
            if !seen_tables.insert(table_name.clone()) {
                return Err(ConfigError::Parse {
                    line: line_no,
                    message: format!("duplicate table header `[{table_name}]`"),
                });
            }
            doc.entry(table_name.clone()).or_default();
            current_table = table_name;
            continue;
        }

        let (key, rest) = parse_key(trimmed, line_no)?;
        let rest = trim_horizontal_ws(rest);
        let Some(after_eq) = rest.strip_prefix('=') else {
            return Err(ConfigError::Parse {
                line: line_no,
                message: format!("expected `=` after key `{key}`"),
            });
        };
        let (value, trailing) = parse_value(trim_horizontal_ws(after_eq), line_no)?;
        ensure_only_comment_or_empty(trailing, line_no)?;

        let table = doc.entry(current_table.clone()).or_default();
        if table.contains_key(&key) {
            return Err(ConfigError::Parse {
                line: line_no,
                message: format!("duplicate key `{key}`"),
            });
        }
        table.insert(
            key,
            TomlEntry {
                value,
                line: line_no,
            },
        );
    }

    Ok(doc)
}

fn trim_horizontal_ws(s: &str) -> &str {
    s.trim_matches([' ', '\t', '\r'])
}

fn ensure_only_comment_or_empty(rest: &str, line: usize) -> Result<(), ConfigError> {
    let trimmed = trim_horizontal_ws(rest);
    if trimmed.is_empty() || trimmed.starts_with('#') {
        Ok(())
    } else {
        Err(ConfigError::Parse {
            line,
            message: format!("unexpected trailing content `{trimmed}`"),
        })
    }
}

pub(super) fn parse_table_header(
    after_open_bracket: &str,
    line: usize,
) -> Result<String, ConfigError> {
    if after_open_bracket.starts_with('[') {
        return Err(ConfigError::Parse {
            line,
            message: "array-of-tables headers `[[...]]` are not supported".to_string(),
        });
    }
    let Some(close_idx) = after_open_bracket.find(']') else {
        return Err(ConfigError::Parse {
            line,
            message: "unterminated table header, expected `]`".to_string(),
        });
    };
    let name = trim_horizontal_ws(&after_open_bracket[..close_idx]);
    let trailing = &after_open_bracket[close_idx + 1..];
    ensure_only_comment_or_empty(trailing, line)?;

    if name.is_empty()
        || name.starts_with('.')
        || name.ends_with('.')
        || name.contains("..")
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    {
        return Err(ConfigError::Parse {
            line,
            message: format!("invalid table name `{name}`"),
        });
    }
    Ok(name.to_string())
}

fn parse_key(input: &str, line: usize) -> Result<(String, &str), ConfigError> {
    if let Some(rest) = input.strip_prefix('"') {
        return parse_basic_string(rest, line);
    }
    if let Some(rest) = input.strip_prefix('\'') {
        return parse_literal_string(rest, line);
    }

    let end = input
        .bytes()
        .position(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')))
        .unwrap_or(input.len());
    if end == 0 {
        return Err(ConfigError::Parse {
            line,
            message: "expected a key name".to_string(),
        });
    }
    Ok((input[..end].to_string(), &input[end..]))
}

fn parse_value(input: &str, line: usize) -> Result<(TomlValue, &str), ConfigError> {
    if input.is_empty() || input.starts_with('#') {
        return Err(ConfigError::Parse {
            line,
            message: "missing value after `=`".to_string(),
        });
    }

    if let Some(rest) = input.strip_prefix('"') {
        let (s, trailing) = parse_basic_string(rest, line)?;
        return Ok((TomlValue::Str(s), trailing));
    }
    if let Some(rest) = input.strip_prefix('\'') {
        let (s, trailing) = parse_literal_string(rest, line)?;
        return Ok((TomlValue::Str(s), trailing));
    }

    let end = input.find([' ', '\t', '\r', '#']).unwrap_or(input.len());
    let token = &input[..end];
    let trailing = &input[end..];

    if token == "true" {
        return Ok((TomlValue::Bool(true), trailing));
    }
    if token == "false" {
        return Ok((TomlValue::Bool(false), trailing));
    }
    if token.bytes().all(|b| b.is_ascii_digit()) {
        let number = token.parse::<u64>().map_err(|_| ConfigError::Parse {
            line,
            message: format!("integer `{token}` out of range"),
        })?;
        return Ok((TomlValue::Integer(number), trailing));
    }

    Err(ConfigError::Parse {
        line,
        message: format!("invalid value `{token}`"),
    })
}

fn parse_basic_string(after_open_quote: &str, line: usize) -> Result<(String, &str), ConfigError> {
    let mut out = String::new();
    let mut chars = after_open_quote.char_indices();

    while let Some((idx, ch)) = chars.next() {
        match ch {
            '"' => return Ok((out, &after_open_quote[idx + 1..])),
            '\\' => {
                let Some((_, escaped)) = chars.next() else {
                    return Err(ConfigError::Parse {
                        line,
                        message: "unterminated escape sequence in string".to_string(),
                    });
                };
                match escaped {
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    other => {
                        return Err(ConfigError::Parse {
                            line,
                            message: format!("unsupported escape sequence `\\{other}`"),
                        });
                    }
                }
            }
            c => out.push(c),
        }
    }

    Err(ConfigError::Parse {
        line,
        message: "unterminated string literal".to_string(),
    })
}

fn parse_literal_string(
    after_open_quote: &str,
    line: usize,
) -> Result<(String, &str), ConfigError> {
    let Some(close_idx) = after_open_quote.find('\'') else {
        return Err(ConfigError::Parse {
            line,
            message: "unterminated literal string".to_string(),
        });
    };
    Ok((
        after_open_quote[..close_idx].to_string(),
        &after_open_quote[close_idx + 1..],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_toml_reads_tables_scalars_comments_and_escapes() {
        let src = r#"
# Top comment
[editor]
tab_size = 2
modal = true # inline comment

[keymaps.global]
"Ctrl+K \\" = "view.split_vertical"
'Ctrl+W' = 'none'
"#;
        let doc = parse_toml_document(src).unwrap();
        assert_eq!(doc["editor"]["tab_size"].value, TomlValue::Integer(2));
        assert_eq!(doc["editor"]["modal"].value, TomlValue::Bool(true));
        assert_eq!(
            doc["keymaps.global"]["Ctrl+K \\"].value,
            TomlValue::Str("view.split_vertical".to_string())
        );
        assert_eq!(
            doc["keymaps.global"]["Ctrl+W"].value,
            TomlValue::Str("none".to_string())
        );
    }

    #[test]
    fn parse_toml_rejects_duplicate_keys_with_line_number() {
        let src = "[editor]\ntab_size = 2\ntab_size = 4\n";
        let err = parse_toml_document(src).unwrap_err();
        assert!(matches!(err, ConfigError::Parse { line: 3, .. }));
    }

    #[test]
    fn parse_toml_rejects_unterminated_string_and_bad_header() {
        assert!(matches!(
            parse_toml_document("[editor\n"),
            Err(ConfigError::Parse { line: 1, .. })
        ));
        assert!(matches!(
            parse_toml_document("[editor]\na = \"unclosed\n"),
            Err(ConfigError::Parse { line: 2, .. })
        ));
    }
}
