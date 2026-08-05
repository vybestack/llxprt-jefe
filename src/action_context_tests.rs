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
    let mut state = AppState {
        nav: jefe::state::navigation::NavState::rooted(ScreenId::Errors),
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
fn actions_mode_is_full_s4_after_s4_migration() {
    let state = AppState {
        nav: jefe::state::navigation::NavState::rooted(ScreenId::Actions),
        ..AppState::default()
    };
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
    let mut state = AppState {
        nav: jefe::state::navigation::NavState::rooted(ScreenId::Issues),
        ..AppState::default()
    };
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
    let mut search = AppState::default();
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

    let modal = AppState {
        modal: ModalState::ConfirmDeleteRepository {
            id: jefe::domain::RepositoryId("repo".to_owned()),
            confirm_focus: ConfirmFocus::Confirm,
        },
        ..AppState::default()
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
    let mut prs = AppState {
        nav: jefe::state::navigation::NavState::rooted(ScreenId::PullRequests),
        ..AppState::default()
    };
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

    let mut actions = AppState {
        nav: jefe::state::navigation::NavState::rooted(ScreenId::Actions),
        ..AppState::default()
    };
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

/// Compose the shipped inventory into a snapshot, as the app does at startup.
fn compiled_snapshot() -> jefe::domain::action_registry::ActionRegistrySnapshot {
    use jefe::domain::Id;
    use jefe::domain::action_registry::{
        ActionAvailability, Availability, AvailabilityGeneration, RegistryCandidate,
    };
    use jefe::domain::effects::{Correlation, CorrelationId, EffectFamily, SemanticKey};
    use jefe::domain::input_context::ContextStack;

    let inventory = jefe::domain::default_action_inventory::compiled_inventory()
        .unwrap_or_else(|error| panic!("compiled inventory: {error}"));
    let entries = inventory
        .actions
        .iter()
        .map(|action| ActionAvailability::new(action.id.clone(), Availability::Available))
        .collect();
    let owner = Id::parse("core.keymap").unwrap_or_else(|error| panic!("owner: {error}"));
    let correlation_id = CorrelationId::new(1);
    let generation = AvailabilityGeneration::new(
        Correlation {
            correlation_id,
            owner,
            screen_generation: 0,
            activation_generation: 0,
            semantic_key: SemanticKey::new(EffectFamily::Provider, "action-availability"),
        },
        entries,
    );
    let stacks = inventory
        .bindings
        .iter()
        .filter_map(|binding| ContextStack::from_ordered([binding.context.as_str()], false).ok())
        .collect();
    RegistryCandidate::new(
        inventory.actions,
        inventory.bindings,
        Vec::new(),
        stacks,
        generation,
    )
    .compose()
    .unwrap_or_else(|error| panic!("compiled snapshot: {error}"))
}

/// The Keys editor consumes its own input, but it deliberately lets `Ctrl+Q`
/// fall through so the protected emergency exit stays reachable. That only
/// works if the modal derives a valid context: a stack that repeats `global`
/// is rejected as a duplicate, which would swallow the exit instead.
#[test]
fn a_modal_context_keeps_the_protected_exit_reachable() {
    let snapshot = compiled_snapshot();

    for screen in [
        ScreenId::Dashboard,
        ScreenId::Repositories,
        ScreenId::Actions,
        ScreenId::Issues,
        ScreenId::PullRequests,
        ScreenId::Errors,
        ScreenId::Terminals,
    ] {
        let mut state = AppState {
            nav: jefe::state::navigation::NavState::rooted(screen),
            ..AppState::default()
        };
        state.action_registry_snapshot = Some(snapshot.clone());
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
