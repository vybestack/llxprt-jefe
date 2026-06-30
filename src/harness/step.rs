//! Step primitives for the harness scenario language.
//!
//! Each step is a single JSON object with a discriminator key naming the
//! primitive (e.g. `{"wait": 100}`), deserialized into the fully typed
//! [`Step`] enum. No `serde_json::Value` escapes into the domain model.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P01
//! @requirement REQ-TMUX-HARNESS-001

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::de::{self, MapAccess, Visitor};

use super::error::ScenarioError;

/// A single concrete step primitive.
///
/// One variant per scenario primitive plus a `Macro` invocation variant that
/// macro expansion replaces with concrete steps.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Pause for `milliseconds` before the next step.
    Wait { milliseconds: u64 },
    /// Type a full line of text (implies a trailing Enter in later phases).
    Line { text: String },
    /// Send a single named key (e.g. `"Enter"`, `"Escape"`).
    Key { key: String },
    /// Send a sequence of named keys.
    Keys { keys: Vec<String> },
    /// Block until `pattern` appears on screen.
    WaitFor { pattern: String },
    /// Block until `pattern` no longer appears on screen.
    WaitForNot { pattern: String },
    /// Assert that `pattern` is currently on screen.
    Expect { pattern: String },
    /// Assert that `pattern` appears exactly `count` times.
    ExpectCount { pattern: String, count: u32 },
    /// Capture the current screen under `name`.
    Capture { name: String },
    /// Sample the scrollback history under `name`.
    HistorySample { name: String },
    /// Assert that history changed since the prior sample `name`.
    ExpectHistoryDelta { name: String },
    /// Enter (`true`) or exit (`false`) tmux copy mode.
    CopyMode { enabled: bool },
    /// Block until the target process exits, up to `timeout_ms`.
    WaitForExit { timeout_ms: u64 },
    /// Invoke macro `name` with `args`; expanded away before execution.
    Macro {
        name: String,
        args: BTreeMap<String, String>,
    },
}

impl<'de> Deserialize<'de> for Step {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_map(StepVisitor)
    }
}

/// serde visitor that maps a single-key JSON object to a [`Step`] variant.
///
/// The object must contain exactly one discriminator key for single-field
/// steps. `expectCount` additionally requires `count`, and `macro` requires
/// `args`.
struct StepVisitor;

impl<'de> Visitor<'de> for StepVisitor {
    type Value = Step;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a step object with a single kind key")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        // Collect into a typed map keyed by field name. serde_json::Value is
        // used only transiently here as a parse buffer; it never reaches the
        // domain model (the returned Step is fully typed).
        let mut entries: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        while let Some(key) = map.next_key::<String>()? {
            let value = map.next_value::<serde_json::Value>()?;
            if entries.insert(key.clone(), value).is_some() {
                return Err(de::Error::custom(format!(
                    "invalid step: duplicate field '{key}' in step"
                )));
            }
        }
        dispatch_step(&mut entries).map_err(de::Error::custom)
    }
}

/// Dispatch a parsed step-object map to the matching [`Step`] variant.
///
/// Takes the map by mutable reference so single-field variants can remove
/// their discriminator and verify that no unexpected keys remain.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
fn dispatch_step(entries: &mut BTreeMap<String, serde_json::Value>) -> Result<Step, ScenarioError> {
    // The discriminator is whichever key names a known primitive or macro.
    let kind = step_kind(entries)?;
    match kind.as_str() {
        "wait" => single_u64(entries, "wait").map(|v| Step::Wait { milliseconds: v }),
        "line" => single_string(entries, "line").map(|v| Step::Line { text: v }),
        "key" => single_string(entries, "key").map(|v| Step::Key { key: v }),
        "keys" => single_string_vec(entries, "keys").map(|v| Step::Keys { keys: v }),
        "waitFor" => single_string(entries, "waitFor").map(|v| Step::WaitFor { pattern: v }),
        "waitForNot" => {
            single_string(entries, "waitForNot").map(|v| Step::WaitForNot { pattern: v })
        }
        "expect" => single_string(entries, "expect").map(|v| Step::Expect { pattern: v }),
        "expectCount" => build_expect_count(entries),
        "capture" => single_string(entries, "capture").map(|v| Step::Capture { name: v }),
        "historySample" => {
            single_string(entries, "historySample").map(|v| Step::HistorySample { name: v })
        }
        "expectHistoryDelta" => single_string(entries, "expectHistoryDelta")
            .map(|v| Step::ExpectHistoryDelta { name: v }),
        "copyMode" => single_bool(entries, "copyMode").map(|v| Step::CopyMode { enabled: v }),
        "waitForExit" => {
            single_u64(entries, "waitForExit").map(|v| Step::WaitForExit { timeout_ms: v })
        }
        "macro" => build_macro(entries),
        other => Err(ScenarioError::UnknownStepKind {
            kind: other.to_string(),
        }),
    }
}

