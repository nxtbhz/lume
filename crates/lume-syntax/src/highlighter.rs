use tree_sitter::{Node, Parser, Tree};

use crate::language::Language;
use crate::token::{HighlightToken, TokenType};

pub struct SyntaxHighlighter {
    language: Language,
    parser: Option<Parser>,
    tree: Option<Tree>,
}

impl SyntaxHighlighter {
    pub fn new(language: Language) -> Result<Self, String> {
        if let Some(ts_lang) = language.tree_sitter_language() {
            let mut parser = Parser::new();
            parser
                .set_language(&ts_lang)
                .map_err(|e| format!("Tree-sitter language error: {:?}", e))?;
            Ok(Self {
                language,
                parser: Some(parser),
                tree: None,
            })
        } else {
            Ok(Self {
                language,
                parser: None,
                tree: None,
            })
        }
    }

    pub fn update(&mut self, text: &str) {
        if let Some(ref mut parser) = self.parser {
            self.tree = parser.parse(text, None);
        }
    }

    pub fn language(&self) -> Language {
        self.language
    }

    pub fn highlight_range(
        &self,
        start_byte: usize,
        end_byte: usize,
        text: &str,
    ) -> Vec<HighlightToken> {
        let mut tokens = Vec::new();
        if let Some(ref tree) = self.tree {
            let root = tree.root_node();
            self.collect_tokens(root, start_byte, end_byte, text, &mut tokens);
        } else {
            // Lexer fallback for non-tree-sitter languages (Assembly, JSON, TOML, Markdown, etc.)
            self.lex_tokens(start_byte, end_byte, text, &mut tokens);
        }
        tokens
    }

