//! Data types for the handle-free provider request reducer
//! (issue #390 CW-10, Slice B).
//!
//! Pure data only: no reducer logic, no I/O, no clock. The reducer in
//! [`super`] composes these types; tests reach them through the public
//! re-exports of the parent module.

use crate::domain::effects::{ProviderInvocation, ProviderRequestKey};
use crate::domain::plugin::action::{ActionConfirmation, ActionOutcome};
use crate::domain::plugin::field::Field;
use crate::domain::{Id, TypedMap};
use crate::runtime::provider::protocol::{Outcome, ProgressPayload, ProgressTracker};

/// Maximum simultaneously active provider requests per session.
pub const MAX_ACTIVE_REQUESTS: usize = 16;

/// Confirmation token lifetime in seconds (5 minutes, CW10-08).
pub const CONFIRMATION_TTL_SECONDS: u64 = 300;

/// Immutable action policy derived exactly from the action declaration.
///
/// The reducer receives this as data and never reads the action registry.
/// A caller builds it from a validated
/// [`Action`](crate::domain::plugin::action::Action) declaration's
/// [`confirmation`](crate::domain::plugin::action::Action::confirmation),
/// [`allowed_outcomes`](crate::domain::plugin::action::Action::allowed_outcomes),
/// and [`destructive`](crate::domain::plugin::action::Action::destructive)
/// accessors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionPolicy {
    confirmation: ActionConfirmation,
    allowed_outcomes: Vec<ActionOutcome>,
    declared_routes: Vec<Id>,
    destructive: bool,
}

impl ActionPolicy {
    /// Build a policy from a validated action declaration's properties.
    #[must_use]
    pub fn new(
        confirmation: ActionConfirmation,
        allowed_outcomes: Vec<ActionOutcome>,
        destructive: bool,
    ) -> Self {
        Self {
            confirmation,
            allowed_outcomes,
            declared_routes: Vec::new(),
            destructive,
        }
    }

    /// Bind the package's immutable owner-scoped route declarations.
    #[must_use]
    pub fn with_declared_routes(mut self, declared_routes: Vec<Id>) -> Self {
        self.declared_routes = declared_routes;
        self
    }

    /// Whether the invoking package declared this exact route.
    #[must_use]
    pub fn allows_route(&self, route: &Id) -> bool {
        self.declared_routes.contains(route)
    }

    /// The declared confirmation mode.
    #[must_use]
    pub const fn confirmation(&self) -> ActionConfirmation {
        self.confirmation
    }

    /// Whether a particular outcome was declared by the action.
    #[must_use]
    pub fn allows(&self, outcome: ActionOutcome) -> bool {
        self.allowed_outcomes.contains(&outcome)
    }

    /// Whether the action is declared destructive.
    #[must_use]
    pub const fn destructive(&self) -> bool {
        self.destructive
    }
}

/// Why a provider request generation became unavailable (CW10-09/CW10-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnavailableReason {
    /// The provider process crashed.
    Crash,
    /// The provider stream closed unexpectedly.
    Eof,
    /// The provider violated the closed protocol (`PLG-E502`).
    Protocol,
    /// The invocation exceeded its timeout.
    Timeout,
}

impl UnavailableReason {
    /// A stable, operator-readable label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Crash => "provider process crashed",
            Self::Eof => "provider stream closed unexpectedly",
            Self::Protocol => "provider protocol violation (PLG-E502)",
            Self::Timeout => "provider invocation timed out",
        }
    }
}

/// Lifecycle status of one active request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RequestStatus {
    /// Invoked; awaiting progress or a terminal result.
    Live,
    /// At least one progress event was accepted.
    Progressing,
    /// A terminal outcome was received (first terminal wins).
    Completed(Outcome),
    /// A terminal error was received (first terminal wins).
    Failed(String),
    /// Cancelled by the host (first terminal wins).
    Cancelled,
    /// The generation became unavailable (crash/EOF/protocol/timeout).
    Unavailable(UnavailableReason),
}

impl RequestStatus {
    /// Whether this status is terminal — no further event may change it.
    pub(super) const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed(_) | Self::Failed(_) | Self::Cancelled | Self::Unavailable(_)
        )
    }

    /// Whether the request is still accepting progress and terminal results.
    pub(super) const fn is_live(&self) -> bool {
        matches!(self, Self::Live | Self::Progressing)
    }
}

/// One active provider request with its fixed positive generation.
///
/// Retains the original context references, arguments, and the immutable
/// action policy so the confirmation flow can build an exact invocation B
/// and so outcome acceptance can validate against the declared policy.
#[derive(Debug, Clone)]
pub struct ActiveRequest {
    /// The request/generation identity.
    pub(super) key: ProviderRequestKey,
    pub(super) context_screen: Id,
    pub(super) context_instance: Id,
    pub(super) context_refs: TypedMap,
    pub(super) arguments: TypedMap,
    pub(super) policy: ActionPolicy,
    pub(super) progress: ProgressTracker,
    pub(super) last_progress: Option<ProgressPayload>,
    pub(super) status: RequestStatus,
}

