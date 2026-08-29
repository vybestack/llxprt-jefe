//! Provider request reducer handlers (issue #390 CW-10, Slice B).
//!
//! Translates typed [`ProviderMessage`] variants into bounded calls on the
//! pure [`ProviderRequestState`] reducer and stages at most one closed
//! [`ProviderEffect`] per transition. The handler never holds a process
//! handle, reads a clock, or performs I/O — timestamps arrive in the message.

use crate::domain::effects::{
    Effect, EffectFamily, ProviderEffect, ProviderHostOutcome, ProviderNotice,
    ProviderNoticeSeverity, ProviderRequestKey, RetryPolicy, SemanticKey,
};
use crate::messages::ProviderMessage;
use crate::runtime::provider::protocol::{Outcome, Severity};
use crate::state::provider_requests::{
    ActionPolicy, CancelOutcome, ConfirmInput, InvokeInput, ProviderRequestError,
};

use super::AppState;

fn current_provider_invocation_context(
    state: &AppState,
) -> Result<
    (
        crate::domain::Id,
        crate::domain::Id,
        crate::domain::TypedMap,
    ),
    String,
> {
    let current = state.nav.current();
    let context_screen = crate::domain::Id::parse(current.screen.as_str())
        .map_err(|error| format!("current provider screen is invalid: {error}"))?;
    let context_instance = crate::domain::Id::parse(&current.id.to_string())
        .map_err(|error| format!("current provider screen instance is invalid: {error}"))?;
    let context_refs = super::provider_action_context::project_current_context(state)
        .map_err(|error| format!("provider action context is invalid: {error}"))?;
    Ok((context_screen, context_instance, context_refs))
}

impl AppState {
    #[must_use]
    pub fn latest_current_provider_request(
        &self,
    ) -> Option<&crate::state::provider_requests::ActiveRequest> {
        let current = self.nav.current();
        let screen = current.screen.as_str();
        let instance = current.id.to_string();
        self.provider_requests
            .requests()
            .iter()
            .rev()
            .find(|request| {
                request.context_screen().as_str() == screen
                    && request.context_instance().as_str() == instance
            })
    }

    #[must_use]
    pub fn has_queued_current_provider_confirmation(&self) -> bool {
        let current = self.nav.current();
        self.provider_requests
            .first_pending_confirmation_for(current.screen.as_str(), &current.id.to_string())
            .is_some()
    }

    /// Apply a typed provider request lifecycle message.
    pub(super) fn apply_provider_message(&mut self, message: ProviderMessage) {
        match message {
            ProviderMessage::Invoke {
                owner,
                action_id,
                arguments,
                policy,
            } => self.apply_provider_invoke(owner, action_id, arguments, policy),
            msg @ ProviderMessage::Confirm { .. } => self.apply_confirm_msg(msg),
            ProviderMessage::EditConfirmationField { field_id, value } => {
                self.edit_provider_confirmation_field(field_id, value);
            }
            ProviderMessage::CycleConfirmationFocus => self.cycle_provider_confirmation_focus(),
            ProviderMessage::CancelConfirmation => self.cancel_provider_confirmation(),
            ProviderMessage::Retry { old_key } => self.apply_provider_retry(old_key),
            ProviderMessage::HealthChanged { unavailable } => {
                self.provider_action_health = unavailable;
            }
            ProviderMessage::DismissTerminals => {
                self.set_provider_surface_action(None);
                let current = self.nav.current();
                let screen = current.screen.as_str().to_owned();
                let instance = current.id.to_string();
                self.provider_requests
                    .drain_terminal_for(&screen, &instance);
            }
            signal => self.handle_provider_signal(signal),
        }
    }

