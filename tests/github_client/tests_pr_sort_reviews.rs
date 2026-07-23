//! sort_pr_reviews unit tests (issue #238) — public API only.
//!
//! The thread-grouping test that needs `assign_threads_to_reviews` remains on
//! the lib target in `src/github/tests_pr_sort_reviews.rs`.

use jefe::domain::{PrReview, PrReviewState};
use jefe::github::sort_pr_reviews;

fn make_sort_review(
    review_id: Option<&str>,
    author: &str,
    submitted_at: &str,
    body: Option<&str>,
) -> PrReview {
    PrReview {
        review_id: review_id.map(str::to_string),
        author_login: author.to_string(),
        state: PrReviewState::Commented,
        submitted_at: submitted_at.to_string(),
        body: body.map(str::to_string),
        review_threads: vec![],
    }
}

/// Issue #238: reviews sort by submitted_at descending.
#[test]
fn test_sort_pr_reviews_by_submitted_at_desc() {
    let mut reviews = vec![
        make_sort_review(Some("PRR_1"), "old", "2026-07-01T10:00:00Z", Some("old")),
        make_sort_review(Some("PRR_2"), "new", "2026-07-03T10:00:00Z", Some("new")),
        make_sort_review(Some("PRR_3"), "mid", "2026-07-02T10:00:00Z", Some("mid")),
    ];
    sort_pr_reviews(&mut reviews);
    assert_eq!(
        reviews
            .iter()
            .map(|r| r.author_login.as_str())
            .collect::<Vec<_>>(),
        vec!["new", "mid", "old"]
    );
}

/// Issue #238: equal timestamps break ties by review_id descending.
#[test]
fn test_sort_pr_reviews_breaks_ties_by_review_id_desc() {
    let mut reviews = vec![
        make_sort_review(Some("PRR_1"), "a", "2026-07-01T10:00:00Z", Some("a")),
        make_sort_review(Some("PRR_3"), "c", "2026-07-01T10:00:00Z", Some("c")),
        make_sort_review(Some("PRR_2"), "b", "2026-07-01T10:00:00Z", Some("b")),
    ];
    sort_pr_reviews(&mut reviews);
    assert_eq!(
        reviews
            .iter()
            .map(|r| r.review_id.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("PRR_3"), Some("PRR_2"), Some("PRR_1")]
    );
}

/// Issue #238: when both submitted_at and review_id are missing, fall back to
/// author_login ascending for a deterministic order.
#[test]
fn test_sort_pr_reviews_falls_back_to_author_login_when_time_and_id_missing() {
    let mut reviews = vec![
        make_sort_review(None, "b", "", Some("b")),
        make_sort_review(None, "a", "", Some("a")),
        make_sort_review(None, "c", "", Some("c")),
    ];
    sort_pr_reviews(&mut reviews);
    assert_eq!(
        reviews
            .iter()
            .map(|r| r.author_login.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
}

/// Issue #238: missing/empty timestamps sort last (older side).
#[test]
fn test_sort_pr_reviews_puts_empty_timestamps_last() {
    let mut reviews = vec![
        make_sort_review(Some("PRR_1"), "missing", "", Some("missing")),
        make_sort_review(
            Some("PRR_2"),
            "newest",
            "2026-07-02T10:00:00Z",
            Some("newest"),
        ),
        make_sort_review(
            Some("PRR_3"),
            "older",
            "2026-07-01T10:00:00Z",
            Some("older"),
        ),
    ];
    sort_pr_reviews(&mut reviews);
    assert_eq!(
        reviews
            .iter()
            .map(|r| r.author_login.as_str())
            .collect::<Vec<_>>(),
        vec!["newest", "older", "missing"]
    );
}

/// Issue #238: threads stay grouped with their parent review after reordering.
#[test]
fn test_sort_pr_reviews_keeps_thread_groups_with_parent() {
    use jefe::domain::PrReviewThread;
    use jefe::github::assign_threads_to_reviews;

    let mut reviews = vec![
        make_sort_review(Some("PRR_OLD"), "old", "2026-07-01T10:00:00Z", Some("old")),
        make_sort_review(Some("PRR_NEW"), "new", "2026-07-03T10:00:00Z", Some("new")),
    ];
    let threads = vec![
        PrReviewThread {
            thread_id: "t_old".to_string(),
            is_resolved: false,
            is_outdated: false,
            review_id: Some("PRR_OLD".to_string()),
            path: Some("a.rs".to_string()),
            line: Some(1),
            comments: vec![],
        },
        PrReviewThread {
            thread_id: "t_new".to_string(),
            is_resolved: false,
            is_outdated: false,
            review_id: Some("PRR_NEW".to_string()),
            path: Some("b.rs".to_string()),
            line: Some(2),
            comments: vec![],
        },
    ];
    assign_threads_to_reviews(&mut reviews, threads);
    sort_pr_reviews(&mut reviews);

    assert_eq!(reviews[0].author_login, "new");
    assert_eq!(reviews[0].review_threads[0].thread_id, "t_new");
    assert_eq!(reviews[1].author_login, "old");
    assert_eq!(reviews[1].review_threads[0].thread_id, "t_old");
}
