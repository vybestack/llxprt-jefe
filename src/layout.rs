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

// -----------------------------------------------------------------------------
// Issues-mode detail-pane layout
// -----------------------------------------------------------------------------
//
// The following constants and helpers describe the fixed vertical structure of
// the issue detail pane. They are shared between the rendering component
// (`ui::components::issue_detail`) and the scroll-limit logic
// (`state::types::IssuesState`) so that both agree on how much chrome the pane
// consumes. Keeping them here avoids a dependency from the state layer into the
// UI layer.

/// Number of fixed rows the detail metadata header occupies.
///
/// The header renders exactly five rows:
/// 1. title (`#<number> <title>`),
/// 2. state/author/timestamps,
/// 3. labels/assignees/milestone,
/// 4. external URL,
/// 5. a horizontal rule separator.
pub const DETAIL_HEADER_ROWS: usize = 5;

/// Fixed width of the repository sidebar in Issues mode.
pub const ISSUES_SIDEBAR_WIDTH: u16 = LEFT_COL_WIDTH;

/// Horizontal chrome consumed by the issue list pane border.
pub const ISSUE_LIST_CHROME_COLS: u16 = 2;

/// Rows consumed by the filter controls band when it is visible.
///
/// The component renders a bordered box with two fixed content rows.
pub const FILTER_CONTROLS_ROWS: usize = 4;

/// Fixed rows outside the Issues workspace split.
///
/// Accounts for the status bar and keybind bar.
pub const DETAIL_CHROME_ROWS: usize = OUTER_BARS_HEIGHT as usize;

/// Minimum number of rows the detail scroll viewport will reserve.
///
/// Keeps the viewport usable on very small terminals instead of collapsing to
/// zero rows.
pub const DETAIL_MIN_VIEWPORT_ROWS: usize = 5;

/// Compute the rows allocated to the Issues-mode list and detail panes.
///
/// Subtracts fixed outer bars and conditional bands, then gives 30% of the
/// remaining workspace to the list and the rest to detail. The UI uses these
/// exact row counts for the pane boxes, so the detail viewport and scroll limits
/// are derived from the same allocation the renderer receives.
#[must_use]
pub fn issues_pane_rows(
    term_rows: usize,
    error_visible: bool,
    filter_controls_open: bool,
) -> (usize, usize) {
    let mut workspace_rows = term_rows.saturating_sub(DETAIL_CHROME_ROWS);
    if error_visible {
        workspace_rows = workspace_rows.saturating_sub(1);
    }
    if filter_controls_open {
        workspace_rows = workspace_rows.saturating_sub(FILTER_CONTROLS_ROWS);
    }
    let list_rows = workspace_rows * 3 / 10;
    let detail_rows = workspace_rows.saturating_sub(list_rows);
    (list_rows, detail_rows)
}

/// Compute the rows allocated to the Issues-mode detail pane.
#[must_use]
pub fn issues_detail_pane_rows(
    term_rows: usize,
    error_visible: bool,
    filter_controls_open: bool,
) -> usize {
    issues_pane_rows(term_rows, error_visible, filter_controls_open).1
}

/// Compute the number of rows available for the detail scroll viewport given
/// the total terminal height and conditional Issues-mode bands.
#[must_use]
pub fn issues_detail_viewport_rows(
    term_rows: usize,
    error_visible: bool,
    filter_controls_open: bool,
) -> usize {
    issues_detail_pane_rows(term_rows, error_visible, filter_controls_open)
        .saturating_sub(DETAIL_HEADER_ROWS + 2)
        .max(DETAIL_MIN_VIEWPORT_ROWS)
}

/// Compute the default detail viewport rows when no conditional bands are open.
#[must_use]
pub fn detail_viewport_rows(term_rows: usize) -> usize {
    issues_detail_viewport_rows(term_rows, false, false)
}

/// Compute inner content width for issue-list title lines.
#[must_use]
pub fn issue_list_content_width(term_cols: u16) -> u16 {
    term_cols.saturating_sub(ISSUES_SIDEBAR_WIDTH + ISSUE_LIST_CHROME_COLS)
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

    #[test]
    fn detail_viewport_never_drops_below_minimum() {
        assert_eq!(
            detail_viewport_rows(0),
            DETAIL_MIN_VIEWPORT_ROWS,
            "zero-height terminal should still reserve the minimum viewport"
        );
        assert_eq!(
            detail_viewport_rows(1),
            DETAIL_MIN_VIEWPORT_ROWS,
            "one-row terminal should still reserve the minimum viewport"
        );
    }

    #[test]
    fn detail_viewport_grows_with_terminal_height() {
        let small = detail_viewport_rows(24);
        let large = detail_viewport_rows(80);
        assert!(
            large > small,
            "larger terminal should yield more viewport rows ({large} > {small})"
        );
        assert!(large > DETAIL_MIN_VIEWPORT_ROWS);
    }

    #[test]
    fn detail_viewport_for_typical_height_matches_expected_formula() {
        // term_rows=40: workspace=38, list=11, detail_pane=27, viewport=27-(5+2)=20
        assert_eq!(detail_viewport_rows(40), 20);
    }

    #[test]
    fn issues_pane_rows_account_for_dynamic_bands() {
        assert_eq!(issues_pane_rows(40, false, false), (11, 27));
        assert_eq!(issues_pane_rows(40, true, false), (11, 26));
        assert_eq!(issues_pane_rows(40, false, true), (10, 24));
        assert_eq!(issues_pane_rows(40, true, true), (9, 24));
    }

    #[test]
    fn issues_detail_pane_rows_match_shared_pane_allocation() {
        for (rows, error_visible, filter_open) in [
            (40, false, false),
            (40, true, false),
            (40, false, true),
            (40, true, true),
            (8, true, true),
        ] {
            let (_, detail_rows) = issues_pane_rows(rows, error_visible, filter_open);
            assert_eq!(
                issues_detail_pane_rows(rows, error_visible, filter_open),
                detail_rows
            );
        }
    }

    #[test]
    fn issues_detail_viewport_rows_account_for_dynamic_bands() {
        assert_eq!(issues_detail_viewport_rows(40, false, false), 20);
        assert_eq!(issues_detail_viewport_rows(40, true, false), 19);
        assert_eq!(issues_detail_viewport_rows(40, false, true), 17);
        assert_eq!(issues_detail_viewport_rows(40, true, true), 17);
    }

    #[test]
    fn issue_list_content_width_excludes_sidebar_and_border() {
        assert_eq!(issue_list_content_width(120), 96);
        assert_eq!(issue_list_content_width(10), 0);
    }
}
