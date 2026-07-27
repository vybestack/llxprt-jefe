//! Pure, iocraft-free document wrapping and scroll-geometry projection.
//!
//! Scrollable detail views render text documents whose content *lines* may wrap to
//! several display *rows* at the pane content width; this module is the single
//! source of truth for that line→row projection so the render path, the
//! inline-editor caret placement, and the mouse-selection reverse-map cannot
//! drift.
//!
//! It retains character ranges for the editor/selection model while measuring
//! row capacity in terminal display cells.
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
//! `width` is counted in terminal display cells. Source ranges remain Unicode
//! scalar offsets because the editor and selection model are char-based.
//!
//! This module is side-effect-free and iocraft-free so it is fully
//! unit-testable and reusable by both the renderer and the selection layer.
//!
//! @requirement REQ-DOC-WRAP

use unicode_width::UnicodeWidthChar;

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
/// display rows of at most `width` terminal cells, breaking at whitespace.
///
/// See the module docs for the full semantics. `width == 0` yields one empty
/// row per content line (callers suppress the caret / selection). The result
/// is never empty: even empty input produces a single empty row for line 0.
#[must_use]
pub fn wrap_document(content: &str, width: usize) -> Vec<DocDisplayRow> {
    let mut rows = Vec::new();
    for (line_idx, line) in content.split('\n').enumerate() {
        rows.extend(
            crate::text_wrap::wrap_text(line, width)
                .into_iter()
                .map(|row| DocDisplayRow {
                    text: row.text,
                    line: line_idx,
                    line_char_start: row.start,
                    line_char_end: row.end,
                }),
        );
    }
    rows
}

/// Convert a terminal-cell column in `text` to a clamped character boundary.
#[must_use]
pub fn display_cell_to_char_offset(text: &str, cell_col: usize) -> usize {
    if cell_col == 0 {
        return 0;
    }
    let mut used = 0;
    let mut char_offset = 0;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        let width = UnicodeWidthChar::width(character).unwrap_or(0);
        if width > 0 && cell_col <= used {
            return char_offset;
        }
        used = used.saturating_add(width);
        char_offset += 1;
        while chars
            .peek()
            .is_some_and(|next| UnicodeWidthChar::width(*next).unwrap_or(0) == 0)
        {
            chars.next();
            char_offset += 1;
        }
        if cell_col <= used {
            return char_offset;
        }
    }
    char_offset
}

