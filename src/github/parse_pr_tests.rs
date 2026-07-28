//! Tests for `parse_pr.rs` (extracted to keep that file under the per-file
//! line limit). Covers the GraphQL enum-string mergeable parsing added in #487.

use super::{parse_mergeable_value, parse_pull_request_detail_json};

use serde_json::json;

#[test]
fn enum_strings_map_like_list_parser() {
    assert_eq!(parse_mergeable_value(Some(&json!("MERGEABLE"))), Some(true));
    assert_eq!(
        parse_mergeable_value(Some(&json!("CONFLICTING"))),
        Some(false)
    );
    assert_eq!(parse_mergeable_value(Some(&json!("UNKNOWN"))), None);
    assert_eq!(parse_mergeable_value(None), None);
}

#[test]
fn boolean_json_still_parses_for_detail_fixtures() {
    assert_eq!(parse_mergeable_value(Some(&json!(true))), Some(true));
    assert_eq!(parse_mergeable_value(Some(&json!(false))), Some(false));
}

fn minimal_detail_json(mergeable: serde_json::Value) -> String {
    json!({
        "number": 1,
        "title": "t",
        "state": "OPEN",
        "mergedAt": null,
        "author": {"login": "a"},
        "createdAt": "",
        "updatedAt": "",
        "headRefName": "h",
        "baseRefName": "m",
        "isDraft": false,
        "labels": [],
        "assignees": [],
        "milestone": null,
        "body": "",
        "url": "",
        "reviewDecision": null,
        "statusCheckRollup": [],
        "reviews": [],
        "mergeable": mergeable,
        "mergeStateStatus": "CLEAN"
    })
    .to_string()
}

#[test]
fn detail_json_accepts_graphql_enum_string() {
    let detail =
        match parse_pull_request_detail_json(&minimal_detail_json(json!("CONFLICTING")), "o/r") {
            Ok(detail) => detail,
            Err(error) => panic!("detail should parse: {error}"),
        };
    assert_eq!(detail.mergeable, Some(false));
}
