//! PR-mode GitHub client behavioral tests — list & search builders.
//!
//! Split out of `github/tests.rs` to keep each test module under the
//! source-file length policy. Covers PR list parsing and the search
//! arg/query builders.
//!
//! @plan PLAN-20260624-PR-MODE.P07
//! @requirement REQ-PR-005
//! @pseudocode component-002 lines 1-200

use crate::domain::{
    ChecksFilter, PrCheckStatus, PrFilter, PrFilterState, PrReviewState, PrState,
    ReviewDecisionFilter,
};
use crate::github::{build_pr_search_args, build_pr_search_query, parse_pull_requests_json};

/// Test-only result extension mirroring the one in `github/tests.rs`.
///
/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-005
/// @pseudocode component-002 lines 1-200
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

// =============================================================================
// PR-mode tests (PLAN-20260624-PR-MODE.P07 — RED phase)
// =============================================================================
// These behavioral tests target the pure PR parse helpers, arg/query builders,
// sort, error categorization, and send-payload assembly. They MUST fail RED
// against the P06 total stubs by ASSERTION MISMATCH (wrong/empty returned
// value), never by panic. Network is NOT unit-tested. See the phase doc
// 07-github-client-tdd.md and component-002 pseudocode for traceability.

/// A realistic GraphQL `search` nodes envelope for the PR list query (P07
/// fixture). Two PRs: an open draft and a merged non-draft, exercising the
/// full PullRequest field set incl. statusCheckRollup (contexts.nodes),
/// reviewDecision, labels, assignees, and comments.totalCount.
const PR_LIST_SEARCH_JSON: &str = r#"{
    "data": {
        "search": {
            "nodes": [
                {
                    "number": 42,
                    "title": "Add cat pictures",
                    "state": "OPEN",
                    "mergedAt": null,
                    "author": {"login": "acoliver"},
                    "updatedAt": "2026-06-15T10:00:00Z",
                    "headRefName": "feature/cats",
                    "baseRefName": "main",
                    "isDraft": true,
                    "reviewDecision": "REVIEW_REQUIRED",
                    "statusCheckRollup": {
                        "contexts": {
                            "nodes": [
                                {
                                    "__typename": "CheckRun",
                                    "name": "build",
                                    "status": "COMPLETED",
                                    "conclusion": "SUCCESS",
                                    "detailsUrl": "https://github.com/o/r/runs/1"
                                }
                            ]
                        }
                    },
                    "assignees": {"nodes": [{"login": "acoliver"}, {"login": "bob"}]},
                    "labels": {"nodes": [{"name": "enhancement"}, {"name": "ui"}]},
                    "comments": {"totalCount": 5},
                    "body": "Please add cat pictures to every screen."
                },
                {
                    "number": 17,
                    "title": "Fix startup crash",
                    "state": "MERGED",
                    "mergedAt": "2026-06-10T12:00:00Z",
                    "author": {"login": "dave"},
                    "updatedAt": "2026-06-14T09:30:00Z",
                    "headRefName": "fix/crash",
                    "baseRefName": "main",
                    "isDraft": false,
                    "reviewDecision": "APPROVED",
                    "statusCheckRollup": {
                        "contexts": {
                            "nodes": [
                                {
                                    "__typename": "CheckRun",
                                    "name": "ci",
                                    "status": "COMPLETED",
                                    "conclusion": "FAILURE",
                                    "detailsUrl": "https://github.com/o/r/runs/2"
                                }
                            ]
                        }
                    },
                    "assignees": {"nodes": []},
                    "labels": {"nodes": [{"name": "bug"}]},
                    "comments": {"totalCount": 2},
                    "body": "Crash on startup fixed."
                }
            ],
            "pageInfo": {
                "hasNextPage": true,
                "endCursor": "Y3Vyc29yOnYyOpHOABcd"
            }
        }
    }
}"#;

// =============================================================================
// PR list parsing
// =============================================================================

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-006
/// @pseudocode component-002 lines 138-156
#[test]
fn test_parse_pr_list_maps_all_fields() {
    let response =
        parse_pull_requests_json(PR_LIST_SEARCH_JSON).value_or_panic("should parse PR search");

    assert_eq!(response.pull_requests.len(), 2, "two PR nodes expected");

    let first = &response.pull_requests[0];
    assert_eq!(first.number, 42);
    assert_eq!(first.title, "Add cat pictures");
    assert_eq!(first.state, PrState::Open);
    assert_eq!(first.author_login, "acoliver");
    assert_eq!(first.updated_at, "2026-06-15T10:00:00Z");
    assert_eq!(first.head_ref, "feature/cats");
    assert_eq!(first.base_ref, "main");
    assert!(first.is_draft, "PR #42 is a draft");
    assert_eq!(
        first.review_decision,
        Some(PrReviewState::ReviewRequired),
        "reviewDecision REVIEW_REQUIRED should map"
    );
    assert_eq!(
        first.checks_status,
        PrCheckStatus::Success,
        "rollup with a SUCCESS CheckRun should aggregate to Success"
    );
    assert_eq!(first.labels_summary, "enhancement, ui");
    assert_eq!(first.assignee_summary, "acoliver, bob");
    assert_eq!(first.comment_count, 5);
}

