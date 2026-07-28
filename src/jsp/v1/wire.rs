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

use std::fmt;

use serde::Deserialize;
use serde::de::{Error as _, MapAccess, SeqAccess, Visitor};

use super::limits::{
    ACCEPTED_SCHEMA, MAX_AGENT_KIND_BYTES, MAX_DIAGNOSTIC_CODE_BYTES, MAX_DIAGNOSTIC_SUMMARY_BYTES,
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
pub enum UnsupportedMarker {
    Unsupported,
}

/// `unsupported` string vs supported object, for a generic value payload.
///
/// The serde representation is untagged so the JSON is either the string
/// `"unsupported"` or a supported-state object.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum FieldWire {
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
pub struct SupportedState {
    pub provenance: ProvenanceWire,
    pub availability: AvailabilityKindWire,
    #[serde(default)]
    pub value: Present,
    #[serde(default)]
    pub last_value: Present,
    #[serde(default)]
    pub as_of_ms: Option<u64>,
    #[serde(default)]
    pub diagnostic_code: Option<String>,
}

/// Wrapper that tracks whether a field was present in the JSON and, if so,
/// its value (including explicit `null`).
///
/// Implements `Deserialize` manually via [`StrictValue`] so that an explicit
/// JSON `null` produces `Present { present: true, value: Null }` while an
/// absent field (via `#[serde(default)]`) produces
/// `Present { present: false, value: Null }`.
#[derive(Default, Clone, Debug)]
pub struct Present {
    pub present: bool,
    pub value: serde_json::Value,
}

impl<'de> Deserialize<'de> for Present {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let StrictValue(value) = StrictValue::deserialize(deserializer)?;
        Ok(Self {
            present: true,
            value,
        })
    }
}

/// A JSON value that rejects duplicate object keys anywhere in its subtree.
///
/// `serde_json::Value` resolves duplicate keys last-wins. Capturing a payload
/// through a plain `Value` would therefore let a producer send the same member
/// twice and have the closed payload DTO see only the final occurrence, so the
/// same bytes could mean different things to different JSON libraries. This
/// wrapper keeps duplicate rejection uniform with the top-level envelope, which
/// `deny_unknown_fields` already covers.
pub struct StrictValue(pub serde_json::Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictValueVisitor)
    }
}

/// Visitor building a [`StrictValue`], rejecting duplicate object keys.
struct StrictValueVisitor;

impl<'de> Visitor<'de> for StrictValueVisitor {
    type Value = StrictValue;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a JSON value without duplicate object keys")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(StrictValue(serde_json::Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(StrictValue(serde_json::Value::from(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(StrictValue(serde_json::Value::from(value)))
    }

    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
        let number = serde_json::Number::from_f64(value)
            .ok_or_else(|| E::custom("number is not representable in JSON"))?;
        Ok(StrictValue(serde_json::Value::Number(number)))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(StrictValue(serde_json::Value::String(value.to_owned())))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(StrictValue(serde_json::Value::String(value)))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(StrictValue(serde_json::Value::Null))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut items = Vec::new();
        while let Some(StrictValue(item)) = seq.next_element()? {
            items.push(item);
        }
        Ok(StrictValue(serde_json::Value::Array(items)))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut object = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            let StrictValue(value) = map.next_value()?;
            // The key itself is producer payload and is never echoed.
            if object.insert(key, value).is_some() {
                return Err(A::Error::custom("duplicate object key"));
            }
        }
        Ok(StrictValue(serde_json::Value::Object(object)))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceWire {
    Authoritative,
    Inferred,
}

/// The availability discriminator string: `known`, `unknown`, or `degraded`.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityKindWire {
    Known,
    Unknown,
    Degraded,
}

// ---------------------------------------------------------------------------
// Top-level envelope
// ---------------------------------------------------------------------------

/// The single accepted schema version, enforced during deserialization.
///
/// [`parse`](super::parse) probes the raw discriminators first so an
/// unsupported version reports `JSP-E003`; this type is what makes the closed
/// envelope itself unable to hold any other version.
pub struct AcceptedSchema;

