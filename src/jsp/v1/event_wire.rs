//! Closed wire DTOs for JSP/1 event and heartbeat documents (issue #476).
//!
//! These types are private to the crate and model the exact closed envelopes
//! from specification sections 18 and 19. Like the snapshot DTOs they use
//! `#[serde(deny_unknown_fields)]` and closed enums, so unknown members,
//! duplicate keys, and wrong types fail during deserialization rather than
//! being ignored. Conversion to typed domain values happens in `event` only
//! after the whole document has deserialized.
//!
//! Payload variants list their members explicitly rather than using
//! `#[serde(flatten)]`, because flattening silently disables
//! `deny_unknown_fields` and would reopen the envelope.

use serde::Deserialize;

use super::wire::{AcceptedSchema, TodoItemWire};

/// The `kind` discriminator for an event document.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKindWire {
    Event,
}

/// The `kind` discriminator for a heartbeat document.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeartbeatKindWire {
    Heartbeat,
}

/// The closed top-level event envelope (specification 18).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventWire {
    #[serde(rename = "schema")]
    pub _schema: AcceptedSchema,
    #[serde(rename = "kind")]
    pub _kind: EventKindWire,
    pub agent_id: String,
    pub lifecycle_generation: u64,
    pub source_epoch: String,
    pub source_sequence: u64,
    pub bridge_observed_ms: u64,
    pub event: EventPayloadWire,
}

/// The closed top-level heartbeat envelope (specification 19).
///
/// A heartbeat deliberately has no `source_sequence`: it reports source
/// liveness, not a state transition, so it must not advance or gap the stream.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatWire {
    #[serde(rename = "schema")]
    pub _schema: AcceptedSchema,
    #[serde(rename = "kind")]
    pub _kind: HeartbeatKindWire,
    pub agent_id: String,
    pub lifecycle_generation: u64,
    pub source_epoch: String,
    pub bridge_observed_ms: u64,
}

/// The closed event payload, discriminated by `type`.
///
/// An unknown `type` fails as a closed-shape violation. There is no
/// forward-compatible ignore rule: dropping a transition would leave the
/// status view confidently wrong rather than visibly unknown.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventPayloadWire {
    #[serde(rename = "activity.changed")]
    ActivityChanged { state: String },
    #[serde(rename = "wait.opened")]
    WaitOpened { reason: String },
    #[serde(rename = "wait.resolved")]
    WaitResolved {},
    #[serde(rename = "turn.started")]
    TurnStarted {},
    #[serde(rename = "turn.ended")]
    TurnEnded { outcome: String },
    #[serde(rename = "todos.replaced")]
    TodosReplaced {
        revision: u64,
        items: Vec<TodoItemWire>,
    },
    #[serde(rename = "tool_call.created")]
    ToolCallCreated { label: String, phase: String },
    #[serde(rename = "tool_call.phase_changed")]
    ToolCallPhaseChanged { label: String, phase: String },
    #[serde(rename = "assistant_message.displayed")]
    AssistantMessageDisplayed { content: String, committed_ms: u64 },
    #[serde(rename = "source.error")]
    SourceError { summary: String, code: String },
    #[serde(rename = "session.ended")]
    SessionEnded {},
}
