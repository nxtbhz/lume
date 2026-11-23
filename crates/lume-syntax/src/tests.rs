use crate::highlighter::SyntaxHighlighter;
use crate::language::Language;
use crate::token::TokenType;
use crate::worker::{SyntaxRequest, SyntaxWorker};

#[test]
fn test_c_highlighting() {
    let code =
        "#include <stdio.h>\nint main() {\n    // comment\n    int x = 42;\n    return 0;\n}";
    let mut highlighter = SyntaxHighlighter::new(Language::C).unwrap();
    highlighter.update(code);
    let tokens = highlighter.highlight_range(0, code.len(), code);

    assert!(tokens.iter().any(|t| t.token_type == TokenType::Macro));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Type));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Function));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Comment));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Number));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Keyword));
}

#[test]
fn test_c_and_cpp_header_guards_preserve_syntax_highlighting() {
    let code = "\
#ifndef ARENA_H
#define ARENA_H
#include <stddef.h>
struct region_t { size_t size; };
# if 1
void arena_destroy(void);
#elif 0
int arena_size(void);
#else
int arena_capacity(void);
#endif
#endif /* ARENA_H */
";
    for path in ["arena.h", "arena.hpp"] {
        let mut highlighter = SyntaxHighlighter::new(Language::from_path(path).unwrap()).unwrap();
        highlighter.update(code);
        let tokens = highlighter.highlight_range(0, code.len(), code);

        for (fragment, token_type) in [
            ("#ifndef", TokenType::Macro),
            ("# if", TokenType::Macro),
            ("#elif", TokenType::Macro),
            ("#else", TokenType::Macro),
            ("#endif", TokenType::Macro),
            ("struct", TokenType::Keyword),
            ("size_t", TokenType::Type),
            ("arena_destroy", TokenType::Function),
            ("arena_size", TokenType::Function),
            ("arena_capacity", TokenType::Function),
            ("/* ARENA_H */", TokenType::Comment),
        ] {
            assert!(
                tokens.iter().any(|token| token.token_type == token_type
                    && &code[token.start_byte..token.end_byte] == fragment),
                "{path}: expected {fragment:?} to be highlighted as {token_type:?}"
            );
        }
    }
}

#[test]
fn test_cpp_cuda_highlighting() {
    let code =
        "__global__ void addKernel(int* c, const int* a) {\n    int i = threadIdx.x;\n    c[i] = a[i];\n}";
    let mut highlighter = SyntaxHighlighter::new(Language::Cpp).unwrap();
    highlighter.update(code);
    let tokens = highlighter.highlight_range(0, code.len(), code);

    assert!(tokens.iter().any(|t| t.token_type == TokenType::Keyword));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Type));
}

#[test]
fn test_java_highlighting() {
    let code = "public class Hello {\n    public static void main(String[] args) {\n        // comment\n        System.out.println(100);\n    }\n}";
    let mut highlighter = SyntaxHighlighter::new(Language::Java).unwrap();
    highlighter.update(code);
    let tokens = highlighter.highlight_range(0, code.len(), code);

    assert!(tokens.iter().any(|t| t.token_type == TokenType::Keyword));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Comment));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Number));
}

#[test]
fn test_assembly_highlighting() {
    let code = ".globl main\nmain:\n    mov %rax, %rbx ; test comment\n    syscall\n";
    let mut highlighter = SyntaxHighlighter::new(Language::Assembly).unwrap();
    highlighter.update(code);
    let tokens = highlighter.highlight_range(0, code.len(), code);

    assert!(tokens.iter().any(|t| t.token_type == TokenType::Macro));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Keyword));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Type));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Comment));
}

#[test]
fn test_rust_highlighting() {
    let code =
        "fn main() {\n    // test comment\n    let val: u32 = 123;\n    println!(\"hello\");\n}";
    let mut highlighter = SyntaxHighlighter::new(Language::Rust).unwrap();
    highlighter.update(code);
    let tokens = highlighter.highlight_range(0, code.len(), code);

    assert!(tokens.iter().any(|t| t.token_type == TokenType::Keyword));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Function));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Comment));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Type));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Number));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::String));
}

