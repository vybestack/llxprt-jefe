//! Bounded strict JSON reader for the closed definition contract (issue #382).
//!
//! This parser is a focused analog of `jefe::harness::v1::json`: it rejects
//! duplicate object keys, enforces inclusive depth/member/element/string bounds
//! from the issue's "Deterministic algorithms and limits" section, and admits
//! only decimal integers, booleans, null, strings, arrays, and ordered
//! objects. It returns an ordered [`BoundedJson`] tree that the typed
//! definition layer consumes field-by-field, so unknown/duplicate fields can be
//! reported as `AGT-E201` diagnostics before any serde mapping runs.
//!
//! The bounds (depth 16, map 256, array 1024, string 4096, artifact 1 MiB,
//! path 4096 bytes) are the exact limits the issue mandates.

use super::diagnostics::DefinitionError;
use super::limits::{ARRAY_LIMIT, DATA_DEPTH_LIMIT, MAP_LIMIT, STRING_VALUE_BYTE_LIMIT};

/// Ordered JSON value tree produced by the bounded reader.
///
/// Objects preserve source order and have already rejected duplicate keys at
/// parse time. Numbers are decimal integers only; fractions, exponents, and
/// leading zeros are rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundedJson {
    /// JSON null.
    Null,
    /// JSON boolean.
    Bool(bool),
    /// Decimal integer (the only numeric kind the closed contract admits).
    Int(i64),
    /// UTF-8 string bounded by [`STRING_VALUE_BYTE_LIMIT`].
    Str(String),
    /// Ordered array bounded by [`ARRAY_LIMIT`].
    Array(Vec<Self>),
    /// Ordered object with no duplicate keys, bounded by [`MAP_LIMIT`].
    Object(Vec<(String, Self)>),
}

impl BoundedJson {
    /// Borrow the object members if this is an object.
    #[must_use]
    pub fn as_object(&self) -> Option<&[(String, Self)]> {
        match self {
            Self::Object(members) => Some(members),
            _ => None,
        }
    }

    /// Borrow the array elements if this is an array.
    #[must_use]
    pub fn as_array(&self) -> Option<&[Self]> {
        match self {
            Self::Array(elements) => Some(elements),
            _ => None,
        }
    }

