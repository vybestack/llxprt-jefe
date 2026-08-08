//! Typed provider request lifecycle messages (issue #390 CW-10, Slice B).
//!
//! These messages connect the handle-free [`ProviderRequestState`] reducer to
//! the domain message bus. Inbound messages (Progress, Outcome, Error,
//! GenerationFailed) arrive from the supervisor in Slice C; outbound messages
//! (Invoke, Cancel, Confirm, Retry) originate from user intent in Slice D. The
//! reducer handler translates each message into a bounded call on the pure
//! [`ProviderRequestState`] and stages at most one closed [`ProviderEffect`]
//! per transition.
//!
//! [`ProviderRequestState`]: crate::state::provider_requests::ProviderRequestState
//! [`ProviderEffect`]: crate::domain::effects::ProviderEffect

use crate::domain::effects::ProviderRequestKey;
use crate::domain::{Id, TypedMap};
use crate::runtime::provider::protocol::{Outcome, ProgressPayload};
use crate::state::provider_requests::{ActionPolicy, UnavailableReason};

/// Provider request lifecycle messages.
#[derive(Debug, Clone)]
pub enum ProviderMessage {
    /// Request to invoke a provider action (CW10-02).
    Invoke {
        /// The host-side owner.
        owner: Id,
        /// The provider action to invoke.
        action_id: Id,
        /// Screen the action was invoked from.
        context_screen: Id,
        /// Screen instance the action was invoked from.
        context_instance: Id,
        /// Resource references currently in view.
        context_refs: TypedMap,
        /// Collected arguments.
        arguments: TypedMap,
        /// Immutable action policy derived from the action declaration.
        policy: ActionPolicy,
    },
    /// Progress from an in-flight request (CW10-07).
    Progress {
        /// The request/generation identity.
        key: ProviderRequestKey,
        /// The progress payload.
        payload: ProgressPayload,
    },
    /// Terminal outcome from an in-flight request (CW10-09). A
    /// `RequestHostConfirmation` outcome is the sole confirmation request
    /// path; `now_epoch` drives the confirmation TTL deterministically.
    Outcome {
        /// The request/generation identity.
        key: ProviderRequestKey,
        /// The terminal outcome.
        outcome: Outcome,
        /// Epoch seconds when the outcome arrived (deterministic).
        now_epoch: u64,
    },
    /// Terminal error from an in-flight request (CW10-09).
    Error {
        /// The request/generation identity.
        key: ProviderRequestKey,
        /// The error message.
        message: String,
    },
    /// Cancel an in-flight request (CW10-08/CW10-09).
    Cancel {
        /// The request/generation identity.
        key: ProviderRequestKey,
    },
    /// A generation failed (crash/EOF/protocol/timeout, CW10-09/CW10-10).
    GenerationFailed {
        /// The request/generation identity.
        key: ProviderRequestKey,
        /// Why it failed.
        reason: UnavailableReason,
    },
    /// Confirm a pending continuation (CW10-08).
    Confirm {
        /// The owner that staged the original invocation.
        owner: Id,
        /// The action being invoked.
        action_id: Id,
        /// Screen the action was invoked from.
        context_screen: Id,
        /// Screen instance the action was invoked from.
        context_instance: Id,
        /// Resource references currently in view.
        context_refs: TypedMap,
        /// The generation that requested confirmation.
        generation: u64,
        /// The single-use confirmation id.
        confirmation_id: Id,
        /// Declared continuation values.
        values: TypedMap,
        /// Epoch seconds when confirm was called (deterministic).
        now_epoch: u64,
    },
    /// Retry an old generation with a fresh one (CW10-10).
    Retry {
        /// The old request/generation identity.
        old_key: ProviderRequestKey,
        /// The host-side owner.
        owner: Id,
        /// The provider action to invoke.
        action_id: Id,
        /// Screen the action was invoked from.
        context_screen: Id,
        /// Screen instance the action was invoked from.
        context_instance: Id,
        /// Resource references currently in view.
        context_refs: TypedMap,
        /// Collected arguments.
        arguments: TypedMap,
        /// Immutable action policy derived from the action declaration.
        policy: ActionPolicy,
    },
}

impl ProviderMessage {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Invoke { .. } => "ProviderInvoke",
            Self::Progress { .. } => "ProviderProgress",
            Self::Outcome { .. } => "ProviderOutcome",
            Self::Error { .. } => "ProviderError",
            Self::Cancel { .. } => "ProviderCancel",
            Self::GenerationFailed { .. } => "ProviderGenerationFailed",
            Self::Confirm { .. } => "ProviderConfirm",
            Self::Retry { .. } => "ProviderRetry",
        }
    }
}
