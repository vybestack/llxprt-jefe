//! PR-mode GitHub client behavioral tests — detail, state, sort & payload.
//!
//! Split out of `github/tests.rs` to keep each test module under the
//! source-file length policy. Covers PR detail parsing, state/variant
//! mapping, sort, comments query seam, send-payload assembly, and error
//! categorization.
//!
//! @plan PLAN-20260624-PR-MODE.P07
//! @requirement REQ-PR-005
//! @pseudocode component-002 lines 1-200

use crate::domain::{
    IssueComment, PrCheck, PrCheckStatus, PrReview, PrReviewState, PrState, PullRequest,
    PullRequestDetail,
};
use crate::github::{
    GhClient, GhError, PrSendPayload, build_pr_comments_query, categorize_error,
    parse_check_status, parse_checks_rollup, parse_comments_json, parse_created_comment_json,
    parse_pr_check, parse_pr_review, parse_pr_state, parse_pull_request_detail_json,
    parse_pull_requests_json, parse_review_decision, rollup_nodes, sort_pull_requests,
};
use serde_json::json;

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

/// `gh pr view --json` detail fixture (P07). The PR --json set deliberately
/// OMITS `comments` — comments are sourced separately via list_pr_comments.
const PR_DETAIL_JSON: &str = r#"{
    "number": 42,
    "title": "Add cat pictures",
    "state": "OPEN",
    "mergedAt": null,
    "author": {"login": "acoliver"},
    "createdAt": "2026-06-01T08:00:00Z",
    "updatedAt": "2026-06-15T10:00:00Z",
    "headRefName": "feature/cats",
    "baseRefName": "main",
    "isDraft": true,
    "labels": [{"name": "enhancement"}],
    "assignees": [{"login": "acoliver"}],
    "milestone": {"title": "v2.0"},
    "body": "Please add cat pictures to every screen.",
    "url": "https://github.com/owner/repo/pull/42",
    "reviewDecision": "APPROVED",
    "statusCheckRollup": [
        {
            "__typename": "CheckRun",
            "name": "build",
            "status": "COMPLETED",
            "conclusion": "SUCCESS",
            "detailsUrl": "https://github.com/owner/repo/runs/1"
        },
        {
            "__typename": "StatusContext",
            "context": "legacy/coverage",
            "state": "SUCCESS",
            "targetUrl": "https://codecov.io/o/r"
        }
    ],
    "reviews": [
        {
            "author": {"login": "reviewer1"},
            "state": "APPROVED",
            "submittedAt": "2026-06-14T10:00:00Z",
            "body": "LGTM"
        }
    ],
    "mergeable": true,
    "mergeStateStatus": "MERGEABLE"
}"#;

/// Captured `repository.pullRequest.comments` JSON envelope (P07 fixture):
/// nodes (oldest→newest) + pageInfo{hasNextPage,endCursor} + totalCount.
const PR_COMMENTS_JSON: &str = r#"{
    "data": {
        "repository": {
            "pullRequest": {
                "comments": {
                    "nodes": [
                        {
                            "id": "IC_111",
                            "databaseId": 111,
                            "author": {"login": "alice"},
                            "createdAt": "2026-06-14T09:00:00Z",
                            "lastEditedAt": null,
                            "body": "First PR comment"
                        },
                        {
                            "id": "IC_222",
                            "databaseId": 222,
                            "author": {"login": "bob"},
                            "createdAt": "2026-06-14T10:00:00Z",
                            "lastEditedAt": "2026-06-14T11:00:00Z",
                            "body": "Second PR comment edited"
                        }
                    ],
                    "pageInfo": {
                        "hasNextPage": true,
                        "endCursor": "Y3Vyc29yOnYyOpHOCUR"
                    },
                    "totalCount": 2
                }
            }
        }
    }
}"#;

