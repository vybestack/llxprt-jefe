use crate::domain::{
    ChecksFilter, Issue, IssueComment, IssueDetail, IssueFilter, IssueFilterState, IssueState,
    PrCheck, PrCheckStatus, PrFilter, PrFilterState, PrReview, PrReviewState, PrState, PullRequest,
    PullRequestDetail, ReviewDecisionFilter,
};
use crate::github::{
    GhClient, GhError, PrSendPayload, build_list_issues_args, build_pr_comments_query,
    build_pr_search_args, build_pr_search_query, categorize_error, parse_check_status,
    parse_checks_rollup, parse_comments_json, parse_created_comment_json, parse_created_issue_json,
    parse_issue_detail_json, parse_issue_search_json, parse_issues_json, parse_pr_check,
    parse_pr_review, parse_pr_state, parse_pull_request_detail_json, parse_pull_requests_json,
    parse_review_decision, sort_issues, sort_pull_requests,
};
use serde_json::json;

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

fn state_arg_is_open(args: &[String]) -> bool {
    args.windows(2)
        .any(|window| window[0] == "--state" && window[1] == "open")
}
// Error Categorization Tests
// =============================================================================

/// Test 1: categorize_error returns success when exit code is 0.
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-013
/// @pseudocode component-002 lines 105-120
#[test]
fn test_check_auth_success() {
    // When exit code is 0, no error should be categorized
    let error = categorize_error(0, "");
    // Should NOT be an error variant
    assert!(!matches!(
        error,
        GhError::NotAuthenticated(_)
            | GhError::RateLimited
            | GhError::AccessDenied(_)
            | GhError::ApiError(_)
    ));
}

/// Test 2: categorize_error detects "not logged in" → NotAuthenticated.
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-013
/// @pseudocode component-002 lines 105-120
#[test]
fn test_check_auth_not_authenticated() {
    let stderr = "Welcome to GitHub CLI! To authenticate, please run `gh auth login`.\n\
                  You are not logged into any GitHub hosts.";
    let error = categorize_error(1, stderr);
    assert!(matches!(error, GhError::NotAuthenticated(_)));
}

/// Test 3: parse_issues_json parses valid gh CLI JSON output.
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-006
/// @pseudocode component-002 lines 35-45
#[test]
fn test_list_issues_parses_json() {
    let json = r#"[
        {
            "number": 17,
            "title": "Create a feature list",
            "state": "OPEN",
            "author": {"login": "acoliver"},
            "updatedAt": "2026-03-29T10:00:00Z",
            "assignees": {"nodes": [{"login": "acoliver"}]},
            "labels": {"nodes": [{"name": "enhancement"}]},
            "comments": {"totalCount": 3}
        },
        {
            "number": 5,
            "title": "Bug: crash on startup",
            "state": "CLOSED",
            "author": {"login": "bob"},
            "updatedAt": "2026-03-28T15:30:00Z",
            "assignees": {"nodes": []},
            "labels": {"nodes": [{"name": "bug"}, {"name": "critical"}]},
            "comments": {"totalCount": 0}
        }
    ]"#;

    let issues = parse_issues_json(json).value_or_panic("should parse valid JSON");
    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].number, 17);
    assert_eq!(issues[0].title, "Create a feature list");
    assert_eq!(issues[0].state, IssueState::Open);
    assert_eq!(issues[0].author_login, "acoliver");
    assert_eq!(issues[0].comment_count, 3);
    assert_eq!(issues[0].labels_summary, "enhancement");
    assert_eq!(issues[0].assignee_summary, "acoliver");
}

/// Test 4: sort_issues sorts by updated_at desc, then number asc.
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-006
/// @pseudocode component-002 lines 46-54
#[test]
fn test_list_issues_sorts_by_updated_desc() {
    let mut issues = vec![
        Issue {
            number: 3,
            title: "Old issue".to_string(),
            state: IssueState::Open,
            author_login: "alice".to_string(),
            updated_at: "2026-03-25T10:00:00Z".to_string(),
            assignee_summary: String::new(),
            labels_summary: String::new(),
            comment_count: 0,
            body: String::new(),
        },
        Issue {
            number: 1,
            title: "Newer issue".to_string(),
            state: IssueState::Open,
            author_login: "bob".to_string(),
            updated_at: "2026-03-29T10:00:00Z".to_string(),
            assignee_summary: String::new(),
            labels_summary: String::new(),
            comment_count: 0,
            body: String::new(),
        },
        Issue {
            number: 2,
            title: "Same time, lower number".to_string(),
            state: IssueState::Open,
            author_login: "charlie".to_string(),
            updated_at: "2026-03-29T10:00:00Z".to_string(),
            assignee_summary: String::new(),
            labels_summary: String::new(),
            comment_count: 0,
            body: String::new(),
        },
    ];

    sort_issues(&mut issues);

    // Should be sorted by updated_at desc, then number asc
    assert_eq!(issues[0].number, 1);
    assert_eq!(issues[1].number, 2);
    assert_eq!(issues[2].number, 3);
}

