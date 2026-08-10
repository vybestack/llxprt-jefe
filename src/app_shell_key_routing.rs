//! Single-resolution keyboard routing for the root application shell.

use iocraft::prelude::KeyEvent;

use crate::action_context::{ActionContext, DispatchScope, derive_action_context};
use crate::app_input::{
    action_execution_for, apply_action_execution, forward_key_to_pty, pre_mode_owned,
    try_ctrl_c_interrupt_passthrough,
};
use crate::app_shell::{CtxArc, HookState};
use crate::pty_encoding::PasteEnterSuppression;

use jefe::domain::action_registry::{ActionRegistrySnapshot, HandlerKey, Resolution};
use jefe::domain::keymap::{Chord, ChordError};
use jefe::input::{InputMode, canonical_chord, input_mode_for_state};
use jefe::list_viewport::PageItemCount;
use jefe::state::AppState;

#[derive(Debug)]
pub struct ResolvedRegistryKey {
    pub chord: Chord,
    pub scope: DispatchScope,
    pub resolution: Resolution,
}

#[derive(Debug)]
pub enum RegistryKeyError {
    Chord {
        source: ChordError,
        scope: DispatchScope,
    },
}

impl std::fmt::Display for RegistryKeyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Chord { source, .. } => write!(formatter, "{source}"),
        }
    }
}

struct RouteHandles<'a> {
    ctx: Option<&'a CtxArc>,
    app_state: &'a mut HookState<AppState>,
    should_quit: &'a mut HookState<bool>,
    suppress_next_enter: &'a mut HookState<PasteEnterSuppression>,
}

fn resolve_in_context(
    snapshot: &ActionRegistrySnapshot,
    context: ActionContext,
    key_event: &KeyEvent,
) -> Result<ResolvedRegistryKey, RegistryKeyError> {
    let chord = canonical_chord(key_event).map_err(|source| RegistryKeyError::Chord {
        source,
        scope: context.scope,
    })?;
    let resolution = snapshot.resolve(&chord, &context.stack);
    Ok(ResolvedRegistryKey {
        chord,
        scope: context.scope,
        resolution,
    })
}

fn route_provider_surface_control(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    key_event: &KeyEvent,
) -> bool {
    if !key_event.modifiers.is_empty() {
        return false;
    }
    let control = {
        let state = app_state.read();
        if !matches!(state.modal, jefe::state::ModalState::None)
            || (state.provider_requests.is_idle() && state.provider_surface_action.is_none())
        {
            return false;
        }
        let confirming = state.provider_requests.pending_confirmation_count() > 0;
        match key_event.code {
            iocraft::prelude::KeyCode::Enter if confirming => {
                crate::app_input::ProviderSurfaceControl::ActivateConfirmation
            }
            iocraft::prelude::KeyCode::Tab if confirming => {
                crate::app_input::ProviderSurfaceControl::CycleConfirmationFocus
            }
            iocraft::prelude::KeyCode::Enter => crate::app_input::ProviderSurfaceControl::Retry,
            iocraft::prelude::KeyCode::Esc => crate::app_input::ProviderSurfaceControl::Escape,
            _ => return false,
        }
    };
    crate::app_input::dispatch_provider_surface_control(app_state, &ctx.cloned(), control)
}

pub fn route_registry_key(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    should_quit: &mut HookState<bool>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    key_event: &KeyEvent,
) -> bool {
    if route_provider_surface_control(ctx, app_state, key_event) {
        return true;
    }

    let state = app_state.read();
    let input_mode = input_mode_for_state(&state);
    let context = match derive_action_context(&state, input_mode) {
        Ok(context) => context,
        Err(error) => {
            drop(state);
            app_state.write().warning_message = Some(error.to_string());
            return true;
        }
    };
    let snapshot = state.action_registry_snapshot.clone();
    drop(state);
    let scope = context.scope;
    let Some(snapshot) = snapshot else {
        app_state.write().warning_message = Some("Action registry is unavailable.".to_owned());
        return scope != DispatchScope::PreModeOnly;
    };
    let Some(_ctx_arc) = ctx else {
        return scope != DispatchScope::PreModeOnly;
    };
    let resolved = resolve_in_context(&snapshot, context, key_event);
    let mut handles = RouteHandles {
        ctx,
        app_state,
        should_quit,
        suppress_next_enter,
    };
    route_resolved(&mut handles, key_event, input_mode, resolved)
}

