//! Issue lifecycle gh API methods (close + delete) for issue #182.
//!
//! Extracted from `mod.rs` to keep the main client file under the 1000-line
//! limit. These methods are on `impl super::GhClient` and use `Self::run_gh`,
//! mirroring `pr_threads.rs` for the GraphQL `deleteIssue` mutation and the
//! CLI `gh issue close` command.

use super::{GhClient, GhError};
use crate::domain::CloseReason;

/// The GraphQL `deleteIssue` mutation query string.
const DELETE_ISSUE_QUERY: &str =
    "mutation($id: ID!) { deleteIssue(input: {issueId: $id}) { clientMutationId } }";

/// The GraphQL `markIssueAsDuplicate` mutation query string.
const MARK_DUPLICATE_QUERY: &str = "mutation($canonical: ID!, $duplicate: ID!) { \
    markIssueAsDuplicate(input: {canonicalId: $canonical, duplicateId: $duplicate}) { \
    clientMutationId } }";

/// The GraphQL query to resolve an issue's node id by number.
const ISSUE_NODE_ID_QUERY: &str = "query($owner: String!, $repo: String!, $number: Int!) { \
    repository(owner: $owner, name: $repo) { issue(number: $number) { id } } }";

/// The GraphQL `closeIssue` mutation (issue #204).
///
/// Replaces the REST `gh issue close --reason` + `markIssueAsDuplicate`
/// two-step with a single call that carries `stateReason` and an optional
/// `duplicateIssueId` as first-class fields.
const CLOSE_ISSUE_GRAPHQL_QUERY: &str = "mutation($input: CloseIssueInput!) { \
    closeIssue(input: $input) { issue { state stateReason } } }";

impl GhClient {
    /// Close an issue via `gh issue close` (by number, no node id required).
    pub fn close_issue(&self, owner: &str, repo: &str, number: u64) -> Result<(), GhError> {
        let args = build_close_issue_args(owner, repo, number);
        Self::run_gh(&args)?;
        Ok(())
    }

    /// Close an issue with a reason via `gh issue close --reason` (issue #188).
    pub fn close_issue_with_reason(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        reason: CloseReason,
    ) -> Result<(), GhError> {
        let args = build_close_issue_with_reason_args(owner, repo, number, reason);
        Self::run_gh(&args)?;
        Ok(())
    }

    /// Close an issue via the GraphQL `closeIssue` mutation with a native
    /// `stateReason` and optional `duplicateIssueId` (issue #204).
    ///
    /// Single GraphQL call replacing the REST `gh issue close --reason` +
    /// `markIssueAsDuplicate` two-step. `node_id` is the issue being closed;
    /// `duplicate_node_id` is the canonical (duplicate-of) issue's node id,
    /// required only when `reason == Duplicate`.
    ///
    /// Parses the GraphQL response for a top-level `errors` array (GitHub
    /// returns HTTP 200 with errors on mutation failures like already-closed,
    /// permission denied, or issue not found).
    pub fn close_issue_graphql(
        &self,
        node_id: &str,
        reason: CloseReason,
        duplicate_node_id: Option<&str>,
    ) -> Result<(), GhError> {
        let args = build_close_issue_graphql_args(node_id, reason, duplicate_node_id);
        let stdout = Self::run_gh(&args)?;
        parse_graphql_errors(&stdout)
    }

    /// Mark an issue as a duplicate of another via the GraphQL
    /// `markIssueAsDuplicate` mutation (issue #188).
    ///
    /// `canonical_node_id` is the ORIGINAL (duplicate-of) issue's node id;
    /// `duplicate_node_id` is the issue being closed as a duplicate.
    ///
    /// Failures here are non-fatal at the caller — the close itself has
    /// already succeeded. The caller should log a warning rather than surface
    /// a hard error.
    pub fn mark_issue_as_duplicate(
        &self,
        canonical_node_id: &str,
        duplicate_node_id: &str,
    ) -> Result<(), GhError> {
        let args = build_mark_duplicate_args(canonical_node_id, duplicate_node_id);
        Self::run_gh(&args)?;
        Ok(())
    }

