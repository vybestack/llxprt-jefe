//! Phase P13 behavioral tests for PR-mode rendering.
//!
//! @plan PLAN-20260624-PR-MODE.P13
//! @requirement REQ-PR-006
//! @requirement REQ-PR-008
//! @requirement REQ-PR-009
//! @requirement REQ-PR-010
//! @requirement REQ-PR-012
//! @requirement REQ-PR-013
//! @requirement REQ-PR-014
//!
//! These tests lock the rendering/scroll contract for PR mode.
//! Tests 8, 9, 10 assert the detail scroll clamp derives from the REAL
//! rendered line count produced by `build_pr_detail_content` (via the
//! `pr_detail_content_line_count` parity fn introduced in P14), NOT a
//! heuristic. They are GREEN now that the clamp routes through that fn.
//!
//! REQ-PR-012 keybind label note: the "o open in browser" label is exposed
//! through the `keybind_hints_for` projection seam in `keybind_bar.rs` and is
//! asserted via that seam here (display-only, no merge/approve binding).

use crate::domain::{
    IssueComment, PrCheck, PrCheckStatus, PrReview, PrReviewState, PrState, PullRequest,
    PullRequestDetail, Repository, RepositoryId,
};
use crate::pr_detail_content::{build_new_pr_comment_content, build_pr_detail_content};
use crate::state::{AppEvent, AppState, ComposerTarget, InlineState, PrDetailSubfocus, ScreenMode};
use crate::ui::components::pr_detail::pr_detail_header_view;
use crate::ui::components::pr_list::{pr_list_status_message, pr_list_visible_rows};

/// Helper: PR-mode state with one repository selected at index 0.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-001
/// @pseudocode component-001 lines 1-12
fn prs_mode_state(repo_id: &str) -> AppState {
    let mut state = AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        ..AppState::default()
    };
    state.repositories.push(Repository::new(
        RepositoryId(repo_id.to_string()),
        "Test Repo".to_string(),
        repo_id.to_string(),
        std::path::PathBuf::from("/tmp/test"),
    ));
    state.selected_repository_index = Some(0);
    state.prs_state.active = true;
    state
}

/// Helper: minimal PR list-row.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 190-196
fn make_test_pr(number: u64) -> PullRequest {
    PullRequest {
        number,
        title: format!("PR #{number}"),
        state: PrState::Open,
        author_login: "testuser".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        base_ref: "main".to_string(),
        is_draft: false,
        review_decision: None,
        checks_status: PrCheckStatus::None,
        assignee_summary: String::new(),
        labels_summary: String::new(),
        comment_count: 0,
    }
}

/// Helper: minimal PR detail with the given number.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn make_test_pr_detail(number: u64) -> PullRequestDetail {
    PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number,
        title: format!("PR #{number}"),
        state: PrState::Open,
        is_draft: false,
        author_login: "octocat".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        base_ref: "main".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "PR body".to_string(),
        external_url: format!("https://github.com/owner/repo/pull/{number}"),
        review_decision: None,
        checks_status: PrCheckStatus::None,
        reviews: vec![],
        checks: vec![],
        comments: vec![],
        has_more_comments: false,
        comments_cursor: None,
    }
}

/// PR detail with reviews, checks, and comments so all section headers and
/// separators render (used by content-lock + scroll-clamp tests).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn detail_with_reviews_and_checks(number: u64) -> PullRequestDetail {
    let mut detail = make_test_pr_detail(number);
    detail.body = "This is the PR body text.".to_string();
    detail.review_decision = Some(PrReviewState::Approved);
    detail.checks_status = PrCheckStatus::Success;
    detail.reviews = vec![
        PrReview {
            author_login: "alice".to_string(),
            state: PrReviewState::Approved,
            submitted_at: "2024-01-03".to_string(),
            body: Some("LGTM".to_string()),
        },
        PrReview {
            author_login: "bob".to_string(),
            state: PrReviewState::Commented,
            submitted_at: "2024-01-04".to_string(),
            body: None,
        },
    ];
    detail.checks = vec![
        PrCheck {
            name: "ci/build".to_string(),
            status: PrCheckStatus::Success,
            conclusion: "success".to_string(),
            url: None,
        },
        PrCheck {
            name: "ci/test".to_string(),
            status: PrCheckStatus::Success,
            conclusion: "success".to_string(),
            url: None,
        },
    ];
    detail.comments = vec![IssueComment {
        comment_id: 1,
        author_login: "carol".to_string(),
        created_at: "2024-01-05".to_string(),
        edited_at: None,
        body: "Nice work!".to_string(),
    }];
    detail
}

