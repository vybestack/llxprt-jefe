//! Transport-neutral observation semantic values (issue 476, J1 slice).
//!
//! This module owns the strongly typed, transport-neutral values that a parsed
//! JSP/1 snapshot produces. It has no project-internal dependency, no I/O, no
//! clock, and no credential concern. The `jsp::v1` wire parser converts into
//! these types only after complete validation, so a partially validated
//! payload can never escape as a domain value.
//!
//! The field-state algebra (decision 5) is closed and exhaustive: every
//! required snapshot field is either `Unsupported` or a supported provenance
//! (`Authoritative` or `Inferred`) combined with an availability
//! (`Unknown`, `Known(value)`, or `Degraded { last_value, as_of_ms,
//! diagnostic_code }`). `stale` is explicitly **not** a producer value; it is
//! a local observation-health overlay applied by Jefe transport in a later
//! slice. Producers cannot assert `stale`.

// ---------------------------------------------------------------------------
// Field-state type aliases exported for the typed snapshot contract.
// ---------------------------------------------------------------------------

/// Convenience alias for the process-binding field state. Binding evidence
/// only; process liveness stays Jefe-runtime-owned.
pub type ProcessBindingField = FieldState<ProcessBinding>;
/// Convenience alias for the native-activity field state.
pub type NativeActivityField = FieldState<NativeActivityValue>;
/// Convenience alias for the current-wait field state. `Known(None)` means
/// explicitly not waiting (no unresolved wait object).
pub type CurrentWaitField = FieldState<Option<Wait>>;
/// Convenience alias for the current-turn field state.
pub type CurrentTurnField = FieldState<CurrentTurn>;
/// Convenience alias for the todos field state.
pub type TodosField = FieldState<TodoList>;
/// Convenience alias for the last-displayed-assistant-message field state.
pub type LastMessageField = FieldState<DisplayedAssistantMessage>;
/// Convenience alias for the last-created-tool-call field state.
pub type LastToolField = FieldState<ToolCallValue>;
/// Convenience alias for the source-terminal-state field state. `Known(None)`
/// means a clean terminal with no error state.
pub type SourceTerminalField = FieldState<Option<SourceErrorValue>>;
/// Convenience alias for the source-error-state field state.
pub type SourceErrorField = FieldState<SourceErrorValue>;

// ---------------------------------------------------------------------------
// Provenance and availability
// ---------------------------------------------------------------------------

/// Producer-declared provenance for a supported field value.
///
/// `Authoritative` means the producer directly observed or authored the value.
/// `Inferred` means the producer derived it from other evidence. Both are
/// distinct from `Unsupported`, which is a separate field state rather than a
/// provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    Authoritative,
    Inferred,
}

impl Provenance {
    /// Map the closed wire label to a typed provenance.
    #[must_use]
    pub fn from_wire(label: &str) -> Option<Self> {
        match label {
            "authoritative" => Some(Self::Authoritative),
            "inferred" => Some(Self::Inferred),
            _ => None,
        }
    }

    /// The stable wire label for this provenance.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Authoritative => "authoritative",
            Self::Inferred => "inferred",
        }
    }
}

/// Availability of a supported field value.
///
/// `Known(value)` carries the concrete typed payload. `Unknown` means the
/// producer supports the field but has no current value. `Degraded` carries the
/// last accepted value plus a diagnostic anchor. The closed algebra guarantees
/// downstream `match` sites stay compiler-checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability<T> {
    Unknown,
    Known(T),
    Degraded {
        last_value: T,
        as_of_ms: u64,
        diagnostic_code: DiagnosticCode,
    },
}

impl<T> Availability<T> {
    /// The current known value, if any (degraded last-value does not count).
    #[must_use]
    pub fn known_value(&self) -> Option<&T> {
        match self {
            Self::Known(value) => Some(value),
            Self::Unknown | Self::Degraded { .. } => None,
        }
    }
}

/// Field state: either unsupported by the producer, or supported with a
/// provenance and availability. This is the closed state algebra from
/// decision 5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldState<T> {
    Unsupported,
    Supported {
        provenance: Provenance,
        availability: Availability<T>,
    },
}

impl<T> FieldState<T> {
    /// Build a supported-known field state with the given provenance.
    #[must_use]
    pub fn known(provenance: Provenance, value: T) -> Self {
        Self::Supported {
            provenance,
            availability: Availability::Known(value),
        }
    }

    /// Build a supported-unknown field state with the given provenance.
    #[must_use]
    pub fn unknown(provenance: Provenance) -> Self {
        Self::Supported {
            provenance,
            availability: Availability::Unknown,
        }
    }

    /// Whether this field is supported (not `Unsupported`).
    #[must_use]
    pub const fn is_supported(&self) -> bool {
        matches!(self, Self::Supported { .. })
    }
}

// ---------------------------------------------------------------------------
// Bounded newtype strings
// ---------------------------------------------------------------------------