fn route_resolved(
    handles: &mut RouteHandles<'_>,
    key_event: &KeyEvent,
    input_mode: InputMode,
    resolved: Result<ResolvedRegistryKey, RegistryKeyError>,
) -> bool {
    let resolved = match resolved {
        Ok(resolved) => resolved,
        Err(RegistryKeyError::Chord { scope, .. }) => {
            let forwarded = matches!(
                scope,
                DispatchScope::ShellOverlay | DispatchScope::TerminalCapture
            );
            crate::action_capture_emit::record_untranslatable(key_event, forwarded);
            return route_untranslatable(
                scope,
                handles.ctx,
                handles.suppress_next_enter,
                key_event,
            );
        }
    };
    crate::action_capture_emit::record_key(key_event, &resolved.chord, &resolved.resolution);
    if matches!(
        resolved.scope,
        DispatchScope::ShellOverlay | DispatchScope::TerminalCapture
    ) {
        return route_pty_owned(handles, key_event, resolved);
    }
    if try_ctrl_c_interrupt_passthrough(
        handles.ctx,
        handles.suppress_next_enter,
        input_mode,
        key_event,
    ) {
        return true;
    }
    route_app_owned(handles, key_event, input_mode, resolved)
}

fn route_untranslatable(
    scope: DispatchScope,
    ctx: Option<&CtxArc>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    key_event: &KeyEvent,
) -> bool {
    if matches!(
        scope,
        DispatchScope::ShellOverlay | DispatchScope::TerminalCapture
    ) {
        forward_key_to_pty(ctx, suppress_next_enter, key_event);
        true
    } else {
        matches!(scope, DispatchScope::FullS3 | DispatchScope::FullS4)
    }
}

fn route_pty_owned(
    handles: &mut RouteHandles<'_>,
    key_event: &KeyEvent,
    resolved: ResolvedRegistryKey,
) -> bool {
    match resolved.resolution {
        Resolution::Dispatch { action, handler } => {
            execute_dispatch(handles, key_event, resolved.chord, action, handler)
        }
        Resolution::Unavailable { action, reason } => {
            record_unavailable(&mut handles.app_state.write(), Some(action), reason);
            true
        }
        Resolution::ForwardToPty | Resolution::Unbound => {
            forward_key_to_pty(handles.ctx, handles.suppress_next_enter, key_event);
            true
        }
    }
}

fn route_app_owned(
    handles: &mut RouteHandles<'_>,
    key_event: &KeyEvent,
    input_mode: InputMode,
    resolved: ResolvedRegistryKey,
) -> bool {
    match resolved.resolution {
        Resolution::Dispatch { action, handler } => {
            if resolved.scope == DispatchScope::PreModeOnly
                && !pre_mode_owned(handler, &handles.app_state.read(), input_mode)
            {
                return false;
            }
            execute_dispatch(handles, key_event, resolved.chord, action, handler)
        }
        Resolution::Unavailable { action, reason } => {
            record_unavailable(&mut handles.app_state.write(), Some(action), reason);
            true
        }
        Resolution::ForwardToPty => {
            forward_key_to_pty(handles.ctx, handles.suppress_next_enter, key_event);
            true
        }
        Resolution::Unbound
            if matches!(
                resolved.scope,
                DispatchScope::FullS3 | DispatchScope::FullS4
            ) =>
        {
            if rapid_quit_eligible(input_mode) {
                let _ = crate::app_input::observe_rapid_quit(
                    handles.app_state,
                    handles.should_quit,
                    key_event,
                );
            }
            true
        }
        Resolution::Unbound => false,
    }
}