/// Captured REST created-comment JSON (POST .../issues/{number}/comments).
const PR_CREATED_COMMENT_JSON: &str = r#"{
    "id": 999,
    "html_url": "https://github.com/owner/repo/issues/42#issuecomment-999",
    "user": {"login": "acoliver"},
    "created_at": "2026-06-15T11:00:00Z",
    "updated_at": "2026-06-15T11:00:00Z",
    "body": "New PR comment via REST"
}"#;

/// Heterogeneous statusCheckRollup fixture (Finding 8): ONE CheckRun entry
/// AND ONE StatusContext entry, each carrying its own field shape.
const PR_ROLLUP_HETEROGENEOUS_JSON: &str = r#"[
    {
        "__typename": "CheckRun",
        "name": "build",
        "status": "COMPLETED",
        "conclusion": "SUCCESS",
        "detailsUrl": "https://github.com/o/r/runs/10"
    },
    {
        "__typename": "StatusContext",
        "context": "legacy/coverage",
        "state": "SUCCESS",
        "targetUrl": "https://codecov.io/o/r"
    }
]"#;

/// Build a full PullRequestDetail literal (the type does NOT derive Default).
fn sample_pr_detail() -> PullRequestDetail {
    PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 42,
        title: "Add cat pictures".to_string(),
        state: PrState::Open,
        is_draft: true,
        author_login: "acoliver".to_string(),
        created_at: "2026-06-01T08:00:00Z".to_string(),
        updated_at: "2026-06-15T10:00:00Z".to_string(),
        head_ref: "feature/cats".to_string(),
        base_ref: "main".to_string(),
        labels: vec!["enhancement".to_string()],
        assignees: vec!["acoliver".to_string()],
        milestone: Some("v2.0".to_string()),
        body: "Please add cat pictures to every screen.".to_string(),
        external_url: "https://github.com/owner/repo/pull/42".to_string(),
        review_decision: Some(PrReviewState::Approved),
        checks_status: PrCheckStatus::Success,
        reviews: vec![PrReview {
            review_id: None,
            author_login: "reviewer1".to_string(),
            state: PrReviewState::Approved,
            submitted_at: "2026-06-14T10:00:00Z".to_string(),
            body: Some("LGTM".to_string()),
            review_threads: vec![],
        }],
        checks: vec![PrCheck {
            name: "build".to_string(),
            status: PrCheckStatus::Success,
            conclusion: "SUCCESS".to_string(),
            url: Some("https://github.com/owner/repo/runs/1".to_string()),
        }],
        comments: crate::domain::PaginatedList::from_loaded(
            crate::domain::CommentDetailIdentity {
                scope_repo_id: crate::domain::RepositoryId::default(),
                number: 42,
            },
            vec![],
            crate::domain::PageToken::from_cursor(None, false),
        ),
        mergeable: Some(true),
        merge_state_status: Some("MERGEABLE".to_string()),
    }
}