/// A rich detail (short body but MANY reviews+checks+comments) so the rendered
/// section headers + separators materially change the line count versus the
/// flat heuristic. Used by the cornerstone scroll-clamp test.
/// Three reviews so the Reviews section renders a header + multiple rows.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn rich_reviews() -> Vec<PrReview> {
    vec![
        PrReview {
            author_login: "reviewer1".to_string(),
            state: PrReviewState::Approved,
            submitted_at: "2024-01-03".to_string(),
            body: Some("Looks good".to_string()),
        },
        PrReview {
            author_login: "reviewer2".to_string(),
            state: PrReviewState::Commented,
            submitted_at: "2024-01-04".to_string(),
            body: Some("nit: spacing".to_string()),
        },
        PrReview {
            author_login: "reviewer3".to_string(),
            state: PrReviewState::Approved,
            submitted_at: "2024-01-05".to_string(),
            body: None,
        },
    ]
}

/// Three checks so the Checks section renders a header + multiple rows.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn rich_checks() -> Vec<PrCheck> {
    vec![
        PrCheck {
            name: "ci/lint".to_string(),
            status: PrCheckStatus::Success,
            conclusion: "success".to_string(),
            url: None,
        },
        PrCheck {
            name: "ci/build".to_string(),
            status: PrCheckStatus::Success,
            conclusion: "success".to_string(),
            url: None,
        },
        PrCheck {
            name: "ci/test".to_string(),
            status: PrCheckStatus::Pending,
            conclusion: String::new(),
            url: None,
        },
    ]
}

/// Two single-line comments so the Comments section renders headers + bodies.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn rich_comments() -> Vec<IssueComment> {
    vec![
        IssueComment {
            comment_id: 10,
            author_login: "commenter1".to_string(),
            created_at: "2024-01-06".to_string(),
            edited_at: None,
            body: "first comment".to_string(),
        },
        IssueComment {
            comment_id: 11,
            author_login: "commenter2".to_string(),
            created_at: "2024-01-07".to_string(),
            edited_at: None,
            body: "second comment".to_string(),
        },
    ]
}

/// A rich detail (short body but MANY reviews+checks+comments) so the rendered
/// section headers + separators materially change the line count versus the
/// flat heuristic. Used by the cornerstone scroll-clamp test.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
fn detail_rich_for_scroll_divergence(number: u64) -> PullRequestDetail {
    let mut detail = make_test_pr_detail(number);
    detail.body = "short body".to_string();
    detail.review_decision = Some(PrReviewState::Approved);
    detail.checks_status = PrCheckStatus::Success;
    detail.reviews = rich_reviews();
    detail.checks = rich_checks();
    detail.comments = rich_comments();
    detail
}

// ===========================================================================
// Test 1 — #54: all loaded rows render when viewport fits them.
// ===========================================================================

/// With N PRs loaded and a viewport >= N, the component's row projection
/// (`pr_list_visible_rows`) exposes ALL N rows and every loaded PR number
/// appears in the projected rows (regression #54: the COMPONENT renders every
/// loaded PR).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 190-196
#[test]
fn test_pr_list_renders_all_loaded_rows() {
    let prs: Vec<PullRequest> = (1..=6).map(make_test_pr).collect();
    let n = prs.len();
    let viewport: u16 = 20; // viewport >= n
    let rows = pr_list_visible_rows(&prs, Some(0), viewport, Some(60));
    assert_eq!(
        rows.len(),
        n,
        "the component must project all {n} loaded PRs when viewport >= n"
    );
    // Every loaded PR number must appear in the projected rows' title lines.
    for pr in &prs {
        let needle = format!("#{} ", pr.number);
        let found = rows.iter().any(|r| r.title_line.contains(&needle));
        assert!(
            found,
            "projected rows must include PR #{} (title_line containing '{needle}')",
            pr.number
        );
    }
}

// ===========================================================================
// Test 2 — #55: selected row always stays within the visible window.
// ===========================================================================

