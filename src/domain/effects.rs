//! Closed post-commit effect contract for reducer transitions (issue #381).
//!
//! These types are pure data: no closure, service, adapter handle, generic
//! payload, bus, or queue may appear in any variant. The reducer commits at
//! most [`MAX_TRANSITION_EFFECTS`] ordered effects per transition, the root
//! shell executes them only after all state access is released, and every
//! completion must match its pending [`Correlation`] exactly or change
//! nothing.

use std::num::NonZeroU8;

use crate::agent_candidate::CandidateResolution;

use super::action_registry::ActionAvailability;
use super::agent_definition::{AgentDefinition, Availability};
use super::{AgentId, Id, StateV2, TypedMap};

/// Maximum ordered effects (including completion-produced follow-ups) that
/// one committed transition may carry.
pub const MAX_TRANSITION_EFFECTS: usize = 64;

/// Unique identifier for one issued effect within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CorrelationId(u64);

impl CorrelationId {
    /// Wrap a session-unique correlation number.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the raw correlation number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Closed effect family inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EffectFamily {
    Persistence,
    AgentProbe,
    Runtime,
    GitHub,
    SshTmux,
    Provider,
    ClipboardUrl,
    Timer,
}

/// Semantic identity of an operation within one family.
///
/// Two pending effects with the same semantic key describe the same logical
/// operation; a newer issue supersedes the older pending record.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SemanticKey {
    family: EffectFamily,
    subject: String,
}

impl SemanticKey {
    /// Build a semantic key for `subject` within `family`.
    #[must_use]
    pub fn new(family: EffectFamily, subject: &str) -> Self {
        Self {
            family,
            subject: subject.to_owned(),
        }
    }

    /// The owning effect family.
    #[must_use]
    pub const fn family(&self) -> EffectFamily {
        self.family
    }

    /// The operation subject within the family.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }
}

/// Exact five-field identity that a completion must match to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correlation {
    pub correlation_id: CorrelationId,
    pub owner: Id,
    pub screen_generation: u64,
    pub activation_generation: u64,
    pub semantic_key: SemanticKey,
}

impl Correlation {
    /// Report whether `other` matches on all five identity fields.
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        self == other
    }
}

/// Retry policy for an issued effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryPolicy {
    /// Execute exactly once.
    Never,
    /// Retry a statically idempotent query up to `max_attempts` total tries.
    IdempotentQuery { max_attempts: NonZeroU8 },
}

impl RetryPolicy {
    /// Construct an idempotent-query policy with 1..=3 total attempts.
    ///
    /// # Errors
    ///
    /// Returns [`RetryPolicyError`] when `attempts` is outside 1..=3.
    pub fn idempotent_query(attempts: u8) -> Result<Self, RetryPolicyError> {
        NonZeroU8::new(attempts)
            .filter(|value| value.get() <= 3)
            .map(|max_attempts| Self::IdempotentQuery { max_attempts })
            .ok_or(RetryPolicyError::AttemptsOutOfRange { attempts })
    }
}

/// Rejected retry-policy construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryPolicyError {
    /// Total attempts must be between 1 and 3 inclusive.
    AttemptsOutOfRange { attempts: u8 },
}

impl std::fmt::Display for RetryPolicyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AttemptsOutOfRange { attempts } => {
                write!(formatter, "retry attempts must be 1..=3, got {attempts}")
            }
        }
    }
}

impl std::error::Error for RetryPolicyError {}

/// Classification of a failed effect execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectErrorKind {
    Validation,
    Unavailable,
    Io,
    Conflict,
    Rejected,
}

/// Typed, redacted failure delivered inside a completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectError {
    pub kind: EffectErrorKind,
    pub retryable: bool,
    pub redacted_detail: String,
}

impl EffectError {
    /// Build a typed effect error with an already-redacted detail.
    #[must_use]
    pub fn new(kind: EffectErrorKind, retryable: bool, redacted_detail: &str) -> Self {
        Self {
            kind,
            retryable,
            redacted_detail: redacted_detail.to_owned(),
        }
    }
}

