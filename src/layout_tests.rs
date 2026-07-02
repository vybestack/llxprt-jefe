//! Layout unit tests (extracted from layout.rs).
//!
//! @plan PLAN-20260624-PR-MODE.P04
//! @plan PLAN-20260624-PR-MODE.P11
//! @plan PLAN-20260624-PR-MODE.P12
//! @requirement REQ-PR-006
//! @requirement REQ-PR-009

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
    assert_eq!(issues_pane_rows(40, false, true), (9, 24));
    assert_eq!(issues_pane_rows(40, true, true), (9, 23));
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

// -------------------------------------------------------------------------
// Shared list-viewport / selection-follow helper tests (REQ-PR-006, #54/#55)
//
// @plan PLAN-20260624-PR-MODE.P04
// @requirement REQ-PR-006
// @pseudocode component-001 lines 182-196
//
// RED pure-logic tests against the P03 wrong-but-total stubs
// (list_first_visible_index returns 0; list_visible_window returns empty).
// -------------------------------------------------------------------------

/// The first visible index must ADVANCE to keep `selected` on screen once
/// `selected >= viewport_rows` (selection-follow, #55).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 182-189
#[test]
fn test_list_first_visible_index_follows_selection_past_viewport() {
    // selected=12, len=30, viewport_rows=10 → first_visible must be 3
    // (so rows 3..13 are visible and row 12 is on screen).
    let first = list_first_visible_index(12, 30, 10);
    assert_eq!(
        first, 3,
        "selected=12 with viewport_rows=10 should yield first_visible=3 (got {first})"
    );

    // selected=20, len=30, viewport_rows=10 → first_visible=11.
    let first = list_first_visible_index(20, 30, 10);
    assert_eq!(
        first, 11,
        "selected=20 with viewport_rows=10 should yield first_visible=11 (got {first})"
    );
}

/// first_visible must be 0 when `selected < viewport_rows` (top of list) or
/// when `len <= viewport_rows` (short list), and must never exceed
/// `len.saturating_sub(viewport_rows)`.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 182-189
#[test]
fn test_list_first_visible_index_clamps_at_top_and_short_lists() {
    // selected within first viewport → offset 0.
    assert_eq!(
        list_first_visible_index(3, 30, 10),
        0,
        "selected < viewport_rows should yield first_visible=0"
    );

    // Short list (len <= viewport_rows) → offset 0.
    assert_eq!(
        list_first_visible_index(2, 5, 10),
        0,
        "short list (len <= viewport_rows) should yield first_visible=0"
    );

    // Never scroll past the last full page.
    let max_first = 30usize.saturating_sub(10);
    for sel in 0..30 {
        let first = list_first_visible_index(sel, 30, 10);
        assert!(
            first <= max_first,
            "first_visible ({first}) must not exceed len-viewport_rows ({max_first}) for selected={sel}"
        );
    }

    // Selecting the LAST row must scroll to keep it visible (not offset 0).
    let first = list_first_visible_index(29, 30, 10);
    assert_eq!(
        first, 20,
        "selecting the last row (29) should yield first_visible=20 (got {first})"
    );
}

/// The visible window must contain exactly `min(viewport_rows, rows.len())`
/// rows, start at `list_first_visible_index(...)`, and include `rows[selected]`.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 190-196
#[test]
fn test_list_visible_window_returns_exact_n_rows_and_bounds() {
    let rows: Vec<u32> = (0..30).collect();

    // selected=15, viewport_rows=10 → window of 10 rows including row 15.
    let window = list_visible_window(&rows, 15, 10);
    assert_eq!(
        window.len(),
        10,
        "window must contain exactly min(viewport_rows, len) = 10 rows (got {})",
        window.len()
    );
    // Window starts at first_visible_index.
    let first = list_first_visible_index(15, 30, 10);
    assert!(
        window.first().is_some_and(|&v| v as usize == first),
        "window must start at first_visible_index ({first}): got {window:?}"
    );
    // Window includes the selected row.
    assert!(
        window.contains(&15),
        "window must include rows[selected] (15): got {window:?}"
    );

    // Short list: viewport_rows > len → window = all rows.
    let short: Vec<u32> = (0..3).collect();
    let window = list_visible_window(&short, 1, 10);
    assert_eq!(
        window.len(),
        3,
        "short list (len < viewport_rows) must render exactly len rows (got {})",
        window.len()
    );
}
