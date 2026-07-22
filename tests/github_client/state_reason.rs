//! Tests for `stateReason` / `state_reason` parsing (issue #204).

use jefe::github::{parse_issue_detail_json, parse_issues_json};

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
    use jefe::domain::IssueStateReason;
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
fn test_issue_detail_parses_state_reason_rest_all_values() {
    use jefe::domain::IssueStateReason;

    fn detail_json(state_reason: &str, title: &str) -> String {
        format!(
            r#"{{
            "number": 42,
            "id": "I_kw123",
            "title": "{title}",
            "state": "CLOSED",
            "state_reason": "{state_reason}",
            "author": {{"login": "dave"}},
            "createdAt": "2026-06-01T00:00:00Z",
            "updatedAt": "2026-07-01T00:00:00Z",
            "labels": [],
            "assignees": [],
            "milestone": null,
            "body": "body",
            "url": "https://github.com/owner/repo/issues/42",
            "comments": []
        }}"#
        )
    }

    let completed = parse_issue_detail_json(&detail_json("completed", "Completed issue"))
        .value_or_panic("should parse completed");
    assert_eq!(completed.state_reason, Some(IssueStateReason::Completed));

    let not_planned = parse_issue_detail_json(&detail_json("not_planned", "Not planned issue"))
        .value_or_panic("should parse not_planned");
    assert_eq!(not_planned.state_reason, Some(IssueStateReason::NotPlanned));

    let duplicate = parse_issue_detail_json(&detail_json("duplicate", "Duplicate issue"))
        .value_or_panic("should parse duplicate");
    assert_eq!(duplicate.state_reason, Some(IssueStateReason::Duplicate));
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
fn test_issue_detail_state_reason_none_for_reopened_and_unknown() {
    fn detail_json(state_reason: &str) -> String {
        format!(
            r#"{{
            "number": 42,
            "id": "I_kw123",
            "title": "test",
            "state": "OPEN",
            "state_reason": "{state_reason}",
            "author": {{"login": "dave"}},
            "createdAt": "2026-06-01T00:00:00Z",
            "updatedAt": "2026-07-01T00:00:00Z",
            "labels": [],
            "assignees": [],
            "milestone": null,
            "body": "body",
            "url": "https://github.com/owner/repo/issues/42",
            "comments": []
        }}"#
        )
    }

    let reopened =
        parse_issue_detail_json(&detail_json("reopened")).value_or_panic("should parse reopened");
    assert_eq!(reopened.state_reason, None);

    let unknown =
        parse_issue_detail_json(&detail_json("dismissed")).value_or_panic("should parse unknown");
    assert_eq!(unknown.state_reason, None);
}