/// Companion assertions for the second PR node in `PR_LIST_SEARCH_JSON`
/// (split from `test_parse_pr_list_maps_all_fields` to keep cognitive
/// complexity under the clippy gate — all original assertions preserved).
///
/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-006
/// @pseudocode component-002 lines 138-156
#[test]
fn test_parse_pr_list_maps_second_pr_merged_and_failure() {
    let response =
        parse_pull_requests_json(PR_LIST_SEARCH_JSON).value_or_panic("should parse PR search");

    let second = &response.pull_requests[1];
    assert_eq!(second.number, 17);
    assert_eq!(
        second.state,
        PrState::Merged,
        "state=MERGED with non-null mergedAt should map to Merged"
    );
    assert!(!second.is_draft);
    assert_eq!(
        second.checks_status,
        PrCheckStatus::Failure,
        "rollup with a FAILURE CheckRun should aggregate to Failure"
    );
}

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-007
/// @pseudocode component-002 lines 138-156
#[test]
fn test_parse_pr_list_pagination_cursor_and_has_more() {
    let response =
        parse_pull_requests_json(PR_LIST_SEARCH_JSON).value_or_panic("should parse PR search");

    // cursor must equal the real GraphQL endCursor and has_more == hasNextPage.
    // There is NO derived cursor (no updatedAt/number heuristic).
    assert_eq!(response.cursor, Some("Y3Vyc29yOnYyOpHOABcd".to_string()));
    assert!(response.has_more, "hasNextPage=true should set has_more");
}

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-014
/// @pseudocode component-002 lines 138-156
#[test]
fn test_parse_pr_list_empty_yields_empty_vec() {
    let empty = r#"{
        "data": {
            "search": {
                "nodes": [],
                "pageInfo": {"hasNextPage": false, "endCursor": null}
            }
        }
    }"#;

    let response = parse_pull_requests_json(empty).value_or_panic("should parse empty PR search");
    assert!(
        response.pull_requests.is_empty(),
        "empty nodes should yield empty vec"
    );
    assert_eq!(response.cursor, None);
    assert!(!response.has_more);
}

// =============================================================================
// PR search args / query builder
// =============================================================================

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-007
/// @requirement REQ-PR-008
/// @pseudocode component-002 lines 35-58
#[test]
fn test_build_pr_search_args_uses_graphql_search_with_after_cursor() {
    let filter = PrFilter {
        state: Some(PrFilterState::Open),
        ..PrFilter::default()
    };

    // WITHOUT a cursor: no `after` argument.
    let args_no_cursor = build_pr_search_args("owner", "repo", &filter, None, 30);
    assert!(
        args_no_cursor.iter().any(|a| a == "api"),
        "args should include api graphql"
    );
    assert!(
        args_no_cursor.iter().any(|a| a == "graphql"),
        "args should include api graphql"
    );
    assert!(
        args_no_cursor
            .iter()
            .any(|a| a.contains("search(type: ISSUE")),
        "query should target the GraphQL search endpoint"
    );
    assert!(
        args_no_cursor.iter().any(|a| a.contains("is:pr")),
        "query should filter to PRs"
    );
    assert!(
        args_no_cursor.iter().any(|a| a.contains("first=")),
        "args should carry the first= page-size param"
    );
    assert!(
        !args_no_cursor.iter().any(|a| a.starts_with("after=")),
        "no after cursor on the first page"
    );

    // WITH a cursor: the `after` argument is present and carries the cursor.
    let args_with_cursor = build_pr_search_args("owner", "repo", &filter, Some("CUR123"), 30);
    assert!(
        args_with_cursor.iter().any(|a| a == "after=CUR123"),
        "after cursor must be passed through unchanged"
    );
}

