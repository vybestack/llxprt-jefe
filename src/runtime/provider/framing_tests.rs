//! Bounded line-framing boundary tables (issue #390 CW-10, CW10-06 framing).
//!
//! Every byte-level framing rule is exercised at its accept/reject edge. The
//! JSON well-formedness, duplicate-key, trailing-data, and non-UTF-8 rules are
//! delegated to the shared bounded reader, so these tests also prove that
//! delegation through the framing entry point.

use super::error::{FramingFault, ProviderError};
use super::framing::{MAX_LINE_BYTES, decode};
use crate::domain::bounded_json::BoundedJsonError;

fn line(body: &str) -> Vec<u8> {
    format!("{body}\n").into_bytes()
}

/// Decode a frame, panicking with the failure if it does not parse.
fn decoded(bytes: &[u8]) -> crate::domain::bounded_json::BoundedJson {
    decode(bytes).unwrap_or_else(|error| panic!("frame must decode: {error}"))
}

/// Decode a frame, panicking if it parses when it must be rejected.
fn rejected(bytes: &[u8]) -> ProviderError {
    decode(bytes)
        .err()
        .unwrap_or_else(|| panic!("frame must be rejected"))
}

#[test]
fn a_single_lf_terminated_json_object_decodes() {
    let value = decoded(b"{\"ok\":true}\n");
    assert!(value.as_object().is_some_and(|members| members.len() == 1));
}

#[test]
fn an_interior_line_feed_is_rejected_as_one_frame() {
    // A second physical line inside one decode input is not exactly one JSONL
    // line and must be rejected by framing itself (the whitespace-skipping JSON
    // reader would otherwise consume the boundary).
    let with_second_line = rejected(b"{\"ok\":true}\n{\"second\":true}\n");
    assert!(matches!(
        with_second_line,
        ProviderError::Framing(FramingFault::InteriorLineFeed)
    ));
    // A blank second line is equally rejected at the framing layer rather than
    // silently accepted as trailing whitespace.
    let with_blank_line = rejected(b"{\"ok\":true}\n\n");
    assert!(matches!(
        with_blank_line,
        ProviderError::Framing(FramingFault::InteriorLineFeed)
    ));
}

#[test]
fn a_line_without_a_terminator_is_rejected() {
    let error = rejected(b"{\"ok\":true}");
    assert!(matches!(
        error,
        ProviderError::Framing(FramingFault::MissingTerminator)
    ));
}

#[test]
fn a_carriage_return_is_rejected() {
    let error = rejected(b"{\"ok\":true}\r\n");
    assert!(matches!(
        error,
        ProviderError::Framing(FramingFault::CarriageReturn)
    ));
}

#[test]
fn a_byte_order_mark_is_rejected() {
    let mut frame = vec![0xEF, 0xBB, 0xBF];
    frame.extend_from_slice(b"{\"ok\":true}\n");
    let error = rejected(&frame);
    assert!(matches!(
        error,
        ProviderError::Framing(FramingFault::ByteOrderMark)
    ));
}

#[test]
fn a_blank_line_is_rejected() {
    assert!(matches!(
        rejected(b"\n"),
        ProviderError::Framing(FramingFault::BlankLine)
    ));
    assert!(matches!(
        rejected(b"   \t\n"),
        ProviderError::Framing(FramingFault::BlankLine)
    ));
}

#[test]
fn non_utf8_is_rejected() {
    let mut frame = b"{\"m\":\"".to_vec();
    frame.push(0xFF);
    frame.extend_from_slice(b"\"}\n");
    let error = rejected(&frame);
    assert!(matches!(
        error,
        ProviderError::Json(BoundedJsonError::NotUtf8)
    ));
}

#[test]
fn trailing_non_whitespace_is_rejected() {
    let error = rejected(b"{} junk\n");
    assert!(matches!(
        error,
        ProviderError::Json(BoundedJsonError::TrailingData { .. })
    ));
}

#[test]
fn a_duplicate_key_anywhere_is_rejected() {
    let error = rejected(b"{\"a\":1,\"a\":2}\n");
    assert!(matches!(
        error,
        ProviderError::Json(BoundedJsonError::DuplicateKey { .. })
    ));
    // Duplicate keys nested inside a child object are rejected too.
    let nested = line(r#"{"o":{"x":1,"x":2}}"#);
    let error = rejected(&nested);
    assert!(matches!(
        error,
        ProviderError::Json(BoundedJsonError::DuplicateKey { .. })
    ));
}

#[test]
fn the_line_bound_accepts_its_limit_and_rejects_one_more() {
    // A frame of exactly MAX_LINE_BYTES (including the terminator) is accepted.
    let padding = MAX_LINE_BYTES.saturating_sub(b"{\"m\":\"\"}\n".len());
    let mut at_limit = b"{\"m\":\"".to_vec();
    at_limit.extend(std::iter::repeat_n(b'a', padding));
    at_limit.extend_from_slice(b"\"}\n");
    assert_eq!(at_limit.len(), MAX_LINE_BYTES);
    let _ = decoded(&at_limit);

    // One byte over the limit is rejected before any JSON parsing.
    let mut over_limit = at_limit.clone();
    over_limit.insert(over_limit.len() - 1, b'a');
    assert_eq!(over_limit.len(), MAX_LINE_BYTES + 1);
    let error = rejected(&over_limit);
    assert!(matches!(
        error,
        ProviderError::Framing(FramingFault::Oversize { bytes, limit })
        if bytes == MAX_LINE_BYTES + 1 && limit == MAX_LINE_BYTES
    ));
}

#[test]
fn every_framing_failure_carries_the_protocol_code() {
    assert_eq!(rejected(b"").code(), "PLG-E502");
    assert_eq!(rejected(b"{} junk\n").code(), "PLG-E502");
}