#[test]
fn test_syntax_worker_background() {
    let worker = SyntaxWorker::spawn();
    worker.send_request(SyntaxRequest {
        buffer_id: 1,
        revision: 1,
        text: "fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
        language: Language::Rust,
        visible_range: 0..40,
    });

    let mut received = None;
    for _ in 0..50 {
        if let Some(res) = worker.try_recv_response() {
            received = Some(res);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    assert!(received.is_some());
    let res = received.unwrap();
    assert_eq!(res.buffer_id, 1);
    assert!(!res.tokens.is_empty());
}

#[test]
fn test_crlf_line_endings_no_offset_drift() {
    // Go with CRLF line endings
    let go_code = "package main\r\nimport \"fmt\"\r\nfunc main() {\r\n    return\r\n}\r\n";
    let mut go_hl = SyntaxHighlighter::new(Language::Go).unwrap();
    go_hl.update(go_code);
    let go_tokens = go_hl.highlight_range(0, go_code.len(), go_code);
    let ret_pos = go_code.find("return").unwrap();
    assert!(go_tokens.iter().any(|t| t.start_byte == ret_pos
        && t.end_byte == ret_pos + "return".len()
        && t.token_type == TokenType::Keyword));

    // TOML with CRLF line endings
    let toml_code = "[package]\r\nname = \"lume\"\r\nedition = 2021\r\n";
    let mut toml_hl = SyntaxHighlighter::new(Language::Toml).unwrap();
    toml_hl.update(toml_code);
    let toml_tokens = toml_hl.highlight_range(0, toml_code.len(), toml_code);
    let ed_pos = toml_code.find("edition").unwrap();
    assert!(toml_tokens
        .iter()
        .any(|t| t.start_byte == ed_pos && t.token_type == TokenType::Keyword));
}

#[test]
fn test_language_id_and_path_resolution() {
    assert_eq!(Language::from_id("c"), Some(Language::C));
    assert_eq!(Language::from_id("c++"), Some(Language::Cpp));
    assert_eq!(Language::from_id("cuda"), Some(Language::Cpp));
    assert_eq!(Language::from_id("rs"), Some(Language::Rust));
    assert_eq!(Language::from_id("py"), Some(Language::Python));
    assert_eq!(Language::from_id("java"), Some(Language::Java));
    assert_eq!(Language::from_id("js"), Some(Language::JavaScript));
    assert_eq!(Language::from_id("ts"), Some(Language::TypeScript));
    assert_eq!(Language::from_id("asm"), Some(Language::Assembly));
    assert_eq!(Language::from_id("go"), Some(Language::Go));
    assert_eq!(Language::from_id("json"), Some(Language::Json));
    assert_eq!(Language::from_id("toml"), Some(Language::Toml));
    assert_eq!(Language::from_id("yaml"), Some(Language::Yaml));
    assert_eq!(Language::from_id("md"), Some(Language::Markdown));
    assert_eq!(Language::from_id("sh"), Some(Language::Shell));
    assert_eq!(Language::from_id("html"), Some(Language::Html));
    assert_eq!(Language::from_id("css"), Some(Language::Css));
    assert_eq!(Language::from_id("nonexistent_lang"), None);

    // Path resolution
    assert_eq!(Language::from_path("Cargo.toml"), Some(Language::Toml));
    assert_eq!(Language::from_path("Makefile"), Some(Language::Shell));
    assert_eq!(Language::from_path("Dockerfile"), Some(Language::Shell));
    assert_eq!(Language::from_path("main.rs"), Some(Language::Rust));
    assert_eq!(Language::from_path("script.py"), Some(Language::Python));
    assert_eq!(Language::from_path("main.go"), Some(Language::Go));
    assert_eq!(Language::from_path("data.json"), Some(Language::Json));
    assert_eq!(Language::from_path("doc.md"), Some(Language::Markdown));
    assert_eq!(Language::from_path("config.yml"), Some(Language::Yaml));
    assert_eq!(Language::from_path("index.html"), Some(Language::Html));
    assert_eq!(Language::from_path("style.css"), Some(Language::Css));
    assert_eq!(Language::from_path("unknown.xyz"), None);
}

#[test]
fn test_tokenizers_json_toml_markdown_shell_go_yaml_html() {
    // JSON
    let json_code = r#"{"name": "lume", "count": 42, "flag": true, "extra": null}"#;
    let mut json_hl = SyntaxHighlighter::new(Language::Json).unwrap();
    json_hl.update(json_code);
    let tokens = json_hl.highlight_range(0, json_code.len(), json_code);
    assert!(!tokens.is_empty());
    assert!(tokens
        .iter()
        .any(|t| t.token_type == TokenType::String || t.token_type == TokenType::Function));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Number));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Keyword));

    // TOML
    let toml_code = "[package]\nname = \"lume\"\nversion = 1\n# comment\n";
    let mut toml_hl = SyntaxHighlighter::new(Language::Toml).unwrap();
    toml_hl.update(toml_code);
    let tokens = toml_hl.highlight_range(0, toml_code.len(), toml_code);
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Macro));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Comment));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Keyword));

    // YAML
    let yaml_code = "key = \"value\"\n";
    let mut yaml_hl = SyntaxHighlighter::new(Language::Yaml).unwrap();
    yaml_hl.update(yaml_code);
    let tokens = yaml_hl.highlight_range(0, yaml_code.len(), yaml_code);
    assert!(!tokens.is_empty());

    // Markdown
    let md_code = "# Header 1\n```rust\nlet x = 1;\n```\n> Quote block\n- list item\n";
    let mut md_hl = SyntaxHighlighter::new(Language::Markdown).unwrap();
    md_hl.update(md_code);
    let tokens = md_hl.highlight_range(0, md_code.len(), md_code);
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Macro));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Type));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Comment));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Keyword));

    // Shell
    let sh_code = "#!/bin/sh\nVAR=10\necho $VAR\nif [ $VAR ]; then exit 0; fi\n";
    let mut sh_hl = SyntaxHighlighter::new(Language::Shell).unwrap();
    sh_hl.update(sh_code);
    let tokens = sh_hl.highlight_range(0, sh_code.len(), sh_code);
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Comment));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Macro));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Keyword));

    // Go
    let go_code = "package main\nimport \"fmt\"\n// comment\nfunc test() int {\nreturn 0\n}\n";
    let mut go_hl = SyntaxHighlighter::new(Language::Go).unwrap();
    go_hl.update(go_code);
    let tokens = go_hl.highlight_range(0, go_code.len(), go_code);
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Keyword));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Comment));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Type));

    // HTML
    let html_code = "<div class=\"app\">Hello</div>";
    let mut html_hl = SyntaxHighlighter::new(Language::Html).unwrap();
    html_hl.update(html_code);
    let tokens = html_hl.highlight_range(0, html_code.len(), html_code);
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Macro));
}