    fn collect_tokens(
        &self,
        node: Node,
        start_byte: usize,
        end_byte: usize,
        text: &str,
        tokens: &mut Vec<HighlightToken>,
    ) {
        let node_start = node.start_byte();
        let node_end = node.end_byte();

        if node_end <= start_byte || node_start >= end_byte {
            return;
        }

        let kind = node.kind();
        let token_type = self.classify_node(kind, node, text);

        if let Some(t_type) = token_type {
            if node.child_count() == 0
                || t_type == TokenType::Comment
                || t_type == TokenType::String
                || t_type == TokenType::Macro
            {
                tokens.push(HighlightToken {
                    start_byte: node_start,
                    end_byte: node_end,
                    token_type: t_type,
                });
                return;
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_tokens(child, start_byte, end_byte, text, tokens);
        }
    }

    fn classify_node(&self, kind: &str, node: Node, text: &str) -> Option<TokenType> {
        match self.language {
            Language::C => classify_c_node(kind, node, text),
            Language::Cpp => classify_cpp_node(kind, node, text),
            Language::Rust => classify_rust_node(kind, node, text),
            Language::Python => classify_python_node(kind, node, text),
            Language::Java => classify_java_node(kind, node, text),
            Language::JavaScript | Language::TypeScript => {
                classify_javascript_node(kind, node, text)
            }
            _ => None,
        }
    }

    fn lex_tokens(
        &self,
        start_byte: usize,
        end_byte: usize,
        text: &str,
        tokens: &mut Vec<HighlightToken>,
    ) {
        match self.language {
            Language::Assembly => lex_assembly(text, start_byte, end_byte, tokens),
            Language::Json => lex_json(text, start_byte, end_byte, tokens),
            Language::Toml => lex_toml(text, start_byte, end_byte, tokens),
            Language::Markdown => lex_markdown(text, start_byte, end_byte, tokens),
            Language::Shell => lex_shell(text, start_byte, end_byte, tokens),
            Language::Go => lex_go(text, start_byte, end_byte, tokens),
            Language::Yaml => lex_yaml(text, start_byte, end_byte, tokens),
            Language::Html | Language::Css => lex_html_css(text, start_byte, end_byte, tokens),
            _ => {}
        }
    }
}

fn classify_c_node(kind: &str, node: Node, text: &str) -> Option<TokenType> {
    match kind {
        "primitive_type" | "type_identifier" | "sized_type_specifier" => Some(TokenType::Type),
        "string_literal" | "system_lib_string" | "char_literal" => Some(TokenType::String),
        "number_literal" => Some(TokenType::Number),
        "comment" => Some(TokenType::Comment),
        "preproc_include"
        | "preproc_def"
        | "preproc_function_def"
        | "preproc_ifdef"
        | "preproc_if" => Some(TokenType::Macro),
        "identifier" => {
            if let Some(parent) = node.parent() {
                if parent.kind() == "call_expression" || parent.kind() == "function_declarator" {
                    return Some(TokenType::Function);
                }
            }
            None
        }
        _ => {
            let slice = &text[node.start_byte()..node.end_byte().min(text.len())];
            match slice {
                "if" | "else" | "for" | "while" | "do" | "return" | "break" | "continue"
                | "switch" | "case" | "default" | "goto" | "struct" | "union" | "enum"
                | "typedef" | "static" | "extern" | "const" | "volatile" | "inline" | "sizeof" => {
                    Some(TokenType::Keyword)
                }
                _ => None,
            }
        }
    }
}

fn classify_cpp_node(kind: &str, node: Node, text: &str) -> Option<TokenType> {
    match kind {
        "primitive_type" | "type_identifier" | "sized_type_specifier" | "namespace_identifier" => {
            Some(TokenType::Type)
        }
        "string_literal" | "system_lib_string" | "char_literal" | "raw_string_literal" => {
            Some(TokenType::String)
        }
        "number_literal" => Some(TokenType::Number),
        "comment" => Some(TokenType::Comment),
        "preproc_include"
        | "preproc_def"
        | "preproc_function_def"
        | "preproc_ifdef"
        | "preproc_if" => Some(TokenType::Macro),
        "identifier" | "field_identifier" => {
            if let Some(parent) = node.parent() {
                if parent.kind() == "call_expression" || parent.kind() == "function_declarator" {
                    return Some(TokenType::Function);
                }
            }
            None
        }
        _ => {
            let slice = &text[node.start_byte()..node.end_byte().min(text.len())];
            match slice {
                "class" | "struct" | "union" | "enum" | "public" | "private" | "protected"
                | "virtual" | "override" | "final" | "template" | "typename" | "namespace"
                | "using" | "new" | "delete" | "this" | "nullptr" | "constexpr" | "auto"
                | "decltype" | "static_assert" | "try" | "catch" | "throw" | "noexcept"
                | "const" | "volatile" | "static" | "extern" | "inline" | "explicit" | "friend"
                | "if" | "else" | "for" | "while" | "do" | "return" | "break" | "continue"
                | "switch" | "case" | "default" | "goto" | "sizeof" | "true" | "false"
                | "__global__" | "__device__" | "__host__" | "__shared__" | "__constant__" => {
                    Some(TokenType::Keyword)
                }
                _ => None,
            }
        }
    }
}

fn classify_rust_node(kind: &str, node: Node, text: &str) -> Option<TokenType> {
    match kind {
        "primitive_type" | "type_identifier" => Some(TokenType::Type),
        "string_literal" | "raw_string_literal" | "char_literal" => Some(TokenType::String),
        "integer_literal" | "float_literal" | "boolean_literal" => Some(TokenType::Number),
        "line_comment" | "block_comment" => Some(TokenType::Comment),
        "identifier" => {
            if let Some(parent) = node.parent() {
                if parent.kind() == "macro_invocation" {
                    return Some(TokenType::Macro);
                }
                if parent.kind() == "call_expression" || parent.kind() == "function_item" {
                    return Some(TokenType::Function);
                }
            }
            None
        }
        _ => {
            let slice = &text[node.start_byte()..node.end_byte().min(text.len())];
            match slice {
                "fn" | "let" | "mut" | "pub" | "use" | "mod" | "struct" | "enum" | "impl"
                | "trait" | "match" | "if" | "else" | "for" | "while" | "loop" | "return"
                | "break" | "continue" | "const" | "static" | "async" | "await" | "where"
                | "as" | "in" | "ref" | "type" | "unsafe" | "self" | "Self" => {
                    Some(TokenType::Keyword)
                }
                _ => None,
            }
        }
    }
}

fn classify_python_node(kind: &str, node: Node, text: &str) -> Option<TokenType> {
    match kind {
        "string" => Some(TokenType::String),
        "integer" | "float" => Some(TokenType::Number),
        "comment" => Some(TokenType::Comment),
        "identifier" => {
            if let Some(parent) = node.parent() {
                if parent.kind() == "call" || parent.kind() == "function_definition" {
                    return Some(TokenType::Function);
                }
            }
            None
        }
        _ => {
            let slice = &text[node.start_byte()..node.end_byte().min(text.len())];
            match slice {
                "def" | "class" | "import" | "from" | "as" | "if" | "elif" | "else" | "for"
                | "while" | "return" | "yield" | "break" | "continue" | "try" | "except"
                | "finally" | "raise" | "with" | "pass" | "lambda" | "in" | "is" | "not"
                | "and" | "or" | "None" | "True" | "False" => Some(TokenType::Keyword),
                _ => None,
            }
        }
    }
}

fn classify_java_node(kind: &str, node: Node, text: &str) -> Option<TokenType> {
    match kind {
        "integral_type"
        | "floating_point_type"
        | "boolean_type"
        | "void_type"
        | "type_identifier" => Some(TokenType::Type),
        "string_literal" | "character_literal" => Some(TokenType::String),
        "decimal_integer_literal"
        | "hex_integer_literal"
        | "octal_integer_literal"
        | "binary_integer_literal"
        | "decimal_floating_point_literal"
        | "hex_floating_point_literal"
        | "true"
        | "false" => Some(TokenType::Number),
        "line_comment" | "block_comment" => Some(TokenType::Comment),
        "identifier" => {
            if let Some(parent) = node.parent() {
                if parent.kind() == "method_invocation" || parent.kind() == "method_declaration" {
                    return Some(TokenType::Function);
                }
            }
            None
        }
        _ => {
            let slice = &text[node.start_byte()..node.end_byte().min(text.len())];
            match slice {
                "public" | "private" | "protected" | "static" | "final" | "abstract" | "class"
                | "interface" | "enum" | "record" | "extends" | "implements" | "new"
                | "package" | "import" | "return" | "if" | "else" | "for" | "while" | "do"
                | "switch" | "case" | "default" | "break" | "continue" | "try" | "catch"
                | "finally" | "throw" | "throws" | "this" | "super" | "null" | "instanceof"
                | "synchronized" | "volatile" | "transient" | "native" | "assert" | "yield" => {
                    Some(TokenType::Keyword)
                }
                _ => None,
            }
        }
    }
}

fn classify_javascript_node(kind: &str, node: Node, text: &str) -> Option<TokenType> {
    match kind {
        "string" | "template_string" => Some(TokenType::String),
        "number" | "regex" | "true" | "false" => Some(TokenType::Number),
        "comment" => Some(TokenType::Comment),
        "identifier" | "property_identifier" => {
            if let Some(parent) = node.parent() {
                if parent.kind() == "call_expression"
                    || parent.kind() == "function_declaration"
                    || parent.kind() == "method_definition"
                {
                    return Some(TokenType::Function);
                }
            }
            None
        }
        _ => {
            let slice = &text[node.start_byte()..node.end_byte().min(text.len())];
            match slice {
                "function" | "const" | "let" | "var" | "return" | "if" | "else" | "for"
                | "while" | "do" | "switch" | "case" | "default" | "break" | "continue"
                | "import" | "export" | "from" | "as" | "class" | "extends" | "new" | "this"
                | "super" | "async" | "await" | "yield" | "try" | "catch" | "finally" | "throw"
                | "typeof" | "instanceof" | "void" | "delete" | "in" | "of" | "null"
                | "undefined" => Some(TokenType::Keyword),
                _ => None,
            }
        }
    }
}

// Tokenizer fallbacks for Assembly, JSON, TOML, Markdown, Shell, Go, YAML, HTML
fn lex_assembly(text: &str, start_byte: usize, end_byte: usize, tokens: &mut Vec<HighlightToken>) {
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

fn is_assembly_register(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "rax"
            | "rbx"
            | "rcx"
            | "rdx"
            | "rsi"
            | "rdi"
            | "rbp"
            | "rsp"
            | "r8"
            | "r9"
            | "r10"
            | "r11"
            | "r12"
            | "r13"
            | "r14"
            | "r15"
            | "eax"
            | "ebx"
            | "ecx"
            | "edx"
            | "esi"
            | "edi"
            | "ebp"
            | "esp"
            | "ax"
            | "bx"
            | "cx"
            | "dx"
            | "sp"
            | "bp"
            | "si"
            | "di"
            | "al"
            | "bl"
            | "cl"
            | "dl"
            | "ah"
            | "bh"
            | "ch"
            | "dh"
    )
}

fn is_assembly_mnemonic(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "mov"
            | "push"
            | "pop"
            | "lea"
            | "add"
            | "sub"
            | "imul"
            | "idiv"
            | "mul"
            | "div"
            | "inc"
            | "dec"
            | "and"
            | "or"
            | "xor"
            | "not"
            | "shl"
            | "shr"
            | "sal"
            | "sar"
            | "cmp"
            | "test"
            | "jmp"
            | "je"
            | "jne"
            | "jz"
            | "jnz"
            | "jg"
            | "jge"
            | "jl"
            | "jle"
            | "call"
            | "ret"
            | "leave"
            | "syscall"
            | "int"
            | "nop"
            | "hlt"
    )
}

fn lex_json(text: &str, start_byte: usize, end_byte: usize, tokens: &mut Vec<HighlightToken>) {
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

fn lex_toml(text: &str, start_byte: usize, end_byte: usize, tokens: &mut Vec<HighlightToken>) {
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

fn lex_markdown(text: &str, start_byte: usize, end_byte: usize, tokens: &mut Vec<HighlightToken>) {
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

fn lex_shell(text: &str, start_byte: usize, end_byte: usize, tokens: &mut Vec<HighlightToken>) {
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

fn lex_go(text: &str, start_byte: usize, end_byte: usize, tokens: &mut Vec<HighlightToken>) {
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

fn lex_yaml(text: &str, start_byte: usize, end_byte: usize, tokens: &mut Vec<HighlightToken>) {
    lex_toml(text, start_byte, end_byte, tokens);
}

fn lex_html_css(text: &str, start_byte: usize, end_byte: usize, tokens: &mut Vec<HighlightToken>) {
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
