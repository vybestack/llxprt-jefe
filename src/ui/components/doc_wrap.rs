//! Pure, iocraft-free document wrapping projection for [`ScrollableText`].
//!
//! [`ScrollableText`] renders a scrollable text document (issue/PR detail
//! bodies + comments + the inline editors). Each content *line* may wrap to
//! several display *rows* at the pane content width; this module is the single
//! source of truth for that line→row projection so the render path, the
//! inline-editor caret placement, and the mouse-selection reverse-map cannot
//! drift.
//!
//! It builds on the lower-level [`crate::text_wrap::wrap_text`] primitive
//! (which wraps one logical line into `WrapRow`s carrying half-open
//! `[start, end)` char ranges). This module lifts that to the whole document:
//! it tracks which content *line* each display row belongs to and the char
//! range of that row *within its line*, so consumers can map between screen
//! rows and content coordinates.
//!
//! # Coordinate spaces
//!
//! - **Content line**: 0-based index into `content.split('\n')`. This is the
//!   space the selection model (`SelectionPoint.line`) and the scroll offset
//!   (`detail_scroll_offset`) live in — both stay line-based.
//! - **Display row**: 0-based index into the flat wrapped-rows list. The
//!   render path windows display rows into a fixed-height viewport.
//! - **Line char offset**: 0-based char column within a single content line.
//!
//! `width` is counted in Unicode scalar values (one per `char`), matching
//! [`crate::text_wrap`] and the editor's char-based caret model. Terminal
//! display-width wrapping (CJK/emoji) is a separate, larger change.
//!
//! This module is side-effect-free and iocraft-free so it is fully
//! unit-testable and reusable by both the renderer and the selection layer.
//!
//! @requirement REQ-DOC-WRAP

use crate::text_wrap::wrap_text;

/// One display row produced by wrapping a content document.
///
/// A row always belongs to exactly one content *line* and covers the half-open
/// `[line_char_start, line_char_end)` char range within that line. Even blank
/// lines and trailing-newline rows produce one row (with an empty `text` and a
/// zero-width range anchored at the line start), so the projection is total.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocDisplayRow {
    /// The wrapped text for this row (no trailing newline, trailing spaces
    /// trimmed at wrap boundaries — same semantics as [`crate::text_wrap::WrapRow`]).
    pub text: String,
    /// 0-based content-line index this row belongs to.
    pub line: usize,
    /// Inclusive start char column within the content line.
    pub line_char_start: usize,
    /// Exclusive end char column within the content line.
    pub line_char_end: usize,
}

/// Wrap a full content document (lines joined by `'\n'`) into a flat list of
/// display rows of at most `width` characters, breaking at word boundaries.
///
/// See the module docs for the full semantics. `width == 0` yields one empty
/// row per content line (callers suppress the caret / selection). The result
/// is never empty: even empty input produces a single empty row for line 0.
#[must_use]
pub fn wrap_document(content: &str, width: usize) -> Vec<DocDisplayRow> {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut rows: Vec<DocDisplayRow> = Vec::new();
    for (line_idx, line) in lines.iter().enumerate() {
        for seg in wrap_text(line, width) {
            rows.push(DocDisplayRow {
                text: seg.text,
                line: line_idx,
                line_char_start: seg.start,
                line_char_end: seg.end,
            });
        }
    }
    if rows.is_empty() {
        // Only reachable for content that produced no lines at all, which
        // `split('\n')` never does — guard defensively anyway.
        rows.push(DocDisplayRow {
            text: String::new(),
            line: 0,
            line_char_start: 0,
            line_char_end: 0,
        });
    }
    rows
}

/// The display-row index where content `line` begins, or the last row if `line`
/// is past the end. Used to convert a line-based scroll offset into a
/// display-row window start.
///
/// Returns 0 when there are no rows.
#[must_use]
pub fn line_first_row(rows: &[DocDisplayRow], line: usize) -> usize {
    for (idx, r) in rows.iter().enumerate() {
        if r.line >= line {
            return idx;
        }
    }
    rows.len().saturating_sub(1)
}

/// Map a viewport-relative display row back to `(content_line, line_char_offset)`,
/// the content coordinates the selection model uses.
///
/// `vp_row` is 0-based relative to the top of the visible window
/// (`first_visible_row`). In-range rows map to their row's left edge
/// (`line_char_start`; the caller adds the in-row column). Values past the last
/// row clamp to the last row's line at its END column (`line_char_end`), so a
/// click in empty space below the document selects to the end of the last
/// content line rather than its start. Returns `None` only when there are no
/// rows.
#[must_use]
pub fn viewport_row_to_content(
    rows: &[DocDisplayRow],
    first_visible_row: usize,
    vp_row: usize,
) -> Option<(usize, usize)> {
    let target = first_visible_row.saturating_add(vp_row);
    let last_idx = rows.len().saturating_sub(1);
    let row = rows.get(target.min(last_idx))?;
    let char_offset = if target > last_idx {
        // Past the last row: anchor at the last line's end so selection extends
        // to the document tail, not its head.
        row.line_char_end
    } else {
        row.line_char_start
    };
    Some((row.line, char_offset))
}

