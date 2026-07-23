//! Layout unit tests (extracted from layout.rs).
//!
//! @plan PLAN-20260624-PR-MODE.P04
//! @plan PLAN-20260624-PR-MODE.P11
//! @plan PLAN-20260624-PR-MODE.P12
//! @requirement REQ-PR-006
//! @requirement REQ-PR-009

use jefe::layout::*;

fn effective_render_size_for_test(cols: u16, rows: u16, fullscreen: bool) -> (u16, u16) {
    effective_render_size_for_windowed(cols, rows, !fullscreen)
}

fn compute_pty_layout_for_test(cols: u16, rows: u16, fullscreen: bool) -> PtyLayout {
    compute_pty_layout_for_windowed(cols, rows, !fullscreen)
}

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
    assert_eq!(effective_render_size_for_test(120, 40, true), (120, 40));
    assert_eq!(effective_render_size_for_test(80, 24, true), (80, 24));
}

#[test]
fn effective_render_size_windowed_subtraction() {
    assert_eq!(effective_render_size_for_test(120, 40, false), (118, 38));
    assert_eq!(effective_render_size_for_test(2, 2, false), (1, 1));
    assert_eq!(effective_render_size_for_test(1, 1, false), (1, 1));
}

#[test]
fn fullscreen_and_windowed_terminals_can_project_the_same_render_size() {
    assert_eq!(
        effective_render_size_for_test(100, 25, true),
        effective_render_size_for_test(102, 27, false)
    );
}

#[test]
fn split_layout_matches_the_rendered_filter_and_padding_bands() {
    let layout = split_layout_for_render_size(100, 25);

    assert_eq!(layout.sidebar_origin_col, 1);
    assert_eq!(layout.sidebar_origin_row, 5);
    assert_eq!(layout.sidebar_cols, 98);
    assert_eq!(layout.sidebar_rows, 18);
    assert_eq!(layout.sidebar_content_cols, 94);
}

#[test]
fn split_layout_saturates_at_tiny_render_sizes() {
    let layout = split_layout_for_render_size(2, 6);

    assert_eq!(layout.sidebar_cols, 0);
    assert_eq!(layout.sidebar_rows, 0);
    assert_eq!(layout.sidebar_content_cols, 0);
}

#[test]
fn terminal_manager_pty_layout_matches_lower_workspace_pane() {
    let layout = compute_terminal_manager_pty_layout(120, 40);
    let (_, render_rows) = effective_render_size(120, 40);
    let (list_rows, detail_rows) = actions_pane_rows(usize::from(render_rows), false, false);

    assert_eq!(
        layout.pty_cols,
        120 - LEFT_COL_WIDTH - TERMINAL_WIDGET_CHROME_COLS
    );
    assert_eq!(
        layout.pty_rows,
        u16::try_from(detail_rows).unwrap_or(u16::MAX) - TERMINAL_WIDGET_CHROME_ROWS
    );
    assert_eq!(layout.pane_col0, LEFT_COL_WIDTH + 1);
    assert_eq!(
        layout.pane_row0,
        u16::try_from(list_rows).unwrap_or(u16::MAX) + 3
    );
}

#[test]
fn compute_pty_layout_pane_origin() {
    let layout = compute_pty_layout_for_test(120, 40, true);
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
            let layout = compute_pty_layout_for_test(cols, rows, fullscreen);
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
    let layout = compute_pty_layout_for_test(120, 40, true);
    // pane_row0 = 1 (status bar) + agent_rows(10) + 2 (chrome top border + header)
    assert_eq!(layout.pane_row0, 1 + 10 + 2);
}

#[test]
fn agent_rows_rounding_half_up_windowed() {
    // Windowed: 40-2=38 render rows, 38-2=36 content rows. 25% = 9.0 → exactly 9.
    let layout = compute_pty_layout_for_test(120, 40, false);
    assert_eq!(layout.pane_row0, 1 + 9 + 2);
}

#[test]
fn compute_pty_layout_pane_row0_positive() {
    for fullscreen in [true, false] {
        let layout = compute_pty_layout_for_test(120, 40, fullscreen);
        assert!(
            layout.pane_row0 > 0,
            "pane_row0 not positive for fullscreen={fullscreen}"
        );
    }
}

#[test]
fn detail_viewport_saturates_to_physical_capacity() {
    assert_eq!(detail_viewport_rows(0), 0);
    assert_eq!(detail_viewport_rows(1), 0);
}

#[test]
fn detail_viewport_grows_with_terminal_height() {
    let small = detail_viewport_rows(24);
    let large = detail_viewport_rows(80);
    assert!(
        large > small,
        "larger terminal should yield more viewport rows ({large} > {small})"
    );
    assert!(large > small);
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
    assert_eq!(issues_pane_rows(40, false, true), (9, 24));
    assert_eq!(issues_pane_rows(40, true, true), (9, 23));
}