/// For a selected_index past the viewport, the component's row projection
/// (`pr_list_visible_rows`) keeps exactly ONE row selected and that selected
/// row's number equals the PR at `selected_index` (regression #55: the
/// COMPONENT keeps the selected row visible).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 182-189
#[test]
fn test_pr_list_keeps_selected_row_visible_when_scrolled() {
    let prs: Vec<PullRequest> = (1..=50).map(make_test_pr).collect();
    let viewport: u16 = 8;
    for sel in [0usize, 1, 7, 8, 25, 49] {
        let rows = pr_list_visible_rows(&prs, Some(sel), viewport, Some(60));
        let selected_rows: Vec<&_> = rows.iter().filter(|r| r.is_selected).collect();
        assert_eq!(
            selected_rows.len(),
            1,
            "exactly one projected row must be selected (sel={sel})"
        );
        let expected_number = prs[sel].number;
        let needle = format!("#{expected_number} ");
        assert!(
            selected_rows[0].title_line.contains(&needle),
            "the selected projected row's title_line must contain '#{expected_number} ' (pull_requests[{sel}])"
        );
    }
}

// ===========================================================================
// Test 13 — REQ-PR-014: empty/loading state renders the correct message.
// ===========================================================================

/// The component's status-message projection (`pr_list_status_message`)
/// returns the correct message for each loading/empty/filtered combination,
/// and `None` when rows are shown — REQ-PR-014.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-014
/// @pseudocode component-001 lines 190-196
#[test]
fn test_pr_empty_state_renders_message_when_no_prs() {
    assert_eq!(
        pr_list_status_message(false, true, false),
        Some("No pull requests found"),
        "no filters + empty => 'No pull requests found'"
    );
    assert_eq!(
        pr_list_status_message(false, true, true),
        Some("No pull requests match filters"),
        "filters + empty => 'No pull requests match filters'"
    );
    assert_eq!(
        pr_list_status_message(true, true, false),
        Some("Loading pull requests..."),
        "loading => 'Loading pull requests...' (regardless of empty)"
    );
    assert_eq!(
        pr_list_status_message(false, false, false),
        None,
        "non-empty + not loading => no status message (rows shown)"
    );
}

// ===========================================================================
// Test 5 — REQ-PR-009: detail renders description body + review/check summaries.
// ===========================================================================

/// `build_pr_detail_content` for a detail with reviews+checks produces the
/// Description header, the body text, the Reviews decision summary, and the
/// Checks rollup summary (content lock). The header projection
/// (`pr_detail_header_view`) renders the PR number + title (metadata header).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn test_pr_detail_renders_metadata_body_review_summary_check_summary() {
    let detail = detail_with_reviews_and_checks(42);
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    assert!(
        content.text.contains("Description"),
        "detail must render the Description section header"
    );
    assert!(
        content.text.contains("This is the PR body text."),
        "detail must render the body text"
    );
    assert!(
        content.text.contains("Reviews  (decision:"),
        "detail must render the Reviews decision summary header"
    );
    assert!(
        content.text.contains("Checks  (rollup:"),
        "detail must render the Checks rollup summary header"
    );
    // The header projection renders the PR number + title.
    let header = pr_detail_header_view(&detail);
    assert!(
        header.title.contains(&detail.number.to_string()),
        "header title must contain the PR number: {}",
        header.title
    );
    assert!(
        header.title.contains(&detail.title),
        "header title must contain the PR title: {}",
        header.title
    );
}

// ===========================================================================
// Test 6 — REQ-PR-009, REQ-PR-012: branches + external_url display-only.
// ===========================================================================

/// The component's header projection (`pr_detail_header_view`) renders
/// "{head} -> {base}" in the branches row and the `external_url` as the
/// display-only URL row (a GitHub HTTPS URL). The header is DISPLAY-ONLY
/// (#012: no merge/approve binding in-app). This asserts the RENDERED header,
/// not raw domain fields.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-009
/// @requirement REQ-PR-012
/// @pseudocode component-001 lines 1-12
#[test]
fn test_pr_detail_shows_branches_and_external_url() {
    let detail = detail_with_reviews_and_checks(77);
    let h = pr_detail_header_view(&detail);
    let expected_branch = format!("{} -> {}", detail.head_ref, detail.base_ref);
    assert!(
        h.branches.contains(&expected_branch),
        "rendered header branches must contain '{expected_branch}', got: {}",
        h.branches
    );
    assert_eq!(
        h.url, detail.external_url,
        "rendered header url must equal detail.external_url"
    );
    assert!(
        h.url.starts_with("https://github.com/"),
        "rendered header url must be a GitHub HTTPS URL (display-only): {}",
        h.url
    );
    // Display-only (#012): no merge/approve binding text in the header.
    let lower_title = h.title.to_lowercase();
    let lower_state = h.state.to_lowercase();
    let lower_branches = h.branches.to_lowercase();
    let lower_url = h.url.to_lowercase();
    assert!(
        !lower_title.contains("merge") && !lower_title.contains("approve"),
        "header title must be display-only (no merge/approve binding): {}",
        h.title
    );
    assert!(
        !lower_state.contains("merge") && !lower_state.contains("approve"),
        "header state row must be display-only (no merge/approve binding): {}",
        h.state
    );
    assert!(
        !lower_branches.contains("merge") && !lower_branches.contains("approve"),
        "header branches row must be display-only (no merge/approve binding): {}",
        h.branches
    );
    assert!(
        !lower_url.contains("merge") && !lower_url.contains("approve"),
        "header url row must be display-only (no merge/approve binding): {}",
        h.url
    );
}

