//! Shared `#[cfg(test)]` fixtures for Pull Requests Mode reducer tests.
//!
//! Extracted so `prs_tests_composer_focus.rs` and `prs_tests_cursor_arrows.rs`
//! (and any future PR-mode test module) share ONE copy of the state fixture
//! instead of drifting copies that must be updated in lockstep when
//! `PullRequest`/`PullRequestDetail` fields change.
//!
//! @plan PLAN-20260624-PR-MODE.P14
//! @requirement REQ-PR-010

use crate::domain::{
    CommentDetailIdentity, PageToken, PaginatedList, PrCheckStatus, PrFilter, PrState, PullRequest,
    PullRequestDetail, Repository, RepositoryId,
};
use crate::state::AppState;
use crate::state::types::{InlineState, PrFocus, PrListIdentity, ScreenMode};

/// Build an empty comment list bound to the detail's repo and number (test helper).
fn empty_comments(
    scope_repo_id: RepositoryId,
    number: u64,
) -> PaginatedList<crate::domain::IssueComment, CommentDetailIdentity> {
    PaginatedList::from_loaded(
        CommentDetailIdentity {
            scope_repo_id,
            number,
        },
        Vec::new(),
        PageToken::from_cursor(None, false),
    )
}

/// PR-mode state with a single selected PR and a loaded detail (non-empty body).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 44-50
pub fn prs_state_with_detail(repo_id: &str, pr_number: u64) -> AppState {
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
    state.prs_state.pr_focus = PrFocus::PrDetail;
    state.prs_state.list.replace_items(vec![PullRequest {
        number: pr_number,
        title: format!("PR #{pr_number}"),
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
    }]);
    state.prs_state.list.set_selected_index(Some(0));
    state.prs_state.pr_detail = Some(PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: pr_number,
        title: format!("PR #{pr_number}"),
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
        external_url: format!("https://github.com/owner/repo/pull/{pr_number}"),
        review_decision: None,
        checks_status: PrCheckStatus::None,
        reviews: vec![],
        checks: vec![],
        comments: empty_comments(RepositoryId(repo_id.to_string()), pr_number),
        mergeable: None,
        merge_state_status: None,
    });
    state.prs_state.inline_state = InlineState::None;
    state
}

/// Begin a fresh visible PR-list reload for the given scope/filter and return
/// the allocated request id, so tests can correlate the result event.
///
/// Mirrors what the dispatch layer does (`next_request_id` + `begin_reload`)
/// so a subsequent `PrListLoaded`/`PrListLoadFailed` with the returned id is
/// accepted rather than treated as stale.
pub(super) fn begin_pr_list_reload(
    state: &mut AppState,
    scope_repo_id: &str,
    filter: PrFilter,
) -> u64 {
    let Ok(request_id) = state.prs_state.list.next_request_id() else {
        panic!("request id allocation must succeed in test setup");
    };
    let id = request_id.get();
    state.prs_state.list.begin_reload(
        PrListIdentity {
            scope_repo_id: RepositoryId(scope_repo_id.to_string()),
            filter,
        },
        request_id,
    );
    id
}
