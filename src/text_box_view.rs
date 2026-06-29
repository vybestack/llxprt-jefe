//! Pure multiline text-box viewport projection.
//!
//! This module is iocraft-free and side-effect-free: it turns the raw
//! composer/editor `(text, byte_cursor)` plus a viewport size into a fixed
//! window of display rows with an optional caret cell. The UI component
//! (`ui::components::text_box`) consumes the projection and renders exactly
//! `viewport_rows` rows — the reducer never needs to follow the caret per
//! keystroke because the editable text owns its own local viewport
//! invariant.
//!
//! Line-splitting semantics mirror the existing PR/Issues composer:
//! - empty text → one blank row,
//! - trailing `'\n'` → an extra blank row.
//!
//! @plan PLAN-20260624-PR-MODE.P14
//! @requirement REQ-PR-009
//! @requirement REQ-PR-010
//! @pseudocode component-001 lines 169-176

/// A caret cell expressed in logical `(line, col)` coordinates within the
/// source text (char column, not byte offset).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextCaret {
    /// Zero-based logical line index within the source text.
    pub line: usize,
    /// Zero-based character column within that line.
    pub col: usize,
}

/// One rendered row: the (already truncated/windowed) text plus an optional
/// caret column relative to the start of `text`.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBoxRow {
    /// The windowed/truncated text for this row (no trailing newline).
    pub text: String,
    /// Character column (0-based, relative to `text`) of the caret, or `None`
    /// when this row does not carry the caret.
    pub caret_col: Option<usize>,
}

/// A fixed-size projection of a multiline text editor over a local viewport.
///
/// `rows.len()` is always `viewport_rows` for `viewport_rows > 0` (padded
/// with blank rows), and `0` when `viewport_rows == 0`. The caret, when
/// present, is always on a visible row.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBoxView {
    /// Exactly `viewport_rows` rows (or empty when `viewport_rows == 0`).
    pub rows: Vec<TextBoxRow>,
    /// The first source line visible in the viewport.
    pub first_visible_line: usize,
    /// Total logical line count of the source text.
    pub total_lines: usize,
}

/// Split `text` into logical lines using the same semantics as the composer:
/// empty text yields one empty line; a trailing newline yields an extra empty
/// line at the end.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
fn split_logical_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return vec![""];
    }
    let mut lines: Vec<&str> = text.lines().collect();
    if text.ends_with('\n') {
        lines.push("");
    }
    lines
}

/// Map a byte cursor within `text` to a logical `(line, col)` position,
/// flooring the byte offset down to the nearest UTF-8 char boundary first so
/// multibyte input cannot panic the slice.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
fn byte_cursor_to_caret(text: &str, byte_cursor: usize) -> TextCaret {
    let clamped = byte_cursor.min(text.len());
    let boundary = floor_char_boundary(text, clamped);
    let before = &text[..boundary];
    let line = before.matches('\n').count();
    let last_nl = before.rfind('\n').map_or(0, |p| p + 1);
    let col = before[last_nl..].chars().count();
    TextCaret { line, col }
}