impl<'de> Deserialize<'de> for AcceptedSchema {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        if value == ACCEPTED_SCHEMA {
            Ok(Self)
        } else {
            Err(D::Error::custom("unsupported schema version"))
        }
    }
}

/// The single accepted top-level document kind.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotKindWire {
    Snapshot,
}

/// The closed top-level snapshot envelope.
///
/// `schema` and `kind` are validated by their own closed types during
/// deserialization, so they are never read afterwards.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotWire {
    #[serde(rename = "schema")]
    pub _schema: AcceptedSchema,
    #[serde(rename = "kind")]
    pub _kind: SnapshotKindWire,
    pub agent_id: String,
    pub lifecycle_generation: u64,
    pub source_epoch: String,
    pub source_sequence: u64,
    pub cursor: u64,
    pub bridge_observed_ms: u64,
    pub native_session: NativeSessionWire,
    pub process_binding: FieldWire,
    pub native_activity: FieldWire,
    pub current_wait: FieldWire,
    pub current_turn: FieldWire,
    pub todos: FieldWire,
    pub last_displayed_assistant_message: FieldWire,
    pub last_created_tool_call: FieldWire,
    pub source_terminal_state: FieldWire,
    pub source_error_state: FieldWire,
}

/// Native session metadata object (closed).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSessionWire {
    pub repository: String,
    pub path: String,
    pub agent_kind: String,
    pub pid: u32,
    pub display_name: String,
}

/// Process-binding value payload: `{ "pid": u32, "started_at_ms": u64 }`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessBindingPayload {
    pub pid: u32,
    pub started_at_ms: u64,
}

/// Current-wait value payload: `{ "reason": string }`.
///
/// Like every other value payload this is a closed DTO, so credential, control,
/// and transcript members inside `current_wait.value` are rejected at ingress
/// rather than silently ignored.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaitPayload {
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Typed value payloads (validated after deserialization)
// ---------------------------------------------------------------------------

/// Native activity value payload: `{ "state": "idle" | "thinking" | "acting" }`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeActivityPayload {
    pub state: String,
}

/// Current-turn payload: `{ "elapsed_ms": u64 }`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentTurnPayload {
    pub elapsed_ms: u64,
}

/// Todos payload: `{ "revision": u64, "items": [TodoItemWire] }`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TodosPayload {
    pub revision: u64,
    pub items: Vec<TodoItemWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TodoItemWire {
    pub text: String,
    pub completed: bool,
}

/// Last-displayed-assistant-message payload.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayedMessagePayload {
    pub content: String,
    pub committed_ms: u64,
}

/// Last-created-tool-call payload: `{ "label": string, "phase": string }`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallPayload {
    pub label: String,
    pub phase: String,
}

/// Source-error-state payload: `{ "summary": string, "code": string }`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceErrorPayload {
    pub summary: String,
    pub code: String,
}

/// All field-bound constants grouped for the validator.
pub struct FieldLimits;

impl FieldLimits {
    pub const ID: usize = MAX_ID_BYTES;
    pub const TODOS: usize = MAX_TODOS;
    pub const TODO_TEXT: usize = MAX_TODO_TEXT_BYTES;
    pub const DISPLAYED_CONTENT: usize = MAX_DISPLAYED_CONTENT_BYTES;
    pub const DIAGNOSTIC_SUMMARY: usize = MAX_DIAGNOSTIC_SUMMARY_BYTES;
    pub const TOOL_LABEL: usize = MAX_TOOL_LABEL_BYTES;
    pub const REPOSITORY: usize = MAX_REPOSITORY_BYTES;
    pub const PATH: usize = MAX_PATH_BYTES;
    pub const AGENT_KIND: usize = MAX_AGENT_KIND_BYTES;
    pub const DISPLAY_NAME: usize = MAX_DISPLAY_NAME_BYTES;
    pub const DIAGNOSTIC_CODE: usize = MAX_DIAGNOSTIC_CODE_BYTES;
    pub const ERROR_CODE: usize = MAX_ERROR_CODE_BYTES;
}
