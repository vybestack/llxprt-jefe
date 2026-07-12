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
    /// The first visible display-row index in the viewport. Wrapping may make
    /// a single logical line span several display rows, so this is a row
    /// index, not a logical-line index.
    pub first_visible_row: usize,
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

/// One wrapped segment of a logical line: the text of the segment plus the
/// half-open `[start, end)` char-column range it covers within the source
/// line. Used internally to build display rows and map the caret onto them.
struct WrapSegment {
    text: String,
    start: usize,
    end: usize,
}

/// Split a single logical line into wrapped segments of at most
/// `content_width` display columns, breaking at word boundaries. Returns one
/// segment for a short line, multiple for a long line, and a single empty
/// segment for an empty line.
///
/// Word-wrap (never splitting a word) is delegated to the shared
/// [`crate::text_wrap`] primitive so the editor and the read-only displayer
/// wrap identically.
///
/// `content_width == 0` yields a single empty segment (the caller suppresses
/// the caret for width 0 anyway).
///
/// @requirement REQ-PR-009
/// @requirement REQ-TEXT-WRAP
fn wrap_line(line: &str, content_width: usize) -> Vec<WrapSegment> {
    crate::text_wrap::wrap_text(line, content_width)
        .into_iter()
        .map(|r| WrapSegment {
            text: r.text,
            start: r.start,
            end: r.end,
        })
        .collect()
}

/// Build a fixed-size [`TextBoxView`] projection of `text`.
///
/// - `byte_cursor` is floored to a UTF-8 char boundary before use.
/// - Wrapping: each logical line is split into wrapped display rows of at
///   most `content_width` characters, so long lines fold onto the next row
///   instead of scrolling off the right edge.
/// - Vertical viewport: the caret's wrapped row is always visible;
///   `first_visible_row` is the first visible display-row index (derived
///   from the caret, no stored scroll state).
/// - `rows.len() == viewport_rows` for `viewport_rows > 0` (padded blank);
///   `rows` is empty when `viewport_rows == 0`.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @requirement REQ-TEXTBOX-WRAP
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
            first_visible_row: 0,
            total_lines,
        };
    }

    let (display_rows, caret_row_idx) = build_wrapped_display_rows(&lines, caret, content_width);

    // If no caret row was recorded (e.g. content_width == 0 suppresses the
    // caret), anchor the viewport at the top.
    let caret_row = caret_row_idx.unwrap_or(0);
    let total_display = display_rows.len();
    let first = vertical_first_visible(caret_row, viewport_rows, total_display);

    let mut rows: Vec<TextBoxRow> = Vec::with_capacity(viewport_rows);
    for vp_idx in 0..viewport_rows {
        let disp_idx = first + vp_idx;
        if disp_idx < total_display {
            rows.push(display_rows[disp_idx].clone());
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
        first_visible_row: first,
        total_lines,
    }
}

/// Build the flat list of wrapped display rows for every logical line, plus
/// the index of the row that carries the caret (`None` when suppressed, e.g.
/// `content_width == 0`). Each logical line is wrapped to `content_width`
/// characters; a trailing caret at the end of a full-width line gets its own
/// empty row so it never overflows the visible width.
///
/// @requirement REQ-PR-009
/// @requirement REQ-TEXTBOX-WRAP
fn build_wrapped_display_rows(
    lines: &[&str],
    caret: TextCaret,
    content_width: usize,
) -> (Vec<TextBoxRow>, Option<usize>) {
    let mut display_rows: Vec<TextBoxRow> = Vec::new();
    let mut caret_row_idx: Option<usize> = None;
    for (line_idx, line) in lines.iter().enumerate() {
        // Only the caret line can trigger the full-width-trailing-caret case,
        // so defer the O(n) char count to that line.
        let caret_line_len = if line_idx == caret.line {
            line.chars().count()
        } else {
            0
        };
        for seg in wrap_line(line, content_width) {
            let seg_len = seg.end - seg.start;
            // A trailing caret at the end of a line that fills the full width
            // would overflow; emit the full segment row then a trailing empty
            // row that carries the caret inside the width.
            let full_width_end = content_width != 0
                && line_idx == caret.line
                && seg.end == caret_line_len
                && seg_len == content_width
                && caret.col == seg.end;
            if full_width_end {
                display_rows.push(TextBoxRow {
                    text: seg.text,
                    caret_col: None,
                });
                caret_row_idx = Some(display_rows.len());
                display_rows.push(TextBoxRow {
                    text: String::new(),
                    caret_col: Some(0),
                });
                continue;
            }

            let caret_col = caret_col_for_segment(&seg, seg_len, caret, line_idx, content_width);
            if caret_col.is_some() {
                caret_row_idx = Some(display_rows.len());
            }
            display_rows.push(TextBoxRow {
                text: seg.text,
                caret_col,
            });
        }
    }
    (display_rows, caret_row_idx)
}

