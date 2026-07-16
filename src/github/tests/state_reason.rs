//! Tests for `stateReason` / `state_reason` parsing (issue #204).

use crate::github::{parse_issue_detail_json, parse_issues_json};

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
