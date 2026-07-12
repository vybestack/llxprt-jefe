//! PR list pagination / lazy-load / staleness-discard integration tests.
//!
//! Extracted from `prs_integration_tests.rs` (Checkpoint 19) to keep that
//! file under the 1000-line source-size limit. All helpers are borrowed from
//! the parent module via `pub(super)` re-exports.
//!
//! @plan PLAN-20260624-PR-MODE.P15
//! @requirement REQ-PR-007

use crate::domain::{PageToken, PullRequest, RepositoryId};
use crate::state::AppState;
use crate::state::events::AppEvent;

use super::prs_integration_tests::{ApplyInPlace, dashboard_state, make_test_pr};
use super::prs_test_fixtures::begin_pr_list_reload;

/// Load the first page of 30 PRs, navigate selection to the last row, and mark
/// list_page_pending. Returns the request_id used.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-007
/// @pseudocode component-001 lines 114-115,182-189,213-214
fn load_first_page_and_navigate_to_end(state: &mut AppState) -> u64 {
    let filter = state.prs_state.committed_filter.clone();
    let request_id = begin_pr_list_reload(state, "repo-1", filter);
    let scope = RepositoryId("repo-1".to_string());
    let first_page: Vec<PullRequest> = (1..=30).map(make_test_pr).collect();
    state.apply_in_place(AppEvent::PrListLoaded {
        scope_repo_id: scope.clone(),
        filter: std::boxed::Box::new(state.prs_state.committed_filter.clone()),
        request_id,
        pull_requests: first_page,
        cursor: Some("cursor-page-1".to_string()),
        has_more: true,
    });
    assert_eq!(state.prs_state.pull_requests().len(), 30);
    assert!(state.prs_state.has_more_prs());
    assert_eq!(state.prs_state.selected_pr_index(), Some(0));

    state.prs_state.list_viewport_rows = 10;
    (0..29).for_each(|_| state.apply_in_place(AppEvent::PrNavigateDown));
    assert_eq!(state.prs_state.selected_pr_index(), Some(29));

    // Allocate a fresh request id for the page load (the reload already
    // consumed id 1). Build the page begin via the public AppState method
    // that calls begin_page internally.
    let request_id = state
        .prs_state
        .list
        .next_request_id()
        .map(crate::domain::ListRequestId::get)
        .unwrap_or(0);
    let cursor = match state.prs_state.list.next_page() {
        PageToken::Cursor(c) => Some(c.clone()),
        _ => None,
    };
    state.mark_pr_list_page_loading(
        scope,
        state.prs_state.committed_filter.clone(),
        cursor,
        request_id,
    );
    request_id
}

/// Deliver PrListPageLoaded for the second page and assert APPEND + scroll-follow.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-007
/// @pseudocode component-001 lines 224-229
fn deliver_second_page_and_assert_append(state: &mut AppState, request_id: u64) {
    let scope = RepositoryId("repo-1".to_string());
    let second_page: Vec<PullRequest> = (31..=60).map(make_test_pr).collect();
    state.apply_in_place(AppEvent::PrListPageLoaded {
        scope_repo_id: scope,
        request_id,
        pull_requests: second_page,
        cursor: Some("cursor-page-2".to_string()),
        has_more: false,
    });

    assert_eq!(state.prs_state.pull_requests().len(), 60);
    assert_eq!(state.prs_state.pull_requests()[0].number, 1);
    assert_eq!(state.prs_state.pull_requests()[59].number, 60);
    assert_eq!(state.prs_state.selected_pr_index(), Some(29));
    assert!(!state.prs_state.list_pending());
    // After has_more=false, the continuation is Done (NOT the cursor string).
    assert!(matches!(state.prs_state.list.next_page(), PageToken::Done));
    assert!(!state.prs_state.has_more_prs());

    let sel = state.prs_state.selected_pr_index().unwrap_or(0);
    let len = state.prs_state.pull_requests().len();
    let vp = state.prs_state.list_viewport_rows.max(1);
    let expected_first = crate::layout::list_first_visible_index(sel, len, vp);
    assert_eq!(state.prs_state.list_scroll_offset, expected_first);
    let visible = crate::layout::list_visible_window(state.prs_state.pull_requests(), sel, vp);
    assert!(!visible.is_empty());
    assert!(
        sel >= expected_first && sel < expected_first + visible.len(),
        "selected row must stay visible"
    );
}

