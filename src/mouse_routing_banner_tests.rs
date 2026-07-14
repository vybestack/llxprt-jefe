//! Issue #265 remediation: notice-only banner geometry in mouse routing.
//!
//! Extracted from `mouse_routing_tests.rs` to keep that file under the
//! source-size hard limit. These tests prove that a notice-only banner
//! (draft_notice set, no error) reserves one row for mouse hit testing and
//! detail viewport sizing — matching the render/sizing path.

use super::super::{refresh_detail_viewport_rows, resolve_pane};
use jefe::selection::SelectablePane;
use jefe::state::{AppState, IssueFilterUiState, IssuesState, PullRequestsState, ScreenMode};

/// A notice-only banner (draft_notice set, no error) must shift the Issues
/// workspace down by one row for mouse hit testing — exactly like an error
/// banner. Row 1 is the banner (not selectable → None); row 2 is the workspace.
#[test]
fn notice_only_banner_shifts_issues_workspace_for_mouse_routing() {
    let state = AppState {
        screen_mode: ScreenMode::DashboardIssues,
        issues_state: IssuesState {
            draft_notice: Some("No agents available".to_string()),
            ..IssuesState::default()
        },
        ..AppState::default()
    };

    // Row 1 is the notice banner — must NOT resolve to any selectable pane.
    assert!(
        resolve_pane(&state, 40, 1, 120, 40, false).is_none(),
        "notice-only banner row 1 must be non-selectable (reserved for render)"
    );

    // Row 2+ is the workspace, shifted down by one.
    let Some((pane, geo)) = resolve_pane(&state, 40, 2, 120, 40, false) else {
        panic!("expected a selectable pane at workspace row 2 with notice banner");
    };
    assert!(
        matches!(pane, SelectablePane::IssueList),
        "notice banner must shift the issue-list pane rather than another pane"
    );
    assert_eq!(
        geo.origin_row, 2,
        "workspace must start at row 2 (shifted by notice banner)"
    );
}

/// When neither error nor draft_notice is set, no banner row is reserved:
/// row 1 is the workspace (issue list), confirming the notice test above is
/// not vacuously true.
#[test]
fn no_banner_keeps_issues_workspace_at_row_one_for_mouse_routing() {
    let state = AppState {
        screen_mode: ScreenMode::DashboardIssues,
        ..AppState::default()
    };

    let Some((pane, geo)) = resolve_pane(&state, 40, 1, 120, 40, false) else {
        panic!("expected issue list at row 1 with no banner");
    };
    assert!(
        matches!(pane, SelectablePane::IssueList),
        "no banner → row 1 is the issue list"
    );
    assert_eq!(geo.origin_row, 1);
}

/// A notice-only banner must reduce the detail viewport rows by one — exactly
/// like an error banner — because the banner reserves one row above the
/// workspace (issue #265).
#[test]
fn notice_only_banner_reduces_issues_detail_viewport_rows() {
    let term_cols: u16 = 120;
    let term_rows: u16 = 40;

    // Baseline: no banner.
    let mut state_none = AppState::default();
    refresh_detail_viewport_rows(
        &mut state_none,
        SelectablePane::IssueDetail,
        term_cols,
        term_rows,
    );
    let rows_none = state_none.issues_state.detail_viewport_rows;

    // Notice-only banner.
    let mut state_notice = AppState {
        issues_state: IssuesState {
            draft_notice: Some("No agents available".to_string()),
            ..IssuesState::default()
        },
        ..AppState::default()
    };
    refresh_detail_viewport_rows(
        &mut state_notice,
        SelectablePane::IssueDetail,
        term_cols,
        term_rows,
    );
    let rows_notice = state_notice.issues_state.detail_viewport_rows;

    // Error-only banner (reference).
    let mut state_error = AppState {
        issues_state: IssuesState {
            error: Some("load failed".to_string()),
            ..IssuesState::default()
        },
        ..AppState::default()
    };
    refresh_detail_viewport_rows(
        &mut state_error,
        SelectablePane::IssueDetail,
        term_cols,
        term_rows,
    );
    let rows_error = state_error.issues_state.detail_viewport_rows;

    assert_eq!(
        rows_notice, rows_error,
        "notice-only and error-only banners must reserve the same viewport row"
    );
    assert_eq!(
        rows_none.saturating_sub(rows_notice),
        1,
        "a present banner must reserve exactly one viewport row vs no banner \
         (none={rows_none}, notice={rows_notice})"
    );
}

