use super::transition::TransitionExt;
use super::{AppEvent, AppState, PaneFocus, ShellReturnTarget};
use crate::workbench::{ActivationValues, RouteId};

#[derive(Debug, Clone, Copy)]
struct ExpectedPresentation {
    repository_index: Option<usize>,
    agent_index: Option<usize>,
    pane_focus: PaneFocus,
    search_query: Option<&'static str>,
    repository_scroll: u32,
    agent_scroll: u32,
    terminal_history: Option<usize>,
    terminal_rows: usize,
}

const FRESH: ExpectedPresentation = ExpectedPresentation {
    repository_index: None,
    agent_index: None,
    pane_focus: PaneFocus::Repositories,
    search_query: None,
    repository_scroll: 0,
    agent_scroll: 0,
    terminal_history: None,
    terminal_rows: 0,
};

const FIRST: ExpectedPresentation = ExpectedPresentation {
    repository_index: Some(2),
    agent_index: Some(3),
    pane_focus: PaneFocus::Terminal,
    search_query: Some("first"),
    repository_scroll: 4,
    agent_scroll: 5,
    terminal_history: Some(7),
    terminal_rows: 19,
};

const SECOND: ExpectedPresentation = ExpectedPresentation {
    repository_index: Some(5),
    agent_index: Some(6),
    pane_focus: PaneFocus::Agents,
    search_query: Some("second"),
    repository_scroll: 8,
    agent_scroll: 9,
    terminal_history: Some(11),
    terminal_rows: 23,
};

fn configure_presentation(mut state: AppState, expected: ExpectedPresentation) -> AppState {
    state = state.apply(AppEvent::OpenSearch).committed_pure();
    for value in expected.search_query.unwrap_or_default().chars() {
        assert!(state.push_search_char(value));
    }
    state.selected_repository_index = expected.repository_index;
    state.selected_agent_index = expected.agent_index;
    state.pane_focus = expected.pane_focus;
    state.repository_scroll_offset = expected.repository_scroll;
    state.agent_scroll_offset = expected.agent_scroll;
    state.terminal_history_offset = expected.terminal_history;
    state.terminal_viewport_rows = expected.terminal_rows;
    state
}

fn assert_presentation(state: &AppState, expected: ExpectedPresentation) {
    assert_eq!(state.selected_repository_index, expected.repository_index);
    assert_eq!(state.selected_agent_index, expected.agent_index);
    assert_eq!(state.pane_focus, expected.pane_focus);
    assert_eq!(state.search_query(), expected.search_query);
    assert_eq!(state.repository_scroll_offset, expected.repository_scroll);
    assert_eq!(state.agent_scroll_offset, expected.agent_scroll);
    assert_eq!(state.terminal_history_offset, expected.terminal_history);
    assert_eq!(state.terminal_viewport_rows, expected.terminal_rows);
}

#[test]
fn same_definition_instances_restore_independent_dashboard_presentation() {
    let mut state = configure_presentation(AppState::test_fixture(), FIRST);
    let first_instance = state.nav.current().id;

    state.enter_provider_route(RouteId::from_static("dashboard"), ActivationValues::empty());
    let second_instance = state.nav.current().id;
    assert_ne!(second_instance, first_instance);
    assert_presentation(&state, FRESH);

    state = configure_presentation(state, SECOND);
    state.leave_screen();

    assert_eq!(state.nav.current().id, first_instance);
    assert_presentation(&state, FIRST);
}

#[test]
fn rendered_layout_publishes_only_to_its_exact_screen_instance() {
    let mut state = AppState::test_fixture();
    let first_instance = state.nav.current().id;
    let first_layout = crate::screen_layout::resolve_screen(&state, 120, 40)
        .unwrap_or_else(|| panic!("dashboard must resolve at the fixture terminal size"));

    assert!(state.publish_resolved_layout(first_instance, Some(first_layout.clone())));
    assert_eq!(state.resolved_layout.as_ref(), Some(&first_layout));

    state.enter_provider_route(RouteId::from_static("dashboard"), ActivationValues::empty());
    let second_instance = state.nav.current().id;
    assert_ne!(second_instance, first_instance);
    assert!(state.resolved_layout.is_none());
    assert!(!state.publish_resolved_layout(first_instance, Some(first_layout.clone())));
    assert!(!state.publish_resolved_layout(second_instance, Some(first_layout.clone())));
    assert!(state.resolved_layout.is_none());

    let second_layout = crate::screen_layout::resolve_screen(&state, 100, 30)
        .unwrap_or_else(|| panic!("second dashboard must resolve"));
    assert!(state.publish_resolved_layout(second_instance, Some(second_layout.clone())));
    assert_eq!(state.resolved_layout.as_ref(), Some(&second_layout));

    state.leave_screen();
    assert_eq!(state.nav.current().id, first_instance);
    assert_eq!(state.resolved_layout.as_ref(), Some(&first_layout));
}