/// Typed completion delivered back to the reducer for one issued effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion<T> {
    pub correlation: Correlation,
    pub result: Result<T, EffectError>,
}

/// Durable-state persistence operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistenceEffect {
    /// Persist a complete pre-commit durable candidate at `revision`.
    PersistState {
        candidate: Box<StateV2>,
        revision: u64,
    },
}

/// Persistence completion payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceResponse {
    /// The candidate became the durable authority at `revision`.
    Persisted { revision: u64 },
    /// A newer pending revision superseded this candidate before rename.
    Superseded { revision: u64 },
}

/// One resolved definition probe requested by the availability reducer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentAvailabilityProbe {
    pub definition: Box<AgentDefinition>,
    pub resolution: CandidateResolution,
    pub generation: u64,
}

/// Local agent probe operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeEffect {
    /// Check whether the agent's runtime session is currently alive.
    CheckAgentLiveness {
        agent_id: AgentId,
        session_id: String,
    },
    /// Probe one physically resolved definition for identity and capabilities.
    CheckAgentAvailability(AgentAvailabilityProbe),
}

/// Agent probe completion payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeResponse {
    Liveness {
        alive: bool,
    },
    Availability {
        availability: Box<Availability>,
        generation: u64,
    },
}

/// Local runtime session operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeEffect {
    AttachSession { agent_id: AgentId },
    KillSession { agent_id: AgentId },
    RelaunchSession { agent_id: AgentId },
}

/// Runtime completion payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeResponse {
    Attached,
    Killed,
    Relaunched,
}

/// GitHub data refresh operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitHubEffect {
    RefreshIssues { repository: String },
    RefreshPullRequests { repository: String },
}

/// GitHub completion payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitHubResponse {
    IssuesRefreshed { items: u64 },
    PullRequestsRefreshed { items: u64 },
}

/// Remote session presence operations over SSH-managed multiplexers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SshTmuxEffect {
    ProbeRemoteSession { target: String, session_id: String },
}

/// SSH/tmux completion payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshTmuxResponse {
    SessionPresence { present: bool },
}

/// Identity of one provider request across its whole lifecycle
/// (issue #390 CW-10, Slice B).
///
/// Owner, action, and the fixed positive generation together name exactly one
/// invocation; a later generation is a different request.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProviderRequestKey {
    /// The host-side owner that staged the invocation.
    pub owner: Id,
    /// The provider action being invoked.
    pub action_id: Id,
    /// The fixed positive generation allocated for this invocation.
    pub generation: u64,
}

/// The typed continuation a confirmed second invocation carries
/// (issue #390 CW-10, Slice B).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderContinuation {
    /// The single-use confirmation id the provider issued.
    pub confirmation_id: Id,
    /// Whether the operator approved.
    pub approved: bool,
    /// Declared continuation values.
    pub values: TypedMap,
}

/// One host-to-provider action invocation (issue #390 CW-10, Slice B).
///
/// Pure post-commit effect data: the reducer commits request/generation
/// ownership, releases state, then the supervisor (Slice C) owns the process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderInvocation {
    /// The request/generation identity.
    pub key: ProviderRequestKey,
    /// Collected invocation arguments.
    pub arguments: TypedMap,
    /// Screen the action was invoked from.
    pub context_screen: Id,
    /// Screen instance the action was invoked from.
    pub context_instance: Id,
    /// Resource references currently in view.
    pub context_refs: TypedMap,
    /// Continuation, present only for a confirmed second invocation.
    pub continuation: Option<ProviderContinuation>,
}

/// Provider/package availability operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderEffect {
    ProbePackageAvailability {
        selector: String,
    },
    ProjectActionAvailability {
        entries: Vec<ActionAvailability>,
    },
    /// Start one one-shot provider invocation (issue #390 CW-10, Slice B).
    InvokeAction {
        invocation: ProviderInvocation,
    },
    /// Send a cancel for an in-flight request (issue #390 CW-10, Slice B).
    CancelRequest {
        key: ProviderRequestKey,
    },
}