impl ActiveRequest {
    /// The request/generation identity.
    #[must_use]
    pub const fn key(&self) -> &ProviderRequestKey {
        &self.key
    }

    /// Screen from which this invocation was authorized.
    #[must_use]
    pub const fn context_screen(&self) -> &Id {
        &self.context_screen
    }

    /// Screen instance from which this invocation was authorized.
    #[must_use]
    pub const fn context_instance(&self) -> &Id {
        &self.context_instance
    }

    /// Resource references captured for this invocation.
    #[must_use]
    pub const fn context_refs(&self) -> &TypedMap {
        &self.context_refs
    }

    /// Arguments captured for this invocation.
    #[must_use]
    pub const fn arguments(&self) -> &TypedMap {
        &self.arguments
    }

    /// Immutable outcome and confirmation policy for this invocation.
    #[must_use]
    pub const fn policy(&self) -> &ActionPolicy {
        &self.policy
    }

    /// The latest accepted progress payload, if any.
    #[must_use]
    pub fn latest_progress(&self) -> Option<&ProgressPayload> {
        self.last_progress.as_ref()
    }

    /// Whether the request has reached a terminal state.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// The terminal outcome, when the request completed successfully.
    #[must_use]
    pub fn completed_outcome(&self) -> Option<&Outcome> {
        match &self.status {
            RequestStatus::Completed(outcome) => Some(outcome),
            _ => None,
        }
    }

    /// The unavailability reason, when the generation failed.
    #[must_use]
    pub fn unavailable_reason(&self) -> Option<UnavailableReason> {
        match self.status {
            RequestStatus::Unavailable(reason) => Some(reason),
            _ => None,
        }
    }

    /// Whether the request was cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        matches!(self.status, RequestStatus::Cancelled)
    }

    /// The terminal failure message, if any.
    #[must_use]
    pub fn failed_message(&self) -> Option<&str> {
        match &self.status {
            RequestStatus::Failed(message) => Some(message),
            _ => None,
        }
    }

    /// Whether at least one progress event was accepted.
    #[must_use]
    pub fn is_progressing(&self) -> bool {
        matches!(self.status, RequestStatus::Progressing)
    }
}

/// Owner/action/context/generation binding for a single-use confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ConfirmationBinding {
    pub(super) owner: Id,
    pub(super) action_id: Id,
    pub(super) context_screen: Id,
    pub(super) context_instance: Id,
    pub(super) context_refs: TypedMap,
    pub(super) generation: u64,
}

/// One pending single-use confirmation token (CW10-08).
///
/// Stores the exact UI fields (title, body, confirm label, schema) plus the
/// original invocation A arguments/context and the action policy so a
/// confirmed invocation B can carry the exact original data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PendingConfirmation {
    pub(super) binding: ConfirmationBinding,
    pub(super) confirmation_id: Id,
    pub(super) created_epoch: u64,
    pub(super) title: String,
    pub(super) body: String,
    pub(super) confirm_label: String,
    pub(super) continuation_schema: Vec<Field>,
    pub(super) arguments: TypedMap,
    pub(super) policy: ActionPolicy,
}

impl PendingConfirmation {
    /// Read-only view of this token's UI fields for the pure view projection.
    pub(super) fn view(&self) -> PendingConfirmationView<'_> {
        PendingConfirmationView {
            confirmation_id: &self.confirmation_id,
            title: &self.title,
            body: &self.body,
            confirm_label: &self.confirm_label,
            continuation_schema: &self.continuation_schema,
        }
    }
}

/// Read-only view of a pending confirmation's UI fields (CW10-08/CW10-13).
///
/// Borrows the title/body/confirm label/continuation schema from a
/// [`PendingConfirmation`] so the pure view projection can render the exact
/// declared values without owning or mutating reducer state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingConfirmationView<'a> {
    confirmation_id: &'a Id,
    title: &'a str,
    body: &'a str,
    confirm_label: &'a str,
    continuation_schema: &'a [Field],
}

impl<'a> PendingConfirmationView<'a> {
    /// Single-use confirmation identity declared by the provider.
    #[must_use]
    pub const fn confirmation_id(self) -> &'a Id {
        self.confirmation_id
    }

    /// The modal title declared by the provider.
    #[must_use]
    pub const fn title(self) -> &'a str {
        self.title
    }

    /// The modal body declared by the provider.
    #[must_use]
    pub const fn body(self) -> &'a str {
        self.body
    }

    /// The confirm-button label declared by the provider.
    #[must_use]
    pub const fn confirm_label(self) -> &'a str {
        self.confirm_label
    }

    /// The exact declared continuation field schema.
    #[must_use]
    pub const fn continuation_schema(self) -> &'a [Field] {
        self.continuation_schema
    }
}