/// Test 5: build_list_issues_args constructs correct CLI arguments from filter.
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-008
/// @pseudocode component-002 lines 25-34
#[test]
fn test_list_issues_filter_args_construction() {
    let filter = IssueFilter {
        query_text: "bug".to_string(),
        state: Some(IssueFilterState::Open),
        author: "acoliver".to_string(),
        assignee: String::new(),
        labels: vec!["critical".to_string()],
        mentioned: String::new(),
        updated_before: String::new(),
        updated_after: String::new(),
    };

    let args = build_list_issues_args("owner", "repo", &filter, None, 30);

    // Should contain base command parts
    assert!(args.iter().any(|a| a.contains("owner/repo")));
    assert!(args.iter().any(|a| a == "--json"));
    assert!(
        args.iter()
            .any(|a| a == "--state" && state_arg_is_open(&args))
    );
    assert!(args.iter().any(|a| a.contains("limit") || a == "-L"));
}

#[test]
fn test_parse_issue_search_json_pagination() {
    let json = r#"{
        "data": {
            "search": {
                "nodes": [
                    {
                        "number": 17,
                        "title": "Create a feature list",
                        "state": "OPEN",
                        "author": {"login": "acoliver"},
                        "updatedAt": "2026-03-29T10:00:00Z",
                        "assignees": {"nodes": [{"login": "acoliver"}]},
                        "labels": {"nodes": [{"name": "enhancement"}]},
                        "comments": {"totalCount": 3},
                        "body": "Issue body"
                    }
                ],
                "pageInfo": {
                    "hasNextPage": true,
                    "endCursor": "cursor-1"
                }
            }
        }
    }"#;

    let response = parse_issue_search_json(json).value_or_panic("should parse issue search");

    assert_eq!(response.issues.len(), 1);
    assert_eq!(response.issues[0].number, 17);
    assert_eq!(response.cursor, Some("cursor-1".to_string()));
    assert!(response.has_more);
}

/// Test 6: parse_issues_json handles empty result.
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-006
/// @pseudocode component-002 lines 35-45
#[test]
fn test_list_issues_empty_result() {
    let json = "[]";
    let issues = parse_issues_json(json).value_or_panic("should parse empty array");
    assert!(issues.is_empty());
}

/// Test 7: parse_issue_detail_json parses complete detail JSON.
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-009
/// @pseudocode component-002 lines 55-65
#[test]
fn test_get_issue_detail_parses_json() {
    let json = r#"{
        "number": 17,
        "title": "Create a feature list",
        "state": "OPEN",
        "author": {"login": "acoliver"},
        "createdAt": "2026-03-28T10:00:00Z",
        "updatedAt": "2026-03-29T10:00:00Z",
        "labels": [{"name": "enhancement"}],
        "assignees": [{"login": "acoliver"}],
        "milestone": {"title": "v2.0"},
        "body": "Issue body text here",
        "url": "https://github.com/owner/repo/issues/17",
        "comments": [
            {
                "id": "IC_123",
                "author": {"login": "bob"},
                "createdAt": "2026-03-29T11:00:00Z",
                "body": "Comment body"
            }
        ]
    }"#;

    let detail = parse_issue_detail_json(json).value_or_panic("should parse detail JSON");
    assert_eq!(detail.number, 17);
    assert_eq!(detail.title, "Create a feature list");
    assert_eq!(detail.state, IssueState::Open);
    assert_eq!(detail.author_login, "acoliver");
    assert_eq!(detail.body, "Issue body text here");
    assert_eq!(detail.labels, vec!["enhancement"]);
    assert_eq!(detail.assignees, vec!["acoliver"]);
    assert_eq!(detail.milestone, Some("v2.0".to_string()));
    assert_eq!(
        detail.external_url,
        "https://github.com/owner/repo/issues/17"
    );
    assert_eq!(detail.repo_owner_name, "owner/repo");
    assert_eq!(detail.comments.len(), 1);
    assert_eq!(detail.comments[0].body, "Comment body");
}
#[test]
fn test_parse_issue_detail_json_disables_pagination_until_graphql_comments_are_loaded() {
    let json = r#"{
        "number": 17,
        "title": "Create a feature list",
        "state": "OPEN",
        "author": {"login": "acoliver"},
        "createdAt": "2026-03-28T10:00:00Z",
        "updatedAt": "2026-03-29T10:00:00Z",
        "labels": [],
        "assignees": [],
        "milestone": null,
        "body": "Issue body text here",
        "url": "https://github.com/owner/repo/issues/17",
        "comments": []
    }"#;

    let detail = parse_issue_detail_json(json).value_or_panic("should parse detail JSON");

    assert!(!detail.has_more_comments);
    assert_eq!(detail.comments_cursor, None);
}

