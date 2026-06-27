//! Pull Requests Mode cursor up/down + forward Delete tests (#20).
//!
//! Split out of `prs_tests_composer_focus.rs` to keep each test module within
//! the repository's per-file length budget.
//!
//! These tests assert the cursor-movement LOGIC (byte offsets, column
//! preservation, clamping) AND the width-free cursor-follow that keeps the
//! caret inside the visible scroll window while typing/arrowing. The follow
//! reads the caret line from the SAME `build_pr_detail_content` the renderer
//! uses, so it cannot desync the way the old wrap-width follow did.
//!
//! @plan PLAN-20260624-PR-MODE.P14
//! @requirement REQ-PR-010

use super::prs_test_fixtures::prs_state_with_detail;
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

/// CursorUp on the first logical line is a no-op (the caret stays at its
/// current column), matching the shared `inline_cursor_vertical` helper used by
/// Issues mode. (The old PR-specific helper moved to byte 0, but Issues mode —
/// which PR mode now mirrors — does not.)
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 44-50
#[test]
fn cursor_up_on_first_line_is_noop() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 20;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let state = type_into_composer(state, "abcd\nefgh");
    // Walk to the end, then Up twice: first to line 1 (byte 4), then Up again
    // on the first line — a no-op (caret stays at byte 4).
    let state = state.apply(AppEvent::PrInlineCursorUp);
    let state = state.apply(AppEvent::PrInlineCursorUp);
    let (_text, cursor) = composer_text_cursor(&state);
    assert_eq!(
        cursor, 4,
        "CursorUp on the first line must be a no-op (caret stays at byte 4)"
    );
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
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let state = type_into_composer(state, "abc");
    // Cursor is at end (byte 3); Delete must not change anything.
    let state = state.apply(AppEvent::PrInlineDelete);
    let (text, cursor) = composer_text_cursor(&state);
    assert_eq!(text, "abc");
    assert_eq!(cursor, 3);
}

/// CursorUp/Down movement logic (column preservation across lines) still works
/// on a tall multi-line composer now that up/down route through the shared
/// `inline_cursor_vertical` helper (the same one Issues mode uses). This does
/// NOT assert per-keystroke scroll-follow (removed: PR mode scrolls ONLY on
/// open and on CommentCreated, mirroring Issues).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 44-50
#[test]
fn cursor_up_down_preserve_column_on_tall_composer() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 20;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    // Build a tall multi-line composer.
    let state = type_into_composer(state, "aaaa\nbbbb\ncccc\ndddd\neeee\nffff");
    // Caret sits at the end of the last line ("ffff", byte 24).
    let (_text, cursor) = composer_text_cursor(&state);
    assert_eq!(cursor, "aaaa\nbbbb\ncccc\ndddd\neeee\nffff".len());

    // CursorUp from col 4 on the last line -> col 4 on the previous line.
    let state = state.apply(AppEvent::PrInlineCursorUp);
    let (_text, cursor) = composer_text_cursor(&state);
    assert_eq!(
        cursor,
        "aaaa\nbbbb\ncccc\ndddd\neeee".len(),
        "CursorUp must move up one line preserving column 4"
    );

    // CursorDown returns to the last line at col 4 (end of "ffff").
    let state = state.apply(AppEvent::PrInlineCursorDown);
    let (_text, cursor) = composer_text_cursor(&state);
    assert_eq!(
        cursor,
        "aaaa\nbbbb\ncccc\ndddd\neeee\nffff".len(),
        "CursorDown must return to the last line preserving column 4"
    );
}