/// Deliver stale page responses (wrong scope + wrong request_id) and assert
/// they are DISCARDED.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-007
/// @pseudocode component-001 lines 224-225
fn assert_stale_pages_discarded(state: &mut AppState) {
    let scope = RepositoryId("repo-1".to_string());
    let stale_request_id = state
        .prs_state
        .list
        .next_request_id()
        .map(crate::domain::ListRequestId::get)
        .unwrap_or(0);
    let cursor = match state.prs_state.list.next_page() {
        PageToken::Cursor(c) => Some(c.clone()),
        _ => None,
    };
    state.mark_pr_list_page_loading(
        scope.clone(),
        state.prs_state.committed_filter.clone(),
        cursor,
        stale_request_id,
    );

    // Wrong scope_id — must be discarded.
    let wrong_scope = RepositoryId("repo-2".to_string());
    let stale_page: Vec<PullRequest> = (61..=90).map(make_test_pr).collect();
    state.apply_in_place(AppEvent::PrListPageLoaded {
        scope_repo_id: wrong_scope,
        request_id: stale_request_id,
        pull_requests: stale_page,
        cursor: None,
        has_more: false,
    });
    assert_eq!(state.prs_state.pull_requests().len(), 60);
    assert_eq!(state.prs_state.selected_pr_index(), Some(29));

    // Wrong request_id (correct scope) — must be discarded.
    let stale_page_2: Vec<PullRequest> = (91..=120).map(make_test_pr).collect();
    state.apply_in_place(AppEvent::PrListPageLoaded {
        scope_repo_id: scope,
        request_id: 999,
        pull_requests: stale_page_2,
        cursor: None,
        has_more: false,
    });
    assert_eq!(state.prs_state.pull_requests().len(), 60);
}

/// End-to-end PR list pagination / lazy-load:
///
/// 1. Load first page (30 rows, has_more=true, stored endCursor).
/// 2. Navigate selection DOWN to the last loaded row so the lazy-load trigger
///    fires (selected_pr_index == pull_requests.len()-1 AND has_more_prs).
/// 3. Simulate the page-load dispatch by marking list_page_pending.
/// 4. Deliver PrListPageLoaded and assert apply_pr_list_page_loaded APPENDS
///    the new rows (30 to 60), PRESERVES existing rows + the current selection
///    index, and recomputes list_scroll_offset via list_first_visible_index so
///    the selected row stays visible (no jump, no clipping, #54/#55).
/// 5. Deliver a STALE page response (wrong scope_id or stale request_id) and
///    assert it is DISCARDED (rows NOT duplicated/appended, selection unchanged).
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-007
/// @pseudocode component-001 lines 114-115,182-189,213-214,224-229
#[test]
fn it_pr_list_pagination_lazy_loads_appends_preserves_selection_and_discards_stale() {
    let mut state = dashboard_state();
    state = state.apply(AppEvent::EnterPrsMode);

    let request_id = load_first_page_and_navigate_to_end(&mut state);
    deliver_second_page_and_assert_append(&mut state, request_id);
    assert_stale_pages_discarded(&mut state);
}

/// A PR page-load FAILURE must clear the pending page marker so the list is
/// not stuck loading and a retry can fire (issue #202 regression guard).
///
/// @requirement REQ-PR-007
#[test]
fn it_pr_list_page_failure_clears_pending_and_allows_retry() {
    let mut state = dashboard_state();
    state = state.apply(AppEvent::EnterPrsMode);

    // load_first_page_and_navigate_to_end also begins the page load and
    // returns its request id.
    let page_request_id = load_first_page_and_navigate_to_end(&mut state);
    let scope = RepositoryId("repo-1".to_string());
    assert!(state.prs_state.list_loading(), "page load must be pending");

    // The page fetch fails. The reducer must clear the pending marker and
    // surface the error.
    state.apply_in_place(AppEvent::PrListLoadFailed {
        scope_repo_id: scope,
        request_id: page_request_id,
        error: "boom".to_string(),
    });
    assert!(
        !state.prs_state.list_loading(),
        "page failure must clear loading"
    );
    assert!(
        !state.prs_state.list_pending(),
        "page failure must clear pending"
    );
    assert_eq!(state.prs_state.error.as_deref(), Some("boom"));
    assert_eq!(
        state.prs_state.pull_requests().len(),
        30,
        "rows preserved on failure"
    );

    // A retry must be possible (should_load_more true again at the last row).
    assert!(
        state
            .prs_state
            .list
            .should_load_more(state.prs_state.selected_pr_index())
    );
}

/// A PR page-load response from the wrong repository scope must be discarded
/// even when the request id matches the pending page (issue #202 guard).
///
/// @requirement REQ-PR-007
#[test]
fn it_pr_list_page_wrong_scope_discarded() {
    let mut state = dashboard_state();
    state = state.apply(AppEvent::EnterPrsMode);
    let page_request_id = load_first_page_and_navigate_to_end(&mut state);

    // A page response for a DIFFERENT repository must not be appended.
    let wrong_scope = RepositoryId("repo-other".to_string());
    let wrong_page: Vec<PullRequest> = (61..=90).map(make_test_pr).collect();
    state.apply_in_place(AppEvent::PrListPageLoaded {
        scope_repo_id: wrong_scope,
        request_id: page_request_id,
        pull_requests: wrong_page,
        cursor: None,
        has_more: false,
    });
    assert_eq!(
        state.prs_state.pull_requests().len(),
        30,
        "wrong-scope page must not append"
    );
}
