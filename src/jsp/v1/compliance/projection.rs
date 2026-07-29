//! Normalized client projection and observation-health state (issue 477).
//!
//! The [`NormalizedProjection`] is the byte-stable, language-neutral summary
//! of a client's reduced observation state after each scenario step. It is the
//! value compared against each scenario's `expected` block. It deliberately
//! carries structural state (labels, phases, counts, revisions, health) rather
//! than free-text content, so two implementations produce byte-identical
//! projections without echoing payload prose.
//!
//! Observation health is observer-owned (specification §20): it is computed
//! from transport behavior and heartbeat timing, never producer-reported. The
//! three axes — process liveness, observation health, native activity — remain
//! orthogonal.

use serde::{Deserialize, Serialize};

use crate::domain::observation::{
    Availability, CurrentTurnField, FieldState, LastMessageField, LastToolField,
    NativeActivityField, NativeActivityState, TodosField, ToolPhase, WaitReason,
};

/// Observer-owned observation health (specification §20). Never a wire field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationHealth {
    /// The field is unsupported by the producer/source.
    Unsupported,
    /// The observer is establishing the stream.
    Connecting,
    /// The stream is live and within lease.
    #[default]
    Live,
    /// A heartbeat/lease was missed; telemetry may be stale.
    Stale,
    /// The transport disconnected; a fresh snapshot-first stream is required.
    Disconnected,
    /// A protocol-level error occurred.
    ProtocolError,
}

impl ObservationHealth {
    /// The stable wire label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Connecting => "connecting",
            Self::Live => "live",
            Self::Stale => "stale",
            Self::Disconnected => "disconnected",
            Self::ProtocolError => "protocol_error",
        }
    }
}

/// A normalized projection of reduced client observation state.
///
/// This is the comparison value for scenario oracles. Field naming matches the
/// scenario `expected` JSON so an external implementation can compare directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedProjection {
    /// Opaque agent identity for the bound stream.
    pub agent_id: String,
    /// Lifecycle generation for the bound stream.
    pub generation: u64,
    /// Source epoch for the bound stream.
    pub source_epoch: String,
    /// Snapshot cursor or last contiguous event sequence applied.
    pub last_sequence: u64,
    /// Native activity without source payload.
    pub activity: ActivityProjection,
    /// Native activity provenance.
    pub activity_provenance: ProjectionProvenance,
    /// Explicit wait state without source payload.
    pub wait: WaitProjection,
    /// Explicit wait provenance.
    pub wait_provenance: ProjectionProvenance,
    /// Whether a known current turn is active.
    pub turn_active: bool,
    /// Current-turn availability.
    pub turn_availability: AvailabilityProjection,
    /// Current-turn provenance.
    pub turn_provenance: ProjectionProvenance,
    /// Outcome of the last explicitly ended turn.
    pub turn_outcome: Option<TurnOutcomeProjection>,
    /// Typed todo support and availability state.
    pub todos_state: TodoProjection,
    /// Last applied todo replacement revision.
    pub todos_revision: Option<u64>,
    /// Number of projected todos without their text.
    pub todos_count: usize,
    /// Structural label of the last-created tool.
    pub tool_label: Option<String>,
    /// Phase of the last-created tool.
    pub tool_phase: ToolPhaseProjection,
    /// Last-created-tool availability.
    pub tool_availability: AvailabilityProjection,
    /// Last-created-tool provenance.
    pub tool_provenance: ProjectionProvenance,
    /// Presence of a committed assistant message.
    pub last_message: MessagePresence,
    /// Committed-message availability.
    pub message_availability: AvailabilityProjection,
    /// Committed-message provenance.
    pub message_provenance: ProjectionProvenance,
    /// Presence of source terminal state.
    pub source_terminal: MessagePresence,
    /// Source-terminal availability, including known absence.
    pub terminal_availability: AvailabilityProjection,
    /// Source-terminal provenance.
    pub source_terminal_provenance: ProjectionProvenance,
    /// Presence of source error state.
    pub source_error: MessagePresence,
    /// Source-error availability.
    pub error_availability: AvailabilityProjection,
    /// Source-error provenance.
    pub source_error_provenance: ProjectionProvenance,
    /// Whether the native session emitted its terminal event.
    pub session_ended: bool,
    /// Availability of required native-session metadata.
    pub native_session_availability: AvailabilityProjection,
    /// Provenance of required native-session metadata.
    pub native_session_provenance: ProjectionProvenance,
    /// Availability of process-binding evidence.
    pub process_binding_availability: AvailabilityProjection,
    /// Provenance of process-binding evidence.
    pub process_binding_provenance: ProjectionProvenance,
    /// Observer-owned stream health.
    pub observation_health: ObservationHealth,
    /// Runtime-owned process liveness, when observed.
    pub process_alive: Option<bool>,
}

