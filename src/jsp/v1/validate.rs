//! Wire-to-domain validation and conversion (issue #476, J1 slice).
//!
//! This module converts a fully-deserialized [`wire::SnapshotWire`] into a
//! typed [`Snapshot`](super::contract::Snapshot) of domain observation values.
//! Conversion happens only after the entire document has deserialized, so a
//! partially validated payload can never escape. Every string is bound-checked
//! and every value is parsed into a closed enum/newtype; any violation returns
//! a coded [`JspError`] without echoing payload text.

use crate::domain::observation::{
    AgentKindLabel, Availability, BoundedText, CurrentTurn, CurrentTurnField, CurrentWaitField,
    DiagnosticCode, DiagnosticSummary, DisplayName, DisplayedAssistantMessage, FieldState,
    LastMessageField, LastToolField, NativeActivityField, NativeActivityState, NativeActivityValue,
    NativeSession, ObservationIdentity, OpaqueId, PathRef, ProcessBinding, ProcessBindingField,
    Provenance, RepositoryRef, SourceErrorField, SourceErrorValue, SourceTerminalField, TodoItem,
    TodoList, TodosField, ToolCallValue, ToolLabel, ToolPhase, Wait, WaitReason,
};

use super::contract::{ObservationKey, Snapshot};
use super::error::JspError;
use super::wire::{
    AvailabilityKindWire, CurrentTurnPayload, DisplayedMessagePayload, FieldLimits, FieldWire,
    NativeActivityPayload, NativeSessionWire, ProcessBindingPayload, ProvenanceWire, SnapshotWire,
    SourceErrorPayload, SupportedState, TodoItemWire, TodosPayload, ToolCallPayload, WaitPayload,
};

/// Convert a fully-deserialized wire snapshot into a typed domain snapshot.
///
/// This is the single conversion boundary. It assumes the wire DTO already
/// passed `deny_unknown_fields` and structural deserialization; it applies
/// schema/kind gates, identity invariants, bounds, closed-enum parsing, and
/// field-state semantics.
pub fn convert(wire: SnapshotWire) -> Result<Snapshot, JspError> {
    let identity = build_identity(&wire)?;
    let native_session = build_native_session(&wire.native_session)?;
    let process_binding = convert_process_binding(&wire.process_binding)?;
    let native_activity = convert_native_activity(&wire.native_activity)?;
    let current_wait = convert_current_wait(&wire.current_wait)?;
    let current_turn = convert_current_turn(&wire.current_turn)?;
    let todos = convert_todos(&wire.todos)?;
    let last_message = convert_last_message(&wire.last_displayed_assistant_message)?;
    let last_tool = convert_last_tool(&wire.last_created_tool_call)?;
    let source_terminal = convert_source_terminal(&wire.source_terminal_state)?;
    let source_error = convert_source_error(&wire.source_error_state)?;

    Ok(Snapshot {
        identity: identity.identity().clone(),
        source_sequence: wire.source_sequence,
        cursor: wire.cursor,
        bridge_observed_ms: wire.bridge_observed_ms,
        native_session,
        process_binding,
        native_activity,
        current_wait,
        current_turn,
        todos,
        last_displayed_assistant_message: last_message,
        last_created_tool_call: last_tool,
        source_terminal_state: source_terminal,
        source_error_state: source_error,
    })
}

/// Build and validate the live observation identity (decision 2/3).
fn build_identity(wire: &SnapshotWire) -> Result<ObservationKey, JspError> {
    let identity = build_event_identity(
        &wire.agent_id,
        wire.lifecycle_generation,
        &wire.source_epoch,
    )?;
    Ok(ObservationKey(identity))
}

