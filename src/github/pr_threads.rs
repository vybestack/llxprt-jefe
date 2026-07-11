//! Review-thread gh API methods for pull requests (issue #119).
//!
//! Extracted from `mod.rs` to keep the main client file under the 1000-line
//! limit. These methods are on `impl super::GhClient` and use `Self::run_gh`
//! plus the parse helpers from `parse_pr`.

use crate::domain::{IssueComment, PrReviewThread};

use super::parse_pr::{
    build_pr_review_threads_query, parse_pr_review_threads, parse_pr_review_threads_cursor,
    parse_thread_reply_json,
};
use super::{GhClient, GhError};

/// Page size for the review-threads GraphQL connection.
const PR_REVIEW_THREADS_PAGE_SIZE: u32 = 50;

/// Hard cap on thread pages fetched per PR, bounding worst-case latency on
/// pathological PRs (cap × page size = 1000 threads).
const PR_REVIEW_THREADS_MAX_PAGES: u32 = 20;

impl GhClient {
    /// List review threads for a pull request, following pagination up to
    /// `PR_REVIEW_THREADS_MAX_PAGES` × `PR_REVIEW_THREADS_PAGE_SIZE` (1000)
    /// threads.
    ///
    /// Runs the `build_pr_review_threads_query` GraphQL query targeting
    /// `repository.pullRequest(number:).reviewThreads` and parses each page
    /// via `parse_pr_review_threads`, following `pageInfo.hasNextPage` /
    /// `endCursor` until the connection is exhausted or the page cap is hit
    /// (issue #155 follow-up: a single unpaginated `first=20` fetch silently
    /// dropped every thread beyond the first page). On parse/network error
    /// returns the threads collected so far (graceful degradation — the
    /// detail load must not fail because the threads fetch failed).
    ///
    /// @requirement REQ-PR-009
    #[must_use]
    pub fn list_pr_review_threads(
        &self,
        owner: &str,
        name: &str,
        number: u64,
    ) -> Vec<PrReviewThread> {
        let mut threads = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..PR_REVIEW_THREADS_MAX_PAGES {
            let Some(json) = Self::fetch_thread_page(owner, name, number, cursor.as_deref()) else {
                break;
            };
            threads.extend(parse_pr_review_threads(&json));
            cursor = parse_pr_review_threads_cursor(&json);
            if cursor.is_none() {
                break;
            }
        }
        threads
    }

    /// Fetch one page of the review-threads connection. `None` on
    /// network/parse error (degrades to the threads already collected).
    fn fetch_thread_page(
        owner: &str,
        name: &str,
        number: u64,
        cursor: Option<&str>,
    ) -> Option<serde_json::Value> {
        let mut args = vec![
            "api".to_string(),
            "graphql".to_string(),
            "-f".to_string(),
            format!("query={}", build_pr_review_threads_query(cursor.is_some())),
            "-F".to_string(),
            format!("owner={owner}"),
            "-F".to_string(),
            format!("repo={name}"),
            "-F".to_string(),
            format!("number={number}"),
            "-F".to_string(),
            format!("first={PR_REVIEW_THREADS_PAGE_SIZE}"),
        ];
        if let Some(after) = cursor {
            args.push("-F".to_string());
            args.push(format!("after={after}"));
        }
        let stdout = match Self::run_gh(&args) {
            Ok(stdout) => stdout,
            Err(err) => {
                // Degrade gracefully (threads already collected still show),
                // but surface the failure so truncated results on large PRs
                // are diagnosable instead of silently shorter.
                tracing::warn!("review-threads page fetch failed: {err}");
                return None;
            }
        };
        match serde_json::from_str::<serde_json::Value>(&stdout) {
            Ok(json) => Some(json),
            Err(err) => {
                tracing::warn!("review-threads page parse failed: {err}");
                None
            }
        }
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