// ─── Banner projection (issue #265) ───────────────────────────────────────

/// The issues banner text must return the error when both error and notice
/// are present (error precedence).
#[test]
fn issues_banner_text_error_precedence_over_notice() {
    let banner = issues_banner_text(Some("load failed"), Some("No agents available"));
    assert_eq!(banner, Some("load failed"));
}

/// The issues banner text must fall back to the draft_notice when no error
/// is present.
#[test]
fn issues_banner_text_notice_fallback() {
    let banner = issues_banner_text(None, Some("No agents available"));
    assert_eq!(banner, Some("No agents available"));
}

/// The issues banner text must be None when neither error nor notice exists.
#[test]
fn issues_banner_text_none_when_both_absent() {
    let banner = issues_banner_text(None::<&str>, None::<&str>);
    assert!(banner.is_none());
}

/// A notice-only banner must drive the same pane sizing as an error banner
/// because the `error_visible` sizing parameter is derived from the SAME
/// `issues_banner_text` projection used for rendering (issue #265).
///
/// The preceding tests assert concrete projection values; this verifies that
/// the same error, notice, and absent projections drive the one-row sizing
/// contract.
#[test]
fn issues_pane_rows_banner_projection_drives_sizing() {
    let error_banner = issues_banner_text(Some("load failed"), Some("No agents available"));
    let notice_banner = issues_banner_text(None, Some("No agents available"));
    let no_banner = issues_banner_text(None::<&str>, None::<&str>);

    // Feed those projections into sizing: error and notice banners each
    // reduce the available detail row count by exactly one.
    let error_rows = issues_pane_rows(40, error_banner.is_some(), false);
    let notice_rows = issues_pane_rows(40, notice_banner.is_some(), false);
    let no_banner_rows = issues_pane_rows(40, no_banner.is_some(), false);
    assert_eq!(error_rows, notice_rows);
    assert_eq!(
        notice_rows.1 + 1,
        no_banner_rows.1,
        "a present banner must reduce the detail row count by exactly one"
    );
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
    assert_eq!(issues_detail_viewport_rows(40, true, true), 16);
}

/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
/// @pseudocode component-004 lines 156-159
#[test]
fn prs_detail_viewport_rows_match_issues_band_geometry() {
    for (rows, error_visible, filter_open) in [
        (40, false, false),
        (40, true, false),
        (40, false, true),
        (40, true, true),
        (8, true, true),
    ] {
        assert_eq!(
            prs_detail_viewport_rows(rows, error_visible, filter_open),
            issues_detail_viewport_rows(rows, error_visible, filter_open),
            "PR detail viewport must reuse the shared band geometry"
        );
    }
}

/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn pr_detail_document_viewport_reserves_composer_rows_but_preserves_document_row() {
    assert_eq!(pr_detail_document_viewport_rows(20, false), 20);
    assert_eq!(pr_detail_document_viewport_rows(20, true), 15);
    assert_eq!(pr_detail_document_viewport_rows(5, true), 1);
    assert_eq!(pr_detail_document_viewport_rows(6, true), 1);
    assert_eq!(pr_detail_document_viewport_rows(1, true), 1);
    assert_eq!(pr_detail_document_viewport_rows(0, true), 0);
}

#[test]
fn issue_detail_document_viewport_reserves_composer_rows_but_preserves_document_row() {
    assert_eq!(issue_detail_document_viewport_rows(20, false), 20);
    assert_eq!(issue_detail_document_viewport_rows(20, true), 15);
    assert_eq!(issue_detail_document_viewport_rows(5, true), 1);
    assert_eq!(issue_detail_document_viewport_rows(6, true), 1);
    assert_eq!(issue_detail_document_viewport_rows(1, true), 1);
    assert_eq!(issue_detail_document_viewport_rows(0, true), 0);
}

#[test]
fn issue_list_content_width_excludes_sidebar_and_border() {
    assert_eq!(issue_list_content_width(120), 96);
    assert_eq!(issue_list_content_width(10), 0);
}

