//! Deterministic reference reducer/projection (issue 477).
//!
//! [`ReferenceReducer`] is the pure, deterministic client-side state machine
//! that defines JSP/1 event semantics. It has no I/O, no clock dependency,
//! and no logging. It consumes validated typed JSP/1 documents and produces a
//! [`NormalizedProjection`].
//!
//! Current-state JSP/1 semantics (decision D1):
//! - A stream always begins with a snapshot, which atomically replaces the
//!   projection.
//! - Events increase `source_sequence` by exactly one within an epoch.
//! - A duplicate sequence is a no-op.
//! - A lower (out-of-order) sequence is a no-op.
//! - A gap (sequence > last + 1) is intentionally rejected: the event applies
//!   no native state mutation, but it does mark observation health stale
//!   (observer-owned health degrades because frames were lost) and requires a
//!   fresh snapshot-first stream. Native state is preserved untouched.
//! - An epoch mismatch (the event's `(agent_id, generation, source_epoch)` does
//!   not match the bound stream) is rejected with no mutation and no health
//!   change (an unrelated frame must not degrade the bound stream).
//! - A transport disconnect marks observation health stale or disconnected but
//!   preserves native state until a fresh snapshot arrives.

use crate::domain::observation::{
    AgentObservation, Availability, CurrentTurn, EventRecord, FieldState, HeartbeatRecord,
    NativeActivityState, NativeActivityValue, ObservationEvent, ObservationHealth,
    ObservationIdentity, ProcessBinding, Provenance, TodoList, ToolCallValue, Wait,
};
use crate::jsp::Snapshot;

use super::projection::{
    ActivityProjection, AvailabilityProjection, MessagePresence, NormalizedProjection,
    ProjectionProvenance, TodoProjection, ToolPhaseProjection, TurnOutcomeProjection,
    WaitProjection, project_activity, project_availability, project_message,
    project_optional_availability, project_presence, project_provenance, project_source_terminal,
    project_todos, project_tool, project_turn_active, project_wait,
};

fn observation_from_snapshot(snapshot: &Snapshot) -> AgentObservation {
    AgentObservation {
        identity: Some(snapshot.identity.clone()),
        last_sequence: snapshot.cursor,
        health: ObservationHealth::Live,
        activity: snapshot.native_activity.clone(),
        wait: snapshot.current_wait.clone(),
        turn: snapshot.current_turn.clone(),
        turn_observed_at: None,
        todos: snapshot.todos.clone(),
        last_message: snapshot.last_displayed_assistant_message.clone(),
        tool: snapshot.last_created_tool_call.clone(),
        terminal: snapshot.source_terminal_state.clone(),
        error: snapshot.source_error_state.clone(),
        session_ended: false,
    }
}

/// A reducer error, coded for stable machine-readable reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReducerError {
    /// No snapshot has established a stream.
    SnapshotRequired,
    /// A gap or disconnect invalidated the current stream.
    FreshSnapshotRequired,
    /// A gap was detected: the event sequence skips ahead. No transition mutation.
    Gap { expected: u64, actual: u64 },
    /// The event identity does not match the bound stream identity.
    IdentityMismatch,
    /// The event is not legal in the current reducer state.
    IllegalTransition { transition: &'static str },
}

impl ReducerError {
    /// Stable machine-readable code string.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::SnapshotRequired => "JSP-R-SNAPSHOT-REQUIRED",
            Self::FreshSnapshotRequired => "JSP-R-FRESH-SNAPSHOT-REQUIRED",
            Self::Gap { .. } => "JSP-R-GAP",
            Self::IdentityMismatch => "JSP-R-IDENTITY",
            Self::IllegalTransition { .. } => "JSP-R-TRANSITION",
        }
    }
}

impl std::fmt::Display for ReducerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SnapshotRequired => write!(f, "{}: snapshot must be first", self.code()),
            Self::FreshSnapshotRequired => {
                write!(f, "{}: fresh snapshot-first stream required", self.code())
            }
            Self::Gap { expected, actual } => write!(
                f,
                "{}: sequence gap expected {expected} actual {actual}",
                self.code()
            ),
            Self::IdentityMismatch => write!(f, "{}: stream identity mismatch", self.code()),
            Self::IllegalTransition { transition } => {
                write!(f, "{}: illegal {transition} transition", self.code())
            }
        }
    }
}

