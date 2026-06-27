//! Pull Requests Mode cursor up/down + forward Delete tests (#20).
//!
//! Split out of `prs_tests_composer_focus.rs` to keep each test module within
//! the repository's per-file length budget.
//!
//! @plan PLAN-20260624-PR-MODE.P14
//! @requirement REQ-PR-010

use super::prs_test_fixtures::{prs_state_with_detail, walk_caret_asserting_visible};
use crate::state::AppState;
use crate::state::types::{AppEvent, InlineState};

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

    // Opening + typing a tall composer must scroll the viewport down to keep
    // the caret (now on the last line) visible.
    let bottom_offset = state.prs_state.detail_scroll_offset;
    assert!(
        bottom_offset > 0,
        "precondition: typing past the viewport must scroll to the bottom"
    );

    // Walk up: caret must stay visible at each step, and the viewport must
    // follow the caret upward (offset strictly decreases).
    state = walk_caret_asserting_visible(state, AppEvent::PrInlineCursorUp, 30);
    let top_offset = state.prs_state.detail_scroll_offset;
    assert!(
        top_offset < bottom_offset,
        "walking up must pull the viewport up: top {top_offset} < bottom {bottom_offset}"
    );

    // Walk back down: caret must stay visible at each step, and the viewport
    // must follow the caret back down (offset strictly increases).
    state = walk_caret_asserting_visible(state, AppEvent::PrInlineCursorDown, 30);
    assert!(
        state.prs_state.detail_scroll_offset > top_offset,
        "walking down must scroll the viewport back down past top {top_offset}"
    );
}