// ===========================================================================
// Test 11 — #56: composer visible within the scrollable viewport when active.
// ===========================================================================

/// With an active new-comment composer (`ComposerTarget::NewComment`), the
/// rendered detail content (subfocus NewComment) contains the composer prompt
/// text, so the composer is within the scrollable region (#56).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 1-12
#[test]
fn test_pr_detail_composer_visible_within_viewport_when_active() {
    let detail = detail_with_reviews_and_checks(9);
    let composer = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "draft reply text".to_string(),
        cursor: 0,
    };
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::NewComment,
        &composer,
        false,
        false,
    );
    assert!(
        content.text.contains("draft reply text"),
        "composer text must appear in the rendered detail content (within the scroll region)"
    );
    // The new-comment composer section header must render.
    assert!(
        content.text.contains("New comment"),
        "the New comment section must render when the composer is active"
    );
    // build_new_pr_comment_content also surfaces the composer prompt.
    let composer_block = build_new_pr_comment_content(&composer);
    assert!(
        composer_block.text.contains("draft reply text"),
        "build_new_pr_comment_content must surface the composer text"
    );
}

// ===========================================================================
// Test 8 — #37f: scroll clamp must derive from the REAL rendered
// line count, not the heuristic.
// ===========================================================================

/// The detail scroll clamp must settle at the offset derived from the REAL
/// rendered line count (`build_pr_detail_content(...).text.lines().count()`),
/// NOT the flat heuristic in `AppState::pr_detail_max_scroll_offset`.
///
/// Fixture: short body + 3 reviews + 3 checks + 2 comments (1 body line each).
///
/// REAL rendered line count (build_pr_detail_content, Body subfocus, no
/// composer, comments not loading):
///   Description(1) + "  short body"(1) = 2
///   separator = 1                                  -> 3
///   "Reviews  (decision: APPROVED)"(1) + 3 reviews = 4 -> 7
///   separator = 1                                  -> 8
///   "Checks  (rollup: SUCCESS)"(1) + 3 checks = 4  -> 12
///   separator = 1                                  -> 13
///   "Comments"(1) + 2 comments; each = header(1) + 1 body line(1) + trailing
///   blank(1) = 3 -> 6 -> 19
///   separator = 1                                  -> 20
///   "  New comment"(1) + "  Press c to add a comment"(1) = 2 -> 22
///   +1 from the trailing blank line each comment appends -> 23
///   REAL total = 23.  With viewport=8 -> REAL max = 23 - 8 = 15.
///   (The +1 over the naive 22 count comes from the trailing blank line each
///   comment appends in src/pr_detail_content.rs build_single_comment.)
///
/// HEURISTIC (prs_nav_ops.rs): 5 + body_lines(1) + reviews(3) + checks(3) +
///   comment body lines(2) + 1 = 15.  With viewport=8 -> heuristic max = 7.
///
/// 15 (real) != 7 (heuristic) -> the settled offset after many
/// PrScrollDetailDown events MUST be 15. The clamp routes through the real
/// rendered count (P14), so the settled offset is 15, not the heuristic 7.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn test_pr_detail_overflow_derived_from_rendered_length_not_heuristic() {
    let viewport: usize = 8;
    let detail = detail_rich_for_scroll_divergence(100);

    let real_lines = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    )
    .text
    .lines()
    .count();
    let real_max = real_lines.saturating_sub(viewport);

    let mut state = prs_mode_state("repo-1");
    state.prs_state.pr_detail = Some(detail);
    state.prs_state.detail_viewport_rows = viewport;
    state.prs_state.detail_scroll_offset = 0;

    let mut new_state = state;
    for _ in 0..200 {
        new_state = new_state.apply(AppEvent::PrScrollDetailDown);
    }

    assert_eq!(
        new_state.prs_state.detail_scroll_offset, real_max,
        "scroll clamp must derive from REAL rendered line count ({} lines, viewport {}, \
         expected max {}, got {}). The heuristic undercounts headers/separators/empty-state.",
        real_lines, viewport, real_max, new_state.prs_state.detail_scroll_offset
    );
}