impl std::error::Error for ReducerError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum StreamPhase {
    #[default]
    AwaitingSnapshot,
    Live,
    NeedsFreshSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum NativeSessionLifecycle {
    #[default]
    AwaitingSnapshot,
    Active,
    Ended,
}

impl NativeSessionLifecycle {
    const fn mutation_error(self) -> Option<&'static str> {
        match self {
            Self::Active => None,
            Self::Ended => Some("post-session mutation"),
            Self::AwaitingSnapshot => Some("pre-session mutation"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentProcessIdentity {
    native_pid: u32,
    binding: Option<ProcessBinding>,
}

impl AgentProcessIdentity {
    fn from_snapshot(snapshot: &Snapshot) -> Self {
        let binding = match &snapshot.process_binding {
            FieldState::Supported {
                availability: Availability::Known(binding),
                ..
            }
            | FieldState::Supported {
                availability:
                    Availability::Degraded {
                        last_value: binding,
                        ..
                    },
                ..
            } => Some(binding.clone()),
            FieldState::Unsupported
            | FieldState::Supported {
                availability: Availability::Unknown,
                ..
            } => None,
        };
        Self {
            native_pid: snapshot.native_session.pid,
            binding,
        }
    }
}

/// This holds the typed domain values so the reducer can apply events
/// incrementally, then project to [`NormalizedProjection`] on demand.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ReducerState {
    identity: Option<ObservationIdentity>,
    last_sequence: u64,
    observation: AgentObservation,
    activity: ActivityProjection,
    activity_provenance: ProjectionProvenance,
    wait: WaitProjection,
    wait_provenance: ProjectionProvenance,
    turn_active: bool,
    turn_availability: AvailabilityProjection,
    turn_provenance: ProjectionProvenance,
    turn_outcome: Option<TurnOutcomeProjection>,
    todos: (TodoProjection, Option<u64>, usize),
    tool: (Option<String>, ToolPhaseProjection),
    tool_availability: AvailabilityProjection,
    tool_provenance: ProjectionProvenance,
    message: MessagePresence,
    message_availability: AvailabilityProjection,
    message_provenance: ProjectionProvenance,
    source_terminal: MessagePresence,
    terminal_availability: AvailabilityProjection,
    source_terminal_provenance: ProjectionProvenance,
    source_error: MessagePresence,
    error_availability: AvailabilityProjection,
    source_error_provenance: ProjectionProvenance,
    native_session_availability: AvailabilityProjection,
    native_session_provenance: ProjectionProvenance,
    process_binding_availability: AvailabilityProjection,
    process_binding_provenance: ProjectionProvenance,
    session_lifecycle: NativeSessionLifecycle,
    agent_process_identity: Option<AgentProcessIdentity>,
    health: ObservationHealth,
    process_alive: Option<bool>,
    stream_phase: StreamPhase,
}

impl Default for ReducerState {
    fn default() -> Self {
        Self::empty()
    }
}

impl ReducerState {
    /// An empty state with no bound stream.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            identity: None,
            last_sequence: 0,
            observation: AgentObservation::default(),
            activity: ActivityProjection::Unsupported,
            activity_provenance: ProjectionProvenance::Unsupported,
            wait: WaitProjection::Unsupported,
            wait_provenance: ProjectionProvenance::Unsupported,
            turn_active: false,
            turn_availability: AvailabilityProjection::Unsupported,
            turn_provenance: ProjectionProvenance::Unsupported,
            turn_outcome: None,
            todos: (TodoProjection::Unsupported, None, 0),
            tool: (None, ToolPhaseProjection::Absent),
            tool_availability: AvailabilityProjection::Unsupported,
            tool_provenance: ProjectionProvenance::Unsupported,
            message: MessagePresence::Absent,
            message_availability: AvailabilityProjection::Unsupported,
            message_provenance: ProjectionProvenance::Unsupported,
            source_terminal: MessagePresence::Absent,
            terminal_availability: AvailabilityProjection::Unsupported,
            source_terminal_provenance: ProjectionProvenance::Unsupported,
            source_error: MessagePresence::Absent,
            error_availability: AvailabilityProjection::Unsupported,
            source_error_provenance: ProjectionProvenance::Unsupported,
            native_session_availability: AvailabilityProjection::Unsupported,
            native_session_provenance: ProjectionProvenance::Unsupported,
            process_binding_availability: AvailabilityProjection::Unsupported,
            process_binding_provenance: ProjectionProvenance::Unsupported,
            session_lifecycle: NativeSessionLifecycle::AwaitingSnapshot,
            agent_process_identity: None,
            health: ObservationHealth::Connecting,
            process_alive: None,
            stream_phase: StreamPhase::AwaitingSnapshot,
        }
    }