    /// Borrow the string if this is a string.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Borrow the integer if this is an integer.
    #[must_use]
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Borrow the boolean if this is a boolean.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Whether this is JSON null.
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

/// Parse a complete JSON document with all closed-contract bounds enforced.
///
/// # Errors
///
/// Returns a [`DefinitionError`] (always prefixed `AGT-E201`) for syntax,
/// duplicate-key, non-integer-number, non-UTF-8, or exceeded-bound failures.
pub fn parse_definition_json(input: &[u8]) -> Result<BoundedJson, DefinitionError> {
    if input.len() > super::limits::ARTIFACT_LIMIT {
        return Err(DefinitionError::UnknownField {
            field: format!("artifact exceeds {} bytes", super::limits::ARTIFACT_LIMIT),
        });
    }
    let text = std::str::from_utf8(input).map_err(|_| DefinitionError::UnknownField {
        field: "input is not valid UTF-8".to_string(),
    })?;
    let mut parser = Parser {
        bytes: text.as_bytes(),
        pos: 0,
    };
    parser.skip_ws();
    let value = parser.parse_value(0)?;
    parser.skip_ws();
    if parser.pos != parser.bytes.len() {
        return Err(DefinitionError::UnknownField {
            field: format!("trailing data at byte {}", parser.pos),
        });
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn fail(what: impl Into<String>) -> DefinitionError {
        DefinitionError::UnknownField { field: what.into() }
    }

    fn skip_ws(&mut self) {
        while let Some(&b) = self.bytes.get(self.pos) {
            if matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn expect_byte(&mut self, expected: u8) -> Result<(), DefinitionError> {
        if self.peek() == Some(expected) {
            self.pos += 1;
            Ok(())
        } else {
            Err(Self::fail(format!(
                "expected '{}' at byte {}",
                char::from(expected),
                self.pos
            )))
        }
    }

    fn parse_value(&mut self, depth: usize) -> Result<BoundedJson, DefinitionError> {
        match self.peek() {
            Some(b'{') => {
                if depth >= DATA_DEPTH_LIMIT {
                    return Err(Self::fail(format!(
                        "nesting depth exceeds {DATA_DEPTH_LIMIT}"
                    )));
                }
                self.parse_object(depth)
            }
            Some(b'[') => {
                if depth >= DATA_DEPTH_LIMIT {
                    return Err(Self::fail(format!(
                        "nesting depth exceeds {DATA_DEPTH_LIMIT}"
                    )));
                }
                self.parse_array(depth)
            }
            Some(b'"') => self.parse_string().map(BoundedJson::Str),
            Some(b't' | b'f') => self.parse_bool(),
            Some(b'n') => self.parse_null(),
            Some(b'-' | b'0'..=b'9') => self.parse_int(),
            _ => Err(Self::fail(format!(
                "expected a JSON value at byte {}",
                self.pos
            ))),
        }
    }

    fn parse_object(&mut self, depth: usize) -> Result<BoundedJson, DefinitionError> {
        self.expect_byte(b'{')?;
        let mut members: Vec<(String, BoundedJson)> = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(BoundedJson::Object(members));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            if members.iter().any(|(existing, _)| existing == &key) {
                return Err(DefinitionError::DuplicateJsonField { field: key });
            }
            self.skip_ws();
            self.expect_byte(b':')?;
            self.skip_ws();
            let value = self.parse_value(depth + 1)?;
            members.push((key, value));
            if members.len() > MAP_LIMIT {
                return Err(Self::fail(format!("object exceeds {MAP_LIMIT} members")));
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(BoundedJson::Object(members));
                }
                _ => {
                    return Err(Self::fail(format!(
                        "expected ',' or '}}' at byte {}",
                        self.pos
                    )));
                }
            }
        }
    }

    fn parse_array(&mut self, depth: usize) -> Result<BoundedJson, DefinitionError> {
        self.expect_byte(b'[')?;
        let mut elements = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(BoundedJson::Array(elements));
        }
        loop {
            self.skip_ws();
            elements.push(self.parse_value(depth + 1)?);
            if elements.len() > ARRAY_LIMIT {
                return Err(Self::fail(format!("array exceeds {ARRAY_LIMIT} elements")));
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(BoundedJson::Array(elements));
                }
                _ => {
                    return Err(Self::fail(format!(
                        "expected ',' or ']' at byte {}",
                        self.pos
                    )));
                }
            }
        }
    }

    fn parse_bool(&mut self) -> Result<BoundedJson, DefinitionError> {
        if self.bytes[self.pos..].starts_with(b"true") {
            self.pos += 4;
            Ok(BoundedJson::Bool(true))
        } else if self.bytes[self.pos..].starts_with(b"false") {
            self.pos += 5;
            Ok(BoundedJson::Bool(false))
        } else {
            Err(Self::fail("expected 'true' or 'false'"))
        }
    }

    fn parse_null(&mut self) -> Result<BoundedJson, DefinitionError> {
        if self.bytes[self.pos..].starts_with(b"null") {
            self.pos += 4;
            Ok(BoundedJson::Null)
        } else {
            Err(Self::fail("expected 'null'"))
        }
    }

