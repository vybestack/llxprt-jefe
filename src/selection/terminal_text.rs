//! Wrap-aware, width-aware terminal selection text extraction (issue #197).
//!
//! [`selection_text`] (in [`super::text`]) joins content lines with `\n`
//! unconditionally — correct for most panes but wrong for the terminal pane,
//! where a soft-wrapped row should join its continuation with NO separator.
//!
//! [`terminal_selection_text`] is the terminal-pane-specific extractor: it
//! reads `wraps[row]` from the [`TerminalSnapshot`] to decide whether two
//! consecutive rows are a soft wrap (join directly) or a hard newline (join
//! with `\n`), and skips `wide_spacer` cells so a selection across a width-2
//! glyph copies just the glyph, not the glyph + a spurious space.
//!
//! All functions here are pure, iocraft-free, and `#[must_use]` so they are
//! fully unit-testable without the runtime.

use crate::runtime::TerminalSnapshot;
use crate::selection::text::{TextSelection, normalize_selection};

/// Build the copyable text for a terminal selection from a snapshot.
///
/// Joins covered rows: when `wraps[row]` is true, row `row` soft-wraps into
/// `row+1` and the two rows join with NO separator; otherwise a `\n` separates
/// them. `wide_spacer` cells are skipped so a width-2 glyph contributes just
/// its leading cell's character.
///
/// The selection is normalized first; coordinates are clamped to snapshot
/// bounds so out-of-range selections never panic.
#[must_use]
pub fn terminal_selection_text(snapshot: &TerminalSnapshot, selection: &TextSelection) -> String {
    if snapshot.cells.is_empty() {
        return String::new();
    }
    let (start, end) = normalize_selection(&selection.anchor, &selection.focus);
    let last = snapshot.cells.len().saturating_sub(1);
    let start_line = start.line.min(last);
    let end_line = end.line.min(last);
    // Compare the clamped (not original) lines: when both points land past the
    // last row they clamp to the same line, but the original `start.line !=
    // end.line` would skip this branch and run the multi-row path, producing
    // garbled tail+head-of-same-row text (issue #197 review).
    if start_line == end_line {
        // When the start point was past the last row, start_line was clamped
        // to `last`; reset the column to 0 so we do not slice into an arbitrary
        // column of the wrong row (mirrors the multi-row path and the
        // end-clamping below). Issue #197 review.
        let start_col = if start.line > last { 0 } else { start.col };
        let end_col = if end.line > last { usize::MAX } else { end.col };
        return terminal_row_text(snapshot, start_line, start_col, end_col);
    }

    let mut out = String::new();

    // Tail of the start row (from start.col to end of row). When the start
    // point was past the last row, start_line was clamped to `last`; reset the
    // column to 0 so we do not slice into an arbitrary column of the wrong row
    // (mirrors the end-clamping logic below).
    let start_clamped_down = start.line > last;
    let start_col = if start_clamped_down { 0 } else { start.col };
    out.push_str(&terminal_row_text(
        snapshot,
        start_line,
        start_col,
        usize::MAX,
    ));

    // Middle rows.
    for row in (start_line + 1)..end_line {
        join_row(&mut out, snapshot, row.saturating_sub(1));
        out.push_str(&terminal_row_text(snapshot, row, 0, usize::MAX));
    }

    // Head of the end row (from 0 to end.col).
    join_row(&mut out, snapshot, end_line.saturating_sub(1));
    let end_clamped_down = end.line > last;
    let end_col = if end_clamped_down {
        usize::MAX
    } else {
        end.col
    };
    out.push_str(&terminal_row_text(snapshot, end_line, 0, end_col));

    out
}

/// Append a separator between `prev_row` and `prev_row+1`: nothing if the
/// previous row soft-wraps, `\n` otherwise.
fn join_row(out: &mut String, snapshot: &TerminalSnapshot, prev_row: usize) {
    if !row_soft_wraps(snapshot, prev_row) {
        out.push('\n');
    }
}

/// Whether `row` soft-wraps into `row+1`.
fn row_soft_wraps(snapshot: &TerminalSnapshot, row: usize) -> bool {
    // wraps is empty (default) => no soft wraps. Otherwise check the flag.
    snapshot.wraps.get(row).copied().unwrap_or(false)
}

