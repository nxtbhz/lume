use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use lume_core::RopeBuffer;
use serde_json::Value;
use url::Url;

use crate::lsp::{DefinitionTarget, LspError, Position, PositionEncoding};

/// Maximum accepted LSP message payload size (64 MiB).
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_HEADER_LINE_BYTES: usize = 16 * 1024;
const MAX_HEADER_COUNT: usize = 64;

/// Converts a local filesystem path into a `file://` URI string.
pub fn path_to_uri(path: &Path) -> Result<String, LspError> {
    Url::from_file_path(path)
        .map(|u| u.to_string())
        .map_err(|()| {
            LspError::Protocol(format!(
                "path '{}' cannot be converted to a file:// URI",
                path.display()
            ))
        })
}

/// Converts a `file://` URI string into a local filesystem path.
/// Returns `None` for virtual or non-file URIs.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let url = Url::parse(uri).ok()?;
    if url.scheme() != "file" {
        return None;
    }
    url.to_file_path().ok()
}

/// Reads a single JSON-RPC message framed with LSP `Content-Length` headers.
///
/// Returns `Ok(None)` on clean EOF before any header byte is read.
pub fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut content_length: Option<usize> = None;
    let mut header_count = 0usize;
    let mut line_buf = Vec::new();

    loop {
        line_buf.clear();
        let bytes_read = read_bounded_line(reader, &mut line_buf, MAX_HEADER_LINE_BYTES)?;
        if bytes_read == 0 {
            if header_count == 0 {
                return Ok(None);
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "EOF while reading LSP headers",
            ));
        }

        if !line_buf.ends_with(b"\r\n") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "LSP header line missing CRLF terminator",
            ));
        }
        line_buf.truncate(line_buf.len() - 2);

        if line_buf.is_empty() {
            break;
        }

        header_count += 1;
        if header_count > MAX_HEADER_COUNT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "too many LSP headers",
            ));
        }

        let line_str = std::str::from_utf8(&line_buf)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "non-UTF-8 LSP header line"))?;
        let (name, value) = line_str.split_once(':').ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "malformed LSP header line")
        })?;

        if name.trim().eq_ignore_ascii_case("Content-Length") {
            let parsed = value.trim().parse::<usize>().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid Content-Length header")
            })?;
            if let Some(prev) = content_length {
                if prev != parsed {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "conflicting Content-Length headers",
                    ));
                }
            }
            content_length = Some(parsed);
        }
    }

    let length = content_length.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length header")
    })?;
    if length > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("LSP message length {length} exceeds maximum {MAX_MESSAGE_BYTES}"),
        ));
    }

    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;

    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("invalid JSON: {e}")))
}

fn read_bounded_line(
    reader: &mut impl BufRead,
    out: &mut Vec<u8>,
    max_bytes: usize,
) -> io::Result<usize> {
    let mut total = 0usize;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(total);
        }
        let newline_pos = available.iter().position(|&b| b == b'\n');
        let to_take = match newline_pos {
            Some(pos) => pos + 1,
            None => available.len(),
        };
        if total + to_take > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "LSP header line exceeds maximum length",
            ));
        }
        out.extend_from_slice(&available[..to_take]);
        reader.consume(to_take);
        total += to_take;
        if newline_pos.is_some() {
            return Ok(total);
        }
    }
}

/// Writes a single JSON-RPC message framed with `Content-Length`.
pub fn write_message(writer: &mut impl Write, message: &Value) -> io::Result<()> {
    let payload = serde_json::to_vec(message).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("JSON encode error: {e}"),
        )
    })?;
    if payload.len() > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "LSP payload length {} exceeds maximum {MAX_MESSAGE_BYTES}",
                payload.len()
            ),
        ));
    }
    write!(writer, "Content-Length: {}\r\n\r\n", payload.len())?;
    writer.write_all(&payload)?;
    writer.flush()
}