    fn parse_int(&mut self) -> Result<BoundedJson, DefinitionError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        let digits_start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if self.pos == digits_start {
            return Err(Self::fail("expected digits"));
        }
        if self.bytes[digits_start] == b'0' && self.pos - digits_start > 1 {
            return Err(Self::fail("leading zeros are not allowed"));
        }
        if matches!(self.peek(), Some(b'.' | b'e' | b'E')) {
            return Err(Self::fail("only decimal integers are allowed"));
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| Self::fail("invalid number"))?;
        let value = text
            .parse::<i64>()
            .map_err(|_| Self::fail(format!("integer '{text}' is out of range")))?;
        Ok(BoundedJson::Int(value))
    }

    fn parse_string(&mut self) -> Result<String, DefinitionError> {
        self.expect_byte(b'"')?;
        let mut out = String::new();
        loop {
            let byte = self
                .peek()
                .ok_or_else(|| Self::fail("unterminated string"))?;
            match byte {
                b'"' => {
                    self.pos += 1;
                    if out.len() > STRING_VALUE_BYTE_LIMIT {
                        return Err(Self::fail(format!(
                            "string exceeds {STRING_VALUE_BYTE_LIMIT} bytes"
                        )));
                    }
                    return Ok(out);
                }
                b'\\' => {
                    self.pos += 1;
                    self.parse_escape(&mut out)?;
                }
                0x00..=0x1F => {
                    return Err(Self::fail("unescaped control character in string"));
                }
                _ => {
                    let len = utf8_len(byte);
                    let end = self.pos + len;
                    let chunk = self
                        .bytes
                        .get(self.pos..end)
                        .ok_or_else(|| Self::fail("truncated UTF-8 sequence"))?;
                    let piece = std::str::from_utf8(chunk)
                        .map_err(|_| Self::fail("invalid UTF-8 in string"))?;
                    out.push_str(piece);
                    self.pos = end;
                }
            }
            if out.len() > STRING_VALUE_BYTE_LIMIT {
                return Err(Self::fail(format!(
                    "string exceeds {STRING_VALUE_BYTE_LIMIT} bytes"
                )));
            }
        }
    }

    fn parse_escape(&mut self, out: &mut String) -> Result<(), DefinitionError> {
        let byte = self
            .peek()
            .ok_or_else(|| Self::fail("unterminated escape"))?;
        self.pos += 1;
        match byte {
            b'"' => out.push('"'),
            b'\\' => out.push('\\'),
            b'/' => out.push('/'),
            b'b' => out.push('\u{0008}'),
            b'f' => out.push('\u{000C}'),
            b'n' => out.push('\n'),
            b'r' => out.push('\r'),
            b't' => out.push('\t'),
            b'u' => {
                let ch = self.parse_unicode_escape()?;
                out.push(ch);
            }
            _ => return Err(Self::fail("invalid escape character")),
        }
        Ok(())
    }

    fn parse_unicode_escape(&mut self) -> Result<char, DefinitionError> {
        let high = self.parse_hex4()?;
        if (0xD800..=0xDBFF).contains(&high) {
            if self.bytes.get(self.pos..self.pos + 2) != Some(b"\\u") {
                return Err(Self::fail("unpaired surrogate escape"));
            }
            self.pos += 2;
            let low = self.parse_hex4()?;
            if !(0xDC00..=0xDFFF).contains(&low) {
                return Err(Self::fail("invalid low surrogate"));
            }
            let combined = 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00);
            return char::from_u32(combined).ok_or_else(|| Self::fail("invalid surrogate pair"));
        }
        if (0xDC00..=0xDFFF).contains(&high) {
            return Err(Self::fail("unpaired low surrogate"));
        }
        char::from_u32(high).ok_or_else(|| Self::fail("invalid unicode escape"))
    }

    fn parse_hex4(&mut self) -> Result<u32, DefinitionError> {
        let chunk = self
            .bytes
            .get(self.pos..self.pos + 4)
            .ok_or_else(|| Self::fail("truncated unicode escape"))?;
        let text = std::str::from_utf8(chunk).map_err(|_| Self::fail("invalid unicode escape"))?;
        let value =
            u32::from_str_radix(text, 16).map_err(|_| Self::fail("invalid unicode escape"))?;
        self.pos += 4;
        Ok(value)
    }
}

const fn utf8_len(first_byte: u8) -> usize {
    match first_byte {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

#[cfg(test)]
#[path = "bounded_json_tests.rs"]
mod tests;
