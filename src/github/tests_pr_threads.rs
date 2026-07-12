//!
//! Review-thread parser/query tests for issue #119.
//!
//! Split out of `github/tests_pr_detail.rs` to keep each test module under the
//! source-file length policy. Covers the `reviewThreads` GraphQL query builder
//! and the `parse_pr_review_threads` parser.
//!
//! @requirement REQ-PR-009

use crate::github::{
    build_pr_review_threads_query, parse_pr_review_threads, parse_pr_review_threads_cursor,
};

/// Test helper for unwrapping Results with a descriptive panic message.
trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
#[test]
fn test_build_pr_review_threads_query_targets_pull_request_reviews() {
    let query = build_pr_review_threads_query(false);
    assert!(
        query.contains("pullRequest(number:"),
        "thread query must target pullRequest(number:)"
    );
    assert!(
        query.contains("reviewThreads(first:"),
        "thread query must select the reviewThreads connection"
    );
    assert!(
        query.contains("isResolved"),
        "thread query must select isResolved"
    );
    assert!(
        query.contains("id"),
        "thread query must select the thread node id"
    );
    assert!(
        query.contains("databaseId"),
        "thread query must select comment databaseId"
    );
    assert!(
        !query.contains("$after"),
        "no-cursor variant must NOT include $after variable"
    );
}

/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
#[test]
fn test_build_pr_review_threads_query_with_cursor_includes_after() {
    let query = build_pr_review_threads_query(true);
    assert!(
        query.contains("$after") && query.contains("after:"),
        "with-cursor variant must include the $after/after: clauses"
    );
    assert!(
        query.contains("reviewThreads(first:"),
        "with-cursor variant must still select reviewThreads"
    );
}

/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 174-193
#[test]
fn test_parse_pr_review_threads_extracts_threads_and_comments() {
    let json = r#"{
        "data": {
            "repository": {
                "pullRequest": {
                    "reviewThreads": {
                        "nodes": [
                            {
                                "id": "PRRT_kwAAAA",
                                "isResolved": false,
                                "path": "src/lib.rs",
                                "line": 42,
                                "comments": {
                                    "nodes": [
                                        {
                                            "databaseId": 501,
                                            "author": {"login": "reviewer1"},
                                            "createdAt": "2026-07-01T10:00:00Z",
                                            "lastEditedAt": null,
                                            "body": "Please fix this line"
                                        }
                                    ]
                                }
                            }
                        ]
                    }
                }
            }
        }
    }"#;
    let value: serde_json::Value =
        serde_json::from_str(json).value_or_panic("valid review-threads JSON");
    let threads = parse_pr_review_threads(&value);
    assert_eq!(threads.len(), 1, "one thread expected");
    let thread = &threads[0];
    assert_eq!(thread.thread_id, "PRRT_kwAAAA");
    assert!(!thread.is_resolved);
    assert_eq!(thread.path.as_deref(), Some("src/lib.rs"));
    assert_eq!(thread.line, Some(42));
    assert_eq!(thread.comments.len(), 1);
    assert_eq!(thread.comments[0].comment_id, 501);
    assert_eq!(thread.comments[0].author_login, "reviewer1");
    assert_eq!(thread.comments[0].body, "Please fix this line");
    assert!(thread.comments[0].edited_at.is_none());
}

/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
#[test]
fn test_parse_pr_review_threads_resolved_thread_with_multiple_replies() {
    let json = r#"{
        "data": {
            "repository": {
                "pullRequest": {
                    "reviewThreads": {
                        "nodes": [
                            {
                                "id": "PRRT_kwBBBB",
                                "isResolved": true,
                                "path": "src/main.rs",
                                "line": 10,
                                "comments": {
                                    "nodes": [
                                        {
                                            "databaseId": 600,
                                            "author": {"login": "alice"},
                                            "createdAt": "2026-07-01T10:00:00Z",
                                            "lastEditedAt": null,
                                            "body": "nit: spacing"
                                        },
                                        {
                                            "databaseId": 601,
                                            "author": {"login": "bob"},
                                            "createdAt": "2026-07-01T11:00:00Z",
                                            "lastEditedAt": "2026-07-01T11:30:00Z",
                                            "body": "fixed"
                                        }
                                    ]
                                }
                            }
                        ]
                    }
                }
            }
        }
    }"#;
    let value: serde_json::Value =
        serde_json::from_str(json).value_or_panic("valid review-threads JSON");
    let threads = parse_pr_review_threads(&value);
    assert_eq!(threads.len(), 1);
    let thread = &threads[0];
    assert_eq!(thread.thread_id, "PRRT_kwBBBB");
    assert!(thread.is_resolved);
    assert_eq!(thread.comments.len(), 2);
    assert_eq!(thread.comments[0].author_login, "alice");
    assert_eq!(thread.comments[1].author_login, "bob");
    assert_eq!(
        thread.comments[1].edited_at.as_deref(),
        Some("2026-07-01T11:30:00Z")
    );
}