/// Decide whether the caret belongs to one wrapped segment and, if so,
/// return its column relative to the segment start.
///
/// The caret belongs to this segment when:
/// - its column is inside the half-open `[start, end)` range, or
/// - it sits at the segment end on a non-full segment (a trailing caret that
///   fits on this row), or
/// - the segment is empty (`start == end`) and the caret sits at that
///   position (a blank line / trailing-newline row).
///
/// @requirement REQ-PR-009
/// @requirement REQ-TEXTBOX-WRAP
fn caret_col_for_segment(
    seg: &WrapSegment,
    seg_len: usize,
    caret: TextCaret,
    line_idx: usize,
    content_width: usize,
) -> Option<usize> {
    if content_width == 0 || line_idx != caret.line {
        return None;
    }
    // Segment membership, expressed as independent predicates so the operands
    // stay grouped by source (segment fields together, caret position alone).
    let caret_at_seg_end = caret.col == seg.end;
    let in_range = seg.start <= caret.col && caret.col < seg.end;
    let seg_has_room = seg_len < content_width;
    let seg_is_empty = seg.start == seg.end;
    let trailing_at_end = caret_at_seg_end && seg_has_room;
    let on_blank_row = seg_is_empty && caret_at_seg_end;
    if in_range || trailing_at_end || on_blank_row {
        Some(caret.col - seg.start)
    } else {
        None
    }
}