/// Build and validate the live observation identity from its three parts.
///
/// Shared by snapshot, event, and heartbeat documents so every document kind
/// enforces the same identity triple invariants.
pub(super) fn build_event_identity(
    agent_id: &str,
    lifecycle_generation: u64,
    source_epoch: &str,
) -> Result<ObservationIdentity, JspError> {
    let agent_id = parse_opaque_id("document.agent_id", agent_id)?;
    if lifecycle_generation == 0 {
        return Err(JspError::identity(
            "document.lifecycle_generation: must be a positive integer",
        ));
    }
    let source_epoch = parse_opaque_id("document.source_epoch", source_epoch)?;
    Ok(ObservationIdentity {
        agent_id,
        lifecycle_generation,
        source_epoch,
    })
}

/// Bound-check a string and wrap it, for reuse by the event converters.
pub(super) fn bounded<T>(
    path: &str,
    value: &str,
    max: usize,
    wrap: impl Fn(String) -> T,
) -> Result<T, JspError> {
    parse_bounded_string(path, value, max, wrap)
}

/// Bound-check an entry count, for reuse by the event converters.
pub(super) fn count_bound(path: &str, count: usize, max: usize) -> Result<(), JspError> {
    super::limits::check_count_bound(path, count, max)
}

/// Validate and build the native session descriptive metadata.
fn build_native_session(wire: &NativeSessionWire) -> Result<NativeSession, JspError> {
    let repository = parse_bounded_string(
        "snapshot.native_session.repository",
        &wire.repository,
        FieldLimits::REPOSITORY,
        RepositoryRef,
    )?;
    let path = parse_bounded_string(
        "snapshot.native_session.path",
        &wire.path,
        FieldLimits::PATH,
        PathRef,
    )?;
    let agent_kind = parse_bounded_string(
        "snapshot.native_session.agent_kind",
        &wire.agent_kind,
        FieldLimits::AGENT_KIND,
        AgentKindLabel,
    )?;
    let display_name = parse_bounded_string(
        "snapshot.native_session.display_name",
        &wire.display_name,
        FieldLimits::DISPLAY_NAME,
        DisplayName,
    )?;
    Ok(NativeSession {
        repository,
        path,
        agent_kind,
        pid: wire.pid,
        display_name,
    })
}

/// Convert the process-binding field. This is producer-reported binding
/// evidence only; process liveness stays Jefe-runtime-owned (decision 4).
fn convert_process_binding(field: &FieldWire) -> Result<ProcessBindingField, JspError> {
    let Some(state) = supported_state(field) else {
        return Ok(FieldState::Unsupported);
    };
    let provenance = convert_provenance(&state.provenance);
    let availability = build_availability("snapshot.process_binding", state, |slot, value| {
        let payload: ProcessBindingPayload =
            parse_payload(&slot.root("snapshot.process_binding"), value)?;
        Ok(ProcessBinding {
            pid: payload.pid,
            started_at_ms: payload.started_at_ms,
        })
    })?;
    Ok(FieldState::Supported {
        provenance,
        availability,
    })
}

/// Convert provenance wire value to domain.
fn convert_provenance(wire: &ProvenanceWire) -> Provenance {
    match wire {
        ProvenanceWire::Authoritative => Provenance::Authoritative,
        ProvenanceWire::Inferred => Provenance::Inferred,
    }
}

/// Extract the supported state from a field wire value, or return None if
/// unsupported.
fn supported_state(field: &FieldWire) -> Option<&SupportedState> {
    match field {
        FieldWire::Unsupported(_) => None,
        FieldWire::Supported(state) => Some(state),
    }
}

/// Validate the availability fields on a supported state and produce the
/// availability value via the supplied known-value builder.
fn build_availability<T>(
    path: &str,
    state: &SupportedState,
    build_known: impl Fn(&ValueSlot, &serde_json::Value) -> Result<T, JspError>,
) -> Result<Availability<T>, JspError> {
    match state.availability {
        AvailabilityKindWire::Unknown => {
            reject_present_unknown_extras(path, state)?;
            Ok(Availability::Unknown)
        }
        AvailabilityKindWire::Known => {
            reject_degraded_only_fields(path, state)?;
            let value = require_value(path, state)?;
            Ok(Availability::Known(build_known(&ValueSlot::Value, value)?))
        }
        AvailabilityKindWire::Degraded => {
            let last_value = require_last_value(path, state)?;
            let as_of_ms = require_as_of_ms(path, state)?;
            let diagnostic_code = require_diagnostic_code(path, state)?;
            Ok(Availability::Degraded {
                last_value: build_known(&ValueSlot::LastValue, last_value)?,
                as_of_ms,
                diagnostic_code,
            })
        }
    }
}