/// Regression: `gh api graphql` reserves the literal field name `query` for the
/// GraphQL document itself (`-f query=<doc>`). Binding the search string to a
/// GraphQL variable ALSO named `query` (`-F query=<string>`) makes `gh` reject
/// the request with "unexpected override existing field under \"query\"". The
/// search-string variable must therefore use a distinct name (`searchQuery`),
/// and the document must declare/reference that same `$searchQuery` variable.
///
/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-007
/// @pseudocode component-002 lines 35-58
#[test]
fn test_build_pr_search_args_avoids_query_field_name_collision() {
    let filter = PrFilter {
        state: Some(PrFilterState::Open),
        ..PrFilter::default()
    };

    for cursor in [None, Some("CUR123")] {
        let args = build_pr_search_args("owner", "repo", &filter, cursor, 30);

        // Exactly one argument is the literal GraphQL document field `query=`
        // (the `-f query=<doc>`); the search string must NOT also be bound to a
        // `query` variable, or gh rejects the request.
        let query_field_count = args.iter().filter(|a| a.starts_with("query=")).count();
        assert_eq!(
            query_field_count, 1,
            "exactly one `query=` field (the GraphQL document) is allowed; got args: {args:?}"
        );

        // The search string is bound to the distinct `searchQuery` variable.
        assert!(
            args.iter().any(|a| a.starts_with("searchQuery=")),
            "search string must be bound to the `searchQuery` variable; got args: {args:?}"
        );

        // The document declares and uses `$searchQuery`, never a bare `$query`.
        let doc = args
            .iter()
            .find(|a| a.starts_with("query="))
            .unwrap_or_else(|| panic!("missing GraphQL document field in args: {args:?}"));
        assert!(
            doc.contains("$searchQuery"),
            "GraphQL document must declare and reference `$searchQuery`; got: {doc}"
        );
        assert!(
            !doc.contains("$query"),
            "GraphQL document must not reference a bare `$query` variable; got: {doc}"
        );
    }
}

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-008
/// @pseudocode component-002 lines 59-73
#[test]
fn test_build_pr_search_query_includes_state_and_search_filters() {
    let filter = PrFilter {
        state: Some(PrFilterState::Open),
        labels: vec!["bug".to_string()],
        author: "acoliver".to_string(),
        assignee: "bob".to_string(),
        reviewer: "carol".to_string(),
        query_text: "crash".to_string(),
        ..PrFilter::default()
    };

    let query = build_pr_search_query("owner", "repo", &filter);
    assert!(query.contains("repo:owner/repo"), "query pins the repo");
    assert!(query.contains("is:pr"), "query narrows to PRs");
    assert!(
        query.contains("is:open"),
        "Open state maps to the is:open qualifier"
    );
    assert!(query.contains("label:bug"), "label qualifier emitted");
    assert!(
        query.contains("author:acoliver"),
        "author qualifier emitted"
    );
    assert!(query.contains("assignee:bob"), "assignee qualifier emitted");
    assert!(
        query.contains("review-requested:carol"),
        "reviewer maps to review-requested qualifier"
    );
    assert!(
        query.contains("crash"),
        "free-text query term is appended last"
    );
}

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-008
/// @pseudocode component-002 lines 71-71e
#[test]
fn test_build_pr_search_query_emits_draft_qualifier() {
    // Some(true) -> exact token draft:true
    let q_draft_true = build_pr_search_query(
        "owner",
        "repo",
        &PrFilter {
            is_draft: Some(true),
            ..PrFilter::default()
        },
    );
    assert!(
        q_draft_true.contains("draft:true"),
        "Some(true) must emit the draft:true qualifier"
    );

    // Some(false) -> exact token draft:false
    let q_draft_false = build_pr_search_query(
        "owner",
        "repo",
        &PrFilter {
            is_draft: Some(false),
            ..PrFilter::default()
        },
    );
    assert!(
        q_draft_false.contains("draft:false"),
        "Some(false) must emit the draft:false qualifier"
    );

    // None -> NO draft qualifier at all
    let q_draft_none = build_pr_search_query(
        "owner",
        "repo",
        &PrFilter {
            is_draft: None,
            ..PrFilter::default()
        },
    );
    assert!(
        !q_draft_none.contains("draft:"),
        "None must NOT emit any draft qualifier"
    );
}

