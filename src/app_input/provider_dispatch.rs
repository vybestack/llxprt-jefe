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
use jefe::domain::effects::{
    Effect, EffectCompletion, EffectError, EffectErrorKind, EffectResponse, IssuedEffect,
    ProviderEffect, ProviderHostOutcome, ProviderResponse,
};
use jefe::domain::{Id, TypedMap, TypedValue};
use jefe::messages::{AppMessage, ProviderMessage};
use jefe::services::provider_effect_worker::{
    ProviderEffectHandle, ProviderPanelWorkItem, ProviderWorkItem,
};
use jefe::state::ConfirmFocus;
use jefe::state::transition;

use super::{AppStateHandle, SharedContext};

/// The stable host-side owner id used for all provider invocations.
const HOST_ID_STR: &str = "host";

/// Operator controls owned by the provider execution surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSurfaceControl {
    /// Retry the latest terminal request.
    Retry,
    /// Move focus between pending confirmation controls.
    CycleConfirmationFocus,
    /// Activate the focused pending confirmation control.
    ActivateConfirmation,
    /// Cancel a live request or dismiss retained terminal rows.
    Escape,
}

/// Dispatch a provider-surface control through the same reducer/effect funnel
/// as an initial invocation.
pub fn dispatch_provider_surface_control(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    control: ProviderSurfaceControl,
) -> bool {
    let message = {
        let state = app_state.read();
        provider_surface_message(&state, control)
    };
    let Some(message) = message else {
        return false;
    };
    dispatch_provider_messages(app_state, ctx, message);
    true
}

/// Dispatch one typed edit accepted by the exact pending provider-confirmation Form.
pub fn dispatch_provider_confirmation_field_edit(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    field_id: Id,
    value: TypedValue,
) -> bool {
    let message = {
        let state = app_state.read();
        state
            .provider_confirmation_field_edit(field_id, value)
            .map(|(field_id, value)| ProviderMessage::EditConfirmationField { field_id, value })
    };
    let Some(message) = message else {
        return false;
    };
    dispatch_provider_messages(app_state, ctx, message);
    true
}

fn provider_confirmation_control(
    state: &jefe::state::AppState,
    control: ProviderSurfaceControl,
) -> Option<ConfirmFocus> {
    if control == ProviderSurfaceControl::ActivateConfirmation
        && state
            .nav
            .current()
            .overlays()
            .confirmation_focused_field()
            .is_some()
    {
        return None;
    }
    let action = match control {
        ProviderSurfaceControl::CycleConfirmationFocus => jefe::host_controls::ControlAction::Next,
        ProviderSurfaceControl::ActivateConfirmation => {
            jefe::host_controls::ControlAction::Activate
        }
        ProviderSurfaceControl::Retry | ProviderSurfaceControl::Escape => return None,
    };
    state.provider_confirmation_focus_for(action, 60)
}