    /// Project the current state to a normalized projection.
    #[must_use]
    pub fn project(&self) -> NormalizedProjection {
        let (agent_id, generation, source_epoch) =
            self.identity
                .as_ref()
                .map_or((String::new(), 0u64, String::new()), |id| {
                    (
                        id.agent_id.as_str().to_string(),
                        id.lifecycle_generation,
                        id.source_epoch.as_str().to_string(),
                    )
                });
        let (tool_label, tool_phase) = self.tool.clone();
        NormalizedProjection {
            agent_id,
            generation,
            source_epoch,
            last_sequence: self.last_sequence,
            activity: self.activity,
            activity_provenance: self.activity_provenance,
            wait: self.wait,
            wait_provenance: self.wait_provenance,
            turn_active: self.turn_active,
            turn_availability: self.turn_availability,
            turn_provenance: self.turn_provenance,
            turn_outcome: self.turn_outcome,
            todos_state: self.todos.0,
            todos_revision: self.todos.1,
            todos_count: self.todos.2,
            tool_label,
            tool_phase,
            tool_availability: self.tool_availability,
            tool_provenance: self.tool_provenance,
            last_message: self.message,
            message_availability: self.message_availability,
            message_provenance: self.message_provenance,
            source_terminal: self.source_terminal,
            terminal_availability: self.terminal_availability,
            source_terminal_provenance: self.source_terminal_provenance,
            source_error: self.source_error,
            error_availability: self.error_availability,
            source_error_provenance: self.source_error_provenance,
            session_ended: self.session_lifecycle == NativeSessionLifecycle::Ended,
            native_session_availability: self.native_session_availability,
            native_session_provenance: self.native_session_provenance,
            process_binding_availability: self.process_binding_availability,
            process_binding_provenance: self.process_binding_provenance,
            observation_health: self.health,
            process_alive: self.process_alive,
        }
    }
}

/// The deterministic reference reducer.
#[derive(Debug, Clone, Default)]
pub struct ReferenceReducer {
    state: ReducerState,
}

impl ReferenceReducer {
    /// Create a fresh reducer with no bound stream.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The current normalized projection.
    #[must_use]
    pub fn projection(&self) -> NormalizedProjection {
        self.state.project()
    }

    /// The current payload-preserving runtime observation.
    #[must_use]
    pub fn observation(&self) -> AgentObservation {
        self.state.observation.clone()
    }

    /// Read-only probe of whether a heartbeat for `identity` would be rejected
    /// as requiring a fresh snapshot-first stream, **without** mutating the
    /// reducer. This lets a caller classify a request (e.g. a snapshot
    /// publication) before it decides to apply the heartbeat, so validation
    /// can never observe a post-mutation state.
    ///
    /// Returns `true` only when the heartbeat is bound to the current stream
    /// identity but the stream is no longer live (a gap or disconnect already
    /// invalidated it). Any other precondition (no bound stream, identity
    /// mismatch) is not a "fresh snapshot required" condition and returns
    /// `false`; the caller validates those via other channels.
    #[must_use]
    pub fn fresh_snapshot_required(&self, identity: &ObservationIdentity) -> bool {
        let Some(bound) = self.state.identity.as_ref() else {
            return false;
        };
        bound == identity && self.state.stream_phase != StreamPhase::Live
    }

