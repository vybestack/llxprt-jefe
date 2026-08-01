//! Tests for `parse_pr.rs` (extracted to keep that file under the per-file
//! line limit). Covers the GraphQL enum-string mergeable parsing added in #487.

use super::{
    effective_check_nodes, parse_checks_rollup, parse_mergeable_value,
    parse_pull_request_detail_json, parse_pull_requests_json,
};

use crate::domain::PrCheckStatus;
use serde_json::{Value, json};

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

// =============================================================================
// Issue #514 — superseded check-run attempts must not poison the rollup
// =============================================================================

/// Build a detail JSON string with the given statusCheckRollup array.
fn detail_json_with_rollup(rollup: Vec<Value>) -> String {
    json!({
        "number": 509,
        "title": "t",
        "state": "OPEN",
        "mergedAt": null,
        "author": {"login": "a"},
        "createdAt": "",
        "updatedAt": "",
        "headRefName": "h",
        "headRefOid": "s",
        "baseRefName": "m",
        "isDraft": false,
        "labels": [],
        "assignees": [],
        "milestone": null,
        "body": "",
        "url": "",
        "reviewDecision": null,
        "statusCheckRollup": rollup,
        "reviews": [],
        "mergeable": true,
        "mergeStateStatus": "CLEAN"
    })
    .to_string()
}

/// A GitHub Actions CheckRun node for a given workflow, job name, attempt
/// timing, and conclusion.
fn check_run(name: &str, workflow: &str, started_at: &str, conclusion: &str) -> Value {
    json!({
        "__typename": "CheckRun",
        "name": name,
        "workflowName": workflow,
        "startedAt": started_at,
        "completedAt": started_at,
        "status": "COMPLETED",
        "conclusion": conclusion,
        "detailsUrl": format!("https://github.com/o/r/runs/{started_at}")
    })
}

/// A2: an older CANCELLED attempt superseded by a newer SUCCESS attempt of the
/// same effective check must aggregate to Success (the PR #509 shape).
#[test]
fn rollup_superseded_canceled_resolves_to_success() {
    let nodes = vec![
        check_run(
            "Mergeability gate / Mergeability gate",
            "LLxprt PR Review",
            "2026-07-29T02:10:41Z",
            "CANCELLED",
        ),
        check_run(
            "Mergeability gate / Mergeability gate",
            "LLxprt PR Review",
            "2026-07-29T02:32:43Z",
            "SUCCESS",
        ),
    ];
    assert_eq!(
        parse_checks_rollup(&nodes),
        crate::domain::PrCheckStatus::Success,
        "a superseded CANCELLED attempt must not poison the rollup when a newer SUCCESS exists"
    );
}

/// A5: the PR detail check list must omit superseded attempts so its rows
/// cannot disagree with its aggregate glyph.
#[test]
fn detail_omits_superseded_attempts() {
    let detail = match parse_pull_request_detail_json(
        &detail_json_with_rollup(vec![
            check_run("build", "CI", "2026-07-29T02:10:41Z", "CANCELLED"),
            check_run("build", "CI", "2026-07-29T02:32:43Z", "SUCCESS"),
        ]),
        "o/r",
    ) {
        Ok(detail) => detail,
        Err(error) => panic!("detail should parse: {error}"),
    };
    assert_eq!(
        detail.checks.len(),
        1,
        "detail check list must omit the superseded CANCELLED attempt"
    );
    assert_eq!(
        detail.checks_status,
        crate::domain::PrCheckStatus::Success,
        "detail aggregate must reflect the effective SUCCESS attempt"
    );
}
/// A legacy StatusContext node (status API) for a given context name and state.
fn status_context(context: &str, started_at: &str, state: &str) -> Value {
    json!({
        "__typename": "StatusContext",
        "context": context,
        "startedAt": started_at,
        "state": state,
        "targetUrl": format!("https://example.com/{context}")
    })
}

/// Older FAILURE superseded by a newer SUCCESS of the same check resolves to
/// Success (a re-run that passes clears the earlier failure).
#[test]
fn rollup_failed_then_successful_resolves_to_success() {
    let nodes = vec![
        check_run("build", "CI", "2026-07-29T02:00:00Z", "FAILURE"),
        check_run("build", "CI", "2026-07-29T02:30:00Z", "SUCCESS"),
    ];
    assert_eq!(
        parse_checks_rollup(&nodes),
        PrCheckStatus::Success,
        "a superseded FAILURE must clear when a newer SUCCESS exists"
    );
}

