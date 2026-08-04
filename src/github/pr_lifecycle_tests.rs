//! Behavioral tests for the PR lifecycle `gh` boundary (issue #183).
//!
//! Every test here is about the exact request jefe sends and the exact meaning
//! it takes from the answer. No test touches the network.

use super::{
    build_branch_ref_id_args, build_branches_query_args, build_create_pr_args,
    build_delete_ref_args, parse_branch_ref_id, parse_branches_page, parse_created_pr_number,
    parse_default_branch_name,
};
use crate::github::GhError;

fn flag_value<'args>(args: &'args [String], flag: &str, prefix: &str) -> Option<&'args str> {
    args.windows(2).find_map(|pair| {
        (pair[0] == flag)
            .then(|| pair[1].strip_prefix(prefix))
            .flatten()
    })
}

// ── Resolving the head branch's ref ────────────────────────────────────────

#[test]
fn ref_lookup_qualifies_the_branch_under_refs_heads() {
    let args = build_branch_ref_id_args("acme", "widgets", "feature/login");
    assert_eq!(args[0], "api");
    assert_eq!(args[1], "graphql");
    assert_eq!(
        flag_value(&args, "-F", "qualified="),
        Some("refs/heads/feature/login"),
        "the branch must be qualified so GitHub resolves a branch, not a tag: {args:?}"
    );
    assert_eq!(flag_value(&args, "-F", "owner="), Some("acme"));
    assert_eq!(flag_value(&args, "-F", "name="), Some("widgets"));
}

#[test]
fn ref_lookup_asks_for_the_ref_node_id() {
    let args = build_branch_ref_id_args("acme", "widgets", "main");
    let query = flag_value(&args, "-f", "query=").unwrap_or_default();
    assert!(
        query.contains("ref(qualifiedName: $qualified)"),
        "query must select the ref by qualified name: {query}"
    );
    assert!(
        query.contains("id"),
        "query must select the node id: {query}"
    );
}

#[test]
fn a_resolved_ref_yields_its_node_id() {
    let json = r#"{"data":{"repository":{"ref":{"id":"REF_kwDOABC123"}}}}"#;
    match parse_branch_ref_id(json) {
        Ok(id) => assert_eq!(id, "REF_kwDOABC123"),
        Err(error) => panic!("a resolved ref must parse: {error}"),
    }
}

#[test]
fn a_branch_that_does_not_exist_in_this_repository_is_reported_as_missing() {
    // A fork-headed PR resolves to `null` here: the head branch is not in the
    // base repository, so there is nothing this repository can delete.
    let json = r#"{"data":{"repository":{"ref":null}}}"#;
    match parse_branch_ref_id(json) {
        Err(GhError::ApiError(message)) => assert!(
            message.contains("not found"),
            "the diagnostic must say the branch was not found: {message}"
        ),
        other => panic!("a null ref must be an error, got {other:?}"),
    }
}

#[test]
fn an_empty_ref_id_is_rejected_rather_than_deleted() {
    let json = r#"{"data":{"repository":{"ref":{"id":""}}}}"#;
    assert!(
        parse_branch_ref_id(json).is_err(),
        "an empty node id must never reach a delete mutation"
    );
}

#[test]
fn a_graphql_error_envelope_is_surfaced_when_resolving_a_ref() {
    let json = r#"{"data":null,"errors":[{"message":"Could not resolve to a Repository"}]}"#;
    match parse_branch_ref_id(json) {
        Err(GhError::ApiError(message)) => assert!(
            message.contains("Could not resolve to a Repository"),
            "got: {message}"
        ),
        other => panic!("expected ApiError, got {other:?}"),
    }
}

#[test]
fn a_malformed_ref_response_is_a_parse_error() {
    assert!(matches!(
        parse_branch_ref_id("{ not json"),
        Err(GhError::ParseError(_))
    ));
}

// ── Deleting the ref ───────────────────────────────────────────────────────

#[test]
fn deleting_a_ref_sends_the_delete_ref_mutation_with_the_node_id() {
    let args = build_delete_ref_args("REF_kwDOABC123");
    assert_eq!(args[0], "api");
    assert_eq!(args[1], "graphql");
    let query = flag_value(&args, "-f", "query=").unwrap_or_default();
    assert!(query.contains("deleteRef"), "got: {query}");
    assert!(query.contains("refId: $id"), "got: {query}");
    assert_eq!(flag_value(&args, "-F", "id="), Some("REF_kwDOABC123"));
}

// ── Listing branches for the composer ──────────────────────────────────────

