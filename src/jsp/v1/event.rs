//! Event and heartbeat parsing for JSP/1 (issue #476).
//!
//! [`parse_event`] and [`parse_heartbeat`] mirror the snapshot entry point:
//! they perform no I/O and no logging, enforce the document byte bound before
//! parsing, gate the schema and kind, deserialize a closed envelope, and only
//! then convert into typed domain values. Diagnostics never echo producer
//! payload values.

use crate::domain::observation::{
    BoundedText, DiagnosticSummary, DisplayedAssistantMessage, EventRecord, HeartbeatRecord,
    NativeActivityState, ObservationEvent, SourceErrorValue, TodoItem, TodoList, TodoState,
    ToolCallValue, ToolLabel, ToolPhase, TurnOutcome, WaitReason,
};

use super::error::JspError;
use super::event_wire::{EventPayloadWire, EventWire, HeartbeatWire};
use super::parse::{check_document_bound, deserialize_closed, expect_kind};
use super::validate::{bounded, build_event_identity, count_bound};
use super::wire::{FieldLimits, TodoItemWire};

/// Parse JSP/1 event bytes into a validated [`EventRecord`].
///
/// # Errors
///
/// - `JSP-E001` for malformed JSON, unknown/duplicate fields, unknown event
///   types, or any closed-shape violation.
/// - `JSP-E002` for exceeded inclusive bounds.
/// - `JSP-E003` for unsupported schema or kind.
/// - `JSP-E004` for identity invariants.
/// - `JSP-E005` for illegal field values such as a producer-sent `stale`.
pub fn parse_event(input: &[u8]) -> Result<EventRecord, JspError> {
    check_document_bound(input)?;
    expect_kind(input, "event")?;
    let wire: EventWire = deserialize_closed(input)?;
    convert_event(wire)
}

/// Parse JSP/1 heartbeat bytes into a validated [`HeartbeatRecord`].
///
/// # Errors
///
/// Same taxonomy as [`parse_event`].
pub fn parse_heartbeat(input: &[u8]) -> Result<HeartbeatRecord, JspError> {
    check_document_bound(input)?;
    expect_kind(input, "heartbeat")?;
    let wire: HeartbeatWire = deserialize_closed(input)?;
    convert_heartbeat(wire)
}

/// Convert a deserialized heartbeat envelope into a typed record.
pub(super) fn convert_heartbeat(wire: HeartbeatWire) -> Result<HeartbeatRecord, JspError> {
    Ok(HeartbeatRecord {
        identity: build_event_identity(
            &wire.agent_id,
            wire.lifecycle_generation,
            &wire.source_epoch,
        )?,
        bridge_observed_ms: wire.bridge_observed_ms,
    })
}

/// Convert a deserialized event envelope into a typed record.
pub(super) fn convert_event(wire: EventWire) -> Result<EventRecord, JspError> {
    let identity = build_event_identity(
        &wire.agent_id,
        wire.lifecycle_generation,
        &wire.source_epoch,
    )?;
    let event = convert_payload(wire.event)?;
    Ok(EventRecord {
        identity,
        source_sequence: wire.source_sequence,
        bridge_observed_ms: wire.bridge_observed_ms,
        event,
    })
}

/// Convert the closed payload into the typed transition.
fn convert_payload(payload: EventPayloadWire) -> Result<ObservationEvent, JspError> {
    match payload {
        EventPayloadWire::ActivityChanged { state } => convert_activity(&state),
        EventPayloadWire::WaitOpened { reason } => convert_wait_opened(&reason),
        EventPayloadWire::WaitResolved {} => Ok(ObservationEvent::WaitResolved),
        EventPayloadWire::TurnStarted {} => Ok(ObservationEvent::TurnStarted),
        EventPayloadWire::TurnEnded { outcome } => convert_turn_ended(&outcome),
        EventPayloadWire::TodosReplaced { revision, items } => convert_todos(revision, &items),
        EventPayloadWire::ToolCallCreated { label, phase } => {
            Ok(ObservationEvent::ToolCallCreated {
                tool: convert_tool("event.tool_call.created", &label, &phase)?,
            })
        }
        EventPayloadWire::ToolCallPhaseChanged { label, phase } => {
            Ok(ObservationEvent::ToolCallPhaseChanged {
                tool: convert_tool("event.tool_call.phase_changed", &label, &phase)?,
            })
        }
        EventPayloadWire::AssistantMessageDisplayed {
            content,
            committed_ms,
        } => convert_message(&content, committed_ms),
        EventPayloadWire::SourceError { summary, code } => convert_source_error(&summary, &code),
        EventPayloadWire::SessionEnded {} => Ok(ObservationEvent::SessionEnded),
    }
}

