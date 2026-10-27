use crate::language::{is_assembly_mnemonic, is_assembly_register};
use crate::token::{HighlightToken, TokenType};

pub(crate) fn lex_assembly(
    text: &str,
    start_byte: usize,
    end_byte: usize,
    tokens: &mut Vec<HighlightToken>,
) {
    let sub = &text[..end_byte.min(text.len())];
    let mut offset = 0;
    for raw_line in sub.split_inclusive('\n') {
        let line_start = offset;
        offset += raw_line.len();
        let line = raw_line.trim_end_matches(['\r', '\n']);
        let line_end = line_start + line.len();

        if line_end <= start_byte {
            continue;
        }

        // Check comment
        let comment_idx = line
            .find(';')
            .or_else(|| line.find("//"))
            .or_else(|| line.find('#'));
        let code_part = if let Some(idx) = comment_idx {
            &line[..idx]
        } else {
            line
        };

        // Words
        for (w_start, word) in find_words(code_part) {
            let abs_start = line_start + w_start;
            let abs_end = abs_start + word.len();

            if word.starts_with('.') {
                tokens.push(HighlightToken {
                    start_byte: abs_start,
                    end_byte: abs_end,
                    token_type: TokenType::Macro,
                });
            } else if word.starts_with('%') || is_assembly_register(word) {
                tokens.push(HighlightToken {
                    start_byte: abs_start,
                    end_byte: abs_end,
                    token_type: TokenType::Type,
                });
            } else if is_assembly_mnemonic(word) {
                tokens.push(HighlightToken {
                    start_byte: abs_start,
                    end_byte: abs_end,
                    token_type: TokenType::Keyword,
                });
            } else if word.starts_with('$')
                || word
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == 'x' || c == 'X')
            {
                tokens.push(HighlightToken {
                    start_byte: abs_start,
                    end_byte: abs_end,
                    token_type: TokenType::Number,
                });
            } else if word.ends_with(':') {
                tokens.push(HighlightToken {
                    start_byte: abs_start,
                    end_byte: abs_end,
                    token_type: TokenType::Function,
                });
            }
        }

        if let Some(idx) = comment_idx {
            tokens.push(HighlightToken {
                start_byte: line_start + idx,
                end_byte: line_end,
                token_type: TokenType::Comment,
            });
        }
    }
}

pub(crate) fn lex_json(
    text: &str,
    start_byte: usize,
    end_byte: usize,
    tokens: &mut Vec<HighlightToken>,
) {
    let sub = &text[..end_byte.min(text.len())];
    let bytes = sub.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            let str_start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            if i < bytes.len() {
                i += 1; // closing quote
            }
            let str_end = i;

            if str_end > start_byte {
                // Check if key (followed by ':')
                let mut after = i;
                while after < bytes.len() && bytes[after].is_ascii_whitespace() {
                    after += 1;
                }
                let is_key = after < bytes.len() && bytes[after] == b':';
                tokens.push(HighlightToken {
                    start_byte: str_start,
                    end_byte: str_end,
                    token_type: if is_key {
                        TokenType::Function
                    } else {
                        TokenType::String
                    },
                });
            }
        } else if b.is_ascii_digit() || b == b'-' {
            let num_start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric()
                    || bytes[i] == b'.'
                    || bytes[i] == b'-'
                    || bytes[i] == b'+')
            {
                i += 1;
            }
            if i > start_byte {
                tokens.push(HighlightToken {
                    start_byte: num_start,
                    end_byte: i,
                    token_type: TokenType::Number,
                });
            }
        } else if b.is_ascii_alphabetic() {
            let kw_start = i;
            while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            let word = &sub[kw_start..i];
            if (word == "true" || word == "false" || word == "null") && i > start_byte {
                tokens.push(HighlightToken {
                    start_byte: kw_start,
                    end_byte: i,
                    token_type: TokenType::Keyword,
                });
            }
        } else {
            i += 1;
        }
    }
}