    fn apply_provider_invoke(
        &mut self,
        owner: crate::domain::Id,
        action_id: crate::domain::Id,
        arguments: crate::domain::TypedMap,
        policy: ActionPolicy,
    ) {
        let (context_screen, context_instance, context_refs) =
            match current_provider_invocation_context(self) {
                Ok(context) => context,
                Err(error) => {
                    self.error_message = Some(error);
                    return;
                }
            };
        self.set_provider_surface_action(None);
        self.handle_provider_invoke(InvokeInput {
            owner: &owner,
            action_id: &action_id,
            context_screen: &context_screen,
            context_instance: &context_instance,
            context_refs: &context_refs,
            arguments: &arguments,
            policy: &policy,
        });
    }

    fn apply_provider_retry(&mut self, old_key: ProviderRequestKey) {
        let Some(request) = self.provider_requests.request(&old_key) else {
            self.provider_error(ProviderRequestError::UnknownGeneration);
            return;
        };
        let current = self.nav.current();
        if request.context_screen().as_str() != current.screen.as_str()
            || request.context_instance().as_str() != current.id.to_string()
        {
            self.provider_error(ProviderRequestError::StaleContext);
            return;
        }
        let owner = request.key().owner.clone();
        let action_id = request.key().action_id.clone();
        let arguments = request.arguments().clone();
        let policy = request.policy().clone();
        let (context_screen, context_instance, context_refs) =
            match current_provider_invocation_context(self) {
                Ok(context) => context,
                Err(error) => {
                    self.error_message = Some(error);
                    return;
                }
            };
        self.handle_provider_retry(
            &old_key,
            InvokeInput {
                owner: &owner,
                action_id: &action_id,
                context_screen: &context_screen,
                context_instance: &context_instance,
                context_refs: &context_refs,
                arguments: &arguments,
                policy: &policy,
            },
        );
    }

    fn apply_confirm_msg(&mut self, message: ProviderMessage) {
        let ProviderMessage::Confirm {
            owner,
            action_id,
            generation,
            confirmation_id,
            values,
            now_epoch,
        } = message
        else {
            return;
        };
        let current = self.nav.current();
        let presented = current.overlays().provider_confirmation();
        let owns_message = presented.is_some_and(|identity| {
            identity.owner() == &owner
                && identity.action_id() == &action_id
                && identity.generation() == generation
                && identity.confirmation_id() == &confirmation_id
                && identity.context_screen().as_str() == current.screen.as_str()
                && identity.context_instance().as_str() == current.id.to_string()
        });
        if !owns_message {
            self.provider_error(ProviderRequestError::ConfirmationNotFound);
            return;
        }
        let (context_screen, context_instance, context_refs) =
            match current_provider_invocation_context(self) {
                Ok(context) => context,
                Err(error) => {
                    self.error_message = Some(error);
                    return;
                }
            };
        self.handle_provider_confirm(
            ConfirmInput {
                owner: &owner,
                action_id: &action_id,
                context_screen: &context_screen,
                context_instance: &context_instance,
                context_refs: &context_refs,
                generation,
                confirmation_id: &confirmation_id,
                values: &values,
            },
            now_epoch,
        );
    }

    fn handle_provider_signal(&mut self, message: ProviderMessage) {
        match message {
            ProviderMessage::Progress { key, payload } => {
                if let Err(error) = self.provider_requests.record_progress(&key, payload) {
                    self.provider_error(error);
                }
            }
            ProviderMessage::Outcome {
                key,
                outcome,
                now_epoch,
            } => match self
                .provider_requests
                .record_outcome(&key, outcome.clone(), now_epoch)
            {
                Ok(()) => {
                    if matches!(outcome, Outcome::RequestHostConfirmation { .. }) {
                        self.open_current_provider_confirmation();
                    }
                    if let Some(host_outcome) = provider_host_outcome(outcome) {
                        self.stage_provider_outcome_effect(key, host_outcome);
                    }
                }
                Err(error) => self.provider_error(error),
            },
            ProviderMessage::Error { key, message } => {
                if let Err(error) = self.provider_requests.record_error(&key, message) {
                    self.provider_error(error);
                }
            }
            ProviderMessage::Cancel { key } => match self.provider_requests.cancel(&key) {
                Ok(CancelOutcome::Cancelled { key }) => self.stage_provider_cancel_effect(key),
                // First terminal wins: a cancel after an already-terminal
                // request stages no CancelRequest effect.
                Ok(CancelOutcome::AlreadyTerminal { .. }) => {}
                Err(error) => self.provider_error(error),
            },
            ProviderMessage::GenerationFailed { key, reason } => {
                if let Err(error) = self.provider_requests.mark_unavailable(&key, reason) {
                    self.provider_error(error);
                }
            }
            _ => {}
        }
    }

