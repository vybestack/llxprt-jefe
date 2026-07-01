//! GitHub issue filter argument and parser coverage split out of github/tests.rs.

use crate::domain::{IssueFilter, IssueFilterState};
use crate::github::{build_list_issues_args, parse_issue_search_json};

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

#[test]
fn test_issue_search_args_include_supported_extended_filter_terms() {
    let filter = IssueFilter {
        assignee: "none".to_string(),
        issue_type: "bug report".to_string(),
        milestone: "none".to_string(),
        module: "ui shell".to_string(),
        ..IssueFilter::default()
    };

    let args = crate::github::build_issue_search_args("owner", "repo", &filter, None, 30);
    let query = args
        .windows(2)
        .find_map(|pair| {
            (pair[0] == "-F" && pair[1].starts_with("searchQuery=")).then_some(&pair[1])
        })
        .unwrap_or_else(|| panic!("missing searchQuery in args: {args:?}"));

    assert!(query.contains("no:assignee"));
    assert!(query.contains("no:milestone"));
    assert!(query.contains(r#"label:"module:ui shell""#));
    assert!(!query.contains("type:bug report"));
}

#[test]
fn test_issue_type_repository_args_include_concrete_supported_filters() {
    let filter = IssueFilter {
        state: Some(IssueFilterState::Closed),
        author: "alice".to_string(),
        assignee: "bob".to_string(),
        issue_type: "Bug".to_string(),
        milestone: "Sprint 1".to_string(),
        labels: vec!["priority:high".to_string()],
        mentioned: "carol".to_string(),
        ..IssueFilter::default()
    };

    let args = crate::github::build_issue_search_args("owner", "repo", &filter, Some("cursor"), 20);
    let query = args
        .windows(2)
        .find_map(|pair| (pair[0] == "-f" && pair[1].starts_with("query=")).then_some(&pair[1]))
        .unwrap_or_else(|| panic!("missing GraphQL query in args: {args:?}"));

    assert!(query.contains("after: $after"));
    assert!(query.contains("type: $issueType"));
    assert!(query.contains("states: [CLOSED]"));
    assert!(query.contains("createdBy: $author"));
    assert!(query.contains("assignee: $assignee"));
    assert!(query.contains("milestone: $milestone"));
    assert!(query.contains("mentioned: $mentioned"));
    assert!(query.contains("labels: [\"priority:high\"]"));
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "-F" && pair[1] == "author=alice")
    );
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "-F" && pair[1] == "assignee=bob")
    );
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "-F" && pair[1] == "milestone=Sprint 1")
    );
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "-F" && pair[1] == "mentioned=carol")
    );
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "-F" && pair[1] == "after=cursor")
    );
}

#[test]
fn test_issue_search_args_handle_case_insensitive_any_none_sentinels() {
    let filter = IssueFilter {
        assignee: "None".to_string(),
        milestone: "ANY".to_string(),
        ..IssueFilter::default()
    };

    let args = crate::github::build_issue_search_args("owner", "repo", &filter, None, 30);
    let query = args
        .windows(2)
        .find_map(|pair| {
            (pair[0] == "-F" && pair[1].starts_with("searchQuery=")).then_some(&pair[1])
        })
        .unwrap_or_else(|| panic!("missing searchQuery in args: {args:?}"));

    assert!(query.contains("no:assignee"));
    assert!(!query.contains("milestone:"));
}

#[test]
fn test_issue_search_args_skip_any_for_author_type_and_module() {
    let filter = IssueFilter {
        author: "any".to_string(),
        issue_type: "ANY".to_string(),
        module: "Any".to_string(),
        ..IssueFilter::default()
    };

    let args = crate::github::build_issue_search_args("owner", "repo", &filter, None, 30);
    let query = args
        .windows(2)
        .find_map(|pair| {
            (pair[0] == "-F" && pair[1].starts_with("searchQuery=")).then_some(&pair[1])
        })
        .unwrap_or_else(|| panic!("missing searchQuery in args: {args:?}"));

    assert!(!query.contains("author:any"));
    assert!(!query.contains("type:ANY"));
    assert!(!query.contains("label:module:Any"));
}

#[test]
fn test_issue_search_args_skip_any_for_mentioned_and_updated() {
    let filter = IssueFilter {
        mentioned: "any".to_string(),
        updated_before: "ANY".to_string(),
        updated_after: "Any".to_string(),
        ..IssueFilter::default()
    };

    let args = crate::github::build_issue_search_args("owner", "repo", &filter, None, 30);
    let query = args
        .windows(2)
        .find_map(|pair| {
            (pair[0] == "-F" && pair[1].starts_with("searchQuery=")).then_some(&pair[1])
        })
        .unwrap_or_else(|| panic!("missing searchQuery in args: {args:?}"));

    assert!(!query.contains("mentions:any"));
    assert!(!query.contains("updated:<ANY"));
    assert!(!query.contains("updated:>Any"));
}

#[test]
fn test_issue_search_args_preserve_literal_any_query_text() {
    let filter = IssueFilter {
        query_text: "ANY".to_string(),
        ..IssueFilter::default()
    };

    let args = crate::github::build_issue_search_args("owner", "repo", &filter, None, 30);
    let query = args
        .windows(2)
        .find_map(|pair| {
            (pair[0] == "-F" && pair[1].starts_with("searchQuery=")).then_some(&pair[1])
        })
        .unwrap_or_else(|| panic!("missing searchQuery in args: {args:?}"));

    assert!(query.contains("ANY"));
}

#[test]
fn test_list_issues_args_preserve_literal_any_query_text() {
    let filter = IssueFilter {
        query_text: "any".to_string(),
        ..IssueFilter::default()
    };

    let args = build_list_issues_args("owner", "repo", &filter, None, 30);

    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "--search" && pair[1] == "any")
    );
}

