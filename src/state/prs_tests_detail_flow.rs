//! Pull Requests Mode list-flow tests — list nav bounds, list renders all rows
//! (#54), list staleness discard by scope AND request_id, page-append no-reorder.
//!
//! @plan PLAN-20260624-PR-MODE.P04
//! @requirement REQ-PR-006
//! @requirement REQ-PR-007
//! @requirement REQ-PR-NFR-002

use crate::domain::{
    IssueComment, PrCheckStatus, PrFilter, PrState, PullRequest, PullRequestDetail, Repository,
    RepositoryId,
};
use crate::state::AppState;
use crate::state::types::{AppEvent, PrFocus, ScreenMode};

/// Helper: PR-mode state with a selected repo.
fn prs_mode_state(repo_id: &str) -> AppState {
    let mut state = AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        ..AppState::default()
    };
    state.repositories.push(Repository::new(
        RepositoryId(repo_id.to_string()),
        "Test Repo".to_string(),
        repo_id.to_string(),
        std::path::PathBuf::from("/tmp/test"),
    ));
    state.selected_repository_index = Some(0);
    state.prs_state.active = true;
    state.prs_state.pr_focus = PrFocus::PrList;
    state
}

/// Helper: minimal PR list-row with a given number.
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

/// Helper: minimal PR detail with the given comments list.
fn make_test_pr_detail(number: u64, comments: Vec<IssueComment>) -> PullRequestDetail {
    PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number,
        title: format!("PR #{number}"),
        state: PrState::Open,
        is_draft: false,
        author_login: "octocat".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        base_ref: "main".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "PR body".to_string(),
        external_url: format!("https://github.com/owner/repo/pull/{number}"),
        review_decision: None,
        checks_status: PrCheckStatus::None,
        reviews: vec![],
        checks: vec![],
        comments,
        has_more_comments: true,
        comments_cursor: Some("cursor-1".to_string()),
    }
}

/// PrListLoaded with N rows must render all N (no dropped rows, #54).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 209-220
#[test]
fn test_list_loaded_renders_all_rows_including_first_and_last() {
    let state = prs_mode_state("repo-1");
    let prs: Vec<PullRequest> = (1u64..=10).map(make_test_pr).collect();

    let new_state = state.apply(AppEvent::PrListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(PrFilter::default()),
        request_id: 0,
        pull_requests: prs,
        cursor: None,
        has_more: false,
    });

    // All 10 rows present (no dropped rows — #54).
    assert_eq!(
        new_state.prs_state.pull_requests.len(),
        10,
        "all N loaded rows must be present (#54)"
    );
    assert_eq!(new_state.prs_state.pull_requests[0].number, 1);
    assert_eq!(new_state.prs_state.pull_requests[9].number, 10);
    // First row selected, scroll offset at 0.
    assert_eq!(new_state.prs_state.selected_pr_index, Some(0));
    assert_eq!(new_state.prs_state.list_scroll_offset, 0);
}

/// PrListLoaded with a mismatched scope_repo_id OR a mismatched request_id
/// must be discarded (no state change).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-NFR-002
/// @pseudocode component-001 lines 209-223
#[test]
fn test_list_loaded_discards_stale_scope_or_request_id() {
    let mut state = prs_mode_state("repo-1");
    state.prs_state.loading.list = true;
    state.prs_state.pull_requests = vec![make_test_pr(99)];

    // Stale scope.
    let new_state = state.apply(AppEvent::PrListLoaded {
        scope_repo_id: RepositoryId("repo-WRONG".to_string()),
        filter: Box::new(PrFilter::default()),
        request_id: 0,
        pull_requests: vec![make_test_pr(1), make_test_pr(2)],
        cursor: None,
        has_more: false,
    });
    assert_eq!(
        new_state.prs_state.pull_requests.len(),
        1,
        "stale-scope list must be discarded"
    );
    assert_eq!(new_state.prs_state.pull_requests[0].number, 99);
    assert!(
        new_state.prs_state.loading.list,
        "loading.list must remain true after discarding stale scope"
    );
}

/// PrListLoaded carrying a request_id that does NOT match the pending
/// list_reload request_id must be discarded, even when the scope matches.
/// This exercises the request_id half of the NFR-002 staleness contract.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-NFR-002
/// @pseudocode component-001 lines 209-211,221-223
#[test]
fn test_list_loaded_discards_mismatched_request_id() {
    let mut state = prs_mode_state("repo-1");
    state.prs_state.loading.list = true;
    state.prs_state.pull_requests = vec![make_test_pr(99)];
    // Seed a list reload pending under request_id = R1 (=100).
    state.prs_state.list_reload_pending = Some(crate::state::types::PrListReloadPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: PrFilter::default(),
        request_id: 100,
    });

    // Dispatch PrListLoaded with a DIFFERENT request_id = R2 (=200), same scope.
    let new_state = state.apply(AppEvent::PrListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(PrFilter::default()),
        request_id: 200,
        pull_requests: vec![make_test_pr(1), make_test_pr(2)],
        cursor: None,
        has_more: false,
    });

    // The stale-request-id payload must be DISCARDED: list NOT replaced/updated.
    assert_eq!(
        new_state.prs_state.pull_requests.len(),
        1,
        "mismatched-request_id list must be discarded (no replace)"
    );
    assert_eq!(
        new_state.prs_state.pull_requests[0].number, 99,
        "pre-existing list row must remain after mismatched request_id"
    );
    assert!(
        new_state.prs_state.loading.list,
        "loading.list must remain true after discarding mismatched request_id"
    );
    assert!(
        new_state.prs_state.list_reload_pending.is_some(),
        "list_reload_pending must remain after discarding mismatched request_id"
    );
}

