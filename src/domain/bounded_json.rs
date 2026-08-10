//! Bounded strict JSON reader shared by every closed contract in this crate.
//!
//! Jefe's closed schemas — the agent definition (issue #382) and the plugin
//! manifest (issue #389) — need the same reader: one that rejects duplicate
//! object keys, enforces inclusive depth/member/element/string bounds, admits
//! no JSON extension, and returns an ordered tree so unknown or duplicate
//! fields can be reported before any typed mapping runs.
//!
//! The two schemas differ only in their bounds and in whether they admit
//! fractional numbers, so both arrive as [`BoundedJsonLimits`] rather than
//! forking the parser. A second JSON reader would be a parallel architecture
//! variant that could disagree with this one about duplicate keys, surrogate
//! pairs, or control characters — exactly the parser-divergence class of bug
//! this contract exists to prevent.
//!
//! Numbers are deliberately restrictive. Under
//! [`NumberPolicy::IntegerOnly`] only decimal integers are admitted. Under
//! [`NumberPolicy::Finite`] a non-integer must additionally be canonical
//! [`CanonicalDecimal`] text, which has no exponent form and no trailing
//! fraction zeroes, and which is finite by construction — so NaN, infinity,
//! and any literal large enough to round to infinity are all unrepresentable
//! rather than checked for after the fact.

use std::fmt;

use super::CanonicalDecimal;

/// Whether a schema admits fractional numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberPolicy {
    /// Only decimal integers; a fraction or exponent is an error.
    IntegerOnly,
    /// Decimal integers plus canonical finite decimals.
    Finite,
}

/// Inclusive bounds one closed schema places on its JSON documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundedJsonLimits {
    /// Maximum document size in bytes.
    pub document_bytes: usize,
    /// Maximum nesting depth of arrays and objects.
    pub depth: usize,
    /// Maximum members in one object.
    pub object_members: usize,
    /// Maximum elements in one array.
    pub array_elements: usize,
    /// Maximum bytes in one string value or key.
    pub string_bytes: usize,
    /// Which numbers the schema admits.
    pub numbers: NumberPolicy,
}

/// Ordered JSON value tree produced by the bounded reader.
///
/// Objects preserve source order and have already rejected duplicate keys at
/// parse time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundedJson {
    /// JSON null.
    Null,
    /// JSON boolean.
    Bool(bool),
    /// Decimal integer.
    Int(i64),
    /// Canonical finite decimal, only under [`NumberPolicy::Finite`].
    Number(CanonicalDecimal),
    /// UTF-8 string bounded by [`BoundedJsonLimits::string_bytes`].
    Str(String),
    /// Ordered array bounded by [`BoundedJsonLimits::array_elements`].
    Array(Vec<Self>),
    /// Ordered object with no duplicate keys.
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
            Self::Str(value) => Some(value),
            _ => None,
        }
    }

    /// Borrow the integer if this is an integer.
    #[must_use]
    pub const fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(value) => Some(*value),
            _ => None,
        }
    }

    /// Borrow the canonical decimal if this is a non-integer number.
    #[must_use]
    pub const fn as_decimal(&self) -> Option<&CanonicalDecimal> {
        match self {
            Self::Number(value) => Some(value),
            _ => None,
        }
    }

    /// Borrow the boolean if this is a boolean.
    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// Whether this is JSON null.
    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

/// Why a document failed the bounded reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundedJsonError {
    /// The document exceeds the schema's byte bound.
    DocumentTooLarge { bytes: usize, limit: usize },
    /// The document is not valid UTF-8.
    NotUtf8,
    /// A duplicate object key was declared.
    DuplicateKey { key: String },
    /// Nesting exceeds the schema's depth bound.
    DepthExceeded { limit: usize },
    /// An object exceeds the schema's member bound.
    ObjectTooLarge { limit: usize },
    /// An array exceeds the schema's element bound.
    ArrayTooLarge { limit: usize },
    /// A string exceeds the schema's byte bound.
    StringTooLong { limit: usize },
    /// A number is outside what the schema's [`NumberPolicy`] admits.
    NumberNotAdmitted { text: String },
    /// Content follows the top-level value.
    TrailingData { offset: usize },
    /// Any other syntax failure, with the byte offset that failed.
    Syntax { message: String, offset: usize },
}