/// Extract text from a single terminal row between `start_col` (inclusive) and
/// `end_col` (exclusive, `usize::MAX` means to end of row), skipping
/// `wide_spacer` cells.
fn terminal_row_text(
    snapshot: &TerminalSnapshot,
    row: usize,
    start_col: usize,
    end_col: usize,
) -> String {
    let Some(cells) = snapshot.cells.get(row) else {
        return String::new();
    };
    let cols = snapshot.cols.min(cells.len());
    let s = start_col.min(cols);
    let e = if end_col == usize::MAX {
        cols
    } else {
        end_col.min(cols)
    };
    if s >= e {
        return String::new();
    }
    cells[s..e]
        .iter()
        .filter(|cell| !cell.wide_spacer)
        .map(|cell| cell.ch)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selection::SelectablePane;
    use iocraft::Color;

    fn style() -> crate::runtime::TerminalCellStyle {
        crate::runtime::TerminalCellStyle {
            fg: Color::White,
            bg: Color::Black,
            bold: false,
            dim: false,
            underline: false,
        }
    }

    fn cell(ch: char) -> crate::runtime::TerminalCell {
        crate::runtime::TerminalCell {
            ch,
            style: style(),
            wide_spacer: false,
        }
    }

    fn spacer() -> crate::runtime::TerminalCell {
        crate::runtime::TerminalCell {
            ch: ' ',
            style: style(),
            wide_spacer: true,
        }
    }

    fn point(line: usize, col: usize) -> crate::selection::SelectionPoint {
        crate::selection::SelectionPoint::new(SelectablePane::TerminalView, line, col)
    }

    fn selection(line0: usize, col0: usize, line1: usize, col1: usize) -> TextSelection {
        TextSelection {
            anchor: point(line0, col0),
            focus: point(line1, col1),
        }
    }

    // ── Finding C: soft-wrap handling ──────────────────────────────────────

    #[test]
    fn selection_entirely_within_a_wrapped_row() {
        // Row 0 wraps into row 1; a single-row selection on row 1 returns its text.
        let snap = TerminalSnapshot {
            rows: 2,
            cols: 5,
            cells: vec![
                vec![cell('A'), cell('B'), cell('C'), cell('D'), cell('E')],
                vec![cell('F'), cell('G'), cell('H'), cell(' '), cell(' ')],
            ],
            wraps: vec![true, false],
        };
        let text = terminal_selection_text(&snap, &selection(1, 0, 1, 3));
        assert_eq!(text, "FGH");
    }

    #[test]
    fn selection_crossing_a_soft_wrap_inserts_no_newline() {
        // Row 0 wraps to row 1; selecting across rows 0..1 yields contiguous text
        // with NO `\n` at the row boundary.
        let snap = TerminalSnapshot {
            rows: 2,
            cols: 5,
            cells: vec![
                vec![cell('A'), cell('B'), cell('C'), cell('D'), cell('E')],
                vec![cell('F'), cell('G'), cell('H'), cell(' '), cell(' ')],
            ],
            wraps: vec![true, false],
        };
        let text = terminal_selection_text(&snap, &selection(0, 2, 1, 2));
        assert_eq!(text, "CDEFG");
    }

    #[test]
    fn selection_crossing_a_hard_newline_inserts_newline() {
        // No wrap between rows; selection across rows includes `\n`.
        let snap = TerminalSnapshot {
            rows: 2,
            cols: 5,
            cells: vec![
                vec![cell('A'), cell('B'), cell('C'), cell('D'), cell('E')],
                vec![cell('F'), cell('G'), cell('H'), cell(' '), cell(' ')],
            ],
            wraps: vec![false, false],
        };
        let text = terminal_selection_text(&snap, &selection(0, 2, 1, 2));
        assert_eq!(text, "CDE\nFG");
    }

    #[test]
    fn reversed_selection_across_a_wrap_and_a_newline() {
        // Row 0 wraps to row 1 (no \n); row 1 is a hard break to row 2 (\n).
        let snap = TerminalSnapshot {
            rows: 3,
            cols: 5,
            cells: vec![
                vec![cell('A'), cell('B'), cell('C'), cell('D'), cell('E')],
                vec![cell('F'), cell('G'), cell('H'), cell(' '), cell(' ')],
                vec![cell('X'), cell('Y'), cell('Z'), cell(' '), cell(' ')],
            ],
            wraps: vec![true, false, false],
        };
        // Reversed anchor/focus (focus earlier than anchor).
        // start=(0,2) "CDE", wrap→"FGH  ", hard break, end=(2,2) "XY".
        let text = terminal_selection_text(&snap, &selection(2, 2, 0, 2));
        assert_eq!(text, "CDEFGH  \nXY");
    }

    // ── Finding D: wide-char spacer handling ───────────────────────────────

    #[test]
    fn wide_char_selection_covering_both_cells_copies_just_glyph() {
        // '中' (width-2) at col 0, spacer at col 1, then '!' at col 2.
        let snap = TerminalSnapshot {
            rows: 1,
            cols: 3,
            cells: vec![vec![cell('中'), spacer(), cell('!')]],
            wraps: vec![false],
        };
        // Select cols 0..3 (both glyph cells + '!').
        let text = terminal_selection_text(&snap, &selection(0, 0, 0, 3));
        assert_eq!(text, "中!");
    }

    #[test]
    fn wide_char_selection_ending_on_spacer_copies_up_to_glyph() {
        let snap = TerminalSnapshot {
            rows: 1,
            cols: 3,
            cells: vec![vec![cell('中'), spacer(), cell('!')]],
            wraps: vec![false],
        };
        // Select cols 0..2 — the spacer is skipped.
        let text = terminal_selection_text(&snap, &selection(0, 0, 0, 2));
        assert_eq!(text, "中");
    }

    #[test]
    fn wide_char_selection_starting_on_spacer_starts_after_glyph() {
        let snap = TerminalSnapshot {
            rows: 1,
            cols: 3,
            cells: vec![vec![cell('中'), spacer(), cell('!')]],
            wraps: vec![false],
        };
        // Select cols 1..3 — the spacer is skipped, so only '!' is copied.
        let text = terminal_selection_text(&snap, &selection(0, 1, 0, 3));
        assert_eq!(text, "!");
    }

    // ── Empty wraps (default) behaves like hard newlines everywhere ────────

    #[test]
    fn empty_wraps_treats_all_rows_as_hard_breaks() {
        // No wraps metadata => every row boundary is a `\n`.
        let snap = TerminalSnapshot {
            rows: 2,
            cols: 3,
            cells: vec![
                vec![cell('a'), cell('b'), cell('c')],
                vec![cell('d'), cell('e'), cell('f')],
            ],
            wraps: Vec::new(),
        };
        let text = terminal_selection_text(&snap, &selection(0, 0, 1, 3));
        assert_eq!(text, "abc\ndef");
    }
}