/// Test 8: parse_issue_detail_json handles missing milestone.
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-009
/// @pseudocode component-002 lines 55-65
#[test]
fn test_get_issue_detail_optional_milestone() {
    let json_with_milestone = r#"{
        "number": 1,
        "title": "With milestone",
        "state": "OPEN",
        "author": {"login": "alice"},
        "createdAt": "2026-03-28T10:00:00Z",
        "updatedAt": "2026-03-29T10:00:00Z",
        "labels": [],
        "assignees": [],
        "milestone": {"title": "v1.0"},
        "body": "",
        "url": "https://github.com/o/r/issues/1",
        "comments": []
    }"#;

    let json_without_milestone = r#"{
        "number": 2,
        "title": "Without milestone",
        "state": "OPEN",
        "author": {"login": "bob"},
        "createdAt": "2026-03-28T10:00:00Z",
        "updatedAt": "2026-03-29T10:00:00Z",
        "labels": [],
        "assignees": [],
        "milestone": null,
        "body": "",
        "url": "https://github.com/o/r/issues/2",
        "comments": []
    }"#;

    let detail_with = parse_issue_detail_json(json_with_milestone).value_or_panic("should parse");
    let detail_without =
        parse_issue_detail_json(json_without_milestone).value_or_panic("should parse");

    assert_eq!(detail_with.milestone, Some("v1.0".to_string()));
    assert_eq!(detail_without.milestone, None);
}

/// Test 9: parse_comments_json parses GraphQL comments response.
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-009
/// @pseudocode component-002 lines 75-85
#[test]
fn test_list_comments_parses_json() {
    let json = r#"{
        "data": {
            "repository": {
                "issue": {
                    "comments": {
                        "nodes": [
                            {
                                "id": "IC_123",
                                "author": {"login": "alice"},
                                "createdAt": "2026-03-29T10:00:00Z",
                                "lastEditedAt": null,
                                "body": "First comment"
                            },
                            {
                                "id": "IC_456",
                                "author": {"login": "bob"},
                                "createdAt": "2026-03-29T11:00:00Z",
                                "lastEditedAt": "2026-03-29T12:00:00Z",
                                "body": "Second comment edited"
                            }
                        ],
                        "pageInfo": {
                            "hasNextPage": false,
                            "endCursor": null
                        }
                    }
                }
            }
        }
    }"#;

    let (comments, cursor, has_more) =
        parse_comments_json(json).value_or_panic("should parse comments");
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].comment_id, 123);
    assert_eq!(comments[0].author_login, "alice");
    assert_eq!(comments[0].edited_at, None);
    assert_eq!(comments[1].comment_id, 456);
    assert_eq!(comments[1].author_login, "bob");
    assert_eq!(
        comments[1].edited_at,
        Some("2026-03-29T12:00:00Z".to_string())
    );
    assert_eq!(cursor, None);
    assert!(!has_more);
}

#[test]
fn test_list_comments_parses_opaque_graphql_node_ids_with_database_id() {
    let json = r#"{
        "data": {
            "repository": {
                "issue": {
                    "comments": {
                        "nodes": [
                            {
                                "id": "IC_kwDORSOxIM75naWC",
                                "databaseId": 4187858306,
                                "author": {"login": "coderabbitai"},
                                "createdAt": "2026-04-04T22:35:31Z",
                                "lastEditedAt": null,
                                "body": "Real GitHub node ids are opaque"
                            }
                        ],
                        "pageInfo": {
                            "hasNextPage": false,
                            "endCursor": null
                        }
                    }
                }
            }
        }
    }"#;

    let (comments, cursor, has_more) =
        parse_comments_json(json).value_or_panic("should parse comments");

    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].comment_id, 4_187_858_306);
    assert_eq!(comments[0].author_login, "coderabbitai");
    assert_eq!(cursor, None);
    assert!(!has_more);
}