/// Which member of a supported field state a value was read from.
///
/// Diagnostics must name the member the producer actually sent, so a `degraded`
/// payload reports `last_value` rather than `value`.
enum ValueSlot {
    Value,
    LastValue,
}

impl ValueSlot {
    /// The JSON member name for this slot.
    const fn member(&self) -> &'static str {
        match self {
            Self::Value => "value",
            Self::LastValue => "last_value",
        }
    }

    /// Build the diagnostic path for a leaf inside this slot.
    fn path(&self, field_path: &str, leaf: &str) -> String {
        format!("{field_path}.{}.{leaf}", self.member())
    }

    /// Build the diagnostic path for the slot itself.
    fn root(&self, field_path: &str) -> String {
        format!("{field_path}.{}", self.member())
    }
}

/// Reject `value`/`last_value`/`as_of_ms`/`diagnostic_code` when availability
/// is `unknown`.
fn reject_present_unknown_extras(path: &str, state: &SupportedState) -> Result<(), JspError> {
    if state.value.present
        || state.last_value.present
        || state.as_of_ms.is_some()
        || state.diagnostic_code.is_some()
    {
        Err(JspError::closed_shape(format!(
            "{path}: unknown availability must not carry value fields"
        )))
    } else {
        Ok(())
    }
}

/// Reject the degraded-only members when availability is `known`.
fn reject_degraded_only_fields(path: &str, state: &SupportedState) -> Result<(), JspError> {
    if state.last_value.present || state.as_of_ms.is_some() || state.diagnostic_code.is_some() {
        Err(JspError::closed_shape(format!(
            "{path}: known availability must not carry degraded fields"
        )))
    } else {
        Ok(())
    }
}

/// Require the `value` field for `known` availability.
fn require_value<'a>(
    path: &str,
    state: &'a SupportedState,
) -> Result<&'a serde_json::Value, JspError> {
    if state.value.present {
        Ok(&state.value.value)
    } else {
        Err(JspError::closed_shape(format!(
            "{path}: known availability requires a value field"
        )))
    }
}

/// Require the `last_value` field for `degraded` availability.
fn require_last_value<'a>(
    path: &str,
    state: &'a SupportedState,
) -> Result<&'a serde_json::Value, JspError> {
    reject_known_extra_for_degraded(path, state)?;
    if state.last_value.present {
        Ok(&state.last_value.value)
    } else {
        Err(JspError::closed_shape(format!(
            "{path}: degraded availability requires a last_value field"
        )))
    }
}

/// For `degraded`, reject a stray `value` field (degraded uses `last_value`).
fn reject_known_extra_for_degraded(path: &str, state: &SupportedState) -> Result<(), JspError> {
    if state.value.present {
        Err(JspError::closed_shape(format!(
            "{path}: degraded availability must use last_value, not value"
        )))
    } else {
        Ok(())
    }
}

/// Require the `as_of_ms` field for `degraded` availability.
fn require_as_of_ms(path: &str, state: &SupportedState) -> Result<u64, JspError> {
    state.as_of_ms.ok_or_else(|| {
        JspError::closed_shape(format!(
            "{path}: degraded availability requires an as_of_ms field"
        ))
    })
}

/// Require and bound-check the `diagnostic_code` field for `degraded`.
fn require_diagnostic_code(path: &str, state: &SupportedState) -> Result<DiagnosticCode, JspError> {
    let raw = state.diagnostic_code.as_ref().ok_or_else(|| {
        JspError::closed_shape(format!(
            "{path}: degraded availability requires a diagnostic_code field"
        ))
    })?;
    parse_diagnostic_code(&format!("{path}.diagnostic_code"), raw)
}

