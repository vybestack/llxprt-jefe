//! Unit tests for the pure selection model (iocraft-free).
//!
//! These exercise [`jefe::selection::pane_at`], [`normalize_selection`],
//! [`selection_text`], and [`point_to_content_coords`] without any terminal.

use jefe::layout::{LEFT_COL_WIDTH, TERMINAL_VIEW_CHROME_COLS, TERMINAL_VIEW_CHROME_ROWS};
use jefe::selection::{
    HighlightRange, PaneGeometry, SelectablePane, SelectionPoint, TextSelection,
    normalize_selection, pane_at, point_to_content_coords, row_highlight_range, selection_text,
};
use jefe::state::ScreenMode;

const DASHBOARD: ScreenMode = ScreenMode::Dashboard;
const SPLIT: ScreenMode = ScreenMode::Split;
const ISSUES: ScreenMode = ScreenMode::DashboardIssues;
const PRS: ScreenMode = ScreenMode::DashboardPullRequests;

fn layout(
    cols: u16,
    rows: u16,
    mode: ScreenMode,
    error_visible: bool,
    filter_open: bool,
) -> jefe::selection::ScreenLayout {
    jefe::selection::ScreenLayout::new(cols, rows, mode, error_visible, filter_open)
}

// ── PaneGeometry::contains ──────────────────────────────────────────────────

#[test]
fn geometry_contains_includes_interior_and_edges() {
    let g = PaneGeometry::new(5, 3, 4, 2, 6, 4);
    assert!(g.contains(5, 3));
    assert!(g.contains(8, 4)); // bottom-right inclusive
    assert!(!g.contains(4, 3)); // left of origin
    assert!(!g.contains(9, 4)); // right of edge
    assert!(!g.contains(5, 5)); // below edge
}

#[test]
fn geometry_with_chrome_derives_content_origin() {
    let g = PaneGeometry::with_chrome(10, 5, 40, 20, 2, 3);
    assert_eq!(g.origin_col, 10);
    assert_eq!(g.origin_row, 5);
    assert_eq!(g.content_origin_col, 12);
    assert_eq!(g.content_origin_row, 8);
}

// ── pane_at: dashboard ──────────────────────────────────────────────────────

#[test]
fn pane_at_dashboard_status_bar() {
    let lay = layout(120, 40, DASHBOARD, false, false);
    let Some((pane, geo)) = pane_at(60, 0, DASHBOARD, false, &lay) else {
        panic!("expected status bar at (60, 0)");
    };
    assert!(matches!(pane, SelectablePane::StatusBar));
    assert_eq!(geo.height, 1);
}

#[test]
fn pane_at_dashboard_keybind_bar() {
    let lay = layout(120, 40, DASHBOARD, false, false);
    let Some((pane, _)) = pane_at(60, 39, DASHBOARD, false, &lay) else {
        panic!("expected keybind bar at (60, 39)");
    };
    assert!(matches!(pane, SelectablePane::KeybindBar));
}

#[test]
fn pane_at_dashboard_sidebar() {
    let lay = layout(120, 40, DASHBOARD, false, false);
    let Some((pane, geo)) = pane_at(0, 5, DASHBOARD, false, &lay) else {
        panic!("expected sidebar at (0, 5)");
    };
    assert!(matches!(pane, SelectablePane::Sidebar));
    assert_eq!(geo.origin_col, 0);
    assert_eq!(geo.origin_row, 1);
    // Sidebar content starts after the border, title, and top content padding.
    assert_eq!(geo.content_origin_col, 2);
    assert_eq!(geo.content_origin_row, 4);
}

#[test]
fn pane_at_dashboard_preview() {
    let lay = layout(120, 40, DASHBOARD, false, false);
    // Preview starts at col 120-36 = 84.
    let Some((pane, geo)) = pane_at(100, 5, DASHBOARD, false, &lay) else {
        panic!("expected preview at (100, 5)");
    };
    assert!(matches!(pane, SelectablePane::Preview));
    assert_eq!(geo.origin_col, 84);
}

#[test]
fn pane_at_dashboard_agent_list() {
    let lay = layout(120, 40, DASHBOARD, false, false);
    // Agent list sits below the status bar at row 1.
    let Some((pane, _)) = pane_at(30, 1, DASHBOARD, false, &lay) else {
        panic!("expected agent list at (30, 1)");
    };
    assert!(matches!(pane, SelectablePane::AgentList));
}