    fn handle_provider_invoke(&mut self, input: InvokeInput<'_>) {
        match self.provider_requests.invoke(input) {
            Ok(outcome) => self.stage_provider_invoke_effect(outcome.invocation),
            Err(error) => self.provider_error(error),
        }
    }

    /// A confirmed continuation stages a fresh `InvokeAction` effect carrying
    /// invocation B's exact original arguments/context and continuation values
    /// — the same closed effect family as the initial invocation, never a
    /// separate confirm effect that discards invocation data.
    fn handle_provider_confirm(&mut self, input: ConfirmInput<'_>, now_epoch: u64) {
        match self.provider_requests.confirm(input, now_epoch) {
            Ok(outcome) => {
                self.nav.current_mut().overlays_mut().close();
                self.stage_provider_invoke_effect(outcome.invocation);
            }
            // Terminal consuming/invalidating errors leave the presented token
            // with no live pending identity, so the overlay must go too: the
            // screen would otherwise stay covered by a Confirmation that no key,
            // Back, or CloseModal can dismiss (an expired token is removed by
            // expiry, and ConfirmationNotFound/StaleContext mean the presented
            // identity no longer resolves to a consumable token).
            Err(
                error @ (ProviderRequestError::Expired { .. }
                | ProviderRequestError::ConfirmationNotFound
                | ProviderRequestError::StaleContext),
            ) => {
                self.nav.current_mut().overlays_mut().close();
                self.provider_error(error);
            }
            Err(error) => self.provider_error(error),
        }
    }

    fn edit_provider_confirmation_field(
        &mut self,
        field_id: crate::domain::Id,
        value: crate::domain::TypedValue,
    ) {
        let Some((field_id, value)) = self.provider_confirmation_field_edit(field_id, value) else {
            return;
        };
        let Some(field) = self
            .current_provider_confirmation()
            .and_then(|confirmation| {
                confirmation
                    .continuation_schema()
                    .iter()
                    .find(|field| field.id() == &field_id)
                    .cloned()
            })
        else {
            return;
        };
        if crate::domain::plugin_config::validate_field_value(&field, &value).is_err() {
            return;
        }
        self.nav
            .current_mut()
            .overlays_mut()
            .set_confirmation_value(&field_id, value);
    }

    fn current_instance_owns_pending_confirmation(&self) -> bool {
        self.current_provider_confirmation().is_some()
    }

    fn cycle_provider_confirmation_focus(&mut self) {
        if self.current_instance_owns_pending_confirmation() {
            self.nav
                .current_mut()
                .overlays_mut()
                .cycle_confirmation_focus();
        }
    }

    fn cancel_provider_confirmation(&mut self) {
        let current = self.nav.current();
        let screen = current.screen.as_str().to_owned();
        let instance = current.id.to_string();
        let identity = current.overlays().provider_confirmation().cloned();
        let cancelled =
            identity.is_some_and(|identity| self.provider_requests.cancel_confirmation(&identity));
        if cancelled {
            self.provider_requests
                .drain_terminal_for(&screen, &instance);
            self.nav.current_mut().overlays_mut().close();
        }
    }