    /// Apply a snapshot: atomically replace the projection and bind/rebind the
    /// stream identity. A snapshot always resets observation health to live.
    ///
    /// Ordering semantics: a snapshot cursor C reflects all effects applied
    /// through C; the next event must be exactly C+1. The reducer therefore
    /// seeds `last_sequence` from `snapshot.cursor`, **not** from
    /// `snapshot.source_sequence`. The `source_sequence` is the highest source
    /// frame the producer consumed to build this snapshot, which may exceed the
    /// cursor when the snapshot synthesizes across frames; only the cursor
    /// reflects the contiguous event history a client must continue from.
    pub fn apply_snapshot(&mut self, snapshot: &Snapshot) {
        let next_process_identity = AgentProcessIdentity::from_snapshot(snapshot);
        let lifecycle_changed = match self.state.identity.as_ref() {
            None => true,
            Some(identity) => {
                identity.agent_id != snapshot.identity.agent_id
                    || identity.lifecycle_generation != snapshot.identity.lifecycle_generation
                    || self.state.agent_process_identity.as_ref() != Some(&next_process_identity)
            }
        };
        self.state.identity = Some(snapshot.identity.clone());
        self.state.agent_process_identity = Some(next_process_identity);
        self.state.last_sequence = snapshot.cursor;
        self.state.observation = observation_from_snapshot(snapshot);
        self.state.activity = project_activity(&snapshot.native_activity);
        self.state.activity_provenance = project_provenance(&snapshot.native_activity);
        self.state.wait = project_wait(&snapshot.current_wait);
        self.state.wait_provenance = project_provenance(&snapshot.current_wait);
        self.state.turn_active = project_turn_active(&snapshot.current_turn);
        self.state.turn_availability = project_optional_availability(&snapshot.current_turn);
        self.state.turn_provenance = project_provenance(&snapshot.current_turn);
        self.state.todos = project_todos(&snapshot.todos);
        self.state.message = project_message(&snapshot.last_displayed_assistant_message);
        self.state.message_availability =
            project_availability(&snapshot.last_displayed_assistant_message);
        self.state.message_provenance =
            project_provenance(&snapshot.last_displayed_assistant_message);
        self.state.tool = project_tool(&snapshot.last_created_tool_call);
        self.state.tool_availability = project_availability(&snapshot.last_created_tool_call);
        self.state.tool_provenance = project_provenance(&snapshot.last_created_tool_call);
        (self.state.source_terminal, self.state.terminal_availability) =
            project_source_terminal(&snapshot.source_terminal_state);
        self.state.source_terminal_provenance = project_provenance(&snapshot.source_terminal_state);
        self.state.source_error = project_presence(&snapshot.source_error_state);
        self.state.error_availability = project_availability(&snapshot.source_error_state);
        self.state.source_error_provenance = project_provenance(&snapshot.source_error_state);
        self.state.native_session_availability = AvailabilityProjection::Known;
        self.state.native_session_provenance = ProjectionProvenance::Authoritative;
        self.state.process_binding_availability = project_availability(&snapshot.process_binding);
        self.state.process_binding_provenance = project_provenance(&snapshot.process_binding);
        self.state.turn_outcome = None;
        self.state.session_lifecycle = NativeSessionLifecycle::Active;
        if lifecycle_changed {
            self.state.process_alive = None;
        }
        self.state.health = ObservationHealth::Live;
        self.state.stream_phase = StreamPhase::Live;
    }

    /// Apply a heartbeat: refresh observation health to live without advancing
    /// the sequence or mutating native state.
    pub fn apply_heartbeat(&mut self, heartbeat: &HeartbeatRecord) -> Result<(), ReducerError> {
        let Some(bound) = &self.state.identity else {
            return Err(ReducerError::SnapshotRequired);
        };
        if heartbeat.identity != *bound {
            return Err(ReducerError::IdentityMismatch);
        }
        if self.state.stream_phase != StreamPhase::Live {
            return Err(ReducerError::FreshSnapshotRequired);
        }
        self.state.health = ObservationHealth::Live;
        self.state.observation.health = ObservationHealth::Live;
        Ok(())
    }