#[test]
fn the_branch_query_asks_only_for_branches_and_the_default() {
    let args = build_branches_query_args("acme", "widgets", None);
    let query = flag_value(&args, "-f", "query=").unwrap_or_default();
    assert!(
        query.contains(r#"refPrefix: "refs/heads/""#),
        "only branches, not tags: {query}"
    );
    assert!(
        query.contains("defaultBranchRef"),
        "the composer seeds Base from the default branch: {query}"
    );
    assert!(query.contains("pageInfo"), "got: {query}");
    assert!(
        !args.iter().any(|arg| arg.starts_with("after=")),
        "the first page must not send a cursor: {args:?}"
    );
}

#[test]
fn a_continued_branch_query_carries_the_cursor() {
    let args = build_branches_query_args("acme", "widgets", Some("Y3Vyc29yOjE="));
    assert_eq!(flag_value(&args, "-F", "after="), Some("Y3Vyc29yOjE="));
    let query = flag_value(&args, "-f", "query=").unwrap_or_default();
    assert!(query.contains("$after"), "got: {query}");
}

#[test]
fn a_branch_page_yields_its_names_in_order_and_the_next_cursor() {
    let json = r#"{"data":{"repository":{"defaultBranchRef":{"name":"main"},
        "refs":{"nodes":[{"name":"main"},{"name":"feature/login"}],
        "pageInfo":{"hasNextPage":true,"endCursor":"CUR2"}}}}}"#;
    let (names, cursor) = match parse_branches_page(json) {
        Ok(page) => page,
        Err(error) => panic!("a well-formed page must parse: {error}"),
    };
    assert_eq!(names, vec!["main".to_string(), "feature/login".to_string()]);
    assert_eq!(cursor.as_deref(), Some("CUR2"));
}

#[test]
fn the_last_branch_page_reports_no_cursor() {
    let json = r#"{"data":{"repository":{"defaultBranchRef":{"name":"main"},
        "refs":{"nodes":[{"name":"main"}],
        "pageInfo":{"hasNextPage":false,"endCursor":"CUR2"}}}}}"#;
    match parse_branches_page(json) {
        Ok((_, cursor)) => assert!(cursor.is_none(), "got: {cursor:?}"),
        Err(error) => panic!("a well-formed page must parse: {error}"),
    }
}

#[test]
fn the_default_branch_is_read_from_the_same_page() {
    let json = r#"{"data":{"repository":{"defaultBranchRef":{"name":"trunk"},
        "refs":{"nodes":[],"pageInfo":{"hasNextPage":false}}}}}"#;
    assert_eq!(parse_default_branch_name(json).as_deref(), Some("trunk"));
}

#[test]
fn an_empty_repository_has_no_default_branch() {
    let json = r#"{"data":{"repository":{"defaultBranchRef":null,
        "refs":{"nodes":[],"pageInfo":{"hasNextPage":false}}}}}"#;
    assert!(parse_default_branch_name(json).is_none());
}

#[test]
fn a_graphql_error_envelope_is_surfaced_when_listing_branches() {
    let json = r#"{"data":null,"errors":[{"message":"Bad credentials"}]}"#;
    match parse_branches_page(json) {
        Err(GhError::ApiError(message)) => {
            assert!(message.contains("Bad credentials"), "got: {message}");
        }
        other => panic!("expected ApiError, got {other:?}"),
    }
}

// ── Creating the pull request ──────────────────────────────────────────────

#[test]
fn creating_a_pull_request_posts_head_base_title_and_body() {
    let args = build_create_pr_args(
        "acme",
        "widgets",
        "feature/login",
        "main",
        "Add login",
        "Body text",
    );
    assert_eq!(args[0], "api");
    assert_eq!(
        flag_value(&args, "--method", "").unwrap_or_default(),
        "POST"
    );
    assert!(
        args.iter().any(|arg| arg == "/repos/acme/widgets/pulls"),
        "got: {args:?}"
    );
    assert_eq!(flag_value(&args, "-f", "head="), Some("feature/login"));
    assert_eq!(flag_value(&args, "-f", "base="), Some("main"));
    assert_eq!(flag_value(&args, "-f", "title="), Some("Add login"));
    assert_eq!(flag_value(&args, "-f", "body="), Some("Body text"));
}

#[test]
fn a_multiline_body_is_passed_as_one_argument() {
    let args = build_create_pr_args(
        "acme",
        "widgets",
        "topic",
        "main",
        "T",
        "line one\nline two",
    );
    assert_eq!(flag_value(&args, "-f", "body="), Some("line one\nline two"));
}

#[test]
fn a_created_pull_request_reports_its_number() {
    let json = r#"{"number":42,"html_url":"https://github.com/acme/widgets/pull/42"}"#;
    match parse_created_pr_number(json) {
        Ok(number) => assert_eq!(number, 42),
        Err(error) => panic!("a create response must parse: {error}"),
    }
}

#[test]
fn a_create_response_without_a_number_is_a_parse_error() {
    assert!(matches!(
        parse_created_pr_number(r#"{"html_url":"x"}"#),
        Err(GhError::ParseError(_))
    ));
    assert!(matches!(
        parse_created_pr_number("not json"),
        Err(GhError::ParseError(_))
    ));
}

#[test]
fn a_refused_create_reports_githubs_own_explanation() {
    let json = r#"{"message":"No commits between main and topic","documentation_url":"x"}"#;
    match parse_created_pr_number(json) {
        Err(GhError::ApiError(message)) => {
            assert_eq!(message, "No commits between main and topic");
        }
        other => panic!("expected the explanation to be surfaced, got {other:?}"),
    }
}

#[test]
fn a_created_pull_request_wins_over_any_accompanying_message() {
    let json = r#"{"number":7,"message":"ignored"}"#;
    match parse_created_pr_number(json) {
        Ok(number) => assert_eq!(number, 7),
        Err(error) => panic!("a create that returned a number must succeed: {error}"),
    }
}