/// Fixture-backed corroboration of the `draft` qualifier semantics: a 2-PR
/// search (one draft, one not) parses `is_draft` correctly per row.
/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-008
/// @pseudocode component-002 lines 71-71e
#[test]
fn test_parse_pr_list_is_draft_per_row() {
    let two_prs = r#"{
        "data": {
            "search": {
                "nodes": [
                    {
                        "number": 1, "title": "draft pr", "state": "OPEN", "mergedAt": null,
                        "author": {"login": "a"}, "updatedAt": "2026-06-15T10:00:00Z",
                        "headRefName": "h", "baseRefName": "main", "isDraft": true,
                        "reviewDecision": null,
                        "statusCheckRollup": {"contexts": {"nodes": []}},
                        "assignees": {"nodes": []}, "labels": {"nodes": []},
                        "comments": {"totalCount": 0}, "body": ""
                    },
                    {
                        "number": 2, "title": "ready pr", "state": "OPEN", "mergedAt": null,
                        "author": {"login": "b"}, "updatedAt": "2026-06-15T10:00:00Z",
                        "headRefName": "h", "baseRefName": "main", "isDraft": false,
                        "reviewDecision": null,
                        "statusCheckRollup": {"contexts": {"nodes": []}},
                        "assignees": {"nodes": []}, "labels": {"nodes": []},
                        "comments": {"totalCount": 0}, "body": ""
                    }
                ],
                "pageInfo": {"hasNextPage": false, "endCursor": null}
            }
        }
    }"#;
    let parsed = parse_pull_requests_json(two_prs).value_or_panic("should parse 2-PR fixture");
    assert_eq!(parsed.pull_requests.len(), 2);
    assert!(
        parsed.pull_requests[0].is_draft,
        "first row is the draft PR"
    );
    assert!(
        !parsed.pull_requests[1].is_draft,
        "second row is the non-draft PR"
    );
}

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-008
/// @pseudocode component-002 lines 71f-71u
#[test]
fn test_build_pr_search_query_emits_review_and_checks_qualifiers() {
    fn query(review: ReviewDecisionFilter, checks: ChecksFilter) -> String {
        build_pr_search_query(
            "owner",
            "repo",
            &PrFilter {
                review_decision: review,
                checks_status: checks,
                ..PrFilter::default()
            },
        )
    }

    // review_decision -> exact review: tokens; Any -> none.
    assert!(query(ReviewDecisionFilter::Approved, ChecksFilter::Any).contains("review:approved"));
    assert!(
        query(ReviewDecisionFilter::ChangesRequested, ChecksFilter::Any)
            .contains("review:changes_requested")
    );
    assert!(
        query(ReviewDecisionFilter::ReviewRequired, ChecksFilter::Any).contains("review:required")
    );
    assert!(query(ReviewDecisionFilter::None, ChecksFilter::Any).contains("review:none"));
    assert!(
        !query(ReviewDecisionFilter::Any, ChecksFilter::Any).contains("review:"),
        "Any review must emit NO review qualifier"
    );

    // checks_status -> exact status: tokens; Any -> none.
    assert!(query(ReviewDecisionFilter::Any, ChecksFilter::Success).contains("status:success"));
    assert!(query(ReviewDecisionFilter::Any, ChecksFilter::Failing).contains("status:failure"));
    assert!(query(ReviewDecisionFilter::Any, ChecksFilter::Pending).contains("status:pending"));
    assert!(
        !query(ReviewDecisionFilter::Any, ChecksFilter::Any).contains("status:"),
        "Any checks must emit NO status qualifier"
    );
}

/// Deterministic qualifier ORDER (Finding 1): state, labels, author, assignee,
/// reviewer, draft, review, checks, then free-text query.
/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-008
/// @pseudocode component-002 lines 71f-71u
#[test]
fn test_build_pr_search_query_qualifier_order_is_deterministic() {
    let full = build_pr_search_query(
        "owner",
        "repo",
        &PrFilter {
            state: Some(PrFilterState::Open),
            labels: vec!["bug".to_string()],
            author: "alice".to_string(),
            assignee: "bob".to_string(),
            reviewer: "carol".to_string(),
            is_draft: Some(true),
            review_decision: ReviewDecisionFilter::Approved,
            checks_status: ChecksFilter::Success,
            query_text: "crash".to_string(),
        },
    );
    let pos_state = full.find("is:open");
    let pos_label = full.find("label:bug");
    let pos_author = full.find("author:alice");
    let pos_assignee = full.find("assignee:bob");
    let pos_reviewer = full.find("review-requested:carol");
    let pos_draft = full.find("draft:true");
    let pos_review = full.find("review:approved");
    let pos_checks = full.find("status:success");
    let pos_free = full.find("crash");
    assert!(
        pos_state < pos_label
            && pos_label < pos_author
            && pos_author < pos_assignee
            && pos_assignee < pos_reviewer
            && pos_reviewer < pos_draft
            && pos_draft < pos_review
            && pos_review < pos_checks
            && pos_checks < pos_free,
        "qualifier order must be deterministic: state, labels, author, assignee, reviewer, draft, review, checks, free-text (got: {full})"
    );
}

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-008
/// @pseudocode component-002 lines 35-58
#[test]
fn test_build_pr_search_args_preserves_cursor_with_signal_filters() {
    let filter = PrFilter {
        review_decision: ReviewDecisionFilter::Approved,
        checks_status: ChecksFilter::Success,
        ..PrFilter::default()
    };

    // Signal filtering lives in the query string; the first/after cursor args
    // are passed through unchanged so pagination is server-side and safe.
    let args = build_pr_search_args("owner", "repo", &filter, Some("SIGCUR"), 30);
    assert!(
        args.iter().any(|a| a == "first=30"),
        "first=PR_LIST_PAGE_SIZE must be present unchanged"
    );
    assert!(
        args.iter().any(|a| a == "after=SIGCUR"),
        "after cursor must be passed through unchanged with signal filters set"
    );
}

// =============================================================================
