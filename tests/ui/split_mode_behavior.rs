//! Split mode behavior tests.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P10
//! @requirement REQ-FUNC-003
//! @pseudocode component-001 lines 21-28
//!
//! These tests verify split mode (repository management) behavior.

use jefe::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
use jefe::state::transition::TransitionExt;
use jefe::state::{AppEvent, AppState, ScreenMode};
use std::path::PathBuf;

/// Create a test state with multiple repositories.
fn create_split_test_state() -> AppState {
    let repo1 = Repository::new(
        RepositoryId("repo-1".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "llxprt-code".into(),
        "llxprt-code".into(),
        PathBuf::from("/projects/llxprt-code"),
    );
    let repo2 = Repository::new(
        RepositoryId("repo-2".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "starflight".into(),
        "starflight".into(),
        PathBuf::from("/projects/starflight"),
    );
    let repo3 = Repository::new(
        RepositoryId("repo-3".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "gable-work".into(),
        "gable-work".into(),
        PathBuf::from("/projects/gable-work"),
    );

    AppState {
        screen_mode: ScreenMode::Split,
        repositories: vec![repo1, repo2, repo3],
        selected_repository_index: Some(0),
        ..Default::default()
    }
}

// ============================================================================
// Enter/Exit Split Mode
// ============================================================================

#[test]
fn s_key_enters_split_mode() {
    let state = AppState {
        screen_mode: ScreenMode::Dashboard,
        ..Default::default()
    };

    let state = state.apply(AppEvent::EnterSplitMode).committed_pure();

    assert_eq!(state.screen_mode, ScreenMode::Split);
}

#[test]
fn esc_key_exits_split_mode() {
    let mut state = create_split_test_state();

    state = state.apply(AppEvent::ExitSplitMode).committed_pure();

    assert_eq!(state.screen_mode, ScreenMode::Dashboard);
}

// ============================================================================
// Grab Mode (REQ-FUNC-003)
// ============================================================================

#[test]
fn g_key_enters_grab_mode() {
    let mut state = create_split_test_state();
    state.split_grab_index = None;
    state.selected_repository_index = Some(1);

    state = state.apply(AppEvent::EnterGrabMode).committed_pure();

    assert_eq!(state.split_grab_index, Some(1));
}

#[test]
fn esc_key_exits_grab_mode() {
    let mut state = create_split_test_state();
    state.split_grab_index = Some(1);

    state = state.apply(AppEvent::ExitGrabMode).committed_pure();

    assert_eq!(state.split_grab_index, None);
}

#[test]
fn grab_mode_move_up_reorders_repository() {
    let mut state = create_split_test_state();
    state.selected_repository_index = Some(1);
    state.split_grab_index = Some(1);

    // Repo order: [llxprt-code, starflight, gable-work]
    // Move starflight (index 1) up

    state = state.apply(AppEvent::GrabMoveUp).committed_pure();

    // Expected order: [starflight, llxprt-code, gable-work]
    assert_eq!(state.repositories[0].name, "starflight");
    assert_eq!(state.repositories[1].name, "llxprt-code");
    assert_eq!(state.split_grab_index, Some(0));
    assert_eq!(state.selected_repository_index, Some(0));
}

#[test]
fn grab_mode_move_down_reorders_repository() {
    let mut state = create_split_test_state();
    state.selected_repository_index = Some(0);
    state.split_grab_index = Some(0);

    // Repo order: [llxprt-code, starflight, gable-work]
    // Move llxprt-code (index 0) down

    state = state.apply(AppEvent::GrabMoveDown).committed_pure();

    // Expected order: [starflight, llxprt-code, gable-work]
    assert_eq!(state.repositories[0].name, "starflight");
    assert_eq!(state.repositories[1].name, "llxprt-code");
    assert_eq!(state.split_grab_index, Some(1));
    assert_eq!(state.selected_repository_index, Some(1));
}

#[test]
fn grab_mode_uses_visible_index_space_when_idle_repositories_hidden() {
    let mut state = create_split_test_state();
    state.hide_idle_repositories = true;

    let repo1_id = state.repositories[0].id.clone();
    let repo2_id = state.repositories[1].id.clone();
    let repo3_id = state.repositories[2].id.clone();

    let mut repo1_running = Agent::new(
        AgentId("a1".into()),
        repo1_id.clone(),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Repo1 Running".into(),
        PathBuf::from("/projects/llxprt-code/a1"),
    );
    repo1_running.status = AgentStatus::Running;

    let repo2_idle = Agent::new(
        AgentId("a2".into()),
        repo2_id.clone(),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Repo2 Idle".into(),
        PathBuf::from("/projects/starflight/a2"),
    );

    let mut repo3_running = Agent::new(
        AgentId("a3".into()),
        repo3_id.clone(),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Repo3 Running".into(),
        PathBuf::from("/projects/gable-work/a3"),
    );
    repo3_running.status = AgentStatus::Running;

    state.agents = vec![repo1_running, repo2_idle, repo3_running];
    state.selected_repository_index = Some(2);

    state = state.apply(AppEvent::EnterGrabMode).committed_pure();
    assert_eq!(state.split_grab_index, Some(1));

    state = state.apply(AppEvent::GrabMoveUp).committed_pure();

    assert_eq!(state.repositories[0].id, repo3_id);
    assert_eq!(state.repositories[1].id, repo2_id);
    assert_eq!(state.repositories[2].id, repo1_id);
    assert_eq!(state.split_grab_index, Some(0));
    assert_eq!(state.selected_repository_index, Some(0));
}

#[test]
fn grab_mode_move_up_at_top_stays_at_top() {
    let mut state = create_split_test_state();
    state.selected_repository_index = Some(0);
    state.split_grab_index = Some(0);

    state = state.apply(AppEvent::GrabMoveUp).committed_pure();

    // Should stay at index 0
    assert_eq!(state.split_grab_index, Some(0));
    assert_eq!(state.repositories[0].name, "llxprt-code");
}

#[test]
fn grab_mode_move_down_at_bottom_stays_at_bottom() {
    let mut state = create_split_test_state();
    state.selected_repository_index = Some(2);
    state.split_grab_index = Some(2);

    state = state.apply(AppEvent::GrabMoveDown).committed_pure();

    // Should stay at index 2
    assert_eq!(state.split_grab_index, Some(2));
    assert_eq!(state.repositories[2].name, "gable-work");
}

// ============================================================================
// Repository Filtering in Split Mode
// ============================================================================

#[test]
fn split_mode_filter_by_repository_id() {
    let mut state = create_split_test_state();

    state = state
        .apply(AppEvent::SetSplitFilter(Some(RepositoryId(
            "repo-2".into(),
        ))))
        .committed_pure();

    assert_eq!(state.split_filter, Some(RepositoryId("repo-2".into())));
}

#[test]
fn split_mode_clear_filter() {
    let mut state = create_split_test_state();
    state.split_filter = Some(RepositoryId("repo-2".into()));

    state = state.apply(AppEvent::SetSplitFilter(None)).committed_pure();

    assert_eq!(state.split_filter, None);
}

// ============================================================================
// Navigation in Split Mode
// ============================================================================

#[test]
fn split_mode_navigate_down_increments_selection() {
    let mut state = create_split_test_state();
    state.selected_repository_index = Some(0);

    state = state.apply(AppEvent::NavigateDown).committed_pure();

    assert_eq!(state.selected_repository_index, Some(1));
}

#[test]
fn split_mode_navigate_up_decrements_selection() {
    let mut state = create_split_test_state();
    state.selected_repository_index = Some(1);

    state = state.apply(AppEvent::NavigateUp).committed_pure();

    assert_eq!(state.selected_repository_index, Some(0));
}
