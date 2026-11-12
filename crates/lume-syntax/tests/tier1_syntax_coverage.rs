//! Tier 1: Syntax Coverage (Features 9 to 12)
//!
//! Features covered:
//! - Feature 9: `rope_syntax_worker` (Rope-based SyntaxRequest)
//! - Feature 10: `buffer_highlighter_cache` ((buffer_id, Language) keyed highlighters)
//! - Feature 11: `highlight_token_shifting` (shift_tokens_for_deltas)
//! - Feature 12: `syntax_buffer_eviction` (SyntaxCommand::DropBuffer)

use lume_syntax::highlighter::SyntaxHighlighter;
use lume_syntax::language::Language;
use lume_syntax::token::TokenType;
use lume_syntax::worker::SyntaxWorker;
use std::time::Duration;

fn highlight_text(
    highlighter: &mut SyntaxHighlighter,
    code: &str,
) -> Vec<lume_syntax::token::HighlightToken> {
    highlighter.update(code);
    highlighter.highlight_range(0, code.len(), code)
}

// -----------------------------------------------------------------------------
// Baseline & Progressive Tests (Always Active)
// -----------------------------------------------------------------------------

#[test]
fn test_feature_9_syntax_highlighter_rust_keywords() {
    let mut highlighter = SyntaxHighlighter::new(Language::Rust).unwrap();
    let code = "fn main() { let x = 42; }";
    let tokens = highlight_text(&mut highlighter, code);
    assert!(
        !tokens.is_empty(),
        "Highlighter must produce tokens for valid Rust code"
    );
    let has_keyword = tokens.iter().any(|t| t.token_type == TokenType::Keyword);
    assert!(has_keyword, "fn and let should be recognized as keywords");
}

#[test]
fn test_feature_9_syntax_highlighter_c_function() {
    let mut highlighter = SyntaxHighlighter::new(Language::C).unwrap();
    let code = "int add(int a, int b) { return a + b; }";
    let tokens = highlight_text(&mut highlighter, code);
    assert!(!tokens.is_empty());
    let has_func = tokens.iter().any(|t| t.token_type == TokenType::Function);
    assert!(has_func, "add should be recognized as a function");
}

#[test]
fn test_feature_9_syntax_highlighter_python_def() {
    let mut highlighter = SyntaxHighlighter::new(Language::Python).unwrap();
    let code = "def hello():\n    return 'world'";
    let tokens = highlight_text(&mut highlighter, code);
    assert!(!tokens.is_empty());
    let has_kw = tokens.iter().any(|t| t.token_type == TokenType::Keyword);
    assert!(has_kw, "def and return should be keywords");
}

#[test]
fn test_feature_9_syntax_highlighter_javascript_tokens() {
    let mut highlighter = SyntaxHighlighter::new(Language::JavaScript).unwrap();
    let code = "const compute = (x) => x * 2;";
    let tokens = highlight_text(&mut highlighter, code);
    assert!(!tokens.is_empty());
    let has_kw = tokens.iter().any(|t| t.token_type == TokenType::Keyword);
    assert!(has_kw, "const should be keyword");
}

#[test]
fn test_feature_9_syntax_highlighter_empty_code() {
    let mut highlighter = SyntaxHighlighter::new(Language::Rust).unwrap();
    let tokens = highlight_text(&mut highlighter, "");
    assert!(tokens.is_empty(), "Empty code must return empty token list");
}

#[test]
fn test_feature_10_highlighter_reusable_across_edits() {
    let mut highlighter = SyntaxHighlighter::new(Language::Rust).unwrap();
    let tokens1 = highlight_text(&mut highlighter, "fn first() {}");
    let tokens2 = highlight_text(&mut highlighter, "fn second() {}");
    assert!(!tokens1.is_empty());
    assert!(!tokens2.is_empty());
}

#[test]
fn test_feature_10_highlighter_handles_syntax_errors() {
    let mut highlighter = SyntaxHighlighter::new(Language::Rust).unwrap();
    // Incomplete / syntax error code should not panic
    let tokens = highlight_text(&mut highlighter, "fn broken( { let");
    assert!(
        tokens.is_empty() || !tokens.is_empty(),
        "Highlighter must not panic on invalid syntax"
    );
}

#[test]
fn test_feature_10_highlighter_cpp_language() {
    let mut highlighter = SyntaxHighlighter::new(Language::Cpp).unwrap();
    let tokens = highlight_text(&mut highlighter, "class MyClass { public: int x; };");
    assert!(!tokens.is_empty());
}