/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
/// @requirement REQ-PR-013
#[test]
fn test_parse_pr_review_threads_missing_data_yields_empty_not_panic() {
    let json = r#"{"data": {"repository": {"pullRequest": {"reviewThreads": {"nodes": []}}}}}"#;
    let value: serde_json::Value = serde_json::from_str(json).value_or_panic("valid empty JSON");
    let threads = parse_pr_review_threads(&value);
    assert!(
        threads.is_empty(),
        "empty reviewThreads nodes yields empty threads"
    );
}

/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
/// @requirement REQ-PR-013
#[test]
fn test_parse_pr_review_threads_malformed_thread_is_degraded_not_dropped() {
    // A thread node missing all fields must still yield a degraded entry.
    let json = r#"{
        "data": {
            "repository": {
                "pullRequest": {
                    "reviewThreads": {
                        "nodes": [
                            {}
                        ]
                    }
                }
            }
        }
    }"#;
    let value: serde_json::Value =
        serde_json::from_str(json).value_or_panic("valid JSON with malformed thread");
    let threads = parse_pr_review_threads(&value);
    assert_eq!(threads.len(), 1, "malformed thread must not be dropped");
    assert!(
        !threads[0].thread_id.is_empty(),
        "degraded thread must have a placeholder id"
    );
    assert!(
        threads[0].comments.is_empty(),
        "degraded thread has no comments"
    );
}

/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
/// @requirement REQ-PR-013
#[test]
fn test_parse_pr_review_threads_missing_pullrequest_yields_empty() {
    let json = r#"{"data": {"repository": {}}}"#;
    let value: serde_json::Value =
        serde_json::from_str(json).value_or_panic("valid JSON missing pullRequest");
    let threads = parse_pr_review_threads(&value);
    assert!(
        threads.is_empty(),
        "missing pullRequest yields empty threads"
    );
}

/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
/// @requirement REQ-PR-013
#[test]
fn test_parse_pr_review_threads_legacy_nested_fallback() {
    let json = r#"{
        "data": {
            "repository": {
                "pullRequest": {
                    "reviews": {
                        "nodes": [
                            {
                                "reviewThreads": {
                                    "nodes": [
                                        {
                                            "id": "PRRT_legacy",
                                            "isResolved": false,
                                            "path": "src/lib.rs",
                                            "line": 10,
                                            "comments": {
                                                "nodes": [
                                                    {
                                                        "databaseId": 99,
                                                        "author": {"login": "ghost"},
                                                        "createdAt": "2026-07-01T00:00:00Z",
                                                        "body": "legacy thread comment"
                                                    }
                                                ]
                                            }
                                        }
                                    ]
                                }
                            }
                        ]
                    }
                }
            }
        }
    }"#;
    let value: serde_json::Value =
        serde_json::from_str(json).value_or_panic("valid legacy nested JSON");
    let threads = parse_pr_review_threads(&value);
    assert_eq!(
        threads.len(),
        1,
        "legacy nested path should still parse threads"
    );
    assert_eq!(threads[0].thread_id, "PRRT_legacy");
    assert!(!threads[0].is_resolved);
    assert_eq!(threads[0].comments.len(), 1);
    assert_eq!(threads[0].comments[0].author_login, "ghost");
}

// ── Pagination + isOutdated + parent-review id (issue #155 follow-up) ────

/// The thread query must select `isOutdated`, the parent-review id on each
/// comment, and `pageInfo { hasNextPage endCursor }` so the client can
/// paginate the connection and group threads under their parent review.
#[test]
fn test_build_pr_review_threads_query_selects_outdated_review_and_page_info() {
    for with_cursor in [false, true] {
        let query = build_pr_review_threads_query(with_cursor);
        assert!(
            query.contains("isOutdated"),
            "thread query must select isOutdated (with_cursor={with_cursor})"
        );
        assert!(
            query.contains("pullRequestReview"),
            "thread query must select the parent review id (with_cursor={with_cursor})"
        );
        assert!(
            query.contains("pageInfo") && query.contains("hasNextPage"),
            "thread query must select pageInfo for pagination (with_cursor={with_cursor})"
        );
    }
}