    /// Resolve an issue number to its GraphQL node id (issue #188).
    ///
    /// Used by the duplicate-close path to get the node ids needed for
    /// `markIssueAsDuplicate`.
    pub fn resolve_issue_node_id(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<String, GhError> {
        let args = build_issue_node_id_args(owner, repo, number);
        let stdout = Self::run_gh(&args)?;
        parse_issue_node_id_json(&stdout)
    }

    /// Delete an issue via the GraphQL `deleteIssue` mutation (requires node id).
    pub fn delete_issue(&self, node_id: &str) -> Result<(), GhError> {
        let args = build_delete_issue_args(node_id);
        Self::run_gh(&args)?;
        Ok(())
    }
}

/// Build the `gh issue close` args for the given issue.
#[must_use]
pub fn build_close_issue_args(owner: &str, repo: &str, number: u64) -> Vec<String> {
    vec![
        "issue".to_string(),
        "close".to_string(),
        number.to_string(),
        "--repo".to_string(),
        format!("{owner}/{repo}"),
    ]
}

/// Build the `gh issue close --reason` args for the given issue + reason
/// (issue #188).
#[must_use]
pub fn build_close_issue_with_reason_args(
    owner: &str,
    repo: &str,
    number: u64,
    reason: CloseReason,
) -> Vec<String> {
    vec![
        "issue".to_string(),
        "close".to_string(),
        number.to_string(),
        "--repo".to_string(),
        format!("{owner}/{repo}"),
        "--reason".to_string(),
        reason.gh_reason_flag().to_string(),
    ]
}

/// Map a `CloseReason` to the GraphQL `IssueClosedStateReason` enum string
/// (issue #204).
///
/// GitHub's `closeIssue` mutation accepts `COMPLETED`, `NOT_PLANNED`, and
/// `DUPLICATE`. `Invalid` has no GraphQL representation and maps to
/// `NOT_PLANNED`, matching the REST `gh issue close --reason` behavior.
#[must_use]
pub fn close_reason_graphql_enum(reason: CloseReason) -> &'static str {
    match reason {
        CloseReason::Completed => "COMPLETED",
        CloseReason::NotPlanned | CloseReason::Invalid => "NOT_PLANNED",
        CloseReason::Duplicate => "DUPLICATE",
    }
}

/// Build the `gh api graphql` args for the `closeIssue` mutation (issue #204).
///
/// Constructs a single GraphQL call carrying `stateReason` and an optional
/// `duplicateIssueId`, replacing the REST + `markIssueAsDuplicate` two-step.
/// `node_id` is the GraphQL node id of the issue being closed;
/// `duplicate_node_id` is the canonical (duplicate-of) issue's node id,
/// required only when the reason is `Duplicate`.
#[must_use]
pub fn build_close_issue_graphql_args(
    node_id: &str,
    reason: CloseReason,
    duplicate_node_id: Option<&str>,
) -> Vec<String> {
    let input = build_close_issue_input_json(node_id, reason, duplicate_node_id);
    vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={CLOSE_ISSUE_GRAPHQL_QUERY}"),
        "-F".to_string(),
        format!("input={input}"),
    ]
}

/// Build the JSON-encoded `CloseIssueInput` object for the `closeIssue`
/// mutation (issue #204).
///
/// Pure function: produces the exact JSON string that `gh api graphql -F
/// input=<json>` sends. Uses `serde_json` for proper escaping of node ids.
/// Unit-testable without iocraft or network.
#[must_use]
pub fn build_close_issue_input_json(
    node_id: &str,
    reason: CloseReason,
    duplicate_node_id: Option<&str>,
) -> String {
    let state_reason = close_reason_graphql_enum(reason);
    let mut input = serde_json::json!({
        "issueId": node_id,
        "stateReason": state_reason,
    });
    if reason == CloseReason::Duplicate
        && let Some(dup_id) = duplicate_node_id
        && !dup_id.is_empty()
    {
        input["duplicateIssueId"] = serde_json::Value::String(dup_id.to_string());
    }
    input.to_string()
}