// PR detail parsing
// =============================================================================

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-009
/// @requirement REQ-PR-012
/// @pseudocode component-002 lines 157-166
#[test]
fn test_parse_pr_detail_maps_body_branches_and_external_url() {
    let result = parse_pull_request_detail_json(PR_DETAIL_JSON, "owner/repo");
    assert!(
        result.is_ok(),
        "should parse PR detail: {result:?} (PullRequestDetail does not implement Debug; \
         failure here means the parse stub returned Err)"
    );
    if let Ok(detail) = result {
        assert_eq!(detail.number, 42);
        assert_eq!(detail.title, "Add cat pictures");
        assert_eq!(detail.body, "Please add cat pictures to every screen.");
        assert_eq!(detail.head_ref, "feature/cats");
        assert_eq!(detail.base_ref, "main");
        assert_eq!(detail.repo_owner_name, "owner/repo");
        assert_eq!(
            detail.external_url, "https://github.com/owner/repo/pull/42",
            "external_url comes from the url field"
        );
        assert_eq!(detail.author_login, "acoliver");
        assert_eq!(detail.milestone, Some("v2.0".to_string()));
        assert_eq!(detail.labels, vec!["enhancement"]);
        assert_eq!(detail.assignees, vec!["acoliver"]);
        assert!(detail.is_draft, "detail fixture PR #42 is a draft");
    }
}

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 157-180
#[test]
fn test_parse_pr_detail_reviews_summary() {
    let result = parse_pull_request_detail_json(PR_DETAIL_JSON, "owner/repo");
    assert!(result.is_ok(), "should parse PR detail: {result:?}");
    if let Ok(detail) = result {
        assert_eq!(
            detail.reviews.len(),
            1,
            "one review node should map to one PrReview"
        );
        let review = &detail.reviews[0];
        assert_eq!(review.author_login, "reviewer1");
        assert_eq!(review.state, PrReviewState::Approved);
        assert_eq!(review.submitted_at, "2026-06-14T10:00:00Z");
        assert_eq!(review.body, Some("LGTM".to_string()));

        assert_eq!(
            detail.review_decision,
            Some(PrReviewState::Approved),
            "reviewDecision APPROVED maps to Approved"
        );
    }
}

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 157-193
#[test]
fn test_parse_pr_detail_checks_summary_from_status_rollup() {
    let result = parse_pull_request_detail_json(PR_DETAIL_JSON, "owner/repo");
    assert!(result.is_ok(), "should parse PR detail: {result:?}");
    if let Ok(detail) = result {
        // The fixture rollup has a SUCCESS CheckRun + a SUCCESS StatusContext.
        assert_eq!(
            detail.checks.len(),
            2,
            "both rollup entries should map to PrCheck (none dropped)"
        );
        assert_eq!(
            detail.checks_status,
            PrCheckStatus::Success,
            "all-success rollup aggregates to Success"
        );
    }
}

/// MED-4: `checks_status` MUST be derived from the SAME rollup nodes as the
/// parsed `checks` vec (no double-compute drift). We assert the parity
/// invariant: applying `parse_checks_rollup` to the rollup nodes yields the
/// same status the detail carries, and that status is consistent with the
/// individual parsed checks. This guards against the two computations
/// diverging if the rollup source ever changes between the two calls.
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 157-166
#[test]
fn test_parse_pr_detail_checks_status_consistent_with_parsed_checks() {
    let result = parse_pull_request_detail_json(PR_DETAIL_JSON, "owner/repo");
    let detail = result.value_or_panic("should parse PR detail");

    // Re-extract the rollup nodes from the SAME fixture and compute the
    // canonical status from them.
    let value: serde_json::Value =
        serde_json::from_str(PR_DETAIL_JSON).value_or_panic("fixture is valid JSON");
    // Normalize via the SAME helper production uses (`rollup_nodes`) so this
    // no-drift guard exercises the connection-shape normalization the MED-4
    // fix relies on, not just the flat-array fixture coincidence.
    let nodes = rollup_nodes(value.get("statusCheckRollup"));
    let expected_status = parse_checks_rollup(&nodes);

    assert_eq!(
        detail.checks_status, expected_status,
        "checks_status MUST match parse_checks_rollup over the same rollup nodes (no drift)"
    );
    // The number of parsed checks MUST equal the number of rollup nodes.
    assert_eq!(
        detail.checks.len(),
        nodes.len(),
        "checks vec and rollup nodes must have the same length"
    );
}

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-009
/// @requirement REQ-PR-013
/// @pseudocode component-002 lines 167-193,205-222
#[test]
fn test_parse_status_rollup_handles_checkrun_and_statuscontext_shapes() {
    // Fixture-driven: parse the heterogeneous rollup array directly via the
    // per-node parser, asserting BOTH shapes map correctly and NEITHER is
    // dropped.
    let nodes: Vec<serde_json::Value> =
        serde_json::from_str(PR_ROLLUP_HETEROGENEOUS_JSON).value_or_panic("should parse rollup");
    assert_eq!(nodes.len(), 2, "fixture has two entries");

    let check_run = parse_pr_check(&nodes[0]);
    assert_eq!(check_run.name, "build", "CheckRun uses the name field");
    assert_eq!(
        check_run.status,
        PrCheckStatus::Success,
        "CheckRun conclusion SUCCESS maps to Success"
    );
    assert_eq!(check_run.conclusion, "SUCCESS");
    assert_eq!(
        check_run.url,
        Some("https://github.com/o/r/runs/10".to_string()),
        "CheckRun uses detailsUrl"
    );

    let status_context = parse_pr_check(&nodes[1]);
    assert_eq!(
        status_context.name, "legacy/coverage",
        "StatusContext uses the context field as name"
    );
    assert_eq!(
        status_context.status,
        PrCheckStatus::Success,
        "StatusContext state SUCCESS maps to Success"
    );
    assert_eq!(status_context.conclusion, "SUCCESS");
    assert_eq!(
        status_context.url,
        Some("https://codecov.io/o/r".to_string()),
        "StatusContext uses targetUrl"
    );

    // Aggregate over both nodes.
    assert_eq!(
        parse_checks_rollup(&nodes),
        PrCheckStatus::Success,
        "all-success heterogeneous rollup aggregates to Success"
    );
}

