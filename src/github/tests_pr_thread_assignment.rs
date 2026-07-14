use crate::domain::{PrReview, PrReviewState, PrReviewThread};
use crate::github::assign_threads_to_reviews;

fn review_with_id(review_id: Option<&str>, author: &str) -> PrReview {
    PrReview {
        review_id: review_id.map(String::from),
        author_login: author.to_string(),
        state: PrReviewState::Commented,
        submitted_at: "2026-07-10T00:00:00Z".to_string(),
        body: None,
        review_threads: vec![],
    }
}

fn thread_with_review_id(thread_id: &str, review_id: Option<&str>) -> PrReviewThread {
    PrReviewThread {
        thread_id: thread_id.to_string(),
        is_resolved: false,
        is_outdated: false,
        review_id: review_id.map(String::from),
        path: Some("src/lib.rs".to_string()),
        line: Some(1),
        comments: vec![],
    }
}

#[test]
fn test_assign_threads_groups_by_parent_review() {
    let mut reviews = vec![
        review_with_id(Some("PRR_1"), "bot"),
        review_with_id(Some("PRR_2"), "bot"),
    ];
    let threads = vec![
        thread_with_review_id("T_a", Some("PRR_2")),
        thread_with_review_id("T_b", Some("PRR_1")),
        thread_with_review_id("T_c", Some("PRR_2")),
    ];
    assign_threads_to_reviews(&mut reviews, threads);
    let ids0 = reviews[0]
        .review_threads
        .iter()
        .map(|thread| thread.thread_id.as_str())
        .collect::<Vec<_>>();
    let ids1 = reviews[1]
        .review_threads
        .iter()
        .map(|thread| thread.thread_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids0, vec!["T_b"]);
    assert_eq!(ids1, vec!["T_a", "T_c"]);
}

#[test]
fn test_assign_threads_unknown_parent_falls_back_to_first_review() {
    let mut reviews = vec![
        review_with_id(Some("PRR_1"), "bot"),
        review_with_id(Some("PRR_2"), "bot"),
    ];
    let threads = vec![
        thread_with_review_id("T_none", None),
        thread_with_review_id("T_ghost", Some("PRR_deleted")),
    ];
    assign_threads_to_reviews(&mut reviews, threads);
    let ids = reviews[0]
        .review_threads
        .iter()
        .map(|thread| thread.thread_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["T_none", "T_ghost"]);
    assert!(reviews[1].review_threads.is_empty());
}

#[test]
fn test_assign_threads_no_reviews_drops_without_panic() {
    let mut reviews = vec![];
    let threads = vec![thread_with_review_id("T_a", Some("PRR_1"))];
    assign_threads_to_reviews(&mut reviews, threads);
    assert!(reviews.is_empty());
}
