//! Provider message dispatch and post-commit effect scheduling
//! (issue #390 CW-10, Slice D).
//!
//! This module is the composition funnel for `AppMessage::Provider`. It
//! commits the typed provider message through the pure reducer (which may
//! stage a closed `ProviderEffect`), releases every state guard, and then
//! hands any staged provider effects to the background provider worker via
//! the [`ProviderEffectHandle`].
//!
//! The background worker runs the one-shot lifecycle off the UI executor via
//! `smol::unblock` and routes typed progress/error/outcome/unavailability
//! messages back through the reducer with stale-generation protection.

use jefe::domain::action_registry::ActionId;
use jefe::domain::effects::{Effect, IssuedEffect, ProviderEffect};
use jefe::domain::{Id, TypedMap};
use jefe::messages::{AppMessage, ProviderMessage};
use jefe::services::provider_effect_worker::ProviderWorkItem;
use jefe::state::transition;

use super::{AppStateHandle, SharedContext};

/// The stable host-side owner id used for all provider invocations.
const HOST_ID_STR: &str = "host";

/// Dispatch a provider message: commit the pure reducer transition, then
/// schedule any staged provider effects through the background worker handle.
pub(super) fn dispatch_provider_messages(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: ProviderMessage,
) {
    let effects = {
        let mut state = app_state.write();
        transition::commit_in_place(&mut state, AppMessage::Provider(Box::new(message)))
    };
    if effects.is_empty() {
        super::refresh_action_availability(app_state);
        return;
    }
    schedule_provider_effects(app_state, ctx, effects);
    super::refresh_action_availability(app_state);
}

/// Initiate a provider action invocation from a keybind dispatch.
///
/// Looks up the descriptor from the coordinator's catalog, builds the
/// `ProviderMessage::Invoke` with the exact action policy, and dispatches it
/// through [`dispatch_provider_messages`]. Returns `false` if the action is not
/// in the catalog (nothing was dispatched).
pub fn invoke_provider_action(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    action_id: &ActionId,
    context_screen: &Id,
    context_instance: &Id,
    context_refs: &TypedMap,
) -> bool {
    let Some(ctx_arc) = ctx else {
        return false;
    };
    let descriptor = {
        let Ok(ctx_guard) = ctx_arc.try_lock() else {
            return false;
        };
        let Some(coordinator) = ctx_guard.provider_coordinator.as_ref() else {
            return false;
        };
        coordinator.catalog().get(action_id).cloned()
    };
    let Some(descriptor) = descriptor else {
        return false;
    };

    // The descriptor stores the already-parsed ActionId; convert its validated
    // string form into a domain Id without re-validating (both share the same
    // grammar constraints).
    let Ok(action_id_domain) = Id::parse(descriptor.action_id.as_str()) else {
        tracing::error!(
            action_id = %descriptor.action_id.as_str(),
            "provider_dispatch: descriptor action_id is not a valid Id"
        );
        return false;
    };
    let Some(owner) = host_id() else { return false };
    let message = ProviderMessage::Invoke {
        owner,
        action_id: action_id_domain,
        context_screen: context_screen.clone(),
        context_instance: context_instance.clone(),
        context_refs: context_refs.clone(),
        arguments: TypedMap::new(),
        policy: descriptor.policy.clone(),
    };
    dispatch_provider_messages(app_state, ctx, message);
    true
}

/// Schedule staged provider effects for background execution.
///
/// `InvokeAction` effects are pushed to the [`ProviderEffectHandle`] so the
/// background worker runs the lifecycle off the UI thread. `CancelRequest`
/// effects are a no-op for one-shot providers (the lifecycle already ran).
fn schedule_provider_effects(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    effects: Vec<IssuedEffect>,
) {
    let Some(ctx_arc) = ctx else {
        let mut state = app_state.write();
        transition::reject_unexecuted_effects(&mut state, effects);
        return;
    };

    let handle = {
        let Ok(ctx_guard) = ctx_arc.try_lock() else {
            let mut state = app_state.write();
            transition::reject_unexecuted_effects(&mut state, effects);
            return;
        };
        ctx_guard.provider_effect_handle.clone()
    };

    for issued in effects {
        match &issued.effect {
            Effect::Provider(ProviderEffect::InvokeAction { invocation }) => {
                handle.schedule(ProviderWorkItem {
                    invocation: invocation.clone(),
                    correlation: issued.correlation,
                });
            }
            Effect::Provider(ProviderEffect::CancelRequest { key }) => {
                tracing::debug!(
                    action = %key.action_id.as_str(),
                    "cancel request for completed one-shot"
                );
            }
            other => {
                let mut state = app_state.write();
                state.error_message = Some(format!(
                    "non-provider effect reached provider dispatch: {:?}",
                    other.family()
                ));
            }
        }
    }
}

/// The stable host-side owner id for all provider invocations.
///
/// Returns `None` only if the literal `"host"` fails to parse, which is
/// unreachable because it satisfies the `Id` grammar (lowercase, no trailing
/// separator). The caller treats `None` as a dispatch failure.
fn host_id() -> Option<Id> {
    Id::parse(HOST_ID_STR).ok()
}
