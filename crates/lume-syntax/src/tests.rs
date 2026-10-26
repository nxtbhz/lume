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
    assert_eq!(Language::from_path("Makefile"), Some(Language::Assembly));
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
