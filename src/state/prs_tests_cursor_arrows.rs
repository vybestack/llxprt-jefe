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

// =============================================================================
// NewComment composer: document scroll stability + TextBox caret visibility.
//
// The reducer NO LONGER follows the caret per keystroke. The NewComment
// composer text/cursor is rendered by the dedicated TextBox component, which
// owns its own local viewport/caret invariant (see `text_box_view`). These
// tests assert (1) `detail_scroll_offset` stays stable while typing/arrowing,
// (2) `build_pr_detail_content` returns `cursor: None` for a NewComment
// composer (the document no longer flattens the composer), and (3) the
// `TextBox` view projection keeps the caret visible regardless of the
// (intentionally stale) document scroll offset.
//
// @plan PLAN-20260624-PR-MODE.P14
// @requirement REQ-PR-009
// @requirement REQ-PR-010
// @pseudocode component-001 lines 169-176
// =============================================================================

/// `build_pr_detail_content` must return `cursor: None` when a NewComment
/// composer is active — the composer text/cursor is rendered by the TextBox,
/// not flattened into the read-only document.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn new_comment_composer_content_cursor_is_none() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 20;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let state = type_into_composer(state, "hello\nworld");

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
    );
    assert!(
        content.cursor.is_none(),
        "NewComment composer must NOT flatten a cursor into the read-only document"
    );
}

/// Typing into the NewComment composer must NOT mutate the document
/// `detail_scroll_offset` per keystroke. The composer owns its own local
/// viewport; the reducer stays pure.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
#[test]
fn typing_in_composer_does_not_mutate_detail_scroll_offset() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 5;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let offset_after_open = state.prs_state.detail_scroll_offset;

    // Type many lines — the document scroll offset must stay exactly where
    // open left it.
    let state = type_into_composer(state, "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8");
    assert_eq!(
        state.prs_state.detail_scroll_offset, offset_after_open,
        "typing must NOT mutate detail_scroll_offset (composer owns its viewport)"
    );
}

/// Arrow keys within the composer must NOT mutate the document
/// `detail_scroll_offset`.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
#[test]
fn arrowing_in_composer_does_not_mutate_detail_scroll_offset() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 5;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let offset_after_open = state.prs_state.detail_scroll_offset;

    let mut state = type_into_composer(state, "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8");
    for _ in 0..7 {
        state = state.apply(AppEvent::PrInlineCursorUp);
    }
    assert_eq!(
        state.prs_state.detail_scroll_offset, offset_after_open,
        "arrowing must NOT mutate detail_scroll_offset"
    );
}

/// Backspacing the composer must NOT mutate the document scroll offset.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
#[test]
fn backspacing_in_composer_does_not_mutate_detail_scroll_offset() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 5;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let offset_after_open = state.prs_state.detail_scroll_offset;

    let mut state = type_into_composer(state, "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8");
    for _ in 0.."l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8".len() {
        state = state.apply(AppEvent::PrInlineBackspace);
    }
    assert_eq!(
        state.prs_state.detail_scroll_offset, offset_after_open,
        "backspacing must NOT mutate detail_scroll_offset"
    );
}

/// Even with an intentionally stale / mismatched `detail_scroll_offset`, the
/// `TextBox` view projection (built from the composer text + byte cursor)
/// must keep the caret visible. This proves the composer's caret visibility
/// is independent of the document scroll.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn text_box_view_keeps_caret_visible_with_stale_document_offset() {
    use crate::text_box_view::build_text_box_view;

    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 5;
    let mut state = state.apply(AppEvent::PrOpenNewCommentComposer);
    // Force an intentionally stale document scroll offset after open, so it
    // genuinely diverges from where the reducer placed the view.
    state.prs_state.detail_scroll_offset = 0;
    let state = type_into_composer(state, "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8");

    let (text, byte_cursor) = composer_text_cursor(&state);
    let view = build_text_box_view(&text, byte_cursor, 5, 40);
    assert!(!view.rows.is_empty(), "TextBox view must produce rows");
    let caret_row = view.rows.iter().find(|r| r.caret_col.is_some());
    assert!(
        caret_row.is_some(),
        "TextBox view must keep the caret on a visible row even with a stale document offset"
    );
}

/// The TextBox view must show the current composer text (last line) when the
/// caret is at the end of a tall multi-line draft.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn text_box_view_shows_current_text_and_caret_for_tall_draft() {
    use crate::text_box_view::build_text_box_view;

    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.detail_viewport_rows = 5;
    let state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let state = type_into_composer(state, "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8");

    let (text, byte_cursor) = composer_text_cursor(&state);
    let view = build_text_box_view(&text, byte_cursor, 5, 40);
    // The last line "l8" must be visible (caret is at end of it).
    assert!(
        view.rows.iter().any(|r| r.text == "l8"),
        "TextBox view must show the current (last) line 'l8', got rows: {:?}",
        view.rows.iter().map(|r| r.text.clone()).collect::<Vec<_>>()
    );
    // And the caret must be on that row.
    let l8_row = view.rows.iter().find(|r| r.text == "l8");
    assert!(
        l8_row.is_some_and(|r| r.caret_col.is_some()),
        "caret must be visible on the current line 'l8'"
    );
}