#[derive(Debug)]
struct SavedInteraction {
    selection: crate::selection::TextSelection,
    snapshot: crate::runtime::TerminalSnapshot,
    git_snapshot: crate::dashboard_git_info::DashboardGitInfoSnapshot,
    point: crate::selection::SelectionPoint,
    shell_generation: u64,
}

fn configure_interaction(state: &mut AppState, agent: &crate::domain::AgentId) -> SavedInteraction {
    let point =
        crate::selection::SelectionPoint::new(crate::selection::SelectablePane::Preview, 2, 3);
    let selection = crate::selection::TextSelection::collapsed(point);
    let snapshot = crate::runtime::TerminalSnapshot::default();
    let git_snapshot = crate::dashboard_git_info::DashboardGitInfoSnapshot::default();
    state.pane_focus = PaneFocus::Agents;
    state.open_shell_overlay(agent.clone());
    state.shell_return_target = ShellReturnTarget::TerminalManager;
    state.selection = Some(selection);
    state.selection_snapshot = Some(snapshot.clone());
    state.selection_dashboard_git_info = Some(git_snapshot.clone());
    state.terminal_gesture_state = crate::selection::GestureState::JefeOwned { anchor: point };
    SavedInteraction {
        selection,
        snapshot,
        git_snapshot,
        point,
        shell_generation: state.shell_overlay.generation,
    }
}

fn assert_interaction_is_fresh(state: &AppState, agent: &crate::domain::AgentId) {
    assert!(state.selection.is_none());
    assert!(state.selection_snapshot.is_none());
    assert!(state.selection_dashboard_git_info.is_none());
    assert_eq!(
        state.terminal_gesture_state,
        crate::selection::GestureState::Idle
    );
    assert!(!state.shell_overlay_active());
    assert_eq!(state.shell_return_target, ShellReturnTarget::Dashboard);
    assert!(state.has_shell_window(agent));
}

fn assert_interaction_is_restored(
    state: &AppState,
    agent: &crate::domain::AgentId,
    saved: SavedInteraction,
) {
    assert_eq!(state.selection, Some(saved.selection));
    assert_eq!(state.selection_snapshot, Some(saved.snapshot));
    assert_eq!(state.selection_dashboard_git_info, Some(saved.git_snapshot));
    assert_eq!(
        state.terminal_gesture_state,
        crate::selection::GestureState::JefeOwned {
            anchor: saved.point
        }
    );
    assert_eq!(state.shell_overlay_agent_id(), Some(agent));
    assert_eq!(state.shell_overlay.generation, saved.shell_generation);
    assert_eq!(
        state.shell_overlay.previous_pane_focus,
        Some(PaneFocus::Agents)
    );
    assert_eq!(
        state.shell_return_target,
        ShellReturnTarget::TerminalManager
    );
    assert!(state.has_shell_window(agent));
}

#[test]
fn same_definition_instances_restore_selection_gesture_and_visible_shell_controller() {
    let mut state = AppState::test_fixture();
    let first_instance = state.nav.current().id;
    let agent = crate::domain::AgentId("agent-one".into());
    let saved = configure_interaction(&mut state, &agent);

    state.enter_provider_route(RouteId::from_static("dashboard"), ActivationValues::empty());
    assert_interaction_is_fresh(&state, &agent);
    state.selection = Some(crate::selection::TextSelection::collapsed(
        crate::selection::SelectionPoint::new(crate::selection::SelectablePane::Sidebar, 5, 1),
    ));

    state.leave_screen();
    assert_eq!(state.nav.current().id, first_instance);
    assert_interaction_is_restored(&state, &agent, saved);
}
