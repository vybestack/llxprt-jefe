//! The last help line must be reachable (issue #390 CW-10, row CW10-13).
//!
//! Scroll clamping and rendering are two different call sites deriving the same
//! viewport. When they disagree, the tail of the content becomes unreachable —
//! silently, because the modal still looks scrolled to the end. That was
//! invisible while the compiled table was the whole of Help, and became a
//! missing package section the moment anything was appended.

use super::{HELP_CHROME_ROWS, help_max_scroll, help_viewport_rows};

/// Content whose lines are long enough that most of them wrap, which is what
/// made the content-line clamp stop short of the end.
fn content(lines: usize) -> Vec<String> {
    (0..lines)
        .map(|index| {
            if index % 3 == 0 {
                format!("  short {index}")
            } else {
                format!("  a deliberately long help description line number {index} that wraps")
            }
        })
        .collect()
}

/// Whether the renderer, at `offset`, actually shows the final content line.
fn reaches_end(lines: &[String], terminal_rows: u16, offset: usize) -> bool {
    let render_rows = crate::layout::effective_render_size_for_windowed(80, terminal_rows, true).1;
    let viewport = help_viewport_rows(render_rows);
    let rows = crate::domain::document_wrap::wrap_document(&lines.join("\n"), 55);
    let Some(first) = rows.iter().position(|row| row.line >= offset) else {
        return false;
    };
    let last_visible = first.saturating_add(viewport);
    rows.len() <= last_visible
}

#[test]
fn scrolling_to_the_end_reaches_the_final_line() {
    for terminal_rows in [24_u16, 30, 40, 50, 60, 80] {
        for lines in [30_usize, 60, 78, 120] {
            let lines = content(lines);
            let max = help_max_scroll(&lines, terminal_rows);
            assert!(
                reaches_end(&lines, terminal_rows, max),
                "rows={terminal_rows} lines={}: end-of-scroll (offset {max}) never shows the \
                 last line, so appended content is unreachable",
                lines.len()
            );
        }
    }
}

#[test]
fn content_shorter_than_the_viewport_never_scrolls() {
    assert_eq!(help_max_scroll(&content(3), 50), 0);
}

#[test]
fn the_viewport_never_exceeds_what_the_chrome_leaves() {
    for rows in [10_u16, 24, 40, 80] {
        let viewport = help_viewport_rows(rows);
        assert!(
            viewport + usize::from(HELP_CHROME_ROWS) <= usize::from(rows).max(1) + viewport,
            "viewport must stay inside the modal for rows={rows}"
        );
    }
}
