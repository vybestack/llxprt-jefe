//! Scalar validators for the schema-1 contract (issue #380).
//!
//! Relative paths, env names, capture ids, strict base64 decoding, and the
//! secret rules all live here so the typed parser stays declarative.

use super::contract::RelPath;
use super::error::HarnessError;
use super::limits::{MAX_ENV_NAME_LEN, MAX_PATH_BYTES, MAX_SECRETS, MAX_STRING_BYTES};

/// Validate a workspace-relative path per the contract: UTF-8, 1..=4096
/// bytes, `/` separated, and no root/prefix, empty, `.`, `..`, NUL, or
/// backslash component.
///
/// # Errors
///
/// `HAR-E001` for structural violations, `HAR-E002` when over length.
pub fn validate_rel_path(field: &str, raw: &str) -> Result<RelPath, HarnessError> {
    if raw.is_empty() {
        return Err(HarnessError::syntax(format!("{field}: path is empty")));
    }
    if raw.len() > MAX_PATH_BYTES {
        return Err(HarnessError::limit(format!(
            "{field}: path exceeds {MAX_PATH_BYTES} bytes"
        )));
    }
    if raw.starts_with('/') {
        return Err(HarnessError::syntax(format!(
            "{field}: path must be relative"
        )));
    }
    if raw.contains('\u{0}') || raw.contains('\\') {
        return Err(HarnessError::syntax(format!(
            "{field}: path contains a NUL or backslash"
        )));
    }
    for component in raw.split('/') {
        if component.is_empty() {
            return Err(HarnessError::syntax(format!(
                "{field}: path has an empty component"
            )));
        }
        if component == "." || component == ".." {
            return Err(HarnessError::syntax(format!(
                "{field}: path component '{component}' is not allowed"
            )));
        }
    }
    Ok(RelPath::validated(raw.to_string()))
}

/// Validate an env name against `[A-Z_][A-Z0-9_]{0,127}`.
///
/// # Errors
///
/// `HAR-E001` on any violation.
pub fn validate_env_name(field: &str, name: &str) -> Result<(), HarnessError> {
    let bytes = name.as_bytes();
    let valid_first = bytes
        .first()
        .is_some_and(|b| b.is_ascii_uppercase() || *b == b'_');
    let valid_rest = bytes[1.min(bytes.len())..]
        .iter()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_');
    if !valid_first || !valid_rest || bytes.len() > 1 + MAX_ENV_NAME_LEN {
        return Err(HarnessError::syntax(format!(
            "{field}: env name '{name}' must match [A-Z_][A-Z0-9_]{{0,{MAX_ENV_NAME_LEN}}}"
        )));
    }
    Ok(())
}

/// Validate a capture id: 1..=64 bytes of `[A-Za-z0-9._-]`, not `.` or `..`.
/// Capture names become workspace file names, so the character set is closed.
///
/// # Errors
///
/// `HAR-E001` on any violation.
pub fn validate_id(field: &str, id: &str) -> Result<(), HarnessError> {
    let valid_chars = id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if id.is_empty() || id.len() > 64 || !valid_chars || id == "." || id == ".." {
        return Err(HarnessError::syntax(format!(
            "{field}: id '{id}' must be 1..=64 chars of [A-Za-z0-9._-] and not '.'/'..'"
        )));
    }
    Ok(())
}

/// Validate the secrets list: at most 64 entries, none empty.
///
/// # Errors
///
/// `HAR-E001` for an empty secret, `HAR-E002` when over count.
pub fn validate_secrets(secrets: &[String]) -> Result<(), HarnessError> {
    if secrets.len() > MAX_SECRETS {
        return Err(HarnessError::limit(format!(
            "secrets exceed {MAX_SECRETS} entries"
        )));
    }
    if secrets.iter().any(String::is_empty) {
        return Err(HarnessError::syntax("secrets must not be empty"));
    }
    Ok(())
}

/// Decode strict standard base64 (RFC 4648 with `=` padding, no whitespace).
///
/// # Errors
///
/// `HAR-E001` for malformed input, `HAR-E002` when the decoded size would
/// exceed the string bound.
pub fn decode_base64(field: &str, input: &str) -> Result<Vec<u8>, HarnessError> {
    if input.len() % 4 != 0 {
        return Err(malformed(field));
    }
    if input.len() / 4 * 3 > MAX_STRING_BYTES {
        return Err(HarnessError::limit(format!(
            "{field}: decoded base64 exceeds {MAX_STRING_BYTES} bytes"
        )));
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for (index, chunk) in bytes.chunks(4).enumerate() {
        let last = index + 1 == bytes.len() / 4;
        decode_chunk(field, chunk, last, &mut out)?;
    }
    Ok(out)
}

fn decode_chunk(
    field: &str,
    chunk: &[u8],
    last: bool,
    out: &mut Vec<u8>,
) -> Result<(), HarnessError> {
    let pad = match chunk {
        [_, _, b'=', b'='] => 2,
        [_, _, _, b'='] => 1,
        _ => 0,
    };
    if pad > 0 && !last {
        return Err(malformed(field));
    }
    if chunk[..4 - pad].contains(&b'=') {
        return Err(malformed(field));
    }
    let mut accum: u32 = 0;
    for &byte in &chunk[..4 - pad] {
        accum = (accum << 6) | u32::from(decode_symbol(field, byte)?);
    }
    accum <<= 6 * pad;
    // Reject non-canonical trailing bits (e.g. "QQ==" is valid, "QR==" not):
    // the bits below the produced bytes must all be zero.
    if pad > 0 && accum & ((1u32 << (8 * pad)) - 1) != 0 {
        return Err(malformed(field));
    }
    let produced = 3 - pad;
    let full = accum.to_be_bytes();
    out.extend_from_slice(&full[1..=produced]);
    Ok(())
}

fn decode_symbol(field: &str, byte: u8) -> Result<u8, HarnessError> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(malformed(field)),
    }
}

fn malformed(field: &str) -> HarnessError {
    HarnessError::syntax(format!("{field}: malformed base64"))
}

#[cfg(test)]
#[path = "validate_tests.rs"]
mod validate_tests;
