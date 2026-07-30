//! Focused source-state to action-context selection tests for issue #383 S3.

use jefe::domain::AgentId;
use jefe::input::InputMode;
use jefe::state::{AppState, DashboardGrabPane, PaneFocus, ScreenMode};

use super::{DispatchScope, derive_action_context};

fn context_names(state: &AppState) -> (DispatchScope, Vec<String>) {
    let result = derive_action_context(state, jefe::input::input_mode_for_state(state));
    let Ok(context) = result else {
        panic!("state context should derive, got {result:?}");
    };
    (
        context.scope,
        context
            .stack
            .iter()
            .map(|value| value.as_str().to_owned())
            .collect(),
    )
}

#[test]
fn shell_overlay_has_absolute_context_precedence() {
    let mut state = AppState {
        screen_mode: ScreenMode::DashboardErrors,
        ..AppState::default()
    };
    state.open_shell_overlay(AgentId("agent-shell".to_owned()));

    assert_eq!(
        context_names(&state),
        (
            DispatchScope::ShellOverlay,
            vec!["shell-overlay".to_owned()]
        )
    );
}

#[test]
fn terminal_capture_uses_terminal_then_global() {
    let state = AppState {
        pane_focus: PaneFocus::Terminal,
        terminal_focused: true,
        ..AppState::default()
    };

    assert_eq!(
        context_names(&state),
        (
            DispatchScope::TerminalCapture,
            vec!["terminal".to_owned(), "global".to_owned()]
        )
    );
}

#[test]
fn dashboard_grab_uses_focused_child_before_dashboard() {
    let state = AppState {
        dashboard_grab: Some(DashboardGrabPane::Repository { visible_index: 0 }),
        ..AppState::default()
    };

    assert_eq!(
        context_names(&state),
        (
            DispatchScope::FullS3,
            vec![
                "dashboard.grab".to_owned(),
                "dashboard.reorder".to_owned(),
                "dashboard".to_owned(),
                "global".to_owned(),
            ]
        )
    );
}

#[test]
fn actions_mode_is_pre_mode_only_in_s3() {
    let state = AppState {
        screen_mode: ScreenMode::DashboardActions,
        ..AppState::default()
    };
    let result = derive_action_context(&state, InputMode::ActionsNormal);
    let Ok(context) = result else {
        panic!("actions pre-mode context should derive, got {result:?}");
    };
    assert_eq!(context.scope, DispatchScope::PreModeOnly);
    assert_eq!(
        context
            .stack
            .iter()
            .map(jefe::domain::input_context::ContextId::as_str)
            .collect::<Vec<_>>(),
        vec!["actions", "global"]
    );
}
