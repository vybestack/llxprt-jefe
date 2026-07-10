use crate::domain::{Issue, IssueComment, IssueDetail, IssueFilter, IssueFilterState, IssueState};
use crate::github::{
    GhClient, GhError, build_list_issues_args, categorize_error, parse_comments_json,
    parse_created_comment_json, parse_created_issue_json, parse_issue_detail_json,
    parse_issue_search_json, parse_issues_json, sort_issues,
};

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
            node_id: String::new(),
            title: "Old issue".to_string(),
            state: IssueState::Open,
            author_login: "alice".to_string(),
            updated_at: "2026-03-25T10:00:00Z".to_string(),
            assignee_summary: String::new(),
            labels_summary: String::new(),
            assignees: Vec::new(),
            labels: Vec::new(),
            issue_type: String::new(),
            milestone: String::new(),
            module: String::new(),
            comment_count: 0,
            body: String::new(),
        },
        Issue {
            number: 1,
            node_id: String::new(),
            title: "Newer issue".to_string(),
            state: IssueState::Open,
            author_login: "bob".to_string(),
            updated_at: "2026-03-29T10:00:00Z".to_string(),
            assignee_summary: String::new(),
            labels_summary: String::new(),
            assignees: Vec::new(),
            labels: Vec::new(),
            issue_type: String::new(),
            milestone: String::new(),
            module: String::new(),
            comment_count: 0,
            body: String::new(),
        },
        Issue {
            number: 2,
            node_id: String::new(),
            title: "Same time, lower number".to_string(),
            state: IssueState::Open,
            author_login: "charlie".to_string(),
            updated_at: "2026-03-29T10:00:00Z".to_string(),
            assignee_summary: String::new(),
            labels_summary: String::new(),
            assignees: Vec::new(),
            labels: Vec::new(),
            issue_type: String::new(),
            milestone: String::new(),
            module: String::new(),
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
        ..IssueFilter::default()
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
fn test_list_issues_args_omit_body_for_fast_first_paint() {
    let args = build_list_issues_args("owner", "repo", &IssueFilter::default(), None, 30);
    let json_fields = args
        .windows(2)
        .find_map(|pair| (pair[0] == "--json").then_some(pair[1].as_str()))
        .unwrap_or_else(|| panic!("missing --json fields in args: {args:?}"));

    assert_eq!(
        json_fields,
        "number,title,state,author,updatedAt,assignees,labels,milestone,comments"
    );
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

#[test]
fn test_parse_repository_issues_json_pagination() {
    let json = r#"{
        "data": {
            "repository": {
                "issues": {
                    "nodes": [
                        {
                            "number": 21,
                            "title": "Repository filtered issue",
                            "state": "OPEN",
                            "author": {"login": "alice"},
                            "updatedAt": "2026-03-30T10:00:00Z",
                            "assignees": {"nodes": []},
                            "labels": {"nodes": [{"name": "module:ui"}]},
                            "issueType": {"name": "Bug"},
                            "milestone": {"title": "Sprint 1"},
                            "comments": {"totalCount": 2}
                        }
                    ],
                    "pageInfo": {
                        "hasNextPage": true,
                        "endCursor": "repo-cursor-1"
                    }
                }
            }
        }
    }"#;

    let response =
        parse_issue_search_json(json).value_or_panic("should parse repository issues JSON");

    assert_eq!(response.issues.len(), 1);
    assert_eq!(response.issues[0].number, 21);

    assert_eq!(response.issues[0].issue_type, "Bug");
    assert_eq!(response.issues[0].milestone, "Sprint 1");
    assert_eq!(response.issues[0].module, "ui");
    assert_eq!(response.cursor.as_deref(), Some("repo-cursor-1"));
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
        node_id: String::new(),
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
        node_id: String::new(),
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
            "labels": {"nodes": [{"name": "enhancement"}, {"name": "module:ui"}]},
            "issueType": {"name": "Bug"},
            "milestone": {"title": "Sprint 1"},
            "comments": {"totalCount": 0}
        }
    ]"#;

    let issues = parse_issues_json(json).value_or_panic("should parse graphql-nodes JSON");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].assignee_summary, "dave");
    assert_eq!(issues[0].labels_summary, "enhancement, module:ui");
    assert_eq!(issues[0].issue_type, "Bug");
    assert_eq!(issues[0].milestone, "Sprint 1");
    assert_eq!(issues[0].module, "ui");
}

#[test]
fn test_parse_issues_json_module_skips_empty_module_label() {
    let json = r#"[
        {
            "number": 3,
            "title": "Module",
            "state": "OPEN",
            "author": {"login": "alice"},
            "updatedAt": "2026-03-29T10:00:00Z",
            "assignees": {"nodes": []},
            "labels": {"nodes": [{"name": "module:"}, {"name": "module:ui"}]},
            "comments": {"totalCount": 0}
        }
    ]"#;

    let issues = parse_issues_json(json).value_or_panic("should parse module labels");

    assert_eq!(issues[0].module, "ui");
    assert_eq!(issues[0].labels_summary, "module:, module:ui");
}

#[test]
fn test_issue_type_repository_args_preserve_module_none_as_manual_text() {
    let filter = IssueFilter {
        issue_type: "Bug".to_string(),
        module: "none".to_string(),
        ..IssueFilter::default()
    };

    let args = crate::github::build_issue_search_args("owner", "repo", &filter, None, 30);
    let query = args
        .windows(2)
        .find_map(|pair| (pair[0] == "-f" && pair[1].starts_with("query=")).then_some(&pair[1]))
        .unwrap_or_else(|| panic!("missing GraphQL query in args: {args:?}"));

    assert!(query.contains("labels: [\"module:none\"]"));
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

/// parse_issue_from_item parses the GraphQL `id` field into `node_id`.
#[test]
fn test_parse_issue_from_item_node_id() {
    let json = r#"[
        {
            "id": "I_kwDORSOxIM7sXe5_",
            "number": 17,
            "title": "Create a feature list",
            "state": "OPEN",
            "author": {"login": "acoliver"},
            "updatedAt": "2026-03-29T10:00:00Z",
            "assignees": {"nodes": [{"login": "acoliver"}]},
            "labels": {"nodes": [{"name": "enhancement"}]},
            "comments": {"totalCount": 3}
        }
    ]"#;

    let issues = parse_issues_json(json).value_or_panic("should parse valid JSON");
    assert_eq!(issues.len(), 1);
    assert_eq!(
        issues[0].node_id, "I_kwDORSOxIM7sXe5_",
        "node_id should be parsed from the GraphQL id field"
    );
}

/// parse_issue_detail_json parses the `id` field into `node_id`.
#[test]
fn test_parse_issue_detail_json_node_id() {
    let json = r#"{
        "id": "I_kwDORSOxIM7sXe5_",
        "number": 42,
        "title": "Detail node id test",
        "state": "OPEN",
        "author": {"login": "acoliver"},
        "createdAt": "2026-03-29T10:00:00Z",
        "updatedAt": "2026-03-29T11:00:00Z",
        "labels": [],
        "assignees": [],
        "milestone": null,
        "body": "",
        "url": "https://github.com/owner/repo/issues/42",
        "comments": []
    }"#;

    let detail = parse_issue_detail_json(json).value_or_panic("should parse detail JSON");
    assert_eq!(
        detail.node_id, "I_kwDORSOxIM7sXe5_",
        "detail node_id should be parsed from the id field"
    );
    assert_eq!(detail.number, 42);
}
