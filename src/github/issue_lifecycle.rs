//! Issue lifecycle gh API methods (close + delete) for issue #182.
//!
//! Extracted from `mod.rs` to keep the main client file under the 1000-line
//! limit. These methods are on `impl super::GhClient` and use `Self::run_gh`,
//! mirroring `pr_threads.rs` for the GraphQL `deleteIssue` mutation and the
//! CLI `gh issue close` command.

use super::{GhClient, GhError};

/// The GraphQL `deleteIssue` mutation query string.
const DELETE_ISSUE_QUERY: &str =
    "mutation($id: ID!) { deleteIssue(input: {issueId: $id}) { clientMutationId } }";

impl GhClient {
    /// Close an issue via `gh issue close` (by number, no node id required).
    pub fn close_issue(&self, owner: &str, repo: &str, number: u64) -> Result<(), GhError> {
        let args = build_close_issue_args(owner, repo, number);
        Self::run_gh(&args)?;
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::{build_close_issue_args, build_delete_issue_args};

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
        assert_eq!(args[0], "api", "first arg should be 'api'");
        assert_eq!(args[1], "graphql", "second arg should be 'graphql'");
        assert_eq!(args[2], "-f", "third arg should be '-f'");
        assert!(
            args[3].contains("deleteIssue"),
            "query should contain deleteIssue mutation"
        );
        assert!(
            args[3].contains("issueId: $id"),
            "query should reference issueId variable"
        );
        assert_eq!(args[4], "-F", "fifth arg should be '-F'");
        assert_eq!(
            args[5],
            format!("id={node_id}"),
            "last arg should carry the node id"
        );
    }
}
