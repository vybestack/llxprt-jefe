//! Focused source-state to action-context selection tests for issue #383 S3.

use jefe::domain::AgentId;
use jefe::input::InputMode;
use jefe::state::{AppState, ConfirmFocus, DashboardGrabPane, ModalState, PaneFocus, ScreenId};

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
    let mut state = crate::test_app_state();
    state.nav = jefe::state::navigation::NavState::rooted(ScreenId::Errors);
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
    let mut state = crate::test_app_state();
    state.pane_focus = PaneFocus::Terminal;
    state.terminal_focused = true;

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
    let mut state = crate::test_app_state();
    state.dashboard_grab = Some(DashboardGrabPane::Repository { visible_index: 0 });

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
fn actions_mode_is_full_s4_after_s4_migration() {
    let mut state = crate::test_app_state();
    state.nav = jefe::state::navigation::NavState::rooted(ScreenId::Actions);
    let result = derive_action_context(&state, InputMode::ActionsNormal);
    let Ok(context) = result else {
        panic!("actions S4 context should derive, got {result:?}");
    };
    assert_eq!(context.scope, DispatchScope::FullS4);
    assert_eq!(
        context
            .stack
            .iter()
            .map(jefe::domain::input_context::ContextId::as_str)
            .collect::<Vec<_>>(),
        vec!["actions.run-list", "actions", "global"]
    );
}

#[test]
fn issues_special_state_precedes_focused_panel_and_screen() {
    let mut state = crate::test_app_state();
    state.nav = jefe::state::navigation::NavState::rooted(ScreenId::Issues);
    state.issues_state.issue_focus = jefe::state::IssueFocus::IssueDetail;
    state.issues_state.property_editor = Some(jefe::state::IssuePropertyEditorState {
        kind: jefe::state::IssuePropertyKind::Title,
        options: Vec::new(),
        selected_index: 0,
        title_text: String::new(),
        title_cursor: 0,
        error: None,
        baseline: Vec::new(),
        loading_failed: false,
        options_loading: false,
        load_request_id: 0,
    });

    assert_eq!(
        context_names(&state),
        (
            DispatchScope::FullS4,
            vec!["issues.property".to_owned(), "global".to_owned()]
        )
    );
}
#[test]
fn dashboard_overlays_inherit_only_terminal_toggle_pre_mode_context() {
    let mut search = crate::test_app_state();
    search.dashboard_search.input_focused = true;
    assert_eq!(
        context_names(&search),
        (
            DispatchScope::FullS4,
            vec![
                "dashboard.search".to_owned(),
                "dashboard.pre-mode".to_owned(),
                "global".to_owned(),
            ]
        )
    );

    let mut modal = crate::test_app_state();
    modal.modal = ModalState::ConfirmDeleteRepository {
        id: jefe::domain::RepositoryId("repo".to_owned()),
        confirm_focus: ConfirmFocus::Confirm,
    };
    assert_eq!(
        context_names(&modal),
        (
            DispatchScope::FullS4,
            vec![
                "modal.confirm".to_owned(),
                "dashboard.pre-mode".to_owned(),
                "global".to_owned(),
            ]
        )
    );
}

#[test]
fn pr_changes_and_actions_focus_are_full_s4_contexts() {
    let mut prs = crate::test_app_state();
    prs.nav = jefe::state::navigation::NavState::rooted(ScreenId::PullRequests);
    prs.prs_state.pr_focus = jefe::state::PrFocus::PrChanges;
    assert_eq!(
        context_names(&prs),
        (
            DispatchScope::FullS4,
            vec![
                "prs.changes".to_owned(),
                "prs".to_owned(),
                "global".to_owned(),
            ]
        )
    );

    let mut actions = crate::test_app_state();
    actions.nav = jefe::state::navigation::NavState::rooted(ScreenId::Actions);
    actions.actions_state.focus = jefe::state::ActionsFocus::Detail;
    assert_eq!(
        context_names(&actions),
        (
            DispatchScope::FullS4,
            vec![
                "actions.detail".to_owned(),
                "actions".to_owned(),
                "global".to_owned(),
            ]
        )
    );
}

/// The Keys editor consumes its own input, but it deliberately lets `Ctrl+Q`
/// fall through so the protected emergency exit stays reachable. That only
/// works if the modal derives a valid context: a stack that repeats `global`
/// is rejected as a duplicate, which would swallow the exit instead.
#[test]
fn a_modal_context_keeps_the_protected_exit_reachable() {
    for screen in [
        ScreenId::Dashboard,
        ScreenId::Repositories,
        ScreenId::Actions,
        ScreenId::Issues,
        ScreenId::PullRequests,
        ScreenId::Errors,
        ScreenId::Terminals,
    ] {
        let mut state = crate::test_app_state();
        state.nav = jefe::state::navigation::NavState::rooted(screen);
        state.modal = ModalState::Help;

        let result = derive_action_context(&state, jefe::input::input_mode_for_state(&state));
        let Ok(context) = result else {
            panic!("a modal on {screen:?} should derive a context, got {result:?}");
        };
        let names: Vec<String> = context
            .stack
            .iter()
            .map(|value| value.as_str().to_owned())
            .collect();
        assert!(
            names.iter().any(|name| name == "global"),
            "a modal on {screen:?} must keep global reachable, got {names:?}"
        );
    }
}
