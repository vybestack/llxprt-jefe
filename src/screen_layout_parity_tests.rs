//! Parity between the resolver and the geometry the screens render today
//! (issue #384, CW04-02 and CW04-08).
//!
//! These are the tests that make the consumer cutover safe: they compare the
//! descriptor's output against the hand-maintained mirror arithmetic the PTY,
//! mouse, and selection paths still use. While both exist they must agree; once
//! the consumers read the snapshot, the mirror arithmetic goes away and these
//! become the record of what it used to produce.

use crate::layout::{compute_pty_layout_for_windowed, dashboard_middle_row_heights_inner};
use crate::screen_layout::resolve_screen;
use crate::state::AppState;
use crate::workbench::{PanelId, ScreenId};

fn dashboard() -> AppState {
    AppState {
        nav: crate::state::navigation::NavState::rooted(ScreenId::Dashboard),
        ..AppState::default()
    }
}

/// Resolve the dashboard at a raw terminal size.
fn resolve(cols: u16, rows: u16) -> crate::workbench::ResolvedLayout {
    resolve_screen(&dashboard(), cols, rows)
        .unwrap_or_else(|| unreachable!("the shipped registry always resolves"))
}

/// Rows at which the shipped helper's own clamp is inactive, so its result is
/// the plain quarter/three-quarter proportion the descriptor declares.
fn unclamped_split(render_rows: u16) -> Option<(u16, u16)> {
    let content = render_rows.saturating_sub(crate::layout::OUTER_BARS_HEIGHT);
    if content <= crate::layout::AGENT_PANE_MIN_ROWS + crate::layout::TERMINAL_PANE_MIN_ROWS {
        return None;
    }
    // Widened so the helper stays correct if it is ever reused with a larger
    // row count; `content * 25` overflows a u16 above 2621.
    let preferred = u16::try_from((u32::from(content) * 25 + 50) / 100).unwrap_or(u16::MAX);
    let clamped = preferred.clamp(
        crate::layout::AGENT_PANE_MIN_ROWS,
        content - crate::layout::TERMINAL_PANE_MIN_ROWS,
    );
    (preferred == clamped).then(|| dashboard_middle_row_heights_inner(render_rows))
}

#[test]
fn the_agent_and_terminal_rows_match_the_shipped_split() {
    let mut compared = 0_u32;
    for rows in 12_u16..=80 {
        let (_, render_rows) = crate::layout::effective_render_size(120, rows);
        let Some((expected_agent, expected_terminal)) = unclamped_split(render_rows) else {
            continue;
        };
        // Below four rows the agent pane is all border and title with no
        // content line. The resolver hides it rather than rendering an empty
        // husk; that difference is asserted separately.
        if expected_agent < 4 {
            continue;
        }
        let layout = resolve(120, rows);
        let agent = layout
            .panel(&PanelId::from_static("agents"))
            .filter(|panel| panel.visible)
            .map(|panel| panel.chrome.height);
        let terminal = layout
            .panel(&PanelId::from_static("terminal"))
            .filter(|panel| panel.visible)
            .map(|panel| panel.chrome.height);
        assert_eq!(
            (agent, terminal),
            (Some(expected_agent), Some(expected_terminal)),
            "dashboard split diverged at 120x{rows}"
        );
        compared += 1;
    }
    assert!(compared > 50, "expected a broad sweep, compared {compared}");
}

#[test]
fn a_pane_with_no_content_row_is_hidden_rather_than_rendered_as_an_empty_husk() {
    // At 120x12 the agent pane is allocated three rows, which its border and
    // title consume entirely. The shipped layout draws that empty box; the
    // resolver hides the pane, so the rows go to a pane that can use them and
    // no consumer is handed a zero-height content rectangle.
    let layout = resolve(120, 12);
    assert_eq!(
        layout
            .panel(&PanelId::from_static("agents"))
            .map(|panel| panel.visible),
        Some(false)
    );
    let Some(terminal) = layout.panel(&PanelId::from_static("terminal")) else {
        unreachable!("the dashboard always declares a terminal panel");
    };
    assert!(terminal.visible && terminal.content.height >= 1);
}

#[test]
fn a_terminal_too_narrow_for_a_usable_viewport_reports_too_small() {
    // The shipped helper fabricates a two-column viewport with `.max(2)` when
    // the middle column has nothing left to give. The resolver refuses: a
    // required pane that cannot hold content means the screen does not fit, so
    // the user gets the too-small notice instead of a terminal that cannot
    // display anything. This is the guard the snapshot replaces.
    let layout = resolve(60, 40);
    assert!(
        layout.too_small.is_some(),
        "a 60-column dashboard leaves the terminal no content columns"
    );
    assert_eq!(layout.visible_panels().count(), 1);
}

#[test]
fn the_terminal_content_rect_matches_the_shipped_pty_viewport() {
    for cols in [80_u16, 100, 120, 160, 200] {
        for rows in [24_u16, 30, 40, 50, 60] {
            let layout = resolve(cols, rows);
            let Some(terminal) = layout
                .panel(&PanelId::from_static("terminal"))
                .filter(|panel| panel.visible)
            else {
                continue;
            };
            let expected = compute_pty_layout_for_windowed(cols, rows, false);
            assert_eq!(
                (terminal.content.width, terminal.content.height),
                (expected.pty_cols, expected.pty_rows),
                "pty viewport diverged at {cols}x{rows}"
            );
            assert_eq!(
                (terminal.content.col, terminal.content.row),
                (expected.pane_col0, expected.pane_row0),
                "pty origin diverged at {cols}x{rows}"
            );
        }
    }
}

#[test]
fn the_repository_sidebar_keeps_its_shipped_width() {
    for cols in [80_u16, 100, 120, 200] {
        let layout = resolve(cols, 40);
        let Some(sidebar) = layout
            .panel(&PanelId::from_static("repositories"))
            .filter(|panel| panel.visible)
        else {
            continue;
        };
        assert_eq!(
            sidebar.chrome.width,
            crate::layout::LEFT_COL_WIDTH,
            "sidebar width diverged at {cols} columns"
        );
        assert_eq!(sidebar.chrome.col, 0);
    }
}

#[test]
fn the_preview_pane_keeps_its_shipped_width_and_right_edge() {
    for cols in [80_u16, 120, 200] {
        let layout = resolve(cols, 40);
        let Some(preview) = layout
            .panel(&PanelId::from_static("preview"))
            .filter(|panel| panel.visible)
        else {
            continue;
        };
        let (render_cols, _) = crate::layout::effective_render_size(cols, 40);
        assert_eq!(preview.chrome.width, crate::layout::RIGHT_COL_WIDTH);
        assert_eq!(
            preview.chrome.right(),
            u32::from(render_cols),
            "preview must sit against the right edge at {cols} columns"
        );
    }
}

#[test]
fn the_three_dashboard_columns_tile_the_width_exactly() {
    for cols in 60_u16..=200 {
        let layout = resolve(cols, 40);
        let widths: Vec<u16> = ["repositories", "agents", "preview"]
            .into_iter()
            .filter_map(|id| {
                layout
                    .panel(&PanelId::from_static(id))
                    .filter(|panel| panel.visible)
                    .map(|panel| panel.chrome.width)
            })
            .collect();
        if widths.len() != 3 {
            continue;
        }
        let (render_cols, _) = crate::layout::effective_render_size(cols, 40);
        let total: u32 = widths.iter().map(|width| u32::from(*width)).sum();
        assert_eq!(
            total,
            u32::from(render_cols),
            "columns must tile the width exactly at {cols}"
        );
    }
}
