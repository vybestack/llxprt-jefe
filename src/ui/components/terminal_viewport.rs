//! Pure viewport projection for the terminal scrollback (issue #198).
//!
//! This module is intentionally **iocraft-free** — no `use iocraft::prelude::*`,
//! no `Color`, no `Props`. The [`build_terminal_viewport`] function is a pure
//! `#[must_use]` projection that windows history+live rows for a given offset
//! and viewport size, returning a [`TerminalSnapshot`]-like projection plus an
//! optional follow indicator. Unit-test it directly.

use crate::runtime::{TerminalCell, TerminalCellStyle, TerminalSnapshot};
use crate::state::FollowIndicator;

/// The result of the viewport projection: the windowed snapshot to paint and
/// an optional follow indicator descriptor.
#[derive(Debug, Clone)]
pub struct TerminalViewportProjection {
    /// The windowed snapshot rows to render (history rows above live rows,
    /// windowed by the offset).
    pub snapshot: TerminalSnapshot,
    /// Follow indicator when scrolled back; `None` when following (live).
    pub indicator: Option<FollowIndicator>,
    /// The absolute content-line index that the first viewport row (row 0)
    /// corresponds to (issue #198 review fix #5). Selection highlight math must
    /// add this to the viewport-local row index to get the absolute content row
    /// that `row_highlight_range` expects.
    pub start_line: usize,
}

/// Build the terminal viewport projection from a live snapshot and retained
/// history lines.
///
/// - When `offset` is `None` (follow-tail), the projection is the bottom
///   `viewport_rows` of history+live (the live follow view).
/// - When `offset` is `Some(n)`, the projection is the `viewport_rows` window
///   starting `n` lines above the bottom of history+live.
///
/// History rows are plain text (no styles); they use `default_style`. Live
/// rows keep their original styles.
///
/// This function is pure (no I/O, no iocraft) and `#[must_use]` so it can be
/// unit-tested directly (issue #198).
#[must_use]
pub fn build_terminal_viewport(
    live_snapshot: &TerminalSnapshot,
    history_lines: &[String],
    offset: Option<usize>,
    viewport_rows: usize,
    viewport_cols: usize,
    default_style: TerminalCellStyle,
) -> TerminalViewportProjection {
    let indicator = crate::state::terminal_follow_indicator(offset);

    // Compute the top-relative content start line once (single source of truth)
    // so the viewport projection and the reported `start_line` can never drift
    // apart (OCR finding: the value was previously computed twice).
    let total_lines = history_lines.len() + live_snapshot.rows;
    let start_line = crate::state::scrollback_ops::terminal_content_start_line(
        offset,
        total_lines,
        viewport_rows,
    );

    let snapshot = if viewport_rows == 0 || viewport_cols == 0 {
        TerminalSnapshot::default()
    } else {
        build_windowed_snapshot(
            live_snapshot,
            history_lines,
            start_line,
            viewport_rows,
            viewport_cols,
            default_style,
        )
    };

    TerminalViewportProjection {
        snapshot,
        indicator,
        start_line,
    }
}

/// Build the windowed snapshot from history + live rows.
///
/// `start_line` is the already-computed top-relative content start line
/// (shared with the projection's reported `start_line`) so there is a single
/// source of truth for the windowing offset.
fn build_windowed_snapshot(
    live_snapshot: &TerminalSnapshot,
    history_lines: &[String],
    start_line: usize,
    viewport_rows: usize,
    viewport_cols: usize,
    default_style: TerminalCellStyle,
) -> TerminalSnapshot {
    // Compose the full content as a flat list of "rows" where each row is
    // either a history string or a live snapshot row index.
    let live_rows = live_snapshot.rows;
    let total_lines = history_lines.len() + live_rows;

    let mut cells: Vec<Vec<TerminalCell>> = Vec::with_capacity(viewport_rows);
    for row_idx in 0..viewport_rows {
        let content_line = start_line + row_idx;
        if content_line >= total_lines {
            // Past the end: fill with blank cells.
            cells.push(vec![blank_cell(default_style); viewport_cols]);
            continue;
        }

        let history_count = history_lines.len();
        if content_line < history_count {
            // History row: plain text with default style.
            let line = &history_lines[content_line];
            cells.push(string_to_cells(line, viewport_cols, default_style));
        } else {
            // Live row: use the styled cells from the snapshot.
            let live_row = content_line - history_count;
            if live_row < live_rows {
                let row_cells = live_snapshot.cells.get(live_row).map_or_else(
                    || vec![blank_cell(default_style); viewport_cols],
                    |row| clamp_row(row, viewport_cols, default_style),
                );
                cells.push(row_cells);
            } else {
                cells.push(vec![blank_cell(default_style); viewport_cols]);
            }
        }
    }

    TerminalSnapshot {
        rows: viewport_rows,
        cols: viewport_cols,
        cells,
        wraps: Vec::new(),
    }
}

