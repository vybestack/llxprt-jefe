use crate::domain::{Issue, IssueComment, IssueDetail, IssueFilter, IssueFilterState, IssueState};
use crate::github::{
    GhClient, GhError, build_assign_issue_args, build_list_issues_args, build_viewer_login_args,
    categorize_error, parse_comments_json, parse_created_comment_json, parse_issue_detail_json,
    parse_issue_search_json, parse_issues_json, parse_viewer_login, sort_issues,
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

fn state_arg_is_open(args: &[String]) -> bool {
    args.windows(2)
        .any(|window| window[0] == "--state" && window[1] == "open")
}

#[test]
fn test_check_auth_success() {
    let error = categorize_error(0, "");
    assert!(!matches!(
        error,
        GhError::NotAuthenticated(_)
            | GhError::RateLimited
            | GhError::AccessDenied(_)
            | GhError::ApiError(_)
    ));
}

#[test]
fn test_check_auth_not_authenticated() {
    let stderr = "Welcome to GitHub CLI! To authenticate, please run `gh auth login`.\n\
                  You are not logged into any GitHub hosts.";
    let error = categorize_error(1, stderr);
    assert!(matches!(error, GhError::NotAuthenticated(_)));
}

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

#[test]
fn test_list_issues_parses_state_reason_graphql() {
    use crate::domain::IssueStateReason;
    let json = r#"[
        {
            "number": 1,
            "title": "Done issue",
            "state": "CLOSED",
            "stateReason": "COMPLETED",
            "author": {"login": "a"},
            "updatedAt": "2026-07-01T00:00:00Z",
            "assignees": {"nodes": []},
            "labels": {"nodes": []},
            "comments": {"totalCount": 0}
        },
        {
            "number": 2,
            "title": "Wontfix issue",
            "state": "CLOSED",
            "stateReason": "NOT_PLANNED",
            "author": {"login": "b"},
            "updatedAt": "2026-07-01T00:00:00Z",
            "assignees": {"nodes": []},
            "labels": {"nodes": []},
            "comments": {"totalCount": 0}
        },
        {
            "number": 3,
            "title": "Duplicate issue",
            "state": "CLOSED",
            "stateReason": "DUPLICATE",
            "author": {"login": "c"},
            "updatedAt": "2026-07-01T00:00:00Z",
            "assignees": {"nodes": []},
            "labels": {"nodes": []},
            "comments": {"totalCount": 0}
        }
    ]"#;

    let issues = parse_issues_json(json).value_or_panic("should parse GraphQL stateReason");
    assert_eq!(issues[0].state_reason, Some(IssueStateReason::Completed));
    assert_eq!(issues[1].state_reason, Some(IssueStateReason::NotPlanned));
    assert_eq!(issues[2].state_reason, Some(IssueStateReason::Duplicate));
}

#[test]
fn test_list_issues_state_reason_none_when_missing() {
    let json = r#"[
        {
            "number": 1,
            "title": "Open issue",
            "state": "OPEN",
            "author": {"login": "a"},
            "updatedAt": "2026-07-01T00:00:00Z",
            "assignees": {"nodes": []},
            "labels": {"nodes": []},
            "comments": {"totalCount": 0}
        }
    ]"#;

    let issues = parse_issues_json(json).value_or_panic("should parse without stateReason");
    assert_eq!(issues[0].state_reason, None);
}

#[test]
fn test_list_issues_state_reason_none_when_reopened_or_unknown() {
    let json = r#"[
        {
            "number": 1,
            "title": "Reopened",
            "state": "OPEN",
            "stateReason": "REOPENED",
            "author": {"login": "a"},
            "updatedAt": "2026-07-01T00:00:00Z",
            "assignees": {"nodes": []},
            "labels": {"nodes": []},
            "comments": {"totalCount": 0}
        },
        {
            "number": 2,
            "title": "Unknown reason",
            "state": "CLOSED",
            "stateReason": "SOMETHING_ELSE",
            "author": {"login": "a"},
            "updatedAt": "2026-07-01T00:00:00Z",
            "assignees": {"nodes": []},
            "labels": {"nodes": []},
            "comments": {"totalCount": 0}
        }
    ]"#;

    let issues = parse_issues_json(json).value_or_panic("should parse reopened/unknown");
    assert_eq!(issues[0].state_reason, None);
    assert_eq!(issues[1].state_reason, None);
}