/// Rejected provider-request reducer transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderRequestError {
    /// The active-request bound was reached.
    ActiveLimitExceeded {
        /// The inclusive maximum.
        limit: usize,
    },
    /// No active request matched the event's generation (old/unknown generation).
    UnknownGeneration,
    /// Data arrived after the request was already terminal (PLG-E502).
    ///
    /// The first terminal result is preserved; later bytes are a typed
    /// protocol violation rather than a silent no-op.
    PostTerminal,
    /// A progress monotonicity fault (PLG-E502); the generation is marked
    /// unavailable.
    ProgressFault,
    /// The confirmation token expired.
    Expired {
        /// Seconds since the token was created.
        elapsed: u64,
    },
    /// No pending confirmation matched the supplied identity.
    ConfirmationNotFound,
    /// The u64 generation counter exhausted.
    GenerationExhausted,
    /// A `RequestHostConfirmation` outcome violated the action policy
    /// (PLG-E502): the action did not declare `ProviderContinuation`,
    /// did not declare `RequestHostConfirmation`, or the destructive flag
    /// did not match.
    PolicyViolation,
    /// The terminal outcome was not declared by the action (PLG-E502).
    UndeclaredOutcome,
}

impl std::fmt::Display for ProviderRequestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ActiveLimitExceeded { limit } => {
                write!(formatter, "active provider requests are bounded at {limit}")
            }
            Self::UnknownGeneration => {
                formatter.write_str("no active request matches this generation")
            }
            Self::PostTerminal => {
                formatter.write_str("provider protocol violation: data after terminal (PLG-E502)")
            }
            Self::ProgressFault => formatter
                .write_str("provider protocol violation: progress monotonicity fault (PLG-E502)"),
            Self::Expired { elapsed } => write!(
                formatter,
                "confirmation expired after {elapsed}s (limit {CONFIRMATION_TTL_SECONDS}s)"
            ),
            Self::ConfirmationNotFound => {
                formatter.write_str("no pending confirmation matches this identity")
            }
            Self::GenerationExhausted => {
                formatter.write_str("provider generation counter exhausted")
            }
            Self::PolicyViolation => formatter
                .write_str("provider confirmation request violated action policy (PLG-E502)"),
            Self::UndeclaredOutcome => {
                formatter.write_str("provider outcome was not declared by the action (PLG-E502)")
            }
        }
    }
}

impl std::error::Error for ProviderRequestError {}

/// Successful invocation outcome carrying the effect data to stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvokeOutcome {
    /// The allocated request/generation identity.
    pub key: ProviderRequestKey,
    /// The invocation the supervisor executes after state is released.
    pub invocation: ProviderInvocation,
}

/// Result of cancelling an in-flight request (CW10-08/CW10-09).
///
/// Cancel is itself terminal. A cancel that arrives after the request already
/// reached a terminal state is an explicit no-effect result, consistent with
/// first-terminal semantics: it stages no `CancelRequest` effect and leaves
/// the first terminal result authoritative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelOutcome {
    /// The request was live and is now cancelled; stage a `CancelRequest`.
    Cancelled {
        /// The request that was cancelled.
        key: ProviderRequestKey,
    },
    /// The request was already terminal; no effect is staged.
    AlreadyTerminal {
        /// The request that was already terminal.
        key: ProviderRequestKey,
    },
}

/// Successful confirmation outcome carrying the fresh invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmOutcome {
    /// The new generation allocated for invocation B.
    pub key: ProviderRequestKey,
    /// The fresh invocation carrying the exact typed continuation.
    pub invocation: ProviderInvocation,
}

/// Input for an invocation request.
#[derive(Debug, Clone)]
pub struct InvokeInput<'a> {
    /// The host-side owner.
    pub owner: &'a Id,
    /// The provider action to invoke.
    pub action_id: &'a Id,
    /// Screen the action was invoked from.
    pub context_screen: &'a Id,
    /// Screen instance the action was invoked from.
    pub context_instance: &'a Id,
    /// Resource references currently in view.
    pub context_refs: &'a TypedMap,
    /// Collected arguments.
    pub arguments: &'a TypedMap,
    /// Immutable action policy derived from the action declaration.
    pub policy: &'a ActionPolicy,
}

/// Input for confirming a pending continuation.
#[derive(Debug, Clone)]
pub struct ConfirmInput<'a> {
    /// The owner that staged the original invocation.
    pub owner: &'a Id,
    /// The action being invoked.
    pub action_id: &'a Id,
    /// Screen the action was invoked from.
    pub context_screen: &'a Id,
    /// Screen instance the action was invoked from.
    pub context_instance: &'a Id,
    /// Resource references currently in view.
    pub context_refs: &'a TypedMap,
    /// The generation that requested confirmation.
    pub generation: u64,
    /// The single-use confirmation id.
    pub confirmation_id: &'a Id,
    /// Declared continuation values.
    pub values: &'a TypedMap,
}