/// Build the `gh api graphql` args for the `deleteIssue` mutation.
#[must_use]
pub fn build_delete_issue_args(node_id: &str) -> Vec<String> {
    vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={DELETE_ISSUE_QUERY}"),
        "-F".to_string(),
        format!("id={node_id}"),
    ]
}

/// Build the `gh api graphql` args for the `markIssueAsDuplicate` mutation
/// (issue #188).
#[must_use]
pub fn build_mark_duplicate_args(canonical_node_id: &str, duplicate_node_id: &str) -> Vec<String> {
    vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={MARK_DUPLICATE_QUERY}"),
        "-F".to_string(),
        format!("canonical={canonical_node_id}"),
        "-F".to_string(),
        format!("duplicate={duplicate_node_id}"),
    ]
}

/// Build the `gh api graphql` args to resolve an issue's node id by number
/// (issue #188).
#[must_use]
pub fn build_issue_node_id_args(owner: &str, repo: &str, number: u64) -> Vec<String> {
    vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={ISSUE_NODE_ID_QUERY}"),
        "-F".to_string(),
        format!("owner={owner}"),
        "-F".to_string(),
        format!("repo={repo}"),
        "-F".to_string(),
        format!("number={number}"),
    ]
}

/// Parse the GraphQL JSON response to extract `data.repository.issue.id`
/// (issue #188).
///
/// Returns `GhError::ParseError` when the path is missing, the issue is
/// `null` (not found), or the id is empty.
pub fn parse_issue_node_id_json(stdout: &str) -> Result<String, GhError> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Err(GhError::ParseError(
            "empty response when resolving issue node id".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(trimmed)
        .map_err(|e| GhError::ParseError(format!("invalid JSON resolving issue node id: {e}")))?;
    // GitHub's GraphQL API returns HTTP 200 with a top-level `errors` array on
    // validation/auth/resource failures. Surface those messages rather than a
    // generic "not found" so duplicate-close failures are diagnosable.
    if let Some(messages) = graphql_error_messages(&value) {
        return Err(GhError::ApiError(format!(
            "GraphQL error resolving issue node id: {}",
            messages.join("; ")
        )));
    }
    let id = value
        .get("data")
        .and_then(|d| d.get("repository"))
        .and_then(|r| r.get("issue"))
        .and_then(|i| i.get("id"))
        .and_then(|id| id.as_str())
        .ok_or_else(|| GhError::ParseError("issue node id not found in response".to_string()))?;
    if id.is_empty() {
        return Err(GhError::ParseError(
            "issue node id is empty in response".to_string(),
        ));
    }
    Ok(id.to_string())
}

/// Parse a GraphQL mutation response for a top-level `errors` array.
///
/// GitHub's GraphQL API returns HTTP 200 with `{"errors": [...]}` on
/// mutation failures (e.g., already closed, permission denied, issue not
/// found). Without this check, `run_gh` returns `Ok(())` on a failed
/// mutation, causing state desynchronization between the TUI and GitHub.
fn parse_graphql_errors(stdout: &str) -> Result<(), GhError> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).map_err(|e| {
        GhError::ParseError(format!("invalid JSON parsing closeIssue response: {e}"))
    })?;
    if let Some(messages) = graphql_error_messages(&value) {
        return Err(GhError::ApiError(format!(
            "GraphQL closeIssue mutation failed: {}",
            messages.join("; ")
        )));
    }
    Ok(())
}

