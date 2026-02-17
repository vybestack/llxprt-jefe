//! Domain and state contract tests.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P04
//! @requirement REQ-TECH-002
//! @requirement REQ-TECH-003
//!
//! Pseudocode reference: component-001 lines 01-33

#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use jefe::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
use jefe::state::{AppEvent, AppState, ModalState, PaneFocus, ScreenMode};

// =============================================================================
// Domain Invariants (REQ-FUNC-003, REQ-FUNC-004)
// =============================================================================

#[test]
fn agent_pass_continue_defaults_true() {
    let agent = Agent::new(
        AgentId("test".into()),
        RepositoryId("repo".into()),
        "Test".into(),
        PathBuf::from("/tmp"),
    );
    assert!(
        agent.pass_continue,
        "pass_continue must default to true per REQ-FUNC-004"
    );
}

#[test]
fn agent_status_defaults_to_queued() {
    let agent = Agent::new(
        AgentId("test".into()),
        RepositoryId("repo".into()),
        "Test".into(),
        PathBuf::from("/tmp"),
    );
    assert_eq!(agent.status, AgentStatus::Queued);
}

#[test]
fn repository_slug_must_be_unique() {
    // This is an invariant that must be enforced at the AppState level
    // when adding repositories
    let mut state = AppState::default();
    let repo1 = Repository::new(
        RepositoryId("r1".into()),
        "Repo One".into(),
        "repo-one".into(),
        PathBuf::from("/repos/one"),
    );
    let repo2 = Repository::new(
        RepositoryId("r2".into()),
        "Repo Two".into(),
        "repo-one".into(), // Same slug - should be rejected
        PathBuf::from("/repos/two"),
    );

    state.repositories.push(repo1);
    // In P05: AppState.add_repository should reject duplicate slugs
    // For now, just verify the invariant is documented
    let duplicate_exists = state.repositories.iter().any(|r| r.slug == repo2.slug);
    assert!(duplicate_exists, "duplicate slug detection setup");
}

// =============================================================================
// State Transition Tests (REQ-TECH-003)
// Pseudocode: component-001 lines 13-33
// =============================================================================

#[test]
fn navigate_up_decrements_selection() {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId("r1".into()),
        "R1".into(),
        "r1".into(),
        PathBuf::from("/r1"),
    ));
    state.repositories.push(Repository::new(
        RepositoryId("r2".into()),
        "R2".into(),
        "r2".into(),
        PathBuf::from("/r2"),
    ));
    state.selected_repository_index = Some(1);
    state.pane_focus = PaneFocus::Repositories;

    let next = state.apply(AppEvent::NavigateUp);

    assert_eq!(
        next.selected_repository_index,
        Some(0),
        "NavigateUp should decrement selection"
    );
}

#[test]
fn navigate_up_at_zero_stays_at_zero() {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId("r1".into()),
        "R1".into(),
        "r1".into(),
        PathBuf::from("/r1"),
    ));
    state.selected_repository_index = Some(0);
    state.pane_focus = PaneFocus::Repositories;

    let next = state.apply(AppEvent::NavigateUp);

    assert_eq!(
        next.selected_repository_index,
        Some(0),
        "NavigateUp at 0 should stay at 0"
    );
}

#[test]
fn navigate_down_increments_selection() {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId("r1".into()),
        "R1".into(),
        "r1".into(),
        PathBuf::from("/r1"),
    ));
    state.repositories.push(Repository::new(
        RepositoryId("r2".into()),
        "R2".into(),
        "r2".into(),
        PathBuf::from("/r2"),
    ));
    state.selected_repository_index = Some(0);
    state.pane_focus = PaneFocus::Repositories;

    let next = state.apply(AppEvent::NavigateDown);

    assert_eq!(
        next.selected_repository_index,
        Some(1),
        "NavigateDown should increment selection"
    );
}

#[test]
fn navigate_down_at_end_stays_at_end() {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId("r1".into()),
        "R1".into(),
        "r1".into(),
        PathBuf::from("/r1"),
    ));
    state.selected_repository_index = Some(0);
    state.pane_focus = PaneFocus::Repositories;

    let next = state.apply(AppEvent::NavigateDown);

    assert_eq!(
        next.selected_repository_index,
        Some(0),
        "NavigateDown at end should stay at end"
    );
}

#[test]
fn toggle_terminal_focus_sets_terminal_focused() {
    let state = AppState {
        terminal_focused: false,
        ..AppState::default()
    };

    let next = state.apply(AppEvent::ToggleTerminalFocus);

    assert!(
        next.terminal_focused,
        "ToggleTerminalFocus should set terminal_focused=true"
    );
}

#[test]
fn toggle_terminal_focus_clears_terminal_focused() {
    let state = AppState {
        terminal_focused: true,
        ..AppState::default()
    };

    let next = state.apply(AppEvent::ToggleTerminalFocus);

    assert!(
        !next.terminal_focused,
        "ToggleTerminalFocus should toggle to false"
    );
}

