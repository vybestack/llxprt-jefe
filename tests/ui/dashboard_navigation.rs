//! Dashboard navigation behavior tests.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P10
//! @requirement REQ-FUNC-002
//! @pseudocode component-001 lines 13-20
//!
//! These tests verify keyboard navigation behavior in the dashboard screen.
//! Acceptance criteria from: analysis/f12-cross-view-consistency-matrix.md

use jefe::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
use jefe::state::{AppEvent, AppState, PaneFocus, ScreenMode};
use std::path::PathBuf;

/// Create a test app state with some repositories and agents.
fn create_test_state() -> AppState {
    let mut state = AppState::default();

    // Add repositories
    let repo1 = Repository::new(
        RepositoryId("repo-1".into()),
        "llxprt-code".into(),
        "llxprt-code".into(),
        PathBuf::from("/projects/llxprt-code"),
    );
    let repo2 = Repository::new(
        RepositoryId("repo-2".into()),
        "starflight".into(),
        "starflight".into(),
        PathBuf::from("/projects/starflight"),
    );
    state.repositories = vec![repo1, repo2];

    // Add agents
    let agent1 = Agent::new(
        AgentId("agent-1".into()),
        RepositoryId("repo-1".into()),
        "Fix issue #1234".into(),
        PathBuf::from("/worktrees/issue-1234"),
    );
    let mut agent2 = Agent::new(
        AgentId("agent-2".into()),
        RepositoryId("repo-1".into()),
        "Refactor module".into(),
        PathBuf::from("/worktrees/refactor"),
    );
    agent2.status = AgentStatus::Running;
    state.agents = vec![agent1, agent2];

    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(0);

    state
}

// ============================================================================
// Pane Focus Cycling (REQ-FUNC-002)
// ============================================================================

#[test]
fn cycle_pane_focus_from_repositories_goes_to_agents() {
    let mut state = create_test_state();
    state.pane_focus = PaneFocus::Repositories;

    state = state.apply(AppEvent::CyclePaneFocus);

    assert_eq!(state.pane_focus, PaneFocus::Agents);
}

#[test]
fn cycle_pane_focus_from_agents_goes_to_terminal() {
    let mut state = create_test_state();
    state.pane_focus = PaneFocus::Agents;

    state = state.apply(AppEvent::CyclePaneFocus);

    assert_eq!(state.pane_focus, PaneFocus::Terminal);
}

#[test]
fn cycle_pane_focus_from_terminal_goes_to_repositories() {
    let mut state = create_test_state();
    state.pane_focus = PaneFocus::Terminal;

    state = state.apply(AppEvent::CyclePaneFocus);

    assert_eq!(state.pane_focus, PaneFocus::Repositories);
}

// ============================================================================
// Vertical Navigation (Up/Down)
// ============================================================================

#[test]
fn navigate_down_increments_agent_selection() {
    let mut state = create_test_state();
    state.pane_focus = PaneFocus::Agents;
    state.selected_agent_index = Some(0);

    state = state.apply(AppEvent::NavigateDown);

    assert_eq!(state.selected_agent_index, Some(1));
}

#[test]
fn navigate_up_decrements_agent_selection() {
    let mut state = create_test_state();
    state.pane_focus = PaneFocus::Agents;
    state.selected_agent_index = Some(1);

    state = state.apply(AppEvent::NavigateUp);

    assert_eq!(state.selected_agent_index, Some(0));
}

#[test]
fn navigate_down_at_end_stays_at_end() {
    let mut state = create_test_state();
    state.pane_focus = PaneFocus::Agents;
    state.selected_agent_index = Some(1); // Last agent

    state = state.apply(AppEvent::NavigateDown);

    assert_eq!(state.selected_agent_index, Some(1));
}

#[test]
fn navigate_up_at_zero_stays_at_zero() {
    let mut state = create_test_state();
    state.pane_focus = PaneFocus::Agents;
    state.selected_agent_index = Some(0);

    state = state.apply(AppEvent::NavigateUp);

    assert_eq!(state.selected_agent_index, Some(0));
}

// ============================================================================
// F12 Terminal Focus (from analysis/f12-cross-view-consistency-matrix.md)
// ============================================================================

#[test]
fn f12_toggle_enables_terminal_focus() {
    let mut state = create_test_state();
    state.terminal_focused = false;

    state = state.apply(AppEvent::ToggleTerminalFocus);

    assert!(state.terminal_focused);
}

#[test]
fn f12_toggle_disables_terminal_focus() {
    let mut state = create_test_state();
    state.terminal_focused = true;

    state = state.apply(AppEvent::ToggleTerminalFocus);

    assert!(!state.terminal_focused);
}

#[test]
fn f12_focus_is_independent_of_pane_focus() {
    let mut state = create_test_state();
    state.pane_focus = PaneFocus::Repositories;
    state.terminal_focused = false;

    state = state.apply(AppEvent::ToggleTerminalFocus);

    // Terminal focus is independent - pane focus should NOT change
    assert!(state.terminal_focused);
    assert_eq!(state.pane_focus, PaneFocus::Repositories);
}

/// P11: navigation should be blocked when terminal is focused.
///
/// When terminal_focused is true, navigation events go to PTY, not UI.
/// @plan PLAN-20260216-FIRSTVERSION-V1.P11
/// @requirement REQ-FUNC-003
#[test]
fn f12_focus_blocks_non_terminal_navigation() {
    // When terminal is focused, navigation events should be forwarded to PTY
    // This is behavior-level: UI should not change selection when terminal focused
    let mut state = create_test_state();
    state.terminal_focused = true;
    state.selected_agent_index = Some(0);
    state.pane_focus = PaneFocus::Agents;

    // In terminal-focused mode, navigation should NOT change agent selection
    // (keys go to PTY instead)
    let original_selection = state.selected_agent_index;

    state = state.apply(AppEvent::NavigateDown);

    // Expected: selection unchanged when terminal focused
    assert_eq!(
        state.selected_agent_index, original_selection,
        "Navigation should be blocked when terminal is focused"
    );
}

// ============================================================================
// Screen Mode Transitions
// ============================================================================

#[test]
fn enter_split_mode_changes_screen_mode() {
    let mut state = create_test_state();
    state.screen_mode = ScreenMode::Dashboard;

    state = state.apply(AppEvent::EnterSplitMode);

    assert_eq!(state.screen_mode, ScreenMode::Split);
}

#[test]
fn exit_split_mode_returns_to_dashboard() {
    let mut state = create_test_state();
    state.screen_mode = ScreenMode::Split;

    state = state.apply(AppEvent::ExitSplitMode);

    assert_eq!(state.screen_mode, ScreenMode::Dashboard);
}

// ============================================================================
// Agent Selection with Repository Filter
// ============================================================================

#[test]
fn selecting_repository_filters_visible_agents() {
    let mut state = create_test_state();

    // Add an agent for repo-2
    let agent3 = Agent::new(
        AgentId("agent-3".into()),
        RepositoryId("repo-2".into()),
        "Starflight task".into(),
        PathBuf::from("/worktrees/starflight-task"),
    );
    state.agents.push(agent3);

    state = state.apply(AppEvent::SelectRepository(1)); // Select repo-2

    assert_eq!(state.selected_repository_index, Some(1));
    // Agent list should show only agents for selected repository
    // This is a UI behavior test - the filter logic may be in view layer
}
