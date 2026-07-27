//! Private closed wire DTOs for JSP/1 (issue #476, J1 slice).
//!
//! These types are `pub(crate)` and never leak into the public API. They model
//! the exact closed JSON envelope and use `#[serde(deny_unknown_fields)]` plus
//! exhaustive field-state enums so that any unknown field, wrong type, or
//! duplicate field fails at deserialization with a `JSP-E001` closed-shape
//! error. Conversion to typed domain values happens only in [`validate`] after
//! the entire document has deserialized.
//!
//! Field-state algebra (decision 5): each required field is either the literal
//! string `"unsupported"` or an object with `provenance` and `availability`
//! (plus `value`/`last_value`/`as_of_ms`/`diagnostic_code` as appropriate).

use serde::Deserialize;

use super::limits::{
    MAX_AGENT_KIND_BYTES, MAX_DIAGNOSTIC_CODE_BYTES, MAX_DIAGNOSTIC_SUMMARY_BYTES,
    MAX_DISPLAY_NAME_BYTES, MAX_DISPLAYED_CONTENT_BYTES, MAX_ERROR_CODE_BYTES, MAX_ID_BYTES,
    MAX_PATH_BYTES, MAX_REPOSITORY_BYTES, MAX_TODO_TEXT_BYTES, MAX_TODOS, MAX_TOOL_LABEL_BYTES,
};

// ---------------------------------------------------------------------------
// Closed field-state enums
// ---------------------------------------------------------------------------

/// The literal string `"unsupported"`.
///
/// Serde deserializes this from the exact JSON string `"unsupported"`. Any
/// other string value fails deserialization, which is exactly the closed-shape
/// behavior we want.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UnsupportedMarker {
    Unsupported,
}

/// `unsupported` string vs supported object, for a generic value payload.
///
/// The serde representation is untagged so the JSON is either the string
/// `"unsupported"` or a supported-state object.
#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum FieldWire {
    Unsupported(UnsupportedMarker),
    Supported(Box<SupportedState>),
}

// ---------------------------------------------------------------------------
// Supported field state
// ---------------------------------------------------------------------------

/// A supported field state object with provenance and availability.
///
/// The flat representation is:
/// ```json
/// { "provenance": "authoritative", "availability": "known", "value": ... }
/// ```
/// For `degraded`, the sibling fields are `last_value`, `as_of_ms`, and
/// `diagnostic_code`. `deny_unknown_fields` keeps the object closed.
///
/// The `value` and `last_value` fields use [`Present`] to preserve the
/// distinction between absent and explicit JSON `null`, which is needed for
/// optional-entity fields (decision 6: "Optional current entities use known
/// null").
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SupportedState {
    pub(crate) provenance: ProvenanceWire,
    pub(crate) availability: AvailabilityKindWire,
    #[serde(default)]
    pub(crate) value: Present,
    #[serde(default)]
    pub(crate) last_value: Present,
    #[serde(default)]
    pub(crate) as_of_ms: Option<u64>,
    #[serde(default)]
    pub(crate) diagnostic_code: Option<String>,
}

/// Wrapper that tracks whether a field was present in the JSON and, if so,
/// its value (including explicit `null`).
///
/// Implements `Deserialize` manually via a `deserialize_any`-backed visitor so
/// that an explicit JSON `null` produces `Present { present: true, value: Null }`
/// while an absent field (via `#[serde(default)]`) produces
/// `Present { present: false, value: Null }`.
#[derive(Default, Clone, Debug)]
pub(crate) struct Present {
    pub(crate) present: bool,
    pub(crate) value: serde_json::Value,
}

