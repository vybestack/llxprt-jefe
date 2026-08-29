//! Single-resolution keyboard routing for the root application shell.

use iocraft::prelude::KeyEvent;

use crate::action_context::{ActionContext, DispatchScope, derive_action_context};
use crate::app_input::{
    action_execution_for, apply_action_execution, forward_key_to_pty, pre_mode_owned,
    try_ctrl_c_interrupt_passthrough,
};
use crate::app_shell::{CtxArc, HookState};
use crate::pty_encoding::PasteEnterSuppression;
use jefe::domain::action_registry::{HandlerKey, Resolution};
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

fn resolve_state_in_context(
    state: &AppState,
    context: ActionContext,
    key_event: &KeyEvent,
) -> Result<ResolvedRegistryKey, RegistryKeyError> {
    let chord = canonical_chord(key_event).map_err(|source| RegistryKeyError::Chord {
        source,
        scope: context.scope,
    })?;
    let resolution = state.resolve_action(&chord, &context.stack);
    Ok(ResolvedRegistryKey {
        chord,
        scope: context.scope,
        resolution,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderSurfaceKeyRoute {
    Dispatch(crate::app_input::ProviderSurfaceControl),
    Consume,
    Unhandled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProviderConfirmationEditRoute {
    Dispatch(jefe::domain::TypedValue),
    Draft(jefe::domain::TypedValue),
    Consume,
    Unhandled,
}

fn provider_confirmation_edit_route(
    field: &jefe::domain::plugin::field::Field,
    current: Option<&jefe::domain::TypedValue>,
    key_event: &KeyEvent,
) -> ProviderConfirmationEditRoute {
    use iocraft::prelude::{KeyCode, KeyModifiers};
    use jefe::domain::plugin::field::FieldKind;
    use jefe::form_value_edit::{FormValueEdit, edit_form_value, form_value_is_complete};

    // A continuation field owns every focused key: text kinds accept unmodified
    // characters and SHIFT-only characters (uppercase and shifted symbols), while
    // navigation/editing keys are consumed so they cannot leak to the global
    // stack beneath the blocking overlay. Table keys are never focused.
    let unmodified = key_event.modifiers.is_empty();
    let shift_only = key_event.modifiers == KeyModifiers::SHIFT;
    if !unmodified && !shift_only {
        return ProviderConfirmationEditRoute::Unhandled;
    }
    let edit = match (field.kind(), key_event.code) {
        (FieldKind::Boolean | FieldKind::Enum, KeyCode::Char(' ')) if unmodified => {
            FormValueEdit::Toggle
        }
        (FieldKind::Boolean | FieldKind::Enum, KeyCode::Char(_)) => {
            return ProviderConfirmationEditRoute::Consume;
        }
        (
            FieldKind::String
            | FieldKind::Path
            | FieldKind::Integer
            | FieldKind::FiniteNumber
            | FieldKind::StringList
            | FieldKind::SecretReference,
            KeyCode::Char(character),
        ) => FormValueEdit::Character(character),
        (_, KeyCode::Backspace) => FormValueEdit::Backspace,
        (
            _,
            KeyCode::Up
            | KeyCode::Down
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Delete
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::PageUp
            | KeyCode::PageDown,
        ) => return ProviderConfirmationEditRoute::Consume,
        _ => return ProviderConfirmationEditRoute::Unhandled,
    };
    edit_form_value(field, current, edit).map_or(ProviderConfirmationEditRoute::Consume, |value| {
        if form_value_is_complete(field, &value) {
            ProviderConfirmationEditRoute::Dispatch(value)
        } else {
            ProviderConfirmationEditRoute::Draft(value)
        }
    })
}

fn provider_surface_key_route(
    key_event: &KeyEvent,
    confirming: bool,
    provider_field_focused: bool,
) -> ProviderSurfaceKeyRoute {
    use crate::app_input::ProviderSurfaceControl;
    use iocraft::prelude::KeyCode;

    match key_event.code {
        KeyCode::Enter if confirming && provider_field_focused => ProviderSurfaceKeyRoute::Consume,
        KeyCode::Enter if confirming => {
            ProviderSurfaceKeyRoute::Dispatch(ProviderSurfaceControl::ActivateConfirmation)
        }
        KeyCode::Tab if confirming => {
            ProviderSurfaceKeyRoute::Dispatch(ProviderSurfaceControl::CycleConfirmationFocus)
        }
        KeyCode::Enter => ProviderSurfaceKeyRoute::Dispatch(ProviderSurfaceControl::Retry),
        KeyCode::Esc => ProviderSurfaceKeyRoute::Dispatch(ProviderSurfaceControl::Escape),
        _ => ProviderSurfaceKeyRoute::Unhandled,
    }
}

fn provider_surface_route_state(state: &AppState) -> Option<(bool, bool)> {
    let confirming = state.current_provider_confirmation().is_some();
    let overlay = state.active_overlay_kind();
    if !matches!(state.modal, jefe::state::ModalState::None)
        || state.shell_overlay_active()
        || input_mode_for_state(state) == InputMode::TerminalCapture
        || state.has_queued_current_provider_confirmation() && !confirming
        || overlay
            .is_some_and(|kind| kind != jefe::workbench::OverlayKind::Confirmation || !confirming)
        || (state.latest_current_provider_request().is_none()
            && state.provider_surface_action().is_none())
    {
        return None;
    }
    Some((
        confirming,
        state
            .nav
            .current()
            .overlays()
            .confirmation_focused_field()
            .is_some(),
    ))
}

fn route_provider_confirmation_edit(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    key_event: &KeyEvent,
) -> Option<bool> {
    let state = app_state.read();
    let field_id = state
        .nav
        .current()
        .overlays()
        .confirmation_focused_field()?
        .clone();
    let Some(field) = state
        .current_provider_confirmation()
        .and_then(|confirmation| {
            confirmation
                .continuation_schema()
                .iter()
                .find(|field| field.id() == &field_id)
        })
    else {
        // Field no longer resolves from the schema behind the same identity:
        // fail open to the control route so Esc/Tab/Enter keep working instead
        // of consuming every key.
        drop(state);
        return None;
    };
    let current = state
        .nav
        .current()
        .overlays()
        .confirmation_values()
        .and_then(|values| values.get(&field_id));
    let route = provider_confirmation_edit_route(field, current, key_event);
    drop(state);
    match route {
        ProviderConfirmationEditRoute::Dispatch(value) => {
            Some(crate::app_input::dispatch_provider_confirmation_field_edit(
                app_state,
                &ctx.cloned(),
                field_id,
                value,
            ))
        }
        ProviderConfirmationEditRoute::Draft(value) => Some(
            app_state
                .write()
                .set_provider_confirmation_draft(field_id, value),
        ),
        ProviderConfirmationEditRoute::Consume => Some(true),
        ProviderConfirmationEditRoute::Unhandled => None,
    }
}

fn route_provider_surface_control(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    key_event: &KeyEvent,
) -> bool {
    if !key_event.modifiers.is_empty() {
        return false;
    }
    let Some((confirming, provider_field_focused)) =
        provider_surface_route_state(&app_state.read())
    else {
        return false;
    };
    if let Some(handled) = route_provider_confirmation_edit(ctx, app_state, key_event) {
        return handled;
    }
    match provider_surface_key_route(key_event, confirming, provider_field_focused) {
        ProviderSurfaceKeyRoute::Dispatch(control) => {
            crate::app_input::dispatch_provider_surface_control(app_state, &ctx.cloned(), control)
        }
        ProviderSurfaceKeyRoute::Consume => true,
        ProviderSurfaceKeyRoute::Unhandled => false,
    }
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
    let scope = context.scope;
    let resolved = resolve_state_in_context(&state, context, key_event);
    drop(state);
    let Some(_ctx_arc) = ctx else {
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
    crate::app_input::invoke_provider_action(routes.app_state, &routes.ctx.cloned(), &action)
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
    let context = derive_action_context(state, input_mode_for_state(state));
    let Ok(context) = context else {
        panic!("S3 context should derive, got {context:?}");
    };
    let result = resolve_state_in_context(state, context, key_event);
    let Ok(resolved) = result else {
        panic!("S3 key should resolve, got {result:?}");
    };
    resolved
}

#[cfg(test)]
#[path = "app_shell_key_routing_tests.rs"]
mod tests;
