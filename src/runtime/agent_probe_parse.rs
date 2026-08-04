//! Framing and bounded parsing for captured agent-probe evidence.

use crate::domain::agent_definition::bounded_json::{BoundedJson, parse_definition_json};
use crate::domain::agent_definition::json_pointer::JsonPointer;
use crate::domain::agent_definition::limits::STRING_VALUE_BYTE_LIMIT;
use crate::domain::agent_definition::normalize::{Normalize, strip_ansi_escape};
use crate::domain::agent_definition::probe::{
    AnchoredPattern, IdentityRecognizer, ProbeFraming, ProbeSpec, parse_text_identity,
};

/// Runtime parse failure mapped to AGT-E202 by the adapter.
pub(super) enum ProbeEvidenceError {
    Bounds(String),
    InvalidUtf8,
    MalformedFraming,
    IdentityMismatch,
}

/// Parse one selected identity stream according to its closed framing.
pub(super) fn parse_identity(bytes: &[u8], spec: &ProbeSpec) -> Result<String, ProbeEvidenceError> {
    let text = bounded_text(bytes, spec.max_bytes)?;
    validate_line_bounds(text)?;
    let normalized = normalize_identity(text, spec);
    match spec.framing {
        ProbeFraming::Utf8Text => parse_text(&normalized, &spec.identity),
        ProbeFraming::SingleJson => parse_single_json(normalized.as_bytes(), &spec.identity),
        ProbeFraming::JsonLines => parse_json_lines(&normalized, &spec.identity),
    }
}

fn bounded_text(bytes: &[u8], max_bytes: usize) -> Result<&str, ProbeEvidenceError> {
    if bytes.len() > max_bytes {
        return Err(ProbeEvidenceError::Bounds(format!(
            "probe stream is {} bytes; maximum is {max_bytes}",
            bytes.len()
        )));
    }
    std::str::from_utf8(bytes).map_err(|_| ProbeEvidenceError::InvalidUtf8)
}

fn validate_line_bounds(text: &str) -> Result<(), ProbeEvidenceError> {
    if text
        .split('\n')
        .any(|line| line.strip_suffix('\r').unwrap_or(line).len() > STRING_VALUE_BYTE_LIMIT)
    {
        return Err(ProbeEvidenceError::Bounds(format!(
            "probe line exceeds {STRING_VALUE_BYTE_LIMIT} bytes"
        )));
    }
    Ok(())
}

fn normalize_identity(text: &str, spec: &ProbeSpec) -> String {
    match spec.normalize {
        Normalize::None => text.to_string(),
        Normalize::StripAnsi => strip_ansi_escape(text.as_bytes()),
    }
}

fn parse_text(text: &str, recognizer: &IdentityRecognizer) -> Result<String, ProbeEvidenceError> {
    if !matches!(recognizer, IdentityRecognizer::Line { .. }) {
        return Err(ProbeEvidenceError::MalformedFraming);
    }
    parse_text_identity(text, recognizer).ok_or(ProbeEvidenceError::IdentityMismatch)
}

fn parse_single_json(
    bytes: &[u8],
    recognizer: &IdentityRecognizer,
) -> Result<String, ProbeEvidenceError> {
    let value = parse_definition_json(bytes).map_err(|_| ProbeEvidenceError::MalformedFraming)?;
    recognize_json(&value, recognizer)?.ok_or(ProbeEvidenceError::IdentityMismatch)
}

fn parse_json_lines(
    text: &str,
    recognizer: &IdentityRecognizer,
) -> Result<String, ProbeEvidenceError> {
    if !matches!(recognizer, IdentityRecognizer::JsonPointer { .. }) || text.is_empty() {
        return Err(ProbeEvidenceError::MalformedFraming);
    }
    let mut matched = None;
    for line in text.lines() {
        if line.trim().is_empty() {
            return Err(ProbeEvidenceError::MalformedFraming);
        }
        let value = parse_definition_json(line.as_bytes())
            .map_err(|_| ProbeEvidenceError::MalformedFraming)?;
        if matched.is_none() {
            matched = recognize_json(&value, recognizer)?;
        }
    }
    matched.ok_or(ProbeEvidenceError::IdentityMismatch)
}

fn recognize_json(
    value: &BoundedJson,
    recognizer: &IdentityRecognizer,
) -> Result<Option<String>, ProbeEvidenceError> {
    let IdentityRecognizer::JsonPointer {
        pointer,
        anchored_pattern,
    } = recognizer
    else {
        return Err(ProbeEvidenceError::MalformedFraming);
    };
    let pointer = JsonPointer::parse(pointer).map_err(|_| ProbeEvidenceError::MalformedFraming)?;
    let Some(candidate) = pointer.evaluate(value).and_then(BoundedJson::as_str) else {
        return Ok(None);
    };
    Ok(recognize_pattern(candidate, anchored_pattern))
}

fn recognize_pattern(candidate: &str, pattern: &AnchoredPattern) -> Option<String> {
    let trimmed = candidate.trim();
    if !trimmed.is_empty() && pattern.matches(trimmed) {
        Some(trimmed.to_string())
    } else {
        None
    }
}