#[test]
fn test_list_issues_args_skip_any_for_author_type_and_module() {
    let filter = IssueFilter {
        author: "any".to_string(),
        issue_type: "ANY".to_string(),
        module: "Any".to_string(),
        mentioned: "any".to_string(),
        ..IssueFilter::default()
    };

    let args = build_list_issues_args("owner", "repo", &filter, None, 30);
    assert!(!args.windows(2).any(|pair| pair[0] == "--mention"));
    assert!(!args.windows(2).any(|pair| pair[0] == "--author"));
    assert!(
        !args
            .windows(2)
            .any(|pair| pair[0] == "--search" && pair[1].contains("type:ANY"))
    );
    assert!(
        !args
            .windows(2)
            .any(|pair| pair[0] == "--search" && pair[1].contains("label:module:Any"))
    );
}

#[test]
fn test_list_issues_args_do_not_duplicate_concrete_assignee() {
    let filter = IssueFilter {
        assignee: "alice".to_string(),
        issue_type: "Bug".to_string(),
        ..IssueFilter::default()
    };

    let args = build_list_issues_args("owner", "repo", &filter, None, 30);
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "--assignee" && pair[1] == "alice")
    );
    let search = args
        .windows(2)
        .find_map(|pair| (pair[0] == "--search").then_some(pair[1].as_str()))
        .unwrap_or("");

    assert!(!search.contains("assignee:alice"));
    assert!(!search.contains("type:Bug"));
}

#[test]
fn test_list_issues_args_bridge_extended_filters_through_search() {
    let filter = IssueFilter {
        assignee: "none".to_string(),
        issue_type: "Bug".to_string(),
        milestone: "Sprint 1".to_string(),
        module: "ui shell".to_string(),
        ..IssueFilter::default()
    };

    let args = build_list_issues_args("owner", "repo", &filter, None, 30);
    let search = args
        .windows(2)
        .find_map(|pair| (pair[0] == "--search").then_some(pair[1].as_str()))
        .unwrap_or_else(|| panic!("missing --search in args: {args:?}"));

    assert!(search.contains("no:assignee"));
    assert!(!search.contains("type:Bug"));
    assert!(search.contains(r#"milestone:"Sprint 1""#));
    assert!(search.contains(r#"label:"module:ui shell""#));
}

#[test]
fn test_issue_search_args_do_not_duplicate_module_label_filter() {
    let filter = IssueFilter {
        labels: vec!["module:ui".to_string()],
        module: "ui".to_string(),
        ..IssueFilter::default()
    };

    let args = crate::github::build_issue_search_args("owner", "repo", &filter, None, 30);

    let query = args
        .windows(2)
        .find_map(|pair| {
            (pair[0] == "-F" && pair[1].starts_with("searchQuery=")).then_some(&pair[1])
        })
        .unwrap_or_else(|| panic!("missing searchQuery in args: {args:?}"));

    assert_eq!(query.matches("label:module:ui").count(), 1);
}

#[test]
fn test_list_issues_args_do_not_duplicate_module_label_filter() {
    let filter = IssueFilter {
        labels: vec!["module:ui".to_string()],
        module: "ui".to_string(),
        ..IssueFilter::default()
    };

    let args = build_list_issues_args("owner", "repo", &filter, None, 30);
    let native_label_count = args
        .windows(2)
        .filter(|pair| pair[0] == "--label" && pair[1] == "module:ui")
        .count();
    let search = args
        .windows(2)
        .find_map(|pair| (pair[0] == "--search").then_some(pair[1].as_str()))
        .unwrap_or("");

    assert_eq!(native_label_count, 1);
    assert!(!search.contains("label:module:ui"));
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
fn test_issue_search_args_preserve_module_none_as_manual_text() {
    let filter = IssueFilter {
        module: "none".to_string(),
        ..IssueFilter::default()
    };

    let args = crate::github::build_issue_search_args("owner", "repo", &filter, None, 30);
    let query = args
        .windows(2)
        .find_map(|pair| {
            (pair[0] == "-F" && pair[1].starts_with("searchQuery=")).then_some(&pair[1])
        })
        .unwrap_or_else(|| panic!("missing searchQuery in args: {args:?}"));

    assert!(query.contains("label:module:none"));
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
fn issue_query_fields(query_arg: &str) -> Vec<&str> {
    let Some(issue_fields) = query_arg.split("... on Issue {").nth(1) else {
        return Vec::new();
    };
    let Some(before_comments) = issue_fields.split("comments").next() else {
        return Vec::new();
    };
    before_comments
        .split_whitespace()
        .filter(|token| token.chars().all(char::is_alphanumeric))
        .collect()
}

#[test]
fn issue_search_args_omit_body_for_fast_first_paint() {
    let args =
        crate::github::build_issue_search_args("owner", "repo", &IssueFilter::default(), None, 30);
    let query_arg = args
        .iter()
        .find(|arg| arg.starts_with("query="))
        .unwrap_or_else(|| panic!("missing GraphQL query arg: {args:?}"));

    let fields = issue_query_fields(query_arg);
    assert!(fields.contains(&"title"));
    assert!(!fields.contains(&"body"));

    let paged_args = crate::github::build_issue_search_args(
        "owner",
        "repo",
        &IssueFilter::default(),
        Some("cursor"),
        30,
    );
    let paged_query_arg = paged_args
        .iter()
        .find(|arg| arg.starts_with("query="))
        .unwrap_or_else(|| panic!("missing paged GraphQL query arg: {paged_args:?}"));
    let paged_fields = issue_query_fields(paged_query_arg);
    assert!(!paged_fields.contains(&"body"));
}