/// PrListPageLoaded must APPEND rows to the existing list without reordering
/// or replacing existing rows, and must preserve the selection.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-007
/// @pseudocode component-001 lines 224-229
#[test]
fn test_list_page_loaded_appends_without_reordering() {
    let mut state = prs_mode_state("repo-1");
    state.prs_state.pull_requests = vec![make_test_pr(1), make_test_pr(2), make_test_pr(3)];
    state.prs_state.selected_pr_index = Some(1);
    state.prs_state.next_pr_list_request_id = 1;
    state.prs_state.list_page_pending = Some(crate::state::types::PrListPagePending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: PrFilter::default(),
        cursor: Some("cursor-1".to_string()),
        request_id: 0,
    });

    let new_state = state.apply(AppEvent::PrListPageLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        request_id: 0,
        pull_requests: vec![make_test_pr(4), make_test_pr(5)],
        cursor: Some("cursor-2".to_string()),
        has_more: false,
    });

    assert_eq!(
        new_state.prs_state.pull_requests.len(),
        5,
        "page must append, not replace"
    );
    // Original order preserved, new rows appended.
    assert_eq!(new_state.prs_state.pull_requests[0].number, 1);
    assert_eq!(new_state.prs_state.pull_requests[2].number, 3);
    assert_eq!(new_state.prs_state.pull_requests[3].number, 4);
    assert_eq!(new_state.prs_state.pull_requests[4].number, 5);
    // Selection preserved.
    assert_eq!(new_state.prs_state.selected_pr_index, Some(1));
}

/// PrNavigateDown must increment selected_pr_index within bounds and
/// PrNavigateUp must clamp at zero.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 99-118
#[test]
fn test_list_navigation_keeps_selection_in_bounds() {
    let mut state = prs_mode_state("repo-1");
    state.prs_state.pull_requests = vec![make_test_pr(1), make_test_pr(2), make_test_pr(3)];
    state.prs_state.selected_pr_index = Some(0);

    // Down advances.
    let new_state = state.apply(AppEvent::PrNavigateDown);
    assert_eq!(new_state.prs_state.selected_pr_index, Some(1));

    // Down to last.
    let new_state = new_state.apply(AppEvent::PrNavigateDown);
    assert_eq!(new_state.prs_state.selected_pr_index, Some(2));

    // Down at bottom clamps.
    let new_state = new_state.apply(AppEvent::PrNavigateDown);
    assert_eq!(
        new_state.prs_state.selected_pr_index,
        Some(2),
        "selection must clamp at the last index"
    );

    // Up decrements.
    let new_state = new_state.apply(AppEvent::PrNavigateUp);
    assert_eq!(new_state.prs_state.selected_pr_index, Some(1));

    // Up at top clamps.
    let new_state = new_state.apply(AppEvent::PrNavigateUp);
    let new_state = new_state.apply(AppEvent::PrNavigateUp);
    assert_eq!(
        new_state.prs_state.selected_pr_index,
        Some(0),
        "selection must clamp at index 0"
    );
}

/// PrCommentsPageLoaded must APPEND older comments in stable timeline order
/// (never reorder or replace existing comments).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 236-241
#[test]
fn test_comments_page_loaded_appends_older_stable_order() {
    let existing = IssueComment {
        comment_id: 50,
        author_login: "alice".to_string(),
        created_at: "2024-01-05T00:00:00Z".to_string(),
        edited_at: None,
        body: "existing".to_string(),
    };
    let appended = IssueComment {
        comment_id: 60,
        author_login: "bob".to_string(),
        created_at: "2024-01-06T00:00:00Z".to_string(),
        edited_at: None,
        body: "appended".to_string(),
    };

    let mut state = prs_mode_state("repo-1");
    state.prs_state.pr_detail = Some(make_test_pr_detail(1, vec![existing]));
    state.prs_state.pull_requests = vec![make_test_pr(1)];
    state.prs_state.selected_pr_index = Some(0);
    state.prs_state.comments_page_pending = Some(crate::state::types::PrCommentsPagePending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 1,
        cursor: Some("cursor-1".to_string()),
        request_id: 0,
    });

    let new_state = state.apply(AppEvent::PrCommentsPageLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 1,
        request_id: 0,
        comments: vec![appended],
        cursor: None,
        has_more: false,
    });

    let loaded = new_state
        .prs_state
        .pr_detail
        .clone()
        .unwrap_or_else(|| panic!("detail should remain"));
    assert_eq!(loaded.comments.len(), 2, "page comments must be appended");
    assert_eq!(
        loaded.comments[0].comment_id, 50,
        "existing order preserved"
    );
    assert_eq!(loaded.comments[1].comment_id, 60, "new comment appended");
}