/// Compute `(cursor_line, scroll_offset, viewport_rows)` for the active PR
/// composer using the SAME `build_pr_detail_content` the renderer consumes, so
/// a test can assert the caret falls inside the rendered scroll window
/// `[offset, offset + viewport)`. This is exactly the predicate
/// `ScrollableText` uses to decide whether to DRAW the caret, so a passing
/// assertion proves the caret is actually visible to the user.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
fn caret_window(state: &AppState) -> (usize, usize, usize) {
    let detail = state
        .prs_state
        .pr_detail
        .as_ref()
        .unwrap_or_else(|| panic!("a loaded PR detail should exist"));
    let content = crate::pr_detail_content::build_pr_detail_content(
        detail,
        state.prs_state.detail_subfocus,
        &state.prs_state.inline_state,
        state.prs_state.loading.detail,
        state.prs_state.loading.comments,
    );
    let (cursor_line, _col) = content
        .cursor
        .unwrap_or_else(|| panic!("an active composer cursor should exist"));
    (
        cursor_line,
        state.prs_state.detail_scroll_offset,
        state.prs_state.detail_viewport_rows.max(1),
    )
}

/// Typing past the bottom of the viewport must scroll the detail pane so the
/// caret stays inside the visible window. Regression (#20): after the wrapping
/// rewrite the composer no longer followed the caret, so "line 3 goes off the
/// screen and I can't see what I'm typing".
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn typing_below_viewport_keeps_caret_visible() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 5;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let state = type_into_composer(state, "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8");
    let (cursor_line, offset, viewport) = caret_window(&state);
    assert!(
        cursor_line >= offset && cursor_line < offset + viewport,
        "caret line {cursor_line} must stay within the visible window [{offset}, {})",
        offset + viewport
    );
}

/// Arrowing the caret back UP above the current scroll window must scroll the
/// pane up so the caret remains visible. Regression (#20): "no more caret so I
/// can't see what I'm doing if I go backwards".
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn arrowing_up_above_window_keeps_caret_visible() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 5;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let mut state = type_into_composer(state, "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8");
    for _ in 0..7 {
        state = state.apply(AppEvent::PrInlineCursorUp);
    }
    let (cursor_line, offset, viewport) = caret_window(&state);
    assert!(
        cursor_line >= offset && cursor_line < offset + viewport,
        "caret line {cursor_line} must stay within the visible window [{offset}, {}) after arrowing up",
        offset + viewport
    );
}

/// The cursor-follow must NEVER jump the view to the very top while the caret
/// is on a lower line. This is the precise failure the old wrap-width follow
/// produced ("I start typing it jumps up to the top of the pr"). With the
/// caret on a lower input line, the window must have scrolled DOWN (non-zero
/// offset) and the caret must remain visible.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn typing_does_not_yank_view_to_top_when_caret_is_lower() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 4;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let state = type_into_composer(state, "l1\nl2\nl3\nl4\nl5\nl6");
    let (cursor_line, offset, viewport) = caret_window(&state);
    assert!(
        offset > 0,
        "view must scroll down to follow the caret, not sit at the top (offset={offset}, cursor_line={cursor_line})"
    );
    assert!(
        cursor_line >= offset && cursor_line < offset + viewport,
        "caret line {cursor_line} must be within [{offset}, {})",
        offset + viewport
    );
}

/// Backspacing a multi-line composer back down to a single line must pull the
/// caret (and viewport) back up so the caret stays visible. Complements the
/// arrow-up case: here the content itself shrinks while the caret rises.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn backspacing_multiline_keeps_caret_visible() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 5;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let mut state = type_into_composer(state, "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8");
    // Backspace away every char (incl. newlines) back to an empty composer.
    for _ in 0.."l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8".len() {
        state = state.apply(AppEvent::PrInlineBackspace);
    }
    let (cursor_line, offset, viewport) = caret_window(&state);
    assert!(
        cursor_line >= offset && cursor_line < offset + viewport,
        "caret line {cursor_line} must stay within the visible window [{offset}, {}) after backspacing",
        offset + viewport
    );
}

/// A zero-height viewport must be a no-op for the cursor-follow (no panic, no
/// underflow in the `cursor_line + 1 - viewport` computation). Guards the early
/// return in `pr_follow_caret`.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn cursor_follow_with_zero_viewport_is_noop() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 0;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let before = state.prs_state.detail_scroll_offset;
    let state = type_into_composer(state, "a\nb\nc\nd");
    assert_eq!(
        state.prs_state.detail_scroll_offset, before,
        "zero viewport must leave the scroll offset untouched"
    );
}