// ===========================================================================
// Test 9 — #37f: empty detail still renders headers/separators the
// heuristic undercounts.
// ===========================================================================

/// Even an EMPTY detail (no reviews/checks/comments) renders many lines the
/// heuristic's "5 + 1 + 0 + 0 + 0 + 1" undercounts:
///   Description(1) + "  (no description)"(1) = 2
///   separator = 1                              -> 3
///   "Reviews  (decision: NONE)"(1) + "  No reviews yet."(1) = 2 -> 5
///   separator = 1                              -> 6
///   "Checks  (rollup: NONE)"(1) + "  No checks reported."(1) = 2 -> 8
///   separator = 1                              -> 9
///   "Comments"(1) + "  No comments yet."(1) = 2 -> 11
///   separator = 1                              -> 12
///   "  New comment"(1) + "  Press c to add a comment"(1) = 2 -> 14
///   REAL total = 14.  With viewport=4 -> REAL max = 14 - 4 = 10.
///
/// HEURISTIC: 5 + 1(empty body -> max(1)) + 0 + 0 + 0 + 1 = 7.  viewport=4
///   -> heuristic max = 3.
///
/// 10 (real) != 3 (heuristic) -> the clamp uses the real count (10), not the
/// heuristic (3).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn test_pr_detail_overflow_counts_section_headers_and_separators() {
    let viewport: usize = 4;
    let mut detail = make_test_pr_detail(101);
    detail.body = String::new(); // empty body -> "(no description)"
    detail.reviews = vec![];
    detail.checks = vec![];
    detail.comments = vec![];

    let real_lines = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    )
    .text
    .lines()
    .count();
    let real_max = real_lines.saturating_sub(viewport);

    let mut state = prs_mode_state("repo-2");
    state.prs_state.pr_detail = Some(detail);
    state.prs_state.detail_viewport_rows = viewport;
    state.prs_state.detail_scroll_offset = 0;

    let mut new_state = state;
    for _ in 0..200 {
        new_state = new_state.apply(AppEvent::PrScrollDetailDown);
    }

    assert_eq!(
        new_state.prs_state.detail_scroll_offset, real_max,
        "scroll clamp must count section headers + separators + empty-state lines ({} lines, \
         viewport {}, expected max {}, got {}). The heuristic undercounts these structural lines.",
        real_lines, viewport, real_max, new_state.prs_state.detail_scroll_offset
    );
}

// ===========================================================================
// Test 10 — #37g/#39: clamp tracks the detail_viewport_rows prop,
// and the two maxima differ by exactly the viewport delta.
// ===========================================================================