/// Compute the first visible display-row index so the caret row stays in the
/// window `[first, first + viewport_rows)` and the view never scrolls past
/// the last full page.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-TEXTBOX-WRAP
fn vertical_first_visible(caret_row: usize, viewport_rows: usize, total_rows: usize) -> usize {
    if viewport_rows == 0 {
        return 0;
    }
    // Prefer the caret on the last row of the window when below it.
    let caret_first = caret_row.saturating_sub(viewport_rows.saturating_sub(1));
    // Never scroll past the last full page.
    let max_first = total_rows.saturating_sub(viewport_rows);
    caret_first.min(max_first)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Find the (first) row carrying the caret, or panic with a clear message.
    fn caret_row(view: &TextBoxView) -> &TextBoxRow {
        let Some(row) = view.rows.iter().find(|r| r.caret_col.is_some()) else {
            panic!("expected a row carrying the caret, rows: {:?}", view.rows);
        };
        row
    }

    /// Empty text produces one blank row and the caret lands on it.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    #[test]
    fn empty_text_one_blank_row_with_caret() {
        let v = build_text_box_view("", 0, 3, 20);
        assert_eq!(v.rows.len(), 3);
        assert_eq!(v.total_lines, 1);
        assert_eq!(v.first_visible_row, 0);
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
        assert_eq!(v.first_visible_row, 5);
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
        assert_eq!(v.first_visible_row, 0);
        assert_eq!(v.rows[0].text, "l1");
        assert_eq!(v.rows[0].caret_col, Some(0));
    }

    /// A long single line WRAPS onto multiple display rows so the caret is
    /// always visible within `content_width` (no horizontal scroll off-screen).
    /// Caret at column 25 ('z') with width 10 lands on the wrapped row
    /// "uvwxyz" at relative col 5.
    ///
    /// @requirement REQ-PR-009
    /// @requirement REQ-TEXTBOX-WRAP
    #[test]
    fn long_single_line_caret_visible_with_width() {
        let text = "abcdefghijklmnopqrstuvwxyz";
        // Caret at column 25 ('z'); content_width 10.
        let v = build_text_box_view(text, 25, 1, 10);
        assert_eq!(v.rows.len(), 1);
        let row = &v.rows[0];
        // The caret's wrapped row is "uvwxyz" (cols 20..26).
        assert_eq!(row.text, "uvwxyz");
        // Caret relative col = 25 - 20 = 5.
        assert_eq!(row.caret_col, Some(5));
    }

    /// A line exactly `content_width` long fits on one row (no premature wrap),
    /// and the caret at the end gets its own trailing empty wrapped row.
    ///
    /// @requirement REQ-PR-009
    /// @requirement REQ-TEXTBOX-WRAP
    #[test]
    fn exact_width_line_caret_at_end_stays_inside_window() {
        let text = "abcdefghij";
        let v = build_text_box_view(text, text.len(), 3, 10);
        assert_eq!(v.rows[0].text, "abcdefghij");
        assert!(v.rows[0].caret_col.is_none());
        // The caret at the end of a full-width line lands on a trailing empty
        // wrapped row so it never overflows the visible width.
        let caret_row = caret_row(&v);
        assert_eq!(caret_row.text, "");
        assert_eq!(caret_row.caret_col, Some(0));
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
        assert_eq!(v.first_visible_row, 0);
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

    /// A line longer than `content_width` WRAPS onto multiple display rows
    /// instead of being truncated off-screen. No row may exceed the width.
    ///
    /// Regression for issue #212: text boxes don't wrap.
    ///
    /// @requirement REQ-TEXTBOX-WRAP
    #[test]
    fn long_line_wraps_across_rows() {
        // 26 chars at content_width 10 -> 3 wrapped rows.
        let v = build_text_box_view("abcdefghijklmnopqrstuvwxyz", 0, 5, 10);
        assert_eq!(v.rows[0].text, "abcdefghij");
        assert_eq!(v.rows[1].text, "klmnopqrst");
        assert_eq!(v.rows[2].text, "uvwxyz");
        // Caret at col 0 lands on the first wrapped row.
        assert_eq!(v.rows[0].caret_col, Some(0));
        for r in &v.rows {
            assert!(
                r.text.chars().count() <= 10,
                "wrapped row must not exceed content_width: {:?}",
                r.text
            );
        }
    }

    /// When wrapping, the caret must map onto the correct wrapped row at the
    /// correct relative column. Caret at col 25 ('z') on a width-10 wrap.
    ///
    /// Regression for issue #212: text boxes don't wrap.
    ///
    /// @requirement REQ-TEXTBOX-WRAP
    #[test]
    fn long_line_caret_lands_on_correct_wrapped_row() {
        let v = build_text_box_view("abcdefghijklmnopqrstuvwxyz", 25, 5, 10);
        // col 25 is on the third wrapped row "uvwxyz" (cols 20..26), at
        // relative col 5 ('z').
        let caret_row = caret_row(&v);
        assert_eq!(caret_row.text, "uvwxyz");
        assert_eq!(caret_row.caret_col, Some(5));
    }

    /// A caret in the middle of a wrapped line lands on the middle row.
    ///
    /// Regression for issue #212: text boxes don't wrap.
    ///
    /// @requirement REQ-TEXTBOX-WRAP
    #[test]
    fn long_line_caret_in_middle_wrapped_row() {
        // col 15 lands on the second wrapped row "klmnopqrst" (cols 10..20),
        // at relative col 5 ('p').
        let v = build_text_box_view("abcdefghijklmnopqrstuvwxyz", 15, 3, 10);
        let caret_row = caret_row(&v);
        assert_eq!(caret_row.text, "klmnopqrst");
        assert_eq!(caret_row.caret_col, Some(5));
    }

    /// When a single logical line wraps to more rows than the viewport, the
    /// caret's wrapped row must stay visible (vertical viewport follows the
    /// caret across wrapped rows).
    ///
    /// Regression for issue #212: text boxes don't wrap.
    ///
    /// @requirement REQ-TEXTBOX-WRAP
    #[test]
    fn caret_wrapped_row_visible_when_exceeds_viewport() {
        // 30 chars at width 10 -> 3 wrapped rows; viewport of 1 must show the
        // caret's wrapped row.
        let text = "abcdefghijklmnopqrstuvwxyz0123";
        let v = build_text_box_view(text, 25, 1, 10);
        assert_eq!(v.rows.len(), 1);
        // col 25 is on the third wrapped row "uvwxyz0123" (cols 20..30), at
        // relative col 5.
        assert_eq!(v.rows[0].text, "uvwxyz0123");
        assert_eq!(v.rows[0].caret_col, Some(5));
    }

    /// A line exactly `content_width` long fits on one row (no premature wrap),
    /// and the caret at the end gets its own trailing empty wrapped row so the
    /// caret cell stays inside the visible width.
    ///
    /// Regression for issue #212: text boxes don't wrap.
    ///
    /// @requirement REQ-TEXTBOX-WRAP
    #[test]
    fn exact_width_line_no_premature_wrap_caret_on_trailing_row() {
        // 10 chars exactly fill width 10; caret at end (col 10) needs a cell.
        let v = build_text_box_view("abcdefghij", 10, 3, 10);
        assert_eq!(v.rows[0].text, "abcdefghij");
        assert!(v.rows[0].caret_col.is_none());
        // Caret lands on a trailing empty row so it never overflows the width.
        let caret_row = caret_row(&v);
        assert_eq!(caret_row.text, "");
        assert_eq!(caret_row.caret_col, Some(0));
    }

    /// Mixing explicit newlines with wrapping: each logical line wraps
    /// independently, and the caret follows.
    ///
    /// Regression for issue #212: text boxes don't wrap.
    ///
    /// @requirement REQ-TEXTBOX-WRAP
    #[test]
    fn wrap_with_explicit_newlines() {
        // Line 0 ("abcdefghijkl") wraps to ["abcdefgh", "ijkl"]; line 1
        // ("mnop") is short. Caret at end of all text (col 4 of "mnop").
        let text = "abcdefghijkl\nmnop";
        let v = build_text_box_view(text, text.len(), 5, 8);
        assert_eq!(v.rows[0].text, "abcdefgh");
        assert_eq!(v.rows[1].text, "ijkl");
        assert_eq!(v.rows[2].text, "mnop");
        assert_eq!(v.rows[2].caret_col, Some(4));
    }

    /// Wrapping respects multibyte character boundaries: a wide-ish multibyte
    /// sequence still splits on char boundaries and the caret counts chars.
    ///
    /// Regression for issue #212: text boxes don't wrap.
    ///
    /// @requirement REQ-TEXTBOX-WRAP
    #[test]
    fn wrap_multibyte_on_char_boundary() {
        // "héllo" + "WORLD" = h é l l o W O R L D = 10 chars at width 4.
        // Wrapped: ["héll", "oWOR", "LD"].
        let text = "hélloWORLD";
        let v = build_text_box_view(text, 0, 5, 4);
        assert_eq!(v.rows[0].text, "héll");
        assert_eq!(v.rows[1].text, "oWOR");
        assert_eq!(v.rows[2].text, "LD");
    }

    /// A very long single line in a narrow viewport still keeps every wrapped
    /// row within the width and keeps the caret visible (no horizontal scroll
    /// off-screen).
    ///
    /// Regression for issue #212: text boxes don't wrap.
    ///
    /// @requirement REQ-TEXTBOX-WRAP
    #[test]
    fn very_long_line_never_overflows_width() {
        let text = "x".repeat(200);
        let v = build_text_box_view(&text, 150, 6, 20);
        for r in &v.rows {
            assert!(
                r.text.chars().count() <= 20,
                "no wrapped row may exceed content_width: {:?}",
                r.text.chars().count()
            );
        }
        let caret_row = caret_row(&v);
        assert_eq!(caret_row.caret_col, Some(10));
    }

    /// The editor wraps at WORD boundaries: a row never splits a word, and the
    /// caret maps correctly onto the word-wrapped row it lands in.
    ///
    /// Regression for issue #212: text boxes don't wrap (word-wrap, not char).
    ///
    /// @requirement REQ-TEXT-WRAP
    #[test]
    fn editor_wraps_at_word_boundary_not_mid_word() {
        // "alpha beta gamma" at width 8 -> ["alpha", "beta", "gamma"] (each
        // word is wider than the remaining budget on the row).
        let v = build_text_box_view("alpha beta gamma", 0, 3, 8);
        let texts: Vec<&str> = v.rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, vec!["alpha", "beta", "gamma"]);
        // No row may start with a space or split a word.
        for t in &texts {
            assert!(!t.starts_with(' '), "row must not start with space: {t:?}");
        }
    }

    /// Word-wrap with a caret in the middle of a word on a later row: the
    /// caret column is relative to that word-row's start.
    ///
    /// @requirement REQ-TEXT-WRAP
    #[test]
    fn editor_word_wrap_caret_in_middle_word() {
        // "alpha beta gamma" at width 8 -> rows "alpha"(0-5),"beta"(6-10),
        // "gamma"(11-16). Caret at source col 8 (inside "beta" at rel 2).
        let v = build_text_box_view("alpha beta gamma", 8, 3, 8);
        let caret_row = caret_row(&v);
        assert_eq!(caret_row.text, "beta");
        assert_eq!(caret_row.caret_col, Some(2));
    }

    /// A word that fits within the width but not on the current row moves to
    /// the next row whole (not split).
    ///
    /// @requirement REQ-TEXT-WRAP
    #[test]
    fn editor_word_fits_wraps_whole_to_next_row() {
        // "aaaa bbbb" at width 5 -> "aaaa" fits (4), "bbbb" (4) fits width but
        // 4+1(space)+4 > 5 so it wraps whole to row 1.
        let v = build_text_box_view("aaaa bbbb", 0, 2, 5);
        let texts: Vec<&str> = v.rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, vec!["aaaa", "bbbb"]);
    }

    /// Caret in trailing spaces past the last word must still be visible and
    /// land on the (trimmed) row, not vanish because trailing spaces were
    /// dropped from the display text. Regression for the word-wrap trim path.
    ///
    /// @requirement REQ-TEXT-WRAP
    #[test]
    fn caret_in_trailing_spaces_still_visible() {
        // "ab" + 3 spaces at width 10: the row text is trimmed to "ab" but
        // the caret at col 5 (in the spaces) must remain visible.
        let v = build_text_box_view("ab   ", 5, 2, 10);
        let Some(caret_row) = v.rows.iter().find(|r| r.caret_col.is_some()) else {
            panic!(
                "caret in trailing spaces must be visible, rows: {:?}",
                v.rows
            );
        };
        assert_eq!(caret_row.text, "ab");
        // The caret column must not exceed the content width.
        assert!(caret_row.caret_col.unwrap_or(0) <= 10);
    }
}
