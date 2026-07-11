//! Pull Requests Mode repo-navigation tests — focus cycle, repo nav
//! independent of pane_focus (#47), repo-scope reset, select_repository
//! resets PR scope.
//!
//! @plan PLAN-20260624-PR-MODE.P04
//! @requirement REQ-PR-003

use crate::domain::{PrCheckStatus, PrState, PullRequest, Repository, RepositoryId};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::types::{PaneFocus, PrDetailSubfocus, PrFocus, ScreenMode};

/// Helper: PR-mode state with multiple repositories.
fn prs_mode_state() -> AppState {
    let mut state = AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        ..AppState::default()
    };
    for slug in ["repo-1", "repo-2", "repo-3"] {
        state.repositories.push(Repository::new(
            RepositoryId(slug.to_string()),
            slug.to_string(),
            slug.to_string(),
            std::path::PathBuf::from(format!("/tmp/{slug}")),
        ));
    }
    state.selected_repository_index = Some(0);
    state.prs_state.active = true;
    state
}

/// Helper: minimal PR list-row.
fn make_test_pr(number: u64) -> PullRequest {
    PullRequest {
        number,
        title: format!("PR #{number}"),
        state: PrState::Open,
        author_login: "testuser".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        base_ref: "main".to_string(),
        is_draft: false,
        review_decision: None,
        checks_status: PrCheckStatus::None,
        assignee_summary: String::new(),
        labels_summary: String::new(),
        comment_count: 0,
    }
}

/// PrCycleFocus must cycle RepoList → PrList → PrDetail → RepoList (wrap).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-003
/// @pseudocode component-001 lines 154-162
#[test]
fn test_cycle_focus_repo_to_list_to_detail_and_wrap() {
    let mut state = prs_mode_state();
    state.prs_state.pr_focus = PrFocus::RepoList;

    // RepoList → PrList
    let state = state.apply(AppEvent::PrCycleFocus);
    assert_eq!(state.prs_state.pr_focus, PrFocus::PrList);

    // PrList → PrDetail
    let state = state.apply(AppEvent::PrCycleFocus);
    assert_eq!(state.prs_state.pr_focus, PrFocus::PrDetail);

    // PrDetail → RepoList (wrap)
    let state = state.apply(AppEvent::PrCycleFocus);
    assert_eq!(state.prs_state.pr_focus, PrFocus::RepoList);
}

/// PrCycleFocusReverse must cycle RepoList → PrDetail → PrList → RepoList.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-003
/// @pseudocode component-001 lines 154-162
#[test]
fn test_cycle_focus_reverse() {
    let mut state = prs_mode_state();
    state.prs_state.pr_focus = PrFocus::RepoList;

    let state = state.apply(AppEvent::PrCycleFocusReverse);
    assert_eq!(state.prs_state.pr_focus, PrFocus::PrDetail);

    let state = state.apply(AppEvent::PrCycleFocusReverse);
    assert_eq!(state.prs_state.pr_focus, PrFocus::PrList);

    let state = state.apply(AppEvent::PrCycleFocusReverse);
    assert_eq!(state.prs_state.pr_focus, PrFocus::RepoList);
}

/// Repo navigation in PR mode must work independent of pane_focus (#47) —
/// Up/Down in the RepoList focus changes selected_repository_index even when
/// pane_focus is not Repositories.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-003
/// @pseudocode component-001 lines 146-153
#[test]
fn test_navigate_repo_in_prs_mode_changes_selection_independent_of_pane_focus() {
    let mut state = prs_mode_state();
    state.prs_state.pr_focus = PrFocus::RepoList;
    // Set pane_focus to Agents (the #47 bug scenario).
    state.pane_focus = PaneFocus::Agents;

    // Down should move to repo index 1 even though pane_focus is Agents.
    let state = state.apply(AppEvent::PrNavigateDown);
    assert_eq!(state.selected_repository_index, Some(1));

    // Down again to repo index 2.
    let state = state.apply(AppEvent::PrNavigateDown);
    assert_eq!(state.selected_repository_index, Some(2));

    // Down at bottom stays.
    let state = state.apply(AppEvent::PrNavigateDown);
    assert_eq!(state.selected_repository_index, Some(2));

    // Up moves back.
    let state = state.apply(AppEvent::PrNavigateUp);
    assert_eq!(state.selected_repository_index, Some(1));

    // Up at top stays.
    let state = state.apply(AppEvent::PrNavigateUp);
    let state = state.apply(AppEvent::PrNavigateUp);
    assert_eq!(state.selected_repository_index, Some(0));
}