pub(crate) fn lex_toml(
    text: &str,
    start_byte: usize,
    end_byte: usize,
    tokens: &mut Vec<HighlightToken>,
) {
    let sub = &text[..end_byte.min(text.len())];
    let mut offset = 0;
    for raw_line in sub.split_inclusive('\n') {
        let line_start = offset;
        offset += raw_line.len();
        let line = raw_line.trim_end_matches(['\r', '\n']);
        let line_end = line_start + line.len();

        if line_end <= start_byte {
            continue;
        }

        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            tokens.push(HighlightToken {
                start_byte: line_start,
                end_byte: line_end,
                token_type: TokenType::Comment,
            });
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            tokens.push(HighlightToken {
                start_byte: line_start,
                end_byte: line_end,
                token_type: TokenType::Macro,
            });
            continue;
        }

        if let Some(eq_idx) = line.find('=') {
            tokens.push(HighlightToken {
                start_byte: line_start,
                end_byte: line_start + eq_idx,
                token_type: TokenType::Keyword,
            });

            let val_part = &line[eq_idx + 1..];
            if val_part.contains('"') || val_part.contains('\'') {
                tokens.push(HighlightToken {
                    start_byte: line_start + eq_idx + 1,
                    end_byte: line_end,
                    token_type: TokenType::String,
                });
            } else {
                tokens.push(HighlightToken {
                    start_byte: line_start + eq_idx + 1,
                    end_byte: line_end,
                    token_type: TokenType::Number,
                });
            }
        }
    }
}

pub(crate) fn lex_markdown(
    text: &str,
    start_byte: usize,
    end_byte: usize,
    tokens: &mut Vec<HighlightToken>,
) {
    let sub = &text[..end_byte.min(text.len())];
    let mut offset = 0;
    for raw_line in sub.split_inclusive('\n') {
        let line_start = offset;
        offset += raw_line.len();
        let line = raw_line.trim_end_matches(['\r', '\n']);
        let line_end = line_start + line.len();

        if line_end <= start_byte {
            continue;
        }

        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            tokens.push(HighlightToken {
                start_byte: line_start,
                end_byte: line_end,
                token_type: TokenType::Macro,
            });
        } else if trimmed.starts_with("```") {
            tokens.push(HighlightToken {
                start_byte: line_start,
                end_byte: line_end,
                token_type: TokenType::Type,
            });
        } else if trimmed.starts_with('>') {
            tokens.push(HighlightToken {
                start_byte: line_start,
                end_byte: line_end,
                token_type: TokenType::Comment,
            });
        } else if trimmed.starts_with('-') || trimmed.starts_with('*') || trimmed.starts_with("1.")
        {
            tokens.push(HighlightToken {
                start_byte: line_start,
                end_byte: line_start + 2,
                token_type: TokenType::Keyword,
            });
        }
    }
}

pub(crate) fn lex_shell(
    text: &str,
    start_byte: usize,
    end_byte: usize,
    tokens: &mut Vec<HighlightToken>,
) {
    let sub = &text[..end_byte.min(text.len())];
    let mut offset = 0;
    for raw_line in sub.split_inclusive('\n') {
        let line_start = offset;
        offset += raw_line.len();
        let line = raw_line.trim_end_matches(['\r', '\n']);
        let line_end = line_start + line.len();

        if line_end <= start_byte {
            continue;
        }

        let hash_idx = line.find('#');
        let code_part = if let Some(idx) = hash_idx {
            &line[..idx]
        } else {
            line
        };

        for (w_start, word) in find_words(code_part) {
            let abs_start = line_start + w_start;
            let abs_end = abs_start + word.len();

            if word.starts_with('$') {
                tokens.push(HighlightToken {
                    start_byte: abs_start,
                    end_byte: abs_end,
                    token_type: TokenType::Macro,
                });
            } else if matches!(
                word,
                "if" | "then"
                    | "else"
                    | "elif"
                    | "fi"
                    | "for"
                    | "in"
                    | "do"
                    | "done"
                    | "while"
                    | "case"
                    | "esac"
                    | "function"
                    | "echo"
                    | "export"
                    | "return"
                    | "exit"
            ) {
                tokens.push(HighlightToken {
                    start_byte: abs_start,
                    end_byte: abs_end,
                    token_type: TokenType::Keyword,
                });
            }
        }

        if let Some(idx) = hash_idx {
            tokens.push(HighlightToken {
                start_byte: line_start + idx,
                end_byte: line_end,
                token_type: TokenType::Comment,
            });
        }
    }
}

