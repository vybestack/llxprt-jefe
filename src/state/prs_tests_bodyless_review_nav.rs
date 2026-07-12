//! Bodyless-review navigation tests (issue #155).
//!
//! Reviews whose `body` is empty/None are NOT keyboard focus stops: their
//! `Review(ri)` entry is omitted from `pr_detail_subfocus_order`, while their
//! child review threads are always included in document order. This prevents
//! the PR-233 scenario where empty "COMMENTED" review summaries trap focus
//! instead of landing the user directly on the inline conversation threads.
//!
//! Split out of `prs_tests_detail.rs` to keep each module under the
//! source-size limit.

use super::prs_tests_detail::{make_review, make_thread, prs_mode_state};
use crate::domain::{IssueComment, PrReview, PrReviewState, PrReviewThread, PullRequestDetail};
use crate::state::events::AppEvent;
use crate::state::types::PrDetailSubfocus;

/// Access the `pub(super)` order builder through the sibling nav-ops module.
fn nav_order(detail: &PullRequestDetail) -> Vec<PrDetailSubfocus> {
    super::prs_nav_ops::pr_detail_subfocus_order(detail)
}

/// Helper: minimal PR detail with the given number (mirrors the one in the
/// sibling module but kept local so this module is self-contained).
fn make_test_pr_detail(number: u64) -> PullRequestDetail {
    use crate::domain::{PrCheckStatus, PrState};
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
        mergeable: None,
        merge_state_status: None,
    }
}

/// Build a bodyless COMMENTED review with `n_threads` child threads.
fn bodyless_review(author: &str, n_threads: usize) -> PrReview {
    let threads: Vec<PrReviewThread> = (0..n_threads)
        .map(|i| PrReviewThread {
            thread_id: format!("t_{author}_{i}"),
            path: Some("src/main.rs".to_string()),
            line: Some(10),
            is_resolved: false,
            is_outdated: false,
            review_id: None,
            comments: vec![IssueComment {
                comment_id: u64::try_from(i).unwrap_or(u64::MAX),
                author_login: "reviewer".to_string(),
                created_at: "2024-01-01".to_string(),
                edited_at: None,
                body: format!("thread body {i}"),
            }],
        })
        .collect();
    PrReview {
        review_id: None,
        author_login: author.to_string(),
        state: PrReviewState::Commented,
        submitted_at: "2024-01-01".to_string(),
        body: None,
        review_threads: threads,
    }
}

// ── PR 233 shape: multiple bodyless COMMENTED reviews ────────────────────

/// PR 233-shaped fixture: multiple bodyless COMMENTED reviews, each with
/// child threads. The navigation order must contain NO `Review(_)` variants
/// (all reviews are bodyless), contain ALL `ReviewThread(_)` variants in
/// parent/document order, and the order starts with Body and ends with
/// NewComment.
#[test]
fn bodyless_reviews_excluded_from_nav_order_threads_included() {
    let mut detail = make_test_pr_detail(233);
    detail.reviews = vec![
        bodyless_review("bot1", 2),
        bodyless_review("bot2", 1),
        bodyless_review("bot3", 3),
    ];
    let order = nav_order(&detail);

    // No Review variants at all.
    let has_review = order
        .iter()
        .any(|item| matches!(item, PrDetailSubfocus::Review(_)));
    assert!(
        !has_review,
        "bodyless reviews must NOT appear as focus stops, got: {order:?}"
    );

    // All ReviewThread variants present, in document order 0..=5.
    let thread_indices: Vec<usize> = order
        .iter()
        .filter_map(|item| match item {
            PrDetailSubfocus::ReviewThread(i) => Some(*i),
            _ => None,
        })
        .collect();
    assert_eq!(
        thread_indices,
        vec![0, 1, 2, 3, 4, 5],
        "all 6 threads must be in document order, got: {order:?}"
    );

    // Order starts with Body and ends with NewComment.
    assert_eq!(order.first(), Some(&PrDetailSubfocus::Body));
    assert_eq!(order.last(), Some(&PrDetailSubfocus::NewComment));
}

