//! Cross-input invalidation for instance-owned mouse selection and gestures.

use crate::app_shell::HookState;
use jefe::selection::GestureState;
use jefe::state::AppState;

/// Clear the active selection and any pending terminal gesture when non-mouse
/// input changes the interaction context.
pub fn clear_selection(app_state: &mut HookState<AppState>) {
    let mut state = app_state.write();
    state.selection = None;
    state.selection_snapshot = None;
    state.selection_dashboard_git_info = None;
    state.terminal_gesture_state = GestureState::default();
}
