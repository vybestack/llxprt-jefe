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

fn provider_surface_message(
    state: &jefe::state::AppState,
    control: ProviderSurfaceControl,
) -> Option<ProviderMessage> {
    let Some(request) = state.provider_requests.requests().last() else {
        return (state.provider_surface_action.is_some()
            && control == ProviderSurfaceControl::Escape)
            .then_some(ProviderMessage::DismissTerminals);
    };
    let confirmation = state.provider_requests.latest_pending_confirmation_view();
    match control {
        ProviderSurfaceControl::CycleConfirmationFocus if confirmation.is_some() => {
            Some(ProviderMessage::CycleConfirmationFocus)
        }
        ProviderSurfaceControl::ActivateConfirmation
            if confirmation.is_some()
                && state.provider_requests.confirmation_focus() == ConfirmFocus::Cancel =>
        {
            Some(ProviderMessage::CancelConfirmation)
        }
        ProviderSurfaceControl::ActivateConfirmation => {
            let confirmation = confirmation?;
            Some(ProviderMessage::Confirm {
                owner: request.key().owner.clone(),
                action_id: request.key().action_id.clone(),
                context_screen: request.context_screen().clone(),
                context_instance: request.context_instance().clone(),
                context_refs: request.context_refs().clone(),
                generation: request.key().generation,
                confirmation_id: confirmation.confirmation_id().clone(),
                values: TypedMap::new(),
                now_epoch: current_epoch_seconds()?,
            })
        }
        ProviderSurfaceControl::Escape if confirmation.is_some() => {
            Some(ProviderMessage::CancelConfirmation)
        }
        ProviderSurfaceControl::Retry if request.is_terminal() => Some(ProviderMessage::Retry {
            old_key: request.key().clone(),
            owner: request.key().owner.clone(),
            action_id: request.key().action_id.clone(),
            context_screen: request.context_screen().clone(),
            context_instance: request.context_instance().clone(),
            context_refs: request.context_refs().clone(),
            arguments: request.arguments().clone(),
            policy: request.policy().clone(),
        }),
        ProviderSurfaceControl::Escape if request.is_terminal() => {
            Some(ProviderMessage::DismissTerminals)
        }
        ProviderSurfaceControl::Escape => Some(ProviderMessage::Cancel {
            key: request.key().clone(),
        }),
        ProviderSurfaceControl::Retry | ProviderSurfaceControl::CycleConfirmationFocus => None,
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
        tracing::warn!(action_id = %action_id.as_str(), "provider dispatch has no application context");
        return false;
    };
    let descriptor = {
        let Ok(ctx_guard) = ctx_arc.try_lock() else {
            tracing::warn!(action_id = %action_id.as_str(), "provider dispatch could not acquire the application context");
            return false;
        };
        let Some(coordinator) = ctx_guard.provider_coordinator.as_ref() else {
            tracing::warn!(action_id = %action_id.as_str(), "provider dispatch has no runtime coordinator");
            return false;
        };
        coordinator.catalog().get(action_id).cloned()
    };
    let Some(descriptor) = descriptor else {
        tracing::warn!(action_id = %action_id.as_str(), "provider dispatch action is absent from the runtime catalog");
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
                if let ProviderHostAction::Navigate { route, values } = &action {
                    state.enter_provider_route(*route, values.clone());
                }
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
            match screen {
                jefe::state::ScreenId::Issues | jefe::state::ScreenId::PullRequests => {
                    Ok(ProviderHostAction::Refresh(screen))
                }
                _ => Err("provider refresh is unsupported for the current screen".to_owned()),
            }
        }
    })
}