pub(crate) fn lex_go(
    text: &str,
    start_byte: usize,
    end_byte: usize,
    tokens: &mut Vec<HighlightToken>,
) {
    let sub = &text[..end_byte.min(text.len())];
    let mut offset = 0;
    for raw_line in sub.split_inclusive('\n') {
        let line_start = offset;
        offset += raw_line.len();
        let line = raw_line.trim_end_matches(['\r', '\n']);
        let line_end = line_start + line.len();

        if line_end <= start_byte {
            continue;
        }

        let comment_idx = line.find("//");
        let code_part = if let Some(idx) = comment_idx {
            &line[..idx]
        } else {
            line
        };

        for (w_start, word) in find_words(code_part) {
            let abs_start = line_start + w_start;
            let abs_end = abs_start + word.len();

            if matches!(
                word,
                "package"
                    | "import"
                    | "func"
                    | "return"
                    | "var"
                    | "const"
                    | "type"
                    | "struct"
                    | "interface"
                    | "if"
                    | "else"
                    | "for"
                    | "range"
                    | "go"
                    | "chan"
                    | "select"
                    | "case"
                    | "default"
                    | "switch"
                    | "break"
                    | "continue"
                    | "map"
            ) {
                tokens.push(HighlightToken {
                    start_byte: abs_start,
                    end_byte: abs_end,
                    token_type: TokenType::Keyword,
                });
            } else if matches!(
                word,
                "int" | "string" | "bool" | "byte" | "error" | "float64"
            ) {
                tokens.push(HighlightToken {
                    start_byte: abs_start,
                    end_byte: abs_end,
                    token_type: TokenType::Type,
                });
            }
        }

        if let Some(c_idx) = comment_idx {
            tokens.push(HighlightToken {
                start_byte: line_start + c_idx,
                end_byte: line_end,
                token_type: TokenType::Comment,
            });
        }
    }
}

pub(crate) fn lex_yaml(
    text: &str,
    start_byte: usize,
    end_byte: usize,
    tokens: &mut Vec<HighlightToken>,
) {
    lex_toml(text, start_byte, end_byte, tokens);
}

pub(crate) fn lex_html_css(
    text: &str,
    start_byte: usize,
    end_byte: usize,
    tokens: &mut Vec<HighlightToken>,
) {
    let sub = &text[..end_byte.min(text.len())];
    let bytes = sub.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'<' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'>' {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            if i > start_byte {
                tokens.push(HighlightToken {
                    start_byte: start,
                    end_byte: i,
                    token_type: TokenType::Macro,
                });
            }
        } else {
            i += 1;
        }
    }
}

fn find_words(line: &str) -> Vec<(usize, &str)> {
    let mut res = Vec::new();
    let mut in_word = false;
    let mut word_start = 0;

    for (idx, ch) in line.char_indices() {
        if ch.is_alphanumeric() || ch == '_' || ch == '.' || ch == '%' || ch == '$' || ch == ':' {
            if !in_word {
                in_word = true;
                word_start = idx;
            }
        } else if in_word {
            in_word = false;
            res.push((word_start, &line[word_start..idx]));
        }
    }
    if in_word {
        res.push((word_start, &line[word_start..]));
    }
    res
}