/// Repo-scope change in PR mode must reset the PR list, detail, and pending
/// guards (staleness invalidation).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-003
/// @pseudocode component-001 lines 88-98
#[test]
fn test_repo_scope_change_resets_pr_list_detail_and_pending() {
    let mut state = prs_mode_state();
    state.prs_state.pr_focus = PrFocus::RepoList;
    state.pane_focus = PaneFocus::Agents;
    // Populate PR data that should be cleared on repo change.
    state.prs_state.pull_requests = vec![make_test_pr(1), make_test_pr(2)];
    state.prs_state.selected_pr_index = Some(1);
    state.prs_state.detail_scroll_offset = 5;
    state.prs_state.detail_subfocus = PrDetailSubfocus::Review(0);
    state.prs_state.loading.list = false;

    let new_state = state.apply(AppEvent::PrNavigateDown);

    assert_eq!(new_state.selected_repository_index, Some(1));
    assert!(
        new_state.prs_state.pull_requests.is_empty(),
        "pull_requests must be cleared on repo change"
    );
    assert_eq!(
        new_state.prs_state.selected_pr_index, None,
        "selected_pr_index must be cleared"
    );
    assert!(
        new_state.prs_state.pr_detail.is_none(),
        "pr_detail must be cleared"
    );
    assert_eq!(
        new_state.prs_state.detail_scroll_offset, 0,
        "detail_scroll_offset must be reset"
    );
}

/// select_repository_by_index while prs_state.active must invoke
/// reset_prs_for_repo_change (clears PR list/detail/pending, resets
/// selection/cursors). RED now because P03 does NOT wire the reset into
/// select_repository_by_index; GREEN in P05.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-003
/// @pseudocode component-001 lines 88-98
#[test]
fn test_select_repository_resets_pr_scope() {
    let mut state = prs_mode_state();
    state.prs_state.active = true;
    // Populate PR data that should be cleared on select.
    state.prs_state.pull_requests = vec![make_test_pr(10), make_test_pr(20)];
    state.prs_state.selected_pr_index = Some(1);
    state.prs_state.loading.list = false;

    // Select a different repository via the UiNavigation message path.
    let new_state = state.apply(AppEvent::SelectRepository(1));

    assert_eq!(new_state.selected_repository_index, Some(1));
    assert!(
        new_state.prs_state.pull_requests.is_empty(),
        "select_repository must reset PR list when prs_state is active"
    );
    assert_eq!(
        new_state.prs_state.selected_pr_index, None,
        "select_repository must clear selected_pr_index when prs_state is active"
    );
}

/// MED-3: Pressing Down when NO repository is selected must select the FIRST
/// visible repo (index 0), NOT skip to index 1. The old
/// `unwrap_or(0)`+Down logic computed target=1 and skipped the first repo.
///
/// @plan PLAN-20260624-PR-MODE.P05
/// @requirement REQ-PR-003
/// @pseudocode component-001 lines 134-145
#[test]
fn test_navigate_repo_down_from_none_selects_first_visible() {
    let mut state = prs_mode_state();
    state.prs_state.pr_focus = PrFocus::RepoList;
    // No repo selected.
    state.selected_repository_index = None;

    let new_state = state.apply(AppEvent::PrNavigateDown);

    assert_eq!(
        new_state.selected_repository_index,
        Some(0),
        "Down from no selection MUST select the FIRST visible repo (indices[0]), not skip it"
    );
}

/// MED-3: Pressing Up when NO repository is selected is a no-op: the shared
/// helper returns false and leaves the selection as-is (None). We test the
/// helper directly because the public `apply` path runs
/// `normalize_selection_indices` afterwards, which auto-selects the first
/// visible repo whenever the selection is None — so the no-op is only
/// observable at the `move_repo_selection` boundary (the unit under test).
///
/// @plan PLAN-20260624-PR-MODE.P05
/// @requirement REQ-PR-003
/// @pseudocode component-001 lines 134-145
#[test]
fn test_navigate_repo_up_from_none_is_noop() {
    use crate::messages::NavDir;
    let mut state = prs_mode_state();
    state.prs_state.pr_focus = PrFocus::RepoList;
    state.selected_repository_index = None;

    let moved = state.move_repo_selection(NavDir::Up);

    assert!(!moved, "Up from no selection MUST return false (no-op)");
    assert!(
        state.selected_repository_index.is_none(),
        "Up from no selection MUST leave the selection as-is (None)"
    );
}