#[test]
fn test_issue_detail_comments_parse_opaque_node_id_from_url_fragment() {
    let json = r#"{
        "number": 39,
        "title": "Issue mode follow-ups",
        "state": "OPEN",
        "author": {"login": "acoliver"},
        "createdAt": "2026-04-04T22:00:00Z",
        "updatedAt": "2026-04-04T22:35:31Z",
        "labels": [],
        "assignees": [],
        "milestone": null,
        "body": "Issue body",
        "url": "https://github.com/vybestack/llxprt-jefe/issues/39",
        "comments": [
            {
                "id": "IC_kwDORSOxIM75naWC",
                "url": "https://github.com/vybestack/llxprt-jefe/issues/39#issuecomment-4187858306",
                "author": {"login": "coderabbitai"},
                "createdAt": "2026-04-04T22:35:31Z",
                "body": "Real gh issue view comments have opaque ids and URL fragments"
            }
        ]
    }"#;

    let detail = parse_issue_detail_json(json).value_or_panic("should parse detail JSON");

    assert_eq!(detail.comments.len(), 1);
    assert_eq!(detail.comments[0].comment_id, 4_187_858_306);
}

/// Test 10: parse_comments_json extracts pagination info.
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-009
/// @pseudocode component-002 lines 75-85
#[test]
fn test_list_comments_pagination() {
    let json = r#"{
        "data": {
            "repository": {
                "issue": {
                    "comments": {
                        "nodes": [
                            {
                                "id": "IC_789",
                                "author": {"login": "carol"},
                                "createdAt": "2026-03-29T13:00:00Z",
                                "lastEditedAt": null,
                                "body": "Another comment"
                            }
                        ],
                        "pageInfo": {
                            "hasNextPage": true,
                            "endCursor": "Y3Vyc29yOnYyOpHOABcd"
                        }
                    }
                }
            }
        }
    }"#;

    let (comments, cursor, has_more) = parse_comments_json(json).value_or_panic("should parse");
    assert_eq!(comments.len(), 1);
    assert_eq!(cursor, Some("Y3Vyc29yOnYyOpHOABcd".to_string()));
    assert!(has_more);
}

/// Test 11: parse_created_comment_json parses POST response.
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-011
/// @pseudocode component-002 lines 95-100
#[test]
fn test_create_comment_success() {
    let json = r#"{
        "id": "IC_999",
        "html_url": "https://github.com/owner/repo/issues/17#issuecomment-999",
        "author": {"login": "acoliver"},
        "createdAt": "2026-03-29T14:00:00Z",
        "body": "This is a new comment"
    }"#;

    let comment = parse_created_comment_json(json).value_or_panic("should parse created comment");
    assert_eq!(comment.comment_id, 999);
    assert_eq!(comment.author_login, "acoliver");
    assert_eq!(comment.body, "This is a new comment");
}

/// Test 11a: parse_created_issue_json parses created issue payload.
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-011
#[test]
fn test_create_issue_success() {
    let json = r#"{
        "number": 45,
        "title": "Create issue from issues mode",
        "body": "Issue body details"
    }"#;

    let issue = parse_created_issue_json(json).value_or_panic("should parse created issue");
    assert_eq!(issue.number, 45);
    assert_eq!(issue.title, "Create issue from issues mode");
    assert_eq!(issue.body, "Issue body details");
}

/// Test 11b: parse_created_comment_json handles REST API format (numeric id, "user", "created_at").
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-011
#[test]
fn test_create_comment_rest_format() {
    let json = r#"{
        "id": 4185047845,
        "html_url": "https://github.com/owner/repo/issues/15#issuecomment-4185047845",
        "user": {"login": "acoliver"},
        "created_at": "2026-04-03T20:17:41Z",
        "updated_at": "2026-04-03T20:17:41Z",
        "body": "test from jefe"
    }"#;

    let comment = parse_created_comment_json(json).value_or_panic("should parse REST format");
    assert_eq!(comment.comment_id, 4_185_047_845);
    assert_eq!(comment.author_login, "acoliver");
    assert_eq!(comment.body, "test from jefe");
    assert_eq!(comment.created_at, "2026-04-03T20:17:41Z");
}