/// A3: when the latest attempt genuinely failed, the rollup stays Failure even
/// though an earlier attempt succeeded.
#[test]
fn rollup_successful_then_failed_resolves_to_failure() {
    let nodes = vec![
        check_run("build", "CI", "2026-07-29T02:00:00Z", "SUCCESS"),
        check_run("build", "CI", "2026-07-29T02:30:00Z", "FAILURE"),
    ];
    assert_eq!(
        parse_checks_rollup(&nodes),
        PrCheckStatus::Failure,
        "the latest FAILED attempt must drive the aggregate to Failure"
    );
}

/// A4: a still-pending latest attempt projects as Pending, regardless of an
/// earlier completed attempt.
#[test]
fn rollup_latest_pending_projects_pending() {
    let nodes = vec![
        check_run("build", "CI", "2026-07-29T02:00:00Z", "SUCCESS"),
        {
            let mut node = check_run("build", "CI", "2026-07-29T02:30:00Z", "");
            node["status"] = json!("IN_PROGRESS");
            node["conclusion"] = json!(null);
            node
        },
    ];
    assert_eq!(
        parse_checks_rollup(&nodes),
        PrCheckStatus::Pending,
        "the latest IN_PROGRESS attempt must project as Pending"
    );
}

/// A6: independent StatusContext entries aggregate correctly (no CheckRun
/// supersession logic interferes), and a failed status context still fails.
#[test]
fn rollup_status_context_only_aggregates_correctly() {
    let all_success = vec![
        status_context("CodeRabbit", "2026-07-29T02:00:00Z", "SUCCESS"),
        status_context("codecov", "2026-07-29T02:01:00Z", "SUCCESS"),
    ];
    assert_eq!(
        parse_checks_rollup(&all_success),
        PrCheckStatus::Success,
        "two independent successful status contexts aggregate to Success"
    );

    let one_failed = vec![
        status_context("CodeRabbit", "2026-07-29T02:00:00Z", "SUCCESS"),
        status_context("codecov", "2026-07-29T02:01:00Z", "ERROR"),
    ];
    assert_eq!(
        parse_checks_rollup(&one_failed),
        PrCheckStatus::Failure,
        "a failed status context must fail the aggregate"
    );
}

/// A1: effective_check_nodes keeps only the latest attempt per identity,
/// preserving first-occurrence order, and does not merge different identities.
#[test]
fn effective_nodes_keeps_latest_per_identity() {
    let nodes = vec![
        check_run("build", "CI", "2026-07-29T02:00:00Z", "CANCELLED"),
        check_run("test", "CI", "2026-07-29T02:05:00Z", "SUCCESS"),
        check_run("build", "CI", "2026-07-29T02:30:00Z", "SUCCESS"),
    ];
    let effective = effective_check_nodes(&nodes);
    assert_eq!(
        effective.len(),
        2,
        "the two build attempts collapse to one; the test attempt is independent"
    );
    // First-occurrence order is preserved (build appeared first).
    assert_eq!(
        effective[0].get("name").and_then(Value::as_str),
        Some("build"),
    );
    assert_eq!(
        effective[0].get("conclusion").and_then(Value::as_str),
        Some("SUCCESS"),
        "the kept build attempt is the latest (SUCCESS), not the superseded CANCELLED"
    );
    assert_eq!(
        effective[1].get("name").and_then(Value::as_str),
        Some("test"),
    );
}

/// Identity disambiguation: two checks that share a job name but belong to
/// different workflows are distinct effective checks and must both be kept.
#[test]
fn effective_nodes_distinguishes_same_name_different_workflow() {
    let nodes = vec![
        check_run("build", "CI", "2026-07-29T02:00:00Z", "SUCCESS"),
        check_run("build", "Release", "2026-07-29T02:30:00Z", "SUCCESS"),
    ];
    let effective = effective_check_nodes(&nodes);
    assert_eq!(
        effective.len(),
        2,
        "same job name under different workflows are distinct checks"
    );
}

