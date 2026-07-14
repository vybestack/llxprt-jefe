//! Silent background refresh state-transition tests (issue #128).
//!
//! These tests verify the reducer semantics for `PrListSilentRefreshed`,
//! `PrListSilentRefreshFailed`, `PrDetailSilentRefreshed`, and
//! `PrDetailSilentRefreshFailed`:
//! - Selection is preserved by PR number across list replacement/reorder.
//! - Scroll offset is preserved (clamped to new list bounds).
//! - Filter, search query, and `pr_detail` are NOT clobbered.
//! - `list_loading()` / `loading.detail` are NOT set (no spinner flash).
//! - Errors are NOT surfaced on silent failure.
//! - Stale scope/request_id results are discarded.
//! - Loud `PrListLoaded` does NOT cancel an in-flight detail load.

use crate::domain::{
    IssueComment, PrCheckStatus, PrFilter, PrState, PullRequest, PullRequestDetail, Repository,
    RepositoryId,
};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::types::{PrListIdentity, ScreenMode};

use super::prs_test_fixtures::begin_pr_list_reload;

// ── Test fixtures ──────────────────────────────────────────────────────────

/// Build a PR-mode AppState with the given repo scope.
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
    state.prs_state.pr_focus = crate::state::PrFocus::PrList;
    state
}

/// Build a minimal test `PullRequest` with the given number.
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

/// Build a minimal test `PullRequestDetail` with the given comments list.
fn make_test_pr_detail(
    scope_repo_id: &str,
    number: u64,
    comments: Vec<IssueComment>,
) -> PullRequestDetail {
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
        comments: crate::domain::PaginatedList::from_loaded(
            crate::domain::CommentDetailIdentity {
                scope_repo_id: RepositoryId(scope_repo_id.to_string()),
                number,
            },
            comments,
            crate::domain::PageToken::Cursor("cursor-1".to_string()),
        ),
        mergeable: None,
        merge_state_status: None,
    }
}

/// Seed a silent-refresh pending reload for the given scope + filter +
/// request_id so the silent-refresh staleness guard passes.
fn seed_silent_refresh_pending(state: &mut AppState, scope: &str, request_id: u64) {
    state.prs_state.committed_filter = PrFilter::default();
    state.prs_state.list.begin_silent_reload(
        PrListIdentity {
            scope_repo_id: RepositoryId(scope.to_string()),
            filter: PrFilter::default(),
        },
        crate::domain::ListRequestId::from_raw(request_id),
    );
}

// ── Loud PrListLoaded: detail_pending preservation ─────────────────────────

/// A loud (non-silent) `PrListLoaded` must NOT clear `detail_pending` (issue
/// #128). This prevents a concurrent loud list reload (e.g. the post-merge
/// refresh) from cancelling an in-flight detail load. The detail staleness
/// guard already discards stale detail results when scope/PR changes.
///
/// @requirement issue #128
#[test]
fn test_list_loaded_does_not_clear_detail_pending() {
    let mut state = prs_mode_state("repo-1");
    state
        .prs_state
        .list
        .replace_items(vec![make_test_pr(1), make_test_pr(2)]);
    state.prs_state.list.set_selected_index(Some(1));
    state.prs_state.committed_filter = PrFilter::default();
    let _request_id = begin_pr_list_reload(&mut state, "repo-1", PrFilter::default());
    // Simulate an in-flight detail load.
    state.prs_state.detail_pending = Some(crate::state::types::PrDetailPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 2,
        request_id: 42,
    });

    let new_state = state.apply(AppEvent::PrListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(PrFilter::default()),
        request_id: 1,
        pull_requests: vec![make_test_pr(1), make_test_pr(2)],
        cursor: None,
        has_more: false,
    });

    assert!(
        new_state.prs_state.detail_pending.is_some(),
        "loud PrListLoaded must NOT clear detail_pending (issue #128)"
    );
}

// ── Silent list refresh: selection preservation ────────────────────────────

/// Silent refresh preserves selection when the PR list is unchanged in
/// membership and does NOT flash the loading spinner or clear pr_detail.
///
/// @requirement issue #128
#[test]
fn test_silent_refresh_preserves_selection_and_detail() {
    let mut state = prs_mode_state("repo-1");
    state
        .prs_state
        .list
        .replace_items((1u64..=5).map(make_test_pr).collect());
    state.prs_state.list.set_selected_index(Some(2));
    state.prs_state.pr_detail = Some(make_test_pr_detail("repo-1", 3, vec![]));
    seed_silent_refresh_pending(&mut state, "repo-1", 100);

    let new_state = state.apply(AppEvent::PrListSilentRefreshed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(PrFilter::default()),
        request_id: 100,
        pull_requests: (1u64..=5).map(make_test_pr).collect(),
        cursor: None,
        has_more: false,
    });

    assert_eq!(
        new_state.prs_state.selected_pr_index(),
        Some(2),
        "selection must be preserved"
    );
    assert!(
        !new_state.prs_state.list_loading(),
        "silent refresh must NOT set loading.list"
    );
    assert!(
        !new_state.prs_state.list_pending(),
        "silent refresh must clear the pending marker"
    );
    assert!(
        new_state.prs_state.pr_detail.is_some(),
        "silent refresh must NOT clear pr_detail"
    );
}