    /// Apply an event under current-state gap/identity semantics.
    ///
    /// # Gap rejection contract
    ///
    /// A gap (`source_sequence > last + 1`) is intentionally rejected: the
    /// event applies **no native state mutation** (activity, wait, todos,
    /// tool, messages, turn, session), but it **does** mark observation
    /// health [`Stale`](ObservationHealth::Stale) and moves the stream into
    /// the `NeedsFreshSnapshot` phase. This is deliberate: a gap proves the
    /// transport lost frames, so the observer-owned health axis degrades even
    /// though the producer-owned native state must remain untouched until a
    /// fresh snapshot-first stream arrives. Health is observer-owned and
    /// orthogonal to native state; rejecting native mutation is therefore not
    /// a contradiction.
    ///
    /// Returns `Err` for a gap (stale health set) or identity mismatch (no
    /// health change) with no partial native mutation. Returns `Ok(())` for a
    /// duplicate or out-of-order event (no-op).
    pub fn apply_event(&mut self, record: &EventRecord) -> Result<(), ReducerError> {
        let Some(bound) = &self.state.identity else {
            return Err(ReducerError::SnapshotRequired);
        };
        if record.identity != *bound {
            return Err(ReducerError::IdentityMismatch);
        }
        if self.state.stream_phase != StreamPhase::Live {
            return Err(ReducerError::FreshSnapshotRequired);
        }
        let expected = self.state.last_sequence.saturating_add(1);
        if record.source_sequence == self.state.last_sequence {
            // Duplicate: no-op.
            return Ok(());
        }
        if record.source_sequence < expected {
            // Out-of-order (lower or equal): no-op.
            return Ok(());
        }
        if record.source_sequence > expected {
            // Gap: reject, mark stale, require fresh snapshot-first stream.
            self.state.health = ObservationHealth::Stale;
            self.state.observation.health = ObservationHealth::Stale;
            self.state.stream_phase = StreamPhase::NeedsFreshSnapshot;
            return Err(ReducerError::Gap {
                expected,
                actual: record.source_sequence,
            });
        }
        // Validate before mutation so illegal transitions are atomic and do not consume sequence.
        self.validate_transition(&record.event)?;
        self.apply_transition(&record.event);
        self.state.last_sequence = record.source_sequence;
        self.state.observation.last_sequence = record.source_sequence;
        Ok(())
    }

    /// Apply a transport disconnect: observation health degrades but native
    /// state is preserved until a fresh snapshot arrives.
    pub fn apply_disconnect(&mut self, permanent: bool) {
        let health = if permanent {
            ObservationHealth::Disconnected
        } else {
            ObservationHealth::Stale
        };
        self.state.health = health;
        self.state.observation.health = health;
        self.state.stream_phase = StreamPhase::NeedsFreshSnapshot;
    }

    /// Mark observation health stale (e.g. missed lease).
    pub fn mark_observation_stale(&mut self) {
        self.state.health = ObservationHealth::Stale;
        self.state.observation.health = ObservationHealth::Stale;
    }

    /// Mark a rejected, authenticated producer document as a protocol error.
    /// Native state remains available as historical context.
    pub fn mark_protocol_error(&mut self) {
        self.state.health = ObservationHealth::ProtocolError;
        self.state.observation.health = ObservationHealth::ProtocolError;
    }

    /// Apply observer-owned process liveness without erasing historical native state.
    pub fn set_process_alive(&mut self, alive: bool) {
        self.state.process_alive = Some(alive);
    }

    fn validate_transition(&self, event: &ObservationEvent) -> Result<(), ReducerError> {
        if let Some(transition) = self.state.session_lifecycle.mutation_error() {
            return Err(ReducerError::IllegalTransition { transition });
        }
        let illegal = match event {
            ObservationEvent::WaitResolved
                if matches!(
                    self.state.wait,
                    WaitProjection::NotWaiting
                        | WaitProjection::Unsupported
                        | WaitProjection::Unknown
                ) =>
            {
                Some("wait.resolved")
            }
            ObservationEvent::TurnStarted if self.state.turn_active => Some("turn.started"),
            ObservationEvent::TurnEnded { .. } if !self.state.turn_active => Some("turn.ended"),
            ObservationEvent::ToolCallPhaseChanged { tool }
                if !self.tool_phase_transition_is_legal(tool) =>
            {
                Some("tool_call.phase_changed")
            }
            _ => None,
        };
        if let Some(transition) = illegal {
            Err(ReducerError::IllegalTransition { transition })
        } else {
            Ok(())
        }
    }