/// Provider completion payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderResponse {
    PackageAvailability {
        available: bool,
    },
    ActionAvailability {
        entries: Vec<ActionAvailability>,
    },
    /// The invocation started and its envelope was sent (CW-10 Slice B).
    Invoked {
        key: ProviderRequestKey,
    },
    /// A cancel was sent to an in-flight request (CW-10 Slice B).
    Cancelled {
        key: ProviderRequestKey,
    },
}

/// Clipboard and URL hand-off operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardUrlEffect {
    CopyText { text: String },
    OpenUrl { url: String },
}

/// Clipboard/URL completion payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardUrlResponse {
    Copied,
    Opened,
}

/// Shell-owned scheduled wakeups; scheduler handles never enter state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerEffect {
    Wakeup { after_ms: u64 },
}

/// Timer completion payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerResponse {
    Elapsed,
}

/// One ordered post-commit effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Persistence(PersistenceEffect),
    AgentProbe(ProbeEffect),
    Runtime(RuntimeEffect),
    GitHub(GitHubEffect),
    SshTmux(SshTmuxEffect),
    Provider(ProviderEffect),
    ClipboardUrl(ClipboardUrlEffect),
    Timer(TimerEffect),
}

impl Effect {
    /// Report the closed family this effect belongs to.
    #[must_use]
    pub const fn family(&self) -> EffectFamily {
        match self {
            Self::Persistence(_) => EffectFamily::Persistence,
            Self::AgentProbe(_) => EffectFamily::AgentProbe,
            Self::Runtime(_) => EffectFamily::Runtime,
            Self::GitHub(_) => EffectFamily::GitHub,
            Self::SshTmux(_) => EffectFamily::SshTmux,
            Self::Provider(_) => EffectFamily::Provider,
            Self::ClipboardUrl(_) => EffectFamily::ClipboardUrl,
            Self::Timer(_) => EffectFamily::Timer,
        }
    }
}

/// One closed response payload per effect family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectResponse {
    Persistence(PersistenceResponse),
    AgentProbe(ProbeResponse),
    Runtime(RuntimeResponse),
    GitHub(GitHubResponse),
    SshTmux(SshTmuxResponse),
    Provider(ProviderResponse),
    ClipboardUrl(ClipboardUrlResponse),
    Timer(TimerResponse),
}

impl EffectResponse {
    /// Report the closed family this response belongs to.
    #[must_use]
    pub const fn family(&self) -> EffectFamily {
        match self {
            Self::Persistence(_) => EffectFamily::Persistence,
            Self::AgentProbe(_) => EffectFamily::AgentProbe,
            Self::Runtime(_) => EffectFamily::Runtime,
            Self::GitHub(_) => EffectFamily::GitHub,
            Self::SshTmux(_) => EffectFamily::SshTmux,
            Self::Provider(_) => EffectFamily::Provider,
            Self::ClipboardUrl(_) => EffectFamily::ClipboardUrl,
            Self::Timer(_) => EffectFamily::Timer,
        }
    }
}

/// One committed effect paired with its exact correlation and retry policy.
///
/// This is the executor's input: the correlation was registered in the
/// pending ledger before commit, so the later completion can be matched
/// exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedEffect {
    pub effect: Effect,
    pub correlation: Correlation,
    pub retry: RetryPolicy,
}

/// Typed completion for one issued effect across all families.
///
/// The response family always equals the correlation's semantic-key family;
/// the executor converts any family mismatch into a typed validation error
/// before delivery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectCompletion {
    pub correlation: Correlation,
    pub result: Result<EffectResponse, EffectError>,
}

impl EffectCompletion {
    /// The exact pending identity this completion answers.
    #[must_use]
    pub const fn correlation(&self) -> &Correlation {
        &self.correlation
    }

    /// The effect family of this completion.
    #[must_use]
    pub const fn family(&self) -> EffectFamily {
        self.correlation.semantic_key.family()
    }

    /// The typed error, when the effect failed.
    #[must_use]
    pub const fn error(&self) -> Option<&EffectError> {
        match &self.result {
            Ok(_) => None,
            Err(error) => Some(error),
        }
    }
}
