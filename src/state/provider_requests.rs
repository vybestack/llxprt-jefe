//! Handle-free provider request reducer state (issue #390 CW-10, Slice B).
//!
//! This module owns request/generation lifecycle state and nothing else: no
//! process handle, no pipe, no timer handle, no clock read, and no direct
//! I/O. Every timestamp is supplied by the caller as epoch seconds so the
//! reducer stays deterministic. Selection and durable configuration live
//! outside this state — a runtime failure marks only the current generation
//! unavailable and never touches what the persistence layer owns.
//!
//! The reducer enforces the CW-10 bound of at most [`MAX_ACTIVE_REQUESTS`]
//! simultaneously active requests. The staged closed [`ProviderEffect`] is the
//! single post-commit outbound model; the real bounded outbound queue belongs
//! inside the Slice C supervisor and is not duplicated here. Progress is
//! validated by the Slice-A [`ProgressTracker`]; the first terminal result
//! wins and every later event for that request is a typed `PLG-E502` protocol
//! violation rather than a silent no-op; output from an old or unknown
//! generation changes nothing.

use crate::domain::Id;
use crate::domain::effects::{ProviderContinuation, ProviderInvocation, ProviderRequestKey};
use crate::domain::plugin::action::{ActionConfirmation, ActionOutcome};
use crate::domain::plugin_config::validate_fields;
use crate::runtime::provider::protocol::Outcome;

use super::provider_request_model::{ConfirmationBinding, PendingConfirmation, RequestStatus};

// Re-export public data types so consumers reach them through this module.
pub use super::provider_request_model::{
    ActionPolicy, ActiveRequest, CONFIRMATION_TTL_SECONDS, CancelOutcome, ConfirmInput,
    ConfirmOutcome, InvokeInput, InvokeOutcome, MAX_ACTIVE_REQUESTS, PendingConfirmationView,
    ProviderConfirmationIdentity, ProviderRequestError, UnavailableReason,
};

/// Allocate the next generation, failing typed on u64 exhaustion.
///
/// Pure helper extracted so the exhaustion boundary is testable without a
/// test-only production backdoor.
pub(super) fn next_generation(current: u64) -> Result<u64, ProviderRequestError> {
    current
        .checked_add(1)
        .ok_or(ProviderRequestError::GenerationExhausted)
}

fn confirmation_matches(
    pending: &PendingConfirmation,
    identity: &ProviderConfirmationIdentity,
) -> bool {
    pending.confirmation_id == *identity.confirmation_id()
        && pending.binding.owner == *identity.owner()
        && pending.binding.action_id == *identity.action_id()
        && pending.binding.generation == identity.generation()
        && pending.binding.context_screen == *identity.context_screen()
        && pending.binding.context_instance == *identity.context_instance()
}

/// Fields borrowed from a `RequestHostConfirmation` outcome for confirmation
/// token creation.
struct ConfirmationFields<'a> {
    confirmation_id: &'a Id,
    title: &'a str,
    body: &'a str,
    confirm_label: &'a str,
    continuation_schema: &'a [crate::domain::plugin::field::Field],
}

pub(crate) fn validate_continuation_schema(
    schema: &[crate::domain::plugin::field::Field],
) -> Result<(), ProviderRequestError> {
    let host_decision_id = crate::domain::Id::internal(crate::domain::InternalId::OverlayDecision);
    for (index, field) in schema.iter().enumerate() {
        if field.id() == &host_decision_id
            || schema[..index].iter().any(|prior| prior.id() == field.id())
        {
            return Err(ProviderRequestError::InvalidContinuationSchema);
        }
    }
    Ok(())
}