#[test]
fn test_issue_detail_parses_state_reason_rest() {
    use crate::domain::IssueStateReason;
    let json = r#"{
        "number": 42,
        "id": "I_kw123",
        "title": "Closed as not planned",
        "state": "CLOSED",
        "state_reason": "not_planned",
        "author": {"login": "dave"},
        "createdAt": "2026-06-01T00:00:00Z",
        "updatedAt": "2026-07-01T00:00:00Z",
        "labels": [],
        "assignees": [],
        "milestone": null,
        "body": "body",
        "url": "https://github.com/owner/repo/issues/42",
        "comments": []
    }"#;

    let detail = parse_issue_detail_json(json).value_or_panic("should parse REST state_reason");
    assert_eq!(detail.state_reason, Some(IssueStateReason::NotPlanned));
}

#[test]
fn test_issue_detail_state_reason_none_when_missing() {
    let json = r#"{
        "number": 42,
        "id": "I_kw123",
        "title": "Open issue",
        "state": "OPEN",
        "author": {"login": "dave"},
        "createdAt": "2026-06-01T00:00:00Z",
        "updatedAt": "2026-07-01T00:00:00Z",
        "labels": [],
        "assignees": [],
        "milestone": null,
        "body": "body",
        "url": "https://github.com/owner/repo/issues/42",
        "comments": []
    }"#;

    let detail = parse_issue_detail_json(json).value_or_panic("should parse without state_reason");
    assert_eq!(detail.state_reason, None);
}

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
            state_reason: None,
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
            state_reason: None,
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
            state_reason: None,
        },
    ];

    sort_issues(&mut issues);

    assert_eq!(issues[0].number, 1);
    assert_eq!(issues[1].number, 2);
    assert_eq!(issues[2].number, 3);
}

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

#[test]
fn test_list_issues_empty_result() {
    let json = "[]";
    let issues = parse_issues_json(json).value_or_panic("should parse empty array");
    assert!(issues.is_empty());
}

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
fn test_parsed_issue_comments_are_identity_free_before_reducer_rebind() {
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

    assert!(!detail.comments.has_more());
    assert_eq!(detail.comments.next_page(), &crate::domain::PageToken::Done);
    assert!(detail.comments.identity().is_none());
}

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

#[test]
fn test_update_comment_success() {
    let error = categorize_error(0, "");
    assert!(!matches!(
        error,
        GhError::NotAuthenticated(_)
            | GhError::RateLimited
            | GhError::AccessDenied(_)
            | GhError::ApiError(_)
    ));
}

#[test]
fn test_update_issue_body_success() {
    let error = categorize_error(0, "");
    assert!(!matches!(
        error,
        GhError::NotAuthenticated(_)
            | GhError::RateLimited
            | GhError::AccessDenied(_)
            | GhError::ApiError(_)
    ));
}

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
        comments: crate::domain::PaginatedList::from_loaded(
            crate::domain::CommentDetailIdentity {
                scope_repo_id: crate::domain::RepositoryId::default(),
                number: 17,
            },
            vec![],
            crate::domain::PageToken::from_cursor(None, false),
        ),
        issue_type_name: None,
        state_reason: None,
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
        comments: crate::domain::PaginatedList::from_loaded(
            crate::domain::CommentDetailIdentity {
                scope_repo_id: crate::domain::RepositoryId::default(),
                number: 5,
            },
            vec![],
            crate::domain::PageToken::from_cursor(None, false),
        ),
        issue_type_name: None,
        state_reason: None,
    };

    let payload = GhClient::build_send_payload("owner/repo", &detail, None, "Base prompt here");

    assert_eq!(payload.issue_number, 5);
    assert_eq!(payload.issue_state, "closed");
    assert!(payload.focused_comment.is_none());
    assert!(payload.focused_comment_author.is_none());
}

