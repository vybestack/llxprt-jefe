//! Scroll-into-view tests for issue detail subfocus navigation (#151).
//!
//! Extracted from `issues_tests_detail.rs` to keep that file under the
//! 1000-line source-file limit.

use crate::domain::{IssueComment, IssueDetail, IssueState};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::types::{DetailSubfocus, ScreenMode};

fn dashboard_issues_state() -> AppState {
    AppState {
        screen_mode: ScreenMode::DashboardIssues,
        ..AppState::default()
    }
}

fn p15_detail(number: u64) -> IssueDetail {
    IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number,
        title: format!("Issue #{number}"),
        state: IssueState::Open,
        author_login: "octocat".to_string(),
        created_at: "2024-01-01".to_string(),
        updated_at: "2024-01-02".to_string(),
        labels: Vec::new(),
        assignees: Vec::new(),
        milestone: None,
        body: String::new(),
        external_url: format!("https://example.com/{number}"),
        comments: Vec::new(),
        has_more_comments: false,
        comments_cursor: None,
    }
}

fn p15_comment(comment_id: u64, author_login: &str, created_at: &str, body: &str) -> IssueComment {
    IssueComment {
        comment_id,
        author_login: author_login.to_string(),
        created_at: created_at.to_string(),
        edited_at: None,
        body: body.to_string(),
    }
}

/// Tab forward to an offscreen issue comment must scroll the detail so the
/// comment is visible (#151).
#[test]
fn test_issue_subfocus_next_scrolls_to_offscreen_comment() {
    let mut state = dashboard_issues_state();
    let mut detail = p15_detail(1);
    detail.body = "Issue body".to_string();
    detail.comments = (0u32..10)
        .map(|i| {
            p15_comment(
                u64::from(i),
                &format!("user{i}"),
                "2024-01-01",
                &format!("comment body {i}"),
            )
        })
        .collect();
    state.issues_state.issue_detail = Some(detail);
    state.issues_state.detail_subfocus = DetailSubfocus::Body;
    state.issues_state.detail_viewport_rows = 4; // small viewport
    state.issues_state.detail_scroll_offset = 0;

    // Advance subfocus forward through comments to comment #5.
    // Body -> Comment(0)
    state = state.apply(AppEvent::IssueDetailSubfocusNext);
    // Advance through comments 0..=5
    for _ in 0..5 {
        state = state.apply(AppEvent::IssueDetailSubfocusNext);
    }
    assert_eq!(
        state.issues_state.detail_subfocus,
        DetailSubfocus::Comment(5),
        "should have advanced to Comment(5)"
    );
    let offset = state.issues_state.detail_scroll_offset;
    let viewport = state.issues_state.detail_viewport_rows;
    assert!(
        offset > 0,
        "scroll offset should have advanced to reveal comment #5, got {offset}"
    );
    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected detail"));
    let range = crate::issue_detail_content::issue_subfocus_line_range(
        detail,
        DetailSubfocus::Comment(5),
        &state.issues_state.inline_state,
        state.issues_state.loading.comments,
    )
    .unwrap_or_else(|| panic!("expected range for comment 5"));
    assert!(
        range.0 >= offset && range.0 < offset + viewport,
        "comment 5 first line {} must be within viewport [{}, {})",
        range.0,
        offset,
        offset + viewport
    );
}

/// Tab backwards to an offscreen issue comment must scroll the detail so the
/// comment is visible (#151).
#[test]
fn test_issue_subfocus_prev_scrolls_to_offscreen_comment() {
    let mut state = dashboard_issues_state();
    let mut detail = p15_detail(1);
    detail.body = "Issue body".to_string();
    detail.comments = (0u32..10)
        .map(|i| {
            p15_comment(
                u64::from(i),
                &format!("user{i}"),
                "2024-01-01",
                &format!("comment body {i}"),
            )
        })
        .collect();
    state.issues_state.issue_detail = Some(detail);
    state.issues_state.detail_subfocus = DetailSubfocus::NewComment;
    state.issues_state.detail_viewport_rows = 4; // small viewport
    // Start scrolled near the bottom (NewComment is last section).
    state.issues_state.detail_scroll_offset = 100;

    // Prev from NewComment -> Comment(9) (last comment).
    let state = state.apply(AppEvent::IssueDetailSubfocusPrev);
    assert_eq!(
        state.issues_state.detail_subfocus,
        DetailSubfocus::Comment(9),
        "should have moved to Comment(9)"
    );
    let offset = state.issues_state.detail_scroll_offset;
    let viewport = state.issues_state.detail_viewport_rows;
    assert!(
        offset < 100,
        "scroll offset should have decreased to reveal comment 9, got {offset}"
    );
    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected detail"));
    let range = crate::issue_detail_content::issue_subfocus_line_range(
        detail,
        DetailSubfocus::Comment(9),
        &state.issues_state.inline_state,
        state.issues_state.loading.comments,
    )
    .unwrap_or_else(|| panic!("expected range for comment 9"));
    assert!(
        range.0 >= offset && range.0 < offset + viewport,
        "comment 9 first line {} must be within viewport [{}, {})",
        range.0,
        offset,
        offset + viewport
    );
}
