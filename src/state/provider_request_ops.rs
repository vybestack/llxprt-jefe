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
    CancelOutcome, ConfirmInput, InvokeInput, ProviderRequestError,
};

use super::AppState;

impl AppState {
    /// Apply a typed provider request lifecycle message.
    pub(super) fn apply_provider_message(&mut self, message: ProviderMessage) {
        match message {
            ProviderMessage::Invoke {
                owner,
                action_id,
                context_screen,
                context_instance,
                context_refs,
                arguments,
                policy,
            } => {
                self.provider_surface_action = None;
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
            msg @ ProviderMessage::Confirm { .. } => self.apply_confirm_msg(msg),
            ProviderMessage::CycleConfirmationFocus => {
                self.provider_requests.cycle_confirmation_focus();
            }
            ProviderMessage::CancelConfirmation => {
                self.provider_requests.cancel_latest_confirmation();
                self.provider_requests.drain_terminal();
            }
            ProviderMessage::Retry {
                old_key,
                owner,
                action_id,
                context_screen,
                context_instance,
                context_refs,
                arguments,
                policy,
            } => self.handle_provider_retry(
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
            ),
            ProviderMessage::HealthChanged { unavailable } => {
                self.provider_action_health = unavailable;
            }
            ProviderMessage::DismissTerminals => {
                self.provider_surface_action = None;
                self.provider_requests.drain_terminal();
            }
            signal => self.handle_provider_signal(signal),
        }
    }

    fn apply_confirm_msg(&mut self, message: ProviderMessage) {
        let ProviderMessage::Confirm {
            owner,
            action_id,
            context_screen,
            context_instance,
            context_refs,
            generation,
            confirmation_id,
            values,
            now_epoch,
        } = message
        else {
            return;
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
            Ok(outcome) => self.stage_provider_invoke_effect(outcome.invocation),
            Err(error) => self.provider_error(error),
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