/// Converts a UTF-8 byte offset in `text` into an LSP [`Position`] using `encoding`.
///
/// Recognizes LSP 3.17 line breaks (`\r\n`, `\n`, `\r`). Returns `None` if `byte`
/// is out of bounds, not a UTF-8 character boundary, or inside a `\r\n` sequence.
pub fn position_at_byte(
    text: &RopeBuffer,
    byte: usize,
    encoding: PositionEncoding,
) -> Option<Position> {
    let rope = text.rope();
    let total_bytes = rope.len_bytes();
    if byte > total_bytes {
        return None;
    }
    let ch_idx = rope.byte_to_char(byte);
    if rope.char_to_byte(ch_idx) != byte {
        return None;
    }

    let mut line: u32 = 0;
    let mut line_start: usize = 0;
    let mut offset: usize = 0;
    let mut prev_was_cr = false;

    for chunk in rope.chunks() {
        if offset >= byte {
            break;
        }
        let bytes = chunk.as_bytes();
        let take = bytes.len().min(byte - offset);
        for &b in &bytes[..take] {
            if prev_was_cr {
                prev_was_cr = false;
                if b == b'\n' {
                    offset += 1;
                    line_start = offset;
                    continue;
                }
            }
            if b == b'\r' {
                line = line.checked_add(1)?;
                offset += 1;
                line_start = offset;
                prev_was_cr = true;
            } else if b == b'\n' {
                line = line.checked_add(1)?;
                offset += 1;
                line_start = offset;
            } else {
                offset += 1;
            }
        }
    }

    // Reject offsets pointing directly at the `\n` half of a `\r\n` terminator.
    if prev_was_cr && byte < total_bytes && rope.byte(byte) == b'\n' {
        return None;
    }

    let prefix = rope.byte_slice(line_start..byte);
    let col = match encoding {
        PositionEncoding::Utf8 => prefix.len_bytes(),
        PositionEncoding::Utf16 => prefix.len_utf16_cu(),
        PositionEncoding::Utf32 => prefix.len_chars(),
    };
    let character = u32::try_from(col).ok()?;
    Some(Position { line, character })
}

/// Converts an LSP [`Position`] into a UTF-8 byte offset in `text`.
///
/// Recognizes LSP 3.17 line breaks (`\r\n`, `\n`, `\r`). Returns `None` if `position.line`
/// is out of bounds or if `position.character` lands inside a multi-unit character
/// (e.g. mid UTF-16 surrogate pair or mid UTF-8 sequence). Columns past the end of the
/// line resolve to the end of the line's content (before any line terminator).
pub fn byte_at_position(
    text: &RopeBuffer,
    position: Position,
    encoding: PositionEncoding,
) -> Option<usize> {
    let rope = text.rope();
    let (line_start, line_content_end) = find_lsp_line_bounds(text, position.line)?;
    let content = rope.byte_slice(line_start..line_content_end);
    let col = position.character as usize;

    match encoding {
        PositionEncoding::Utf8 => {
            if col >= content.len_bytes() {
                return Some(line_content_end);
            }
            let ch_idx = content.byte_to_char(col);
            if content.char_to_byte(ch_idx) != col {
                return None;
            }
            Some(line_start + col)
        }
        PositionEncoding::Utf16 => {
            if col >= content.len_utf16_cu() {
                return Some(line_content_end);
            }
            let ch_idx = content.utf16_cu_to_char(col);
            if content.char_to_utf16_cu(ch_idx) != col {
                return None;
            }
            Some(line_start + content.char_to_byte(ch_idx))
        }
        PositionEncoding::Utf32 => {
            if col >= content.len_chars() {
                return Some(line_content_end);
            }
            Some(line_start + content.char_to_byte(col))
        }
    }
}