// ---------------------------------------------------------------------------
// Per-field converters
// ---------------------------------------------------------------------------

/// Convert the native-activity field.
fn convert_native_activity(field: &FieldWire) -> Result<NativeActivityField, JspError> {
    let Some(state) = supported_state(field) else {
        return Ok(FieldState::Unsupported);
    };
    let provenance = convert_provenance(&state.provenance);
    let availability = build_availability("snapshot.native_activity", state, |slot, value| {
        let payload: NativeActivityPayload =
            parse_payload(&slot.root("snapshot.native_activity"), value)?;
        let activity = NativeActivityState::from_wire(&payload.state).ok_or_else(|| {
            JspError::field_state(format!(
                "{}: unsupported activity state",
                slot.path("snapshot.native_activity", "state")
            ))
        })?;
        Ok(NativeActivityValue { state: activity })
    })?;
    Ok(FieldState::Supported {
        provenance,
        availability,
    })
}

/// Convert the current-wait field. `Known(null)` means explicitly not waiting.
fn convert_current_wait(field: &FieldWire) -> Result<CurrentWaitField, JspError> {
    let Some(state) = supported_state(field) else {
        return Ok(FieldState::Unsupported);
    };
    let provenance = convert_provenance(&state.provenance);
    if matches!(state.availability, AvailabilityKindWire::Degraded) {
        return Err(JspError::field_state(
            "snapshot.current_wait: degraded availability is not valid for wait state",
        ));
    }
    let availability = build_availability("snapshot.current_wait", state, |slot, value| {
        if value.is_null() {
            Ok(None)
        } else {
            let reason = parse_wait_reason(&slot.root("snapshot.current_wait"), value)?;
            Ok(Some(Wait { reason }))
        }
    })?;
    Ok(FieldState::Supported {
        provenance,
        availability,
    })
}

/// Convert the current-turn field.
fn convert_current_turn(field: &FieldWire) -> Result<CurrentTurnField, JspError> {
    let Some(state) = supported_state(field) else {
        return Ok(FieldState::Unsupported);
    };
    let provenance = convert_provenance(&state.provenance);
    let availability = build_availability("snapshot.current_turn", state, |slot, value| {
        let payload: Option<CurrentTurnPayload> =
            parse_payload(&slot.root("snapshot.current_turn"), value)?;
        Ok(payload.map(|payload| CurrentTurn {
            elapsed_ms: payload.elapsed_ms,
        }))
    })?;
    Ok(FieldState::Supported {
        provenance,
        availability,
    })
}

/// Convert the todos field with revision/length/text bounds (decision 8).
fn convert_todos(field: &FieldWire) -> Result<TodosField, JspError> {
    let Some(state) = supported_state(field) else {
        return Ok(FieldState::Unsupported);
    };
    let provenance = convert_provenance(&state.provenance);
    let availability = build_availability("snapshot.todos", state, |slot, value| {
        let payload: TodosPayload = parse_payload(&slot.root("snapshot.todos"), value)?;
        if payload.revision == 0 {
            return Err(JspError::field_state(format!(
                "{}: must be a positive integer",
                slot.path("snapshot.todos", "revision")
            )));
        }
        super::limits::check_count_bound(
            &slot.path("snapshot.todos", "items"),
            payload.items.len(),
            FieldLimits::TODOS,
        )?;
        let mut items = Vec::with_capacity(payload.items.len());
        for (index, item) in payload.items.iter().enumerate() {
            items.push(parse_todo_item(slot, index, item)?);
        }
        Ok(TodoList {
            revision: payload.revision,
            items,
        })
    })?;
    Ok(FieldState::Supported {
        provenance,
        availability,
    })
}

/// Parse a single todo item with text bound.
fn parse_todo_item(
    slot: &ValueSlot,
    index: usize,
    item: &TodoItemWire,
) -> Result<TodoItem, JspError> {
    let path = slot.path("snapshot.todos", &format!("items[{index}].text"));
    let text = parse_bounded_string(&path, &item.text, FieldLimits::TODO_TEXT, BoundedText)?;
    Ok(TodoItem {
        text,
        completed: item.completed,
    })
}

