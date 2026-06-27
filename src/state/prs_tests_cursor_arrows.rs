//! Pull Requests Mode cursor up/down + forward Delete tests (#20).
//!
//! Split out of `prs_tests_composer_focus.rs` to keep each test module within
//! the repository's per-file length budget.
//!
//! @plan PLAN-20260624-PR-MODE.P14
//! @requirement REQ-PR-010

use crate::domain::{
    PrCheckStatus, PrState, PullRequest, PullRequestDetail, Repository, RepositoryId,
};
use crate::state::AppState;
use crate::state::types::{AppEvent, InlineState, PrFocus, ScreenMode};

/// Helper: PR-mode state with a loaded detail (mirrors the one in
/// `prs_tests_composer_focus`).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 44-50
fn prs_state_with_detail(repo_id: &str, pr_number: u64) -> AppState {
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

/// Extract the active (text, cursor) pair from the inline composer/editor so
/// cursor-movement tests can assert on byte offsets without rebuilding content.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 44-50
fn composer_text_cursor(state: &AppState) -> (String, usize) {
    match &state.prs_state.inline_state {
        InlineState::Composer { text, cursor, .. } | InlineState::Editor { text, cursor, .. } => {
            (text.clone(), *cursor)
        }
        InlineState::None => panic!("expected an active composer/editor"),
    }
}

/// Type a string of characters into the current composer via the reducer,
/// returning the updated state (each char goes through AppEvent::PrInlineChar
/// so the REAL reducer path is exercised, not a direct mutation).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 44-50
fn type_into_composer(mut state: AppState, text: &str) -> AppState {
    for ch in text.chars() {
        state = state.apply(AppEvent::PrInlineChar(ch));
    }
    state
}

/// Compute the caret's absolute rendered line (same way the renderer does) for
/// viewport-follow assertions.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
fn composer_caret_line(state: &AppState) -> usize {
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
/// still within the visible viewport.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
fn walk_caret_asserting_visible(mut state: AppState, event: AppEvent, steps: usize) -> AppState {
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

/// CursorUp from a multi-line composer must move the byte cursor to the
/// PREVIOUS logical line at the preserved character column (not be a no-op).
/// Regression: "hit arrows, can't see where" (#20).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 44-50
#[test]
fn cursor_up_moves_to_previous_line_preserving_column() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 20;
    state.prs_state.detail_content_width = 80;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    // "abcd\nefgh" — cursor lands after 'h' (byte 9).
    let state = type_into_composer(state, "abcd\nefgh");
    let (text, cursor) = composer_text_cursor(&state);
    assert_eq!(text, "abcd\nefgh");
    assert_eq!(cursor, "abcd\nefgh".len());

    // CursorUp: from col 4 on line 2 -> col 4 on line 1 (after 'd').
    let state = state.apply(AppEvent::PrInlineCursorUp);
    let (_text, cursor) = composer_text_cursor(&state);
    assert_eq!(
        cursor, 4,
        "CursorUp must move to byte 4 (after 'abcd'), preserving column 4"
    );
}

/// CursorDown returns the caret to the next line at the preserved column.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 44-50
#[test]
fn cursor_down_moves_to_next_line_preserving_column() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 20;
    state.prs_state.detail_content_width = 80;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let state = type_into_composer(state, "abcd\nefgh");
    // Move up first, then down.
    let state = state.apply(AppEvent::PrInlineCursorUp);
    let state = state.apply(AppEvent::PrInlineCursorDown);
    let (_text, cursor) = composer_text_cursor(&state);
    assert_eq!(
        cursor, 9,
        "CursorDown must return to byte 9 (end of 'efgh'), preserving column 4"
    );
}

/// CursorUp on the first logical line moves to byte 0 (start of text).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 44-50
#[test]
fn cursor_up_on_first_line_goes_to_start() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 20;
    state.prs_state.detail_content_width = 80;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let state = type_into_composer(state, "abcd\nefgh");
    // Walk to the end, then Up twice: first to line 1, then to start.
    let state = state.apply(AppEvent::PrInlineCursorUp);
    let state = state.apply(AppEvent::PrInlineCursorUp);
    let (_text, cursor) = composer_text_cursor(&state);
    assert_eq!(cursor, 0, "CursorUp on the first line must move to byte 0");
}

