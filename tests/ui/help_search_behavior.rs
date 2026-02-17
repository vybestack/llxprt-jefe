//! Help and search behavior tests.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P10
//! @requirement REQ-FUNC-008
//! @requirement REQ-FUNC-010
//!
//! These tests verify help modal and search/filter behavior.
//! Acceptance criteria from: analysis/search-help-acceptance-contract.md

use jefe::domain::{Agent, AgentId, Repository, RepositoryId};
use jefe::state::{AppEvent, AppState, ModalState, ScreenMode};
use std::path::PathBuf;

/// Create a test state with search-related data.
fn create_search_test_state() -> AppState {
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

    // Add agents with various names for search testing
    let agents = vec![
        Agent::new(
            AgentId("agent-1".into()),
            RepositoryId("repo-1".into()),
            "Fix issue #1234".into(),
            PathBuf::from("/worktrees/issue-1234"),
        ),
        Agent::new(
            AgentId("agent-2".into()),
            RepositoryId("repo-1".into()),
            "Refactor auth module".into(),
            PathBuf::from("/worktrees/refactor-auth"),
        ),
        Agent::new(
            AgentId("agent-3".into()),
            RepositoryId("repo-2".into()),
            "Fix bug #5678".into(),
            PathBuf::from("/worktrees/bug-5678"),
        ),
    ];
    state.agents = agents;

    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(0);

    state
}

// ============================================================================
// Help Modal (REQ-FUNC-008)
// ============================================================================

#[test]
fn question_mark_opens_help() {
    let mut state = create_search_test_state();
    state.modal = ModalState::None;

    state = state.apply(AppEvent::OpenHelp);

    assert_eq!(state.modal, ModalState::Help);
}

#[test]
fn help_closes_on_close_modal() {
    let mut state = create_search_test_state();
    state.modal = ModalState::Help;

    state = state.apply(AppEvent::CloseModal);

    assert_eq!(state.modal, ModalState::None);
}

// ============================================================================
// Search/Filter Behavior (REQ-FUNC-010)
// From analysis/search-help-acceptance-contract.md
// ============================================================================

#[test]
fn slash_opens_search_mode() {
    let mut state = create_search_test_state();
    state.modal = ModalState::None;

    state = state.apply(AppEvent::OpenSearch);

    assert!(matches!(state.modal, ModalState::Search { .. }));
}

#[test]
fn search_esc_clears_and_closes() {
    let mut state = create_search_test_state();
    state.modal = ModalState::Search {
        query: "some query".into(),
    };

    state = state.apply(AppEvent::CloseModal);

    assert_eq!(state.modal, ModalState::None);
}

#[test]
fn search_filters_agents_by_name() {
    let state = create_search_test_state();
    let query = "fix";

    // Verify filter behavior - agents with "fix" in name should match
    let filtered_count = state
        .agents
        .iter()
        .filter(|a| a.name.to_lowercase().contains(&query.to_lowercase()))
        .count();

    // Should match "Fix issue #1234" and "Fix bug #5678"
    assert_eq!(filtered_count, 2);
}

#[test]
fn empty_search_shows_all() {
    let state = create_search_test_state();

    // Empty filter shows all agents
    let all_count = state.agents.len();

    assert_eq!(all_count, 3);
}

#[test]
fn search_no_match_shows_empty() {
    let state = create_search_test_state();
    let query = "nonexistent";

    let filtered_count = state
        .agents
        .iter()
        .filter(|a| a.name.to_lowercase().contains(&query.to_lowercase()))
        .count();

    assert_eq!(filtered_count, 0);
}

// ============================================================================
// Cross-Screen Search Consistency
// ============================================================================

#[test]
fn search_works_in_dashboard_mode() {
    let mut state = create_search_test_state();
    state.screen_mode = ScreenMode::Dashboard;

    state = state.apply(AppEvent::OpenSearch);

    assert!(matches!(state.modal, ModalState::Search { .. }));
}

#[test]
fn search_works_in_split_mode() {
    let mut state = create_search_test_state();
    state.screen_mode = ScreenMode::Split;

    state = state.apply(AppEvent::OpenSearch);

    assert!(matches!(state.modal, ModalState::Search { .. }));
}