/// Silent refresh preserves selection when PRs are reordered — the selected
/// PR is tracked by number, not by index.
///
/// @requirement issue #128
#[test]
fn test_silent_refresh_preserves_selection_when_pr_reordered() {
    let mut state = prs_mode_state("repo-1");
    state
        .prs_state
        .list
        .replace_items(vec![make_test_pr(1), make_test_pr(2), make_test_pr(3)]);
    state.prs_state.list.set_selected_index(Some(1)); // PR #2
    seed_silent_refresh_pending(&mut state, "repo-1", 100);

    let new_state = state.apply(AppEvent::PrListSilentRefreshed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(PrFilter::default()),
        request_id: 100,
        pull_requests: vec![make_test_pr(3), make_test_pr(2), make_test_pr(1)],
        cursor: None,
        has_more: false,
    });

    assert_eq!(
        new_state.prs_state.selected_pr_index(),
        Some(1),
        "selection must track PR #2 by number (now at index 1)"
    );
    assert_eq!(
        new_state
            .prs_state
            .pull_requests()
            .get(1)
            .map(|pr| pr.number),
        Some(2),
        "index 1 must be PR #2"
    );
}

/// Silent refresh falls back to first PR when the selected PR is no longer in
/// the list (merged/closed elsewhere).
///
/// @requirement issue #128
#[test]
fn test_silent_refresh_handles_selected_pr_removed() {
    let mut state = prs_mode_state("repo-1");
    state
        .prs_state
        .list
        .replace_items(vec![make_test_pr(1), make_test_pr(2), make_test_pr(3)]);
    state.prs_state.list.set_selected_index(Some(1)); // PR #2
    seed_silent_refresh_pending(&mut state, "repo-1", 100);

    let new_state = state.apply(AppEvent::PrListSilentRefreshed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(PrFilter::default()),
        request_id: 100,
        pull_requests: vec![make_test_pr(1), make_test_pr(3)],
        cursor: None,
        has_more: false,
    });

    assert_eq!(
        new_state.prs_state.selected_pr_index(),
        Some(0),
        "removed PR: selection falls back to first"
    );
}

// ── Silent list refresh: staleness guards ──────────────────────────────────

/// Silent refresh discards results with a mismatched scope_repo_id.
///
/// @requirement issue #128
#[test]
fn test_silent_refresh_discards_stale_scope() {
    let mut state = prs_mode_state("repo-1");
    state
        .prs_state
        .list
        .replace_items(vec![make_test_pr(1), make_test_pr(2)]);
    state.prs_state.list.set_selected_index(Some(0));
    seed_silent_refresh_pending(&mut state, "repo-1", 100);

    let new_state = state.apply(AppEvent::PrListSilentRefreshed {
        scope_repo_id: RepositoryId("repo-WRONG".to_string()),
        filter: Box::new(PrFilter::default()),
        request_id: 100,
        pull_requests: vec![make_test_pr(99)],
        cursor: None,
        has_more: false,
    });

    assert_eq!(
        new_state.prs_state.pull_requests().len(),
        2,
        "stale scope result must be discarded"
    );
    assert!(
        new_state.prs_state.list_pending(),
        "stale scope: pending must remain"
    );
}

// ── Silent list refresh: no loading/error flags ────────────────────────────

/// Silent refresh does NOT set loading.list or error.
///
/// @requirement issue #128
#[test]
fn test_silent_refresh_does_not_set_loading_or_error() {
    let mut state = prs_mode_state("repo-1");
    state
        .prs_state
        .list
        .replace_items(vec![make_test_pr(1), make_test_pr(2)]);
    state.prs_state.list.set_selected_index(Some(0));
    state.prs_state.error = None;
    seed_silent_refresh_pending(&mut state, "repo-1", 100);

    let new_state = state.apply(AppEvent::PrListSilentRefreshed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(PrFilter::default()),
        request_id: 100,
        pull_requests: vec![make_test_pr(1), make_test_pr(2)],
        cursor: None,
        has_more: false,
    });

    assert!(
        !new_state.prs_state.list_loading(),
        "silent refresh must NOT set loading.list"
    );
    assert!(
        new_state.prs_state.error.is_none(),
        "silent refresh must NOT set error"
    );
}