// =============================================================================
// PR state mapping (Finding 3)
// =============================================================================

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-006
/// @pseudocode component-002 lines 197-201
#[test]
fn test_parse_pr_state_merged_from_state_enum_and_mergedat() {
    // state=MERGED -> Merged
    assert_eq!(
        parse_pr_state(&json!("MERGED"), &json!("2026-06-10T12:00:00Z")),
        PrState::Merged
    );
    // CLOSED + null mergedAt -> Closed
    assert_eq!(
        parse_pr_state(&json!("CLOSED"), &json!(null)),
        PrState::Closed
    );
    // non-null mergedAt + ambiguous state -> Merged (mergedAt backstop)
    assert_eq!(
        parse_pr_state(&json!("CLOSED"), &json!("2026-06-10T12:00:00Z")),
        PrState::Merged,
        "non-null mergedAt must force Merged even if state is ambiguous"
    );
    // OPEN -> Open
    assert_eq!(parse_pr_state(&json!("OPEN"), &json!(null)), PrState::Open);
}

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-006
/// @pseudocode component-002 lines 197-201
#[test]
fn test_parse_pr_state_open_closed_merged() {
    assert_eq!(parse_pr_state(&json!("OPEN"), &json!(null)), PrState::Open);
    assert_eq!(
        parse_pr_state(&json!("CLOSED"), &json!(null)),
        PrState::Closed
    );
    assert_eq!(
        parse_pr_state(&json!("MERGED"), &json!("2026-06-10T12:00:00Z")),
        PrState::Merged
    );
}

// =============================================================================
// Degraded-placeholder parsing (no silent drops — REQ-PR-013)
// =============================================================================

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-009
/// @requirement REQ-PR-013
/// @pseudocode component-002 lines 174-180
#[test]
fn test_parse_malformed_review_yields_degraded_placeholder_not_dropped() {
    // A review node missing author/state/body must still yield a displayable
    // degraded PrReview — never dropped.
    let malformed = json!({});
    let review = parse_pr_review(&malformed);
    // Retained as a displayable record (count preserved upstream by virtue of
    // the total function returning a value rather than None/empty-vec).
    assert!(
        !review.author_login.is_empty(),
        "malformed review must keep a displayable author placeholder"
    );
    // Body is Option<String>; degraded form may be None but the record exists.
    let _ = review.body;
}

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-009
/// @requirement REQ-PR-013
/// @pseudocode component-002 lines 181-193
#[test]
fn test_parse_malformed_check_yields_degraded_placeholder_not_dropped() {
    // A check node missing all fields must still yield a displayable degraded
    // PrCheck — never dropped.
    let malformed = json!({});
    let check = parse_pr_check(&malformed);
    assert!(
        !check.name.is_empty(),
        "malformed check must keep a displayable name placeholder"
    );
}

