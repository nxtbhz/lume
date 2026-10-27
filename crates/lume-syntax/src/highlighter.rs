use tree_sitter::{Node, Parser, Tree};

use crate::language::Language;
use crate::lexer::{
    lex_assembly, lex_go, lex_html_css, lex_json, lex_markdown, lex_shell, lex_toml, lex_yaml,
};
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
