//! Layout constants and coordinate calculation functions.
//!
//! Extracted from main.rs to isolate geometry logic and enable focused unit testing.

/// Left column width (repository list).
pub const LEFT_COL_WIDTH: u16 = 22;
/// Right column width (preview pane).
pub const RIGHT_COL_WIDTH: u16 = 36;
/// Height consumed by status bar + keybind bar.
pub const OUTER_BARS_HEIGHT: u16 = 2;
/// Terminal widget chrome rows: top border + header row + bottom border.
pub const TERMINAL_WIDGET_CHROME_ROWS: u16 = 3;
/// Terminal widget chrome columns: left + right border.
pub const TERMINAL_WIDGET_CHROME_COLS: u16 = 2;
/// Minimum rows reserved for the agent pane so it keeps its chrome and at least one content row.
pub const AGENT_PANE_MIN_ROWS: u16 = 3;
/// Minimum rows reserved for the terminal pane so it keeps its chrome and a usable viewport.
pub const TERMINAL_PANE_MIN_ROWS: u16 = TERMINAL_WIDGET_CHROME_ROWS + 2;

/// Check if fullscreen mode is enabled.
#[must_use]
pub fn is_fullscreen_enabled() -> bool {
    std::env::var("JEFE_WINDOWED").ok().as_deref() != Some("1")
}

/// Calculate effective render dimensions for a given fullscreen flag.
#[must_use]
fn effective_render_size_inner(cols: u16, rows: u16, fullscreen: bool) -> (u16, u16) {
    if fullscreen {
        (cols, rows)
    } else {
        (cols.saturating_sub(2).max(1), rows.saturating_sub(2).max(1))
    }
}

fn dashboard_middle_row_heights_inner(render_rows: u16) -> (u16, u16) {
    let content_rows = render_rows.saturating_sub(OUTER_BARS_HEIGHT);

    if content_rows <= AGENT_PANE_MIN_ROWS + TERMINAL_PANE_MIN_ROWS {
        let terminal_rows = content_rows.saturating_sub(AGENT_PANE_MIN_ROWS).max(1);
        let agent_rows = content_rows.saturating_sub(terminal_rows);
        return (agent_rows, terminal_rows);
    }

    // Round to nearest (half-up) to match taffy's cumulative rounding of
    // percentage-based flex children. Simple truncation (`*25/100`) under-
    // counts by 1 row whenever the product has a fractional part ≥ 0.5.
    let preferred_agent_rows = content_rows
        .saturating_mul(25)
        .saturating_add(50)
        .saturating_div(100);
    let max_agent_rows = content_rows.saturating_sub(TERMINAL_PANE_MIN_ROWS);
    let agent_rows = preferred_agent_rows
        .clamp(AGENT_PANE_MIN_ROWS, max_agent_rows)
        .min(content_rows);
    let terminal_rows = content_rows.saturating_sub(agent_rows).max(1);

    (agent_rows, terminal_rows)
}

/// Calculate effective render dimensions.
#[must_use]
pub fn effective_render_size(cols: u16, rows: u16) -> (u16, u16) {
    effective_render_size_inner(cols, rows, is_fullscreen_enabled())
}

/// Compute adaptive middle-column row heights for the dashboard layout.
#[must_use]
pub fn dashboard_middle_row_heights(term_cols: u16, term_rows: u16) -> (u16, u16) {
    let (_, render_rows) = effective_render_size(term_cols, term_rows);
    dashboard_middle_row_heights_inner(render_rows)
}

/// Compute PTY viewport size and its origin for a given fullscreen flag.
#[must_use]
fn compute_pty_layout_inner(
    term_cols: u16,
    term_rows: u16,
    fullscreen: bool,
) -> (u16, u16, u16, u16) {
    let (render_cols, render_rows) = effective_render_size_inner(term_cols, term_rows, fullscreen);

    let (agent_rows, terminal_slot_rows) = dashboard_middle_row_heights_inner(render_rows);
    let middle_cols = render_cols.saturating_sub(LEFT_COL_WIDTH + RIGHT_COL_WIDTH);

    let pty_rows = terminal_slot_rows
        .saturating_sub(TERMINAL_WIDGET_CHROME_ROWS)
        .max(2);
    let pty_cols = middle_cols
        .saturating_sub(TERMINAL_WIDGET_CHROME_COLS)
        .max(2);

    let pane_col0 = LEFT_COL_WIDTH.saturating_add(1);
    let pane_row0 = 1u16.saturating_add(agent_rows).saturating_add(2);

    (pty_rows, pty_cols, pane_col0, pane_row0)
}