/// Focusing each thread in the PR 233 fixture reveals its inline body (the
/// navigation order is correct AND the renderer projects it). This is a
/// combined nav⇄renderer assertion.
#[test]
fn bodyless_review_threads_focusable_and_reveal_inline_body() {
    let mut detail = make_test_pr_detail(233);
    detail.reviews = vec![bodyless_review("bot1", 3)];
    let order = nav_order(&detail);
    // No Review, 3 threads, Body + NewComment.
    assert_eq!(
        order.len(),
        5,
        "Body + 3 threads + NewComment, got: {order:?}"
    );
    for i in 0..3 {
        let content = crate::pr_detail_content::build_pr_detail_content(
            &detail,
            PrDetailSubfocus::ReviewThread(i),
            &crate::state::InlineState::None,
            false,
            false,
        );
        let expected = format!("thread body {i}");
        assert!(
            content.text.contains(&expected),
            "focusing thread {i} must reveal its inline body"
        );
    }
}

// ── Mixed: bodyless review + review with non-empty body ──────────────────

/// Mixed fixture: a bodyless review with threads followed by a review with a
/// non-empty body and a thread. Navigation order is:
/// Body → first review's threads → Review(non-empty) → its threads → NewComment.
#[test]
fn mixed_bodyless_and_body_review_nav_order() {
    let mut detail = make_test_pr_detail(1);
    let mut body_review = make_review("human", vec![make_thread("t_human")]);
    body_review.body = Some("This review has a body".to_string());
    body_review.state = PrReviewState::Approved;
    detail.reviews = vec![bodyless_review("bot", 2), body_review];

    let order = nav_order(&detail);
    // Expected: Body, ReviewThread(0), ReviewThread(1), Review(1), ReviewThread(2), NewComment
    assert_eq!(
        order,
        vec![
            PrDetailSubfocus::Body,
            PrDetailSubfocus::ReviewThread(0),
            PrDetailSubfocus::ReviewThread(1),
            PrDetailSubfocus::Review(1),
            PrDetailSubfocus::ReviewThread(2),
            PrDetailSubfocus::NewComment,
        ],
        "mixed nav order mismatch, got: {order:?}"
    );
}

/// Whitespace-only review body is treated as bodyless (excluded from nav).
#[test]
fn whitespace_only_body_review_excluded_from_nav() {
    let mut detail = make_test_pr_detail(1);
    let mut review = make_review("whitespace", vec![make_thread("t_ws")]);
    review.body = Some("   \n\t  ".to_string());
    detail.reviews = vec![review];
    let order = nav_order(&detail);
    let has_review = order
        .iter()
        .any(|item| matches!(item, PrDetailSubfocus::Review(_)));
    assert!(
        !has_review,
        "whitespace-only body review must be excluded, got: {order:?}"
    );
}

/// A review with a non-empty body IS a focus stop (not excluded).
#[test]
fn review_with_nonempty_body_is_focus_stop() {
    let mut detail = make_test_pr_detail(1);
    let mut review = make_review("hasbody", vec![]);
    review.body = Some("Meaningful review body".to_string());
    detail.reviews = vec![review];
    let order = nav_order(&detail);
    assert!(
        order
            .iter()
            .any(|item| matches!(item, PrDetailSubfocus::Review(0))),
        "review with non-empty body must be a focus stop, got: {order:?}"
    );
}

/// Subfocus-next through a bodyless-review fixture does not land on a Review
/// focus (it goes Body → Thread0 → Thread1 → ... → NewComment). This tests
/// the full reducer path, not just the order list.
#[test]
fn subfocus_next_skips_bodyless_review_focus_in_reducer() {
    let mut state = prs_mode_state("repo-1");
    let mut detail = make_test_pr_detail(1);
    detail.reviews = vec![bodyless_review("bot", 2)];
    state.prs_state.pr_detail = Some(detail);
    state.prs_state.detail_subfocus = PrDetailSubfocus::Body;
    state.prs_state.detail_viewport_rows = 100;

    // Body → ReviewThread(0) (skipping the bodyless review).
    let s = state.apply(AppEvent::PrDetailSubfocusNext);
    assert_eq!(
        s.prs_state.detail_subfocus,
        PrDetailSubfocus::ReviewThread(0),
        "Body → ReviewThread(0), not Review(0)"
    );
    // ReviewThread(0) → ReviewThread(1).
    let s = s.apply(AppEvent::PrDetailSubfocusNext);
    assert_eq!(
        s.prs_state.detail_subfocus,
        PrDetailSubfocus::ReviewThread(1)
    );
    // ReviewThread(1) → NewComment.
    let s = s.apply(AppEvent::PrDetailSubfocusNext);
    assert_eq!(s.prs_state.detail_subfocus, PrDetailSubfocus::NewComment);
}
