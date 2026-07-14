//! Pull Requests Mode filter/search tests — filter navigate, draft update,
//! apply filter, cycle filter state, apply search, clear search.
//!
//! @plan PLAN-20260624-PR-MODE.P04
//! @requirement REQ-PR-008

use crate::domain::{PrFilter, PrFilterState, Repository, RepositoryId};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::types::ScreenMode;

/// Helper: PR-mode state with filter controls open and a selected repo.
fn prs_filter_open_state() -> AppState {
    let mut state = AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        ..AppState::default()
    };
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        "Test Repo".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/repo1"),
    ));
    state.selected_repository_index = Some(0);
    state.prs_state.active = true;
    state.prs_state.filter_ui.controls_open = true;
    state.prs_state.filter_ui.field_index = 0;
    state
}

/// PrFilterNavigateNext must advance field_index and PrFilterNavigatePrev must
/// reverse it (the EIGHT PR filter fields cycle modulo 8).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 254-264
#[test]
fn test_filter_navigate_and_update_draft_changes_draft_only() {
    let state = prs_filter_open_state();
    assert_eq!(state.prs_state.filter_ui.field_index, 0);

    // Next advances the field index.
    let state = state.apply(AppEvent::PrFilterNavigateNext);
    assert_eq!(state.prs_state.filter_ui.field_index, 1);

    // Prev reverses it.
    let state = state.apply(AppEvent::PrFilterNavigatePrev);
    assert_eq!(state.prs_state.filter_ui.field_index, 0);

    // UpdateDraftFilter must change the DRAFT filter only, NOT committed.
    let state = state.apply(AppEvent::PrUpdateDraftFilter {
        field: "author".to_string(),
        value: "octocat".to_string(),
    });
    assert_eq!(state.prs_state.draft_filter.author, "octocat");
    assert!(
        state.prs_state.committed_filter.author.is_empty(),
        "committed_filter must NOT change from a draft update"
    );
}

/// PrApplyFilter must copy draft→committed, close controls, and trigger a
/// reload (request_id bump / loading.list true).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 265-269
#[test]
fn test_apply_filter_commits_and_resets_for_reload() {
    let mut state = prs_filter_open_state();
    state.prs_state.draft_filter.author = "octocat".to_string();
    state.prs_state.draft_filter.state = Some(PrFilterState::Closed);

    let new_state = state.apply(AppEvent::PrApplyFilter);

    // Draft was committed.
    assert_eq!(new_state.prs_state.committed_filter.author, "octocat");
    assert_eq!(
        new_state.prs_state.committed_filter.state,
        Some(PrFilterState::Closed)
    );
    // Controls closed.
    assert!(!new_state.prs_state.filter_ui.controls_open);
    // Reload requested: the list is cleared (the dispatch layer begins the
    // actual fetch). The reducer drops stale rows/continuation so the next
    // PrListLoaded starts fresh.
    assert!(
        new_state.prs_state.pull_requests().is_empty(),
        "apply filter must clear the list for reload"
    );
}

/// PrCycleFilterState must cycle Open→Closed→Merged→All→Open (wrap).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 259-261
#[test]
fn test_cycle_filter_state_open_closed_merged_all_open() {
    let mut state = prs_filter_open_state();
    state.prs_state.draft_filter.state = Some(PrFilterState::Open);

    let state = state.apply(AppEvent::PrCycleFilterState);
    assert_eq!(
        state.prs_state.draft_filter.state,
        Some(PrFilterState::Closed)
    );

    let state = state.apply(AppEvent::PrCycleFilterState);
    assert_eq!(
        state.prs_state.draft_filter.state,
        Some(PrFilterState::Merged)
    );

    let state = state.apply(AppEvent::PrCycleFilterState);
    assert_eq!(state.prs_state.draft_filter.state, Some(PrFilterState::All));

    // Wrap back to Open.
    let state = state.apply(AppEvent::PrCycleFilterState);
    assert_eq!(
        state.prs_state.draft_filter.state,
        Some(PrFilterState::Open)
    );
}

/// PrApplySearch must commit the TRIMMED search query, blur the input, and
/// trigger a reload.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 282-286
#[test]
fn test_apply_search_commits_trimmed_query_and_resets() {
    let mut state = prs_filter_open_state();
    state.prs_state.search_query = "  bug fix  ".to_string();
    state.prs_state.search_input_focused = true;

    let new_state = state.apply(AppEvent::PrApplySearch);

    assert_eq!(
        new_state.prs_state.committed_filter.query_text, "bug fix",
        "search query must be trimmed when committed"
    );
    assert!(
        !new_state.prs_state.search_input_focused,
        "search input must be blurred after apply"
    );
    // The list is cleared for reload (the dispatch layer begins the fetch).
    assert!(
        new_state.prs_state.pull_requests().is_empty(),
        "apply search must clear the list for reload"
    );
}

/// PrClearSearch must clear the query, blur, and reload.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 287-291
#[test]
fn test_clear_search_blurs_and_reloads() {
    let mut state = prs_filter_open_state();
    state.prs_state.search_query = "bug".to_string();
    state.prs_state.committed_filter.query_text = "bug".to_string();
    state.prs_state.search_input_focused = true;

    let new_state = state.apply(AppEvent::PrClearSearch);

    assert!(
        new_state.prs_state.search_query.is_empty(),
        "search_query must be cleared"
    );
    assert!(
        new_state.prs_state.committed_filter.query_text.is_empty(),
        "committed query_text must be cleared"
    );
    assert!(
        !new_state.prs_state.search_input_focused,
        "search input must be blurred"
    );
    // The list is cleared for reload (the dispatch layer begins the fetch).
    assert!(
        new_state.prs_state.pull_requests().is_empty(),
        "clear search must clear the list for reload"
    );
}

/// PrClearFilter must also reset the filter (re-uses the default PrFilter).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 270-274
#[test]
fn test_clear_filter_resets_committed_and_draft() {
    let mut state = prs_filter_open_state();
    state.prs_state.committed_filter = PrFilter {
        state: Some(PrFilterState::Merged),
        author: "someone".to_string(),
        ..PrFilter::default()
    };
    state.prs_state.draft_filter = state.prs_state.committed_filter.clone();

    let new_state = state.apply(AppEvent::PrClearFilter);

    assert_eq!(
        new_state.prs_state.committed_filter.state,
        Some(PrFilterState::Open)
    );
    assert!(new_state.prs_state.committed_filter.author.is_empty());
    assert_eq!(
        new_state.prs_state.draft_filter.state,
        Some(PrFilterState::Open)
    );
}