// =============================================================================
// review-decision and check-status variant mapping
// =============================================================================

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 202-204
#[test]
fn test_parse_review_decision_variants() {
    assert_eq!(
        parse_review_decision(&json!("APPROVED")),
        Some(PrReviewState::Approved)
    );
    assert_eq!(
        parse_review_decision(&json!("CHANGES_REQUESTED")),
        Some(PrReviewState::ChangesRequested)
    );
    assert_eq!(
        parse_review_decision(&json!("REVIEW_REQUIRED")),
        Some(PrReviewState::ReviewRequired)
    );
    assert_eq!(
        parse_review_decision(&json!(null)),
        None,
        "null reviewDecision maps to None"
    );
    assert_eq!(
        parse_review_decision(&json!("")),
        None,
        "empty reviewDecision maps to None"
    );
}

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 205-222
#[test]
fn test_parse_status_rollup_variants() {
    // Per-token mapping (parse_check_status).
    assert_eq!(parse_check_status("SUCCESS"), PrCheckStatus::Success);
    assert_eq!(parse_check_status("FAILURE"), PrCheckStatus::Failure);
    assert_eq!(parse_check_status("PENDING"), PrCheckStatus::Pending);
    assert_eq!(parse_check_status("NEUTRAL"), PrCheckStatus::Neutral);

    // Aggregate (parse_checks_rollup) over single-node lists.
    let success = json!([{"conclusion": "SUCCESS"}]);
    assert_eq!(
        parse_checks_rollup(&[success[0].clone()]),
        PrCheckStatus::Success
    );
    let failure = json!([{"conclusion": "FAILURE"}]);
    assert_eq!(
        parse_checks_rollup(&[failure[0].clone()]),
        PrCheckStatus::Failure
    );
    let pending = json!([{"status": "IN_PROGRESS"}]);
    assert_eq!(
        parse_checks_rollup(&[pending[0].clone()]),
        PrCheckStatus::Pending
    );
    let neutral = json!([{"conclusion": "NEUTRAL"}]);
    assert_eq!(
        parse_checks_rollup(&[neutral[0].clone()]),
        PrCheckStatus::Neutral
    );
}

// =============================================================================
// Sort
// =============================================================================

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-006
/// @pseudocode component-002 lines 194-196
#[test]
fn test_sort_pull_requests_by_updated_desc() {
    let mut prs = vec![
        PullRequest {
            number: 3,
            title: "old".to_string(),
            state: PrState::Open,
            author_login: "a".to_string(),
            updated_at: "2026-06-01T10:00:00Z".to_string(),
            head_ref: "h".to_string(),
            base_ref: "main".to_string(),
            is_draft: false,
            review_decision: None,
            checks_status: PrCheckStatus::None,
            assignee_summary: String::new(),
            labels_summary: String::new(),
            comment_count: 0,
        },
        PullRequest {
            number: 1,
            title: "newest".to_string(),
            state: PrState::Open,
            author_login: "b".to_string(),
            updated_at: "2026-06-15T10:00:00Z".to_string(),
            head_ref: "h".to_string(),
            base_ref: "main".to_string(),
            is_draft: false,
            review_decision: None,
            checks_status: PrCheckStatus::None,
            assignee_summary: String::new(),
            labels_summary: String::new(),
            comment_count: 0,
        },
        PullRequest {
            number: 2,
            title: "same time lower number".to_string(),
            state: PrState::Open,
            author_login: "c".to_string(),
            updated_at: "2026-06-15T10:00:00Z".to_string(),
            head_ref: "h".to_string(),
            base_ref: "main".to_string(),
            is_draft: false,
            review_decision: None,
            checks_status: PrCheckStatus::None,
            assignee_summary: String::new(),
            labels_summary: String::new(),
            comment_count: 0,
        },
    ];

    sort_pull_requests(&mut prs);

    // updated_at DESC, then number ASC tiebreak.
    assert_eq!(prs[0].number, 1);
    assert_eq!(prs[1].number, 2);
    assert_eq!(prs[2].number, 3);
}

