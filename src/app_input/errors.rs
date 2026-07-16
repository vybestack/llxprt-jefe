//! Errors-mode key dispatch (issue #292).
//!
//! The error log has no async operations — all keys are synchronous navigation,
//! focus, scroll, and clear. Esc exits to Dashboard (unless the detail pane is
//! focused, in which case it refocuses the list first).

use iocraft::prelude::*;

use jefe::state::{AppEvent, AppState, ErrorsFocus};

use super::{AppStateHandle, SharedContext};

/// Entry point: handle a key in DashboardErrors screen mode.
pub(super) fn handle_errors_mode_key(
    app_state: &AppStateHandle,
    _ctx: &SharedContext,
    key_event: &KeyEvent,
) -> Option<AppEvent> {
    let focus = app_state.read().errors_state.focus;

    match key_event.code {
        // Esc: if detail is focused, refocus the list first; otherwise exit.
        KeyCode::Esc if focus != ErrorsFocus::ErrorDetail => Some(AppEvent::ExitErrorsMode),
        KeyCode::Esc => Some(AppEvent::RefocusErrorList),
        // Navigation (works in all panes; repo nav when sidebar focused).
        KeyCode::Up => Some(AppEvent::ErrorsNavigateUp),
        KeyCode::Down => Some(AppEvent::ErrorsNavigateDown),
        KeyCode::Home => Some(AppEvent::ErrorsNavigateHome),
        KeyCode::End => Some(AppEvent::ErrorsNavigateEnd),
        KeyCode::PageUp => Some(AppEvent::ErrorsScrollDetailPageUp),
        KeyCode::PageDown => Some(AppEvent::ErrorsScrollDetailPageDown),
        // Enter: list → detail focus.
        KeyCode::Enter if focus == ErrorsFocus::ErrorList => Some(AppEvent::ErrorsEnter),
        // Tab/Right: cycle focus forward.
        KeyCode::Tab | KeyCode::Right => Some(AppEvent::ErrorsCycleFocus),
        // Left/Backtab: cycle focus reverse.
        KeyCode::BackTab | KeyCode::Left => Some(AppEvent::ErrorsCycleFocusReverse),
        // Ctrl-c/Ctrl-C: clear all errors (destructive — requires modifier
        // like other destructive actions in this codebase).
        KeyCode::Char('c' | 'C') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(AppEvent::ErrorsClearAll)
        }
        // Scroll detail (j/k as vim-like aliases).
        KeyCode::Char('j') if focus == ErrorsFocus::ErrorDetail => {
            Some(AppEvent::ErrorsScrollDetailDown)
        }
        KeyCode::Char('k') if focus == ErrorsFocus::ErrorDetail => {
            Some(AppEvent::ErrorsScrollDetailUp)
        }
        _ => None,
    }
}

/// Read-only access to errors focus for the dispatch gate in normal.rs.
#[allow(dead_code)]
fn _errors_focus(state: &AppState) -> ErrorsFocus {
    state.errors_state.focus
}