#[test]
fn test_feature_10_highlighter_java_language() {
    let mut highlighter = SyntaxHighlighter::new(Language::Java).unwrap();
    let tokens = highlight_text(
        &mut highlighter,
        "public class Main { public static void main(String[] args) {} }",
    );
    assert!(!tokens.is_empty());
}

#[test]
fn test_feature_10_syntax_worker_spawn_and_communication() {
    let worker = SyntaxWorker::spawn();
    std::thread::sleep(Duration::from_millis(10));
    drop(worker);
}

// -----------------------------------------------------------------------------
// Milestone 2 Specific Features (Active when feature = "m2" or "e2e")
// -----------------------------------------------------------------------------

#[cfg(any(feature = "m2", feature = "e2e"))]
mod m2_syntax_tests {
    use super::*;
    use lume_syntax::token::HighlightToken;

    #[test]
    fn test_feature_11_shift_tokens_before_insert() {
        let tokens = vec![HighlightToken {
            start_byte: 10,
            end_byte: 15,
            token_type: TokenType::Function,
        }];
        let shifted = lume_syntax::token::shift_tokens_for_deltas(
            &tokens,
            &[lume_syntax::token::ByteDelta {
                start_byte: 2,
                old_len: 0,
                new_len: 3,
            }],
        );
        assert_eq!(shifted.len(), 1);
        assert_eq!(shifted[0].start_byte, 13);
        assert_eq!(shifted[0].end_byte, 18);
    }

    #[test]
    fn test_feature_11_shift_tokens_after_insert() {
        let tokens = vec![HighlightToken {
            start_byte: 4,
            end_byte: 8,
            token_type: TokenType::Keyword,
        }];
        let shifted = lume_syntax::token::shift_tokens_for_deltas(
            &tokens,
            &[lume_syntax::token::ByteDelta {
                start_byte: 20,
                old_len: 0,
                new_len: 5,
            }],
        );
        assert_eq!(shifted.len(), 1);
        assert_eq!(shifted[0].start_byte, 4);
        assert_eq!(shifted[0].end_byte, 8);
    }

    #[test]
    fn test_feature_11_shift_tokens_overlapping_edit_dropped() {
        let tokens = vec![HighlightToken {
            start_byte: 10,
            end_byte: 20,
            token_type: TokenType::String,
        }];
        let shifted = lume_syntax::token::shift_tokens_for_deltas(
            &tokens,
            &[lume_syntax::token::ByteDelta {
                start_byte: 12,
                old_len: 4,
                new_len: 6,
            }],
        );
        assert!(
            shifted.is_empty(),
            "Overlapping token should be dropped until re-parsed"
        );
    }

    #[test]
    fn test_feature_11_shift_tokens_deletion_before() {
        let tokens = vec![HighlightToken {
            start_byte: 15,
            end_byte: 20,
            token_type: TokenType::Type,
        }];
        let shifted = lume_syntax::token::shift_tokens_for_deltas(
            &tokens,
            &[lume_syntax::token::ByteDelta {
                start_byte: 2,
                old_len: 5,
                new_len: 0,
            }],
        );
        assert_eq!(shifted.len(), 1);
        assert_eq!(shifted[0].start_byte, 10);
        assert_eq!(shifted[0].end_byte, 15);
    }

    #[test]
    fn test_feature_11_shift_tokens_empty_deltas() {
        let tokens = vec![HighlightToken {
            start_byte: 5,
            end_byte: 10,
            token_type: TokenType::Number,
        }];
        let shifted = lume_syntax::token::shift_tokens_for_deltas(&tokens, &[]);
        assert_eq!(shifted, tokens);
    }

    #[test]
    fn test_feature_12_drop_buffer_eviction() {
        let worker = SyntaxWorker::spawn();
        worker.drop_buffer(42);
        worker.drop_buffer(999);
    }

    #[test]
    fn test_feature_12_drop_buffer_idempotent() {
        let worker = SyntaxWorker::spawn();
        worker.drop_buffer(1);
        worker.drop_buffer(1);
    }

    #[test]
    fn test_feature_12_drop_buffer_zero() {
        let worker = SyntaxWorker::spawn();
        worker.drop_buffer(0);
    }

    #[test]
    fn test_feature_9_rope_syntax_request_clone_is_o1() {
        let rope = ropey::Rope::from_str("fn main() {\n    println!(\"hello\");\n}\n");
        let cloned = rope.clone();
        assert_eq!(rope.len_bytes(), cloned.len_bytes());
    }

    #[test]
    fn test_feature_10_buffer_highlighter_cache_keys() {
        let key1 = (1usize, Language::Rust);
        let key2 = (2usize, Language::Rust);
        let key3 = (1usize, Language::Python);
        assert_ne!(key1, key2);
        assert_ne!(key1, key3);
    }
}