// =============================================================================
// PR comments query seam (REQ-PR-010 — pullRequest, NOT issue)
// =============================================================================

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-010
/// @pseudocode component-002 lines 102-107
#[test]
fn test_list_pr_comments_query_targets_pull_request_not_issue() {
    // The PR comments query MUST target repository.pullRequest(number:), NOT
    // repository.issue(number:) (which is NULL for a PR number). It must also
    // select comments(first:) with pageInfo { hasNextPage endCursor }, and
    // pass the $after/after: cursor through when one is supplied.
    let query_with_cursor = build_pr_comments_query(true);
    assert!(
        query_with_cursor.contains("pullRequest("),
        "PR comments query must target the pullRequest object path"
    );
    assert!(
        query_with_cursor.contains("repository("),
        "query must open the repository(...) object"
    );
    assert!(
        query_with_cursor.contains("pullRequest(number:"),
        "query must select pullRequest(number:)"
    );
    assert!(
        !query_with_cursor.contains("issue(number:"),
        "PR comments query must NOT target the issue object path"
    );
    assert!(
        query_with_cursor.contains("comments(first:"),
        "query must select the comments connection"
    );
    assert!(
        query_with_cursor.contains("pageInfo"),
        "query must select pageInfo"
    );
    assert!(
        query_with_cursor.contains("hasNextPage") && query_with_cursor.contains("endCursor"),
        "pageInfo must select hasNextPage and endCursor"
    );
    assert!(
        query_with_cursor.contains("$after") && query_with_cursor.contains("after:"),
        "with_cursor variant must include the $after/after: clauses"
    );

    let query_no_cursor = build_pr_comments_query(false);
    assert!(
        query_no_cursor.contains("pullRequest(number:"),
        "no-cursor variant still targets pullRequest"
    );
    assert!(
        !query_no_cursor.contains("$after"),
        "no-cursor variant must NOT include the $after variable"
    );
    assert!(
        !query_no_cursor.contains("after:"),
        "no-cursor variant must NOT include the after: argument"
    );
}

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-007
/// @requirement REQ-PR-010
/// @pseudocode component-002 lines 102-107
#[test]
fn test_list_pr_comments_parses_comments_and_pageinfo() {
    // Reframed as a pure parse test (network is not unit-tested): feed a
    // captured repository.pullRequest.comments JSON envelope through the
    // REUSED parse_comments_json + parse_page_info and assert the decoded
    // comments (oldest→newest), real cursor==endCursor, has_more==hasNextPage.
    // This proves node-shape compatibility with IssueComment.
    let result = parse_comments_json(PR_COMMENTS_JSON);
    assert!(
        result.is_ok(),
        "parse_comments_json should accept the pullRequest comments envelope: {result:?}"
    );
    if let Ok((comments, cursor, has_more)) = result {
        assert_eq!(comments.len(), 2, "two comment nodes expected");
        assert_eq!(comments[0].comment_id, 111, "oldest first");
        assert_eq!(comments[0].author_login, "alice");
        assert_eq!(comments[0].body, "First PR comment");
        assert_eq!(comments[1].comment_id, 222, "newest second");
        assert_eq!(comments[1].author_login, "bob");
        assert_eq!(comments[1].body, "Second PR comment edited");
        assert_eq!(
            cursor,
            Some("Y3Vyc29yOnYyOpHOCUR".to_string()),
            "cursor must equal the real endCursor"
        );
        assert!(has_more, "has_more must equal hasNextPage");
    }
}

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-010
/// @pseudocode component-002 lines 108-114
#[test]
fn test_create_pr_comment_parses_created_comment() {
    // CREATE transport is unchanged from the issue path (REST
    // /issues/{number}/comments accepts a PR number). Reframed as a
    // parse_created_comment_json test.
    let comment = parse_created_comment_json(PR_CREATED_COMMENT_JSON)
        .value_or_panic("should parse created PR comment");
    assert_eq!(comment.comment_id, 999);
    assert_eq!(comment.author_login, "acoliver");
    assert_eq!(comment.body, "New PR comment via REST");
    assert_eq!(comment.created_at, "2026-06-15T11:00:00Z");
}

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 74-101
#[test]
fn test_get_pull_request_detail_sources_comments_via_list_pr_comments() {
    // The PR --json set OMITS comments, so parse_pull_request_detail_json must
    // yield an empty comments list with an exhausted continuation, proving
    // comments MUST be sourced from the separate list_pr_comments fetch.
    // This is the pure, testable expression of that
    // requirement.
    let result = parse_pull_request_detail_json(PR_DETAIL_JSON, "owner/repo");
    assert!(result.is_ok(), "should parse PR detail: {result:?}");
    if let Ok(detail) = result {
        assert!(
            detail.comments.is_empty(),
            "PR detail JSON omits comments; comments must be sourced separately"
        );
        assert!(
            !detail.comments.has_more(),
            "comments are exhausted until list_pr_comments populates them"
        );
        assert_eq!(
            detail.comments.next_page(),
            &crate::domain::PageToken::Done,
            "continuation is exhausted until list_pr_comments populates comments"
        );
    }
}