/// Determine the discriminator key, rejecting multi-kind or empty step objects.
fn step_kind(entries: &BTreeMap<String, serde_json::Value>) -> Result<String, ScenarioError> {
    let known = [
        "wait",
        "line",
        "key",
        "keys",
        "waitFor",
        "waitForNot",
        "expect",
        "expectCount",
        "capture",
        "historySample",
        "expectHistoryDelta",
        "copyMode",
        "waitForExit",
        "macro",
    ];
    let matches: Vec<&str> = known
        .iter()
        .copied()
        .filter(|k| entries.contains_key(*k))
        .collect();
    match matches.as_slice() {
        [] => unknown_or_empty(entries),
        [only] => Ok((*only).to_string()),
        _ => Err(ScenarioError::InvalidStep {
            reason: format!("step has multiple kind keys: {matches:?}"),
        }),
    }
}

/// Produce the error for a step object with no known kind key.
fn unknown_or_empty(
    entries: &BTreeMap<String, serde_json::Value>,
) -> Result<String, ScenarioError> {
    if let Some(key) = entries.keys().next() {
        Err(ScenarioError::UnknownStepKind { kind: key.clone() })
    } else {
        Err(ScenarioError::InvalidStep {
            reason: "step object is empty".to_string(),
        })
    }
}

/// Extract and remove a single u64 field, rejecting any extra keys.
fn single_u64(
    entries: &mut BTreeMap<String, serde_json::Value>,
    field: &str,
) -> Result<u64, ScenarioError> {
    reject_extras(entries, field)?;
    let value = entries.remove(field);
    value.ok_or_else(|| missing_field(field)).and_then(json_u64)
}

/// Extract and remove a single string field, rejecting any extra keys.
fn single_string(
    entries: &mut BTreeMap<String, serde_json::Value>,
    field: &str,
) -> Result<String, ScenarioError> {
    reject_extras(entries, field)?;
    let value = entries.remove(field);
    value
        .ok_or_else(|| missing_field(field))
        .and_then(json_string)
}

/// Extract and remove a single bool field, rejecting any extra keys.
fn single_bool(
    entries: &mut BTreeMap<String, serde_json::Value>,
    field: &str,
) -> Result<bool, ScenarioError> {
    reject_extras(entries, field)?;
    let value = entries.remove(field);
    value
        .ok_or_else(|| missing_field(field))
        .and_then(json_bool)
}

/// Extract and remove a single string-array field, rejecting any extra keys.
fn single_string_vec(
    entries: &mut BTreeMap<String, serde_json::Value>,
    field: &str,
) -> Result<Vec<String>, ScenarioError> {
    reject_extras(entries, field)?;
    let value = entries.remove(field);
    value
        .ok_or_else(|| missing_field(field))
        .and_then(json_string_vec)
}

/// Build the two-field `expectCount` step.
fn build_expect_count(
    entries: &mut BTreeMap<String, serde_json::Value>,
) -> Result<Step, ScenarioError> {
    reject_extras_multi(entries, &["expectCount", "count"])?;
    let pattern = entries
        .remove("expectCount")
        .ok_or_else(|| missing_field("expectCount"))?;
    let count = entries
        .remove("count")
        .ok_or_else(|| missing_field("count"))?;
    Ok(Step::ExpectCount {
        pattern: json_string(pattern)?,
        count: json_u32(count)?,
    })
}

/// Build the two-field `macro` invocation step.
fn build_macro(entries: &mut BTreeMap<String, serde_json::Value>) -> Result<Step, ScenarioError> {
    reject_extras_multi(entries, &["macro", "args"])?;
    let name = entries
        .remove("macro")
        .ok_or_else(|| missing_field("macro"))?;
    let args_val = entries
        .remove("args")
        .ok_or_else(|| missing_field("args"))?;
    Ok(Step::Macro {
        name: json_string(name)?,
        args: json_args(args_val)?,
    })
}