fn authorize_provider_outcome(
    state: &jefe::state::AppState,
    key: &jefe::domain::effects::ProviderRequestKey,
) -> Result<jefe::state::ScreenId, String> {
    let request = state
        .provider_requests
        .request(key)
        .ok_or_else(|| "provider outcome request is no longer current".to_owned())?;
    let screen = state
        .compiled_screen()
        .ok_or_else(|| "provider outcome is unsupported for the current screen".to_owned())?;
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
    let registry = jefe::workbench::screen_registry().map_err(|error| error.to_string())?;
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
mod host_outcome_tests {
    use jefe::domain::effects::{
        Effect, EffectFamily, IssuedEffect, ProviderEffect, ProviderHostOutcome, ProviderNotice,
        ProviderNoticeSeverity, RetryPolicy, SemanticKey,
    };
    use jefe::domain::plugin::action::{ActionConfirmation, ActionOutcome};
    use jefe::state::ScreenId;
    use jefe::state::provider_requests::{ActionPolicy, InvokeInput};

    use super::*;

    fn active_request(
        state: &mut jefe::state::AppState,
    ) -> jefe::domain::effects::ProviderRequestKey {
        active_request_with_policy(
            state,
            ActionPolicy::new(ActionConfirmation::None, vec![ActionOutcome::Notice], false),
        )
    }

    fn active_request_with_policy(
        state: &mut jefe::state::AppState,
        policy: ActionPolicy,
    ) -> jefe::domain::effects::ProviderRequestKey {
        let owner = Id::parse("host").unwrap_or_else(|error| panic!("owner: {error}"));
        let action = Id::parse("provider.notice").unwrap_or_else(|error| panic!("action: {error}"));
        let screen =
            Id::parse(state.screen().as_str()).unwrap_or_else(|error| panic!("screen: {error}"));
        let instance = Id::parse(&state.nav.current().id.to_string())
            .unwrap_or_else(|error| panic!("instance: {error}"));
        state
            .provider_requests
            .invoke(InvokeInput {
                owner: &owner,
                action_id: &action,
                context_screen: &screen,
                context_instance: &instance,
                context_refs: &TypedMap::new(),
                arguments: &TypedMap::new(),
                policy: &policy,
            })
            .unwrap_or_else(|error| panic!("invoke: {error}"))
            .key
    }

    #[test]
    fn accepted_notice_applies_only_while_exact_screen_instance_is_current() {
        let mut state = jefe::state::AppState::default();
        let key = active_request(&mut state);
        let notice = ProviderNotice {
            severity: ProviderNoticeSeverity::Info,
            message: "completed".to_owned(),
        };

        let applied = prepare_provider_host_outcome_state(
            &mut state,
            &key,
            ProviderHostOutcome::Notice(notice.clone()),
        );
        assert!(matches!(applied, Ok(ProviderHostAction::None)));
        assert_eq!(state.provider_notice, Some(notice));
        assert_eq!(state.warning_message.as_deref(), Some("completed"));

        state.show_screen(ScreenId::Issues);
        state.provider_notice = None;
        state.warning_message = None;
        let refusal = prepare_provider_host_outcome_state(
            &mut state,
            &key,
            ProviderHostOutcome::Notice(ProviderNotice {
                severity: ProviderNoticeSeverity::Warning,
                message: "stale".to_owned(),
            }),
        );
        assert_eq!(
            refusal,
            Err("provider outcome authority is stale".to_owned())
        );
        assert!(state.provider_notice.is_none());
        assert!(state.warning_message.is_none());
    }

    #[test]
    fn provider_activation_conversion_rejects_nested_and_wrong_kind_values() {
        let name = Id::parse("query").unwrap_or_else(|error| panic!("field: {error}"));
        let schema = vec![jefe::workbench::ActivationField {
            name: name.clone(),
            kind: jefe::workbench::ActivationKind::Text,
        }];
        let mut nested = TypedMap::new();
        nested.insert(name.clone(), TypedValue::Map(TypedMap::new()));
        assert!(provider_activation_values(&schema, nested).is_err());

        let mut valid = TypedMap::new();
        valid.insert(name, TypedValue::String("open".to_owned()));
        assert!(provider_activation_values(&schema, valid).is_ok());
    }

    #[test]
    fn refresh_requires_the_exact_current_resource_and_supported_screen() {
        let mut state = jefe::state::AppState::default();
        state.show_screen(ScreenId::Issues);
        let key = active_request(&mut state);

        let accepted = prepare_provider_host_outcome_state(
            &mut state,
            &key,
            ProviderHostOutcome::Refresh {
                resource_ref: TypedMap::new(),
            },
        );
        assert_eq!(accepted, Ok(ProviderHostAction::Refresh(ScreenId::Issues)));

        let mut different = TypedMap::new();
        different.insert(
            Id::parse("repository").unwrap_or_else(|error| panic!("field: {error}")),
            TypedValue::String("other/repository".to_owned()),
        );
        let refused = prepare_provider_host_outcome_state(
            &mut state,
            &key,
            ProviderHostOutcome::Refresh {
                resource_ref: different,
            },
        );
        assert_eq!(
            refused,
            Err("provider refresh no longer owns the current resource".to_owned())
        );
    }

    #[test]
    fn provider_navigation_rejects_core_local_and_foreign_package_routes() {
        let declared =
            Id::parse("vendor.pkg.open").unwrap_or_else(|error| panic!("declared route: {error}"));
        let policy = ActionPolicy::new(
            ActionConfirmation::None,
            vec![ActionOutcome::NavigateDeclaredRoute],
            false,
        )
        .with_declared_routes(vec![declared.clone()]);
        let mut state = jefe::state::AppState::default();
        let key = active_request_with_policy(&mut state, policy);

        for route in ["actions", "local.open", "vendor.other.open"] {
            let refusal = prepare_provider_host_outcome_state(
                &mut state,
                &key,
                ProviderHostOutcome::Navigate {
                    route_id: Id::parse(route)
                        .unwrap_or_else(|error| panic!("route {route}: {error}")),
                    activation: TypedMap::new(),
                },
            );
            assert_eq!(
                refusal,
                Err("provider requested a route not declared by its package".to_owned())
            );
        }

        let declared_but_not_composed = prepare_provider_host_outcome_state(
            &mut state,
            &key,
            ProviderHostOutcome::Navigate {
                route_id: declared,
                activation: TypedMap::new(),
            },
        );
        assert_eq!(
            declared_but_not_composed,
            Err("provider requested an unknown route".to_owned())
        );
    }

    #[test]
    fn provider_navigation_refuses_to_bypass_the_dirty_guard() {
        use jefe::state::navigation_dirty::{DraftToken, SaveIntent};

        let mut state = jefe::state::AppState::default();
        let key = active_request(&mut state);
        let original_screen = state.screen();
        state.mark_screen_dirty(
            DraftToken::next(),
            SaveIntent::Unavailable {
                reason: "test draft has no save target",
            },
        );

        let refused = prepare_provider_host_outcome_state(
            &mut state,
            &key,
            ProviderHostOutcome::Navigate {
                route_id: Id::parse("actions").unwrap_or_else(|error| panic!("route: {error}")),
                activation: TypedMap::new(),
            },
        );
        assert_eq!(
            refused,
            Err("provider navigation is blocked by unsaved changes".to_owned())
        );
        assert_eq!(state.screen(), original_screen);
    }

    #[test]
    fn outcome_completion_closes_the_ledger_before_navigation_changes_generation() {
        let mut state = jefe::state::AppState::default();
        let route = Id::parse("actions").unwrap_or_else(|error| panic!("declared route: {error}"));
        let policy = ActionPolicy::new(
            ActionConfirmation::None,
            vec![ActionOutcome::NavigateDeclaredRoute],
            false,
        )
        .with_declared_routes(vec![route.clone()]);
        let key = active_request_with_policy(&mut state, policy);
        let owner = key.owner.clone();
        let correlation = state
            .pending_effects
            .register(
                owner,
                SemanticKey::new(EffectFamily::Provider, "outcome-provider.notice-1"),
                RetryPolicy::Never,
            )
            .unwrap_or_else(|error| panic!("register effect: {error}"));
        let issued = IssuedEffect {
            effect: Effect::Provider(ProviderEffect::ApplyOutcome {
                key: key.clone(),
                outcome: ProviderHostOutcome::Navigate {
                    route_id: route,
                    activation: TypedMap::new(),
                },
            }),
            correlation: correlation.clone(),
            retry: RetryPolicy::Never,
        };
        let action = prepare_provider_host_outcome_state(
            &mut state,
            &key,
            match &issued.effect {
                Effect::Provider(ProviderEffect::ApplyOutcome { outcome, .. }) => outcome.clone(),
                _ => panic!("fixture must carry a provider host outcome"),
            },
        )
        .unwrap_or_else(|error| panic!("prepare outcome: {error}"));

        complete_provider_outcome_state(
            &mut state,
            &issued,
            Ok(ProviderResponse::OutcomeApplied { key }),
        );
        assert!(!state.pending_effects.is_pending(&correlation));

        let ProviderHostAction::Navigate { route, values } = action else {
            panic!("expected navigation");
        };
        state.enter_provider_route(route, values);
        assert_eq!(state.screen(), ScreenId::Actions);
        assert!(!state.pending_effects.is_pending(&correlation));
    }
}