// ── Silent list refresh failure ────────────────────────────────────────────

/// Silent refresh failure clears pending WITHOUT setting an error.
///
/// @requirement issue #128
#[test]
fn test_silent_refresh_failed_clears_pending_without_error() {
    let mut state = prs_mode_state("repo-1");
    let identity = PrListIdentity {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: PrFilter::default(),
    };
    state
        .prs_state
        .list
        .begin_reload(identity.clone(), crate::domain::ListRequestId::from_raw(99));
    state
        .prs_state
        .list
        .accept_loaded(crate::state::pagination::ReloadResult {
            identity,
            request_id: crate::domain::ListRequestId::from_raw(99),
            items: vec![make_test_pr(1)],
            next_page: crate::domain::PageToken::Cursor("next".to_string()),
        });
    state.prs_state.error = None;
    seed_silent_refresh_pending(&mut state, "repo-1", 100);

    let new_state = state.apply(AppEvent::PrListSilentRefreshFailed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        request_id: 100,
    });

    assert!(
        !new_state.prs_state.list_pending(),
        "silent refresh failure must clear the pending marker"
    );
    assert!(
        new_state.prs_state.error.is_none(),
        "silent refresh failure must NOT set error"
    );
    assert_eq!(
        new_state.prs_state.list.next_page(),
        &crate::domain::PageToken::Cursor("next".to_string()),
        "silent refresh failure must preserve the original cursor"
    );
    assert!(
        new_state.prs_state.list.has_more(),
        "silent refresh failure must preserve load-more continuation"
    );
}

/// Silent list refresh failure with a STALE request_id must be discarded
/// (the pending marker for the CURRENT request_id remains).
///
/// @requirement issue #128
#[test]
fn test_silent_refresh_failed_discards_stale_request_id() {
    let mut state = prs_mode_state("repo-1");
    state.prs_state.list.replace_items(vec![make_test_pr(1)]);
    state.prs_state.committed_filter = PrFilter::default();
    seed_silent_refresh_pending(&mut state, "repo-1", 100);

    let new_state = state.apply(AppEvent::PrListSilentRefreshFailed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        request_id: 999,
    });

    assert!(
        new_state.prs_state.list_pending(),
        "stale silent-refresh failure must be discarded (pending remains)"
    );
}

// ── Silent list refresh: pr_detail preservation ───────────────────────────

/// Silent refresh must NOT clear `pr_detail` (unlike loud `PrListLoaded`).
///
/// @requirement issue #128
#[test]
fn test_silent_refresh_does_not_clear_pr_detail() {
    let mut state = prs_mode_state("repo-1");
    let detail = make_test_pr_detail("repo-1", 2, vec![]);
    state.prs_state.pr_detail = Some(detail);
    state
        .prs_state
        .list
        .replace_items(vec![make_test_pr(1), make_test_pr(2)]);
    state.prs_state.list.set_selected_index(Some(1));
    state.prs_state.detail_scroll_offset = 3;
    seed_silent_refresh_pending(&mut state, "repo-1", 100);

    let new_state = state.apply(AppEvent::PrListSilentRefreshed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(PrFilter::default()),
        request_id: 100,
        pull_requests: vec![make_test_pr(1), make_test_pr(2)],
        cursor: None,
        has_more: false,
    });

    assert!(
        new_state.prs_state.pr_detail.is_some(),
        "silent refresh must NOT clear pr_detail"
    );
    assert_eq!(
        new_state.prs_state.detail_scroll_offset, 3,
        "silent refresh must NOT reset detail_scroll_offset"
    );
}

/// Silent list refresh with an empty list must NOT clear `pr_detail` (issue
/// #128 remediation). The detail pane keeps showing the last-loaded detail
/// until the next manual reload, avoiding an empty flash.
///
/// @requirement issue #128
#[test]
fn test_silent_refresh_empty_list_preserves_pr_detail() {
    let mut state = prs_mode_state("repo-1");
    state.prs_state.pr_detail = Some(make_test_pr_detail("repo-1", 2, vec![]));
    state
        .prs_state
        .list
        .replace_items(vec![make_test_pr(1), make_test_pr(2)]);
    state.prs_state.list.set_selected_index(Some(1));
    seed_silent_refresh_pending(&mut state, "repo-1", 100);

    let new_state = state.apply(AppEvent::PrListSilentRefreshed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(PrFilter::default()),
        request_id: 100,
        pull_requests: vec![],
        cursor: None,
        has_more: false,
    });

    assert!(
        new_state.prs_state.pr_detail.is_some(),
        "silent refresh with empty list must NOT clear pr_detail"
    );
    assert!(
        new_state.prs_state.selected_pr_index().is_none(),
        "empty list must clear selected_pr_index"
    );
}

