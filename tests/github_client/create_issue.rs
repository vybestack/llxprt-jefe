use jefe::domain::IssueState;
use jefe::github::{GhError, parse_created_issue_json};

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
fn test_create_issue_success() {
    let json = r#"{
        "number": 45,
        "title": "Create issue from issues mode",
        "body": "Issue body details",
        "node_id": "I_kwDOABC",
        "user": {"login": "acoliver"},
        "created_at": "2026-03-29T14:00:00Z",
        "updated_at": "2026-03-29T14:01:00Z"
    }"#;

    let issue = parse_created_issue_json(json).value_or_panic("should parse created issue");
    assert_eq!(issue.number, 45);
    assert_eq!(issue.title, "Create issue from issues mode");
    assert_eq!(issue.body, "Issue body details");
    assert_eq!(issue.node_id, "I_kwDOABC");
    assert_eq!(issue.author_login, "acoliver");
    assert_eq!(issue.updated_at, "2026-03-29T14:01:00Z");

    let list_issue = issue.into_list_issue();
    assert_eq!(list_issue.number, 45);
    assert_eq!(list_issue.state, IssueState::Open);
    assert_eq!(list_issue.title, "Create issue from issues mode");
    assert_eq!(list_issue.node_id, "I_kwDOABC");
}

#[test]
fn test_create_issue_missing_node_id_is_error() {
    let json = r#"{
        "number": 45,
        "title": "Create issue from issues mode",
        "body": "Issue body details",
        "user": {"login": "acoliver"},
        "created_at": "2026-03-29T14:00:00Z"
    }"#;

    let result = parse_created_issue_json(json);
    assert!(
        matches!(result, Err(GhError::ParseError(_))),
        "missing node_id must be a ParseError, got {result:?}"
    );
}

#[test]
fn test_create_issue_empty_node_id_is_error() {
    let json = r#"{
        "number": 45,
        "title": "Create issue from issues mode",
        "body": "Issue body details",
        "node_id": "",
        "user": {"login": "acoliver"},
        "created_at": "2026-03-29T14:00:00Z"
    }"#;

    let result = parse_created_issue_json(json);
    assert!(
        matches!(result, Err(GhError::ParseError(_))),
        "empty node_id must be a ParseError, got {result:?}"
    );
}
