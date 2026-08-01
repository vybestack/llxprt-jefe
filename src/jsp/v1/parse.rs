//! JSP/1 snapshot parser entry point (issue #476, J1 slice).
//!
//! [`parse_snapshot`] is the single public entry point. It performs no I/O and
//! no logging. It enforces the document byte bound, deserializes the closed
//! wire envelope (which rejects unknown/duplicate fields and wrong types),
//! then delegates to [`validate`] for typed conversion. A partially validated
//! payload never escapes: the wire DTO converts to a [`Snapshot`] only after
//! complete validation.
//!
//! Diagnostics carry stable code/path/location and never echo producer payload
//! values.

use super::contract::Snapshot;
use super::error::JspError;
use super::limits::{ACCEPTED_SCHEMA, MAX_DOCUMENT_BYTES, SNAPSHOT_KIND};
use super::validate;
use super::wire::SnapshotWire;

#[cfg(test)]
use super::error::JspCode;

/// Parse JSP/1 snapshot bytes into a validated typed [`Snapshot`].
///
/// # Errors
///
/// - `JSP-E001` for malformed JSON, trailing data, unknown/duplicate fields,
///   wrong types, or any closed-shape violation.
/// - `JSP-E002` for exceeded inclusive bounds.
/// - `JSP-E003` for unsupported schema or kind.
/// - `JSP-E004` for identity/binding invariants.
/// - `JSP-E005` for illegal field-state combinations.
/// - `JSP-E006` for snapshot semantic invariants.
///
/// The parser performs no I/O and no logging. Diagnostics never echo producer
/// payload values.
pub fn parse_snapshot(input: &[u8]) -> Result<Snapshot, JspError> {
    check_document_bound(input)?;
    expect_kind(input, SNAPSHOT_KIND)?;
    let wire: SnapshotWire = deserialize_closed(input)?;
    validate::convert(wire)
}

/// Envelope discriminators read before full-document deserialization.
///
/// Unknown fields are ignored here on purpose: this probe answers only "is
/// this a JSP/1 snapshot document?" so an unsupported version or kind reports
/// `JSP-E003` instead of being masked by the closed envelope's `JSP-E001`
/// missing-field failure. [`validate`] re-checks both values after the closed
/// deserialization, so this probe never widens what is accepted.
#[derive(serde::Deserialize)]
struct EnvelopeProbe {
    #[serde(default)]
    schema: Option<u64>,
    #[serde(default)]
    kind: Option<String>,
}

/// Pre-deserialization schema/kind gate shared by every document kind.
///
/// The probe reads only the two discriminators so a version or kind mismatch
/// reports `JSP-E003` instead of being masked by the closed envelope's
/// `JSP-E001` missing-field failure.
pub(super) fn expect_kind(input: &[u8], expected: &str) -> Result<(), JspError> {
    let Ok(probe) = serde_json::from_slice::<EnvelopeProbe>(input) else {
        // Malformed or non-object input is reported by the closed
        // deserialization below with a precise location.
        return Ok(());
    };
    if let Some(schema) = probe.schema
        && schema != ACCEPTED_SCHEMA
    {
        return Err(JspError::unsupported_version(format!(
            "document.schema: unsupported schema version (accepted: {ACCEPTED_SCHEMA})"
        )));
    }
    if let Some(kind) = probe.kind
        && kind != expected
    {
        return Err(JspError::unsupported_version(format!(
            "document.kind: unsupported kind (accepted: {expected})"
        )));
    }
    Ok(())
}

/// Enforce the inclusive document byte bound before any parsing.
pub(super) fn check_document_bound(input: &[u8]) -> Result<(), JspError> {
    if input.len() > MAX_DOCUMENT_BYTES {
        return Err(JspError::bound(format!(
            "snapshot: document size {} exceeds maximum {} bytes",
            input.len(),
            MAX_DOCUMENT_BYTES
        )));
    }
    Ok(())
}