// =============================================================================
// PrSendPayload assembly
// =============================================================================

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-011
/// @pseudocode component-002 lines 123-136
#[test]
fn test_build_pr_send_payload_with_focused_comment() {
    let detail = sample_pr_detail();
    let focused_comment = IssueComment {
        comment_id: 123,
        author_login: "carol".to_string(),
        created_at: "2026-06-14T10:00:00Z".to_string(),
        edited_at: None,
        body: "This is the focused PR comment".to_string(),
    };

    let payload = GhClient::build_pr_send_payload(
        "owner/repo",
        &detail,
        Some(&focused_comment),
        "Please help with this PR",
    );

    assert_eq!(payload.repository, "owner/repo");
    assert_eq!(payload.pr_number, 42);
    assert_eq!(payload.pr_title, "Add cat pictures");
    assert_eq!(payload.pr_body, "Please add cat pictures to every screen.");
    assert_eq!(payload.pr_state, "open", "PrState::Open -> 'open'");
    assert_eq!(payload.head_ref, "feature/cats");
    assert_eq!(payload.base_ref, "main");
    assert_eq!(
        payload.external_url,
        "https://github.com/owner/repo/pull/42"
    );
    assert!(
        !payload.review_summary.is_empty(),
        "review_summary should carry the review"
    );
    assert!(
        !payload.check_summary.is_empty(),
        "check_summary should carry the check"
    );
    assert_eq!(
        payload.focused_comment,
        Some("This is the focused PR comment".to_string())
    );
    assert_eq!(payload.focused_comment_author, Some("carol".to_string()));
    assert_eq!(payload.pr_base_prompt, "Please help with this PR");

    // PrSendPayload carries NO prompt_markdown/work_dir/signature fields.
    let _ = PrSendPayload {
        repository: String::new(),
        pr_number: 0,
        pr_title: String::new(),
        pr_body: String::new(),
        pr_state: String::new(),
        head_ref: String::new(),
        base_ref: String::new(),
        external_url: String::new(),
        review_summary: Vec::new(),
        check_summary: Vec::new(),
        focused_comment: None,
        focused_comment_author: None,
        pr_base_prompt: String::new(),
    };
}

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-011
/// @pseudocode component-002 lines 123-136
#[test]
fn test_build_pr_send_payload_without_focused_comment() {
    let mut detail = sample_pr_detail();
    detail.state = PrState::Merged;

    let payload = GhClient::build_pr_send_payload("owner/repo", &detail, None, "Base prompt");

    assert_eq!(payload.pr_number, 42);
    assert_eq!(payload.pr_state, "merged", "PrState::Merged -> 'merged'");
    assert!(
        payload.focused_comment.is_none(),
        "no focused comment -> None"
    );
    assert!(
        payload.focused_comment_author.is_none(),
        "no focused comment -> author None"
    );
}