// ── Silent list refresh: search query + filter preservation ───────────────

/// Silent list refresh must preserve the search query and committed filter.
///
/// @requirement issue #128
#[test]
fn test_silent_refresh_preserves_search_query_and_filter() {
    let mut state = prs_mode_state("repo-1");
    state
        .prs_state
        .list
        .replace_items(vec![make_test_pr(1), make_test_pr(2)]);
    state.prs_state.list.set_selected_index(Some(0));
    state.prs_state.search_query = "foo".to_string();
    let filter = PrFilter {
        state: Some(crate::domain::PrFilterState::Open),
        ..PrFilter::default()
    };
    state.prs_state.committed_filter = filter.clone();
    // Seed pending with the SAME filter so the staleness guard passes.
    state.prs_state.list.begin_silent_reload(
        PrListIdentity {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            filter: filter.clone(),
        },
        crate::domain::ListRequestId::from_raw(100),
    );

    let new_state = state.apply(AppEvent::PrListSilentRefreshed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(filter),
        request_id: 100,
        pull_requests: vec![make_test_pr(1), make_test_pr(2)],
        cursor: None,
        has_more: false,
    });

    assert_eq!(
        new_state.prs_state.search_query, "foo",
        "silent refresh must preserve search_query"
    );
    assert_eq!(
        new_state.prs_state.committed_filter.state,
        Some(crate::domain::PrFilterState::Open),
        "silent refresh must preserve committed_filter"
    );
}

// ── Silent detail refresh ──────────────────────────────────────────────────

/// Silent detail refresh must preserve `detail_subfocus` and
/// `detail_scroll_offset` (NOT reset them to Body/0 like the loud load).
///
/// @requirement issue #128
#[test]
fn test_silent_detail_refresh_preserves_subfocus_and_scroll() {
    let mut state = prs_mode_state("repo-1");
    state.prs_state.list.replace_items(vec![make_test_pr(5)]);
    state.prs_state.list.set_selected_index(Some(0));
    state.prs_state.pr_detail = Some(make_test_pr_detail("repo-1", 5, vec![]));
    state.prs_state.detail_subfocus = crate::state::types::PrDetailSubfocus::Comment(1);
    state.prs_state.detail_scroll_offset = 5;
    state.prs_state.detail_pending = Some(crate::state::types::PrDetailPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 5,
        request_id: 42,
    });

    let new_state = state.apply(AppEvent::PrDetailSilentRefreshed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 5,
        request_id: 42,
        detail: std::boxed::Box::new(make_test_pr_detail("repo-1", 5, vec![])),
    });

    assert_eq!(
        new_state.prs_state.detail_subfocus,
        crate::state::types::PrDetailSubfocus::Comment(1),
        "silent detail refresh must preserve detail_subfocus"
    );
    assert_eq!(
        new_state.prs_state.detail_scroll_offset, 5,
        "silent detail refresh must preserve detail_scroll_offset"
    );
    assert!(
        new_state.prs_state.detail_pending.is_none(),
        "silent detail refresh must clear detail_pending"
    );
    assert!(
        !new_state.prs_state.loading.detail,
        "silent detail refresh must NOT set loading.detail"
    );
    let Some(detail) = new_state.prs_state.pr_detail.as_ref() else {
        panic!("silent detail refresh should preserve a loaded detail");
    };
    assert_eq!(
        detail.comments.identity(),
        Some(&crate::domain::CommentDetailIdentity {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            number: 5,
        })
    );
}

/// Silent detail refresh failure must clear `detail_pending` WITHOUT setting an
/// error or `loading.detail`.
///
/// @requirement issue #128
#[test]
fn test_silent_detail_refresh_failed_does_not_set_error() {
    let mut state = prs_mode_state("repo-1");
    state.prs_state.list.replace_items(vec![make_test_pr(5)]);
    state.prs_state.list.set_selected_index(Some(0));
    state.prs_state.error = None;
    state.prs_state.loading.detail = false;
    state.prs_state.detail_pending = Some(crate::state::types::PrDetailPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 5,
        request_id: 42,
    });

    let new_state = state.apply(AppEvent::PrDetailSilentRefreshFailed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 5,
        request_id: 42,
    });

    assert!(
        new_state.prs_state.error.is_none(),
        "silent detail refresh failure must NOT set an error"
    );
    assert!(
        new_state.prs_state.detail_pending.is_none(),
        "silent detail refresh failure must clear detail_pending"
    );
    assert!(
        !new_state.prs_state.loading.detail,
        "silent detail refresh failure must NOT set loading.detail"
    );
}
