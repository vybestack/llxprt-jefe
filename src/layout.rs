//! Layout constants and coordinate calculation functions.
//!
//! Extracted from main.rs to isolate geometry logic and enable focused unit testing.
//!
//! # Layout invariants
//!
//! The geometry helpers below guarantee the following invariants for every
//! input terminal size (including 0×0):
//!
//! - **Viewport never collapses**: `pty_rows` is always `>= 2` and `pty_cols`
//!   is always `>= 2`. Degenerate inputs are clamped so the PTY viewport stays
//!   usable.
//! - **Pane origin is strictly positive**: `pane_col0` is always
//!   `LEFT_COL_WIDTH + 1` (i.e. `> 0`) and `pane_row0` is always `> 0`.
//! - **Agent/terminal split**: The agent pane prefers 25% of the content rows
//!   (half-up rounded) but is clamped so the terminal pane always retains at
//!   least [`TERMINAL_PANE_MIN_ROWS`] rows. Under extreme tightness (very few
//!   content rows) the terminal pane degrades but never drops below 1 row.
//! - **Single source of truth**: All fixed geometry derives from the module
//!   constants (grouped as [`AppLayoutSpec::DEFAULT`]). Change a dimension in
//!   one place and every screen + PTY layout follows automatically.

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

/// Static layout specification: the fixed geometry contract shared across the
/// dashboard and issues screens.
///
/// Grouping these into one typed value documents their cohesion and makes the
/// layout contract reviewable in a single place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppLayoutSpec {
    /// Width of the left repository sidebar column.
    pub left_col_width: u16,
    /// Width of the right preview column.
    pub right_col_width: u16,
    /// Rows consumed by the outer status bar + keybind bar.
    pub outer_bars_height: u16,
    /// Rows consumed by the terminal widget chrome (top border + header + bottom border).
    pub terminal_widget_chrome_rows: u16,
    /// Columns consumed by the terminal widget chrome (left + right border).
    pub terminal_widget_chrome_cols: u16,
    /// Minimum rows reserved for the agent pane.
    pub agent_pane_min_rows: u16,
    /// Minimum rows reserved for the terminal pane.
    pub terminal_pane_min_rows: u16,
}

impl AppLayoutSpec {
    /// The canonical layout spec used by the application.
    ///
    /// Every field references the corresponding module constant so there is a
    /// single source of truth for the layout contract.
    pub const DEFAULT: Self = Self {
        left_col_width: LEFT_COL_WIDTH,
        right_col_width: RIGHT_COL_WIDTH,
        outer_bars_height: OUTER_BARS_HEIGHT,
        terminal_widget_chrome_rows: TERMINAL_WIDGET_CHROME_ROWS,
        terminal_widget_chrome_cols: TERMINAL_WIDGET_CHROME_COLS,
        agent_pane_min_rows: AGENT_PANE_MIN_ROWS,
        terminal_pane_min_rows: TERMINAL_PANE_MIN_ROWS,
    };
}