#[test]
fn test_python_and_javascript_highlighting() {
    // Python
    let py_code = "def calc(x):\n    # comment\n    return x + 1\n";
    let mut py_hl = SyntaxHighlighter::new(Language::Python).unwrap();
    py_hl.update(py_code);
    let tokens = py_hl.highlight_range(0, py_code.len(), py_code);
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Keyword));

    // JavaScript
    let js_code = "function greet(name) {\n    return 'Hello ' + name;\n}\n";
    let mut js_hl = SyntaxHighlighter::new(Language::JavaScript).unwrap();
    js_hl.update(js_code);
    let tokens = js_hl.highlight_range(0, js_code.len(), js_code);
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Keyword));

    // TypeScript
    let ts_code = "function add(a: number): number {\n    return a + 1;\n}\n";
    let mut ts_hl = SyntaxHighlighter::new(Language::TypeScript).unwrap();
    ts_hl.update(ts_code);
    let tokens = ts_hl.highlight_range(0, ts_code.len(), ts_code);
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Keyword));
}

#[test]
fn test_markdown_lexer_list_bounds_and_utf8() {
    let mut hl = SyntaxHighlighter::new(Language::Markdown).unwrap();
    // Bare "-" without newline, indented list, and UTF-8 char immediately after "-"
    for src in ["-", "  - item\n", "-ação\n", "1. item\n"] {
        hl.update(src);
        let tokens = hl.highlight_range(0, src.len(), src);
        assert!(!tokens.is_empty(), "expected token for {src:?}");
        for tok in &tokens {
            assert!(tok.start_byte <= tok.end_byte);
            assert!(
                tok.end_byte <= src.len(),
                "out of bounds for {src:?}: {tok:?}"
            );
            assert!(
                src.is_char_boundary(tok.start_byte),
                "start_byte not char boundary for {src:?}: {tok:?}"
            );
            assert!(
                src.is_char_boundary(tok.end_byte),
                "end_byte not char boundary for {src:?}: {tok:?}"
            );
        }
    }
}