fn find_lsp_line_bounds(text: &RopeBuffer, target_line: u32) -> Option<(usize, usize)> {
    let rope = text.rope();
    let mut current_line: u32 = 0;
    let mut line_start: usize = 0;
    let mut offset: usize = 0;
    let mut prev_was_cr = false;

    for chunk in rope.chunks() {
        for &b in chunk.as_bytes() {
            if prev_was_cr {
                prev_was_cr = false;
                if b == b'\n' {
                    offset += 1;
                    line_start = offset;
                    continue;
                }
            }
            if b == b'\r' {
                if current_line == target_line {
                    return Some((line_start, offset));
                }
                current_line = current_line.checked_add(1)?;
                offset += 1;
                line_start = offset;
                prev_was_cr = true;
            } else if b == b'\n' {
                if current_line == target_line {
                    return Some((line_start, offset));
                }
                current_line = current_line.checked_add(1)?;
                offset += 1;
                line_start = offset;
            } else {
                offset += 1;
            }
        }
    }

    if current_line == target_line {
        Some((line_start, rope.len_bytes()))
    } else {
        None
    }
}

/// Parses a `textDocument/definition` response (`null`, `Location`, `LocationLink`, or array).
///
/// Preserves server order, deduplicates identical targets, and skips non-`file://` URIs.
pub fn parse_definition(
    value: Value,
    encoding: PositionEncoding,
) -> Result<Vec<DefinitionTarget>, LspError> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    match value {
        Value::Null => Ok(out),
        Value::Object(_) => {
            if let Some(target) = parse_definition_item(&value, encoding)? {
                out.push(target);
            }
            Ok(out)
        }
        Value::Array(items) => {
            for item in &items {
                if let Some(target) = parse_definition_item(item, encoding)? {
                    if seen.insert(target.clone()) {
                        out.push(target);
                    }
                }
            }
            Ok(out)
        }
        _ => Err(LspError::Protocol(
            "unexpected JSON type in textDocument/definition response".to_string(),
        )),
    }
}

fn parse_definition_item(
    item: &Value,
    encoding: PositionEncoding,
) -> Result<Option<DefinitionTarget>, LspError> {
    let obj = item
        .as_object()
        .ok_or_else(|| LspError::Protocol("definition entry must be a JSON object".to_string()))?;

    let (uri, pos_value) = if let Some(target_uri) = obj.get("targetUri") {
        // LocationLink
        let uri = target_uri.as_str().ok_or_else(|| {
            LspError::Protocol("LocationLink.targetUri must be a string".to_string())
        })?;
        let start = obj
            .get("targetSelectionRange")
            .and_then(|r| r.get("start"))
            .ok_or_else(|| {
                LspError::Protocol("LocationLink missing targetSelectionRange.start".to_string())
            })?;
        (uri, start)
    } else if let Some(uri_val) = obj.get("uri") {
        // Location
        let uri = uri_val
            .as_str()
            .ok_or_else(|| LspError::Protocol("Location.uri must be a string".to_string()))?;
        let start = obj
            .get("range")
            .and_then(|r| r.get("start"))
            .ok_or_else(|| LspError::Protocol("Location missing range.start".to_string()))?;
        (uri, start)
    } else {
        return Err(LspError::Protocol(
            "definition object missing both 'uri' and 'targetUri'".to_string(),
        ));
    };

    let position = parse_position(pos_value)?;
    let Some(path) = uri_to_path(uri) else {
        return Ok(None);
    };

    Ok(Some(DefinitionTarget {
        path,
        encoding,
        position,
    }))
}