fn provider_surface_message(
    state: &jefe::state::AppState,
    control: ProviderSurfaceControl,
) -> Option<ProviderMessage> {
    let Some(request) = state.latest_current_provider_request() else {
        return (state.provider_surface_action().is_some()
            && control == ProviderSurfaceControl::Escape)
            .then_some(ProviderMessage::DismissTerminals);
    };
    let confirmation = state.current_provider_confirmation();
    match control {
        ProviderSurfaceControl::CycleConfirmationFocus
            if provider_confirmation_control(state, control).is_some() =>
        {
            Some(ProviderMessage::CycleConfirmationFocus)
        }
        ProviderSurfaceControl::ActivateConfirmation
            if provider_confirmation_control(state, control) == Some(ConfirmFocus::Cancel) =>
        {
            Some(ProviderMessage::CancelConfirmation)
        }
        ProviderSurfaceControl::ActivateConfirmation
            if provider_confirmation_control(state, control) == Some(ConfirmFocus::Confirm) =>
        {
            let confirmation = confirmation?;
            Some(ProviderMessage::Confirm {
                owner: confirmation.owner().clone(),
                action_id: confirmation.action_id().clone(),
                generation: confirmation.generation(),
                confirmation_id: confirmation.confirmation_id().clone(),
                values: state
                    .nav
                    .current()
                    .overlays()
                    .confirmation_values()?
                    .clone(),
                now_epoch: current_epoch_seconds()?,
            })
        }
        ProviderSurfaceControl::Escape if confirmation.is_some() => {
            Some(ProviderMessage::CancelConfirmation)
        }
        ProviderSurfaceControl::Retry
            if request.is_terminal() && state.provider_retry_control_accepts() =>
        {
            Some(provider_retry_message(request))
        }
        ProviderSurfaceControl::Escape if request.is_terminal() => {
            Some(ProviderMessage::DismissTerminals)
        }
        ProviderSurfaceControl::Escape if state.provider_cancel_control_accepts() => {
            Some(ProviderMessage::Cancel {
                key: request.key().clone(),
            })
        }
        ProviderSurfaceControl::Retry
        | ProviderSurfaceControl::CycleConfirmationFocus
        | ProviderSurfaceControl::ActivateConfirmation
        | ProviderSurfaceControl::Escape => None,
    }
}

fn provider_retry_message(
    request: &jefe::state::provider_requests::ActiveRequest,
) -> ProviderMessage {
    ProviderMessage::Retry {
        old_key: request.key().clone(),
    }
}

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

fn keybind_invocation_arguments(
    declared: &[jefe::domain::plugin::field::Field],
) -> Option<TypedMap> {
    declared.is_empty().then(TypedMap::new)
}

/// Initiate a provider action invocation from a keybind dispatch.
///
/// Looks up the descriptor from the committed workbench catalog, builds the
/// `ProviderMessage::Invoke` with the exact action policy, and dispatches it
/// through [`dispatch_provider_messages`]. Returns `false` if the action is not
/// in the catalog (nothing was dispatched).
pub fn invoke_provider_action(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    action_id: &ActionId,
) -> bool {
    let Some(ctx_arc) = ctx else {
        tracing::warn!(action_id = %action_id.as_str(), "provider dispatch has no application context");
        return false;
    };
    let descriptor = {
        let Ok(ctx_guard) = ctx_arc.try_lock() else {
            tracing::warn!(action_id = %action_id.as_str(), "provider dispatch could not acquire the application context");
            return false;
        };
        if ctx_guard.provider_coordinator.is_none() {
            tracing::warn!(action_id = %action_id.as_str(), "provider dispatch has no runtime coordinator");
            return false;
        }
        ctx_guard
            .workbench
            .provider_catalog()
            .get(action_id)
            .cloned()
    };
    let Some(descriptor) = descriptor else {
        tracing::warn!(action_id = %action_id.as_str(), "provider dispatch action is absent from the runtime catalog");
        return false;
    };

    dispatch_keybind_invocation(
        &descriptor.action_id,
        &descriptor.arguments,
        &descriptor.policy,
        |message| dispatch_provider_messages(app_state, ctx, message),
    )
}

fn dispatch_keybind_invocation(
    action_id: &ActionId,
    declared_arguments: &[jefe::domain::plugin::field::Field],
    policy: &jefe::state::provider_requests::ActionPolicy,
    dispatch: impl FnOnce(ProviderMessage),
) -> bool {
    // The descriptor stores the already-parsed ActionId; convert its validated
    // string form into a domain Id without re-validating (both share the same
    // grammar constraints).
    let Ok(action_id_domain) = Id::parse(action_id.as_str()) else {
        tracing::error!(
            action_id = %action_id.as_str(),
            "provider_dispatch: descriptor action_id is not a valid Id"
        );
        return false;
    };
    let Some(owner) = host_id() else { return false };
    let Some(arguments) = keybind_invocation_arguments(declared_arguments) else {
        tracing::warn!(
            action_id = %action_id.as_str(),
            "provider action requires typed arguments that keybind dispatch cannot collect"
        );
        return false;
    };
    dispatch(ProviderMessage::Invoke {
        owner,
        action_id: action_id_domain,
        arguments,
        policy: policy.clone(),
    });
    true
}