fn record_unavailable(
    state: &mut AppState,
    action: Option<jefe::domain::action_registry::ActionId>,
    reason: String,
) {
    state.record_unavailable_action(action, reason);
}

fn rapid_quit_eligible(input_mode: InputMode) -> bool {
    matches!(
        input_mode,
        InputMode::Normal
            | InputMode::IssuesNormal
            | InputMode::PrsNormal
            | InputMode::ActionsNormal
    )
}

fn execute_dispatch(
    routes: &mut RouteHandles<'_>,
    key_event: &KeyEvent,
    chord: Chord,
    action: jefe::domain::action_registry::ActionId,
    handler: HandlerKey,
) -> bool {
    if matches!(handler, HandlerKey::ProviderAction) {
        return execute_provider_action(routes, action);
    }
    let execution = action_execution_for(
        handler,
        chord,
        &routes.app_state.read(),
        dashboard_page_items(routes.app_state),
    );
    apply_action_execution(
        execution,
        routes.app_state,
        routes.should_quit,
        &routes.ctx.cloned(),
        routes.suppress_next_enter,
        key_event,
    )
}

fn execute_provider_action(
    routes: &mut RouteHandles<'_>,
    action: jefe::domain::action_registry::ActionId,
) -> bool {
    let (screen, instance) = {
        let state = routes.app_state.read();
        let names = (
            state.screen().as_str().to_owned(),
            state.nav.current().id.to_string(),
        );
        drop(state);
        names
    };
    let Ok(context_screen) = jefe::domain::Id::parse(&screen) else {
        tracing::error!(screen = %screen, "current provider screen is not a valid domain id");
        return false;
    };
    let Ok(context_instance) = jefe::domain::Id::parse(&instance) else {
        tracing::error!(screen_instance = %instance, "current provider screen instance is not a valid domain id");
        return false;
    };
    crate::app_input::invoke_provider_action(
        routes.app_state,
        &routes.ctx.cloned(),
        &action,
        &context_screen,
        &context_instance,
        &jefe::domain::TypedMap::new(),
    )
}

pub struct MouseResolutionInput<'a> {
    pub chord: Chord,
    pub resolution: Resolution,
    pub key_event: &'a KeyEvent,
}

pub fn execute_mouse_resolution(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    should_quit: &mut HookState<bool>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    input: MouseResolutionInput<'_>,
) -> bool {
    let mut routes = RouteHandles {
        ctx,
        app_state,
        should_quit,
        suppress_next_enter,
    };
    match input.resolution {
        Resolution::Dispatch { action, handler } => {
            execute_dispatch(&mut routes, input.key_event, input.chord, action, handler)
        }
        Resolution::Unavailable { action, reason } => {
            record_unavailable(&mut routes.app_state.write(), Some(action), reason);
            true
        }
        Resolution::ForwardToPty | Resolution::Unbound => false,
    }
}

fn dashboard_page_items(app_state: &HookState<AppState>) -> PageItemCount {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let state = app_state.read();
    crate::app_input::dashboard_page_item_count(&state, state.compiled_screen(), cols, rows)
}

#[cfg(test)]
pub fn resolve_compiled_registry_key(
    state: &AppState,
    key_event: &KeyEvent,
) -> ResolvedRegistryKey {
    let snapshot = state.action_registry_snapshot.clone().unwrap_or_else(|| {
        let dir = std::env::temp_dir().join(format!("jefe_s3_route_{}", std::process::id()));
        let result = jefe::startup::build_persistence(Some(&dir));
        let Ok(startup) = result else {
            panic!("compiled S3 snapshot should compose, got {result:?}");
        };
        startup.keymap_snapshot
    });
    let context = derive_action_context(state, input_mode_for_state(state));
    let Ok(context) = context else {
        panic!("S3 context should derive, got {context:?}");
    };
    let result = resolve_in_context(&snapshot, context, key_event);
    let Ok(resolved) = result else {
        panic!("S3 key should resolve, got {result:?}");
    };
    resolved
}

#[cfg(test)]
#[path = "app_shell_key_routing_tests.rs"]
mod tests;
