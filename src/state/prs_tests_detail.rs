//! Pull Requests Mode detail-pane tests — detail loaded, staleness discard,
//! scroll-detail bounded by rendered length.
//!
//! @plan PLAN-20260624-PR-MODE.P04
//! @requirement REQ-PR-009
//! @requirement REQ-PR-NFR-002

use crate::domain::{
    PrCheckStatus, PrState, PullRequest, PullRequestDetail, Repository, RepositoryId,
};
use crate::state::AppState;
use crate::state::types::{AppEvent, PrDetailSubfocus, ScreenMode};

/// Helper: PR-mode state with one repository selected at index 0.
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

/// Helper: minimal PR detail with the given number.
fn make_test_pr_detail(number: u64) -> PullRequestDetail {
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
        comments: vec![],
        has_more_comments: false,
        comments_cursor: None,
        mergeable: None,
        merge_state_status: None,
    }
}

/// PrDetailLoaded must set detail_subfocus=Body, clear loading.detail, and
/// populate pr_detail.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 230-235
#[test]
fn test_detail_loaded_sets_subfocus_body_and_clears_loading() {
    let mut state = prs_mode_state("repo-1");
    state.prs_state.pull_requests = vec![make_test_pr(1)];
    state.prs_state.selected_pr_index = Some(0);
    state.prs_state.loading.detail = true;
    state.prs_state.detail_subfocus = PrDetailSubfocus::Review(0);

    let new_state = state.apply(AppEvent::PrDetailLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 1,
        request_id: 0,
        detail: Box::new(make_test_pr_detail(1)),
    });

    assert!(!new_state.prs_state.loading.detail);
    assert_eq!(new_state.prs_state.detail_subfocus, PrDetailSubfocus::Body);
    assert_eq!(new_state.prs_state.detail_scroll_offset, 0);
    let loaded = new_state
        .prs_state
        .pr_detail
        .clone()
        .unwrap_or_else(|| panic!("pr_detail should be Some"));
    assert_eq!(loaded.number, 1);
}

/// PrDetailLoaded with a stale pr_number (does not match the selected PR)
/// must be discarded — the existing detail is preserved. The request_id half
/// of the staleness contract is covered by the sibling
/// `test_detail_loaded_discards_mismatched_request_id`.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-NFR-002
/// @pseudocode component-001 lines 230-235
#[test]
fn test_detail_loaded_discards_stale_pr_number_or_request_id() {
    let mut state = prs_mode_state("repo-1");
    state.prs_state.pull_requests = vec![make_test_pr(1), make_test_pr(2)];
    state.prs_state.selected_pr_index = Some(1); // selected PR is #2
    state.prs_state.loading.detail = true;
    let current = make_test_pr_detail(2);
    state.prs_state.pr_detail = Some(current);

    // Stale: arrives for PR #1 while PR #2 is selected.
    let new_state = state.apply(AppEvent::PrDetailLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 1,
        request_id: 0,
        detail: Box::new(make_test_pr_detail(1)),
    });

    let loaded = new_state
        .prs_state
        .pr_detail
        .clone()
        .unwrap_or_else(|| panic!("pr_detail should remain"));
    assert_eq!(loaded.number, 2, "stale detail for PR #1 must be discarded");
    assert!(
        new_state.prs_state.loading.detail,
        "loading.detail must remain true after discarding stale"
    );
}

