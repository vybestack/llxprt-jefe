//! Bounded UTF-8 JSONL line framing for the action-provider protocol
//! (issue #390 CW-10, Slice A).
//!
//! The protocol carries exactly one UTF-8 JSON object terminated by a single
//! line feed per frame. This module owns the byte-level framing rules — one LF
//! terminator, no carriage return, no byte-order mark, no blank line, and the
//! 1,048,576-byte line bound — and then delegates JSON well-formedness,
//! duplicate-key rejection at every nesting level, trailing-data rejection, and
//! bound checking to the shared bounded reader, so there is exactly one JSON
//! architecture in the crate. A second reader could disagree about duplicate
//! keys, surrogate pairs, or control characters, which is the bug class the
//! shared reader exists to prevent.
//!
//! Framing fails fast: byte-level faults and the size bound are checked before
//! the bounded reader sees the content, and every failure surfaces as the typed
//! [`ProviderError`] carrying `PLG-E502`.

use crate::domain::bounded_json::{self, BoundedJson, BoundedJsonLimits, NumberPolicy};

use super::error::{FramingFault, ProviderError};

/// The inclusive maximum length of one provider line, terminator included.
///
/// Matches the protocol contract: lines above this bound are a fatal
/// `PLG-E502`. The terminator is counted so a line and its bound are
/// unambiguous at the byte edge.
pub const MAX_LINE_BYTES: usize = 1_048_576;

/// The maximum nesting depth admitted on the wire.
const WIRE_DEPTH: usize = 64;

/// The maximum members one wire object may carry.
const WIRE_OBJECT_MEMBERS: usize = 1024;

/// The maximum elements one wire array may carry.
const WIRE_ARRAY_ELEMENTS: usize = 4096;

/// Bounded-reader limits for the provider protocol.
///
/// The envelope's numeric fields are all decimal integers (versions,
/// generations, sequences, counts), and a typed value carries any canonical
/// decimal as canonical text. The one place a JSON number with a fraction is
/// legitimate is a field-declaration scalar (a `FiniteNumber` bound or default
/// in a confirmation schema), so the reader admits finite decimals; every
/// integer field still enforces an integer literal through `as_int`, so a
/// fractional generation or sequence is rejected as a type mismatch.
const WIRE_LIMITS: BoundedJsonLimits = BoundedJsonLimits {
    document_bytes: MAX_LINE_BYTES,
    depth: WIRE_DEPTH,
    object_members: WIRE_OBJECT_MEMBERS,
    array_elements: WIRE_ARRAY_ELEMENTS,
    string_bytes: MAX_LINE_BYTES,
    numbers: NumberPolicy::Finite,
};

/// The UTF-8 byte-order mark, which a provider line may not begin with.
const BYTE_ORDER_MARK: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Decode one complete provider line.
///
/// `bytes` is the full frame including its terminating line feed. Framing is
/// validated first and fails fast; the remaining content is then handed to the
/// shared bounded reader for JSON well-formedness, duplicate-key rejection,
/// trailing-data rejection, and bound enforcement.
///
/// # Errors
///
/// Returns [`ProviderError`] (`PLG-E502`) for any framing or JSON fault.
pub fn decode(bytes: &[u8]) -> Result<BoundedJson, ProviderError> {
    validate_frame(bytes)?;
    // `validate_frame` guarantees a trailing line feed, so the content before
    // it is the one JSON object this frame carries.
    let content = &bytes[..bytes.len() - 1];
    bounded_json::parse(content, &WIRE_LIMITS).map_err(ProviderError::Json)
}

/// Enforce the byte-level framing rules before any JSON parsing runs.
fn validate_frame(bytes: &[u8]) -> Result<(), ProviderError> {
    if bytes.last() != Some(&b'\n') {
        return Err(ProviderError::Framing(FramingFault::MissingTerminator));
    }
    if bytes.len() > MAX_LINE_BYTES {
        return Err(ProviderError::Framing(FramingFault::Oversize {
            bytes: bytes.len(),
            limit: MAX_LINE_BYTES,
        }));
    }
    if bytes.contains(&b'\r') {
        return Err(ProviderError::Framing(FramingFault::CarriageReturn));
    }
    if bytes.starts_with(&BYTE_ORDER_MARK) {
        return Err(ProviderError::Framing(FramingFault::ByteOrderMark));
    }
    // The trailing line feed is the only line feed a single JSONL frame may
    // contain; any interior line feed means the input spans more than one
    // physical line, which the whitespace-skipping JSON reader would otherwise
    // accept (e.g. a trailing blank line) or reject as trailing data.
    let content = &bytes[..bytes.len() - 1];
    if content.contains(&b'\n') {
        return Err(ProviderError::Framing(FramingFault::InteriorLineFeed));
    }
    if content.is_empty() || content.iter().all(|byte| matches!(byte, b' ' | b'\t')) {
        return Err(ProviderError::Framing(FramingFault::BlankLine));
    }
    Ok(())
}