/// `isOutdated` and the parent-review id (first comment's
/// `pullRequestReview.id`) parse onto the thread.
#[test]
fn test_parse_pr_review_threads_extracts_outdated_and_review_id() {
    let json = r#"{
        "data": {
            "repository": {
                "pullRequest": {
                    "reviewThreads": {
                        "nodes": [
                            {
                                "id": "PRRT_out1",
                                "isResolved": false,
                                "isOutdated": true,
                                "path": "src/lib.rs",
                                "line": null,
                                "comments": {
                                    "nodes": [
                                        {
                                            "databaseId": 700,
                                            "author": {"login": "coderabbit"},
                                            "createdAt": "2026-07-10T10:00:00Z",
                                            "body": "stale finding",
                                            "pullRequestReview": {"id": "PRR_parent1"}
                                        }
                                    ]
                                }
                            }
                        ],
                        "pageInfo": {"hasNextPage": false, "endCursor": "abc"}
                    }
                }
            }
        }
    }"#;
    let value: serde_json::Value =
        serde_json::from_str(json).value_or_panic("valid outdated-thread JSON");
    let threads = parse_pr_review_threads(&value);
    assert_eq!(threads.len(), 1);
    assert!(threads[0].is_outdated, "isOutdated must parse to true");
    assert_eq!(
        threads[0].review_id.as_deref(),
        Some("PRR_parent1"),
        "parent review id must come from the first comment's pullRequestReview"
    );
}

/// Threads missing `isOutdated`/`pullRequestReview` degrade to
/// `is_outdated=false` / `review_id=None` (never dropped).
#[test]
fn test_parse_pr_review_threads_missing_outdated_and_review_id_degrade() {
    let json = r#"{
        "data": {
            "repository": {
                "pullRequest": {
                    "reviewThreads": {
                        "nodes": [
                            {
                                "id": "PRRT_old",
                                "isResolved": true,
                                "path": "src/main.rs",
                                "line": 3,
                                "comments": {"nodes": [{"databaseId": 1, "author": {"login": "x"}, "createdAt": "2026-07-01T00:00:00Z", "body": "b"}]}
                            }
                        ]
                    }
                }
            }
        }
    }"#;
    let value: serde_json::Value =
        serde_json::from_str(json).value_or_panic("valid legacy-shape thread JSON");
    let threads = parse_pr_review_threads(&value);
    assert_eq!(threads.len(), 1);
    assert!(!threads[0].is_outdated, "missing isOutdated degrades false");
    assert!(
        threads[0].review_id.is_none(),
        "missing pullRequestReview degrades to None"
    );
}

/// `parse_pr_review_threads_cursor` returns the endCursor while hasNextPage
/// is true, and None on the last page (or malformed pageInfo).
#[test]
fn test_parse_pr_review_threads_cursor_follows_has_next_page() {
    let with_next = r#"{"data": {"repository": {"pullRequest": {"reviewThreads": {"nodes": [], "pageInfo": {"hasNextPage": true, "endCursor": "CUR123"}}}}}}"#;
    let value: serde_json::Value =
        serde_json::from_str(with_next).value_or_panic("valid page-info JSON");
    assert_eq!(
        parse_pr_review_threads_cursor(&value).as_deref(),
        Some("CUR123"),
        "hasNextPage=true must yield the endCursor"
    );

    let last_page = r#"{"data": {"repository": {"pullRequest": {"reviewThreads": {"nodes": [], "pageInfo": {"hasNextPage": false, "endCursor": "CUR456"}}}}}}"#;
    let value: serde_json::Value =
        serde_json::from_str(last_page).value_or_panic("valid last-page JSON");
    assert!(
        parse_pr_review_threads_cursor(&value).is_none(),
        "hasNextPage=false must yield None"
    );

    let missing = r#"{"data": {"repository": {"pullRequest": {"reviewThreads": {"nodes": []}}}}}"#;
    let value: serde_json::Value =
        serde_json::from_str(missing).value_or_panic("valid missing-pageInfo JSON");
    assert!(
        parse_pr_review_threads_cursor(&value).is_none(),
        "missing pageInfo must yield None"
    );
}

