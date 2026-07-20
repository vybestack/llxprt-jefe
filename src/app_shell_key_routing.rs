//! Key-routing helpers for the root application shell.

use iocraft::prelude::{KeyCode, KeyEvent};

use crate::app_input::{
    forward_key_to_pty, handle_f12_toggle, handle_global_shortcut_key,
    try_intercept_terminal_scrollback,
};
use crate::app_shell::{CtxArc, HookState};
use crate::pty_encoding::PasteEnterSuppression;

use jefe::input::InputMode;
use jefe::state::{AppState, ScreenMode};

pub fn route_shell_overlay_key(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    key_event: &KeyEvent,
) -> bool {
    if !crate::app_input::shell_overlay::try_close_shell_overlay(
        app_state,
        &ctx.cloned(),
        key_event,
    ) {
        forward_key_to_pty(ctx, suppress_next_enter, key_event);
    }
    true
}

pub fn route_terminal_capture_key(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    input_mode: InputMode,
    key_event: &KeyEvent,
) -> bool {
    if input_mode != InputMode::TerminalCapture {
        return false;
    }
    if !try_intercept_terminal_scrollback(app_state, &ctx.cloned(), key_event) {
        forward_key_to_pty(ctx, suppress_next_enter, key_event);
    }
    true
}

pub fn handle_pre_mode_shortcut(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    key_event: &KeyEvent,
    screen_mode: ScreenMode,
    terminal_focused: bool,
    input_mode: InputMode,
) -> bool {
    if key_event.code == KeyCode::F(12)
        && matches!(
            screen_mode,
            ScreenMode::Dashboard | ScreenMode::Split | ScreenMode::DashboardActions
        )
    {
        handle_f12_toggle(app_state, &ctx.cloned());
        return true;
    }
    if handle_global_shortcut_key(app_state, &ctx.cloned(), key_event) {
        return true;
    }
    input_mode == InputMode::Normal
        && screen_mode == ScreenMode::Dashboard
        && !terminal_focused
        && crate::app_input::shell_overlay::handle_shell_shortcut_key(
            app_state,
            &ctx.cloned(),
            key_event,
        )
}