/// A bounded producer-supplied diagnostic code. It is an opaque correlation
/// label only: Jefe never interprets it and never echoes it in parser
/// diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticCode(pub(crate) String);

impl DiagnosticCode {
    /// The diagnostic code as a borrowed string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A bounded opaque safe-ASCII identifier (agent id or source epoch). The
/// parser enforces 1..=128 bytes of visible ASCII in the closed grammar.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OpaqueId(pub(crate) String);

impl OpaqueId {
    /// The identifier as a borrowed string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A bounded repository identifier string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryRef(pub(crate) String);

impl RepositoryRef {
    /// The repository reference as a borrowed string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A bounded absolute path string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathRef(pub(crate) String);

impl PathRef {
    /// The path as a borrowed string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A bounded native agent-kind label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentKindLabel(pub(crate) String);

impl AgentKindLabel {
    /// The agent-kind label as a borrowed string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A bounded display name string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayName(pub(crate) String);

impl DisplayName {
    /// The display name as a borrowed string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A bounded free-text content string (assistant message, todo text, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedText(pub(crate) String);

impl BoundedText {
    /// The text as a borrowed string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A bounded tool label string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolLabel(pub(crate) String);

impl ToolLabel {
    /// The label as a borrowed string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A bounded diagnostic summary string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticSummary(pub(crate) String);

impl DiagnosticSummary {
    /// The summary as a borrowed string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// Semantic value structs
// ---------------------------------------------------------------------------

/// Identity of the live observation key. Repository/path/kind/pid/display-name
/// never participate in this key (decision 2).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObservationIdentity {
    /// Opaque Jefe agent identifier.
    pub agent_id: OpaqueId,
    /// Positive lifecycle generation (>=1).
    pub lifecycle_generation: u64,
    /// Producer/broker stream identity.
    pub source_epoch: OpaqueId,
}

/// Source/native-session identity metadata. Descriptive only; never part of
/// the live observation key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeSession {
    pub repository: RepositoryRef,
    pub path: PathRef,
    pub agent_kind: AgentKindLabel,
    pub pid: u32,
    pub display_name: DisplayName,
}

/// Producer-reported process binding metadata. This is binding evidence, not
/// liveness (decision 4): process liveness remains Jefe-runtime-owned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessBinding {
    pub pid: u32,
    pub started_at_ms: u64,
}

/// Native activity state, source-owned (decision 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeActivityState {
    Idle,
    Thinking,
    Acting,
}

impl NativeActivityState {
    /// Map the closed wire label to a typed activity state.
    #[must_use]
    pub fn from_wire(label: &str) -> Option<Self> {
        match label {
            "idle" => Some(Self::Idle),
            "thinking" => Some(Self::Thinking),
            "acting" => Some(Self::Acting),
            _ => None,
        }
    }

    /// The stable wire label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Thinking => "thinking",
            Self::Acting => "acting",
        }
    }
}

/// Native activity field value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeActivityValue {
    pub state: NativeActivityState,
}

/// An explicit unresolved wait reason (decision 7). Silence and elapsed time
/// never create waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitReason {
    Permission,
    Question,
    Elicitation,
    Choice,
    UserInput,
    Other,
}

impl WaitReason {
    /// Map the closed wire label to a typed wait reason.
    #[must_use]
    pub fn from_wire(label: &str) -> Option<Self> {
        match label {
            "permission" => Some(Self::Permission),
            "question" => Some(Self::Question),
            "elicitation" => Some(Self::Elicitation),
            "choice" => Some(Self::Choice),
            "user_input" => Some(Self::UserInput),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    /// The stable wire label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Permission => "permission",
            Self::Question => "question",
            Self::Elicitation => "elicitation",
            Self::Choice => "choice",
            Self::UserInput => "user_input",
            Self::Other => "other",
        }
    }
}

/// An explicit unresolved wait object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wait {
    pub reason: WaitReason,
}

/// Current turn runtime with an elapsed-millisecond anchor (decision 9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentTurn {
    pub elapsed_ms: u64,
}

/// A single todo entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoItem {
    pub text: BoundedText,
    pub completed: bool,
}

/// Full-replacement todo list with a strictly increasing revision (decision 8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoList {
    pub revision: u64,
    pub items: Vec<TodoItem>,
}

/// Last displayed assistant message, changing only at a user-visible display
/// or commit boundary (decision 7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayedAssistantMessage {
    pub content: BoundedText,
    pub committed_ms: u64,
}

/// Phase of the last-created tool call (decision 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPhase {
    Proposed,
    AwaitingApproval,
    Scheduled,
    Executing,
    Succeeded,
    Failed,
    Cancelled,
}