// =============================================================================
// Error categorization + malformed JSON (no panic)
// =============================================================================

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-013
/// @pseudocode component-002 lines 223-227
#[test]
fn test_categorize_error_not_authenticated_and_rate_limited() {
    let auth_err = categorize_error(1, "You are not logged into any GitHub hosts.");
    assert!(
        matches!(auth_err, GhError::NotAuthenticated(_)),
        "auth-failure stderr should map to NotAuthenticated"
    );

    let rate_err = categorize_error(1, "API rate limit exceeded. Please try again later.");
    assert!(
        matches!(rate_err, GhError::RateLimited),
        "rate-limit stderr should map to RateLimited"
    );
}

/// @plan PLAN-20260624-PR-MODE.P07
/// @requirement REQ-PR-013
/// @pseudocode component-002 lines 138-166
#[test]
fn test_parse_malformed_json_returns_parse_error_not_panic() {
    // Malformed JSON must yield a typed GhError::ParseError, never a panic.
    let result = parse_pull_requests_json("{ this is not valid json");
    assert!(
        matches!(result, Err(GhError::ParseError(_))),
        "malformed JSON should yield a ParseError, not Ok or another error variant"
    );
}

// =============================================================================
// Mergeable + mergeStateStatus parsing (issue #92)
// =============================================================================

/// The PR detail JSON fixture now includes `mergeable` and `mergeStateStatus`
/// fields; they must be parsed into the `PullRequestDetail` correctly.
///
/// @requirement REQ-PR-009
#[test]
fn test_parse_pr_detail_mergeable_fields() {
    let detail = parse_pull_request_detail_json(PR_DETAIL_JSON, "owner/repo")
        .value_or_panic("valid PR detail JSON");
    assert_eq!(
        detail.mergeable,
        Some(true),
        "mergeable must parse as Some(true) from the fixture"
    );
    assert_eq!(
        detail.merge_state_status,
        Some("MERGEABLE".to_string()),
        "merge_state_status must parse from mergeStateStatus field"
    );
}

/// When `mergeable` is false (conflicting PR), the parser must yield Some(false).
///
/// @requirement REQ-PR-009
#[test]
fn test_parse_pr_detail_not_mergeable() {
    let json = r#"{
        "number": 99,
        "title": "Conflicting PR",
        "state": "OPEN",
        "mergedAt": null,
        "author": {"login": "someone"},
        "createdAt": "2026-06-01T00:00:00Z",
        "updatedAt": "2026-06-01T00:00:00Z",
        "headRefName": "conflict",
        "baseRefName": "main",
        "isDraft": false,
        "labels": [],
        "assignees": [],
        "milestone": null,
        "body": "",
        "url": "https://github.com/o/r/pull/99",
        "reviewDecision": null,
        "statusCheckRollup": [],
        "reviews": [],
        "mergeable": false,
        "mergeStateStatus": "DIRTY"
    }"#;
    let detail = parse_pull_request_detail_json(json, "o/r")
        .value_or_panic("valid PR detail JSON with mergeable=false");
    assert_eq!(detail.mergeable, Some(false));
    assert_eq!(detail.merge_state_status, Some("DIRTY".to_string()));
}

/// When the JSON omits mergeable fields entirely, they default to None.
///
/// @requirement REQ-PR-009
#[test]
fn test_parse_pr_detail_missing_mergeable_defaults_none() {
    let json = r#"{
        "number": 1,
        "title": "No merge info",
        "state": "OPEN",
        "mergedAt": null,
        "author": {"login": "x"},
        "createdAt": "",
        "updatedAt": "",
        "headRefName": "b",
        "baseRefName": "m",
        "isDraft": false,
        "labels": [],
        "assignees": [],
        "milestone": null,
        "body": "",
        "url": "",
        "reviewDecision": null,
        "statusCheckRollup": [],
        "reviews": []
    }"#;
    let detail = parse_pull_request_detail_json(json, "o/r")
        .value_or_panic("valid PR detail JSON without mergeable fields");
    assert_eq!(detail.mergeable, None);
    assert_eq!(detail.merge_state_status, None);
}