/// CursorDown on the last logical line moves to text.len() (end of text).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 44-50
#[test]
fn cursor_down_on_last_line_goes_to_end() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 20;
    state.prs_state.detail_content_width = 80;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let state = type_into_composer(state, "abcd\nefgh");
    // Move up to line 1, then down twice: back to line 2, then to end.
    let state = state.apply(AppEvent::PrInlineCursorUp);
    let state = state.apply(AppEvent::PrInlineCursorDown);
    let state = state.apply(AppEvent::PrInlineCursorDown);
    let (text, cursor) = composer_text_cursor(&state);
    assert_eq!(
        cursor,
        text.len(),
        "CursorDown on the last line must move to text.len()"
    );
}

/// CursorUp clamps the column when the previous line is shorter (so the caret
/// does not overshoot).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 44-50
#[test]
fn cursor_up_clamps_column_on_shorter_previous_line() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 20;
    state.prs_state.detail_content_width = 80;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    // Line 1 = "ab", line 2 = "cdefgh" — caret at col 6 on line 2.
    let state = type_into_composer(state, "ab\ncdefgh");
    let state = state.apply(AppEvent::PrInlineCursorUp);
    let (_text, cursor) = composer_text_cursor(&state);
    // Previous line "ab" has length 2, so clamp col 6 -> byte 2.
    assert_eq!(
        cursor, 2,
        "CursorUp onto a shorter line must clamp to that line's length"
    );
}

/// Forward Delete (PrInlineDelete) removes the char AT the cursor, leaving the
/// cursor in place (mirrors backspace but in the forward direction).
/// Regression: forward-Delete was a no-op (#20).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 44-50
#[test]
fn forward_delete_removes_char_at_cursor() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 20;
    state.prs_state.detail_content_width = 80;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let state = type_into_composer(state, "abc");
    // Cursor is at end (byte 3). Move left once -> byte 2 (on 'c').
    let state = state.apply(AppEvent::PrInlineCursorLeft);
    let state = state.apply(AppEvent::PrInlineDelete);
    let (text, cursor) = composer_text_cursor(&state);
    assert_eq!(
        text, "ab",
        "forward Delete must remove the char at the cursor"
    );
    assert_eq!(cursor, 2, "cursor must stay in place after forward Delete");
}

/// Forward Delete at the end of text is a safe no-op (nothing to delete).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 44-50
#[test]
fn forward_delete_at_end_is_noop() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 20;
    state.prs_state.detail_content_width = 80;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let state = type_into_composer(state, "abc");
    // Cursor is at end (byte 3); Delete must not change anything.
    let state = state.apply(AppEvent::PrInlineDelete);
    let (text, cursor) = composer_text_cursor(&state);
    assert_eq!(text, "abc");
    assert_eq!(cursor, 3);
}

/// CursorUp/Down keep the caret within the visible viewport after the move
/// (drives through the real reducer so the scroll-follow runs).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
#[test]
fn cursor_up_down_keep_caret_within_viewport() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 4;
    state.prs_state.detail_content_width = 80;
    let mut state = state.apply(AppEvent::PrOpenNewCommentComposer);
    // Build a tall multi-line composer.
    state = type_into_composer(state, "aaaa\nbbbb\ncccc\ndddd\neeee\nffff");

    // Walk up: caret must stay visible at each step.
    state = walk_caret_asserting_visible(state, AppEvent::PrInlineCursorUp, 30);
    // Walk back down: caret must stay visible at each step.
    state = walk_caret_asserting_visible(state, AppEvent::PrInlineCursorDown, 30);
    let _ = state;
}