/// Convert an activity transition, rejecting states outside the inventory.
fn convert_activity(state: &str) -> Result<ObservationEvent, JspError> {
    let state = NativeActivityState::from_wire(state).ok_or_else(|| {
        JspError::field_state("event.activity.changed.state: unsupported activity state")
    })?;
    Ok(ObservationEvent::ActivityChanged { state })
}

/// Convert a wait-opened transition. Only an explicit reason creates waiting.
fn convert_wait_opened(reason: &str) -> Result<ObservationEvent, JspError> {
    let reason = WaitReason::from_wire(reason)
        .ok_or_else(|| JspError::field_state("event.wait.opened.reason: unsupported reason"))?;
    Ok(ObservationEvent::WaitOpened { reason })
}

/// Convert a turn-ended transition with a closed outcome.
fn convert_turn_ended(outcome: &str) -> Result<ObservationEvent, JspError> {
    let outcome = TurnOutcome::from_wire(outcome)
        .ok_or_else(|| JspError::field_state("event.turn.ended.outcome: unsupported outcome"))?;
    Ok(ObservationEvent::TurnEnded { outcome })
}

/// Convert a full todo replacement, applying the section 8 invariants.
fn convert_todos(revision: u64, items: &[TodoItemWire]) -> Result<ObservationEvent, JspError> {
    if revision == 0 {
        return Err(JspError::field_state(
            "event.todos.replaced.revision: must be a positive integer",
        ));
    }
    count_bound(
        "event.todos.replaced.items",
        items.len(),
        FieldLimits::TODOS,
    )?;
    let mut converted = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let path = format!("event.todos.replaced.items[{index}].text");
        let state_path = format!("event.todos.replaced.items[{index}].state");
        super::limits::check_bound(&state_path, item.state.len(), FieldLimits::TODO_STATE)?;
        converted.push(TodoItem {
            text: bounded(&path, &item.text, FieldLimits::TODO_TEXT, BoundedText)?,
            state: TodoState::from_wire(&item.state),
        });
    }
    Ok(ObservationEvent::TodosReplaced {
        todos: TodoList {
            revision,
            items: converted,
        },
    })
}

/// Convert a tool-call transition, rejecting phases outside the inventory.
fn convert_tool(path: &str, label: &str, phase: &str) -> Result<ToolCallValue, JspError> {
    let label = bounded(
        &format!("{path}.label"),
        label,
        FieldLimits::TOOL_LABEL,
        ToolLabel,
    )?;
    let phase = ToolPhase::from_wire(phase)
        .ok_or_else(|| JspError::field_state(format!("{path}.phase: unsupported phase")))?;
    Ok(ToolCallValue { label, phase })
}

/// Convert a displayed-message transition.
fn convert_message(content: &str, committed_ms: u64) -> Result<ObservationEvent, JspError> {
    Ok(ObservationEvent::AssistantMessageDisplayed {
        message: DisplayedAssistantMessage {
            content: bounded(
                "event.assistant_message.displayed.content",
                content,
                FieldLimits::DISPLAYED_CONTENT,
                BoundedText,
            )?,
            committed_ms,
        },
    })
}

/// Convert a source-error transition.
fn convert_source_error(summary: &str, code: &str) -> Result<ObservationEvent, JspError> {
    Ok(ObservationEvent::SourceError {
        error: SourceErrorValue {
            summary: bounded(
                "event.source.error.summary",
                summary,
                FieldLimits::DIAGNOSTIC_SUMMARY,
                DiagnosticSummary,
            )?,
            code: bounded(
                "event.source.error.code",
                code,
                FieldLimits::ERROR_CODE,
                BoundedText,
            )?,
        },
    })
}
