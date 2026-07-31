//! Private strict-harness action capture protocol (issue #383 S8, D9).
//!
//! The strict schema-1 harness runs the application in a separate process, so
//! it cannot observe canonical resolution or mouse hit identity from a frame
//! alone. This module owns the harness-only artifact protocol that closes that
//! gap. It is deliberately crate-private: no public runtime API and no
//! alternate input path is introduced (D9/D13).
//!
//! One record keeps four independently observable values for a keyboard input:
//!
//! * `original` — the exact platform event (key code plus raw modifier bits),
//! * `canonical_chord` — the canonical chord text the registry resolved,
//! * `resolution` — the resolution class the registry produced,
//! * `pty_bytes` — the exact bytes written to the PTY.
//!
//! `pty_bytes` is never derived from `canonical_chord`: it carries the literal
//! encoder output, so a forwarded key proves byte-exact passthrough rather than
//! a re-encoding of the chord's display text.

use serde::{Deserialize, Serialize};

use super::error::HarnessError;

/// Environment variable naming the capture artifact path. Only the contained
/// schema-1 runner sets it; the application appends records when it is present.
pub const CAPTURE_PATH_ENV: &str = "JEFE_HARNESS_ACTION_CAPTURE";

/// Workspace-relative name of the capture artifact. It lives inside the
/// contained workspace so it is removed with it.
pub const CAPTURE_ARTIFACT: &str = "action-capture.jsonl";

/// The resolution class recorded for one input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResolutionClass {
    Dispatch,
    Unavailable,
    ForwardToPty,
    Unbound,
}

/// The exact original platform event, before canonicalization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginalKeyEvent {
    /// Debug rendering of the platform key code (e.g. `Char(c)`, `F(12)`).
    pub code: String,
    /// Raw platform modifier bits, preserved exactly as received.
    pub modifiers: u8,
}

/// One captured keyboard input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyCapture {
    pub original: OriginalKeyEvent,
    pub canonical_chord: String,
    pub resolution: ResolutionClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler: Option<String>,
    /// Exact bytes written to the PTY; empty when the input did not forward.
    #[serde(default)]
    pub pty_bytes: Vec<u8>,
}

/// One captured mouse activation: frame, cell, hit surface, and action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MouseCapture {
    /// Monotonic frame counter at activation time.
    pub frame: u64,
    pub column: u16,
    pub row: u16,
    /// Stable identity of the hit surface (e.g. `keys.row`, `confirm.button`).
    pub hit: String,
    /// The `ActionId` the hit surface contributed.
    pub action: String,
    pub resolution: ResolutionClass,
}

/// One line of the capture artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ActionCaptureRecord {
    Key(KeyCapture),
    Mouse(MouseCapture),
}

/// Encode one record as a single newline-terminated JSON line.
///
/// # Errors
///
/// `HAR-E005` when the record cannot be serialized.
pub fn encode_record(record: &ActionCaptureRecord) -> Result<String, HarnessError> {
    let encoded = serde_json::to_string(record)
        .map_err(|err| HarnessError::process(format!("encode action capture: {err}")))?;
    Ok(format!("{encoded}\n"))
}

/// Decode every record from an artifact body, ignoring blank lines.
///
/// # Errors
///
/// `HAR-E001` for a malformed record line.
pub fn decode_records(body: &str) -> Result<Vec<ActionCaptureRecord>, HarnessError> {
    let mut records = Vec::new();
    for (index, line) in body.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record = serde_json::from_str(line).map_err(|err| {
            HarnessError::syntax(format!("action capture line {}: {err}", index + 1))
        })?;
        records.push(record);
    }
    Ok(records)
}
