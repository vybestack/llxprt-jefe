//! Lossless TOML syntax overlay for settings documents.

use crate::domain::ByteSpan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxNode {
    pub path: Vec<String>,
    pub key_span: ByteSpan,
    pub value_span: ByteSpan,
    pub statement_span: ByteSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableNode {
    pub path: Vec<String>,
    pub span: ByteSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxOverlay {
    pub nodes: Vec<SyntaxNode>,
    pub tables: Vec<TableNode>,
    pub comments: Vec<ByteSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub span: Option<ByteSpan>,
    pub detail: String,
}

pub fn scan(bytes: &[u8]) -> Result<SyntaxOverlay, SyntaxError> {
    let mut cursor = 0usize;
    let mut table = Vec::new();
    let mut overlay = SyntaxOverlay {
        nodes: Vec::new(),
        tables: Vec::new(),
        comments: Vec::new(),
    };
    while cursor < bytes.len() {
        cursor = skip_trivia(bytes, cursor, &mut overlay.comments);
        if cursor >= bytes.len() {
            break;
        }
        if bytes[cursor] == b'[' {
            let (path, end) = scan_table(bytes, cursor)?;
            overlay.tables.push(TableNode {
                path: path.clone(),
                span: span(cursor, end),
            });
            table = path;
            cursor = line_end(bytes, end);
        } else {
            let (node, end) = scan_assignment(bytes, cursor, &table, &mut overlay.comments)?;
            overlay.nodes.push(node);
            cursor = end;
        }
    }
    Ok(overlay)
}

fn skip_trivia(bytes: &[u8], mut cursor: usize, comments: &mut Vec<ByteSpan>) -> usize {
    while cursor < bytes.len() {
        match bytes[cursor] {
            b' ' | b'\t' | b'\r' | b'\n' => cursor += 1,
            b'#' => {
                let end = comment_end(bytes, cursor);
                comments.push(span(cursor, end));
                cursor = end;
            }
            _ => break,
        }
    }
    cursor
}

fn scan_table(bytes: &[u8], start: usize) -> Result<(Vec<String>, usize), SyntaxError> {
    let array = bytes.get(start + 1) == Some(&b'[');
    let open = if array { 2 } else { 1 };
    let close = if array {
        b"]]".as_slice()
    } else {
        b"]".as_slice()
    };
    let mut cursor = start + open;
    let mut quote = None;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        match quote {
            Some(mark) if byte == mark => quote = None,
            Some(b'"') if byte == b'\\' => cursor = cursor.saturating_add(1),
            None if matches!(byte, b'"' | b'\'') => quote = Some(byte),
            None if bytes[cursor..].starts_with(close) => {
                let expression = trim_ascii(&bytes[start + open..cursor]);
                let path = decode_table_key(expression)?;
                return Ok((path, cursor + close.len()));
            }
            None if byte == b'\n' => break,
            Some(_) | None => {}
        }
        cursor += 1;
    }
    Err(syntax_error(start, cursor, "unterminated table header"))
}

fn scan_assignment(
    bytes: &[u8],
    start: usize,
    table: &[String],
    comments: &mut Vec<ByteSpan>,
) -> Result<(SyntaxNode, usize), SyntaxError> {
    let equals = find_equals(bytes, start)?;
    let key_bytes = trim_ascii(&bytes[start..equals]);
    let mut path = table.to_vec();
    path.extend(decode_assignment_key(key_bytes)?);
    let value_start = skip_horizontal(bytes, equals + 1);
    let scanned = scan_value(bytes, value_start, comments)?;
    let value_end = trim_end_ascii(bytes, value_start, scanned.value_end);
    if value_start >= value_end {
        return Err(syntax_error(
            value_start,
            scanned.next,
            "assignment has no value",
        ));
    }
    Ok((
        SyntaxNode {
            path,
            key_span: span(start, equals),
            value_span: span(value_start, value_end),
            statement_span: span(start, scanned.next),
        },
        scanned.next,
    ))
}

fn find_equals(bytes: &[u8], start: usize) -> Result<usize, SyntaxError> {
    let mut cursor = start;
    let mut quote = None;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        match quote {
            Some(mark) if byte == mark => quote = None,
            Some(b'"') if byte == b'\\' => cursor = cursor.saturating_add(1),
            None if matches!(byte, b'"' | b'\'') => quote = Some(byte),
            None if byte == b'=' => return Ok(cursor),
            None if byte == b'\n' => break,
            Some(_) | None => {}
        }
        cursor += 1;
    }
    Err(syntax_error(start, cursor, "assignment is missing '='"))
}