#[test]
fn pane_at_dashboard_terminal_unfocused() {
    let lay = layout(120, 40, DASHBOARD, false, false);
    // Terminal widget is below the agent list. Use a row deep in the middle column.
    let Some((pane, _)) = pane_at(30, 20, DASHBOARD, false, &lay) else {
        panic!("expected terminal view at (30, 20)");
    };
    assert!(matches!(pane, SelectablePane::TerminalView));
}

#[test]
fn pane_at_dashboard_agent_terminal_boundary() {
    let lay = layout(120, 40, DASHBOARD, false, false);
    // Find the exact boundary row between AgentList and TerminalView.
    let Some((_, agent_geo)) = pane_at(30, 1, DASHBOARD, false, &lay) else {
        panic!("expected agent list at (30, 1)");
    };
    let agent_end_row = agent_geo.origin_row + agent_geo.height;
    // The row at agent_end_row should be the first terminal row.
    let Some((pane, term_geo)) = pane_at(30, agent_end_row, DASHBOARD, false, &lay) else {
        panic!("expected terminal view at agent boundary row {agent_end_row}");
    };
    assert!(matches!(pane, SelectablePane::TerminalView));
    assert_eq!(term_geo.origin_row, agent_end_row);
    // The row just above the boundary should still be AgentList.
    let Some((above_pane, _)) = pane_at(30, agent_end_row - 1, DASHBOARD, false, &lay) else {
        panic!("expected agent list at row {}", agent_end_row - 1);
    };
    assert!(matches!(above_pane, SelectablePane::AgentList));
}

#[test]
fn pane_at_dashboard_terminal_focused_returns_none_in_terminal_region() {
    let lay = layout(120, 40, DASHBOARD, false, false);
    // When the terminal is focused, mouse over the terminal goes to the PTY,
    // so pane_at yields None for that region (but other panes still resolve).
    let in_terminal = pane_at(30, 20, DASHBOARD, true, &lay);
    assert!(in_terminal.is_none());
    // Sidebar still resolves even when terminal focused.
    let sidebar = pane_at(0, 5, DASHBOARD, true, &lay);
    assert!(matches!(
        sidebar.map(|(p, _)| p),
        Some(SelectablePane::Sidebar)
    ));
}

// Verify the terminal pane's content-origin geometry in the dashboard middle
// column. `pane_at` with `terminal_input_enabled = false` resolves the
// terminal region to TerminalView with chrome offsets; pin the content origin
// so selection coordinates match the rendered snapshot. (Issue #197: the
// mouse router relies on this geometry when it decides to paint a Jefe
// selection over a focused terminal.)
#[test]
fn pane_at_dashboard_terminal_content_origin_geometry() {
    let lay = layout(120, 40, DASHBOARD, false, false);
    // terminal_input_enabled = false mirrors what the router passes when the
    // routing policy chose AppSelection over the focused terminal.
    let selectable = pane_at(30, 20, DASHBOARD, false, &lay);
    let Some((pane, geo)) = selectable else {
        panic!("expected TerminalView to resolve when terminal is selectable");
    };
    assert!(matches!(pane, SelectablePane::TerminalView));
    // Content origin must match the unfocused terminal geometry so selection
    // coordinates line up with the rendered snapshot.
    assert_eq!(
        geo.content_origin_col,
        LEFT_COL_WIDTH + TERMINAL_VIEW_CHROME_COLS
    );
    assert_eq!(
        geo.content_origin_row,
        geo.origin_row + TERMINAL_VIEW_CHROME_ROWS
    );
}

// ── pane_at: split mode ─────────────────────────────────────────────────────

#[test]
fn pane_at_split_uses_full_width_sidebar_between_layout_bands() {
    let lay = layout(100, 25, SPLIT, false, false);

    for col in [1, 50, 98] {
        let Some((pane, geo)) = pane_at(col, 5, SPLIT, false, &lay) else {
            panic!("expected split sidebar at column {col}");
        };
        assert!(matches!(pane, SelectablePane::Sidebar));
        assert_eq!(geo.origin_col, 1);
        assert_eq!(geo.origin_row, 5);
        assert_eq!(geo.width, 98);
        assert_eq!(geo.height, 18);
        assert_eq!(geo.content_origin_row, 8);
    }
}

