//!
//! Review-thread parser/query tests for issue #119.
//!
//! Split out of `github/tests_pr_detail.rs` to keep each test module under the
//! source-file length policy. Covers the `reviewThreads` GraphQL query builder
//! and the `parse_pr_review_threads` parser.
//!
//! @requirement REQ-PR-009

use crate::github::{build_pr_review_threads_query, parse_pr_review_threads};

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