#[test]
fn test_error_categorization_rate_limit() {
    let stderr = "API rate limit exceeded. Please wait a few minutes and try again.";
    let error = categorize_error(1, stderr);
    assert!(matches!(error, GhError::RateLimited));
}

#[test]
fn test_error_categorization_not_authenticated() {
    let stderr = "401 Bad credentials - authentication required";
    let error = categorize_error(1, stderr);
    assert!(matches!(error, GhError::NotAuthenticated(_)));
}

#[test]
fn test_error_categorization_access_denied() {
    let stderr = "HTTP 403: Resource not accessible by personal access token";
    let error = categorize_error(1, stderr);
    assert!(matches!(error, GhError::AccessDenied(_)));
}

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

#[test]
fn test_parse_viewer_login_bare_jq_string() {
    let login = parse_viewer_login("acoliver\n").value_or_panic("bare login should parse");
    assert_eq!(login, "acoliver");
}

#[test]
fn test_parse_viewer_login_trims_surrounding_whitespace() {
    let login = parse_viewer_login("\n  acoliver  \n").value_or_panic("trimmed login parses");
    assert_eq!(login, "acoliver");
}

#[test]
fn test_parse_viewer_login_strips_surrounding_quotes() {
    let login = parse_viewer_login(r#""acoliver""#).value_or_panic("quoted login parses");
    assert_eq!(login, "acoliver");
}

#[test]
fn test_parse_viewer_login_rejects_multiline_garbage() {
    // A bare-form output with embedded newlines/whitespace inside the login
    // is not a valid GitHub login and must be rejected.
    let result = parse_viewer_login("warning: something\nacoliver");
    assert!(
        matches!(result, Err(GhError::ParseError(_))),
        "multiline bare output must be rejected, got {result:?}"
    );
}

#[test]
fn test_parse_viewer_login_rejects_valid_first_line_with_trailing_garbage() {
    // Even when the first line is itself a valid login, trailing lines mean
    // the gh output was malformed; reject rather than silently taking line 1.
    let result = parse_viewer_login("acoliver\nunexpected garbage");
    assert!(
        matches!(result, Err(GhError::ParseError(_))),
        "multiline output with a valid first line must be rejected, got {result:?}"
    );
}

#[test]
fn test_parse_viewer_login_rejects_invalid_login_chars() {
    // GitHub logins cannot contain '@' or spaces; reject rather than passing
    // a malformed value to the assignment request.
    let result = parse_viewer_login("not a login");
    assert!(
        matches!(result, Err(GhError::ParseError(_))),
        "login with invalid chars must be rejected, got {result:?}"
    );
}

#[test]
fn test_parse_viewer_login_empty_is_error() {
    let result = parse_viewer_login("   \n  ");
    assert!(
        matches!(result, Err(GhError::ParseError(_))),
        "empty viewer output must be a ParseError, got {result:?}"
    );
}

#[test]
fn test_parse_viewer_login_missing_login_field_is_error() {
    let json = r#"{"id": 1234, "name": "No Login"}"#;
    let result = parse_viewer_login(json);
    assert!(
        matches!(result, Err(GhError::ParseError(_))),
        "missing login field must be a ParseError, got {result:?}"
    );
}

#[test]
fn test_parse_viewer_login_malformed_json_is_error() {
    let result = parse_viewer_login("{ not json");
    assert!(
        matches!(result, Err(GhError::ParseError(_))),
        "malformed JSON must be a ParseError, got {result:?}"
    );
}

#[test]
fn test_build_viewer_login_args_shape() {
    let args = build_viewer_login_args();
    assert_eq!(
        args,
        vec![
            "api".to_string(),
            "user".to_string(),
            "--jq".to_string(),
            ".login".to_string(),
        ]
    );
}

#[test]
fn test_build_assign_issue_args_shape() {
    let args = build_assign_issue_args("acme", "widgets", 166, "acoliver");
    assert_eq!(
        args,
        vec![
            "api".to_string(),
            "--method".to_string(),
            "POST".to_string(),
            "/repos/acme/widgets/issues/166/assignees".to_string(),
            "-f".to_string(),
            "assignees[]=acoliver".to_string(),
        ]
    );
}