/// Schedule staged provider effects for background execution.
///
/// `InvokeAction` effects are pushed to the [`ProviderEffectHandle`] so the
/// background worker runs the lifecycle off the UI thread. `CancelRequest`
/// effects are routed to the exact live one-shot or persistent session.
pub fn schedule_provider_effects(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    effects: Vec<IssuedEffect>,
) {
    let mut runtime_handle = None;
    for issued in effects {
        match &issued.effect {
            Effect::Provider(ProviderEffect::ApplyOutcome { key, outcome }) => {
                apply_provider_host_outcome(app_state, ctx, &issued, key.clone(), outcome.clone());
            }
            Effect::Provider(ProviderEffect::InvokeAction { invocation }) => {
                let Some(handle) = runtime_handle
                    .get_or_insert_with(|| acquire_provider_effect_handle(app_state, ctx))
                    .as_ref()
                else {
                    reject_provider_effect(app_state, issued);
                    continue;
                };
                handle.schedule(ProviderWorkItem {
                    invocation: invocation.clone(),
                    correlation: issued.correlation,
                });
            }
            Effect::Provider(
                effect @ (ProviderEffect::ActivatePanel { .. }
                | ProviderEffect::DeactivatePanel { .. }
                | ProviderEffect::PanelEvent { .. }),
            ) => {
                tracing::debug!("scheduling provider panel command");
                let Some(handle) = runtime_handle
                    .get_or_insert_with(|| acquire_provider_effect_handle(app_state, ctx))
                    .as_ref()
                else {
                    reject_provider_effect(app_state, issued);
                    continue;
                };
                handle.schedule_panel(ProviderPanelWorkItem {
                    effect: effect.clone(),
                    correlation: issued.correlation,
                });
            }
            Effect::Provider(ProviderEffect::CancelRequest { key }) => {
                let Some(handle) = runtime_handle
                    .get_or_insert_with(|| acquire_provider_effect_handle(app_state, ctx))
                    .as_ref()
                else {
                    reject_provider_effect(app_state, issued);
                    continue;
                };
                handle.schedule_cancel(key.clone());
                tracing::debug!(
                    action = %key.action_id.as_str(),
                    "cancel forwarded to live provider session"
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

fn acquire_provider_effect_handle(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
) -> Option<ProviderEffectHandle> {
    let Some(ctx_arc) = ctx else {
        return None;
    };
    let Ok(ctx_guard) = ctx_arc.try_lock() else {
        app_state.write().error_message =
            Some("provider dispatch could not acquire the application context".to_owned());
        return None;
    };
    Some(ctx_guard.provider_effect_handle.clone())
}

fn reject_provider_effect(app_state: &mut AppStateHandle, issued: IssuedEffect) {
    let mut state = app_state.write();
    transition::reject_unexecuted_effects(&mut state, vec![issued]);
}

fn apply_provider_host_outcome(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    issued: &IssuedEffect,
    key: jefe::domain::effects::ProviderRequestKey,
    outcome: ProviderHostOutcome,
) {
    let action = {
        let mut state = app_state.write();
        let action = match prepare_provider_host_outcome_state(&mut state, &key, outcome) {
            Ok(action) => {
                complete_provider_outcome_state(
                    &mut state,
                    issued,
                    Ok(ProviderResponse::OutcomeApplied { key }),
                );
                apply_provider_host_action_state(&mut state, &action);
                action
            }
            Err(detail) => {
                complete_provider_outcome_state(
                    &mut state,
                    issued,
                    Err(EffectError::new(EffectErrorKind::Rejected, false, &detail)),
                );
                return;
            }
        };
        drop(state);
        action
    };
    let staged = app_state.write().take_staged_effects();
    schedule_provider_effects(app_state, ctx, staged);

    match action {
        ProviderHostAction::Refresh(jefe::state::ScreenId::Issues) => {
            super::issues_list_dispatch::request_issue_list_silent_refresh(app_state, ctx);
        }
        ProviderHostAction::Refresh(jefe::state::ScreenId::PullRequests) => {
            super::prs_orchestration::request_pr_background_refresh(app_state, ctx, false);
        }
        ProviderHostAction::None
        | ProviderHostAction::Navigate { .. }
        | ProviderHostAction::Refresh(_) => {}
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ProviderHostAction {
    None,
    Navigate {
        route: jefe::workbench::RouteId,
        values: jefe::workbench::ActivationValues,
    },
    Refresh(jefe::state::ScreenId),
}

fn apply_provider_host_action_state(
    state: &mut jefe::state::AppState,
    action: &ProviderHostAction,
) {
    if let ProviderHostAction::Navigate { route, values } = action {
        transition::commit_pure_site(
            state,
            AppMessage::Provider(Box::new(ProviderMessage::DismissTerminals)),
        );
        state.enter_provider_route(*route, values.clone());
    }
}

fn prepare_provider_host_outcome_state(
    state: &mut jefe::state::AppState,
    key: &jefe::domain::effects::ProviderRequestKey,
    outcome: ProviderHostOutcome,
) -> Result<ProviderHostAction, String> {
    authorize_provider_outcome(state, key).and_then(|screen| match outcome {
        ProviderHostOutcome::Notice(notice) => {
            state.warning_message = Some(jefe::state::provider_view::provider_notice_line(&notice));
            state.provider_notice = Some(notice);
            Ok(ProviderHostAction::None)
        }
        ProviderHostOutcome::Navigate {
            route_id,
            activation,
        } => prepare_provider_navigation(state, key, &route_id, activation),
        ProviderHostOutcome::Refresh { resource_ref } => {
            let request = state
                .provider_requests
                .request(key)
                .ok_or_else(|| "provider outcome request is no longer current".to_owned())?;
            if request.context_refs() != &resource_ref {
                return Err("provider refresh no longer owns the current resource".to_owned());
            }
            match screen.compiled() {
                Some(
                    screen @ (jefe::state::ScreenId::Issues | jefe::state::ScreenId::PullRequests),
                ) => Ok(ProviderHostAction::Refresh(screen)),
                Some(
                    jefe::state::ScreenId::Repositories
                    | jefe::state::ScreenId::Actions
                    | jefe::state::ScreenId::Errors
                    | jefe::state::ScreenId::Terminals
                    | jefe::state::ScreenId::Settings,
                )
                | None => Err("provider refresh is unsupported for the current screen".to_owned()),
            }
        }
    })
}

fn authorize_provider_outcome(
    state: &jefe::state::AppState,
    key: &jefe::domain::effects::ProviderRequestKey,
) -> Result<jefe::workbench::ScreenIdentity, String> {
    let request = state
        .provider_requests
        .request(key)
        .ok_or_else(|| "provider outcome request is no longer current".to_owned())?;
    let screen = state.screen();
    if request.context_screen().as_str() != screen.as_str()
        || request.context_instance().as_str() != state.nav.current().id.to_string()
    {
        return Err("provider outcome authority is stale".to_owned());
    }
    Ok(screen)
}

fn prepare_provider_navigation(
    state: &jefe::state::AppState,
    key: &jefe::domain::effects::ProviderRequestKey,
    route_id: &Id,
    activation: TypedMap,
) -> Result<ProviderHostAction, String> {
    if state.nav.current().dirty.is_dirty() {
        return Err("provider navigation is blocked by unsaved changes".to_owned());
    }
    let request = state
        .provider_requests
        .request(key)
        .ok_or_else(|| "provider outcome request is no longer current".to_owned())?;
    if !request.policy().allows_route(route_id) {
        return Err("provider requested a route not declared by its package".to_owned());
    }
    let registry = state.published_workbench().screen_registry();
    let descriptor = registry
        .screens()
        .iter()
        .find(|candidate| candidate.route.as_str() == route_id.as_str())
        .ok_or_else(|| "provider requested an unknown route".to_owned())?;
    let values = provider_activation_values(&descriptor.activation, activation)?;
    let declaration = jefe::workbench::route_declaration(registry, descriptor.route)
        .map_err(|error| error.to_string())?;
    declaration
        .validate(&values)
        .map_err(|error| error.to_string())?;
    Ok(ProviderHostAction::Navigate {
        route: descriptor.route,
        values,
    })
}

fn provider_activation_values(
    schema: &[jefe::workbench::ActivationField],
    values: TypedMap,
) -> Result<jefe::workbench::ActivationValues, String> {
    let entries = values
        .into_iter()
        .map(|(name, value)| {
            let field = schema
                .iter()
                .find(|field| field.name == name)
                .ok_or_else(|| "provider supplied an undeclared activation field".to_owned())?;
            let value = provider_activation_value(&field.kind, value)?;
            Ok((name, value))
        })
        .collect::<Result<Vec<_>, String>>()?;
    jefe::workbench::ActivationValues::new(entries).map_err(|error| error.to_string())
}

fn provider_activation_value(
    kind: &jefe::workbench::ActivationKind,
    value: TypedValue,
) -> Result<jefe::workbench::ActivationValue, String> {
    use jefe::workbench::{ActivationKind, ActivationValue};

    match (kind, value) {
        (ActivationKind::Boolean, TypedValue::Bool(value)) => Ok(ActivationValue::Boolean(value)),
        (ActivationKind::OptionalBoolean, TypedValue::Bool(value)) => {
            Ok(ActivationValue::OptionalBoolean(Some(value)))
        }
        (ActivationKind::Text, TypedValue::String(value)) => Ok(ActivationValue::Text(value)),
        (ActivationKind::Integer, TypedValue::Integer(value)) => {
            Ok(ActivationValue::Integer(value))
        }
        (ActivationKind::Enumerated { permitted }, TypedValue::String(value))
            if permitted.contains(&value) =>
        {
            Ok(ActivationValue::Enumerated(value))
        }
        (ActivationKind::Path, TypedValue::String(value)) => {
            Ok(ActivationValue::Path(value.into()))
        }
        (ActivationKind::TextList, TypedValue::List(values)) => values
            .into_iter()
            .map(|value| match value {
                TypedValue::String(value) => Ok(value),
                _ => Err("provider activation list contains a non-string value".to_owned()),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(ActivationValue::TextList),
        _ => Err("provider activation value does not match the route schema".to_owned()),
    }
}

fn complete_provider_outcome_state(
    state: &mut jefe::state::AppState,
    issued: &IssuedEffect,
    result: Result<ProviderResponse, EffectError>,
) {
    let completion = EffectCompletion {
        correlation: issued.correlation.clone(),
        result: result.map(EffectResponse::Provider),
    };
    transition::commit_pure_site(state, AppMessage::EffectCompletion(Box::new(completion)));
}

/// The stable host-side owner id for all provider invocations.
///
/// Returns `None` only if the literal `"host"` fails to parse, which is
/// unreachable because it satisfies the `Id` grammar (lowercase, no trailing
/// separator). The caller treats `None` as a dispatch failure.
fn host_id() -> Option<Id> {
    Id::parse(HOST_ID_STR).ok()
}

fn current_epoch_seconds() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

#[cfg(test)]
#[path = "provider_dispatch_tests.rs"]
mod host_outcome_tests;
