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

pub fn route_registry_key(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    should_quit: &mut HookState<bool>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    key_event: &KeyEvent,
) -> bool {
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
    drop(state);
    let scope = context.scope;
    let Some(ctx_arc) = ctx else {
        return scope != DispatchScope::PreModeOnly;
    };
    let resolved = if let Ok(guard) = ctx_arc.lock() {
        resolve_in_context(&guard.keymap_snapshot, context, key_event)
    } else {
        app_state.write().warning_message =
            Some("Action registry context lock unavailable.".to_owned());
        return scope != DispatchScope::PreModeOnly;
    };
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
            return route_untranslatable(
                scope,
                handles.ctx,
                handles.suppress_next_enter,
                key_event,
            );
        }
    };
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
        scope == DispatchScope::FullS3
    }
}

fn route_pty_owned(
    handles: &mut RouteHandles<'_>,
    key_event: &KeyEvent,
    resolved: ResolvedRegistryKey,
) -> bool {
    match resolved.resolution {
        Resolution::Dispatch { handler, .. } => {
            let page_items = dashboard_page_items(handles.app_state);
            let execution = action_execution_for(
                handler,
                resolved.chord,
                &handles.app_state.read(),
                page_items,
            );
            apply_action_execution(
                execution,
                handles.app_state,
                handles.should_quit,
                &handles.ctx.cloned(),
                handles.suppress_next_enter,
                key_event,
            )
        }
        Resolution::Unavailable { reason, .. } => {
            handles.app_state.write().warning_message = Some(reason);
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
        Resolution::Dispatch { handler, .. } => {
            if resolved.scope == DispatchScope::PreModeOnly
                && !pre_mode_owned(handler, &handles.app_state.read(), input_mode)
            {
                return false;
            }
            execute_app_handler(handles, key_event, handler, resolved)
        }
        Resolution::Unavailable { reason, .. } => {
            handles.app_state.write().warning_message = Some(reason);
            true
        }
        Resolution::ForwardToPty => {
            forward_key_to_pty(handles.ctx, handles.suppress_next_enter, key_event);
            true
        }
        Resolution::Unbound if resolved.scope == DispatchScope::FullS3 => {
            let _ = crate::app_input::observe_rapid_quit(
                handles.app_state,
                handles.should_quit,
                key_event,
            );
            true
        }
        Resolution::Unbound => false,
    }
}

fn execute_app_handler(
    routes: &mut RouteHandles<'_>,
    key_event: &KeyEvent,
    handler: HandlerKey,
    resolved: ResolvedRegistryKey,
) -> bool {
    let execution = action_execution_for(
        handler,
        resolved.chord,
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

fn dashboard_page_items(app_state: &HookState<AppState>) -> PageItemCount {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let state = app_state.read();
    crate::app_input::dashboard_page_item_count(&state, state.screen_mode, cols, rows)
}

#[cfg(test)]
pub fn resolve_compiled_registry_key(
    state: &AppState,
    key_event: &KeyEvent,
) -> ResolvedRegistryKey {
    let dir = std::env::temp_dir().join(format!("jefe_s3_route_{}", std::process::id()));
    let result = jefe::startup::build_persistence(Some(&dir));
    let Ok(startup) = result else {
        panic!("compiled S3 snapshot should compose, got {result:?}");
    };
    let context = derive_action_context(state, input_mode_for_state(state));
    let Ok(context) = context else {
        panic!("S3 context should derive, got {context:?}");
    };
    let result = resolve_in_context(&startup.keymap_snapshot, context, key_event);
    let Ok(resolved) = result else {
        panic!("S3 key should resolve, got {result:?}");
    };
    resolved
}

#[cfg(test)]
#[path = "app_shell_key_routing_tests.rs"]
mod tests;
