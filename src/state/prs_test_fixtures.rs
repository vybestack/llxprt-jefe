//! Shared `#[cfg(test)]` fixtures for Pull Requests Mode reducer tests.
//!
//! Extracted so `prs_tests_composer_focus.rs` and `prs_tests_cursor_arrows.rs`
//! (and any future PR-mode test module) share ONE copy of the state fixture and
//! caret/viewport helpers instead of drifting copies that must be updated in
//! lockstep when `PullRequest`/`PullRequestDetail` fields change.
//!
//! @plan PLAN-20260624-PR-MODE.P14
//! @requirement REQ-PR-010

use crate::domain::{
    PrCheckStatus, PrState, PullRequest, PullRequestDetail, Repository, RepositoryId,
};
use crate::state::AppState;
use crate::state::types::{AppEvent, InlineState, PrFocus, ScreenMode};

/// PR-mode state with a single selected PR and a loaded (empty-body) detail.
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

/// Compute the caret's absolute rendered line the SAME way the renderer and the
/// scroll-follow logic do (same `build_pr_detail_content` + `wrap_width` source),
/// so tests can assert the caret stays inside the visible window.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
pub fn composer_caret_line(state: &AppState) -> usize {
    let detail = state
        .prs_state
        .pr_detail
        .as_ref()
        .unwrap_or_else(|| panic!("detail should exist"));
    let content = crate::pr_detail_content::build_pr_detail_content(
        detail,
        state.prs_state.detail_subfocus,
        &state.prs_state.inline_state,
        state.prs_state.loading.detail,
        state.prs_state.loading.comments,
        crate::state::prs_inline_ops::wrap_width_from_state(state.prs_state.detail_content_width),
    );
    content
        .cursor
        .unwrap_or_else(|| panic!("composer must expose a caret while moving"))
        .0
}

/// Apply `event` `steps` times, asserting after each step that the caret is
/// still within the visible viewport `[offset, offset + viewport_rows)`.
/// Returns the final state so callers can assert directional scroll movement.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
pub fn walk_caret_asserting_visible(
    mut state: AppState,
    event: AppEvent,
    steps: usize,
) -> AppState {
    for _ in 0..steps {
        state = state.apply(event.clone());
        let cursor_line = composer_caret_line(&state);
        let offset = state.prs_state.detail_scroll_offset;
        let viewport = state.prs_state.detail_viewport_rows;
        assert!(
            cursor_line >= offset && cursor_line < offset + viewport,
            "caret line {cursor_line} must stay within viewport [{offset}, {})",
            offset + viewport
        );
    }
    state
}