/// Convert a plain-text string into a row of styled cells, clamped to `max_cols`.
fn string_to_cells(text: &str, max_cols: usize, style: TerminalCellStyle) -> Vec<TerminalCell> {
    let mut cells: Vec<TerminalCell> = text
        .chars()
        .take(max_cols)
        .map(|ch| TerminalCell {
            ch,
            style,
            wide_spacer: false,
        })
        .collect();
    // Pad to max_cols with blank cells.
    while cells.len() < max_cols {
        cells.push(blank_cell(style));
    }
    cells
}

/// Clamp a live snapshot row to `max_cols`, padding with blanks if shorter.
fn clamp_row(row: &[TerminalCell], max_cols: usize, style: TerminalCellStyle) -> Vec<TerminalCell> {
    let mut cells: Vec<TerminalCell> = row.iter().take(max_cols).copied().collect();
    while cells.len() < max_cols {
        cells.push(blank_cell(style));
    }
    cells
}

fn blank_cell(style: TerminalCellStyle) -> TerminalCell {
    TerminalCell {
        ch: ' ',
        style,
        wide_spacer: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iocraft::Color;

    fn default_style() -> TerminalCellStyle {
        TerminalCellStyle {
            fg: Color::White,
            bg: Color::Black,
            bold: false,
            dim: false,
            underline: false,
        }
    }

    fn make_live_snapshot(rows: &[&str]) -> TerminalSnapshot {
        let style = default_style();
        let cells: Vec<Vec<TerminalCell>> = rows
            .iter()
            .map(|row| {
                let mut line: Vec<TerminalCell> = row
                    .chars()
                    .map(|ch| TerminalCell {
                        ch,
                        style,
                        wide_spacer: false,
                    })
                    .collect();
                while line.len() < 80 {
                    line.push(TerminalCell {
                        ch: ' ',
                        style,
                        wide_spacer: false,
                    });
                }
                line
            })
            .collect();
        TerminalSnapshot {
            rows: rows.len(),
            cols: 80,
            cells,
            wraps: Vec::new(),
        }
    }

    fn row_text(snapshot: &TerminalSnapshot, row: usize) -> String {
        snapshot.cells.get(row).map_or_else(String::new, |cells| {
            cells
                .iter()
                .map(|c| c.ch)
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
    }

    #[test]
    fn offset_none_shows_bottom_viewport_follow() {
        // History: lines 1-10, Live: lines 11-12.
        // Total = 12, viewport = 3 → follow shows lines 10, 11, 12.
        let history: Vec<String> = (1..=10).map(|i| format!("history{i}")).collect();
        let live = make_live_snapshot(&["live11", "live12"]);
        let proj = build_terminal_viewport(&live, &history, None, 3, 80, default_style());
        assert!(proj.indicator.is_none());
        assert_eq!(proj.snapshot.rows, 3);
        assert_eq!(row_text(&proj.snapshot, 0), "history10");
        assert_eq!(row_text(&proj.snapshot, 1), "live11");
        assert_eq!(row_text(&proj.snapshot, 2), "live12");
    }

    #[test]
    fn offset_some_shows_scrolled_back_window() {
        // History: lines 1-10, Live: lines 11-12.
        // Total = 12, viewport = 3, offset = 5 → start at 12-3-5=4.
        // Shows lines 5, 6, 7.
        let history: Vec<String> = (1..=10).map(|i| format!("history{i}")).collect();
        let live = make_live_snapshot(&["live11", "live12"]);
        let proj = build_terminal_viewport(&live, &history, Some(5), 3, 80, default_style());
        assert!(proj.indicator.is_some());
        assert_eq!(proj.snapshot.rows, 3);
        assert_eq!(row_text(&proj.snapshot, 0), "history5");
        assert_eq!(row_text(&proj.snapshot, 1), "history6");
        assert_eq!(row_text(&proj.snapshot, 2), "history7");
    }

    #[test]
    fn offset_clamps_at_top_of_content() {
        // Total = 12, viewport = 3, offset = 100 → start = max(0, 9-100)=0.
        let history: Vec<String> = (1..=10).map(|i| format!("history{i}")).collect();
        let live = make_live_snapshot(&["live11", "live12"]);
        let proj = build_terminal_viewport(&live, &history, Some(100), 3, 80, default_style());
        assert_eq!(row_text(&proj.snapshot, 0), "history1");
        assert_eq!(row_text(&proj.snapshot, 1), "history2");
        assert_eq!(row_text(&proj.snapshot, 2), "history3");
    }

    #[test]
    fn indicator_present_iff_scrolled_back() {
        let history: Vec<String> = vec!["h1".to_owned()];
        let live = make_live_snapshot(&["l1"]);

        // None → no indicator.
        let proj = build_terminal_viewport(&live, &history, None, 1, 80, default_style());
        assert!(proj.indicator.is_none());

        // Some → indicator present.
        let proj = build_terminal_viewport(&live, &history, Some(1), 1, 80, default_style());
        assert!(proj.indicator.is_some());
    }

    #[test]
    fn no_history_shows_only_live() {
        let history: Vec<String> = vec![];
        let live = make_live_snapshot(&["row1", "row2"]);
        let proj = build_terminal_viewport(&live, &history, None, 2, 80, default_style());
        assert_eq!(proj.snapshot.rows, 2);
        assert_eq!(row_text(&proj.snapshot, 0), "row1");
        assert_eq!(row_text(&proj.snapshot, 1), "row2");
    }

    #[test]
    fn zero_viewport_returns_empty_snapshot() {
        let live = make_live_snapshot(&["r1"]);
        let proj = build_terminal_viewport(&live, &[], None, 0, 80, default_style());
        assert_eq!(proj.snapshot.rows, 0);
    }

    #[test]
    fn history_rows_use_default_style() {
        let history = vec!["styled".to_owned()];
        let live = make_live_snapshot(&["live"]);
        let proj = build_terminal_viewport(&live, &history, Some(1), 1, 80, default_style());
        // The first (and only) row is a history row.
        let cell = &proj.snapshot.cells[0][0];
        assert_eq!(cell.ch, 's');
        assert_eq!(cell.style, default_style());
    }

    #[test]
    fn live_rows_keep_their_styles() {
        let live_style = TerminalCellStyle {
            fg: Color::Red,
            bg: Color::Blue,
            bold: true,
            dim: false,
            underline: true,
        };
        let live_cell = TerminalCell {
            ch: 'X',
            style: live_style,
            wide_spacer: false,
        };
        let live = TerminalSnapshot {
            rows: 1,
            cols: 1,
            cells: vec![vec![live_cell]],
            wraps: Vec::new(),
        };
        // No history, follow view.
        let proj = build_terminal_viewport(&live, &[], None, 1, 1, default_style());
        assert_eq!(proj.snapshot.cells[0][0].style, live_style);
    }

    // ── content_start_line (issue #198 review fix #5) ─────────────────────

    #[test]
    fn start_line_is_zero_for_follow_tail() {
        let history: Vec<String> = (1..=10).map(|i| format!("h{i}")).collect();
        let live = make_live_snapshot(&["l1", "l2"]);
        let proj = build_terminal_viewport(&live, &history, None, 3, 80, default_style());
        // total=12, viewport=3, offset=None → start = max(12-3,0) - 0 = 9
        assert_eq!(proj.start_line, 9);
    }

    #[test]
    fn start_line_nonzero_when_scrolled_back() {
        let history: Vec<String> = (1..=10).map(|i| format!("history{i}")).collect();
        let live = make_live_snapshot(&["l1", "l2"]);
        // total=12, viewport=3, offset=Some(5) → max=9, start=9-5=4
        let proj = build_terminal_viewport(&live, &history, Some(5), 3, 80, default_style());
        assert_eq!(proj.start_line, 4);
        // Row 0 of the viewport must correspond to content line 4.
        assert_eq!(row_text(&proj.snapshot, 0), "history5");
    }

    #[test]
    fn start_line_used_for_selection_highlight_matches_content() {
        // Behavioral test (review fix #5): when scrolled back so that the
        // viewport starts at a nonzero content line, the start_line must be
        // added to the viewport-local row index to get the absolute content row
        // that row_highlight_range expects.
        use crate::selection::{SelectablePane, SelectionPoint, TextSelection};

        let history: Vec<String> = (1..=10).map(|i| format!("h{i}")).collect();
        let live = make_live_snapshot(&["l1", "l2"]);
        // total=12, viewport=3, offset=Some(5) → start=4.
        // Viewport rows: history5 (line 4), history6 (line 5), history7 (line 6).
        let proj = build_terminal_viewport(&live, &history, Some(5), 3, 80, default_style());
        assert_eq!(proj.start_line, 4);

        // Select from line 4 col 0 to line 5 col 3 (in absolute content coords).
        let selection = TextSelection {
            anchor: SelectionPoint::new(SelectablePane::TerminalView, 4, 0),
            focus: SelectionPoint::new(SelectablePane::TerminalView, 5, 3),
        };

        // Viewport row 0 = content line 4 → must be highlighted.
        assert!(
            crate::selection::row_highlight_range(&selection, proj.start_line).is_some(),
            "row 0 (content line 4) must be highlighted"
        );
        // Viewport row 1 = content line 5 → must be highlighted.
        assert!(
            crate::selection::row_highlight_range(&selection, proj.start_line + 1).is_some(),
            "row 1 (content line 5) must be highlighted"
        );
        // Viewport row 2 = content line 6 → must NOT be highlighted.
        assert!(
            crate::selection::row_highlight_range(&selection, proj.start_line + 2).is_none(),
            "row 2 (content line 6) must NOT be highlighted"
        );

        // WITHOUT the start_line offset, row 0 would be queried as line 0 and
        // would not match — proving the offset is required.
        assert!(
            crate::selection::row_highlight_range(&selection, 0).is_none(),
            "without content_start_line offset, row 0 queried as line 0 must NOT match"
        );
    }

    // ── Follow indicator does not consume a content row (review fix #6) ────

    #[test]
    fn indicator_present_does_not_reduce_viewport_rows() {
        // When scrolled back, the indicator is present, but the projection
        // snapshot must still have exactly `viewport_rows` rows — the indicator
        // is overlaid, not a separate flex row that reduces content height.
        let history: Vec<String> = (1..=10).map(|i| format!("h{i}")).collect();
        let live = make_live_snapshot(&["l1", "l2"]);
        let viewport_rows = 5;
        let proj =
            build_terminal_viewport(&live, &history, Some(3), viewport_rows, 80, default_style());
        // Indicator present (scrolled back).
        assert!(proj.indicator.is_some());
        // Snapshot must have the FULL viewport row count, not viewport_rows-1.
        assert_eq!(
            proj.snapshot.rows, viewport_rows,
            "follow indicator must not reduce content row count (overlay approach)"
        );
    }

    #[test]
    fn indicator_absent_at_follow_tail() {
        let history: Vec<String> = (1..=10).map(|i| format!("h{i}")).collect();
        let live = make_live_snapshot(&["l1", "l2"]);
        let proj = build_terminal_viewport(&live, &history, None, 5, 80, default_style());
        assert!(proj.indicator.is_none());
        assert_eq!(proj.snapshot.rows, 5);
    }
}