impl fmt::Display for BoundedJsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DocumentTooLarge { bytes, limit } => {
                write!(
                    formatter,
                    "document is {bytes} bytes, over the {limit} limit"
                )
            }
            Self::NotUtf8 => formatter.write_str("document is not valid UTF-8"),
            Self::DuplicateKey { key } => write!(formatter, "duplicate object key {key:?}"),
            Self::DepthExceeded { limit } => {
                write!(formatter, "nesting depth exceeds {limit}")
            }
            Self::ObjectTooLarge { limit } => write!(formatter, "object exceeds {limit} members"),
            Self::ArrayTooLarge { limit } => write!(formatter, "array exceeds {limit} elements"),
            Self::StringTooLong { limit } => write!(formatter, "string exceeds {limit} bytes"),
            Self::NumberNotAdmitted { text } => {
                write!(formatter, "number {text:?} is not admitted by this schema")
            }
            Self::TrailingData { offset } => write!(formatter, "trailing data at byte {offset}"),
            Self::Syntax { message, offset } => write!(formatter, "{message} at byte {offset}"),
        }
    }
}

impl std::error::Error for BoundedJsonError {}

/// Parse a complete JSON document with the schema's bounds enforced.
///
/// # Errors
///
/// Returns [`BoundedJsonError`] for syntax, duplicate-key, non-UTF-8,
/// inadmissible-number, or exceeded-bound failures.
pub fn parse(input: &[u8], limits: &BoundedJsonLimits) -> Result<BoundedJson, BoundedJsonError> {
    parse_tracking_top_member(input, limits, None).map(|(value, _)| value)
}

/// Parse a document and retain the exact source-byte length of one top-level
/// object member's value.
///
/// The measurement comes from the same bounded parser that validates the
/// document, so whitespace cannot be lost to canonical reserialization and no
/// second JSON authority is introduced.
///
/// # Errors
///
/// Returns the same failures as [`parse`]. A missing member is represented by
/// `None` because closed-schema mapping remains the caller's responsibility.
pub fn parse_with_top_member_bytes(
    input: &[u8],
    limits: &BoundedJsonLimits,
    member: &str,
) -> Result<(BoundedJson, Option<usize>), BoundedJsonError> {
    parse_tracking_top_member(input, limits, Some(member))
}

fn parse_tracking_top_member(
    input: &[u8],
    limits: &BoundedJsonLimits,
    tracked_member: Option<&str>,
) -> Result<(BoundedJson, Option<usize>), BoundedJsonError> {
    if input.len() > limits.document_bytes {
        return Err(BoundedJsonError::DocumentTooLarge {
            bytes: input.len(),
            limit: limits.document_bytes,
        });
    }
    let text = std::str::from_utf8(input).map_err(|_| BoundedJsonError::NotUtf8)?;
    let mut parser = Parser {
        bytes: text.as_bytes(),
        pos: 0,
        limits,
        tracked_member: tracked_member.map(str::to_owned),
        tracked_member_bytes: None,
    };
    parser.skip_ws();
    let value = parser.parse_value(0)?;
    parser.skip_ws();
    if parser.pos != parser.bytes.len() {
        return Err(BoundedJsonError::TrailingData { offset: parser.pos });
    }
    Ok((value, parser.tracked_member_bytes))
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    limits: &'a BoundedJsonLimits,
    tracked_member: Option<String>,
    tracked_member_bytes: Option<usize>,
}