    fn tool_phase_transition_is_legal(&self, tool: &ToolCallValue) -> bool {
        let Some(current_label) = self.state.tool.0.as_deref() else {
            return false;
        };
        if current_label != tool.label.as_str() {
            return true;
        }
        ToolLifecycle::from_phase(self.state.tool.1).allows(ToolLifecycle::from_phase(
            ToolPhaseProjection::from_phase(tool.phase),
        ))
    }
    /// Apply a single typed transition to the internal state.
    fn apply_transition(&mut self, event: &ObservationEvent) {
        match event {
            ObservationEvent::ActivityChanged { state } => {
                self.apply_activity_changed(*state);
            }
            ObservationEvent::WaitOpened { reason } => {
                self.state.wait = WaitProjection::from_reason(*reason);
                self.state.wait_provenance = ProjectionProvenance::Authoritative;
                self.state.observation.wait =
                    FieldState::known(Provenance::Authoritative, Some(Wait { reason: *reason }));
            }
            ObservationEvent::WaitResolved => {
                self.state.wait = WaitProjection::NotWaiting;
                self.state.wait_provenance = ProjectionProvenance::Authoritative;
                self.state.observation.wait = FieldState::known(Provenance::Authoritative, None);
            }
            ObservationEvent::TurnStarted => self.apply_turn_started(),
            ObservationEvent::TurnEnded { outcome } => self.apply_turn_ended(*outcome),
            ObservationEvent::TodosReplaced { todos } => {
                self.apply_todos_replaced(todos);
            }
            ObservationEvent::ToolCallCreated { tool } => {
                self.apply_tool_created(tool);
            }
            ObservationEvent::ToolCallPhaseChanged { tool } => {
                self.apply_tool_phase_changed(tool);
            }
            ObservationEvent::AssistantMessageDisplayed { message } => {
                self.state.message = MessagePresence::Present;
                self.state.message_availability = AvailabilityProjection::Known;
                self.state.message_provenance = ProjectionProvenance::Inferred;
                self.state.observation.last_message =
                    FieldState::known(Provenance::Inferred, message.clone());
            }
            ObservationEvent::SourceError { error } => {
                self.state.source_error = MessagePresence::Present;
                self.state.error_availability = AvailabilityProjection::Known;
                self.state.source_error_provenance = ProjectionProvenance::Authoritative;
                self.state.observation.error =
                    FieldState::known(Provenance::Authoritative, error.clone());
            }
            ObservationEvent::SessionEnded => {
                self.state.turn_active = false;
                self.state.turn_availability = AvailabilityProjection::KnownAbsent;
                self.state.turn_provenance = ProjectionProvenance::Authoritative;
                self.state.observation.turn = FieldState::known(Provenance::Authoritative, None);
                self.state.observation.session_ended = true;
                self.state.session_lifecycle = NativeSessionLifecycle::Ended;
            }
        }
    }

    fn apply_activity_changed(&mut self, state: NativeActivityState) {
        self.state.activity = match state {
            NativeActivityState::Idle => ActivityProjection::Idle,
            NativeActivityState::Thinking => ActivityProjection::Thinking,
            NativeActivityState::Acting => ActivityProjection::Acting,
        };
        self.state.activity_provenance = ProjectionProvenance::Authoritative;
        self.state.observation.activity =
            FieldState::known(Provenance::Authoritative, NativeActivityValue { state });
    }

    fn apply_turn_started(&mut self) {
        self.state.turn_active = true;
        self.state.turn_availability = AvailabilityProjection::Known;
        self.state.turn_provenance = ProjectionProvenance::Authoritative;
        self.state.turn_outcome = None;
        self.state.observation.turn = FieldState::known(
            Provenance::Authoritative,
            Some(CurrentTurn { elapsed_ms: 0 }),
        );
    }