#[test]
fn issues_detail_content_width_subtracts_sidebar_and_chrome() {
    assert_eq!(issues_detail_content_width(120), 92);
    assert_eq!(issues_detail_content_width(20), 0);
}

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn prs_pane_rows_match_issues_band_geometry() {
    for (rows, error_visible, filter_open) in [
        (40, false, false),
        (40, true, false),
        (40, false, true),
        (40, true, true),
        (8, true, true),
    ] {
        assert_eq!(
            prs_pane_rows(rows, error_visible, filter_open),
            issues_pane_rows(rows, error_visible, filter_open),
            "PR pane rows must reuse the shared band geometry"
        );
        assert_eq!(
            prs_detail_pane_rows(rows, error_visible, filter_open),
            prs_pane_rows(rows, error_visible, filter_open).1,
            "PR detail pane rows must equal the detail half of prs_pane_rows"
        );
    }
}

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn pr_layout_constants_match_issues_geometry() {
    assert_eq!(PRS_SIDEBAR_WIDTH, LEFT_COL_WIDTH);
    assert_eq!(PR_DETAIL_HEADER_ROWS, DETAIL_HEADER_ROWS);
    assert_eq!(pr_list_content_width(120), issue_list_content_width(120));
    assert_eq!(pr_list_content_width(10), 0);
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
                let layout = compute_pty_layout_for_test(cols, rows, fullscreen);
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
                let layout = compute_pty_layout_for_test(cols, rows, fullscreen);
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
                let layout = compute_pty_layout_for_test(cols, rows, fullscreen);
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
            let (_, eff_rows) = effective_render_size_for_test(cols, term_rows, fullscreen);
            let content_rows = eff_rows.saturating_sub(OUTER_BARS_HEIGHT);
            let agent_rows = expected_agent_rows(content_rows);
            let layout = compute_pty_layout_for_test(cols, term_rows, fullscreen);
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
        let actual = compute_pty_layout_for_test(cols, rows, fullscreen);
        assert_eq!(
            actual, expected,
            "golden mismatch for ({cols}x{rows}, fullscreen={fullscreen})"
        );
    }
}

// ── reveal_range_scroll_offset (#151) ───────────────────────────────────────
//
// Pure scroll-into-view math: given a content-line range [start, end], the
// current offset, and the viewport height, compute the minimal offset that
// keeps the range visible. These tests cover the no-op, above, below,
// straddle, and taller-than-viewport cases.

#[test]
fn reveal_range_noop_when_item_already_visible() {
    // item lines 3..5, viewport 0..9 (10 rows): fully visible.
    assert_eq!(reveal_range_scroll_offset(3, 5, 0, 10), 0);
    // Scrolled mid-document, item in the middle: no movement.
    assert_eq!(reveal_range_scroll_offset(12, 14, 10, 10), 10);
}

#[test]
fn reveal_range_scrolls_up_when_item_above_viewport() {
    // item lines 2..3, offset 10 (viewport 10..19): snap first line to top.
    assert_eq!(reveal_range_scroll_offset(2, 3, 10, 10), 2);
    // Single-line item far above.
    assert_eq!(reveal_range_scroll_offset(0, 0, 5, 10), 0);
}

#[test]
fn reveal_range_scrolls_down_when_item_below_viewport() {
    // item lines 20..20, offset 0, viewport 10 rows (0..9): bring last line
    // to the bottom row → offset = 20 - 9 = 11.
    assert_eq!(reveal_range_scroll_offset(20, 20, 0, 10), 11);
    // Multi-line item entirely below: anchor its last line at the bottom.
    assert_eq!(reveal_range_scroll_offset(25, 27, 0, 10), 18);
}

#[test]
fn reveal_range_handles_item_straddling_bottom_edge() {
    // item lines 8..12, offset 0, viewport 10 (0..9): line 8 is visible but
    // lines 10-12 are below. Bring line 12 to the bottom row:
    // offset = 12 - 9 = 3.
    assert_eq!(reveal_range_scroll_offset(8, 12, 0, 10), 3);
}

#[test]
fn reveal_range_anchors_top_when_item_taller_than_viewport() {
    // item lines 5..20 (16 lines), viewport 10 rows. Anchoring the bottom
    // would put the top off-screen; instead anchor the first line at the top.
    assert_eq!(reveal_range_scroll_offset(5, 20, 0, 10), 5);
    // Already scrolled: item taller than viewport but top is visible.
    assert_eq!(reveal_range_scroll_offset(5, 20, 5, 10), 5);
}

#[test]
fn reveal_range_returns_offset_when_viewport_rows_is_zero() {
    // Degenerate: no viewport → no movement (caller guards this).
    assert_eq!(reveal_range_scroll_offset(5, 10, 3, 0), 3);
}

#[test]
fn reveal_range_scrolls_up_when_item_straddles_top_edge() {
    // item lines 0..2, offset 1, viewport 10 (1..10): line 0 is above but
    // 1-2 are visible. Since item_start (0) < offset (1) but item_end (2) is
    // within the viewport, the item is NOT fully visible — it straddles the
    // top. Scroll up so line 0 is at the top.
    assert_eq!(reveal_range_scroll_offset(0, 2, 1, 10), 0);
    // item lines 3..7, offset 5, viewport 10 (5..14): lines 3-4 are scrolled
    // off the top while 5-7 are visible. Scroll up to item_start so the whole
    // item is revealed from its first line.
    assert_eq!(reveal_range_scroll_offset(3, 7, 5, 10), 3);
}