impl<'de> Deserialize<'de> for Present {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(Self {
            present: true,
            value,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProvenanceWire {
    Authoritative,
    Inferred,
}

/// The availability discriminator string: `known`, `unknown`, or `degraded`.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AvailabilityKindWire {
    Known,
    Unknown,
    Degraded,
}

// ---------------------------------------------------------------------------
// Top-level envelope
// ---------------------------------------------------------------------------

/// The closed top-level snapshot envelope.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SnapshotWire {
    pub(crate) schema: u64,
    pub(crate) kind: String,
    pub(crate) agent_id: String,
    pub(crate) lifecycle_generation: u64,
    pub(crate) source_epoch: String,
    pub(crate) source_sequence: u64,
    pub(crate) cursor: u64,
    pub(crate) bridge_observed_ms: u64,
    pub(crate) native_session: NativeSessionWire,
    pub(crate) process_binding: FieldWire,
    pub(crate) native_activity: FieldWire,
    pub(crate) current_wait: FieldWire,
    pub(crate) current_turn: FieldWire,
    pub(crate) todos: FieldWire,
    pub(crate) last_displayed_assistant_message: FieldWire,
    pub(crate) last_created_tool_call: FieldWire,
    pub(crate) source_terminal_state: FieldWire,
    pub(crate) source_error_state: FieldWire,
}

/// Native session metadata object (closed).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeSessionWire {
    pub(crate) repository: String,
    pub(crate) path: String,
    pub(crate) agent_kind: String,
    pub(crate) pid: u32,
    pub(crate) display_name: String,
}

/// Process-binding value payload: `{ "pid": u32, "started_at_ms": u64 }`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProcessBindingPayload {
    pub(crate) pid: u32,
    pub(crate) started_at_ms: u64,
}

// ---------------------------------------------------------------------------
// Typed value payloads (validated after deserialization)
// ---------------------------------------------------------------------------

/// Native activity value payload: `{ "state": "idle" | "thinking" | "acting" }`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeActivityPayload {
    pub(crate) state: String,
}

/// Current-turn payload: `{ "elapsed_ms": u64 }`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CurrentTurnPayload {
    pub(crate) elapsed_ms: u64,
}

/// Todos payload: `{ "revision": u64, "items": [TodoItemWire] }`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TodosPayload {
    pub(crate) revision: u64,
    pub(crate) items: Vec<TodoItemWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TodoItemWire {
    pub(crate) text: String,
    pub(crate) completed: bool,
}

/// Last-displayed-assistant-message payload.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DisplayedMessagePayload {
    pub(crate) content: String,
    pub(crate) committed_ms: u64,
}

/// Last-created-tool-call payload: `{ "label": string, "phase": string }`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ToolCallPayload {
    pub(crate) label: String,
    pub(crate) phase: String,
}

/// Source-error-state payload: `{ "summary": string, "code": string }`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceErrorPayload {
    pub(crate) summary: String,
    pub(crate) code: String,
}

/// All field-bound constants grouped for the validator.
pub(crate) struct FieldLimits;

impl FieldLimits {
    pub(crate) const ID: usize = MAX_ID_BYTES;
    pub(crate) const TODOS: usize = MAX_TODOS;
    pub(crate) const TODO_TEXT: usize = MAX_TODO_TEXT_BYTES;
    pub(crate) const DISPLAYED_CONTENT: usize = MAX_DISPLAYED_CONTENT_BYTES;
    pub(crate) const DIAGNOSTIC_SUMMARY: usize = MAX_DIAGNOSTIC_SUMMARY_BYTES;
    pub(crate) const TOOL_LABEL: usize = MAX_TOOL_LABEL_BYTES;
    pub(crate) const REPOSITORY: usize = MAX_REPOSITORY_BYTES;
    pub(crate) const PATH: usize = MAX_PATH_BYTES;
    pub(crate) const AGENT_KIND: usize = MAX_AGENT_KIND_BYTES;
    pub(crate) const DISPLAY_NAME: usize = MAX_DISPLAY_NAME_BYTES;
    pub(crate) const DIAGNOSTIC_CODE: usize = MAX_DIAGNOSTIC_CODE_BYTES;
    pub(crate) const ERROR_CODE: usize = MAX_ERROR_CODE_BYTES;
}