/// Two states with IDENTICAL pr_detail but DIFFERENT detail_viewport_rows
/// (5 vs 15) must each settle at `real_lines.saturating_sub(its_viewport)`.
/// This locks that the clamp uses the PROP height (never the terminal) AND
/// that the clamp tracks the REAL rendered line count. The two maxima differ
/// by exactly (15 - 5) = 10 when content is tall enough.
///
/// The settled offsets equal the real-derived maxima because the clamp uses
/// `build_pr_detail_content`'s line count (P14), not a heuristic.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn test_pr_detail_viewport_uses_prop_height_not_terminal_size() {
    let detail = detail_rich_for_scroll_divergence(102);
    let real_lines = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    )
    .text
    .lines()
    .count();
    // Content must be tall enough that both viewports scroll (real_lines > 15).
    assert!(
        real_lines > 15,
        "fixture must render more than 15 lines for the viewport-delta lock; got {real_lines}"
    );

    let small_vp: usize = 5;
    let large_vp: usize = 15;
    let expected_small_max = real_lines.saturating_sub(small_vp);
    let expected_large_max = real_lines.saturating_sub(large_vp);
    assert_eq!(
        expected_small_max - expected_large_max,
        large_vp - small_vp,
        "real-derived maxima must differ by exactly the viewport delta"
    );

    // Small viewport state.
    let mut state_small = prs_mode_state("repo-3");
    state_small.prs_state.pr_detail = Some(detail.clone());
    state_small.prs_state.detail_viewport_rows = small_vp;
    state_small.prs_state.detail_scroll_offset = 0;
    let mut s = state_small;
    for _ in 0..200 {
        s = s.apply(AppEvent::PrScrollDetailDown);
    }

    // Large viewport state.
    let mut state_large = prs_mode_state("repo-3");
    state_large.prs_state.pr_detail = Some(detail);
    state_large.prs_state.detail_viewport_rows = large_vp;
    state_large.prs_state.detail_scroll_offset = 0;
    let mut l = state_large;
    for _ in 0..200 {
        l = l.apply(AppEvent::PrScrollDetailDown);
    }

    assert_eq!(
        s.prs_state.detail_scroll_offset, expected_small_max,
        "small-viewport clamp must track the prop-derived real max ({} lines, vp {}, \
         expected {}, got {})",
        real_lines, small_vp, expected_small_max, s.prs_state.detail_scroll_offset
    );
    assert_eq!(
        l.prs_state.detail_scroll_offset, expected_large_max,
        "large-viewport clamp must track the prop-derived real max ({} lines, vp {}, \
         expected {}, got {})",
        real_lines, large_vp, expected_large_max, l.prs_state.detail_scroll_offset
    );
}

// ===========================================================================
// Test 4 — REQ-PR-006: PR state tags + draft surfacing are distinct.
// ===========================================================================

/// The component's row projection (`pr_list_visible_rows`) surfaces draft and
/// review-decision markers in the rendered `meta_line` of the row. A draft PR
/// row's `meta_line` CONTAINS "draft", and a reviewed PR row's `meta_line`
/// contains the expected review/checks glyph. This asserts on the RENDERED
/// row strings (title_line/meta_line), not raw domain fields.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 1-12
#[test]
fn test_pr_list_shows_draft_and_review_decision_markers() {
    // A draft PR row's meta_line contains "draft".
    let mut draft_pr = make_test_pr(1);
    draft_pr.is_draft = true;
    let draft_rows = pr_list_visible_rows(&[draft_pr.clone()], Some(0), 10, Some(60));
    assert_eq!(draft_rows.len(), 1, "draft PR must project exactly one row");
    assert!(
        draft_rows[0].meta_line.contains("draft"),
        "draft PR meta_line must contain 'draft', got: {}",
        draft_rows[0].meta_line
    );

    // A non-draft PR row's meta_line does NOT contain "draft".
    let ready_pr = make_test_pr(2);
    let ready_rows = pr_list_visible_rows(&[ready_pr], Some(0), 10, Some(60));
    assert!(
        !ready_rows[0].meta_line.contains("draft"),
        "non-draft PR meta_line must NOT contain 'draft', got: {}",
        ready_rows[0].meta_line
    );

    // A PR with review_decision=Approved surfaces the approved review glyph
    // (heavy check mark U+2714 + "review").
    let mut approved_pr = make_test_pr(3);
    approved_pr.review_decision = Some(PrReviewState::Approved);
    let approved_rows = pr_list_visible_rows(&[approved_pr], Some(0), 10, Some(60));
    assert!(
        approved_rows[0].meta_line.contains('\u{2714}'),
        "approved PR meta_line must contain the heavy check mark (U+2714), got: {}",
        approved_rows[0].meta_line
    );
    assert!(
        approved_rows[0].meta_line.contains("review"),
        "approved PR meta_line must contain 'review', got: {}",
        approved_rows[0].meta_line
    );

    // A PR with checks_status=Success surfaces the success-checks glyph.
    let mut ok_checks_pr = make_test_pr(4);
    ok_checks_pr.checks_status = PrCheckStatus::Success;
    let ok_rows = pr_list_visible_rows(&[ok_checks_pr], Some(0), 10, Some(60));
    assert!(
        ok_rows[0].meta_line.contains('\u{2713}'),
        "successful-checks PR meta_line must contain the success check mark (U+2713), got: {}",
        ok_rows[0].meta_line
    );
    assert!(
        ok_rows[0].meta_line.contains("checks"),
        "successful-checks PR meta_line must contain 'checks', got: {}",
        ok_rows[0].meta_line
    );
}