/// A GraphQL-list-path CheckRun node. Unlike the `gh pr view --json` shape,
/// raw GraphQL exposes the workflow name nested under
/// `checkSuite.workflowRun.workflow.name` (NOT a top-level `workflowName`),
/// and every GitHub Actions workflow reports the same `app.slug`. This is the
/// production shape for the PR list query.
fn graphql_check_run(name: &str, workflow: &str, started_at: &str, conclusion: &str) -> Value {
    json!({
        "__typename": "CheckRun",
        "name": name,
        "startedAt": started_at,
        "completedAt": started_at,
        "status": "COMPLETED",
        "conclusion": conclusion,
        "detailsUrl": format!("https://github.com/o/r/runs/{started_at}"),
        "checkSuite": {
            "app": {"slug": "github-actions"},
            "workflowRun": {"workflow": {"name": workflow}}
        }
    })
}

/// Cross-workflow disambiguation on the raw-GraphQL list shape: two jobs named
/// "Build" under different workflows share the same `app.slug` ("github-actions")
/// but must NOT collapse. With a FAILURE in one workflow and a SUCCESS in the
/// other, the aggregate must be Failure — proving both are kept. An app-slug-only
/// disambiguator would collapse them to the latest (SUCCESS) and wrongly report
/// Success.
#[test]
fn graphql_path_same_name_different_workflow_stays_distinct() {
    let nodes = vec![
        graphql_check_run("Build", "CI", "2026-07-29T02:00:00Z", "FAILURE"),
        graphql_check_run("Build", "Release", "2026-07-29T02:30:00Z", "SUCCESS"),
    ];
    let effective = effective_check_nodes(&nodes);
    assert_eq!(
        effective.len(),
        2,
        "same job name under different workflows must stay distinct on the GraphQL shape"
    );
    assert_eq!(
        parse_checks_rollup(&nodes),
        PrCheckStatus::Failure,
        "the CI Build failure must drive the aggregate to Failure even though \
         the Release Build succeeded"
    );
}

/// Supersession still collapses attempts within the SAME workflow on the
/// raw-GraphQL list shape (the nested workflow name groups them): an older
/// CANCELLED and a newer SUCCESS for "Build" under "CI" resolve to Success.
#[test]
fn graphql_path_supersession_collapses_within_same_workflow() {
    let nodes = vec![
        graphql_check_run("Build", "CI", "2026-07-29T02:00:00Z", "CANCELLED"),
        graphql_check_run("Build", "CI", "2026-07-29T02:30:00Z", "SUCCESS"),
    ];
    assert_eq!(
        effective_check_nodes(&nodes).len(),
        1,
        "two attempts of the same workflow job must collapse to one"
    );
    assert_eq!(
        parse_checks_rollup(&nodes),
        PrCheckStatus::Success,
        "the superseded CANCELLED must clear when the same workflow job later succeeds"
    );
}

/// A7: the PR list projection routes through the same effective-check
/// selection as the detail projection, so a superseded rollup on a list node
/// aggregates to Success too.
#[test]
fn list_projection_dedupes_superseded_checks() {
    let json = json!({
        "data": { "search": { "nodes": [
            {
                "number": 509, "title": "t", "state": "OPEN", "mergedAt": null,
                "author": {"login": "a"}, "updatedAt": "", "headRefName": "h",
                "headRefOid": "s", "baseRefName": "m", "isDraft": false,
                "mergeable": "MERGEABLE",
                "statusCheckRollup": { "contexts": { "nodes": [
                    check_run("build", "CI", "2026-07-29T02:00:00Z", "CANCELLED"),
                    check_run("build", "CI", "2026-07-29T02:30:00Z", "SUCCESS")
                ] } }
            }
        ], "pageInfo": {"hasNextPage": false, "endCursor": null} } }
    })
    .to_string();
    let response = match parse_pull_requests_json(&json) {
        Ok(response) => response,
        Err(error) => panic!("list JSON should parse: {error:?}"),
    };
    assert_eq!(response.pull_requests.len(), 1);
    assert_eq!(
        response.pull_requests[0].checks_status,
        PrCheckStatus::Success,
        "list projection must use the same dedup as the detail projection"
    );
}
