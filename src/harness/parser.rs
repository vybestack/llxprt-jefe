//! Scenario JSON parsing entry point.
//!
//! `parse_scenario` is the single public parse function. It deserializes JSON
//! into the typed [`Scenario`] model, then validates it. All serde failures
//! are converted into structured [`ScenarioError`] values.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P01
//! @requirement REQ-TMUX-HARNESS-001

use super::error::ScenarioError;
use super::scenario::Scenario;

/// Parse a JSON scenario document into a validated typed [`Scenario`].
///
/// # Errors
///
/// Returns [`ScenarioError::Json`] for malformed JSON or shape mismatches,
/// [`ScenarioError::UnknownStepKind`] for unrecognized step discriminators,
/// and [`ScenarioError::InvalidConfig`] / [`ScenarioError::MissingField`] for
/// validation violations.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
pub fn parse_scenario(json: &str) -> Result<Scenario, ScenarioError> {
    let scenario: Scenario = serde_json::from_str(json).map_err(serde_to_scenario_error)?;
    scenario.validate()?;
    Ok(scenario)
}

/// Convert a serde_json error into a context-rich [`ScenarioError`].
///
/// serde_json emits `missing field` and type errors as strings. The `Step`
/// deserializer surfaces its own typed errors through `de::Error::custom`,
/// whose Display prefixes (`unknown step kind: 'x'`, `invalid step: ...`)
/// are recognized here so callers still see the most specific variant.
/// Everything else falls back to `Json`.
fn serde_to_scenario_error(err: serde_json::Error) -> ScenarioError {
    let msg = err.to_string();
    if let Some(field) = extract_between(&msg, "missing field `", '`') {
        return ScenarioError::MissingField {
            field,
            context: "scenario".to_string(),
        };
    }
    if let Some(kind) = extract_between(&msg, "unknown step kind: '", '\'') {
        return ScenarioError::UnknownStepKind { kind };
    }
    if let Some((field, context)) = extract_scoped_missing_field(&msg) {
        return ScenarioError::MissingField { field, context };
    }
    if msg.starts_with("invalid step") {
        return ScenarioError::InvalidStep { reason: msg };
    }
    ScenarioError::Json { message: msg }
}

/// Recover a structured scoped missing-field error from Display text.
fn extract_scoped_missing_field(msg: &str) -> Option<(String, String)> {
    let field = extract_between(msg, "missing required field '", '\'')?;
    if msg.contains(" in step") {
        return Some((field, "step".to_string()));
    }
    if msg.contains(" in macro") {
        return Some((field, "macro".to_string()));
    }
    None
}

/// Extract the substring between `open_marker` and the next `close_char`
/// following the marker, if present.
fn extract_between(msg: &str, open_marker: &str, close_char: char) -> Option<String> {
    let start = msg.find(open_marker)? + open_marker.len();
    let rest = msg.get(start..)?;
    let end = rest.find(close_char)?;
    Some(rest.get(..end)?.to_string())
}