/// Compute PTY viewport size and its origin within the fullscreen render grid.
///
/// Layout mirrors dashboard proportions:
/// - top status bar (1 row)
/// - bottom keybind bar (1 row)
/// - middle column split: agent list prefers 25% and terminal gets the rest
/// - under tight heights, the terminal keeps enough rows for its chrome and viewport
/// - terminal widget chrome: border + header + border
#[must_use]
pub fn compute_pty_layout(term_cols: u16, term_rows: u16) -> (u16, u16, u16, u16) {
    compute_pty_layout_inner(term_cols, term_rows, is_fullscreen_enabled())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_render_size_fullscreen_passthrough() {
        assert_eq!(effective_render_size_inner(120, 40, true), (120, 40));
        assert_eq!(effective_render_size_inner(80, 24, true), (80, 24));
    }

    #[test]
    fn effective_render_size_windowed_subtraction() {
        assert_eq!(effective_render_size_inner(120, 40, false), (118, 38));
        assert_eq!(effective_render_size_inner(2, 2, false), (1, 1));
        assert_eq!(effective_render_size_inner(1, 1, false), (1, 1));
    }

    #[test]
    fn compute_pty_layout_pane_origin() {
        let (_, _, pane_col0, _) = compute_pty_layout_inner(120, 40, true);
        assert_eq!(pane_col0, LEFT_COL_WIDTH + 1);
    }

    #[test]
    fn dashboard_middle_row_heights_preserve_default_split_when_space_allows() {
        assert_eq!(dashboard_middle_row_heights_inner(40), (10, 28));
    }

    #[test]
    fn dashboard_middle_row_heights_protect_terminal_space_when_rows_are_tight() {
        assert_eq!(dashboard_middle_row_heights_inner(10), (3, 5));
    }

    #[test]
    fn dashboard_middle_row_heights_degrade_gracefully_when_extremely_small() {
        assert_eq!(dashboard_middle_row_heights_inner(4), (1, 1));
        assert_eq!(dashboard_middle_row_heights_inner(3), (0, 1));
    }

    #[test]
    fn compute_pty_layout_dimensions_always_at_least_two() {
        for fullscreen in [true, false] {
            for (cols, rows) in [(120, 40), (10, 10), (0, 0), (60, 20)] {
                let (pty_rows, pty_cols, _, _) = compute_pty_layout_inner(cols, rows, fullscreen);
                assert!(
                    pty_rows >= 2,
                    "pty_rows < 2 for ({cols}, {rows}, fullscreen={fullscreen})"
                );
                assert!(
                    pty_cols >= 2,
                    "pty_cols < 2 for ({cols}, {rows}, fullscreen={fullscreen})"
                );
            }
        }
    }

    #[test]
    fn agent_rows_rounding_half_up_fullscreen() {
        // 40 rows - 2 bars = 38 content rows. 25% = 9.5 → rounds to 10.
        let (_, _, _, pane_row0) = compute_pty_layout_inner(120, 40, true);
        // pane_row0 = 1 (status bar) + agent_rows(10) + 2 (chrome top border + header)
        assert_eq!(pane_row0, 1 + 10 + 2);
    }

    #[test]
    fn agent_rows_rounding_half_up_windowed() {
        // Windowed: 40-2=38 render rows, 38-2=36 content rows. 25% = 9.0 → exactly 9.
        let (_, _, _, pane_row0) = compute_pty_layout_inner(120, 40, false);
        assert_eq!(pane_row0, 1 + 9 + 2);
    }

    #[test]
    fn compute_pty_layout_pane_row0_positive() {
        for fullscreen in [true, false] {
            let (_, _, _, pane_row0) = compute_pty_layout_inner(120, 40, fullscreen);
            assert!(
                pane_row0 > 0,
                "pane_row0 not positive for fullscreen={fullscreen}"
            );
        }
    }
}