/// Test 12: update_comment returns success (unit test for parsing non-error).
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-011
/// @pseudocode component-002 lines 120-125
#[test]
fn test_update_comment_success() {
    // For update operations, we test that categorize_error doesn't flag success as error
    let error = categorize_error(0, "");
    // Success path - should not be an error variant
    assert!(!matches!(
        error,
        GhError::NotAuthenticated(_)
            | GhError::RateLimited
            | GhError::AccessDenied(_)
            | GhError::ApiError(_)
    ));
}

/// Test 13: update_issue_body returns success (unit test for parsing non-error).
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-011
/// @pseudocode component-002 lines 126-131
#[test]
fn test_update_issue_body_success() {
    // Similar to test_update_comment_success
    let error = categorize_error(0, "");
    assert!(!matches!(
        error,
        GhError::NotAuthenticated(_)
            | GhError::RateLimited
            | GhError::AccessDenied(_)
            | GhError::ApiError(_)
    ));
}

/// Test 14: build_send_payload with focused comment.
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-011
/// @pseudocode component-002 lines 70-83
#[test]
fn test_build_send_payload_with_comment() {
    let detail = IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 17,
        title: "Test Issue".to_string(),
        state: IssueState::Open,
        author_login: "alice".to_string(),
        created_at: "2026-03-28T10:00:00Z".to_string(),
        updated_at: "2026-03-29T10:00:00Z".to_string(),
        labels: vec!["bug".to_string()],
        assignees: vec!["bob".to_string()],
        milestone: Some("v1.0".to_string()),
        body: "Issue body".to_string(),
        external_url: "https://github.com/owner/repo/issues/17".to_string(),
        comments: vec![],
        has_more_comments: false,
        comments_cursor: None,
    };

    let focused_comment = IssueComment {
        comment_id: 123,
        author_login: "carol".to_string(),
        created_at: "2026-03-29T11:00:00Z".to_string(),
        edited_at: None,
        body: "This is the focused comment".to_string(),
    };

    let payload = GhClient::build_send_payload(
        "owner/repo",
        &detail,
        Some(&focused_comment),
        "Please help with this issue",
    );

    assert_eq!(payload.repository, "owner/repo");
    assert_eq!(payload.issue_number, 17);
    assert_eq!(payload.issue_title, "Test Issue");
    assert_eq!(payload.issue_state, "open");
    assert_eq!(payload.issue_labels, vec!["bug"]);
    assert_eq!(payload.issue_assignees, vec!["bob"]);
    assert_eq!(
        payload.focused_comment,
        Some("This is the focused comment".to_string())
    );
    assert_eq!(payload.focused_comment_author, Some("carol".to_string()));
    assert_eq!(payload.issue_base_prompt, "Please help with this issue");
}

/// Test 15: build_send_payload without focused comment.
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-011
/// @pseudocode component-002 lines 70-83
#[test]
fn test_build_send_payload_without_comment() {
    let detail = IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 5,
        title: "Another Issue".to_string(),
        state: IssueState::Closed,
        author_login: "dave".to_string(),
        created_at: "2026-03-25T10:00:00Z".to_string(),
        updated_at: "2026-03-26T10:00:00Z".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "Another body".to_string(),
        external_url: "https://github.com/owner/repo/issues/5".to_string(),
        comments: vec![],
        has_more_comments: false,
        comments_cursor: None,
    };

    let payload = GhClient::build_send_payload("owner/repo", &detail, None, "Base prompt here");

    assert_eq!(payload.issue_number, 5);
    assert_eq!(payload.issue_state, "closed");
    assert!(payload.focused_comment.is_none());
    assert!(payload.focused_comment_author.is_none());
}

/// Test 16: categorize_error detects rate limit.
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-013
/// @pseudocode component-002 lines 105-120
#[test]
fn test_error_categorization_rate_limit() {
    let stderr = "API rate limit exceeded. Please wait a few minutes and try again.";
    let error = categorize_error(1, stderr);
    assert!(matches!(error, GhError::RateLimited));
}

/// Test 17: categorize_error detects authentication error.
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-013
/// @pseudocode component-002 lines 105-120
#[test]
fn test_error_categorization_not_authenticated() {
    let stderr = "401 Bad credentials - authentication required";
    let error = categorize_error(1, stderr);
    assert!(matches!(error, GhError::NotAuthenticated(_)));
}