impl ToolPhase {
    /// Map the closed wire label to a typed tool phase. `stale` is rejected
    /// here because it is a local transport-health overlay, not a producer
    /// value (decision 5).
    #[must_use]
    pub fn from_wire(label: &str) -> Option<Self> {
        match label {
            "proposed" => Some(Self::Proposed),
            "awaiting_approval" => Some(Self::AwaitingApproval),
            "scheduled" => Some(Self::Scheduled),
            "executing" => Some(Self::Executing),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// The stable wire label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::AwaitingApproval => "awaiting_approval",
            Self::Scheduled => "scheduled",
            Self::Executing => "executing",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Last-created tool call field value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallValue {
    pub label: ToolLabel,
    pub phase: ToolPhase,
}

/// Source terminal/error state value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceErrorValue {
    pub summary: DiagnosticSummary,
    pub code: BoundedText,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_round_trips_closed_labels() {
        assert_eq!(
            Provenance::from_wire("authoritative"),
            Some(Provenance::Authoritative)
        );
        assert_eq!(
            Provenance::from_wire("inferred"),
            Some(Provenance::Inferred)
        );
        assert_eq!(Provenance::from_wire("bogus"), None);
        assert_eq!(Provenance::Authoritative.as_str(), "authoritative");
    }

    #[test]
    fn field_state_known_and_unknown_helpers() {
        let known = FieldState::known(Provenance::Authoritative, 42_u32);
        assert!(known.is_supported());
        let unknown = FieldState::<u32>::unknown(Provenance::Inferred);
        assert!(unknown.is_supported());
        let unsupported = FieldState::<u32>::Unsupported;
        assert!(!unsupported.is_supported());
    }

    #[test]
    fn availability_known_value_extracts_only_current() {
        let known = Availability::Known(7_u32);
        assert_eq!(known.known_value(), Some(&7));
        let degraded = Availability::Degraded {
            last_value: 7_u32,
            as_of_ms: 100,
            diagnostic_code: DiagnosticCode("X".to_string()),
        };
        assert_eq!(degraded.known_value(), None);
    }

    #[test]
    fn tool_phase_rejects_stale_overlay() {
        // stale is a local overlay, never a producer value.
        assert!(ToolPhase::from_wire("stale").is_none());
        assert_eq!(
            ToolPhase::from_wire("succeeded"),
            Some(ToolPhase::Succeeded)
        );
    }

    #[test]
    fn native_activity_closed_labels() {
        assert_eq!(
            NativeActivityState::from_wire("idle"),
            Some(NativeActivityState::Idle)
        );
        assert!(NativeActivityState::from_wire("stale").is_none());
    }

    #[test]
    fn wait_reason_closed_labels() {
        assert_eq!(
            WaitReason::from_wire("permission"),
            Some(WaitReason::Permission)
        );
        assert_eq!(
            WaitReason::from_wire("user_input"),
            Some(WaitReason::UserInput)
        );
        assert!(WaitReason::from_wire("silence").is_none());
    }
}

// ---------------------------------------------------------------------------
// Event semantics
// ---------------------------------------------------------------------------

/// How a turn finished (specification 18.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnOutcome {
    Completed,
    Failed,
    Cancelled,
}

impl TurnOutcome {
    /// Map the closed wire label to a typed outcome.
    #[must_use]
    pub fn from_wire(label: &str) -> Option<Self> {
        match label {
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// The stable wire label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// One authoritative native transition.
///
/// The inventory is closed: an unknown event type is rejected rather than
/// ignored, because silently dropping a transition leaves a status view
/// confidently wrong instead of visibly unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationEvent {
    /// Native activity changed.
    ActivityChanged { state: NativeActivityState },
    /// An explicit blocking request opened. Only this creates waiting.
    WaitOpened { reason: WaitReason },
    /// The open blocking request was answered natively.
    WaitResolved,
    /// A turn began; elapsed time anchors at zero.
    TurnStarted,
    /// A turn finished with an explicit outcome.
    TurnEnded { outcome: TurnOutcome },
    /// Full replacement of the structured todo list.
    TodosReplaced { todos: TodoList },
    /// A tool call was created. Creation order defines the current tool.
    ToolCallCreated { tool: ToolCallValue },
    /// The current tool call changed phase.
    ToolCallPhaseChanged { tool: ToolCallValue },
    /// A completed assistant reply became user-visible.
    AssistantMessageDisplayed { message: DisplayedAssistantMessage },
    /// The source reported an error state.
    SourceError { error: SourceErrorValue },
    /// The native session ended.
    SessionEnded,
}

/// A validated event record: identity, ordering, and one transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord {
    /// Live observation key this event belongs to.
    pub identity: ObservationIdentity,
    /// Ordering sequence, used for gap detection only.
    pub source_sequence: u64,
    /// Bridge observation timestamp.
    pub bridge_observed_ms: u64,
    /// The transition itself.
    pub event: ObservationEvent,
}

/// A validated heartbeat: the status source is alive but has no transition.
///
/// A heartbeat carries no sequence, so it can neither advance nor gap the
/// stream. A missed heartbeat means telemetry is stale, never that the agent
/// is idle or dead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatRecord {
    /// Live observation key this heartbeat belongs to.
    pub identity: ObservationIdentity,
    /// Bridge observation timestamp.
    pub bridge_observed_ms: u64,
}