/// Reject any keys beyond `field`.
fn reject_extras(
    entries: &BTreeMap<String, serde_json::Value>,
    field: &str,
) -> Result<(), ScenarioError> {
    if let Some(extra) = entries.keys().find(|k| k.as_str() != field) {
        return Err(ScenarioError::InvalidStep {
            reason: format!("unexpected key '{extra}' in step"),
        });
    }
    Ok(())
}

/// Reject any keys beyond the provided allowed set.
fn reject_extras_multi(
    entries: &BTreeMap<String, serde_json::Value>,
    allowed: &[&str],
) -> Result<(), ScenarioError> {
    for key in entries.keys() {
        if !allowed.iter().any(|a| a == key) {
            return Err(ScenarioError::InvalidStep {
                reason: format!("unexpected key '{key}' in step"),
            });
        }
    }
    Ok(())
}

/// Coerce a JSON value to `u64` with a typed error.
fn json_u64(value: serde_json::Value) -> Result<u64, ScenarioError> {
    value.as_u64().ok_or_else(|| ScenarioError::InvalidStep {
        reason: format!("expected a non-negative integer, got {value}"),
    })
}

/// Coerce a JSON value to `u32` with a typed error, rejecting truncation.
fn json_u32(value: serde_json::Value) -> Result<u32, ScenarioError> {
    let raw = json_u64(value)?;
    u32::try_from(raw).map_err(|_| ScenarioError::InvalidStep {
        reason: format!("count {raw} exceeds u32 range"),
    })
}

/// Coerce a JSON value to `String` with a typed error.
fn json_string(value: serde_json::Value) -> Result<String, ScenarioError> {
    value
        .as_str()
        .map(std::string::ToString::to_string)
        .ok_or_else(|| ScenarioError::InvalidStep {
            reason: format!("expected a string, got {value}"),
        })
}

/// Coerce a JSON value to `bool` with a typed error.
fn json_bool(value: serde_json::Value) -> Result<bool, ScenarioError> {
    value.as_bool().ok_or_else(|| ScenarioError::InvalidStep {
        reason: format!("expected a boolean, got {value}"),
    })
}

/// Coerce a JSON array to `Vec<String>` with a typed error.
fn json_string_vec(value: serde_json::Value) -> Result<Vec<String>, ScenarioError> {
    value
        .as_array()
        .ok_or_else(|| ScenarioError::InvalidStep {
            reason: format!("expected an array, got {value}"),
        })
        .and_then(|arr| {
            arr.iter()
                .map(|v| {
                    v.as_str()
                        .map(std::string::ToString::to_string)
                        .ok_or_else(|| ScenarioError::InvalidStep {
                            reason: format!("expected a string in array, got {v}"),
                        })
                })
                .collect()
        })
}

/// Coerce a JSON object to a macro-argument map.
///
/// Argument values are accepted as strings or non-string scalars and stored
/// verbatim as their JSON text so raw substitution can splice them exactly.
fn json_args(value: serde_json::Value) -> Result<BTreeMap<String, String>, ScenarioError> {
    let obj = value
        .as_object()
        .ok_or_else(|| ScenarioError::InvalidStep {
            reason: format!("expected an object for args, got {value}"),
        })?;
    let mut out = BTreeMap::new();
    for (k, v) in obj {
        out.insert(k.clone(), arg_to_string(v)?);
    }
    Ok(out)
}

/// Render a scalar argument to its raw substitution string.
///
/// Strings use their text; numbers and booleans use their JSON text so the
/// splice is exact and unambiguous.
fn arg_to_string(value: &serde_json::Value) -> Result<String, ScenarioError> {
    match value {
        serde_json::Value::String(s) => Ok(s.clone()),
        serde_json::Value::Number(_) | serde_json::Value::Bool(_) => Ok(value.to_string()),
        other => Err(ScenarioError::InvalidStep {
            reason: format!("macro argument must be a scalar, got {other}"),
        }),
    }
}

/// Build a `MissingField` error for a step field.
fn missing_field(field: &str) -> ScenarioError {
    ScenarioError::MissingField {
        field: field.to_string(),
        context: "step".to_string(),
    }
}