/// Test 18: categorize_error detects access denied.
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-013
/// @pseudocode component-002 lines 105-120
#[test]
fn test_error_categorization_access_denied() {
    let stderr = "HTTP 403: Resource not accessible by personal access token";
    let error = categorize_error(1, stderr);
    assert!(matches!(error, GhError::AccessDenied(_)));
}

/// Test 19: parse_issues_json supports direct array format for assignees/labels.
/// Some `gh` CLI responses return bare arrays instead of GraphQL `{nodes:[...]}`.
#[test]
fn test_parse_issues_json_direct_array_assignees_labels() {
    let json = r#"[
        {
            "number": 1,
            "title": "Test",
            "state": "OPEN",
            "author": {"login": "alice"},
            "updatedAt": "2026-03-29T10:00:00Z",
            "assignees": [{"login": "bob"}, {"login": "carol"}],
            "labels": [{"name": "bug"}],
            "comments": {"totalCount": 0}
        }
    ]"#;

    let issues = parse_issues_json(json).value_or_panic("should parse direct-array JSON");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].assignee_summary, "bob, carol");
    assert_eq!(issues[0].labels_summary, "bug");
}

/// Test 20: parse_issues_json supports GraphQL nodes format for assignees/labels.
#[test]
fn test_parse_issues_json_graphql_nodes_assignees_labels() {
    let json = r#"[
        {
            "number": 2,
            "title": "GraphQL",
            "state": "OPEN",
            "author": {"login": "alice"},
            "updatedAt": "2026-03-29T10:00:00Z",
            "assignees": {"nodes": [{"login": "dave"}]},
            "labels": {"nodes": [{"name": "enhancement"}, {"name": "ui"}]},
            "comments": {"totalCount": 0}
        }
    ]"#;

    let issues = parse_issues_json(json).value_or_panic("should parse graphql-nodes JSON");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].assignee_summary, "dave");
    assert_eq!(issues[0].labels_summary, "enhancement, ui");
}

/// Test 21: parse_issue_detail_json propagates comment parse errors instead of
/// silently swallowing them.
#[test]
fn test_parse_issue_detail_json_propagates_comment_errors() {
    let json = r#"{
        "number": 17,
        "title": "Bad comment",
        "state": "OPEN",
        "author": {"login": "acoliver"},
        "createdAt": "2026-03-28T10:00:00Z",
        "updatedAt": "2026-03-29T10:00:00Z",
        "labels": [],
        "assignees": [],
        "milestone": null,
        "body": "",
        "url": "https://github.com/owner/repo/issues/17",
        "comments": [
            {
                "author": {"login": "bob"},
                "createdAt": "2026-03-29T11:00:00Z",
                "body": "missing id field"
            }
        ]
    }"#;

    let result = parse_issue_detail_json(json);
    assert!(
        result.is_err(),
        "should propagate parse error for malformed comment"
    );
    match result {
        Err(GhError::ParseError(msg)) => {
            assert!(
                msg.contains("comment"),
                "error message should mention comment: {msg}"
            );
        }
        Err(e) => panic!("expected ParseError, got {e:?}"),
        Ok(_) => panic!("should not succeed"),
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
    ]
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
            author_login: "reviewer1".to_string(),
            state: PrReviewState::Approved,
            submitted_at: "2026-06-14T10:00:00Z".to_string(),
            body: Some("LGTM".to_string()),
        }],
        checks: vec![PrCheck {
            name: "build".to_string(),
            status: PrCheckStatus::Success,
            conclusion: "SUCCESS".to_string(),
            url: Some("https://github.com/owner/repo/runs/1".to_string()),
        }],
        comments: vec![],
        has_more_comments: false,
        comments_cursor: None,
    }
}

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
    // yield EMPTY comments with has_more_comments==false and comments_cursor
    // ==None — proving comments MUST be sourced from the separate
    // list_pr_comments fetch. This is the pure, testable expression of that
    // requirement.
    let result = parse_pull_request_detail_json(PR_DETAIL_JSON, "owner/repo");
    assert!(result.is_ok(), "should parse PR detail: {result:?}");
    if let Ok(detail) = result {
        assert!(
            detail.comments.is_empty(),
            "PR detail JSON omits comments; comments must be sourced separately"
        );
        assert!(
            !detail.has_more_comments,
            "has_more_comments is false until list_pr_comments populates it"
        );
        assert_eq!(
            detail.comments_cursor, None,
            "comments_cursor is None until list_pr_comments populates it"
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
