//! Split mode behavior tests.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P10
//! @requirement REQ-FUNC-003
//! @pseudocode component-001 lines 21-28
//!
//! These tests verify split mode (repository management) behavior.

use jefe::domain::{Repository, RepositoryId};
use jefe::state::{AppEvent, AppState, ScreenMode};
use std::path::PathBuf;

/// Create a test state with multiple repositories.
fn create_split_test_state() -> AppState {
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
    let repo3 = Repository::new(
        RepositoryId("repo-3".into()),
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

    let state = state.apply(AppEvent::EnterSplitMode);

    assert_eq!(state.screen_mode, ScreenMode::Split);
}

#[test]
fn esc_key_exits_split_mode() {
    let mut state = create_split_test_state();

    state = state.apply(AppEvent::ExitSplitMode);

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

    state = state.apply(AppEvent::EnterGrabMode);

    assert_eq!(state.split_grab_index, Some(1));
}

#[test]
fn esc_key_exits_grab_mode() {
    let mut state = create_split_test_state();
    state.split_grab_index = Some(1);

    state = state.apply(AppEvent::ExitGrabMode);

    assert_eq!(state.split_grab_index, None);
}

#[test]
fn grab_mode_move_up_reorders_repository() {
    let mut state = create_split_test_state();
    state.selected_repository_index = Some(1);
    state.split_grab_index = Some(1);

    // Repo order: [llxprt-code, starflight, gable-work]
    // Move starflight (index 1) up

    state = state.apply(AppEvent::GrabMoveUp);

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

    state = state.apply(AppEvent::GrabMoveDown);

    // Expected order: [starflight, llxprt-code, gable-work]
    assert_eq!(state.repositories[0].name, "starflight");
    assert_eq!(state.repositories[1].name, "llxprt-code");
    assert_eq!(state.split_grab_index, Some(1));
    assert_eq!(state.selected_repository_index, Some(1));
}

#[test]
fn grab_mode_move_up_at_top_stays_at_top() {
    let mut state = create_split_test_state();
    state.selected_repository_index = Some(0);
    state.split_grab_index = Some(0);

    state = state.apply(AppEvent::GrabMoveUp);

    // Should stay at index 0
    assert_eq!(state.split_grab_index, Some(0));
    assert_eq!(state.repositories[0].name, "llxprt-code");
}

#[test]
fn grab_mode_move_down_at_bottom_stays_at_bottom() {
    let mut state = create_split_test_state();
    state.selected_repository_index = Some(2);
    state.split_grab_index = Some(2);

    state = state.apply(AppEvent::GrabMoveDown);

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

    state = state.apply(AppEvent::SetSplitFilter(Some(RepositoryId(
        "repo-2".into(),
    ))));

    assert_eq!(state.split_filter, Some(RepositoryId("repo-2".into())));
}

#[test]
fn split_mode_clear_filter() {
    let mut state = create_split_test_state();
    state.split_filter = Some(RepositoryId("repo-2".into()));

    state = state.apply(AppEvent::SetSplitFilter(None));

    assert_eq!(state.split_filter, None);
}

// ============================================================================
// Navigation in Split Mode
// ============================================================================

#[test]
fn split_mode_navigate_down_increments_selection() {
    let mut state = create_split_test_state();
    state.selected_repository_index = Some(0);

    state = state.apply(AppEvent::NavigateDown);

    assert_eq!(state.selected_repository_index, Some(1));
}

#[test]
fn split_mode_navigate_up_decrements_selection() {
    let mut state = create_split_test_state();
    state.selected_repository_index = Some(1);

    state = state.apply(AppEvent::NavigateUp);

    assert_eq!(state.selected_repository_index, Some(0));
}