struct ScannedValue {
    value_end: usize,
    next: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StringMode {
    Basic,
    Literal,
    MultiBasic,
    MultiLiteral,
}

fn scan_value(
    bytes: &[u8],
    start: usize,
    comments: &mut Vec<ByteSpan>,
) -> Result<ScannedValue, SyntaxError> {
    let mut cursor = start;
    let mut square = 0usize;
    let mut curly = 0usize;
    let mut mode = None;
    while cursor < bytes.len() {
        if let Some(active) = mode {
            cursor = scan_string_byte(bytes, cursor, active, &mut mode);
            continue;
        }
        match bytes[cursor] {
            b'"' => enter_string(bytes, &mut cursor, &mut mode, StringMode::Basic),
            b'\'' => enter_string(bytes, &mut cursor, &mut mode, StringMode::Literal),
            b'[' => {
                square += 1;
                cursor += 1;
            }
            b']' if square > 0 => {
                square -= 1;
                cursor += 1;
            }
            b'{' => {
                curly += 1;
                cursor += 1;
            }
            b'}' if curly > 0 => {
                curly -= 1;
                cursor += 1;
            }
            b'#' => {
                let end = comment_end(bytes, cursor);
                comments.push(span(cursor, end));
                if square == 0 && curly == 0 {
                    return Ok(ScannedValue {
                        value_end: cursor,
                        next: line_end(bytes, end),
                    });
                }
                cursor = end;
            }
            b'\n' if square == 0 && curly == 0 => {
                return Ok(ScannedValue {
                    value_end: cursor,
                    next: cursor + 1,
                });
            }
            _ => cursor += 1,
        }
    }
    if mode.is_some() || square != 0 || curly != 0 {
        return Err(syntax_error(start, cursor, "unterminated TOML value"));
    }
    Ok(ScannedValue {
        value_end: cursor,
        next: cursor,
    })
}

fn enter_string(
    bytes: &[u8],
    cursor: &mut usize,
    mode: &mut Option<StringMode>,
    single: StringMode,
) {
    let quote = bytes[*cursor];
    if bytes.get(*cursor..*cursor + 3) == Some([quote, quote, quote].as_slice()) {
        *mode = Some(match single {
            StringMode::Basic => StringMode::MultiBasic,
            _ => StringMode::MultiLiteral,
        });
        *cursor += 3;
    } else {
        *mode = Some(single);
        *cursor += 1;
    }
}

fn scan_string_byte(
    bytes: &[u8],
    cursor: usize,
    mode: StringMode,
    active: &mut Option<StringMode>,
) -> usize {
    match mode {
        StringMode::Basic if bytes[cursor] == b'\\' => (cursor + 2).min(bytes.len()),
        StringMode::Basic if bytes[cursor] == b'"' => {
            *active = None;
            cursor + 1
        }
        StringMode::Literal if bytes[cursor] == b'\'' => {
            *active = None;
            cursor + 1
        }
        StringMode::MultiBasic if bytes.get(cursor..cursor + 3) == Some(b"\"\"\"".as_slice()) => {
            *active = None;
            cursor + 3
        }
        StringMode::MultiBasic if bytes[cursor] == b'\\' => (cursor + 2).min(bytes.len()),
        StringMode::MultiLiteral if bytes.get(cursor..cursor + 3) == Some(b"'''".as_slice()) => {
            *active = None;
            cursor + 3
        }
        _ => cursor + 1,
    }
}

fn decode_assignment_key(bytes: &[u8]) -> Result<Vec<String>, SyntaxError> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| syntax_error(0, bytes.len(), "key is not UTF-8"))?;
    let source = format!("{text} = \"__jefe_marker__\"");
    let value: toml::Value = source
        .parse()
        .map_err(|_| syntax_error(0, bytes.len(), "invalid assignment key"))?;
    marker_path(&value, "__jefe_marker__")
        .ok_or_else(|| syntax_error(0, bytes.len(), "invalid key path"))
}

fn decode_table_key(bytes: &[u8]) -> Result<Vec<String>, SyntaxError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| syntax_error(0, bytes.len(), "table key is not UTF-8"))?;
    let source = format!("[{text}]\n__jefe_marker__ = true");
    let value: toml::Value = source
        .parse()
        .map_err(|_| syntax_error(0, bytes.len(), "invalid table key"))?;
    marker_table_path(&value).ok_or_else(|| syntax_error(0, bytes.len(), "invalid table path"))
}

fn marker_path(value: &toml::Value, marker: &str) -> Option<Vec<String>> {
    let table = value.as_table()?;
    for (key, value) in table {
        if value.as_str() == Some(marker) {
            return Some(vec![key.clone()]);
        }
        if let Some(mut nested) = marker_path(value, marker) {
            nested.insert(0, key.clone());
            return Some(nested);
        }
    }
    None
}

fn marker_table_path(value: &toml::Value) -> Option<Vec<String>> {
    let table = value.as_table()?;
    for (key, value) in table {
        if value.as_table().is_some_and(|nested| {
            nested.get("__jefe_marker__").and_then(toml::Value::as_bool) == Some(true)
        }) {
            return Some(vec![key.clone()]);
        }
        if let Some(mut nested) = marker_table_path(value) {
            nested.insert(0, key.clone());
            return Some(nested);
        }
    }
    None
}

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn trim_end_ascii(bytes: &[u8], start: usize, mut end: usize) -> usize {
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    end
}

fn skip_horizontal(bytes: &[u8], mut cursor: usize) -> usize {
    while bytes
        .get(cursor)
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        cursor += 1;
    }
    cursor
}

fn comment_end(bytes: &[u8], mut cursor: usize) -> usize {
    while bytes.get(cursor).is_some_and(|byte| *byte != b'\n') {
        cursor += 1;
    }
    cursor
}

fn line_end(bytes: &[u8], mut cursor: usize) -> usize {
    while bytes.get(cursor).is_some_and(|byte| *byte != b'\n') {
        cursor += 1;
    }
    if bytes.get(cursor) == Some(&b'\n') {
        cursor += 1;
    }
    cursor
}

fn span(start: usize, end: usize) -> ByteSpan {
    ByteSpan::new(start as u64, end as u64)
}

fn syntax_error(start: usize, end: usize, detail: &str) -> SyntaxError {
    SyntaxError {
        span: Some(span(start, end)),
        detail: detail.to_owned(),
    }
}