/// Convert the last-displayed-assistant-message field.
fn convert_last_message(field: &FieldWire) -> Result<LastMessageField, JspError> {
    let Some(state) = supported_state(field) else {
        return Ok(FieldState::Unsupported);
    };
    let provenance = convert_provenance(&state.provenance);
    let availability = build_availability(
        "snapshot.last_displayed_assistant_message",
        state,
        |slot, value| {
            let field = "snapshot.last_displayed_assistant_message";
            let payload: DisplayedMessagePayload = parse_payload(&slot.root(field), value)?;
            let content = parse_bounded_string(
                &slot.path(field, "content"),
                &payload.content,
                FieldLimits::DISPLAYED_CONTENT,
                BoundedText,
            )?;
            Ok(DisplayedAssistantMessage {
                content,
                committed_ms: payload.committed_ms,
            })
        },
    )?;
    Ok(FieldState::Supported {
        provenance,
        availability,
    })
}

/// Convert the last-created-tool-call field. Rejects `stale` phase (decision 5).
fn convert_last_tool(field: &FieldWire) -> Result<LastToolField, JspError> {
    let Some(state) = supported_state(field) else {
        return Ok(FieldState::Unsupported);
    };
    let provenance = convert_provenance(&state.provenance);
    let availability =
        build_availability("snapshot.last_created_tool_call", state, |slot, value| {
            let payload: ToolCallPayload =
                parse_payload(&slot.root("snapshot.last_created_tool_call"), value)?;
            parse_tool_value(slot, &payload)
        })?;
    Ok(FieldState::Supported {
        provenance,
        availability,
    })
}

/// Parse a tool-call value payload into the typed value.
fn parse_tool_value(
    slot: &ValueSlot,
    payload: &ToolCallPayload,
) -> Result<ToolCallValue, JspError> {
    let field = "snapshot.last_created_tool_call";
    let label = parse_bounded_string(
        &slot.path(field, "label"),
        &payload.label,
        FieldLimits::TOOL_LABEL,
        ToolLabel,
    )?;
    let phase = ToolPhase::from_wire(&payload.phase).ok_or_else(|| {
        // `stale` and any unknown phase land here: field-state violation.
        JspError::field_state(format!("{}: unsupported phase", slot.path(field, "phase")))
    })?;
    Ok(ToolCallValue { label, phase })
}

/// Convert the source-terminal-state field. `Known(null)` means clean.
fn convert_source_terminal(field: &FieldWire) -> Result<SourceTerminalField, JspError> {
    let Some(state) = supported_state(field) else {
        return Ok(FieldState::Unsupported);
    };
    let provenance = convert_provenance(&state.provenance);
    if matches!(state.availability, AvailabilityKindWire::Degraded) {
        return Err(JspError::field_state(
            "snapshot.source_terminal_state: degraded availability is not valid",
        ));
    }
    let availability =
        build_availability("snapshot.source_terminal_state", state, |slot, value| {
            if value.is_null() {
                Ok(None)
            } else {
                let root = slot.root("snapshot.source_terminal_state");
                let payload: SourceErrorPayload = parse_payload(&root, value)?;
                Ok(Some(parse_source_error(
                    slot,
                    "snapshot.source_terminal_state",
                    &payload,
                )?))
            }
        })?;
    Ok(FieldState::Supported {
        provenance,
        availability,
    })
}

/// Convert the source-error-state field.
fn convert_source_error(field: &FieldWire) -> Result<SourceErrorField, JspError> {
    let Some(state) = supported_state(field) else {
        return Ok(FieldState::Unsupported);
    };
    let provenance = convert_provenance(&state.provenance);
    let availability = build_availability("snapshot.source_error_state", state, |slot, value| {
        if value.is_null() {
            return Err(JspError::field_state(
                "snapshot.source_error_state: known null is not valid; use unsupported",
            ));
        }
        let root = slot.root("snapshot.source_error_state");
        let payload: SourceErrorPayload = parse_payload(&root, value)?;
        parse_source_error(slot, "snapshot.source_error_state", &payload)
    })?;
    Ok(FieldState::Supported {
        provenance,
        availability,
    })
}

