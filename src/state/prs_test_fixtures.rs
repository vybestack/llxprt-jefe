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
    PrCheckStatus, PrState, PullRequest, PullRequestDetail, Repository, RepositoryId,
};
use crate::state::AppState;
use crate::state::types::{InlineState, PrFocus, ScreenMode};

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
    state.prs_state.pull_requests = vec![PullRequest {
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
    }];
    state.prs_state.selected_pr_index = Some(0);
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
        comments: vec![],
        has_more_comments: false,
        comments_cursor: None,
    });
    state.prs_state.inline_state = InlineState::None;
    state
}
