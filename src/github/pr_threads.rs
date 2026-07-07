//! Review-thread gh API methods for pull requests (issue #119).
//!
//! Extracted from `mod.rs` to keep the main client file under the 1000-line
//! limit. These methods are on `impl super::GhClient` and use `Self::run_gh`
//! plus the parse helpers from `parse_pr`.

use crate::domain::{IssueComment, PrReviewThread};

use super::parse_pr::{
    build_pr_review_threads_query, parse_pr_review_threads, parse_thread_reply_json,
};
use super::{GhClient, GhError};

impl GhClient {
    /// List review threads for a pull request.
    ///
    /// Runs the `build_pr_review_threads_query` GraphQL query targeting
    /// `repository.pullRequest(number:).reviewThreads` and parses the
    /// result via `parse_pr_review_threads`. On parse/network error returns an
    /// empty vec (graceful degradation — the detail load must not fail because
    /// the threads fetch failed).
    ///
    /// @requirement REQ-PR-009
    #[must_use]
    pub fn list_pr_review_threads(
        &self,
        owner: &str,
        name: &str,
        number: u64,
    ) -> Vec<PrReviewThread> {
        let args = vec![
            "api".to_string(),
            "graphql".to_string(),
            "-f".to_string(),
            format!("query={}", build_pr_review_threads_query(false)),
            "-F".to_string(),
            format!("owner={owner}"),
            "-F".to_string(),
            format!("repo={name}"),
            "-F".to_string(),
            format!("number={number}"),
            "-F".to_string(),
            "first=20".to_string(),
        ];
        let Ok(stdout) = Self::run_gh(&args) else {
            return Vec::new();
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) else {
            return Vec::new();
        };
        parse_pr_review_threads(&json)
    }
    /// Resolve a review thread via the GraphQL `resolveReviewThread` mutation.
    ///
    /// @requirement REQ-PR-009
    pub fn resolve_review_thread(&self, thread_id: &str) -> Result<bool, GhError> {
        let query = "mutation($thread: ID!) { resolveReviewThread(input: {threadId: $thread}) { thread { isResolved } } }";
        let args = vec![
            "api".to_string(),
            "graphql".to_string(),
            "-f".to_string(),
            format!("query={query}"),
            "-F".to_string(),
            format!("thread={thread_id}"),
        ];
        Self::run_gh(&args)?;
        Ok(true)
    }

    /// Unresolve a review thread via the GraphQL `unresolveReviewThread` mutation.
    ///
    /// @requirement REQ-PR-009
    pub fn unresolve_review_thread(&self, thread_id: &str) -> Result<bool, GhError> {
        let query = "mutation($thread: ID!) { unresolveReviewThread(input: {threadId: $thread}) { thread { isResolved } } }";
        let args = vec![
            "api".to_string(),
            "graphql".to_string(),
            "-f".to_string(),
            format!("query={query}"),
            "-F".to_string(),
            format!("thread={thread_id}"),
        ];
        Self::run_gh(&args)?;
        Ok(false)
    }

    /// Reply to a review thread via the GraphQL `addPullRequestReviewThreadReply`
    /// mutation. Returns the created comment on success.
    ///
    /// @requirement REQ-PR-009
    pub fn create_pr_review_thread_reply(
        &self,
        thread_id: &str,
        body: &str,
    ) -> Result<IssueComment, GhError> {
        let query = "mutation($thread: ID!, $body: String!) { addPullRequestReviewThreadReply(input: {pullRequestReviewThreadId: $thread, body: $body}) { comment { databaseId author { login } createdAt body } } }";
        let args = vec![
            "api".to_string(),
            "graphql".to_string(),
            "-f".to_string(),
            format!("query={query}"),
            "-F".to_string(),
            format!("thread={thread_id}"),
            "-f".to_string(),
            format!("body={body}"),
        ];
        let stdout = Self::run_gh(&args)?;
        parse_thread_reply_json(&stdout)
    }
}