/// Parse a source-error payload with bound checks.
///
/// The caller supplies its own field path so diagnostics name the field the
/// producer actually sent rather than a sibling that shares this payload shape.
fn parse_source_error(
    slot: &ValueSlot,
    field_path: &str,
    payload: &SourceErrorPayload,
) -> Result<SourceErrorValue, JspError> {
    let summary = parse_bounded_string(
        &slot.path(field_path, "summary"),
        &payload.summary,
        FieldLimits::DIAGNOSTIC_SUMMARY,
        DiagnosticSummary,
    )?;
    let code = parse_bounded_string(
        &slot.path(field_path, "code"),
        &payload.code,
        FieldLimits::ERROR_CODE,
        BoundedText,
    )?;
    Ok(SourceErrorValue { summary, code })
}

// ---------------------------------------------------------------------------
// Scalar parsing helpers
// ---------------------------------------------------------------------------

/// Parse a wait reason from a known-wait JSON object.
///
/// The payload goes through the closed [`WaitPayload`] DTO, so unknown members
/// (including credential and control fields) fail as a closed-shape violation
/// exactly as they do for every other field value.
/// `slot_root` is the already-resolved `value`/`last_value` path, so the leaf
/// is appended directly rather than re-adding the member name.
fn parse_wait_reason(slot_root: &str, value: &serde_json::Value) -> Result<WaitReason, JspError> {
    let payload: WaitPayload = parse_payload(slot_root, value)?;
    WaitReason::from_wire(&payload.reason)
        .ok_or_else(|| JspError::field_state(format!("{slot_root}.reason: unsupported reason")))
}

/// Parse a raw JSON value into a typed payload via serde_json, mapping any
/// failure to a closed-shape error without echoing the payload.
fn parse_payload<'de, T: serde::Deserialize<'de>>(
    path: &str,
    value: &'de serde_json::Value,
) -> Result<T, JspError> {
    T::deserialize(value).map_err(|_| {
        JspError::closed_shape(format!(
            "{path}: value shape does not match the closed contract"
        ))
    })
}

/// Parse an opaque identifier with safe-ASCII and length bounds.
fn parse_opaque_id(path: &str, value: &str) -> Result<OpaqueId, JspError> {
    if value.is_empty() {
        return Err(JspError::closed_shape(format!("{path}: must not be empty")));
    }
    super::limits::check_bound(path, value.len(), FieldLimits::ID)?;
    if !value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        return Err(JspError::closed_shape(format!(
            "{path}: must contain only safe ASCII characters"
        )));
    }
    Ok(OpaqueId(value.to_string()))
}

/// Parse a bounded string into a generic newtype wrapper.
fn parse_bounded_string<T>(
    path: &str,
    value: &str,
    max: usize,
    wrap: impl Fn(String) -> T,
) -> Result<T, JspError> {
    super::limits::check_bound(path, value.len(), max)?;
    Ok(wrap(value.to_string()))
}

/// Parse a diagnostic code with bound check.
fn parse_diagnostic_code(path: &str, value: &str) -> Result<DiagnosticCode, JspError> {
    super::limits::check_bound(path, value.len(), FieldLimits::DIAGNOSTIC_CODE)?;
    Ok(DiagnosticCode(value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_opaque_id_rejects_empty_and_unsafe() {
        assert!(parse_opaque_id("x", "").is_err());
        assert!(parse_opaque_id("x", "has space").is_err());
        assert!(parse_opaque_id("x", "ok_id-1.2").is_ok());
    }

    #[test]
    fn parse_opaque_id_rejects_over_limit() {
        let too_long = "a".repeat(FieldLimits::ID + 1);
        assert!(parse_opaque_id("x", &too_long).is_err());
    }
}