/// Walk `idx` down to the nearest UTF-8 char boundary at or before `idx`.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
fn floor_char_boundary(text: &str, idx: usize) -> usize {
    let mut i = idx.min(text.len());
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Horizontal window for a single row: produce the substring of `line` that
/// fits within `content_width` characters AND keeps `caret_col` visible. When
/// the caret is beyond the right edge, the window scrolls right so the caret
/// is the last visible column; otherwise the window starts at column 0.
///
/// Returns the windowed text and the caret column relative to the window
/// start (still `Some` only when this row carries the caret).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
fn window_row(
    line: &str,
    caret_col: Option<usize>,
    content_width: usize,
) -> (String, Option<usize>) {
    if content_width == 0 {
        return (String::new(), None);
    }
    let caret = caret_col.unwrap_or(0);
    // Window start in char columns.
    let start = if caret < content_width {
        0
    } else {
        // Keep the caret inside the visible width, not as an extra column.
        caret.saturating_add(1).saturating_sub(content_width)
    };
    let (windowed, window_len) = if start == 0 {
        let windowed: String = line.chars().take(content_width).collect();
        let window_len = windowed.chars().count();
        (windowed, window_len)
    } else {
        let chars: Vec<char> = line.chars().collect();
        let windowed: String = chars
            .iter()
            .skip(start)
            .take(content_width)
            .copied()
            .collect();
        let window_len = chars.len().saturating_sub(start).min(content_width);
        (windowed, window_len)
    };
    let rel_caret = caret_col.map(|c| c.saturating_sub(start).min(window_len));
    (windowed, rel_caret)
}

/// Build a fixed-size [`TextBoxView`] projection of `text`.
///
/// - `byte_cursor` is floored to a UTF-8 char boundary before use.
/// - Vertical viewport: the caret line is always visible; `first_visible_line`
///   is derived from the caret (no stored scroll state).
/// - Horizontal viewport: each visible row is windowed to `content_width`
///   characters, keeping the caret column visible on the caret row.
/// - `rows.len() == viewport_rows` for `viewport_rows > 0` (padded blank);
///   `rows` is empty when `viewport_rows == 0`.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
#[must_use]
pub fn build_text_box_view(
    text: &str,
    byte_cursor: usize,
    viewport_rows: usize,
    content_width: usize,
) -> TextBoxView {
    let lines = split_logical_lines(text);
    let total_lines = lines.len();
    let caret = byte_cursor_to_caret(text, byte_cursor);

    if viewport_rows == 0 {
        return TextBoxView {
            rows: Vec::new(),
            first_visible_line: 0,
            total_lines,
        };
    }

    // Vertical viewport: keep the caret visible without stored scroll.
    let first = vertical_first_visible(caret.line, viewport_rows, total_lines);

    let mut rows: Vec<TextBoxRow> = Vec::with_capacity(viewport_rows);
    for vp_idx in 0..viewport_rows {
        let line_idx = first + vp_idx;
        if line_idx < total_lines {
            let caret_col = if caret.line == line_idx {
                Some(caret.col)
            } else {
                None
            };
            let (windowed, rel_caret) = window_row(lines[line_idx], caret_col, content_width);
            rows.push(TextBoxRow {
                text: windowed,
                caret_col: rel_caret,
            });
        } else {
            // Pad blank rows so the component occupies a fixed height.
            rows.push(TextBoxRow {
                text: String::new(),
                caret_col: None,
            });
        }
    }

    TextBoxView {
        rows,
        first_visible_line: first,
        total_lines,
    }
}

/// Compute the first visible line so the caret stays in the window
/// `[first, first + viewport_rows)` and the view never scrolls past the last
/// full page.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
fn vertical_first_visible(caret_line: usize, viewport_rows: usize, total_lines: usize) -> usize {
    if viewport_rows == 0 {
        return 0;
    }
    // Prefer the caret on the last row of the window when below it.
    let caret_first = caret_line.saturating_sub(viewport_rows.saturating_sub(1));
    // Never scroll past the last full page.
    let max_first = total_lines.saturating_sub(viewport_rows);
    caret_first.min(max_first)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Empty text produces one blank row and the caret lands on it.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    #[test]
    fn empty_text_one_blank_row_with_caret() {
        let v = build_text_box_view("", 0, 3, 20);
        assert_eq!(v.rows.len(), 3);
        assert_eq!(v.total_lines, 1);
        assert_eq!(v.first_visible_line, 0);
        // Caret is on the first (blank) row at column 0.
        assert_eq!(v.rows[0].text, "");
        assert_eq!(v.rows[0].caret_col, Some(0));
        // Padded rows have no caret.
        assert_eq!(v.rows[1].caret_col, None);
        assert_eq!(v.rows[2].caret_col, None);
    }

    /// Typing past the bottom of the viewport keeps the caret visible.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    #[test]
    fn caret_visible_after_many_lines() {
        let text = "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8";
        let v = build_text_box_view(text, text.len(), 3, 20);
        assert_eq!(v.rows.len(), 3);
        // Caret is on line 7 ("l8"); viewport is 3 -> first = 7-2 = 5.
        assert_eq!(v.first_visible_line, 5);
        // Rows 5,6,7 = "l6","l7","l8"; caret on row index 2.
        assert_eq!(v.rows[0].text, "l6");
        assert_eq!(v.rows[1].text, "l7");
        assert_eq!(v.rows[2].text, "l8");
        assert_eq!(v.rows[2].caret_col, Some(2));
        assert!(v.rows[0].caret_col.is_none());
        assert!(v.rows[1].caret_col.is_none());
    }

    /// Arrow-up-like cursor movement keeps the caret line visible when it
    /// rises above the prior window.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    #[test]
    fn caret_line_visible_when_rising() {
        let text = "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8";
        // Cursor on line 0 ("l1").
        let v = build_text_box_view(text, 0, 3, 20);
        assert_eq!(v.first_visible_line, 0);
        assert_eq!(v.rows[0].text, "l1");
        assert_eq!(v.rows[0].caret_col, Some(0));
    }

    /// A long single line keeps the caret visible within `content_width` by
    /// scrolling the window right.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    #[test]
    fn long_single_line_caret_visible_with_width() {
        let text = "abcdefghijklmnopqrstuvwxyz";
        // Caret at column 25 ('z'); content_width 10.
        let v = build_text_box_view(text, 25, 1, 10);
        assert_eq!(v.rows.len(), 1);
        let row = &v.rows[0];
        // Window keeps caret as last visible column: start = 25+1-10 = 16.
        assert_eq!(row.text, "qrstuvwxyz");
        // Caret relative col = 25 - 16 = 9.
        assert_eq!(row.caret_col, Some(9));
    }

    /// A caret exactly at `content_width` shifts one column so the caret cell
    /// remains inside the visible width instead of rendering an extra column.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    #[test]
    fn exact_width_line_caret_at_end_stays_inside_window() {
        let text = "abcdefghij";
        let v = build_text_box_view(text, text.len(), 1, 10);
        assert_eq!(v.rows[0].text, "bcdefghij");
        assert_eq!(v.rows[0].caret_col, Some(9));
    }
    /// Multibyte input must not panic and the caret column must count chars.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    #[test]
    fn multibyte_no_panic_correct_char_column() {
        // "héllo" — 'é' is two bytes; caret after the 'o' is byte 6, char col 5.
        let text = "héllo";
        let v = build_text_box_view(text, 6, 1, 20);
        assert_eq!(v.rows.len(), 1);
        assert_eq!(v.rows[0].text, "héllo");
        assert_eq!(v.rows[0].caret_col, Some(5));
    }

    /// A byte cursor in the middle of a multibyte sequence is floored to the
    /// preceding char boundary (no panic).
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    #[test]
    fn mid_sequence_byte_cursor_floors_safely() {
        // "é" is bytes [0xC3,0xA9]. A byte cursor of 1 lands inside it; floor to 0.
        let text = "é";
        let v = build_text_box_view(text, 1, 1, 20);
        assert_eq!(v.rows[0].text, "é");
        assert_eq!(v.rows[0].caret_col, Some(0));
    }

    /// Rows are padded to exactly `viewport_rows`.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    #[test]
    fn rows_padded_exactly_to_viewport() {
        let v = build_text_box_view("hi", 2, 5, 20);
        assert_eq!(v.rows.len(), 5);
        // Total logical lines = 1.
        assert_eq!(v.total_lines, 1);
        // First row has text; remaining 4 are blank padding.
        assert_eq!(v.rows[0].text, "hi");
        for r in &v.rows[1..] {
            assert_eq!(r.text, "");
        }
    }

    /// A trailing newline yields an extra blank row the caret can land on.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    #[test]
    fn trailing_newline_extra_blank_row() {
        // "a\n" -> lines ["a", ""]; cursor at byte 2 is on the blank line.
        let text = "a\n";
        let v = build_text_box_view(text, text.len(), 3, 20);
        assert_eq!(v.total_lines, 2);
        assert_eq!(v.rows[0].text, "a");
        assert_eq!(v.rows[1].text, "");
        // Caret on line 1 (the blank trailing row) at col 0.
        assert_eq!(v.rows[1].caret_col, Some(0));
    }

    /// `viewport_rows == 0` yields no rows and no panic.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    #[test]
    fn zero_viewport_empty_no_panic() {
        let v = build_text_box_view("hello\nworld", 5, 0, 20);
        assert!(v.rows.is_empty());
        assert_eq!(v.first_visible_line, 0);
        assert_eq!(v.total_lines, 2);
    }

    /// `content_width == 0` does not panic and yields empty row text.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    #[test]
    fn zero_content_width_no_panic() {
        let v = build_text_box_view("hello", 3, 1, 0);
        assert_eq!(v.rows.len(), 1);
        assert_eq!(v.rows[0].text, "");
        // No editable cell is visible when width is 0, so the caret is suppressed.
        assert_eq!(v.rows[0].caret_col, None);
    }

    /// Caret on a line shorter than the window stays at column 0 start.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    #[test]
    fn short_line_caret_within_window() {
        let v = build_text_box_view("ab", 1, 1, 10);
        assert_eq!(v.rows[0].text, "ab");
        assert_eq!(v.rows[0].caret_col, Some(1));
    }
}