/// Computed PTY viewport geometry: terminal cell dimensions plus the
/// 1-based screen origin (column/row) of the viewport within the render grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtyLayout {
    /// PTY viewport height in rows (always `>= 2`).
    pub pty_rows: u16,
    /// PTY viewport width in columns (always `>= 2`).
    pub pty_cols: u16,
    /// 1-based screen column where the PTY viewport's left edge sits (always `> 0`).
    pub pane_col0: u16,
    /// 1-based screen row where the PTY viewport's top edge sits (always `> 0`).
    pub pane_row0: u16,
}

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
fn compute_pty_layout_inner(term_cols: u16, term_rows: u16, fullscreen: bool) -> PtyLayout {
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

    PtyLayout {
        pty_rows,
        pty_cols,
        pane_col0,
        pane_row0,
    }
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
pub fn compute_pty_layout(term_cols: u16, term_rows: u16) -> PtyLayout {
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
    fn column_width_constants_hold_expected_values() {
        // The UI screens reference these constants for their fixed-width panes,
        // so changing a value silently here would reshape the dashboard/issues
        // layout. Lock the contract.
        assert_eq!(LEFT_COL_WIDTH, 22);
        assert_eq!(RIGHT_COL_WIDTH, 36);
        assert_eq!(ISSUES_SIDEBAR_WIDTH, LEFT_COL_WIDTH);
    }

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
        let layout = compute_pty_layout_inner(120, 40, true);
        assert_eq!(layout.pane_col0, LEFT_COL_WIDTH + 1);
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
                let layout = compute_pty_layout_inner(cols, rows, fullscreen);
                assert!(
                    layout.pty_rows >= 2,
                    "pty_rows < 2 for ({cols}, {rows}, fullscreen={fullscreen})"
                );
                assert!(
                    layout.pty_cols >= 2,
                    "pty_cols < 2 for ({cols}, {rows}, fullscreen={fullscreen})"
                );
            }
        }
    }

    #[test]
    fn agent_rows_rounding_half_up_fullscreen() {
        // 40 rows - 2 bars = 38 content rows. 25% = 9.5 → rounds to 10.
        let layout = compute_pty_layout_inner(120, 40, true);
        // pane_row0 = 1 (status bar) + agent_rows(10) + 2 (chrome top border + header)
        assert_eq!(layout.pane_row0, 1 + 10 + 2);
    }

    #[test]
    fn agent_rows_rounding_half_up_windowed() {
        // Windowed: 40-2=38 render rows, 38-2=36 content rows. 25% = 9.0 → exactly 9.
        let layout = compute_pty_layout_inner(120, 40, false);
        assert_eq!(layout.pane_row0, 1 + 9 + 2);
    }

    #[test]
    fn compute_pty_layout_pane_row0_positive() {
        for fullscreen in [true, false] {
            let layout = compute_pty_layout_inner(120, 40, fullscreen);
            assert!(
                layout.pane_row0 > 0,
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

    // -------------------------------------------------------------------------
    // AppLayoutSpec: single source of truth for the layout contract.
    // -------------------------------------------------------------------------

    #[test]
    fn app_layout_spec_default_matches_module_constants() {
        let spec = AppLayoutSpec::DEFAULT;
        assert_eq!(spec.left_col_width, LEFT_COL_WIDTH);
        assert_eq!(spec.right_col_width, RIGHT_COL_WIDTH);
        assert_eq!(spec.outer_bars_height, OUTER_BARS_HEIGHT);
        assert_eq!(
            spec.terminal_widget_chrome_rows,
            TERMINAL_WIDGET_CHROME_ROWS
        );
        assert_eq!(
            spec.terminal_widget_chrome_cols,
            TERMINAL_WIDGET_CHROME_COLS
        );
        assert_eq!(spec.agent_pane_min_rows, AGENT_PANE_MIN_ROWS);
        assert_eq!(spec.terminal_pane_min_rows, TERMINAL_PANE_MIN_ROWS);
    }

    // -------------------------------------------------------------------------
    // Property-style tests: deterministic sweeps over input sizes.
    //
    // These replace ad-hoc fuzzing with exhaustive parametric loops (the
    // project idiom — no external proptest/quickcheck dependency).
    // -------------------------------------------------------------------------

    /// Representative column samples for sweeps: edge (0/1/2), small, and large.
    const COL_SAMPLES: [u16; 9] = [0, 1, 2, 10, 20, 60, 80, 120, 200];
    /// Representative row samples plus a dense 0..=64 range (covered in tests).
    const ROW_SAMPLES: [u16; 9] = [0, 1, 2, 3, 4, 8, 24, 40, 50];

    #[test]
    fn prop_pty_dimensions_invariants_hold_across_sizes() {
        for fullscreen in [true, false] {
            for &cols in &COL_SAMPLES {
                for &rows in &ROW_SAMPLES {
                    let layout = compute_pty_layout_inner(cols, rows, fullscreen);
                    assert!(
                        layout.pty_rows >= 2,
                        "pty_rows < 2 for ({cols}, {rows}, fs={fullscreen})"
                    );
                    assert!(
                        layout.pty_cols >= 2,
                        "pty_cols < 2 for ({cols}, {rows}, fs={fullscreen})"
                    );
                }
                // Dense row sweep: every value 0..=64, both fullscreen states.
                for rows in 0..=64u16 {
                    let layout = compute_pty_layout_inner(cols, rows, fullscreen);
                    assert!(
                        layout.pty_rows >= 2,
                        "pty_rows < 2 for (cols={cols}, rows={rows}, fs={fullscreen})"
                    );
                    assert!(
                        layout.pty_cols >= 2,
                        "pty_cols < 2 for (cols={cols}, rows={rows}, fs={fullscreen})"
                    );
                }
            }
        }
    }

    #[test]
    fn prop_pane_origin_invariants() {
        for fullscreen in [true, false] {
            for &cols in &COL_SAMPLES {
                for &rows in &ROW_SAMPLES {
                    let layout = compute_pty_layout_inner(cols, rows, fullscreen);
                    assert_eq!(
                        layout.pane_col0,
                        LEFT_COL_WIDTH + 1,
                        "pane_col0 must equal LEFT_COL_WIDTH+1 for ({cols}, {rows}, fs={fullscreen})"
                    );
                    assert!(
                        layout.pane_col0 > 0,
                        "pane_col0 must be positive for ({cols}, {rows}, fs={fullscreen})"
                    );
                    assert!(
                        layout.pane_row0 > 0,
                        "pane_row0 must be positive for ({cols}, {rows}, fs={fullscreen})"
                    );
                }
            }
        }
    }

    /// Independently recompute the half-up rounded agent rows and confirm the
    /// layout's `pane_row0` matches the derived value (1 + agent_rows + 2).
    #[test]
    fn prop_agent_rows_half_up_rounding() {
        for fullscreen in [true, false] {
            for term_rows in 0..=300u16 {
                let cols: u16 = 120; // wide enough that cols don't constrain rows
                let (_, eff_rows) = effective_render_size_inner(cols, term_rows, fullscreen);
                let content_rows = eff_rows.saturating_sub(OUTER_BARS_HEIGHT);
                let agent_rows = expected_agent_rows(content_rows);
                let layout = compute_pty_layout_inner(cols, term_rows, fullscreen);
                // pane_row0 = 1 + agent_rows + 2
                let expected_pane_row0 = 1u16.saturating_add(agent_rows).saturating_add(2);
                assert_eq!(
                    layout.pane_row0, expected_pane_row0,
                    "pane_row0 mismatch for term_rows={term_rows}, fs={fullscreen}"
                );
            }
        }
    }

    /// For the middle-row split, agent_rows + terminal_rows must equal
    /// content_rows when there is enough space, and terminal_rows is always >= 1.
    #[test]
    fn prop_dashboard_split_sums_to_content_rows() {
        for render_rows in 0..=300u16 {
            let content_rows = render_rows.saturating_sub(OUTER_BARS_HEIGHT);
            let (agent_rows, terminal_rows) = dashboard_middle_row_heights_inner(render_rows);
            assert!(
                terminal_rows >= 1,
                "terminal_rows must be >= 1 for render_rows={render_rows}"
            );
            // When content_rows is large enough to avoid the degenerate floor,
            // the split must partition content_rows exactly.
            if content_rows > AGENT_PANE_MIN_ROWS + TERMINAL_PANE_MIN_ROWS {
                assert_eq!(
                    agent_rows + terminal_rows,
                    content_rows,
                    "split must sum to content_rows for render_rows={render_rows}"
                );
            }
        }
    }

    /// Replicate the agent-pane rounding logic independently to cross-check.
    fn expected_agent_rows(content_rows: u16) -> u16 {
        if content_rows <= AGENT_PANE_MIN_ROWS + TERMINAL_PANE_MIN_ROWS {
            let terminal_rows = content_rows.saturating_sub(AGENT_PANE_MIN_ROWS).max(1);
            return content_rows.saturating_sub(terminal_rows);
        }
        let preferred = content_rows
            .saturating_mul(25)
            .saturating_add(50)
            .saturating_div(100);
        let max_agent = content_rows.saturating_sub(TERMINAL_PANE_MIN_ROWS);
        preferred
            .clamp(AGENT_PANE_MIN_ROWS, max_agent)
            .min(content_rows)
    }

    // -------------------------------------------------------------------------
    // Golden / snapshot tests: lock the exact PtyLayout for representative sizes.
    //
    // These act as snapshot tests (without the insta crate) — they pin the full
    // computed geometry so any unintended change to the layout algorithm is
    // caught. Values are derived from the established algorithm; if you
    // intentionally change the layout, update these in lockstep.
    // -------------------------------------------------------------------------

    /// Representative `(cols, rows, fullscreen, expected)` golden cases.
    ///
    /// These pin the full computed geometry for representative terminal sizes.
    /// Values are derived from the established algorithm; if the layout is
    /// intentionally changed, update these in lockstep.
    const GOLDEN_CASES: &[(u16, u16, bool, PtyLayout)] = &[
        // fullscreen = true
        (
            80,
            24,
            true,
            PtyLayout {
                pty_rows: 13,
                pty_cols: 20,
                pane_col0: 23,
                pane_row0: 9,
            },
        ),
        (
            120,
            40,
            true,
            PtyLayout {
                pty_rows: 25,
                pty_cols: 60,
                pane_col0: 23,
                pane_row0: 13,
            },
        ),
        (
            200,
            50,
            true,
            PtyLayout {
                pty_rows: 33,
                pty_cols: 140,
                pane_col0: 23,
                pane_row0: 15,
            },
        ),
        (
            60,
            20,
            true,
            PtyLayout {
                pty_rows: 10,
                pty_cols: 2,
                pane_col0: 23,
                pane_row0: 8,
            },
        ),
        (
            100,
            30,
            true,
            PtyLayout {
                pty_rows: 18,
                pty_cols: 40,
                pane_col0: 23,
                pane_row0: 10,
            },
        ),
        (
            10,
            10,
            true,
            PtyLayout {
                pty_rows: 2,
                pty_cols: 2,
                pane_col0: 23,
                pane_row0: 6,
            },
        ),
        (
            20,
            8,
            true,
            PtyLayout {
                pty_rows: 2,
                pty_cols: 2,
                pane_col0: 23,
                pane_row0: 6,
            },
        ),
        // fullscreen = false (windowed: each dim shrinks by 2)
        (
            80,
            24,
            false,
            PtyLayout {
                pty_rows: 12,
                pty_cols: 18,
                pane_col0: 23,
                pane_row0: 8,
            },
        ),
        (
            120,
            40,
            false,
            PtyLayout {
                pty_rows: 24,
                pty_cols: 58,
                pane_col0: 23,
                pane_row0: 12,
            },
        ),
        (
            200,
            50,
            false,
            PtyLayout {
                pty_rows: 31,
                pty_cols: 138,
                pane_col0: 23,
                pane_row0: 15,
            },
        ),
        (
            60,
            20,
            false,
            PtyLayout {
                pty_rows: 9,
                pty_cols: 2,
                pane_col0: 23,
                pane_row0: 7,
            },
        ),
        (
            100,
            30,
            false,
            PtyLayout {
                pty_rows: 16,
                pty_cols: 38,
                pane_col0: 23,
                pane_row0: 10,
            },
        ),
        (
            10,
            10,
            false,
            PtyLayout {
                pty_rows: 2,
                pty_cols: 2,
                pane_col0: 23,
                pane_row0: 6,
            },
        ),
        (
            20,
            8,
            false,
            PtyLayout {
                pty_rows: 2,
                pty_cols: 2,
                pane_col0: 23,
                pane_row0: 6,
            },
        ),
    ];

    #[test]
    fn golden_pty_layout_representative_sizes() {
        for &(cols, rows, fullscreen, expected) in GOLDEN_CASES {
            let actual = compute_pty_layout_inner(cols, rows, fullscreen);
            assert_eq!(
                actual, expected,
                "golden mismatch for ({cols}x{rows}, fullscreen={fullscreen})"
            );
        }
    }
}