/// Deserialize the closed wire envelope from bytes, mapping any serde failure
/// to a `JSP-E001` closed-shape error with a safe (payload-free) detail.
///
/// `serde_json` is configured via the wire DTOs' `#[serde(deny_unknown_fields)]`
/// and exhaustive enums to reject unknown fields, duplicate fields, wrong
/// types, trailing data, and non-integer numbers at this boundary.
pub(super) fn deserialize_closed<T: serde::de::DeserializeOwned>(
    input: &[u8],
) -> Result<T, JspError> {
    let mut deserializer = serde_json::Deserializer::from_slice(input);
    let result = T::deserialize(&mut deserializer);
    // Reject trailing data after the top-level value.
    let trailing = deserializer.end().map_err(|_| trailing_data_error());
    match (result, trailing) {
        (Ok(value), Ok(())) => Ok(value),
        (Ok(_), Err(trailing_err)) => Err(trailing_err),
        (Err(parse_err), _) => Err(serde_error_to_jsp(&parse_err)),
    }
}

/// Map a serde_json error to a closed-shape `JSP-E001` error.
///
/// The error category determines the message, but no producer payload text is
/// ever echoed — only the structural reason and (where safe) the line/column.
fn serde_error_to_jsp(error: &serde_json::Error) -> JspError {
    let category = error.classify();
    let location = format_location(error);
    match category {
        serde_json::error::Category::Io | serde_json::error::Category::Eof => {
            JspError::closed_shape(format!("snapshot: unexpected end of input {location}"))
        }
        serde_json::error::Category::Syntax => {
            JspError::closed_shape(format!("snapshot: malformed JSON {location}"))
        }
        serde_json::error::Category::Data => {
            // Data errors cover unknown fields, wrong types, missing fields.
            // Use the serde line/column but never the payload value.
            JspError::closed_shape(format!(
                "snapshot: input does not match the closed contract {location}"
            ))
        }
    }
}

/// Format the safe line/column location from a serde error.
fn format_location(error: &serde_json::Error) -> String {
    let line = error.line();
    let col = error.column();
    if line == 0 {
        String::new()
    } else {
        format!("at line {line} column {col}")
    }
}

/// Trailing data error (always closed-shape).
fn trailing_data_error() -> JspError {
    JspError::closed_shape("snapshot: trailing data after top-level JSON value")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_is_closed_shape_error() {
        let error = parse_snapshot(b"")
            .err()
            .unwrap_or_else(|| panic!("empty input fails"));
        assert_eq!(error.code(), JspCode::EClosedShape);
    }

    #[test]
    fn non_utf8_is_closed_shape_error() {
        let error = parse_snapshot(&[0xFF, 0xFE])
            .err()
            .unwrap_or_else(|| panic!("non-utf8 fails"));
        assert_eq!(error.code(), JspCode::EClosedShape);
    }

    #[test]
    fn truncated_json_is_closed_shape_error() {
        let error = parse_snapshot(b"{\"schema\":")
            .err()
            .unwrap_or_else(|| panic!("truncated fails"));
        assert_eq!(error.code(), JspCode::EClosedShape);
    }

    #[test]
    fn over_limit_document_is_bound_error() {
        let input = vec![b' '; MAX_DOCUMENT_BYTES + 1];
        let error = parse_snapshot(&input)
            .err()
            .unwrap_or_else(|| panic!("oversized doc fails"));
        assert_eq!(
            error.code(),
            JspCode::EBound,
            "the byte bound must fire before any parsing"
        );
    }

    #[test]
    fn at_limit_document_passes_the_byte_bound() {
        let input = vec![b' '; MAX_DOCUMENT_BYTES];
        let error = parse_snapshot(&input)
            .err()
            .unwrap_or_else(|| panic!("whitespace-only doc still fails to parse"));
        assert_eq!(
            error.code(),
            JspCode::EClosedShape,
            "an at-limit document must pass the byte bound and fail on shape"
        );
    }
}
