//! Bounded strict JSON reader for the schema-1 contract (issue #380).
//!
//! This parser exists because the closed contract demands properties serde
//! cannot provide: duplicate-key rejection on every object, inclusive depth /
//! member / element / string-size bounds enforced during parsing, and
//! integer-only decimal numbers. It produces an ordered [`JsonValue`] tree;
//! field-level shape checks live in the typed parse layer.

use super::error::HarnessError;
use super::limits::{
    MAX_ARRAY_ELEMENTS, MAX_BYTES, MAX_DEPTH, MAX_OBJECT_MEMBERS, MAX_STRING_BYTES,
};

/// A parsed JSON value. Objects preserve source order; duplicate keys have
/// already been rejected. Numbers are decimal integers only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Array(Vec<Self>),
    Object(Vec<(String, Self)>),
}

impl JsonValue {
    /// Object members, if this is an object.
    #[must_use]
    pub fn as_object(&self) -> Option<&[(String, Self)]> {
        match self {
            Self::Object(members) => Some(members),
            _ => None,
        }
    }
}

/// Parse a complete JSON document with all structural bounds enforced.
///
/// # Errors
///
/// `HAR-E001` for syntax, duplicate keys, non-integer numbers, or non-UTF-8
/// input; `HAR-E002` for any exceeded structural bound.
pub fn parse_json(input: &[u8]) -> Result<JsonValue, HarnessError> {
    if input.len() > MAX_BYTES {
        return Err(HarnessError::limit(format!(
            "input is {} bytes (max {MAX_BYTES})",
            input.len()
        )));
    }
    let text =
        std::str::from_utf8(input).map_err(|_| HarnessError::syntax("input is not valid UTF-8"))?;
    let mut parser = Parser {
        bytes: text.as_bytes(),
        pos: 0,
    };
    parser.skip_ws();
    let value = parser.parse_value(0)?;
    parser.skip_ws();
    if parser.pos != parser.bytes.len() {
        return Err(HarnessError::syntax(format!(
            "trailing data at byte {}",
            parser.pos
        )));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
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

    fn expect_byte(&mut self, expected: u8) -> Result<(), HarnessError> {
        if self.peek() == Some(expected) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.syntax_at(&format!("expected '{}'", char::from(expected))))
        }
    }

    fn syntax_at(&self, what: &str) -> HarnessError {
        HarnessError::syntax(format!("{what} at byte {}", self.pos))
    }

    /// `depth` counts containers already open; entering container number
    /// `MAX_DEPTH + 1` is a limit violation, scalars never are.
    fn parse_value(&mut self, depth: usize) -> Result<JsonValue, HarnessError> {
        match self.peek() {
            Some(b'{') => check_depth(depth).and_then(|()| self.parse_object(depth)),
            Some(b'[') => check_depth(depth).and_then(|()| self.parse_array(depth)),
            Some(b'"') => self.parse_string().map(JsonValue::Str),
            Some(b't' | b'f') => self.parse_bool(),
            Some(b'n') => self.parse_null(),
            Some(b'-' | b'0'..=b'9') => self.parse_int(),
            _ => Err(self.syntax_at("expected a JSON value")),
        }
    }

    fn parse_object(&mut self, depth: usize) -> Result<JsonValue, HarnessError> {
        self.expect_byte(b'{')?;
        let mut members: Vec<(String, JsonValue)> = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(members));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            if members.iter().any(|(existing, _)| existing == &key) {
                return Err(HarnessError::syntax(format!("duplicate key '{key}'")));
            }
            self.skip_ws();
            self.expect_byte(b':')?;
            self.skip_ws();
            let value = self.parse_value(depth + 1)?;
            members.push((key, value));
            if members.len() > MAX_OBJECT_MEMBERS {
                return Err(HarnessError::limit(format!(
                    "object exceeds {MAX_OBJECT_MEMBERS} members"
                )));
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(JsonValue::Object(members));
                }
                _ => return Err(self.syntax_at("expected ',' or '}'")),
            }
        }
    }

    fn parse_array(&mut self, depth: usize) -> Result<JsonValue, HarnessError> {
        self.expect_byte(b'[')?;
        let mut elements = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::Array(elements));
        }
        loop {
            self.skip_ws();
            elements.push(self.parse_value(depth + 1)?);
            if elements.len() > MAX_ARRAY_ELEMENTS {
                return Err(HarnessError::limit(format!(
                    "array exceeds {MAX_ARRAY_ELEMENTS} elements"
                )));
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(JsonValue::Array(elements));
                }
                _ => return Err(self.syntax_at("expected ',' or ']'")),
            }
        }
    }

    fn parse_bool(&mut self) -> Result<JsonValue, HarnessError> {
        if self.bytes[self.pos..].starts_with(b"true") {
            self.pos += 4;
            Ok(JsonValue::Bool(true))
        } else if self.bytes[self.pos..].starts_with(b"false") {
            self.pos += 5;
            Ok(JsonValue::Bool(false))
        } else {
            Err(self.syntax_at("expected 'true' or 'false'"))
        }
    }

    fn parse_null(&mut self) -> Result<JsonValue, HarnessError> {
        if self.bytes[self.pos..].starts_with(b"null") {
            self.pos += 4;
            Ok(JsonValue::Null)
        } else {
            Err(self.syntax_at("expected 'null'"))
        }
    }

    /// Parse a decimal integer. Fractions, exponents, and leading zeros are
    /// rejected because the contract admits only decimal JSON integers.
    fn parse_int(&mut self) -> Result<JsonValue, HarnessError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        let digits_start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if self.pos == digits_start {
            return Err(self.syntax_at("expected digits"));
        }
        if self.bytes[digits_start] == b'0' && self.pos - digits_start > 1 {
            return Err(self.syntax_at("leading zeros are not allowed"));
        }
        if matches!(self.peek(), Some(b'.' | b'e' | b'E')) {
            return Err(self.syntax_at("only decimal integers are allowed"));
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| self.syntax_at("invalid number"))?;
        let value = text
            .parse::<i64>()
            .map_err(|_| HarnessError::syntax(format!("integer '{text}' is out of range")))?;
        Ok(JsonValue::Int(value))
    }

    fn parse_string(&mut self) -> Result<String, HarnessError> {
        self.expect_byte(b'"')?;
        let mut out = String::new();
        loop {
            let byte = self
                .peek()
                .ok_or_else(|| self.syntax_at("unterminated string"))?;
            match byte {
                b'"' => {
                    self.pos += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.pos += 1;
                    self.parse_escape(&mut out)?;
                }
                0x00..=0x1F => {
                    return Err(self.syntax_at("unescaped control character in string"));
                }
                _ => {
                    let len = utf8_len(byte);
                    let end = self.pos + len;
                    let chunk = self
                        .bytes
                        .get(self.pos..end)
                        .ok_or_else(|| self.syntax_at("truncated UTF-8 sequence"))?;
                    // Input was validated as UTF-8 up front, so this cannot
                    // fail; keep the checked conversion anyway.
                    let piece = std::str::from_utf8(chunk)
                        .map_err(|_| self.syntax_at("invalid UTF-8 in string"))?;
                    out.push_str(piece);
                    self.pos = end;
                }
            }
            if out.len() > MAX_STRING_BYTES {
                return Err(HarnessError::limit(format!(
                    "string exceeds {MAX_STRING_BYTES} bytes"
                )));
            }
        }
    }

    fn parse_escape(&mut self, out: &mut String) -> Result<(), HarnessError> {
        let byte = self
            .peek()
            .ok_or_else(|| self.syntax_at("unterminated escape"))?;
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
            _ => return Err(self.syntax_at("invalid escape character")),
        }
        Ok(())
    }

    fn parse_unicode_escape(&mut self) -> Result<char, HarnessError> {
        let high = self.parse_hex4()?;
        if (0xD800..=0xDBFF).contains(&high) {
            if self.bytes.get(self.pos..self.pos + 2) != Some(b"\\u") {
                return Err(self.syntax_at("unpaired surrogate escape"));
            }
            self.pos += 2;
            let low = self.parse_hex4()?;
            if !(0xDC00..=0xDFFF).contains(&low) {
                return Err(self.syntax_at("invalid low surrogate"));
            }
            let combined = 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00);
            return char::from_u32(combined)
                .ok_or_else(|| self.syntax_at("invalid surrogate pair"));
        }
        if (0xDC00..=0xDFFF).contains(&high) {
            return Err(self.syntax_at("unpaired low surrogate"));
        }
        char::from_u32(high).ok_or_else(|| self.syntax_at("invalid unicode escape"))
    }

    fn parse_hex4(&mut self) -> Result<u32, HarnessError> {
        let chunk = self
            .bytes
            .get(self.pos..self.pos + 4)
            .ok_or_else(|| self.syntax_at("truncated unicode escape"))?;
        let text =
            std::str::from_utf8(chunk).map_err(|_| self.syntax_at("invalid unicode escape"))?;
        let value =
            u32::from_str_radix(text, 16).map_err(|_| self.syntax_at("invalid unicode escape"))?;
        self.pos += 4;
        Ok(value)
    }
}

fn check_depth(depth: usize) -> Result<(), HarnessError> {
    if depth >= MAX_DEPTH {
        return Err(HarnessError::limit(format!(
            "nesting depth exceeds {MAX_DEPTH}"
        )));
    }
    Ok(())
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
#[path = "json_tests.rs"]
mod json_tests;