    fn apply_turn_ended(&mut self, outcome: crate::domain::observation::TurnOutcome) {
        self.state.turn_active = false;
        self.state.turn_availability = AvailabilityProjection::KnownAbsent;
        self.state.turn_provenance = ProjectionProvenance::Authoritative;
        self.state.turn_outcome = Some(match outcome {
            crate::domain::observation::TurnOutcome::Completed => TurnOutcomeProjection::Completed,
            crate::domain::observation::TurnOutcome::Failed => TurnOutcomeProjection::Failed,
            crate::domain::observation::TurnOutcome::Cancelled => TurnOutcomeProjection::Cancelled,
        });
        // Activity is authoritative from the producer and must not be inferred
        // from the turn ending. A producer that goes idle sends its own
        // `activity.changed`; synthesizing one here would report activity the
        // source never claimed.
        self.state.observation.turn = FieldState::known(Provenance::Authoritative, None);
    }

    /// Apply a full todo replacement with the strictly-increasing revision
    /// rule: a replacement whose revision does not exceed the applied revision
    /// is ignored as stale.
    fn apply_todos_replaced(&mut self, todos: &TodoList) {
        let stale = self
            .state
            .todos
            .1
            .is_some_and(|applied| todos.revision <= applied);
        if !stale {
            self.state.todos = (
                if todos.items.is_empty() {
                    TodoProjection::AuthoritativeEmpty
                } else {
                    TodoProjection::Authoritative
                },
                Some(todos.revision),
                todos.items.len(),
            );
            self.state.observation.todos =
                FieldState::known(Provenance::Authoritative, todos.clone());
        }
    }

    /// Apply a tool creation: the most recently created tool becomes the
    /// headline (last-created-tool projection).
    fn apply_tool_created(&mut self, tool: &ToolCallValue) {
        self.state.tool = (
            Some(tool.label.as_str().to_string()),
            ToolPhaseProjection::from_phase(tool.phase),
        );
        self.state.tool_availability = AvailabilityProjection::Known;
        self.state.tool_provenance = ProjectionProvenance::Authoritative;
        self.state.observation.tool = FieldState::known(Provenance::Authoritative, tool.clone());
    }

    /// Apply a tool phase change to the current (last-created) tool. An update
    /// referencing a different label does not replace the headline tool.
    fn apply_tool_phase_changed(&mut self, tool: &ToolCallValue) {
        let is_current = self
            .state
            .tool
            .0
            .as_deref()
            .is_some_and(|label| label == tool.label.as_str());
        if is_current {
            self.state.tool.1 = ToolPhaseProjection::from_phase(tool.phase);
            self.state.tool_provenance = ProjectionProvenance::Authoritative;
            self.state.observation.tool =
                FieldState::known(Provenance::Authoritative, tool.clone());
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolLifecycle {
    Proposed,
    AwaitingApproval,
    Scheduled,
    Executing,
    Terminal,
    Unavailable,
}

impl ToolLifecycle {
    const fn from_phase(phase: ToolPhaseProjection) -> Self {
        match phase {
            ToolPhaseProjection::Proposed => Self::Proposed,
            ToolPhaseProjection::AwaitingApproval => Self::AwaitingApproval,
            ToolPhaseProjection::Scheduled => Self::Scheduled,
            ToolPhaseProjection::Executing => Self::Executing,
            ToolPhaseProjection::Succeeded
            | ToolPhaseProjection::Failed
            | ToolPhaseProjection::Cancelled => Self::Terminal,
            ToolPhaseProjection::Unknown | ToolPhaseProjection::Absent => Self::Unavailable,
        }
    }

    const fn allows(self, next: Self) -> bool {
        match self {
            Self::Proposed => !matches!(next, Self::Unavailable),
            Self::AwaitingApproval => !matches!(next, Self::Proposed | Self::Unavailable),
            Self::Scheduled => matches!(next, Self::Scheduled | Self::Executing | Self::Terminal),
            Self::Executing => matches!(next, Self::Executing | Self::Terminal),
            Self::Terminal | Self::Unavailable => false,
        }
    }
}