/// Payload-free projection of the complete JSP field availability algebra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityProjection {
    /// The producer does not support the field.
    Unsupported,
    /// The field is supported but no value is currently known.
    Unknown,
    /// The field has a current value.
    Known,
    /// The optional field is authoritatively known to be absent.
    KnownAbsent,
    /// Only a stale last value is available.
    Degraded,
}

/// Visible producer provenance retained by normalized projections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionProvenance {
    /// The producer does not support the field.
    Unsupported,
    /// The source directly supplied the value.
    Authoritative,
    /// The producer inferred the value under the frozen JSP/1 contract.
    Inferred,
}

/// Todo state keeps support, availability, provenance, and local revision
/// rejection distinct instead of collapsing all absent values to a count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoProjection {
    Unsupported,
    Unknown,
    AuthoritativeEmpty,
    Authoritative,
    Inferred,
    Degraded,
}

/// Normalized native activity projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityProjection {
    Idle,
    Thinking,
    Acting,
    Unsupported,
    Unknown,
    Degraded,
}

/// Normalized wait projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitProjection {
    NotWaiting,
    Permission,
    Question,
    Elicitation,
    Choice,
    UserInput,
    Other,
    Unsupported,
    Unknown,
}

impl WaitProjection {
    /// Map a typed wait reason to its projection.
    #[must_use]
    pub fn from_reason(reason: WaitReason) -> Self {
        match reason {
            WaitReason::Permission => Self::Permission,
            WaitReason::Question => Self::Question,
            WaitReason::Elicitation => Self::Elicitation,
            WaitReason::Choice => Self::Choice,
            WaitReason::UserInput => Self::UserInput,
            WaitReason::Other => Self::Other,
        }
    }
}

/// Normalized turn outcome projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnOutcomeProjection {
    Completed,
    Failed,
    Cancelled,
}

/// Normalized tool phase projection. `Unknown` covers a supported-but-unknown
/// tool field (privacy mode); `Absent` covers unsupported/no-tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolPhaseProjection {
    Proposed,
    AwaitingApproval,
    Scheduled,
    Executing,
    Succeeded,
    Failed,
    Cancelled,
    Unknown,
    Absent,
}

impl ToolPhaseProjection {
    /// Map a typed tool phase to its projection.
    #[must_use]
    pub fn from_phase(phase: ToolPhase) -> Self {
        match phase {
            ToolPhase::Proposed => Self::Proposed,
            ToolPhase::AwaitingApproval => Self::AwaitingApproval,
            ToolPhase::Scheduled => Self::Scheduled,
            ToolPhase::Executing => Self::Executing,
            ToolPhase::Succeeded => Self::Succeeded,
            ToolPhase::Failed => Self::Failed,
            ToolPhase::Cancelled => Self::Cancelled,
        }
    }
}

/// Normalized displayed-message presence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessagePresence {
    Present,
    Absent,
    Unknown,
}

/// Project a native-activity field state.
pub(crate) fn project_activity(field: &NativeActivityField) -> ActivityProjection {
    match field {
        FieldState::Unsupported => ActivityProjection::Unsupported,
        FieldState::Supported { availability, .. } => match availability {
            Availability::Known(value) => match value.state {
                NativeActivityState::Idle => ActivityProjection::Idle,
                NativeActivityState::Thinking => ActivityProjection::Thinking,
                NativeActivityState::Acting => ActivityProjection::Acting,
            },
            Availability::Unknown => ActivityProjection::Unknown,
            Availability::Degraded { .. } => ActivityProjection::Degraded,
        },
    }
}

/// Project a wait field state.
pub(crate) fn project_wait(field: &crate::domain::observation::CurrentWaitField) -> WaitProjection {
    match field {
        FieldState::Unsupported => WaitProjection::Unsupported,
        FieldState::Supported { availability, .. } => match availability {
            Availability::Known(None) => WaitProjection::NotWaiting,
            Availability::Known(Some(wait)) => WaitProjection::from_reason(wait.reason),
            // Unknown and degraded (invalid for wait) both project to unknown.
            Availability::Unknown | Availability::Degraded { .. } => WaitProjection::Unknown,
        },
    }
}