fn parse_position(value: &Value) -> Result<Position, LspError> {
    let line = value
        .get("line")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| LspError::Protocol("invalid LSP position.line".to_string()))?;
    let character = value
        .get("character")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| LspError::Protocol("invalid LSP position.character".to_string()))?;
    Ok(Position { line, character })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn positions_round_trip_unicode_tab_crlf() {
        let text = RopeBuffer::from_str("é😀\tFoo\r\nbar");

        // 'é' = 2 UTF-8 bytes, 1 UTF-16 unit, 1 UTF-32 char
        // '😀' = 4 UTF-8 bytes, 2 UTF-16 units, 1 UTF-32 char
        // '\t' = 1 UTF-8 byte, 1 UTF-16 unit, 1 UTF-32 char
        // "Foo" starts at byte 7 on line 0
        let foo_utf8 = Position {
            line: 0,
            character: 7,
        };
        let foo_utf16 = Position {
            line: 0,
            character: 4,
        };
        let foo_utf32 = Position {
            line: 0,
            character: 3,
        };

        assert_eq!(
            position_at_byte(&text, 7, PositionEncoding::Utf8),
            Some(foo_utf8)
        );
        assert_eq!(
            byte_at_position(&text, foo_utf8, PositionEncoding::Utf8),
            Some(7)
        );

        assert_eq!(
            position_at_byte(&text, 7, PositionEncoding::Utf16),
            Some(foo_utf16)
        );
        assert_eq!(
            byte_at_position(&text, foo_utf16, PositionEncoding::Utf16),
            Some(7)
        );

        assert_eq!(
            position_at_byte(&text, 7, PositionEncoding::Utf32),
            Some(foo_utf32)
        );
        assert_eq!(
            byte_at_position(&text, foo_utf32, PositionEncoding::Utf32),
            Some(7)
        );

        // Second line ("bar") starts at byte 12 (after "é😀\tFoo\r\n")
        let line1_start = Position {
            line: 1,
            character: 0,
        };
        for enc in [
            PositionEncoding::Utf8,
            PositionEncoding::Utf16,
            PositionEncoding::Utf32,
        ] {
            assert_eq!(position_at_byte(&text, 12, enc), Some(line1_start));
            assert_eq!(byte_at_position(&text, line1_start, enc), Some(12));
        }

        // Byte 11 is between '\r' and '\n' -> rejected
        assert_eq!(position_at_byte(&text, 11, PositionEncoding::Utf16), None);

        // Middle of UTF-16 surrogate pair ('😀' occupies UTF-16 columns 1..3) is rejected
        let middle = Position {
            line: 0,
            character: 2,
        };
        assert_eq!(
            byte_at_position(&text, middle, PositionEncoding::Utf16),
            None
        );

        // Middle of UTF-8 multibyte character is also rejected
        assert_eq!(
            byte_at_position(
                &text,
                Position {
                    line: 0,
                    character: 1
                },
                PositionEncoding::Utf8
            ),
            None
        );

        // Column past end of line resolves to end of line content (byte 10 before \r\n, byte 15 at EOF)
        for enc in [
            PositionEncoding::Utf8,
            PositionEncoding::Utf16,
            PositionEncoding::Utf32,
        ] {
            assert_eq!(
                byte_at_position(
                    &text,
                    Position {
                        line: 0,
                        character: 500
                    },
                    enc
                ),
                Some(10)
            );
            assert_eq!(
                byte_at_position(
                    &text,
                    Position {
                        line: 1,
                        character: 500
                    },
                    enc
                ),
                Some(15)
            );
            // Out-of-range line returns None
            assert_eq!(
                byte_at_position(
                    &text,
                    Position {
                        line: 2,
                        character: 0
                    },
                    enc
                ),
                None
            );
        }

        // Bare CR line break is recognized per LSP 3.17
        let cr_text = RopeBuffer::from_str("abc\rdef");
        assert_eq!(
            position_at_byte(&cr_text, 4, PositionEncoding::Utf16),
            Some(Position {
                line: 1,
                character: 0
            })
        );
        assert_eq!(
            byte_at_position(
                &cr_text,
                Position {
                    line: 1,
                    character: 2
                },
                PositionEncoding::Utf16
            ),
            Some(6)
        );
    }

    #[test]
    fn definition_response_shapes() {
        // Null response -> empty targets
        let empty = parse_definition(Value::Null, PositionEncoding::Utf16).unwrap();
        assert!(empty.is_empty());

        // Local path with space, '#', and Unicode
        let special_path = PathBuf::from("/tmp/dir with space/c#proj/ação.h");
        let special_uri = path_to_uri(&special_path).unwrap();
        assert_eq!(uri_to_path(&special_uri), Some(special_path.clone()));

        // Single Location
        let single_loc = json!({
            "uri": special_uri,
            "range": {
                "start": { "line": 4, "character": 7 },
                "end": { "line": 4, "character": 11 }
            }
        });
        let parsed = parse_definition(single_loc.clone(), PositionEncoding::Utf16).unwrap();
        assert_eq!(
            parsed,
            vec![DefinitionTarget {
                path: special_path.clone(),
                encoding: PositionEncoding::Utf16,
                position: Position {
                    line: 4,
                    character: 7
                }
            }]
        );

        // Single LocationLink uses targetUri + targetSelectionRange.start
        let other_path = PathBuf::from("/tmp/include/arch.h");
        let other_uri = path_to_uri(&other_path).unwrap();
        let single_link = json!({
            "originSelectionRange": {
                "start": { "line": 1, "character": 0 },
                "end": { "line": 1, "character": 4 }
            },
            "targetUri": other_uri,
            "targetRange": {
                "start": { "line": 10, "character": 0 },
                "end": { "line": 15, "character": 1 }
            },
            "targetSelectionRange": {
                "start": { "line": 10, "character": 7 },
                "end": { "line": 10, "character": 11 }
            }
        });
        let parsed_link = parse_definition(single_link.clone(), PositionEncoding::Utf8).unwrap();
        assert_eq!(
            parsed_link,
            vec![DefinitionTarget {
                path: other_path.clone(),
                encoding: PositionEncoding::Utf8,
                position: Position {
                    line: 10,
                    character: 7
                }
            }]
        );

        // Array combining Location, LocationLink, duplicates, and virtual URI
        let array = json!([
            single_loc,
            {
                "uri": "untitled:virtual-doc",
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 1 }
                }
            },
            single_link,
            {
                "uri": special_uri,
                "range": {
                    "start": { "line": 4, "character": 7 },
                    "end": { "line": 4, "character": 20 }
                }
            }
        ]);
        let parsed_arr = parse_definition(array, PositionEncoding::Utf16).unwrap();
        assert_eq!(
            parsed_arr,
            vec![
                DefinitionTarget {
                    path: special_path,
                    encoding: PositionEncoding::Utf16,
                    position: Position {
                        line: 4,
                        character: 7
                    }
                },
                DefinitionTarget {
                    path: other_path,
                    encoding: PositionEncoding::Utf16,
                    position: Position {
                        line: 10,
                        character: 7
                    }
                }
            ]
        );

        // Virtual-only response yields empty list, not error
        let virtual_only = json!({
            "uri": "clangd://builtin/macros",
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 0 }
            }
        });
        assert!(parse_definition(virtual_only, PositionEncoding::Utf16)
            .unwrap()
            .is_empty());

        // Malformed top-level JSON returns Protocol error
        assert!(matches!(
            parse_definition(json!(42), PositionEncoding::Utf16),
            Err(LspError::Protocol(_))
        ));
    }

    #[test]
    fn framing_round_trip_and_limits() {
        let msg = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"});
        let mut wire = Vec::new();
        write_message(&mut wire, &msg).unwrap();

        let mut cursor = Cursor::new(wire);
        assert_eq!(read_message(&mut cursor).unwrap(), Some(msg));
        assert_eq!(read_message(&mut cursor).unwrap(), None);

        // Oversized Content-Length (> 64 MiB) is rejected before allocation
        let huge = format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE_BYTES + 1);
        let mut huge_cursor = Cursor::new(huge.into_bytes());
        let err = read_message(&mut huge_cursor).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