/// Extract non-empty GraphQL error messages from a parsed response, if any.
fn graphql_error_messages(value: &serde_json::Value) -> Option<Vec<String>> {
    let errors = value.get("errors")?.as_array()?;
    let messages: Vec<String> = errors
        .iter()
        .filter_map(|e| e.get("message").and_then(|m| m.as_str()).map(String::from))
        .collect();
    if messages.is_empty() {
        None
    } else {
        Some(messages)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_close_issue_args, build_close_issue_graphql_args, build_close_issue_input_json,
        build_close_issue_with_reason_args, build_delete_issue_args, build_issue_node_id_args,
        build_mark_duplicate_args, close_reason_graphql_enum, parse_graphql_errors,
        parse_issue_node_id_json,
    };
    use crate::domain::CloseReason;
    use crate::github::GhError;

    #[test]
    fn build_close_issue_args_constructs_correct_command() {
        let args = build_close_issue_args("acme", "widgets", 42);
        assert_eq!(
            args,
            vec!["issue", "close", "42", "--repo", "acme/widgets",]
        );
    }

    #[test]
    fn build_delete_issue_args_constructs_graphql_mutation() {
        let node_id = "I_kwDORSOxIM7sXe5_";
        let args = build_delete_issue_args(node_id);
        assert_eq!(args.len(), 6, "delete args should have 6 elements");
        assert_eq!(args[0], "api");
        assert_eq!(args[1], "graphql");
        assert_eq!(args[2], "-f");
        assert!(
            args[3].contains("deleteIssue"),
            "query should contain deleteIssue mutation"
        );
        assert!(args[3].contains("issueId: $id"));
        assert_eq!(args[4], "-F");
        assert_eq!(args[5], format!("id={node_id}"));
    }

    #[test]
    fn build_close_with_reason_completed() {
        let args =
            build_close_issue_with_reason_args("acme", "widgets", 42, CloseReason::Completed);
        assert_eq!(
            args,
            vec![
                "issue",
                "close",
                "42",
                "--repo",
                "acme/widgets",
                "--reason",
                "completed",
            ]
        );
    }

    #[test]
    fn build_close_with_reason_not_planned() {
        let args =
            build_close_issue_with_reason_args("acme", "widgets", 42, CloseReason::NotPlanned);
        assert_eq!(
            args,
            vec![
                "issue",
                "close",
                "42",
                "--repo",
                "acme/widgets",
                "--reason",
                "not planned",
            ]
        );
    }

    #[test]
    fn build_close_with_reason_duplicate_maps_to_not_planned() {
        let args =
            build_close_issue_with_reason_args("acme", "widgets", 42, CloseReason::Duplicate);
        assert_eq!(
            args,
            vec![
                "issue",
                "close",
                "42",
                "--repo",
                "acme/widgets",
                "--reason",
                "not planned",
            ]
        );
    }

    #[test]
    fn build_close_with_reason_invalid_maps_to_not_planned() {
        let args = build_close_issue_with_reason_args("acme", "widgets", 42, CloseReason::Invalid);
        assert_eq!(
            args,
            vec![
                "issue",
                "close",
                "42",
                "--repo",
                "acme/widgets",
                "--reason",
                "not planned",
            ]
        );
    }

    #[test]
    fn build_mark_duplicate_args_constructs_graphql_mutation() {
        let canonical = "I_kwDOABC123";
        let duplicate = "I_kwDOXYZ789";
        let args = build_mark_duplicate_args(canonical, duplicate);
        assert_eq!(args.len(), 8);
        assert_eq!(args[0], "api");
        assert_eq!(args[1], "graphql");
        assert_eq!(args[2], "-f");
        assert!(args[3].contains("markIssueAsDuplicate"));
        assert!(args[3].contains("canonicalId: $canonical"));
        assert!(args[3].contains("duplicateId: $duplicate"));
        assert_eq!(args[4], "-F");
        assert_eq!(args[5], format!("canonical={canonical}"));
        assert_eq!(args[6], "-F");
        assert_eq!(args[7], format!("duplicate={duplicate}"));
    }

    #[test]
    fn build_issue_node_id_args_constructs_graphql_query() {
        let args = build_issue_node_id_args("acme", "widgets", 42);
        assert_eq!(args.len(), 10);
        assert_eq!(args[0], "api");
        assert_eq!(args[1], "graphql");
        assert_eq!(args[2], "-f");
        assert!(args[3].contains("repository(owner: $owner, name: $repo)"));
        assert!(args[3].contains("issue(number: $number)"));
        assert_eq!(args[4], "-F");
        assert_eq!(args[5], "owner=acme");
        assert_eq!(args[6], "-F");
        assert_eq!(args[7], "repo=widgets");
        assert_eq!(args[8], "-F");
        assert_eq!(args[9], "number=42");
    }

    #[test]
    fn parse_issue_node_id_extracts_id_from_valid_json() {
        let json = r#"{"data":{"repository":{"issue":{"id":"I_kwDORSOxIM7sXe5_"}}}}"#;
        let result = parse_issue_node_id_json(json);
        let id = match result {
            Ok(ref id) => id.as_str(),
            Err(ref e) => panic!("parse should succeed, got error: {e}"),
        };
        assert_eq!(id, "I_kwDORSOxIM7sXe5_");
    }

    #[test]
    fn parse_issue_node_id_errors_on_empty_response() {
        let result = parse_issue_node_id_json("");
        assert!(matches!(result, Err(GhError::ParseError(_))));
    }

    #[test]
    fn parse_issue_node_id_errors_on_missing_issue_key() {
        let json = r#"{"data":{"repository":{}}}"#;
        let result = parse_issue_node_id_json(json);
        assert!(matches!(result, Err(GhError::ParseError(_))));
    }

    #[test]
    fn parse_issue_node_id_errors_on_missing_repository_key() {
        let json = r#"{"data":{}}"#;
        let result = parse_issue_node_id_json(json);
        assert!(matches!(result, Err(GhError::ParseError(_))));
    }

    #[test]
    fn parse_issue_node_id_errors_on_null_issue() {
        // GitHub returns `"issue": null` when the number does not resolve.
        let json = r#"{"data":{"repository":{"issue":null}}}"#;
        let result = parse_issue_node_id_json(json);
        assert!(
            matches!(result, Err(GhError::ParseError(_))),
            "a null issue must surface a parse error, not succeed"
        );
    }

    #[test]
    fn parse_issue_node_id_handles_unicode_escape_in_path_neighbors() {
        // serde_json correctly decodes \uXXXX escapes anywhere in the payload;
        // the target id here is plain ASCII but a neighboring field uses one.
        let json = r#"{"data":{"repository":{"name":"widgets","issue":{"id":"I_kwABC123","title":"\u00e9"}}}}"#;
        let result = parse_issue_node_id_json(json);
        match result {
            Ok(id) => assert_eq!(id, "I_kwABC123"),
            Err(e) => panic!("unicode neighbor must not break parsing: {e}"),
        }
    }

    #[test]
    fn parse_issue_node_id_errors_on_invalid_json() {
        let result = parse_issue_node_id_json("{ not valid json");
        assert!(matches!(result, Err(GhError::ParseError(_))));
    }

    #[test]
    fn parse_issue_node_id_surfaces_graphql_errors_array() {
        // GitHub returns HTTP 200 with a top-level `errors` array on failures.
        let json = r#"{"data":null,"errors":[{"message":"issue not found"}]}"#;
        let result = parse_issue_node_id_json(json);
        match result {
            Err(GhError::ApiError(msg)) => assert!(
                msg.contains("issue not found"),
                "should surface the GraphQL error message, got: {msg}"
            ),
            other => panic!("expected ApiError, got {other:?}"),
        }
    }

    #[test]
    fn close_reason_graphql_enum_maps_correctly() {
        assert_eq!(
            close_reason_graphql_enum(CloseReason::Completed),
            "COMPLETED"
        );
        assert_eq!(
            close_reason_graphql_enum(CloseReason::NotPlanned),
            "NOT_PLANNED"
        );
        assert_eq!(
            close_reason_graphql_enum(CloseReason::Duplicate),
            "DUPLICATE"
        );
        assert_eq!(
            close_reason_graphql_enum(CloseReason::Invalid),
            "NOT_PLANNED"
        );
    }

    #[test]
    fn build_close_issue_input_json_completed() {
        let json = build_close_issue_input_json("I_kw123", CloseReason::Completed, None);
        assert_eq!(json, r#"{"issueId":"I_kw123","stateReason":"COMPLETED"}"#);
    }

    #[test]
    fn build_close_issue_input_json_not_planned() {
        let json = build_close_issue_input_json("I_kw123", CloseReason::NotPlanned, None);
        assert_eq!(json, r#"{"issueId":"I_kw123","stateReason":"NOT_PLANNED"}"#);
    }

    #[test]
    fn build_close_issue_input_json_duplicate_with_target() {
        let json =
            build_close_issue_input_json("I_dup", CloseReason::Duplicate, Some("I_canonical"));
        assert!(
            json.contains(r#""duplicateIssueId":"I_canonical""#),
            "duplicate close input should include duplicateIssueId: {json}"
        );
        assert!(json.contains(r#""stateReason":"DUPLICATE""#));
    }

    #[test]
    fn build_close_issue_input_json_duplicate_without_target_omits_field() {
        let json = build_close_issue_input_json("I_dup", CloseReason::Duplicate, None);
        assert!(
            !json.contains("duplicateIssueId"),
            "duplicate close without a target should NOT include duplicateIssueId: {json}"
        );
        assert!(json.contains(r#""stateReason":"DUPLICATE""#));
    }

    #[test]
    fn build_close_issue_input_json_duplicate_with_empty_target_omits_field() {
        let json = build_close_issue_input_json("I_dup", CloseReason::Duplicate, Some(""));
        assert!(
            !json.contains("duplicateIssueId"),
            "duplicate close with empty target should NOT include duplicateIssueId: {json}"
        );
    }

    #[test]
    fn build_close_issue_input_json_invalid_maps_to_not_planned() {
        let json = build_close_issue_input_json("I_kw123", CloseReason::Invalid, None);
        assert!(json.contains(r#""stateReason":"NOT_PLANNED""#));
    }

    #[test]
    fn build_close_issue_graphql_args_shape() {
        let args = build_close_issue_graphql_args("I_kw123", CloseReason::Completed, None);
        assert_eq!(args.len(), 6);
        assert_eq!(args[0], "api");
        assert_eq!(args[1], "graphql");
        assert_eq!(args[2], "-f");
        assert!(
            args[3].contains("closeIssue"),
            "query should contain closeIssue mutation: {}",
            args[3]
        );
        assert_eq!(args[4], "-F");
        assert!(
            args[5].starts_with("input="),
            "should pass input as -F parameter: {}",
            args[5]
        );
        let input = &args[5]["input=".len()..];
        assert!(input.contains(r#""issueId":"I_kw123""#));
        assert!(input.contains(r#""stateReason":"COMPLETED""#));
    }

    #[test]
    fn build_close_issue_graphql_args_duplicate_includes_target() {
        let args =
            build_close_issue_graphql_args("I_dup", CloseReason::Duplicate, Some("I_canonical"));
        let input = &args[5]["input=".len()..];
        assert!(
            input.contains(r#""duplicateIssueId":"I_canonical""#),
            "duplicate graphql args should include duplicateIssueId: {input}"
        );
    }

    #[test]
    fn parse_graphql_errors_ok_on_success() {
        let json =
            r#"{"data":{"closeIssue":{"issue":{"state":"CLOSED","stateReason":"COMPLETED"}}}}"#;
        assert!(parse_graphql_errors(json).is_ok());
    }

    #[test]
    fn parse_graphql_errors_ok_on_empty() {
        assert!(parse_graphql_errors("").is_ok());
        assert!(parse_graphql_errors("   ").is_ok());
    }

    #[test]
    fn parse_graphql_errors_surfaces_errors_array() {
        let json = r#"{"data":null,"errors":[{"message":"issue already closed"}]}"#;
        let result = parse_graphql_errors(json);
        match result {
            Err(GhError::ApiError(msg)) => assert!(
                msg.contains("issue already closed"),
                "should surface the GraphQL error message, got: {msg}"
            ),
            other => panic!("expected ApiError, got {other:?}"),
        }
    }

    #[test]
    fn parse_graphql_errors_errors_on_invalid_json() {
        let result = parse_graphql_errors("{ not valid");
        assert!(matches!(result, Err(GhError::ParseError(_))));
    }
}
