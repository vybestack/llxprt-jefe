//! RFC 6901 JSON-pointer validation and evaluation (issue #382 CW-02).
//!
//! The probe spec requires RFC 6901 pointers. This module validates pointer
//! syntax (empty string for the root, `/segment` segments, `~0`/`~1` escapes)
//! and evaluates a validated pointer against a [`super::bounded_json::BoundedJson`]
//! tree produced by the bounded reader. There is no regex dependency.

use super::bounded_json::BoundedJson;
use super::diagnostics::DefinitionError;
use super::limits::PATH_LIMIT;

/// A validated RFC 6901 JSON pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonPointer {
    raw: String,
    tokens: Vec<String>,
}

impl JsonPointer {
    /// Parse and validate an RFC 6901 pointer string.
    ///
    /// # Errors
    ///
    /// Returns [`DefinitionError`] when the input is not valid RFC 6901 syntax
    /// or exceeds the path-byte limit.
    pub fn parse(raw: &str) -> Result<Self, DefinitionError> {
        if raw.len() > PATH_LIMIT {
            return Err(DefinitionError::UnknownField {
                field: format!("JSON pointer exceeds {PATH_LIMIT} bytes"),
            });
        }
        let tokens = parse_tokens(raw)?;
        Ok(Self {
            raw: raw.to_owned(),
            tokens,
        })
    }

    /// Borrow the raw pointer string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Borrow the decoded reference tokens.
    #[must_use]
    pub fn tokens(&self) -> &[String] {
        &self.tokens
    }

    /// Evaluate this pointer against a JSON value, returning the referenced
    /// value if present.
    #[must_use]
    pub fn evaluate<'a>(&self, root: &'a BoundedJson) -> Option<&'a BoundedJson> {
        let mut current = root;
        for token in &self.tokens {
            current = match current {
                BoundedJson::Object(members) => members
                    .iter()
                    .find(|(key, _)| key == token)
                    .map(|(_, value)| value)?,
                BoundedJson::Array(elements) => {
                    let index: usize = token.parse().ok()?;
                    elements.get(index)?
                }
                _ => return None,
            };
        }
        Some(current)
    }
}

fn parse_tokens(raw: &str) -> Result<Vec<String>, DefinitionError> {
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    if !raw.starts_with('/') {
        return Err(DefinitionError::UnknownField {
            field: "JSON pointer must start with '/' or be empty".to_string(),
        });
    }
    let mut tokens = Vec::new();
    for segment in raw.split('/') {
        if segment.is_empty() {
            // The leading '/' produces one empty segment; subsequent empty
            // segments are valid only when the previous token resolved to an
            // array element with an empty-string key, which RFC 6901 forbids
            // for objects but allows syntactically. We accept empty segments
            // only at the leading position.
            if tokens.is_empty() {
                continue;
            }
            return Err(DefinitionError::UnknownField {
                field: "JSON pointer has an empty segment".to_string(),
            });
        }
        let decoded = decode_token(segment)?;
        tokens.push(decoded);
    }
    Ok(tokens)
}

fn decode_token(token: &str) -> Result<String, DefinitionError> {
    let mut out = String::new();
    let bytes = token.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'~' => {
                let next = bytes.get(i + 1).copied();
                match next {
                    Some(b'0') => {
                        out.push('~');
                        i += 2;
                    }
                    Some(b'1') => {
                        out.push('/');
                        i += 2;
                    }
                    _ => {
                        return Err(DefinitionError::UnknownField {
                            field: "JSON pointer has an invalid '~' escape".to_string(),
                        });
                    }
                }
            }
            b => {
                let len = utf8_len(b);
                let end = i + len;
                let chunk =
                    std::str::from_utf8(bytes.get(i..end).unwrap_or(&[])).map_err(|_| {
                        DefinitionError::UnknownField {
                            field: "invalid UTF-8 in JSON pointer".to_string(),
                        }
                    })?;
                out.push_str(chunk);
                i = end;
            }
        }
    }
    Ok(out)
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
#[path = "json_pointer_tests.rs"]
mod tests;
