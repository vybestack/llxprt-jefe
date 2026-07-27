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
use super::limits::{ACCEPTED_SCHEMA, SNAPSHOT_KIND};
use super::wire::{
    AvailabilityKindWire, CurrentTurnPayload, DisplayedMessagePayload, FieldLimits, FieldWire,
    NativeActivityPayload, NativeSessionWire, ProcessBindingPayload, ProvenanceWire, SnapshotWire,
    SourceErrorPayload, SupportedState, TodoItemWire, TodosPayload, ToolCallPayload,
};

/// Convert a fully-deserialized wire snapshot into a typed domain snapshot.
///
/// This is the single conversion boundary. It assumes the wire DTO already
/// passed `deny_unknown_fields` and structural deserialization; it applies
/// schema/kind gates, identity invariants, bounds, closed-enum parsing, and
/// field-state semantics.
pub(crate) fn convert(wire: SnapshotWire) -> Result<Snapshot, JspError> {
    validate_schema_and_kind(&wire)?;
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

/// Reject the document if schema or kind is wrong. This is the version/kind
/// gate (decision 1): unknown schema/kind fails with `JSP-E003`.
fn validate_schema_and_kind(wire: &SnapshotWire) -> Result<(), JspError> {
    if wire.schema != ACCEPTED_SCHEMA {
        return Err(JspError::unsupported_version(format!(
            "snapshot.schema: unsupported schema version (accepted: {ACCEPTED_SCHEMA})"
        )));
    }
    if wire.kind != SNAPSHOT_KIND {
        return Err(JspError::unsupported_version(format!(
            "snapshot.kind: unsupported kind (accepted: \"{SNAPSHOT_KIND}\")"
        )));
    }
    Ok(())
}

/// Build and validate the live observation identity (decision 2/3).
fn build_identity(wire: &SnapshotWire) -> Result<ObservationKey, JspError> {
    let agent_id = parse_opaque_id("snapshot.agent_id", &wire.agent_id)?;
    if wire.lifecycle_generation == 0 {
        return Err(JspError::identity(
            "snapshot.lifecycle_generation: must be a positive integer",
        ));
    }
    let source_epoch = parse_opaque_id("snapshot.source_epoch", &wire.source_epoch)?;
    Ok(ObservationKey(ObservationIdentity {
        agent_id,
        lifecycle_generation: wire.lifecycle_generation,
        source_epoch,
    }))
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
    let availability = build_availability("snapshot.process_binding", state, |value| {
        let payload: ProcessBindingPayload = parse_payload("snapshot.process_binding", value)?;
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
    build_known: impl Fn(&serde_json::Value) -> Result<T, JspError>,
) -> Result<Availability<T>, JspError> {
    match state.availability {
        AvailabilityKindWire::Unknown => {
            reject_present_unknown_extras(path, state)?;
            Ok(Availability::Unknown)
        }
        AvailabilityKindWire::Known => {
            reject_degraded_only_fields(path, state)?;
            let value = require_value(path, state)?;
            Ok(Availability::Known(build_known(value)?))
        }
        AvailabilityKindWire::Degraded => {
            let last_value = require_last_value(path, state)?;
            let as_of_ms = require_as_of_ms(path, state)?;
            let diagnostic_code = require_diagnostic_code(path, state)?;
            Ok(Availability::Degraded {
                last_value: build_known(last_value)?,
                as_of_ms,
                diagnostic_code,
            })
        }
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
    let availability = build_availability("snapshot.native_activity", state, |value| {
        let payload: NativeActivityPayload = parse_payload("snapshot.native_activity", value)?;
        let activity = NativeActivityState::from_wire(&payload.state).ok_or_else(|| {
            JspError::field_state("snapshot.native_activity.state: unsupported activity state")
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
    let availability = build_availability("snapshot.current_wait", state, |value| {
        if value.is_null() {
            Ok(None)
        } else {
            let reason = parse_wait_reason("snapshot.current_wait", value)?;
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
    let availability = build_availability("snapshot.current_turn", state, |value| {
        let payload: CurrentTurnPayload = parse_payload("snapshot.current_turn", value)?;
        Ok(CurrentTurn {
            elapsed_ms: payload.elapsed_ms,
        })
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
    let availability = build_availability("snapshot.todos", state, |value| {
        let payload: TodosPayload = parse_payload("snapshot.todos", value)?;
        if payload.revision == 0 {
            return Err(JspError::field_state(
                "snapshot.todos.value.revision: must be a positive integer",
            ));
        }
        super::limits::check_bound(
            "snapshot.todos.value.items",
            payload.items.len(),
            FieldLimits::TODOS,
        )?;
        let mut items = Vec::with_capacity(payload.items.len());
        for (index, item) in payload.items.iter().enumerate() {
            items.push(parse_todo_item(index, item)?);
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
fn parse_todo_item(index: usize, item: &TodoItemWire) -> Result<TodoItem, JspError> {
    let path = format!("snapshot.todos.value.items[{index}].text");
    let text = parse_bounded_text(&path, &item.text, FieldLimits::TODO_TEXT, BoundedText)?;
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
        |value| {
            let payload: DisplayedMessagePayload =
                parse_payload("snapshot.last_displayed_assistant_message", value)?;
            let content = parse_bounded_text(
                "snapshot.last_displayed_assistant_message.value.content",
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
    let availability = build_availability("snapshot.last_created_tool_call", state, |value| {
        let payload: ToolCallPayload = parse_payload("snapshot.last_created_tool_call", value)?;
        parse_tool_value(&payload)
    })?;
    Ok(FieldState::Supported {
        provenance,
        availability,
    })
}

/// Parse a tool-call value payload into the typed value.
fn parse_tool_value(payload: &ToolCallPayload) -> Result<ToolCallValue, JspError> {
    let label = parse_bounded_text(
        "snapshot.last_created_tool_call.value.label",
        &payload.label,
        FieldLimits::TOOL_LABEL,
        ToolLabel,
    )?;
    let phase = ToolPhase::from_wire(&payload.phase).ok_or_else(|| {
        // `stale` and any unknown phase land here: field-state violation.
        JspError::field_state("snapshot.last_created_tool_call.value.phase: unsupported phase")
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
    let availability = build_availability("snapshot.source_terminal_state", state, |value| {
        if value.is_null() {
            Ok(None)
        } else {
            let payload: SourceErrorPayload =
                parse_payload("snapshot.source_terminal_state", value)?;
            Ok(Some(parse_source_error(&payload)?))
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
    let availability = build_availability("snapshot.source_error_state", state, |value| {
        if value.is_null() {
            return Err(JspError::field_state(
                "snapshot.source_error_state: known null is not valid; use unsupported",
            ));
        }
        let payload: SourceErrorPayload = parse_payload("snapshot.source_error_state", value)?;
        parse_source_error(&payload)
    })?;
    Ok(FieldState::Supported {
        provenance,
        availability,
    })
}

/// Parse a source-error payload with bound checks.
fn parse_source_error(payload: &SourceErrorPayload) -> Result<SourceErrorValue, JspError> {
    let summary = parse_bounded_string(
        "snapshot.source_error_state.value.summary",
        &payload.summary,
        FieldLimits::DIAGNOSTIC_SUMMARY,
        DiagnosticSummary,
    )?;
    let code = parse_bounded_text(
        "snapshot.source_error_state.value.code",
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
fn parse_wait_reason(path: &str, value: &serde_json::Value) -> Result<WaitReason, JspError> {
    let obj = value
        .as_object()
        .ok_or_else(|| JspError::closed_shape(format!("{path}.value: expected a wait object")))?;
    let reason_value = obj
        .get("reason")
        .ok_or_else(|| JspError::closed_shape(format!("{path}.value.reason: missing field")))?;
    let reason_str = reason_value
        .as_str()
        .ok_or_else(|| JspError::closed_shape(format!("{path}.value.reason: expected a string")))?;
    WaitReason::from_wire(reason_str)
        .ok_or_else(|| JspError::field_state(format!("{path}.value.reason: unsupported reason")))
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

/// Parse a bounded text into a `BoundedText`-style newtype.
fn parse_bounded_text<T>(
    path: &str,
    value: &str,
    max: usize,
    wrap: impl Fn(String) -> T,
) -> Result<T, JspError> {
    parse_bounded_string(path, value, max, wrap)
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