/// Project a current-turn field into turn-active.
pub(crate) fn project_turn_active(field: &CurrentTurnField) -> bool {
    matches!(
        field,
        FieldState::Supported {
            availability: Availability::Known(_),
            ..
        }
    )
}

/// Retain the provenance carried by a field.
pub(crate) fn project_provenance<T>(field: &FieldState<T>) -> ProjectionProvenance {
    match field {
        FieldState::Unsupported => ProjectionProvenance::Unsupported,
        FieldState::Supported { provenance, .. } => match provenance {
            crate::domain::observation::Provenance::Authoritative => {
                ProjectionProvenance::Authoritative
            }
            crate::domain::observation::Provenance::Inferred => ProjectionProvenance::Inferred,
        },
    }
}

pub(crate) fn project_availability<T>(field: &FieldState<T>) -> AvailabilityProjection {
    match field {
        FieldState::Unsupported => AvailabilityProjection::Unsupported,
        FieldState::Supported {
            availability: Availability::Unknown,
            ..
        } => AvailabilityProjection::Unknown,
        FieldState::Supported {
            availability: Availability::Known(_),
            ..
        } => AvailabilityProjection::Known,
        FieldState::Supported {
            availability: Availability::Degraded { .. },
            ..
        } => AvailabilityProjection::Degraded,
    }
}

pub(crate) fn project_optional_availability<T>(
    field: &FieldState<Option<T>>,
) -> AvailabilityProjection {
    match field {
        FieldState::Supported {
            availability: Availability::Known(None),
            ..
        } => AvailabilityProjection::KnownAbsent,
        _ => project_availability(field),
    }
}

/// Project todos without collapsing support, provenance, and availability.
pub(crate) fn project_todos(field: &TodosField) -> (TodoProjection, Option<u64>, usize) {
    match field {
        FieldState::Unsupported => (TodoProjection::Unsupported, None, 0),
        FieldState::Supported {
            provenance,
            availability: Availability::Known(list),
        } => {
            let state = match (provenance, list.items.is_empty()) {
                (crate::domain::observation::Provenance::Authoritative, true) => {
                    TodoProjection::AuthoritativeEmpty
                }
                (crate::domain::observation::Provenance::Authoritative, false) => {
                    TodoProjection::Authoritative
                }
                (crate::domain::observation::Provenance::Inferred, _) => TodoProjection::Inferred,
            };
            (state, Some(list.revision), list.items.len())
        }
        FieldState::Supported {
            availability: Availability::Unknown,
            ..
        } => (TodoProjection::Unknown, None, 0),
        FieldState::Supported {
            availability: Availability::Degraded { last_value, .. },
            ..
        } => (
            TodoProjection::Degraded,
            Some(last_value.revision),
            last_value.items.len(),
        ),
    }
}

/// Project a last-message field into presence.
pub(crate) fn project_message(field: &LastMessageField) -> MessagePresence {
    match field {
        FieldState::Unsupported => MessagePresence::Absent,
        FieldState::Supported { availability, .. } => match availability {
            Availability::Unknown => MessagePresence::Unknown,
            // Known and degraded both indicate a message exists.
            Availability::Known(_) | Availability::Degraded { .. } => MessagePresence::Present,
        },
    }
}

/// Project support and availability without exposing a sensitive payload.
pub(crate) fn project_presence<T>(field: &FieldState<T>) -> MessagePresence {
    match field {
        FieldState::Unsupported => MessagePresence::Absent,
        FieldState::Supported {
            availability: Availability::Unknown,
            ..
        } => MessagePresence::Unknown,
        FieldState::Supported {
            availability: Availability::Known(_) | Availability::Degraded { .. },
            ..
        } => MessagePresence::Present,
    }
}

/// Project a last-tool field into label/phase.
pub(crate) fn project_tool(field: &LastToolField) -> (Option<String>, ToolPhaseProjection) {
    match field {
        FieldState::Unsupported => (None, ToolPhaseProjection::Absent),
        FieldState::Supported { availability, .. } => match availability {
            Availability::Known(tool) => (
                Some(tool.label.as_str().to_string()),
                ToolPhaseProjection::from_phase(tool.phase),
            ),
            Availability::Unknown => (None, ToolPhaseProjection::Unknown),
            Availability::Degraded { last_value, .. } => (
                Some(last_value.label.as_str().to_string()),
                ToolPhaseProjection::from_phase(last_value.phase),
            ),
        },
    }
}

#[cfg(test)]
#[path = "projection_tests.rs"]
mod tests;