/// `hasNextPage=true` with a missing or empty `endCursor` must stop
/// pagination (None) rather than loop or panic — the truncation is logged.
#[test]
fn test_parse_pr_review_threads_cursor_next_page_without_cursor_stops() {
    let no_cursor = r#"{"data": {"repository": {"pullRequest": {"reviewThreads": {"nodes": [], "pageInfo": {"hasNextPage": true}}}}}}"#;
    let value: serde_json::Value =
        serde_json::from_str(no_cursor).value_or_panic("valid no-cursor JSON");
    assert!(
        parse_pr_review_threads_cursor(&value).is_none(),
        "hasNextPage=true without endCursor must stop pagination"
    );

    let empty_cursor = r#"{"data": {"repository": {"pullRequest": {"reviewThreads": {"nodes": [], "pageInfo": {"hasNextPage": true, "endCursor": ""}}}}}}"#;
    let value: serde_json::Value =
        serde_json::from_str(empty_cursor).value_or_panic("valid empty-cursor JSON");
    assert!(
        parse_pr_review_threads_cursor(&value).is_none(),
        "hasNextPage=true with empty endCursor must stop pagination"
    );
}

/// `hasNextPage=true` with a WHITESPACE-ONLY `endCursor` must stop
/// pagination (None) rather than return the whitespace as a bogus cursor —
/// a whitespace-only cursor would be sent to GitHub as a real pagination
/// token and silently break the thread fetch.
#[test]
fn test_parse_pr_review_threads_cursor_whitespace_only_stops() {
    let ws_cursor = r#"{"data": {"repository": {"pullRequest": {"reviewThreads": {"nodes": [], "pageInfo": {"hasNextPage": true, "endCursor": " "}}}}}}"#;
    let value: serde_json::Value =
        serde_json::from_str(ws_cursor).value_or_panic("valid whitespace-cursor JSON");
    assert!(
        parse_pr_review_threads_cursor(&value).is_none(),
        "hasNextPage=true with whitespace-only endCursor must stop pagination, not return a bogus cursor"
    );
}

/// A thread node with an empty `comments.nodes` array parses without
/// panicking: no comments, no parent review id (falls back at grouping).
#[test]
fn test_parse_pr_review_threads_empty_comments_array() {
    let json = r#"{"data": {"repository": {"pullRequest": {"reviewThreads": {"nodes": [
        {"id": "T1", "isResolved": false, "isOutdated": false, "path": "a.rs", "line": 1,
         "comments": {"nodes": []}}
    ]}}}}}"#;
    let value: serde_json::Value =
        serde_json::from_str(json).value_or_panic("valid empty-comments JSON");
    let threads = parse_pr_review_threads(&value);
    assert_eq!(threads.len(), 1, "commentless thread still parses");
    assert!(threads[0].comments.is_empty(), "no comments");
    assert!(
        threads[0].review_id.is_none(),
        "no first comment means no parent review id"
    );
}

// ── assign_threads_to_reviews grouping (issue #155 follow-up) ────────────

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

/// Threads attach to THEIR parent review (matched by review id), not all to
/// the first review — mirroring github.com's per-review grouping.
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
    let ids0: Vec<&str> = reviews[0]
        .review_threads
        .iter()
        .map(|t| t.thread_id.as_str())
        .collect();
    let ids1: Vec<&str> = reviews[1]
        .review_threads
        .iter()
        .map(|t| t.thread_id.as_str())
        .collect();
    assert_eq!(ids0, vec!["T_b"], "PRR_1's thread lands on review 0");
    assert_eq!(
        ids1,
        vec!["T_a", "T_c"],
        "PRR_2's threads land on review 1 in fetch order"
    );
}

/// A thread whose parent-review id is missing or unknown falls back to the
/// first review, so it is never dropped.
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
    let ids0: Vec<&str> = reviews[0]
        .review_threads
        .iter()
        .map(|t| t.thread_id.as_str())
        .collect();
    assert_eq!(
        ids0,
        vec!["T_none", "T_ghost"],
        "orphan threads must fall back to the first review, never dropped"
    );
    assert!(reviews[1].review_threads.is_empty());
}

/// No reviews at all: threads are dropped (no slot), and no panic.
#[test]
fn test_assign_threads_no_reviews_drops_without_panic() {
    let mut reviews: Vec<PrReview> = vec![];
    let threads = vec![thread_with_review_id("T_a", Some("PRR_1"))];
    assign_threads_to_reviews(&mut reviews, threads);
    assert!(reviews.is_empty());
}
