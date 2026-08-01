//! Tests for linked-PR parsing from the issue search `timelineItems`
//! connection (issue #187).

use jefe::github::parse_issue_search_json;

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
fn test_parse_issue_search_json_populates_linked_pr_numbers() {
    let json = r#"{
        "data": {
            "search": {
                "nodes": [
                    {
                        "number": 42,
                        "title": "Has linked PRs",
                        "state": "OPEN",
                        "author": {"login": "acoliver"},
                        "updatedAt": "2026-03-29T10:00:00Z",
                        "assignees": {"nodes": []},
                        "labels": {"nodes": []},
                        "comments": {"totalCount": 0},
                        "timelineItems": {
                            "nodes": [
                                {
                                    "__typename": "CrossReferencedEvent",
                                    "source": {"__typename": "PullRequest", "number": 7}
                                },
                                {
                                    "__typename": "CrossReferencedEvent",
                                    "source": {"__typename": "Issue", "number": 9}
                                },
                                {
                                    "__typename": "CrossReferencedEvent",
                                    "source": {"__typename": "PullRequest", "number": 7}
                                }
                            ]
                        }
                    },
                    {
                        "number": 43,
                        "title": "No linked PRs",
                        "state": "OPEN",
                        "author": {"login": "acoliver"},
                        "updatedAt": "2026-03-29T10:00:00Z",
                        "assignees": {"nodes": []},
                        "labels": {"nodes": []},
                        "comments": {"totalCount": 0}
                    }
                ],
                "pageInfo": {"hasNextPage": false, "endCursor": null}
            }
        }
    }"#;

    let response =
        parse_issue_search_json(json).value_or_panic("should parse issue search with timeline");

    assert_eq!(response.issues.len(), 2);
    assert_eq!(response.issues[0].linked_pr_numbers, vec![7]);
    assert!(
        response.issues[1].linked_pr_numbers.is_empty(),
        "issue without timelineItems should have no linked PRs"
    );
}