/// Validate one outcome against the immutable action policy.
///
/// Outcome kinds are checked against the declared allowed outcomes. The
/// `RequestHostConfirmation` outcome additionally requires the action to have
/// declared `ProviderContinuation` and a matching destructive flag. Panel and
/// config-migration messages use their dedicated issue #391 message bodies and
/// never reach this action-outcome validator.
fn validate_outcome(policy: &ActionPolicy, outcome: &Outcome) -> Result<(), ProviderRequestError> {
    match outcome {
        Outcome::RequestHostConfirmation {
            destructive,
            continuation_schema,
            ..
        } => {
            if policy.confirmation() != ActionConfirmation::ProviderContinuation
                || !policy.allows(ActionOutcome::RequestHostConfirmation)
                || *destructive != policy.destructive()
            {
                return Err(ProviderRequestError::PolicyViolation);
            }
            validate_continuation_schema(continuation_schema)
        }
        Outcome::Navigate { .. } => {
            if policy.allows(ActionOutcome::NavigateDeclaredRoute) {
                Ok(())
            } else {
                Err(ProviderRequestError::UndeclaredOutcome)
            }
        }
        Outcome::Refresh { .. } => {
            if policy.allows(ActionOutcome::RefreshCurrentResource) {
                Ok(())
            } else {
                Err(ProviderRequestError::UndeclaredOutcome)
            }
        }
        Outcome::Notice { .. } => {
            if policy.allows(ActionOutcome::Notice) {
                Ok(())
            } else {
                Err(ProviderRequestError::UndeclaredOutcome)
            }
        }
    }
}

/// Handle-free provider request reducer state.
///
/// Owns bounded request/generation/progress/confirmation runtime state. No
/// process handle, pipe, timer, clock, or persisted field lives here.
#[derive(Debug, Clone, Default)]
pub struct ProviderRequestState {
    requests: Vec<ActiveRequest>,
    pending_confirmations: Vec<PendingConfirmation>,
    next_generation: u64,
}

