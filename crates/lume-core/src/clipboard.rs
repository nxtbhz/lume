//! Zero-dependency pure-Rust RFC 4648 Base64 encoder and ANSI OSC 52 clipboard escape sequence generator.

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard RFC 4648 Base64 encoder in 100% safe Rust without external dependencies.
pub fn encode_base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let (chunks, remainder) = data.as_chunks::<3>();

    for chunk in chunks {
        let b = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32);
        out.push(BASE64_ALPHABET[((b >> 18) & 0x3F) as usize] as char);
        out.push(BASE64_ALPHABET[((b >> 12) & 0x3F) as usize] as char);
        out.push(BASE64_ALPHABET[((b >> 6) & 0x3F) as usize] as char);
        out.push(BASE64_ALPHABET[(b & 0x3F) as usize] as char);
    }

    match remainder.len() {
        1 => {
            let b = (remainder[0] as u32) << 16;
            out.push(BASE64_ALPHABET[((b >> 18) & 0x3F) as usize] as char);
            out.push(BASE64_ALPHABET[((b >> 12) & 0x3F) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let b = ((remainder[0] as u32) << 16) | ((remainder[1] as u32) << 8);
            out.push(BASE64_ALPHABET[((b >> 18) & 0x3F) as usize] as char);
            out.push(BASE64_ALPHABET[((b >> 12) & 0x3F) as usize] as char);
            out.push(BASE64_ALPHABET[((b >> 6) & 0x3F) as usize] as char);
            out.push('=');
        }
        _ => {}
    }

    out
}

/// Maximum payload byte limit (10MB) for OSC 52 sequence generation to avoid choking PTY stream.
pub const MAX_OSC52_BYTES: usize = 10 * 1024 * 1024;

/// Builds standard ANSI OSC 52 clipboard sequence: `\x1b]52;c;<base64>\x07`
pub fn osc52_copy_sequence(text: &str) -> String {
    if text.len() > MAX_OSC52_BYTES {
        return String::new();
    }
    let b64 = encode_base64(text.as_bytes());
    format!("\x1b]52;c;{}\x07", b64)
}

/// Builds OSC 52 clipboard sequence, wrapping in tmux DCS passthrough (`\x1bPtmux;\x1b...\x1b\\`)
/// with doubled ESC characters when `in_tmux` is true.
pub fn osc52_sequence_with_passthrough(text: &str, in_tmux: bool) -> String {
    let seq = osc52_copy_sequence(text);
    if in_tmux {
        let doubled = seq.replace('\x1b', "\x1b\x1b");
        format!("\x1bPtmux;{}\x1b\\", doubled)
    } else {
        seq
    }
}

/// Automatically detects whether the session is inside tmux or screen,
/// generating the appropriate direct or DCS-wrapped OSC 52 escape sequence.
pub fn osc52_sequence_auto(text: &str) -> String {
    let in_tmux = std::env::var_os("TMUX").is_some()
        || std::env::var("TERM")
            .map(|t| t.starts_with("screen") || t.starts_with("tmux"))
            .unwrap_or(false);
    osc52_sequence_with_passthrough(text, in_tmux)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_base64_rfc_vectors() {
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        assert_eq!(encode_base64(b"foob"), "Zm9vYg==");
        assert_eq!(encode_base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode_base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn test_encode_base64_multibyte_utf8() {
        let text = "Olá, Mundo! 🚀 Área de transferência";
        let encoded = encode_base64(text.as_bytes());
        assert!(!encoded.is_empty());
        assert!(encoded
            .chars()
            .all(|c| c.is_alphanumeric() || c == '+' || c == '/' || c == '='));
    }

    #[test]
    fn test_osc52_copy_sequence_format() {
        let seq = osc52_copy_sequence("hello");
        assert_eq!(seq, "\x1b]52;c;aGVsbG8=\x07");
    }

    #[test]
    fn test_osc52_tmux_passthrough() {
        let normal = osc52_sequence_with_passthrough("hello", false);
        assert_eq!(normal, "\x1b]52;c;aGVsbG8=\x07");

        let tmux = osc52_sequence_with_passthrough("hello", true);
        assert_eq!(tmux, "\x1bPtmux;\x1b\x1b]52;c;aGVsbG8=\x07\x1b\\");
    }

    #[test]
    fn test_osc52_max_bytes_limit() {
        let text = "a".repeat(MAX_OSC52_BYTES + 1);
        assert_eq!(osc52_copy_sequence(&text), "");
    }
}