#[test]
fn pane_at_split_excludes_padding_filter_and_outer_boundaries() {
    let lay = layout(100, 25, SPLIT, false, false);

    for point in [(0, 5), (99, 5), (50, 1), (50, 2), (50, 4), (50, 23)] {
        assert!(
            pane_at(point.0, point.1, SPLIT, false, &lay).is_none(),
            "split chrome point {point:?} must not resolve to a pane"
        );
    }
    assert!(matches!(
        pane_at(50, 22, SPLIT, false, &lay).map(|(pane, _)| pane),
        Some(SelectablePane::Sidebar)
    ));
    assert!(matches!(
        pane_at(50, 24, SPLIT, false, &lay).map(|(pane, _)| pane),
        Some(SelectablePane::KeybindBar)
    ));
}

// ── pane_at: issues mode ────────────────────────────────────────────────────

#[test]
fn pane_at_issues_sidebar() {
    let lay = layout(120, 40, ISSUES, false, false);
    let Some((pane, _)) = pane_at(5, 10, ISSUES, false, &lay) else {
        panic!("expected issues sidebar at (5, 10)");
    };
    assert!(matches!(pane, SelectablePane::Sidebar));
}

#[test]
fn pane_at_issues_list() {
    let lay = layout(120, 40, ISSUES, false, false);
    // Workspace starts at col 22; list is the top split.
    let Some((pane, _)) = pane_at(40, 2, ISSUES, false, &lay) else {
        panic!("expected issue list at (40, 2)");
    };
    assert!(matches!(pane, SelectablePane::IssueList));
}

#[test]
fn pane_at_issues_detail() {
    let lay = layout(120, 40, ISSUES, false, false);
    // Detail sits below the list. Use a row well past the list split (30% of ~38 rows).
    let Some((pane, _)) = pane_at(40, 25, ISSUES, false, &lay) else {
        panic!("expected issue detail at (40, 25)");
    };
    assert!(matches!(pane, SelectablePane::IssueDetail));
}

#[test]
fn pane_at_issues_with_error_banner_shifts_workspace_down() {
    let lay = layout(120, 40, ISSUES, true, false);
    // Row 1 is the error banner — not selectable (returns None).
    assert!(pane_at(40, 1, ISSUES, false, &lay).is_none());
    // Row 2+ is the workspace, shifted down by one.
    let Some((pane, geo)) = pane_at(40, 2, ISSUES, false, &lay) else {
        panic!("expected a pane for workspace at (40, 2)");
    };
    assert_eq!(geo.origin_row, 2);
    let _ = pane;
}

#[test]
fn pane_at_issues_with_filter_controls_shifts_workspace_down() {
    let lay = layout(120, 40, ISSUES, false, true);
    // Filter band occupies 5 rows starting at row 1 — not selectable (it is
    // a separate UI element with no content provider).
    assert!(pane_at(40, 2, ISSUES, false, &lay).is_none());
    // Below the filter band (row 6+) is the issue list.
    let Some((pane, geo)) = pane_at(40, 6, ISSUES, false, &lay) else {
        panic!("expected issue list below filter band at (40, 6)");
    };
    assert!(matches!(pane, SelectablePane::IssueList));
    assert_eq!(geo.origin_row, 6);
    let _ = geo;
}

// ── pane_at: PR mode (mirrors issues geometry, different pane names) ─────────

#[test]
fn pane_at_pr_list() {
    let lay = layout(120, 40, PRS, false, false);
    let Some((pane, _)) = pane_at(40, 2, PRS, false, &lay) else {
        panic!("expected pr list at (40, 2)");
    };
    assert!(matches!(pane, SelectablePane::PrList));
}

#[test]
fn pane_at_pr_detail() {
    let lay = layout(120, 40, PRS, false, false);
    let Some((pane, _)) = pane_at(40, 25, PRS, false, &lay) else {
        panic!("expected pr detail at (40, 25)");
    };
    assert!(matches!(pane, SelectablePane::PrDetail));
}

// ── pane_at: out of bounds ──────────────────────────────────────────────────

#[test]
fn pane_at_out_of_bounds_returns_none() {
    let lay = layout(120, 40, DASHBOARD, false, false);
    assert!(pane_at(200, 5, DASHBOARD, false, &lay).is_none());
    assert!(pane_at(5, 200, DASHBOARD, false, &lay).is_none());
}