#[test]
fn test_yaml_lexer_colon_key_value() {
    let mut hl = SyntaxHighlighter::new(Language::Yaml).unwrap();
    let code = "# comment\nname: \"lume\"\nenabled: true\ncount: 42\n  - item: 99\n";
    hl.update(code);
    let tokens = hl.highlight_range(0, code.len(), code);
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Comment));
    assert!(tokens
        .iter()
        .any(|t| t.token_type == TokenType::Keyword && &code[t.start_byte..t.end_byte] == "name"));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::String));
    assert!(tokens
        .iter()
        .any(|t| t.token_type == TokenType::Keyword && &code[t.start_byte..t.end_byte] == "true"));
    assert!(tokens.iter().any(|t| t.token_type == TokenType::Number));
    assert!(tokens
        .iter()
        .any(|t| t.token_type == TokenType::Operator && &code[t.start_byte..t.end_byte] == "-"));
    assert!(tokens
        .iter()
        .any(|t| t.token_type == TokenType::Keyword && &code[t.start_byte..t.end_byte] == "item"));
}

#[test]
fn test_syntax_worker_does_not_starve_other_tabs() {
    let worker = SyntaxWorker::spawn();
    // First request occupies the worker with Tree-sitter initialization,
    // while requests for buffer 1 and buffer 2 queue up simultaneously.
    worker.send_request(SyntaxRequest {
        buffer_id: 0,
        revision: 1,
        language: Language::Rust,
        text: "fn init() {}".to_string(),
        visible_range: 0..12,
    });
    worker.send_request(SyntaxRequest {
        buffer_id: 1,
        revision: 1,
        language: Language::Rust,
        text: "fn a() {}".to_string(),
        visible_range: 0..9,
    });
    worker.send_request(SyntaxRequest {
        buffer_id: 2,
        revision: 1,
        language: Language::Rust,
        text: "fn b() {}".to_string(),
        visible_range: 0..9,
    });

    let mut received_tabs = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while received_tabs.len() < 3 && std::time::Instant::now() < deadline {
        if let Some(resp) = worker.try_recv_response() {
            received_tabs.push(resp.buffer_id);
        } else {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    received_tabs.sort_unstable();
    assert_eq!(received_tabs, vec![0, 1, 2]);
}

#[test]
fn test_utf8_char_boundary_safety_in_lexers_and_worker() {
    let text = "# Comentário com acentuação e emojis 🦀 🚀\nname = \"lume\"\n";
    let mut tokens = Vec::new();
    // Test every single byte index as boundary, including mid-multibyte indices
    for byte_idx in 0..text.len() {
        tokens.clear();
        crate::lexer::lex_toml(text, 0, byte_idx, &mut tokens);
        crate::lexer::lex_assembly(text, 0, byte_idx, &mut tokens);
        crate::lexer::lex_markdown(text, 0, byte_idx, &mut tokens);
        crate::lexer::lex_shell(text, 0, byte_idx, &mut tokens);
        crate::lexer::lex_yaml(text, 0, byte_idx, &mut tokens);
    }

    let worker = SyntaxWorker::spawn();
    for byte_idx in 0..text.len() {
        worker.send_request(SyntaxRequest {
            buffer_id: 99,
            revision: byte_idx as u64,
            language: Language::Toml,
            text: text.to_string(),
            visible_range: 0..byte_idx,
        });
    }
}

#[test]
fn test_lex_json_trailing_backslash_no_oob() {
    let text = r#"{"k":"\"#;
    let mut tokens = Vec::new();
    crate::lexer::lex_json(text, 0, text.len(), &mut tokens);
    for t in &tokens {
        assert!(
            t.start_byte <= t.end_byte && t.end_byte <= text.len(),
            "token {:?} out of bounds for len {}",
            t,
            text.len()
        );
    }
}

fn fresh_tokens(language: Language, code: &str) -> Vec<crate::token::HighlightToken> {
    let mut fresh = SyntaxHighlighter::new(language).unwrap();
    fresh.update(code);
    fresh.highlight_range(0, code.len(), code)
}

#[test]
fn test_c_incremental_typing_and_deletion_matches_fresh_parse() {
    let final_code = "int is_delim(const char *c, char *delim)\n{\n    return 0;\n}\n\nstruct token_t tok(const char *src, const char *delim)\n{\n    static *str_ptr = NULL;\n    struct token_t token;\n}\n\nint main(int argc, char **argv)\n{\n    if(argc < 2)\n    {\n        die(\"Missing parameter\");\n    }\n    for(int i=0;i<argc;i++) printf(\"%s\\n\", argv[i]);\n    return 0;\n}\n";

    // 1. Typing character-by-character from empty must produce the exact same tokens as a fresh parse.
    let mut hl = SyntaxHighlighter::new(Language::C).unwrap();
    for end in 1..=final_code.len() {
        if final_code.is_char_boundary(end) {
            let prefix = &final_code[..end];
            hl.update(prefix);
        }
    }
    let inc_tokens = hl.highlight_range(0, final_code.len(), final_code);
    assert_eq!(inc_tokens, fresh_tokens(Language::C, final_code));

    // 2. Deleting lines at the top of an existing parsed C file must not leave stale byte offsets.
    let with_includes = format!("#include <stdio.h>\n#include <stdlib.h>\n\n{}", final_code);
    let mut hl_del = SyntaxHighlighter::new(Language::C).unwrap();
    hl_del.update(&with_includes);
    hl_del.update(final_code);
    let after_del_tokens = hl_del.highlight_range(0, final_code.len(), final_code);
    assert_eq!(after_del_tokens, fresh_tokens(Language::C, final_code));
}

#[test]
fn test_incremental_utf8_and_crlf_edits_match_fresh_parse() {
    // Multi-byte UTF-8 emojis sharing prefix/suffix bytes (🦀 U+1F980 vs 🚀 U+1F680) and accented text
    let mut hl_rust = SyntaxHighlighter::new(Language::Rust).unwrap();
    let r1 = "fn greet() {\n    let msg = \"Olá 🦀 mundo\";\n    let n = 10;\n}\n";
    let r2 = "fn greet() {\n    let msg = \"Olá 🚀 mundo!\";\n    let n = 42;\n}\n";
    hl_rust.update(r1);
    hl_rust.update(r2);
    assert_eq!(
        hl_rust.highlight_range(0, r2.len(), r2),
        fresh_tokens(Language::Rust, r2)
    );

    // Full replacement & identical resend
    let r3 = "struct Point { x: i32, y: i32 }\n";
    hl_rust.update(r3);
    assert_eq!(
        hl_rust.highlight_range(0, r3.len(), r3),
        fresh_tokens(Language::Rust, r3)
    );
    hl_rust.update(r3);
    assert_eq!(
        hl_rust.highlight_range(0, r3.len(), r3),
        fresh_tokens(Language::Rust, r3)
    );

    // Windows CRLF line endings
    let mut hl_c = SyntaxHighlighter::new(Language::C).unwrap();
    let c1 = "int main(void) {\r\n    int x = 1;\r\n    return 0;\r\n}\r\n";
    let c2 = "int main(void) {\r\n    const char *s = \"ok\";\r\n    int x = 1;\r\n    return 0;\r\n}\r\n";
    hl_c.update(c1);
    hl_c.update(c2);
    assert_eq!(
        hl_c.highlight_range(0, c2.len(), c2),
        fresh_tokens(Language::C, c2)
    );
}

#[test]
fn test_syntax_worker_incremental_updates_match_fresh_parse() {
    let worker = SyntaxWorker::spawn();
    let v1 = "#include <stdio.h>\nint main() {\n    return 0;\n}\n";
    let v2 = "int is_delim(const char *c) {\n    return 0;\n}\n";

    let wait_rev = |target_rev: u64| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if let Some(res) = worker.try_recv_response() {
                if res.buffer_id == 7 && res.revision == target_rev {
                    return res.tokens;
                }
            } else {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        panic!("timed out waiting for SyntaxWorker revision {target_rev}");
    };

    worker.send_request(SyntaxRequest {
        buffer_id: 7,
        revision: 1,
        language: Language::C,
        text: v1.to_string(),
        visible_range: 0..v1.len(),
    });
    assert_eq!(wait_rev(1), fresh_tokens(Language::C, v1));

    worker.send_request(SyntaxRequest {
        buffer_id: 7,
        revision: 2,
        language: Language::C,
        text: v2.to_string(),
        visible_range: 0..v2.len(),
    });
    assert_eq!(wait_rev(2), fresh_tokens(Language::C, v2));
}