impl Parser<'_> {
    fn syntax(&self, message: impl Into<String>) -> BoundedJsonError {
        BoundedJsonError::Syntax {
            message: message.into(),
            offset: self.pos,
        }
    }

    fn skip_ws(&mut self) {
        while let Some(&byte) = self.bytes.get(self.pos) {
            if matches!(byte, b' ' | b'\t' | b'\n' | b'\r') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn expect_byte(&mut self, expected: u8) -> Result<(), BoundedJsonError> {
        if self.peek() == Some(expected) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.syntax(format!("expected '{}'", char::from(expected))))
        }
    }

    fn enter(&self, depth: usize) -> Result<(), BoundedJsonError> {
        if depth >= self.limits.depth {
            return Err(BoundedJsonError::DepthExceeded {
                limit: self.limits.depth,
            });
        }
        Ok(())
    }

    fn parse_value(&mut self, depth: usize) -> Result<BoundedJson, BoundedJsonError> {
        match self.peek() {
            Some(b'{') => {
                self.enter(depth)?;
                self.parse_object(depth)
            }
            Some(b'[') => {
                self.enter(depth)?;
                self.parse_array(depth)
            }
            Some(b'"') => self.parse_string().map(BoundedJson::Str),
            Some(b't' | b'f') => self.parse_bool(),
            Some(b'n') => self.parse_null(),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            _ => Err(self.syntax("expected a JSON value")),
        }
    }

    fn parse_object(&mut self, depth: usize) -> Result<BoundedJson, BoundedJsonError> {
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
                return Err(BoundedJsonError::DuplicateKey { key });
            }
            self.skip_ws();
            self.expect_byte(b':')?;
            self.skip_ws();
            let value_start = self.pos;
            let value = self.parse_value(depth + 1)?;
            if depth == 0 && self.tracked_member.as_deref() == Some(key.as_str()) {
                self.tracked_member_bytes = Some(self.pos.saturating_sub(value_start));
            }
            members.push((key, value));
            if members.len() > self.limits.object_members {
                return Err(BoundedJsonError::ObjectTooLarge {
                    limit: self.limits.object_members,
                });
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(BoundedJson::Object(members));
                }
                _ => return Err(self.syntax("expected ',' or '}'")),
            }
        }
    }

    fn parse_array(&mut self, depth: usize) -> Result<BoundedJson, BoundedJsonError> {
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
            if elements.len() > self.limits.array_elements {
                return Err(BoundedJsonError::ArrayTooLarge {
                    limit: self.limits.array_elements,
                });
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(BoundedJson::Array(elements));
                }
                _ => return Err(self.syntax("expected ',' or ']'")),
            }
        }
    }

    fn parse_bool(&mut self) -> Result<BoundedJson, BoundedJsonError> {
        if self.bytes[self.pos..].starts_with(b"true") {
            self.pos += 4;
            Ok(BoundedJson::Bool(true))
        } else if self.bytes[self.pos..].starts_with(b"false") {
            self.pos += 5;
            Ok(BoundedJson::Bool(false))
        } else {
            Err(self.syntax("expected 'true' or 'false'"))
        }
    }

    fn parse_null(&mut self) -> Result<BoundedJson, BoundedJsonError> {
        if self.bytes[self.pos..].starts_with(b"null") {
            self.pos += 4;
            Ok(BoundedJson::Null)
        } else {
            Err(self.syntax("expected 'null'"))
        }
    }

    /// Read a number, admitting only what the schema's policy allows.
    fn parse_number(&mut self) -> Result<BoundedJson, BoundedJsonError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        let digits_start = self.pos;
        self.skip_digits();
        if self.pos == digits_start {
            return Err(self.syntax("expected digits"));
        }
        if self.bytes[digits_start] == b'0' && self.pos - digits_start > 1 {
            return Err(self.syntax("leading zeros are not allowed"));
        }
        let fractional = self.read_fraction()?;
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| self.syntax("invalid number"))?
            .to_owned();
        if fractional {
            return CanonicalDecimal::parse(&text)
                .map(BoundedJson::Number)
                .map_err(|_| BoundedJsonError::NumberNotAdmitted { text });
        }
        text.parse::<i64>()
            .map(BoundedJson::Int)
            .map_err(|_| BoundedJsonError::NumberNotAdmitted { text })
    }

    fn skip_digits(&mut self) {
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
    }

    /// Consume a fractional part, reporting whether one was present.
    ///
    /// An exponent is never admitted: canonical decimal text has no exponent
    /// form, so admitting one would create two spellings of one value.
    fn read_fraction(&mut self) -> Result<bool, BoundedJsonError> {
        if matches!(self.peek(), Some(b'e' | b'E')) {
            return Err(self.syntax("exponent notation is not allowed"));
        }
        if self.peek() != Some(b'.') {
            return Ok(false);
        }
        if self.limits.numbers == NumberPolicy::IntegerOnly {
            return Err(self.syntax("only decimal integers are allowed"));
        }
        self.pos += 1;
        let fraction_start = self.pos;
        self.skip_digits();
        if self.pos == fraction_start {
            return Err(self.syntax("expected digits after '.'"));
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            return Err(self.syntax("exponent notation is not allowed"));
        }
        Ok(true)
    }

    fn check_string_len(&self, out: &str) -> Result<(), BoundedJsonError> {
        if out.len() > self.limits.string_bytes {
            return Err(BoundedJsonError::StringTooLong {
                limit: self.limits.string_bytes,
            });
        }
        Ok(())
    }

    fn parse_string(&mut self) -> Result<String, BoundedJsonError> {
        self.expect_byte(b'"')?;
        let mut out = String::new();
        loop {
            let byte = self
                .peek()
                .ok_or_else(|| self.syntax("unterminated string"))?;
            match byte {
                b'"' => {
                    self.pos += 1;
                    self.check_string_len(&out)?;
                    return Ok(out);
                }
                b'\\' => {
                    self.pos += 1;
                    self.parse_escape(&mut out)?;
                }
                0x00..=0x1F => {
                    return Err(self.syntax("unescaped control character in string"));
                }
                _ => self.push_utf8(byte, &mut out)?,
            }
            self.check_string_len(&out)?;
        }
    }

    fn push_utf8(&mut self, first: u8, out: &mut String) -> Result<(), BoundedJsonError> {
        let end = self.pos + utf8_len(first);
        let chunk = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| self.syntax("truncated UTF-8 sequence"))?;
        let piece =
            std::str::from_utf8(chunk).map_err(|_| self.syntax("invalid UTF-8 in string"))?;
        out.push_str(piece);
        self.pos = end;
        Ok(())
    }

    fn parse_escape(&mut self, out: &mut String) -> Result<(), BoundedJsonError> {
        let byte = self
            .peek()
            .ok_or_else(|| self.syntax("unterminated escape"))?;
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
                let character = self.parse_unicode_escape()?;
                out.push(character);
            }
            _ => return Err(self.syntax("invalid escape character")),
        }
        Ok(())
    }

    fn parse_unicode_escape(&mut self) -> Result<char, BoundedJsonError> {
        let high = self.parse_hex4()?;
        if (0xD800..=0xDBFF).contains(&high) {
            if self.bytes.get(self.pos..self.pos + 2) != Some(b"\\u") {
                return Err(self.syntax("unpaired surrogate escape"));
            }
            self.pos += 2;
            let low = self.parse_hex4()?;
            if !(0xDC00..=0xDFFF).contains(&low) {
                return Err(self.syntax("invalid low surrogate"));
            }
            let combined = 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00);
            return char::from_u32(combined).ok_or_else(|| self.syntax("invalid surrogate pair"));
        }
        if (0xDC00..=0xDFFF).contains(&high) {
            return Err(self.syntax("unpaired low surrogate"));
        }
        char::from_u32(high).ok_or_else(|| self.syntax("invalid unicode escape"))
    }

    fn parse_hex4(&mut self) -> Result<u32, BoundedJsonError> {
        let chunk = self
            .bytes
            .get(self.pos..self.pos + 4)
            .ok_or_else(|| self.syntax("truncated unicode escape"))?;
        let text = std::str::from_utf8(chunk).map_err(|_| self.syntax("invalid unicode escape"))?;
        let value =
            u32::from_str_radix(text, 16).map_err(|_| self.syntax("invalid unicode escape"))?;
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