    pub(super) fn open_current_provider_confirmation(&mut self) {
        if !matches!(self.modal, super::ModalState::None) {
            return;
        }
        let current = self.nav.current();
        let candidate = self
            .provider_requests
            .first_pending_confirmation_for(current.screen.as_str(), &current.id.to_string())
            .map(|confirmation| {
                (
                    confirmation.identity(),
                    confirmation.continuation_schema().to_vec(),
                )
            });
        if let Some((identity, continuation_schema)) = candidate {
            let opened = self
                .nav
                .current_mut()
                .overlays_mut()
                .open_provider_confirmation(identity.clone(), &continuation_schema);
            if !opened && self.nav.current().overlays().active().is_none() {
                // The descriptor does not declare a Confirmation overlay, so
                // this token can never be presented or consumed. Refuse it
                // loudly instead of stranding it in pending_confirmations.
                self.provider_requests.cancel_confirmation(&identity);
                self.error_message = Some(
                    "provider confirmation cannot be presented: screen does not declare a Confirmation overlay".to_owned(),
                );
            }
        }
    }

    fn handle_provider_retry(&mut self, old_key: &ProviderRequestKey, input: InvokeInput<'_>) {
        match self.provider_requests.retry(old_key, input) {
            Ok(outcome) => self.stage_provider_invoke_effect(outcome.invocation),
            Err(error) => self.provider_error(error),
        }
    }

    fn stage_provider_invoke_effect(
        &mut self,
        invocation: crate::domain::effects::ProviderInvocation,
    ) {
        let owner = invocation.key.owner.clone();
        let subject = format!(
            "invoke-{}-{}",
            invocation.key.action_id, invocation.key.generation
        );
        let semantic_key = SemanticKey::new(EffectFamily::Provider, &subject);
        let effect = Effect::Provider(ProviderEffect::InvokeAction { invocation });
        if let Err(error) =
            self.register_pending_effect(owner, semantic_key, effect, RetryPolicy::Never)
        {
            self.error_message = Some(error.to_string());
        }
    }

    fn stage_provider_cancel_effect(&mut self, key: ProviderRequestKey) {
        let owner = key.owner.clone();
        let subject = format!("cancel-{}-{}", key.action_id, key.generation);
        let semantic_key = SemanticKey::new(EffectFamily::Provider, &subject);
        let effect = Effect::Provider(ProviderEffect::CancelRequest { key });
        if let Err(error) =
            self.register_pending_effect(owner, semantic_key, effect, RetryPolicy::Never)
        {
            self.error_message = Some(error.to_string());
        }
    }

    fn stage_provider_outcome_effect(
        &mut self,
        key: ProviderRequestKey,
        outcome: ProviderHostOutcome,
    ) {
        let owner = key.owner.clone();
        let subject = format!("outcome-{}-{}", key.action_id, key.generation);
        let semantic_key = SemanticKey::new(EffectFamily::Provider, &subject);
        let effect = Effect::Provider(ProviderEffect::ApplyOutcome { key, outcome });
        if let Err(error) =
            self.register_pending_effect(owner, semantic_key, effect, RetryPolicy::Never)
        {
            self.error_message = Some(error.to_string());
        }
    }

    /// Report a typed provider request error, except stale-generation output
    /// which is expected and silently ignored (CW10-10).
    fn provider_error(&mut self, error: ProviderRequestError) {
        if error != ProviderRequestError::UnknownGeneration {
            self.error_message = Some(error.to_string());
        }
    }
}

fn provider_host_outcome(outcome: Outcome) -> Option<ProviderHostOutcome> {
    match outcome {
        Outcome::Navigate {
            route_id,
            activation,
        } => Some(ProviderHostOutcome::Navigate {
            route_id,
            activation,
        }),
        Outcome::Refresh { resource_ref } => Some(ProviderHostOutcome::Refresh { resource_ref }),
        Outcome::Notice { severity, message } => {
            Some(ProviderHostOutcome::Notice(ProviderNotice {
                severity: match severity {
                    Severity::Info => ProviderNoticeSeverity::Info,
                    Severity::Warning => ProviderNoticeSeverity::Warning,
                },
                message,
            }))
        }
        Outcome::RequestHostConfirmation { .. } => None,
    }
}
