//! Phase P13 (UI TDD) behavioral tests for PR-mode filter/screen rendering.
//!
//! Split out of `pr_render_tests.rs` to keep each test module within the
//! repository's per-file length budget. These tests cover the filter-controls
//! field projection, the two-column screen layout, the error banner, and the
//! keybind-bar labels.
//!
//! @plan PLAN-20260624-PR-MODE.P13
//! @requirement REQ-PR-008
//! @requirement REQ-PR-012
//! @requirement REQ-PR-013

use crate::domain::{ChecksFilter, PrFilter, PrFilterState, ReviewDecisionFilter};
use crate::layout::{
    LEFT_COL_WIDTH, PRS_SIDEBAR_WIDTH, PrsColumns, pr_error_banner_line, prs_detail_viewport_rows,
    prs_main_columns,
};
use crate::state::ScreenMode;
use crate::ui::components::keybind_bar::keybind_hints_for;
use crate::ui::components::pr_filter_controls::pr_filter_field_views;

// ===========================================================================
// Test 12 — REQ-PR-008: filter controls render all 8 fields + active highlight.
// ===========================================================================

/// The eight documented filter-field labels in render order.
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 249-263
const EXPECTED_PR_FILTER_LABELS: [&str; 8] = [
    "state", "draft", "review", "checks", "author", "assignee", "reviewer", "labels",
];

/// Assert the eight projected filter fields match the contract for the given
/// active_index (labels, count, exactly-one-active, spot-checked values).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 249-263
fn assert_filter_fields_contract(filter: &PrFilter, labels_text: &str, active_index: usize) {
    let views = pr_filter_field_views(filter, labels_text, active_index);
    assert_eq!(
        views.len(),
        8,
        "exactly 8 filter fields must render (active_index={active_index})"
    );
    let labels: Vec<&str> = views.iter().map(|v| v.label.as_str()).collect();
    assert_eq!(
        labels, EXPECTED_PR_FILTER_LABELS,
        "field labels must be in the documented order (active_index={active_index})"
    );
    let active_count = views.iter().filter(|v| v.active).count();
    assert_eq!(
        active_count, 1,
        "exactly one field must be active (active_index={active_index})"
    );
    assert!(
        views[active_index].active,
        "the field at active_index={active_index} must be the active one"
    );
    assert_eq!(views[0].value, "open", "state value must be 'open'");
    assert_eq!(
        views[1].value, "ready-only",
        "draft value must be 'ready-only'"
    );
    assert_eq!(
        views[2].value, "approved",
        "review value must be 'approved'"
    );
    assert_eq!(views[3].value, "success", "checks value must be 'success'");
    assert_eq!(views[4].value, "alice", "author must render non-empty text");
    assert_eq!(views[5].value, "any", "assignee must be 'any' when empty");
    assert_eq!(
        views[7].value, "enhancement",
        "labels must render non-empty text"
    );
}

/// The component's field projection (`pr_filter_field_views`) renders exactly
/// 8 fields in the documented order (state, draft, review, checks, author,
/// assignee, reviewer, labels), with EXACTLY ONE field active at the
/// `active_index`, and the expected display values — REQ-PR-008.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 249-263
#[test]
fn test_pr_filter_controls_render_all_fields_and_highlight_active() {
    let filter = PrFilter {
        query_text: "search".to_string(),
        state: Some(PrFilterState::Open),
        author: "alice".to_string(),
        assignee: String::new(),
        reviewer: String::new(),
        is_draft: Some(false),
        labels: vec!["bug".to_string()],
        review_decision: ReviewDecisionFilter::Approved,
        checks_status: ChecksFilter::Success,
    };
    let labels_text = "enhancement";
    for active_index in [0usize, 3, 7] {
        assert_filter_fields_contract(&filter, labels_text, active_index);
    }
}

// ===========================================================================
// Test 14 — mockups: sidebar width 22u and two-column layout.
// ===========================================================================

/// The PR-mode main row is a two-column layout: a fixed 22-column sidebar
/// plus a flex-grow workspace that fills the remaining width. The pure
/// projection `prs_main_columns` exposes this contract so a test can assert
/// the column geometry without a render harness (mockups.md measurements:
/// 22u fixed sidebar + flex-grow workspace).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-001
/// @pseudocode component-001 lines 1-12
#[test]
fn test_pr_screen_layout_sidebar_22u_and_two_column() {
    let cols: PrsColumns = prs_main_columns(120);
    assert_eq!(
        cols.sidebar_width, 22,
        "PR sidebar must be 22 columns wide (mockups measurement)"
    );
    assert_eq!(
        cols.sidebar_width, PRS_SIDEBAR_WIDTH,
        "prs_main_columns.sidebar_width must equal PRS_SIDEBAR_WIDTH"
    );
    assert_eq!(
        cols.sidebar_width, LEFT_COL_WIDTH,
        "PRS_SIDEBAR_WIDTH must equal LEFT_COL_WIDTH"
    );
    assert_eq!(
        cols.workspace_width,
        120 - 22,
        "workspace must fill the remaining columns after the 22u sidebar"
    );
    assert_eq!(
        cols.sidebar_width + cols.workspace_width,
        120,
        "the two columns must tile the full terminal width"
    );

    // Small-terminal case: the sidebar width saturates (never negative) and the
    // workspace collapses to 0 without panicking.
    let tiny = prs_main_columns(10);
    assert_eq!(
        tiny.sidebar_width, 22,
        "sidebar width is fixed at 22u even on tiny terminals"
    );
    assert_eq!(
        tiny.workspace_width, 0,
        "workspace collapses to 0 when term_cols < sidebar_width (no panic)"
    );
}