// ── normalize_selection ─────────────────────────────────────────────────────

#[test]
fn normalize_keeps_order_when_anchor_before_focus() {
    let early = SelectionPoint::new(SelectablePane::IssueDetail, 0, 2);
    let late = SelectionPoint::new(SelectablePane::IssueDetail, 1, 0);
    let (start, end) = normalize_selection(&early, &late);
    assert_eq!((start.line, start.col), (0, 2));
    assert_eq!((end.line, end.col), (1, 0));
}

#[test]
fn normalize_swaps_when_anchor_after_focus() {
    let early = SelectionPoint::new(SelectablePane::IssueDetail, 1, 0);
    let late = SelectionPoint::new(SelectablePane::IssueDetail, 0, 2);
    let (start, end) = normalize_selection(&late, &early);
    assert_eq!((start.line, start.col), (0, 2));
    assert_eq!((end.line, end.col), (1, 0));
}

#[test]
fn normalize_same_point_is_equal_pair() {
    let pt = SelectionPoint::new(SelectablePane::IssueDetail, 3, 4);
    let (start, end) = normalize_selection(&pt, &pt);
    assert_eq!(start, pt);
    assert_eq!(end, pt);
}

// ── selection_text ──────────────────────────────────────────────────────────

fn lines(input: &[&str]) -> Vec<String> {
    input.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn selection_text_single_line_substring() {
    let l = lines(&["hello world", "second"]);
    let sel = TextSelection {
        anchor: SelectionPoint::new(SelectablePane::IssueDetail, 0, 6),
        focus: SelectionPoint::new(SelectablePane::IssueDetail, 0, 11),
    };
    assert_eq!(selection_text(&sel, &l), "world");
}

#[test]
fn selection_text_single_line_reversed() {
    let l = lines(&["hello world", "second"]);
    let sel = TextSelection {
        anchor: SelectionPoint::new(SelectablePane::IssueDetail, 0, 11),
        focus: SelectionPoint::new(SelectablePane::IssueDetail, 0, 6),
    };
    assert_eq!(selection_text(&sel, &l), "world");
}

#[test]
fn selection_text_multi_line() {
    let l = lines(&["abc", "def", "ghi"]);
    let sel = TextSelection {
        anchor: SelectionPoint::new(SelectablePane::IssueDetail, 0, 1),
        focus: SelectionPoint::new(SelectablePane::IssueDetail, 2, 2),
    };
    assert_eq!(selection_text(&sel, &l), "bc\ndef\ngh");
}

#[test]
fn selection_text_empty_when_anchor_equals_focus() {
    let l = lines(&["abc", "def"]);
    let sel = TextSelection::collapsed(SelectionPoint::new(SelectablePane::IssueDetail, 0, 1));
    assert_eq!(selection_text(&sel, &l), "");
}

#[test]
fn selection_text_clamps_past_end_of_line() {
    let l = lines(&["ab"]);
    let sel = TextSelection {
        anchor: SelectionPoint::new(SelectablePane::IssueDetail, 0, 0),
        focus: SelectionPoint::new(SelectablePane::IssueDetail, 0, 99),
    };
    assert_eq!(selection_text(&sel, &l), "ab");
}

#[test]
fn selection_text_clamps_past_last_line() {
    let l = lines(&["ab", "cd"]);
    let sel = TextSelection {
        anchor: SelectionPoint::new(SelectablePane::IssueDetail, 0, 0),
        focus: SelectionPoint::new(SelectablePane::IssueDetail, 99, 0),
    };
    assert_eq!(selection_text(&sel, &l), "ab\ncd");
}

#[test]
fn selection_text_empty_lines_input_returns_empty() {
    let sel = TextSelection::collapsed(SelectionPoint::new(SelectablePane::IssueDetail, 0, 0));
    assert_eq!(selection_text(&sel, &[]), "");
}

// ── point_to_content_coords ─────────────────────────────────────────────────

#[test]
fn point_to_content_coords_adjusts_for_content_origin_and_scroll() {
    // Content origin at (22, 5): a click at col 25, row 7 → content (line 2, col 3)
    // before scroll; with scroll_offset 3 → line 5.
    let geo = PaneGeometry::new(20, 3, 60, 20, 22, 5);
    let (line, col) = point_to_content_coords(25, 7, 3, &geo);
    assert_eq!(line, 5); // row 7 - content_origin 5 + scroll 3
    assert_eq!(col, 3); // col 25 - content_origin 22
}

#[test]
fn point_to_content_coords_zero_scroll() {
    let geo = PaneGeometry::new(0, 1, 40, 10, 0, 1);
    let (line, col) = point_to_content_coords(2, 3, 0, &geo);
    assert_eq!(line, 2);
    assert_eq!(col, 2);
}

#[test]
fn point_to_content_coords_clamps_before_origin() {
    let geo = PaneGeometry::new(22, 5, 60, 20, 24, 7);
    let (line, col) = point_to_content_coords(10, 2, 0, &geo);
    assert_eq!(line, 0); // row 2 - content_origin 7 saturates to 0
    assert_eq!(col, 0); // col 10 - content_origin 24 saturates to 0
}

#[test]
fn point_to_content_coords_accounts_for_list_chrome() {
    // Simulate a bordered list pane whose widget box starts at (22, 1) with
    // content starting at (23, 3) (border + title). A click on the first
    // content row should map to content line 0.
    let geo = PaneGeometry::with_chrome(22, 1, 60, 10, 1, 2);
    let (line, col) = point_to_content_coords(23, 3, 0, &geo);
    assert_eq!(line, 0);
    assert_eq!(col, 0);
}

#[test]
fn point_to_content_coords_detail_pane_header_is_content() {
    // Detail pane: content (the header rows) starts directly below the border,
    // 1 row below the widget-box top. A click on the first header row maps to
    // content line 0.
    let geo = PaneGeometry::with_chrome(22, 20, 60, 18, 2, 1);
    let (line, _col) = point_to_content_coords(24, 21, 0, &geo);
    assert_eq!(line, 0); // first header row (title)
}

// ── pane_at: content origins account for chrome (#141 follow-up) ────────────

#[test]
fn pane_at_pr_list_content_origin_accounts_for_border_and_title() {
    let lay = layout(120, 40, PRS, false, false);
    // PR list widget box top is at row 1. Content starts at row 3 (border +
    // title), col 23 (border). Clicking the first content row maps to line 0.
    let Some((pane, geo)) = pane_at(23, 3, PRS, false, &lay) else {
        panic!("expected pr list at (23, 3)");
    };
    assert!(matches!(pane, SelectablePane::PrList));
    assert_eq!(geo.content_origin_col, geo.origin_col + 1);
    assert_eq!(geo.content_origin_row, geo.origin_row + 2);
}

#[test]
fn pane_at_pr_detail_content_origin_accounts_for_header_rows() {
    let lay = layout(120, 40, PRS, false, false);
    // Detail pane content starts directly below the border (1 row). The fixed
    // header rows are part of the selectable content (rendered above the scroll
    // viewport but not scrolled), so content_origin_row == origin_row + 1.
    let Some((pane, geo)) = pane_at(40, 25, PRS, false, &lay) else {
        panic!("expected pr detail at (40, 25)");
    };
    assert!(matches!(pane, SelectablePane::PrDetail));
    assert_eq!(geo.content_origin_row, geo.origin_row + 1);
    assert_eq!(geo.content_origin_col, geo.origin_col + 2);
}

#[test]
fn pane_at_status_bar_content_origin_accounts_for_padding() {
    let lay = layout(120, 40, DASHBOARD, false, false);
    let Some((pane, geo)) = pane_at(60, 0, DASHBOARD, false, &lay) else {
        panic!("expected status bar at (60, 0)");
    };
    assert!(matches!(pane, SelectablePane::StatusBar));
    assert_eq!(geo.content_origin_col, 1); // padding_left
}

#[test]
fn pane_at_dashboard_agent_list_content_origin_accounts_for_chrome() {
    let lay = layout(120, 40, DASHBOARD, false, false);
    let Some((pane, geo)) = pane_at(30, 1, DASHBOARD, false, &lay) else {
        panic!("expected agent list at (30, 1)");
    };
    assert!(matches!(pane, SelectablePane::AgentList));
    assert_eq!(geo.content_origin_col, geo.origin_col + 2);
    assert_eq!(geo.content_origin_row, geo.origin_row + 3);
}

// ── row_highlight_range ─────────────────────────────────────────────────────

fn sel(start_line: usize, start_col: usize, end_line: usize, end_col: usize) -> TextSelection {
    TextSelection {
        anchor: SelectionPoint::new(SelectablePane::IssueDetail, start_line, start_col),
        focus: SelectionPoint::new(SelectablePane::IssueDetail, end_line, end_col),
    }
}

#[test]
fn highlight_range_none_for_empty_selection() {
    let s = TextSelection::collapsed(SelectionPoint::new(SelectablePane::IssueDetail, 2, 3));
    assert_eq!(row_highlight_range(&s, 2), None);
}

#[test]
fn highlight_range_single_line_substring() {
    let s = sel(1, 2, 1, 5);
    assert_eq!(
        row_highlight_range(&s, 1),
        Some(HighlightRange { start: 2, end: 5 })
    );
}

#[test]
fn highlight_range_line_outside_selection_is_none() {
    let s = sel(1, 0, 3, 0);
    assert_eq!(row_highlight_range(&s, 0), None);
    assert_eq!(row_highlight_range(&s, 4), None);
}

#[test]
fn highlight_range_start_line_tail_to_end() {
    let s = sel(1, 2, 3, 4);
    assert_eq!(
        row_highlight_range(&s, 1),
        Some(HighlightRange {
            start: 2,
            end: usize::MAX
        })
    );
}

#[test]
fn highlight_range_end_line_head_from_zero() {
    let s = sel(1, 2, 3, 4);
    assert_eq!(
        row_highlight_range(&s, 3),
        Some(HighlightRange { start: 0, end: 4 })
    );
}

#[test]
fn highlight_range_middle_line_full() {
    let s = sel(1, 2, 3, 4);
    assert_eq!(
        row_highlight_range(&s, 2),
        Some(HighlightRange {
            start: 0,
            end: usize::MAX
        })
    );
}

#[test]
fn highlight_range_works_with_reversed_anchor_focus() {
    let s = sel(3, 4, 1, 2);
    assert_eq!(
        row_highlight_range(&s, 1),
        Some(HighlightRange {
            start: 2,
            end: usize::MAX
        })
    );
}

// ── pane_at: modal / form / overlay panes (issue #178) ─────────────────────
//
// Full-screen modals/forms (Help, NewAgent, NewRepository, Confirm) cover the
// modal's actual rendered bounds. When `layout.overlay` indicates one is
// active, pane_at must return that pane for coordinates inside the modal's
// bounds. AgentForm/RepositoryForm fill the entire screen; HelpModal is 60 wide
// with variable height; ConfirmModal is 50×10.

fn layout_with_overlay(
    cols: u16,
    rows: u16,
    mode: ScreenMode,
    overlay: jefe::selection::OverlayPane,
) -> jefe::selection::ScreenLayout {
    jefe::selection::ScreenLayout::new(cols, rows, mode, false, false).with_overlay(overlay)
}

#[test]
fn pane_at_help_modal_resolves_within_bounds() {
    let lay = layout_with_overlay(120, 40, DASHBOARD, jefe::selection::OverlayPane::HelpModal);
    // HelpModal is 60 wide; height at 40 rows = viewport(22) + chrome(7) = 29.
    for &(c, r) in &[(0, 0), (30, 5), (59, 28)] {
        let Some((pane, geo)) = pane_at(c, r, DASHBOARD, false, &lay) else {
            panic!("expected help modal at ({c}, {r})");
        };
        assert!(
            matches!(pane, SelectablePane::HelpModal),
            "expected HelpModal at ({c}, {r}), got {pane:?}"
        );
        assert_eq!(geo.width, 60, "help modal width should be 60");
        assert_eq!(geo.height, 29, "help modal height should be 29 at 40 rows");
    }
}

#[test]
fn pane_at_help_modal_outside_bounds_returns_none() {
    let lay = layout_with_overlay(120, 40, DASHBOARD, jefe::selection::OverlayPane::HelpModal);
    // Col 60+ is outside the 60-wide help modal.
    assert!(pane_at(60, 5, DASHBOARD, false, &lay).is_none());
    // Row 29+ is outside the 29-tall help modal.
    assert!(pane_at(30, 29, DASHBOARD, false, &lay).is_none());
}

#[test]
fn pane_at_agent_form_overlay_covers_full_screen() {
    let lay = layout_with_overlay(120, 40, DASHBOARD, jefe::selection::OverlayPane::AgentForm);
    let Some((pane, geo)) = pane_at(50, 10, DASHBOARD, false, &lay) else {
        panic!("expected agent form at (50, 10)");
    };
    assert!(matches!(pane, SelectablePane::AgentForm));
    assert_eq!(geo.width, 120);
    assert_eq!(geo.height, 40);
}

#[test]
fn pane_at_repository_form_overlay_covers_full_screen() {
    let lay = layout_with_overlay(
        120,
        40,
        DASHBOARD,
        jefe::selection::OverlayPane::RepositoryForm,
    );
    let Some((pane, geo)) = pane_at(50, 10, DASHBOARD, false, &lay) else {
        panic!("expected repository form at (50, 10)");
    };
    assert!(matches!(pane, SelectablePane::RepositoryForm));
    assert_eq!(geo.width, 120);
    assert_eq!(geo.height, 40);
}

#[test]
fn pane_at_confirm_modal_resolves_within_50x10_bounds() {
    let lay = layout_with_overlay(
        120,
        40,
        DASHBOARD,
        jefe::selection::OverlayPane::ConfirmModal,
    );
    // ConfirmModal is 50 wide, 10 tall.
    for &(c, r) in &[(0, 0), (25, 5), (49, 9)] {
        let Some((pane, geo)) = pane_at(c, r, DASHBOARD, false, &lay) else {
            panic!("expected confirm modal at ({c}, {r})");
        };
        assert!(
            matches!(pane, SelectablePane::ConfirmModal),
            "expected ConfirmModal at ({c}, {r}), got {pane:?}"
        );
        assert_eq!(geo.width, 50);
        assert_eq!(geo.height, 10);
    }
}

#[test]
fn pane_at_confirm_modal_outside_bounds_returns_none() {
    let lay = layout_with_overlay(
        120,
        40,
        DASHBOARD,
        jefe::selection::OverlayPane::ConfirmModal,
    );
    // Col 50+ is outside the 50-wide confirm modal.
    assert!(pane_at(50, 5, DASHBOARD, false, &lay).is_none());
    // Row 10+ is outside the 10-tall confirm modal.
    assert!(pane_at(25, 10, DASHBOARD, false, &lay).is_none());
}

#[test]
fn pane_at_no_overlay_falls_through_to_normal_panes() {
    // Default ScreenLayout has no overlay; normal panes resolve.
    let lay = layout(120, 40, DASHBOARD, false, false);
    let Some((pane, _)) = pane_at(0, 5, DASHBOARD, false, &lay) else {
        panic!("expected sidebar at (0, 5) with no overlay");
    };
    assert!(matches!(pane, SelectablePane::Sidebar));
}

#[test]
fn pane_at_agent_chooser_overlay_resolves_within_workspace() {
    // Agent chooser is positioned absolutely at top:2, left:4 within the
    // workspace (which starts after the sidebar at col 22). A coordinate
    // inside the chooser bounds should resolve to AgentChooser.
    let lay = layout_with_overlay(120, 40, ISSUES, jefe::selection::OverlayPane::AgentChooser);
    // Chooser origin: col 22+4=26, row 1+2=3. Click inside the chooser.
    let resolved = pane_at(28, 4, ISSUES, false, &lay);
    let Some((pane, _)) = resolved else {
        panic!("expected agent chooser at (28, 4)");
    };
    assert!(matches!(pane, SelectablePane::AgentChooser));
}

#[test]
fn pane_at_merge_chooser_overlay_resolves_within_workspace() {
    let lay = layout_with_overlay(120, 40, PRS, jefe::selection::OverlayPane::MergeChooser);
    let resolved = pane_at(28, 4, PRS, false, &lay);
    let Some((pane, _)) = resolved else {
        panic!("expected merge chooser at (28, 4)");
    };
    assert!(matches!(pane, SelectablePane::MergeChooser));
}

#[test]
fn pane_at_issue_delete_confirm_overlay_resolves_within_workspace() {
    let lay = layout_with_overlay(
        120,
        40,
        ISSUES,
        jefe::selection::OverlayPane::IssueDeleteConfirm,
    );
    let resolved = pane_at(28, 4, ISSUES, false, &lay);
    let Some((pane, _)) = resolved else {
        panic!("expected issue delete confirm at (28, 4)");
    };
    assert!(matches!(pane, SelectablePane::IssueDeleteConfirm));
}