/// Find the display row + relative column that carries the caret at
/// `(content_line, line_char_col)`, for inline-editor caret placement.
///
/// Returns `(global_row_index, col_within_row)` where `col_within_row` is the
/// caret column relative to the row's `line_char_start`. The caret belongs to
/// the row whose `[line_char_start, line_char_end)` contains `line_char_col`,
/// or — for a caret at a line end — the row ending at that column. Clamps
/// safely to the line's rows (never panics on a column in a gap).
#[must_use]
pub fn caret_row_for_line_col(
    rows: &[DocDisplayRow],
    content_line: usize,
    line_char_col: usize,
) -> Option<(usize, usize)> {
    // Gather this line's rows in order.
    let mut line_rows: Vec<usize> = Vec::new();
    for (idx, r) in rows.iter().enumerate() {
        if r.line == content_line {
            line_rows.push(idx);
        }
    }
    let (&first, rest) = line_rows.split_first()?;
    let mut best_idx = first;
    let mut best_rel = 0usize;
    for idx in std::iter::once(first).chain(rest.iter().copied()) {
        let r = &rows[idx];
        if line_char_col < r.line_char_end {
            return Some((idx, line_char_col.saturating_sub(r.line_char_start)));
        }
        best_idx = idx;
        best_rel = r.line_char_end.saturating_sub(r.line_char_start);
    }
    Some((best_idx, best_rel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_document_one_row_for_line_zero() {
        let rows = wrap_document("", 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].line, 0);
        assert!(rows[0].text.is_empty());
    }

    #[test]
    fn short_lines_one_row_each() {
        let rows = wrap_document("alpha\nbeta\ngamma", 50);
        assert_eq!(rows.len(), 3);
        assert_eq!((rows[0].line, rows[0].text.as_str()), (0, "alpha"));
        assert_eq!((rows[1].line, rows[1].text.as_str()), (1, "beta"));
        assert_eq!((rows[2].line, rows[2].text.as_str()), (2, "gamma"));
    }

    #[test]
    fn long_line_wraps_into_multiple_rows_same_line() {
        // width 5: "alpha bravo" -> "alpha" | "bravo" (both on line 0).
        let rows = wrap_document("alpha bravo", 5);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.line == 0), "both rows on line 0");
        assert_eq!(rows[0].text, "alpha");
        assert_eq!(rows[1].text, "bravo");
    }

    #[test]
    fn row_char_ranges_are_within_line_bounds() {
        let rows = wrap_document("alpha bravo\nx", 5);
        // line 0 rows: wrap_text keeps ranges CONTIGUOUS, so the trailing
        // space after "alpha" (col 5) belongs to row 0's range [0,6) even
        // though the displayed text is trimmed to "alpha"; row 1 covers
        // "bravo" [6,11).
        let line0: Vec<&DocDisplayRow> = rows.iter().filter(|r| r.line == 0).collect();
        assert_eq!(line0.len(), 2);
        assert_eq!((line0[0].line_char_start, line0[0].line_char_end), (0, 6));
        assert_eq!((line0[1].line_char_start, line0[1].line_char_end), (6, 11));
    }

    #[test]
    fn line_first_row_locates_content_line_start() {
        // line 0 wraps to 2 rows; line 1 starts at display row 2.
        let rows = wrap_document("alpha bravo\nsecond", 5);
        assert_eq!(line_first_row(&rows, 0), 0);
        assert_eq!(line_first_row(&rows, 1), 2);
    }

    #[test]
    fn line_first_row_clamps_past_end() {
        let rows = wrap_document("one\ntwo", 50);
        assert_eq!(line_first_row(&rows, 99), rows.len() - 1);
    }

    #[test]
    fn viewport_row_to_content_maps_wrapped_rows() {
        // line 0 wraps to 2 rows; clicking viewport row 1 hits line 0 row 1.
        let rows = wrap_document("alpha bravo\nsecond", 5);
        let first = line_first_row(&rows, 0);
        // vp row 0 -> line 0, char start 0
        assert_eq!(viewport_row_to_content(&rows, first, 0), Some((0, 0)));
        // vp row 1 -> line 0, char start 6 (the "bravo" row)
        assert_eq!(viewport_row_to_content(&rows, first, 1), Some((0, 6)));
        // vp row 2 -> line 1
        assert_eq!(viewport_row_to_content(&rows, first, 2), Some((1, 0)));
        // vp row past the last row clamps to the last line's END (11 for line
        // 0 "alpha bravo"), so a click in empty space below selects to the
        // document tail, not its head.
        assert_eq!(viewport_row_to_content(&rows, first, 99), Some((1, 6)));
    }

    #[test]
    fn caret_row_for_line_col_finds_wrapped_subrow() {
        // width 5: "alpha"(0..6 incl trailing space) | "bravo"(6..12) |
        // "charl"(12..17) | "ie"(17..19) — "charlie" (7 chars) hard-breaks.
        let rows = wrap_document("alpha bravo charlie", 5);
        // caret at col 8 (inside "bravo" [6,12)) -> row 1, rel 2
        assert_eq!(caret_row_for_line_col(&rows, 0, 8), Some((1, 2)));
        // caret at col 0 -> row 0 rel 0
        assert_eq!(caret_row_for_line_col(&rows, 0, 0), Some((0, 0)));
        // caret at col 19 (end) -> last row (idx 3, "ie" [17,19)) rel 2
        assert_eq!(caret_row_for_line_col(&rows, 0, 19), Some((3, 2)));
    }

    #[test]
    fn caret_row_for_unknown_line_returns_none() {
        let rows = wrap_document("alpha\nbeta", 50);
        assert_eq!(caret_row_for_line_col(&rows, 99, 0), None);
    }

    #[test]
    fn trailing_newline_yields_empty_row() {
        let rows = wrap_document("abc\n", 50);
        // line 0 "abc", line 1 "" (from trailing newline)
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].line, 1);
        assert!(rows[1].text.is_empty());
    }

    #[test]
    fn zero_width_one_empty_row_per_line() {
        let rows = wrap_document("abc\ndef", 0);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.text.is_empty()));
    }
}