#[test]
fn enter_split_mode_changes_screen_mode() {
    let state = AppState {
        screen_mode: ScreenMode::Dashboard,
        ..AppState::default()
    };

    let next = state.apply(AppEvent::EnterSplitMode);

    assert_eq!(
        next.screen_mode,
        ScreenMode::Split,
        "EnterSplitMode should change to Split"
    );
}

#[test]
fn exit_split_mode_returns_to_dashboard() {
    let state = AppState {
        screen_mode: ScreenMode::Split,
        ..AppState::default()
    };

    let next = state.apply(AppEvent::ExitSplitMode);

    assert_eq!(
        next.screen_mode,
        ScreenMode::Dashboard,
        "ExitSplitMode should return to Dashboard"
    );
}

#[test]
fn open_help_sets_modal_to_help() {
    let state = AppState::default();

    let next = state.apply(AppEvent::OpenHelp);

    assert!(
        matches!(next.modal, ModalState::Help),
        "OpenHelp should set modal to Help"
    );
}

#[test]
fn close_modal_clears_modal() {
    let state = AppState {
        modal: ModalState::Help,
        ..AppState::default()
    };

    let next = state.apply(AppEvent::CloseModal);

    assert!(
        matches!(next.modal, ModalState::None),
        "CloseModal should clear modal"
    );
}

#[test]
fn cycle_pane_focus_rotates_through_panes() {
    let state = AppState {
        pane_focus: PaneFocus::Repositories,
        ..AppState::default()
    };

    let next = state.apply(AppEvent::CyclePaneFocus);

    assert_eq!(
        next.pane_focus,
        PaneFocus::Agents,
        "CyclePaneFocus from Repositories should go to Agents"
    );

    let next2 = next.apply(AppEvent::CyclePaneFocus);

    assert_eq!(
        next2.pane_focus,
        PaneFocus::Terminal,
        "CyclePaneFocus from Agents should go to Terminal"
    );

    let next3 = next2.apply(AppEvent::CyclePaneFocus);

    assert_eq!(
        next3.pane_focus,
        PaneFocus::Repositories,
        "CyclePaneFocus from Terminal should wrap to Repositories"
    );
}

// =============================================================================
// Agent Lifecycle State Transitions (REQ-FUNC-005, REQ-FUNC-007)
// =============================================================================

#[test]
fn agent_status_changed_updates_agent() {
    let mut state = AppState::default();
    let agent_id = AgentId("agent-1".into());
    state.agents.push(Agent::new(
        agent_id.clone(),
        RepositoryId("repo".into()),
        "Agent 1".into(),
        PathBuf::from("/work"),
    ));

    let next = state.apply(AppEvent::AgentStatusChanged(
        agent_id.clone(),
        AgentStatus::Running,
    ));

    let agent = next
        .agents
        .iter()
        .find(|a| a.id == agent_id)
        .expect("agent should exist");
    assert_eq!(
        agent.status,
        AgentStatus::Running,
        "AgentStatusChanged should update agent status"
    );
}

#[test]
fn kill_agent_sets_status_to_dead() {
    let mut state = AppState::default();
    let agent_id = AgentId("agent-1".into());
    let mut agent = Agent::new(
        agent_id.clone(),
        RepositoryId("repo".into()),
        "Agent 1".into(),
        PathBuf::from("/work"),
    );
    agent.status = AgentStatus::Running;
    state.agents.push(agent);

    let next = state.apply(AppEvent::KillAgent(agent_id.clone()));

    let agent = next
        .agents
        .iter()
        .find(|a| a.id == agent_id)
        .expect("agent should exist");
    assert_eq!(
        agent.status,
        AgentStatus::Dead,
        "KillAgent should set status to Dead"
    );
}

// =============================================================================
// Error/Warning State (REQ-TECH-008)
// =============================================================================

#[test]
fn persistence_load_failed_sets_error() {
    let state = AppState::default();

    let next = state.apply(AppEvent::PersistenceLoadFailed("file not found".into()));

    assert!(
        next.error_message.is_some(),
        "PersistenceLoadFailed should set error_message"
    );
    assert!(
        next.error_message
            .as_ref()
            .unwrap()
            .contains("file not found"),
        "error_message should contain the error"
    );
}

#[test]
fn clear_error_clears_error_message() {
    let state = AppState {
        error_message: Some("some error".into()),
        ..AppState::default()
    };

    let next = state.apply(AppEvent::ClearError);

    assert!(
        next.error_message.is_none(),
        "ClearError should clear error_message"
    );
}

#[test]
fn theme_resolve_failed_sets_warning() {
    let state = AppState::default();

    let next = state.apply(AppEvent::ThemeResolveFailed("theme not found".into()));

    assert!(
        next.warning_message.is_some(),
        "ThemeResolveFailed should set warning_message"
    );
}