// ── Issue #265 (second review): cross-mode banner isolation ──────────────
//
// screen_layout_for must derive banner/error/filter visibility from the
// ACTIVE ScreenMode only, not OR inactive mode state. An Issues draft_notice
// surviving EnterPrsMode must not shift PR mouse geometry, and vice versa.

/// An Issues draft_notice surviving into PR mode must NOT reserve a banner
/// row for PR mouse geometry. The PR workspace must start at row 1 (same as
/// when no Issues notice exists), not row 2.
#[test]
fn issues_draft_notice_does_not_shift_pr_mode_geometry() {
    // State with Issues draft_notice but screen_mode = DashboardPullRequests.
    let state = AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        issues_state: IssuesState {
            draft_notice: Some("No agents available".to_string()),
            ..IssuesState::default()
        },
        ..AppState::default()
    };

    // In PR mode with no PR error, row 1 must be the workspace (PR list).
    // The Issues draft_notice must NOT reserve a banner row.
    let Some((pane, geo)) = resolve_pane(&state, 40, 1, 120, 40, false) else {
        panic!("expected PR list at row 1 — Issues draft_notice must not shift PR geometry");
    };
    assert!(
        matches!(pane, SelectablePane::PrList),
        "Issues draft_notice must not reserve a banner row in PR mode"
    );
    assert_eq!(geo.origin_row, 1, "PR workspace must start at row 1");
}

/// An Issues error surviving into PR mode must NOT reserve a banner row for
/// PR mouse geometry. Only the PR mode's own error should affect PR geometry.
#[test]
fn issues_error_does_not_shift_pr_mode_geometry() {
    let state = AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        issues_state: IssuesState {
            error: Some("load failed".to_string()),
            ..IssuesState::default()
        },
        ..AppState::default()
    };

    let Some((pane, geo)) = resolve_pane(&state, 40, 1, 120, 40, false) else {
        panic!("expected PR list at row 1 — Issues error must not shift PR geometry");
    };
    assert!(
        matches!(pane, SelectablePane::PrList),
        "Issues error must not reserve a banner row in PR mode"
    );
    assert_eq!(geo.origin_row, 1);
}

/// A PR error surviving into Issues mode must NOT reserve a banner row for
/// Issues mouse geometry. Only the Issues mode's own error/notice should
/// affect Issues geometry.
#[test]
fn pr_error_does_not_shift_issues_mode_geometry() {
    let state = AppState {
        screen_mode: ScreenMode::DashboardIssues,
        prs_state: PullRequestsState {
            error: Some("PR load failed".to_string()),
            ..PullRequestsState::default()
        },
        ..AppState::default()
    };

    let Some((pane, geo)) = resolve_pane(&state, 40, 1, 120, 40, false) else {
        panic!("expected issue list at row 1 — PR error must not shift Issues geometry");
    };
    assert!(
        matches!(pane, SelectablePane::IssueList),
        "PR error must not reserve a banner row in Issues mode"
    );
    assert_eq!(geo.origin_row, 1);
}

/// A PR error in PR mode must reserve a banner row (sanity check — the
/// active mode's own error still works).
#[test]
fn pr_error_in_pr_mode_shifts_pr_geometry() {
    let state = AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        prs_state: PullRequestsState {
            error: Some("PR load failed".to_string()),
            ..PullRequestsState::default()
        },
        ..AppState::default()
    };

    // Row 1 is the error banner — not selectable.
    assert!(
        resolve_pane(&state, 40, 1, 120, 40, false).is_none(),
        "PR error banner row 1 must be non-selectable"
    );
    // Row 2+ is the workspace.
    let Some((pane, geo)) = resolve_pane(&state, 40, 2, 120, 40, false) else {
        panic!("expected workspace at row 2 with PR error banner");
    };
    assert!(
        matches!(pane, SelectablePane::PrList),
        "PR error banner must shift the PR-list pane rather than another pane"
    );
    assert_eq!(geo.origin_row, 2);
}

/// Issues filter controls open must not affect PR mode geometry (cross-mode
/// filter isolation).
#[test]
fn issues_filter_open_does_not_shift_pr_mode_geometry() {
    let state = AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        issues_state: IssuesState {
            filter_ui: IssueFilterUiState {
                controls_open: true,
                ..Default::default()
            },
            ..IssuesState::default()
        },
        ..AppState::default()
    };

    let Some((pane, geo)) = resolve_pane(&state, 40, 1, 120, 40, false) else {
        panic!("expected PR list at row 1 — Issues filter must not shift PR geometry");
    };
    assert!(matches!(pane, SelectablePane::PrList));
    assert_eq!(geo.origin_row, 1);
}