/// Map a visible wrapped row and terminal-cell column to content coordinates.
#[must_use]
pub fn viewport_cell_to_content(
    rows: &[DocDisplayRow],
    first_visible_row: usize,
    vp_row: usize,
    cell_col: usize,
) -> Option<(usize, usize)> {
    let target = first_visible_row.saturating_add(vp_row);
    let last_idx = rows.len().checked_sub(1)?;
    let row = rows.get(target.min(last_idx))?;
    if target > last_idx {
        return Some((row.line, row.line_char_end));
    }
    let relative = display_cell_to_char_offset(&row.text, cell_col);
    Some((
        row.line,
        row.line_char_start
            .saturating_add(relative)
            .min(row.line_char_end),
    ))
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

/// Compute the largest content-line scroll offset that can reveal the document tail.
///
/// The returned offset remains in content-line units. It is the earliest line
/// whose wrapped suffix fits in `viewport_rows`; when no full suffix fits, the
/// final content line is the best representable line-based offset.
#[must_use]
pub fn max_content_line_scroll_offset(rows: &[DocDisplayRow], viewport_rows: usize) -> usize {
    let Some(last) = rows.last() else {
        return 0;
    };
    if viewport_rows == 0 {
        return last.line;
    }
    if rows.len() <= viewport_rows {
        return 0;
    }

    let required_first_row = rows.len().saturating_sub(viewport_rows);
    rows.iter()
        .enumerate()
        .find(|(index, row)| {
            *index >= required_first_row
                && (*index == 0 || rows[index.saturating_sub(1)].line != row.line)
        })
        .map_or(last.line, |(_, row)| row.line)
}

/// Compute the minimal content-line offset that reveals an inclusive line range.
///
/// This preserves the state model's content-line offsets while using wrapped
/// display rows for visibility. Ranges taller than the viewport anchor at their
/// first content line, matching the existing line-based reveal policy.
#[must_use]
pub fn reveal_content_line_range(
    rows: &[DocDisplayRow],
    item_start: usize,
    item_end: usize,
    offset: usize,
    viewport_rows: usize,
) -> usize {
    if viewport_rows == 0 || rows.is_empty() {
        return offset;
    }
    let first_visible = line_first_row(rows, offset);
    let item_first = line_first_row(rows, item_start);
    let item_last = line_last_row(rows, item_end);
    let last_visible = first_visible
        .saturating_add(viewport_rows)
        .saturating_sub(1);
    if item_first >= first_visible && item_last <= last_visible {
        return offset;
    }
    if item_last < first_visible || item_first < first_visible {
        return item_start;
    }
    if item_last.saturating_sub(item_first).saturating_add(1) > viewport_rows {
        return item_start;
    }

    first_line_revealing_row(rows, item_last, viewport_rows).min(item_start)
}

fn line_last_row(rows: &[DocDisplayRow], line: usize) -> usize {
    rows.iter()
        .enumerate()
        .rev()
        .find(|(_, row)| row.line <= line)
        .map_or(0, |(index, _)| index)
}

fn first_line_revealing_row(
    rows: &[DocDisplayRow],
    target_row: usize,
    viewport_rows: usize,
) -> usize {
    rows.iter()
        .enumerate()
        .filter(|(index, row)| *index == 0 || rows[index.saturating_sub(1)].line != row.line)
        .find(|(index, _)| index.saturating_add(viewport_rows) > target_row)
        .map_or_else(
            || rows.last().map_or(0, |row| row.line),
            |(_, row)| row.line,
        )
}

/// Find the display row + relative column that carries the caret at
/// `(content_line, line_char_col)`, for inline-editor caret placement.
///
/// Returns `(global_row_index, char_offset_within_row)` where
/// `char_offset_within_row` is the 0-based Unicode SCALAR offset of the caret
/// column relative to the row's `line_char_start`. This matches the renderer
/// (`ScrollableText`'s `cursor_row_element`), which slices row text by scalar
/// position via `chars.iter().take(col)` to paint the glyph under the caret.
/// Returning a terminal-cell width here instead would shift the caret for wide
/// (CJK/emoji) and zero-width (combining mark) glyphs (issue #429).
///
/// The caret belongs to the row whose `[line_char_start, line_char_end)`
/// contains `line_char_col`, or — for a caret at a line end — the row ending
/// at that column. Clamps safely to the line's rows (never panics on a column
/// in a gap).
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
            let char_col = line_char_col.saturating_sub(r.line_char_start);
            return Some((idx, char_col));
        }
        best_idx = idx;
        best_rel = r.text.chars().count();
    }
    Some((best_idx, best_rel))
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

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

    /// The inline-editor caret column coordinate is a Unicode SCALAR offset
    /// relative to the row's `line_char_start`, NOT a terminal-cell width.
    /// The renderer (`cursor_row_element`) slices the row's chars by scalar
    /// position to paint the glyph under the caret, so the projection must
    /// return the scalar offset to match. For wide CJK glyphs (display width
    /// 2, scalar width 1) the two diverge: a caret between two CJK glyphs is
    /// at scalar offset 1 but cell-width offset 2. This case must return the
    /// scalar offset (1), otherwise the rendered caret shifts one cell too
    /// far right (issue #429).
    #[test]
    fn caret_row_for_line_col_returns_char_offset_for_cjk() {
        // "a甲b丙" fits one row at width 6: [line 0, char range [0,4)].
        let rows = wrap_document("a甲b丙", 6);
        assert_eq!(rows.len(), 1, "fixture must fit one row: {rows:?}");
        // caret at char col 2 lands on 'b'. Its scalar offset within the row
        // is 2; its cell-width offset would be 3 ('a' + '甲' = 1 + 2 cells).
        assert_eq!(
            caret_row_for_line_col(&rows, 0, 2),
            Some((0, 2)),
            "caret column must be a scalar offset, not a terminal-cell width"
        );
    }

    /// A combining mark (`e\u{301}`) is display width 0 but scalar width 1.
    /// The caret after it must advance by one scalar offset, not zero. The
    /// renderer slices one char per scalar, so a zero-width column here would
    /// paint the caret on the combining mark instead of the following base
    /// glyph (issue #429).
    #[test]
    fn caret_row_for_line_col_counts_combining_marks_as_scalars() {
        // "e\u{301}x" fits one row: [line 0, char range [0,3)].
        let rows = wrap_document("e\u{301}x", 4);
        assert_eq!(rows.len(), 1, "fixture must fit one row: {rows:?}");
        // caret at char col 2 lands on 'x'. Scalar offset within the row is 2
        // (base + combining mark); cell-width offset would be 1 (combining
        // mark contributes 0 cells).
        assert_eq!(
            caret_row_for_line_col(&rows, 0, 2),
            Some((0, 2)),
            "combining mark must count as one scalar offset"
        );
    }

    #[test]
    fn wrapped_scroll_bound_stays_in_content_line_units() {
        let rows = wrap_document("alpha bravo charlie\nanchor\nhelp", 5);
        assert_eq!(max_content_line_scroll_offset(&rows, 4), 1);
        assert_eq!(max_content_line_scroll_offset(&rows, rows.len()), 0);
        assert_eq!(max_content_line_scroll_offset(&rows, 0), 2);
    }

    #[test]
    fn reveal_range_uses_wrapped_rows_to_keep_tail_anchor_visible() {
        let rows = wrap_document("alpha bravo charlie\nanchor\nhelp", 5);
        assert_eq!(reveal_content_line_range(&rows, 1, 2, 0, 4), 1);
        assert_eq!(reveal_content_line_range(&rows, 1, 2, 1, 4), 1);
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

    #[test]
    fn document_rows_match_the_shared_terminal_cell_wrapper() {
        let rows = wrap_document("甲乙丙", 4);
        let shared = crate::text_wrap::wrap_text("甲乙丙", 4);

        assert_eq!(
            rows.iter()
                .map(|row| (row.text.as_str(), row.line_char_start, row.line_char_end))
                .collect::<Vec<_>>(),
            shared
                .iter()
                .map(|row| (row.text.as_str(), row.start, row.end))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn cjk_wraps_by_terminal_cells_and_retains_char_ranges() {
        let rows = wrap_document("甲乙丙", 4);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            (
                rows[0].text.as_str(),
                rows[0].line_char_start,
                rows[0].line_char_end,
            ),
            ("甲乙", 0, 2)
        );
        assert_eq!(
            (
                rows[1].text.as_str(),
                rows[1].line_char_start,
                rows[1].line_char_end,
            ),
            ("丙", 2, 3)
        );
    }

    #[test]
    fn overwide_glyph_uses_finite_placeholder_and_retains_source_range() {
        let rows = wrap_document("甲", 1);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "…");
        assert_eq!((rows[0].line_char_start, rows[0].line_char_end), (0, 1));
        assert!(UnicodeWidthStr::width(rows[0].text.as_str()) <= 1);
    }

    #[test]
    fn display_cells_map_wide_and_combining_text_to_char_boundaries() {
        assert_eq!(display_cell_to_char_offset("甲乙", 1), 1);
        assert_eq!(display_cell_to_char_offset("甲乙", 2), 1);
        assert_eq!(display_cell_to_char_offset("e\u{301}x", 1), 2);
        assert_eq!(display_cell_to_char_offset("e\u{301}x", 2), 3);
    }

    #[test]
    fn viewport_cell_mapping_clamps_to_the_visible_wrapped_row() {
        let rows = wrap_document("甲乙丙", 4);
        assert_eq!(viewport_cell_to_content(&rows, 0, 0, 99), Some((0, 2)));
        assert_eq!(viewport_cell_to_content(&rows, 0, 1, 1), Some((0, 3)));
    }
}