impl ProviderRequestState {
    /// Construct empty state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of active requests (including terminal ones not yet drained).
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.requests.len()
    }

    /// Number of live (non-terminal) requests — the ones that consume capacity
    /// against [`MAX_ACTIVE_REQUESTS`] (S18). Terminal requests do not count:
    /// they remain visible until drained but do not block the seventeenth
    /// sequential completed request.
    #[must_use]
    pub fn live_count(&self) -> usize {
        self.requests
            .iter()
            .filter(|req| !req.is_terminal())
            .count()
    }

    /// Number of pending single-use confirmation tokens.
    #[must_use]
    pub fn pending_confirmation_count(&self) -> usize {
        self.pending_confirmations.len()
    }

    /// Read the oldest pending confirmation regardless of owner context.
    #[must_use]
    pub fn first_pending_confirmation(&self) -> Option<PendingConfirmationView<'_>> {
        self.pending_confirmations
            .first()
            .map(PendingConfirmation::view)
    }

    /// Consume one exact pending confirmation without starting invocation B.
    pub fn cancel_confirmation(&mut self, identity: &ProviderConfirmationIdentity) -> bool {
        let Some(index) = self
            .pending_confirmations
            .iter()
            .position(|pending| confirmation_matches(pending, identity))
        else {
            return false;
        };
        self.pending_confirmations.remove(index);
        true
    }

    /// Read one exact pending provider confirmation.
    #[must_use]
    pub fn pending_confirmation_view(
        &self,
        identity: &ProviderConfirmationIdentity,
    ) -> Option<PendingConfirmationView<'_>> {
        self.pending_confirmations
            .iter()
            .find(|pending| confirmation_matches(pending, identity))
            .map(PendingConfirmation::view)
    }

    /// Read the oldest queued confirmation owned by one exact screen instance.
    #[must_use]
    pub fn first_pending_confirmation_for(
        &self,
        context_screen: &str,
        context_instance: &str,
    ) -> Option<PendingConfirmationView<'_>> {
        self.pending_confirmations
            .iter()
            .find(|pending| {
                pending.binding.context_screen.as_str() == context_screen
                    && pending.binding.context_instance.as_str() == context_instance
            })
            .map(PendingConfirmation::view)
    }

    /// Whether no request is active.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.requests.is_empty()
    }

    /// Read-only access to active requests.
    #[must_use]
    pub fn requests(&self) -> &[ActiveRequest] {
        &self.requests
    }

    /// Find an active request by its key.
    #[must_use]
    pub fn request(&self, key: &ProviderRequestKey) -> Option<&ActiveRequest> {
        self.requests.iter().find(|req| &req.key == key)
    }

    /// Remove terminal requests that have been observed, freeing active slots.
    ///
    /// Returns how many were removed. Terminal requests remain until drained so
    /// the UI can show the final result; this is the explicit eviction call.
    pub fn drain_terminal(&mut self) -> usize {
        let before = self.requests.len();
        self.requests.retain(|req| !req.is_terminal());
        before - self.requests.len()
    }

    /// Remove observed terminal requests owned by one exact screen instance.
    pub fn drain_terminal_for(&mut self, context_screen: &str, context_instance: &str) -> usize {
        let before = self.requests.len();
        self.requests.retain(|request| {
            !request.is_terminal()
                || request.context_screen.as_str() != context_screen
                || request.context_instance.as_str() != context_instance
        });
        before - self.requests.len()
    }

    /// Allocate the next fixed positive generation.
    fn fresh_generation(&mut self) -> Result<u64, ProviderRequestError> {
        let value = next_generation(self.next_generation)?;
        self.next_generation = value;
        Ok(value)
    }

    /// Attempt to invoke a provider action (CW10-02/CW10-06).
    ///
    /// Allocates a fresh generation, registers the active request (retaining
    /// the original context refs, arguments, and immutable action policy), and
    /// returns the invocation data the supervisor needs. No outbound queue is
    /// duplicated here; the caller stages the closed `InvokeAction` effect.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderRequestError::ActiveLimitExceeded`] when the active
    /// bound is reached, or [`ProviderRequestError::GenerationExhausted`]
    /// when the u64 counter is exhausted.
    pub fn invoke(
        &mut self,
        input: InvokeInput<'_>,
    ) -> Result<InvokeOutcome, ProviderRequestError> {
        if self.live_count() >= MAX_ACTIVE_REQUESTS {
            return Err(ProviderRequestError::ActiveLimitExceeded {
                limit: MAX_ACTIVE_REQUESTS,
            });
        }
        let generation = self.fresh_generation()?;
        let key = ProviderRequestKey {
            owner: input.owner.clone(),
            action_id: input.action_id.clone(),
            generation,
        };
        let invocation = ProviderInvocation {
            key: key.clone(),
            arguments: input.arguments.clone(),
            context_screen: input.context_screen.clone(),
            context_instance: input.context_instance.clone(),
            context_refs: input.context_refs.clone(),
            continuation: None,
        };
        self.requests.push(ActiveRequest {
            key: key.clone(),
            context_screen: input.context_screen.clone(),
            context_instance: input.context_instance.clone(),
            context_refs: input.context_refs.clone(),
            arguments: input.arguments.clone(),
            policy: input.policy.clone(),
            progress: crate::runtime::provider::protocol::ProgressTracker::new(),
            last_progress: None,
            status: RequestStatus::Live,
        });
        Ok(InvokeOutcome { key, invocation })
    }

    /// Record a progress event for a request (CW10-07).
    ///
    /// Old-generation or unknown events change nothing. A monotonicity fault
    /// marks the generation unavailable and is observable as `PLG-E502`. The
    /// first terminal wins: progress after a terminal result is a typed
    /// `PLG-E502` post-terminal violation, not a silent no-op.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderRequestError::UnknownGeneration`],
    /// [`ProviderRequestError::PostTerminal`], or
    /// [`ProviderRequestError::ProgressFault`].
    pub fn record_progress(
        &mut self,
        key: &ProviderRequestKey,
        payload: crate::runtime::provider::protocol::ProgressPayload,
    ) -> Result<(), ProviderRequestError> {
        let index = self
            .requests
            .iter()
            .position(|req| &req.key == key)
            .ok_or(ProviderRequestError::UnknownGeneration)?;
        if !self.requests[index].status.is_live() {
            return Err(ProviderRequestError::PostTerminal);
        }
        if self.requests[index]
            .progress
            .observe(payload.sequence, payload.completed, payload.total)
            .is_err()
        {
            self.requests[index].status = RequestStatus::Unavailable(UnavailableReason::Protocol);
            return Err(ProviderRequestError::ProgressFault);
        }
        self.requests[index].last_progress = Some(payload);
        if self.requests[index].status == RequestStatus::Live {
            self.requests[index].status = RequestStatus::Progressing;
        }
        Ok(())
    }

    /// Record a terminal outcome, validating against the declared action
    /// policy before committing (CW10-09).
    ///
    /// A `RequestHostConfirmation` outcome is the sole confirmation request
    /// path: it validates that the action declared `ProviderContinuation`,
    /// declared `RequestHostConfirmation`, and that the destructive flag
    /// matches the policy, then persists the exact title/body/confirm label/
    /// schema plus owner/action/context/generation for UI. Other outcome kinds
    /// are validated against the declared allowed outcomes. Navigation and
    /// refresh details may be revalidated by the later host adapter. Panel and
    /// config-migration messages use dedicated message bodies and never reach
    /// this action-outcome reducer.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderRequestError::UnknownGeneration`],
    /// [`ProviderRequestError::PostTerminal`],
    /// [`ProviderRequestError::PolicyViolation`], or
    /// [`ProviderRequestError::UndeclaredOutcome`].
    pub fn record_outcome(
        &mut self,
        key: &ProviderRequestKey,
        outcome: Outcome,
        now_epoch: u64,
    ) -> Result<(), ProviderRequestError> {
        let index = self
            .requests
            .iter()
            .position(|req| &req.key == key)
            .ok_or(ProviderRequestError::UnknownGeneration)?;
        if self.requests[index].status.is_terminal() {
            return Err(ProviderRequestError::PostTerminal);
        }

        let policy = self.requests[index].policy.clone();
        if let Err(error) = validate_outcome(&policy, &outcome) {
            self.requests[index].status = RequestStatus::Unavailable(UnavailableReason::Protocol);
            return Err(error);
        }

        if let Outcome::RequestHostConfirmation {
            confirmation_id,
            title,
            body,
            confirm_label,
            destructive: _,
            continuation_schema,
        } = &outcome
        {
            self.register_confirmation(
                index,
                &ConfirmationFields {
                    confirmation_id,
                    title,
                    body,
                    confirm_label,
                    continuation_schema,
                },
                now_epoch,
            );
        }

        self.requests[index].status = RequestStatus::Completed(outcome);
        Ok(())
    }

    /// Create the single-use confirmation token from an accepted
    /// `RequestHostConfirmation` outcome, carrying the exact UI fields and
    /// original invocation A data.
    fn register_confirmation(
        &mut self,
        index: usize,
        fields: &ConfirmationFields<'_>,
        now_epoch: u64,
    ) {
        let binding = ConfirmationBinding {
            owner: self.requests[index].key.owner.clone(),
            action_id: self.requests[index].key.action_id.clone(),
            context_screen: self.requests[index].context_screen.clone(),
            context_instance: self.requests[index].context_instance.clone(),
            context_refs: self.requests[index].context_refs.clone(),
            generation: self.requests[index].key.generation,
        };
        let arguments = self.requests[index].arguments.clone();
        let policy = self.requests[index].policy.clone();
        // Replace only the same authenticated single-use token. Provider-supplied
        // ids are scoped by the immutable invocation binding, not globally.
        self.pending_confirmations.retain(|token| {
            token.confirmation_id != *fields.confirmation_id || token.binding != binding
        });
        self.pending_confirmations.push(PendingConfirmation {
            binding,
            confirmation_id: fields.confirmation_id.clone(),
            created_epoch: now_epoch,
            title: fields.title.to_owned(),
            body: fields.body.to_owned(),
            confirm_label: fields.confirm_label.to_owned(),
            continuation_schema: fields.continuation_schema.to_vec(),
            arguments,
            policy,
        });
    }

    /// Record a terminal error (first terminal wins, CW10-09).
    ///
    /// # Errors
    ///
    /// Returns [`ProviderRequestError::UnknownGeneration`] or
    /// [`ProviderRequestError::PostTerminal`].
    pub fn record_error(
        &mut self,
        key: &ProviderRequestKey,
        message: String,
    ) -> Result<(), ProviderRequestError> {
        let index = self
            .requests
            .iter()
            .position(|req| &req.key == key)
            .ok_or(ProviderRequestError::UnknownGeneration)?;
        if self.requests[index].status.is_terminal() {
            return Err(ProviderRequestError::PostTerminal);
        }
        self.requests[index].status = RequestStatus::Failed(message);
        Ok(())
    }

    /// Cancel an in-flight request (CW10-08/CW10-09).
    ///
    /// Cancel is itself terminal and creates no continuation. If the request
    /// already reached a terminal state the cancel is an explicit no-effect
    /// result ([`CancelOutcome::AlreadyTerminal`]) consistent with
    /// first-terminal semantics: no `CancelRequest` effect is staged and the
    /// first terminal result stays authoritative. Old-generation cancels are
    /// rejected as [`UnknownGeneration`].
    ///
    /// # Errors
    ///
    /// Returns [`ProviderRequestError::UnknownGeneration`] when no active
    /// request matches.
    ///
    /// [`UnknownGeneration`]: ProviderRequestError::UnknownGeneration
    pub fn cancel(
        &mut self,
        key: &ProviderRequestKey,
    ) -> Result<CancelOutcome, ProviderRequestError> {
        let index = self
            .requests
            .iter()
            .position(|req| &req.key == key)
            .ok_or(ProviderRequestError::UnknownGeneration)?;
        if self.requests[index].status.is_terminal() {
            return Ok(CancelOutcome::AlreadyTerminal { key: key.clone() });
        }
        self.requests[index].status = RequestStatus::Cancelled;
        Ok(CancelOutcome::Cancelled { key: key.clone() })
    }

    /// Mark a generation unavailable (CW10-09/CW10-10).
    ///
    /// Crash, EOF, protocol, or timeout makes the current generation
    /// unavailable. If the request already reached a different terminal state,
    /// the first terminal wins and this is a no-op.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderRequestError::UnknownGeneration`].
    pub fn mark_unavailable(
        &mut self,
        key: &ProviderRequestKey,
        reason: UnavailableReason,
    ) -> Result<(), ProviderRequestError> {
        let index = self
            .requests
            .iter()
            .position(|req| &req.key == key)
            .ok_or(ProviderRequestError::UnknownGeneration)?;
        if !self.requests[index].status.is_terminal() {
            self.requests[index].status = RequestStatus::Unavailable(reason);
        }
        Ok(())
    }

    /// Confirm a pending continuation (CW10-08).
    ///
    /// Validates the single-use token, then allocates and commits invocation B
    /// atomically. The exact owner/action/context/generation binding and
    /// confirmation id must match, and the token must not be expired. The
    /// generation counter is advanced **only** when the token is validated and
    /// consumed: an unknown confirm returns [`ConfirmationNotFound`], while a
    /// changed screen/resource binding returns [`ProviderRequestError::StaleContext`],
    /// both without advancing the counter; an expired confirm consumes the token
    /// (fail-fast single-use, so a repeated expired attempt cannot probe or
    /// reuse it) without advancing the counter. Generation exhaustion preserves
    /// the token: the next generation is computed without mutating, then the
    /// token is removed and the generation/request are committed together.
    ///
    /// On success exactly one fresh invocation B (new generation) is returned
    /// carrying the original invocation A arguments/context and the exact typed
    /// [`ProviderContinuation`]. Cancel creates no continuation; this method
    /// is the sole confirm path.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderRequestError::ActiveLimitExceeded`],
    /// [`ProviderRequestError::GenerationExhausted`],
    /// [`ProviderRequestError::ConfirmationNotFound`],
    /// [`ProviderRequestError::StaleContext`], or [`ProviderRequestError::Expired`].
    ///
    /// [`ConfirmationNotFound`]: ProviderRequestError::ConfirmationNotFound
    pub fn confirm(
        &mut self,
        input: ConfirmInput<'_>,
        now_epoch: u64,
    ) -> Result<ConfirmOutcome, ProviderRequestError> {
        let position = self.validate_confirmation(&input, now_epoch)?;
        self.commit_confirmation(position, input)
    }

    fn validate_confirmation(
        &mut self,
        input: &ConfirmInput<'_>,
        now_epoch: u64,
    ) -> Result<usize, ProviderRequestError> {
        if self.live_count() >= MAX_ACTIVE_REQUESTS {
            return Err(ProviderRequestError::ActiveLimitExceeded {
                limit: MAX_ACTIVE_REQUESTS,
            });
        }
        let position = self
            .pending_confirmations
            .iter()
            .position(|token| {
                token.confirmation_id == *input.confirmation_id
                    && token.binding.owner == *input.owner
                    && token.binding.action_id == *input.action_id
                    && token.binding.generation == input.generation
            })
            .ok_or(ProviderRequestError::ConfirmationNotFound)?;
        let token = &self.pending_confirmations[position];
        if token.binding.context_screen != *input.context_screen
            || token.binding.context_instance != *input.context_instance
            || token.binding.context_refs != *input.context_refs
        {
            return Err(ProviderRequestError::StaleContext);
        }
        let elapsed = now_epoch.saturating_sub(token.created_epoch);
        if elapsed >= CONFIRMATION_TTL_SECONDS {
            self.pending_confirmations.remove(position);
            return Err(ProviderRequestError::Expired { elapsed });
        }
        let has_exact_fields = token.continuation_schema.len() == input.values.len()
            && token
                .continuation_schema
                .iter()
                .all(|field| input.values.contains_key(field.id()));
        if !has_exact_fields
            || !validate_fields(&token.continuation_schema, input.values).is_empty()
        {
            return Err(ProviderRequestError::InvalidContinuationValues);
        }
        Ok(position)
    }

    fn commit_confirmation(
        &mut self,
        position: usize,
        input: ConfirmInput<'_>,
    ) -> Result<ConfirmOutcome, ProviderRequestError> {
        let generation = next_generation(self.next_generation)?;
        let token = self.pending_confirmations.remove(position);
        self.next_generation = generation;
        let key = ProviderRequestKey {
            owner: token.binding.owner.clone(),
            action_id: token.binding.action_id.clone(),
            generation,
        };
        let continuation = ProviderContinuation {
            confirmation_id: token.confirmation_id.clone(),
            approved: true,
            values: input.values.clone(),
        };
        let invocation = ProviderInvocation {
            key: key.clone(),
            arguments: token.arguments.clone(),
            context_screen: token.binding.context_screen.clone(),
            context_instance: token.binding.context_instance.clone(),
            context_refs: token.binding.context_refs.clone(),
            continuation: Some(continuation),
        };
        self.requests.push(ActiveRequest {
            key: key.clone(),
            context_screen: token.binding.context_screen.clone(),
            context_instance: token.binding.context_instance.clone(),
            context_refs: token.binding.context_refs.clone(),
            arguments: token.arguments.clone(),
            policy: token.policy.clone(),
            progress: crate::runtime::provider::protocol::ProgressTracker::new(),
            last_progress: None,
            status: RequestStatus::Live,
        });
        Ok(ConfirmOutcome { key, invocation })
    }

    /// Explicit retry: allocate a new generation for an existing request
    /// (CW10-10).
    ///
    /// All old-generation output changes nothing: the old request is left in
    /// its current state and a fresh generation starts. If the old request is
    /// still live it is marked unavailable before the new one begins. An
    /// unknown old key is rejected as [`UnknownGeneration`] rather than
    /// silently starting a new request.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderRequestError::UnknownGeneration`], the active-limit,
    /// or generation-exhaustion errors.
    ///
    /// [`UnknownGeneration`]: ProviderRequestError::UnknownGeneration
    pub fn retry(
        &mut self,
        old_key: &ProviderRequestKey,
        input: InvokeInput<'_>,
    ) -> Result<InvokeOutcome, ProviderRequestError> {
        let index = self
            .requests
            .iter()
            .position(|req| &req.key == old_key)
            .ok_or(ProviderRequestError::UnknownGeneration)?;
        if !self.requests[index].status.is_terminal() {
            self.requests[index].status = RequestStatus::Unavailable(UnavailableReason::Crash);
        }
        self.invoke(input)
    }
}

#[cfg(test)]
#[path = "provider_requests_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "provider_request_red_tests.rs"]
mod red_tests;

#[cfg(test)]
#[path = "provider_requests_continuation_tests.rs"]
mod continuation_tests;
