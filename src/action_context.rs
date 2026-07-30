//! Pure selection of the action-registry context for one application state.
//!
//! The selector encodes source routing precedence without resolving a key. S3
//! contexts are fully registry-owned; later-slice screens expose only their
//! inherited global/pre-mode seam.

use jefe::domain::input_context::{ContextStack, ContextStackError};
use jefe::input::InputMode;
use jefe::state::{AppState, PaneFocus, ScreenMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchScope {
    ShellOverlay,
    TerminalCapture,
    FullS3,
    PreModeOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionContext {
    pub stack: ContextStack,
    pub scope: DispatchScope,
}

pub fn derive_action_context(
    state: &AppState,
    input_mode: InputMode,
) -> Result<ActionContext, ContextStackError> {
    if state.shell_overlay_active() {
        return action_context(&["shell-overlay"], false, DispatchScope::ShellOverlay);
    }
    if input_mode == InputMode::TerminalCapture {
        return action_context(
            &["terminal", "global"],
            true,
            DispatchScope::TerminalCapture,
        );
    }
    match (state.screen_mode, input_mode) {
        (ScreenMode::Dashboard, InputMode::Normal) => dashboard_context(state),
        (ScreenMode::Split, InputMode::Normal) => {
            action_context(&["split", "global"], false, DispatchScope::FullS3)
        }
        (ScreenMode::DashboardErrors, InputMode::Normal) => {
            action_context(&["errors", "global"], false, DispatchScope::FullS3)
        }
        (ScreenMode::DashboardTerminals, InputMode::Normal) => action_context(
            &["terminal-manager", "global"],
            false,
            DispatchScope::FullS3,
        ),
        (ScreenMode::DashboardActions, _) => {
            action_context(&["actions", "global"], false, DispatchScope::PreModeOnly)
        }
        (ScreenMode::Dashboard, _) => {
            action_context(&["dashboard", "global"], false, DispatchScope::PreModeOnly)
        }
        (ScreenMode::Split, _) => {
            action_context(&["split", "global"], false, DispatchScope::PreModeOnly)
        }
        _ => action_context(&["global"], false, DispatchScope::PreModeOnly),
    }
}

fn dashboard_context(state: &AppState) -> Result<ActionContext, ContextStackError> {
    if state.dashboard_grab.is_some() {
        return action_context(
            &["dashboard.grab", "dashboard.reorder", "dashboard", "global"],
            false,
            DispatchScope::FullS3,
        );
    }
    if matches!(
        state.pane_focus,
        PaneFocus::Repositories | PaneFocus::Agents
    ) {
        return action_context(
            &["dashboard.reorder", "dashboard", "global"],
            false,
            DispatchScope::FullS3,
        );
    }
    action_context(&["dashboard", "global"], false, DispatchScope::FullS3)
}

fn action_context(
    contexts: &[&str],
    terminal_capture: bool,
    scope: DispatchScope,
) -> Result<ActionContext, ContextStackError> {
    ContextStack::from_ordered(contexts.iter().copied(), terminal_capture)
        .map(|stack| ActionContext { stack, scope })
}

#[cfg(test)]
#[path = "action_context_tests.rs"]
mod tests;