/// PrDetailLoaded carrying a request_id that does NOT match the pending
/// detail_pending request_id must be discarded, even when the scope and
/// pr_number match. This exercises the request_id half of the NFR-002
/// staleness contract for the detail load.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-NFR-002
/// @pseudocode component-001 lines 230-235
#[test]
fn test_detail_loaded_discards_mismatched_request_id() {
    let mut state = prs_mode_state("repo-1");
    state.prs_state.pull_requests = vec![make_test_pr(1), make_test_pr(2)];
    state.prs_state.selected_pr_index = Some(1); // selected PR is #2
    state.prs_state.loading.detail = true;
    let current = make_test_pr_detail(2);
    state.prs_state.pr_detail = Some(current);
    // Seed a detail load pending under request_id = R1 (=100).
    state.prs_state.detail_pending = Some(crate::state::types::PrDetailPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 2,
        request_id: 100,
    });

    // Dispatch PrDetailLoaded with a DIFFERENT request_id = R2 (=200),
    // matching scope and pr_number.
    let new_state = state.apply(AppEvent::PrDetailLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 2,
        request_id: 200,
        detail: Box::new(make_test_pr_detail(2)),
    });

    // The stale-request-id detail must be DISCARDED: existing detail preserved.
    let loaded = new_state
        .prs_state
        .pr_detail
        .clone()
        .unwrap_or_else(|| panic!("pr_detail should remain"));
    assert_eq!(
        loaded.number, 2,
        "existing detail must remain after mismatched request_id"
    );
    assert!(
        new_state.prs_state.loading.detail,
        "loading.detail must remain true after discarding mismatched request_id"
    );
    assert!(
        new_state.prs_state.detail_pending.is_some(),
        "detail_pending must remain after discarding mismatched request_id"
    );
}

/// PrDetailLoaded with a stale scope_repo_id must be discarded.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-NFR-002
/// @pseudocode component-001 lines 230-235
#[test]
fn test_detail_loaded_discards_stale_scope() {
    let mut state = prs_mode_state("repo-1");
    state.prs_state.pull_requests = vec![make_test_pr(1)];
    state.prs_state.selected_pr_index = Some(0);
    state.prs_state.loading.detail = true;

    let new_state = state.apply(AppEvent::PrDetailLoaded {
        scope_repo_id: RepositoryId("repo-WRONG".to_string()),
        pr_number: 1,
        request_id: 0,
        detail: Box::new(make_test_pr_detail(1)),
    });

    assert!(
        new_state.prs_state.pr_detail.is_none(),
        "stale-scope detail must be discarded"
    );
    assert!(
        new_state.prs_state.loading.detail,
        "loading.detail must remain true after discarding stale scope"
    );
}

/// ScrollDetailDown must be bounded by the rendered content length — it must
/// never exceed the maximum scroll offset derived from the real rendered length.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn test_scroll_detail_down_bounded_by_rendered_length() {
    let mut state = prs_mode_state("repo-1");
    let mut detail = make_test_pr_detail(1);
    // Short body so the rendered length is small relative to the viewport.
    detail.body = "line one\nline two".to_string();
    state.prs_state.pr_detail = Some(detail);
    state.prs_state.detail_viewport_rows = 20; // viewport larger than content
    state.prs_state.detail_scroll_offset = 0;

    // Scrolling down repeatedly must never exceed the max offset (saturating
    // at content_height - viewport_rows, which is 0 when content fits).
    let mut new_state = state.apply(AppEvent::PrScrollDetailDown);
    new_state = new_state.apply(AppEvent::PrScrollDetailDown);
    new_state = new_state.apply(AppEvent::PrScrollDetailDown);

    assert_eq!(
        new_state.prs_state.detail_scroll_offset, 0,
        "scroll must be bounded by rendered length (content fits viewport → offset stays 0)"
    );
}

/// ScrollDetailDown on content taller than the viewport advances the offset
/// but clamps at the max (rendered_length - viewport_rows).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn test_scroll_detail_down_advances_then_clamps() {
    let mut state = prs_mode_state("repo-1");
    let mut detail = make_test_pr_detail(1);
    detail.body = (0..50)
        .map(|i| format!("body line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    state.prs_state.pr_detail = Some(detail);
    state.prs_state.detail_viewport_rows = 10;
    state.prs_state.detail_scroll_offset = 0;

    // Scroll down many times — must clamp, not exceed the max.
    let mut new_state = state;
    for _ in 0..200 {
        new_state = new_state.apply(AppEvent::PrScrollDetailDown);
    }

    // The offset must be bounded (non-negative, and not absurdly large).
    // With 50 lines of body + header rows, the max offset is bounded.
    assert!(
        new_state.prs_state.detail_scroll_offset <= 100,
        "scroll offset must be bounded by rendered length, got {}",
        new_state.prs_state.detail_scroll_offset
    );
    assert!(
        new_state.prs_state.detail_scroll_offset > 0,
        "scroll offset should have advanced past 0 with long content"
    );
}