// ===========================================================================
// Test 15 — REQ-PR-013: error banner renders its text and consumes a row.
// ===========================================================================

/// When an error is present, the PR-mode screen renders an `Error: {msg}`
/// banner (asserted via the pure `pr_error_banner_line` projection — the
/// screen delegates its banner text to it), and `None` when there is no
/// error. Additionally the banner consumes a row: `prs_detail_viewport_rows`
/// with `has_error=true` yields fewer detail rows than without.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-013
/// @pseudocode component-001 lines 1-12
#[test]
fn test_pr_screen_renders_error_banner_when_error_present() {
    // PRIMARY: the rendered banner text/content (the screen delegates to
    // pr_error_banner_line for its banner Text content).
    assert_eq!(
        pr_error_banner_line(Some("boom")),
        Some("Error: boom".to_string()),
        "error banner must render 'Error: <msg>' when an error is present"
    );
    assert_eq!(
        pr_error_banner_line(None),
        None,
        "no error banner must render when there is no error"
    );

    // GEOMETRY: the error banner consumes a row (detail viewport shrinks).
    let term_rows = 40;
    let no_error = prs_detail_viewport_rows(term_rows, false, false);
    let with_error = prs_detail_viewport_rows(term_rows, true, false);
    assert!(
        with_error < no_error,
        "error banner must shrink the detail viewport (no_error={no_error}, with_error={with_error})"
    );
}

// ===========================================================================
// Test 16 — REQ-PR-012: keybind bar lists `o open` (display-only).
// ===========================================================================

/// The PR-mode keybind bar (`keybind_hints_for`, to which the `KeybindBar`
/// component delegates) includes an `o open` label and an `m merge`
/// label (issue #92). When the terminal is focused, the bar short-circuits
/// to the `F12 unfocus` hint. It also includes the property-edit shortcuts
/// (issue #175): `L labels A assignees M milestone T title W state`.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-012
/// @pseudocode component-001 lines 1-12
#[test]
fn test_pr_keybind_bar_and_help_list_o_open_in_browser() {
    let hints = keybind_hints_for(ScreenMode::DashboardPullRequests, false, None);
    assert!(
        hints.contains("o open"),
        "PR-mode keybind bar must list 'o open', got: {hints}"
    );
    assert!(
        hints.contains("m merge"),
        "PR-mode keybind bar must list 'm merge' (issue #92), got: {hints}"
    );
    assert!(
        hints.contains("L labels"),
        "PR-mode keybind bar must list 'L labels' (issue #175), got: {hints}"
    );
    assert!(
        hints.contains("W state"),
        "PR-mode keybind bar must list 'W state' (issue #175), got: {hints}"
    );
    assert!(
        !hints.contains("approve"),
        "keybind bar must not have an approve binding: {hints}"
    );

    // terminal_focused short-circuit: the bar shows the unfocus hint instead.
    assert_eq!(
        keybind_hints_for(ScreenMode::DashboardPullRequests, true, None),
        "F12 unfocus",
        "focused-terminal keybind bar must short-circuit to 'F12 unfocus'"
    );
}

// ===========================================================================
// Issue #175: keybind bar lists property-edit shortcuts for both modes.
// ===========================================================================

/// The issues-mode keybind bar includes the property-edit shortcuts
/// (issue #175): `L labels A assignees M milestone T title Y type W state`.
#[test]
fn test_issues_keybind_bar_lists_property_edit_shortcuts() {
    let hints = keybind_hints_for(ScreenMode::DashboardIssues, false, None);
    assert!(
        hints.contains("L labels"),
        "Issues keybind bar must list 'L labels' (issue #175), got: {hints}"
    );
    assert!(
        hints.contains("A assignees"),
        "Issues keybind bar must list 'A assignees' (issue #175), got: {hints}"
    );
    assert!(
        hints.contains("M milestone"),
        "Issues keybind bar must list 'M milestone' (issue #175), got: {hints}"
    );
    assert!(
        hints.contains("T title"),
        "Issues keybind bar must list 'T title' (issue #175), got: {hints}"
    );
    assert!(
        hints.contains("Y type"),
        "Issues keybind bar must list 'Y type' (issue #175, issues only), got: {hints}"
    );
    assert!(
        hints.contains("W state"),
        "Issues keybind bar must list 'W state' (issue #175), got: {hints}"
    );
}

/// The PR-mode keybind bar lists property-edit shortcuts but NOT `Y type`
/// (PRs don't have the Type property).
#[test]
fn test_pr_keybind_bar_has_no_type_shortcut() {
    let hints = keybind_hints_for(ScreenMode::DashboardPullRequests, false, None);
    assert!(
        !hints.contains("Y type"),
        "PR keybind bar must NOT list 'Y type' (PRs have no Type property), got: {hints}"
    );
}
