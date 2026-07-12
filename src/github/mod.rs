//! GitHub client boundary — wraps `gh` CLI subprocess calls.
//!
//! @plan PLAN-20260329-ISSUES-MODE.P08
//! @requirement REQ-ISS-013
//! @requirement REQ-ISS-NFR-002
//! @requirement REQ-ISS-NFR-003
//! @pseudocode component-002 lines 01-03
//!
//! This module is intentionally isolated from `crate::ui` and `crate::state`.
//! It depends only on `crate::domain` types for data transfer.

use crate::domain::{
    Issue, IssueComment, IssueDetail, IssueFilter, IssueState, PrCheck, PrFilter, PrReview,
    PrReviewState, PrReviewThread, PrState, PullRequestDetail,
};
use std::process::Command;

mod create_issue;
mod issue_lifecycle;
mod pr_threads;
mod repo_merge;
pub use create_issue::{CreatedIssue, parse_created_issue_json};
pub use issue_lifecycle::{build_close_issue_args, build_delete_issue_args};
use repo_merge::parse_repo_merge_methods;

mod auth_device;
pub use auth_device::{
    AUTH_SCOPES, DeviceCode, build_auth_login_args, build_auth_login_env,
    is_not_authenticated_error, parse_device_code, redact_device_codes,
};
mod viewer;
pub use viewer::{build_assign_issue_args, build_viewer_login_args, parse_viewer_login};

mod actions;
pub use actions::{
    WorkflowRunListResponse, build_runs_api_path, parse_api_runs_json, parse_jobs_json,
    parse_runs_json, parse_single_run_json, parse_workflows_json,
};

mod parse;
use parse::{active_issue_type_filter, issue_type_requires_search_filter};
pub use parse::{
    build_issue_search_args, build_list_issues_args, categorize_error, parse_comments_json,
    parse_created_comment_json, parse_issue_detail_json, parse_issue_search_json,
    parse_issues_json, sort_issues,
};

mod parse_pr;
pub use parse_pr::{
    build_pr_comments_query, build_pr_review_threads_query, build_pr_search_args,
    build_pr_search_query, parse_check_status, parse_checks_rollup, parse_pr_check,
    parse_pr_review, parse_pr_review_threads, parse_pr_review_threads_cursor, parse_pr_state,
    parse_pull_request_detail_json, parse_pull_requests_json, parse_review_decision,
    parse_thread_reply_json, rollup_nodes, sort_pull_requests,
};

/// Error types for GitHub CLI operations.
///
/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-013
/// @pseudocode component-002 lines 84-91
#[derive(Debug)]
pub enum GhError {
    NotAuthenticated(String),
    NotInstalled,
    RateLimited,
    AccessDenied(String),
    ApiError(String),
    ParseError(String),
    NetworkError(String),
}

impl std::fmt::Display for GhError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAuthenticated(msg) => write!(f, "Not authenticated: {msg}"),
            Self::NotInstalled => write!(f, "GitHub CLI (gh) is not installed"),
            Self::RateLimited => write!(f, "GitHub API rate limit exceeded"),
            Self::AccessDenied(msg) => write!(f, "Access denied: {msg}"),
            Self::ApiError(msg) => write!(f, "API error: {msg}"),
            Self::ParseError(msg) => write!(f, "Parse error: {msg}"),
            Self::NetworkError(msg) => write!(f, "Network error: {msg}"),
        }
    }
}

impl std::error::Error for GhError {}

/// Response from listing issues.
pub struct IssueListResponse {
    pub issues: Vec<Issue>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

/// Response from listing pull requests (mirrors [`IssueListResponse`]).
///
/// `#[derive(Default)]` is sound because `Vec`, `Option`, and `bool` all
/// implement `Default`; the empty-vec default needs no `PullRequest: Default`.
///
/// @plan PLAN-20260624-PR-MODE.P06
/// @requirement REQ-PR-006
/// @pseudocode component-002 lines 05-06
#[derive(Default)]
pub struct PrListResponse {
    pub pull_requests: Vec<crate::domain::PullRequest>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

/// Response from listing comments.
pub struct CommentsResponse {
    pub comments: Vec<IssueComment>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

const ISSUE_DETAIL_COMMENT_PAGE_SIZE: u32 = 30;
/// Default page size for the PR list GraphQL search query.
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-006
const PR_LIST_PAGE_SIZE: u32 = 30;

/// Payload for sending issue context to an agent.
///
/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-011
/// @pseudocode component-002 lines 70-83
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SendPayload {
    pub repository: String,
    pub issue_number: u64,
    pub issue_title: String,
    pub issue_body: String,
    pub issue_state: String,
    pub issue_labels: Vec<String>,
    pub issue_assignees: Vec<String>,
    pub focused_comment: Option<String>,
    pub focused_comment_author: Option<String>,
    pub issue_base_prompt: String,
}

/// Payload for sending PR context to an agent (mirrors [`SendPayload`]'s
/// structured, owned-field design). Carries NO `prompt_markdown`/`work_dir`/
/// `signature` — those are not payload concerns.
///
/// @plan PLAN-20260624-PR-MODE.P06
/// @requirement REQ-PR-011
/// @pseudocode component-002 lines 123-129
#[derive(Default)]
pub struct PrSendPayload {
    pub repository: String,
    pub pr_number: u64,
    pub pr_title: String,
    pub pr_body: String,
    pub pr_state: String,
    pub head_ref: String,
    pub base_ref: String,
    pub external_url: String,
    pub review_summary: Vec<String>,
    pub check_summary: Vec<String>,
    pub focused_comment: Option<String>,
    pub focused_comment_author: Option<String>,
    pub pr_base_prompt: String,
}

/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-013
/// @pseudocode component-002 lines 01-03
#[derive(Clone, Copy, Debug)]
pub struct GhClient;

impl GhClient {
    /// Create a new client instance.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Check if gh CLI is authenticated.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-013
    /// @pseudocode component-002 lines 04-08
    pub fn check_auth(&self) -> Result<(), GhError> {
        let output = Command::new("gh")
            .args(["auth", "status"])
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    GhError::NotInstalled
                } else {
                    GhError::NetworkError(e.to_string())
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(categorize_error(output.status.code().unwrap_or(1), &stderr));
        }

        Ok(())
    }

    /// List issues for a repository with filtering and pagination.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-006
    /// @pseudocode component-002 lines 09-25
    pub fn list_issues(
        &self,
        owner: &str,
        repo: &str,
        filter: &IssueFilter,
        cursor: Option<&str>,
        page_size: u32,
    ) -> Result<IssueListResponse, GhError> {
        fetch_issue_search_page(owner, repo, filter, cursor, page_size)
    }

    /// Get full issue detail.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-009
    /// @pseudocode component-002 lines 26-32
    pub fn get_issue_detail(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<IssueDetail, GhError> {
        let output = Command::new("gh")
            .args([
                "issue",
                "view",
                "--repo",
                &format!("{owner}/{repo}"),
                &number.to_string(),
                "--json",
                "number,title,state,author,createdAt,updatedAt,labels,assignees,milestone,body,url,comments,id",
            ])
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    GhError::NotInstalled
                } else {
                    GhError::NetworkError(e.to_string())
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(categorize_error(output.status.code().unwrap_or(1), &stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut detail = parse_issue_detail_json(&stdout)?;
        let comments_response =
            self.list_comments(owner, repo, number, None, ISSUE_DETAIL_COMMENT_PAGE_SIZE)?;
        detail.comments = comments_response.comments;
        detail.comments_cursor = comments_response.cursor;
        detail.has_more_comments = comments_response.has_more;
        Ok(detail)
    }

    /// List comments for an issue with pagination.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-009
    /// @pseudocode component-002 lines 33-43
    pub fn list_comments(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        cursor: Option<&str>,
        page_size: u32,
    ) -> Result<CommentsResponse, GhError> {
        // Build GraphQL query using parameterized variables for safety.
        // `databaseId` is the numeric REST comment id needed for update/delete
        // operations; GraphQL `id` is an opaque node id and is not parseable.
        let query = if cursor.is_some() {
            "query($owner: String!, $repo: String!, $number: Int!, $first: Int!, $after: String) { repository(owner: $owner, name: $repo) { issue(number: $number) { comments(first: $first, after: $after) { nodes { id databaseId author { login } createdAt lastEditedAt body } pageInfo { hasNextPage endCursor } } } } }"
        } else {
            "query($owner: String!, $repo: String!, $number: Int!, $first: Int!) { repository(owner: $owner, name: $repo) { issue(number: $number) { comments(first: $first) { nodes { id databaseId author { login } createdAt lastEditedAt body } pageInfo { hasNextPage endCursor } } } } }"
        };

        let mut args = vec![
            "api".to_string(),
            "graphql".to_string(),
            "-f".to_string(),
            format!("query={query}"),
            "-F".to_string(),
            format!("owner={owner}"),
            "-F".to_string(),
            format!("repo={repo}"),
            "-F".to_string(),
            format!("number={number}"),
            "-F".to_string(),
            format!("first={page_size}"),
        ];
        if let Some(c) = cursor {
            args.push("-F".to_string());
            args.push(format!("after={c}"));
        }

        let output = Command::new("gh").args(&args).output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                GhError::NotInstalled
            } else {
                GhError::NetworkError(e.to_string())
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(categorize_error(output.status.code().unwrap_or(1), &stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let (comments, end_cursor, has_more) = parse_comments_json(&stdout)?;

        Ok(CommentsResponse {
            comments,
            cursor: end_cursor,
            has_more,
        })
    }

    /// Create a new issue.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-011
    pub fn create_issue(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        body: &str,
    ) -> Result<CreatedIssue, GhError> {
        let output = Command::new("gh")
            .args([
                "api",
                "--method",
                "POST",
                &format!("/repos/{owner}/{repo}/issues"),
                "-f",
                &format!("title={title}"),
                "-f",
                &format!("body={body}"),
            ])
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    GhError::NotInstalled
                } else {
                    GhError::NetworkError(e.to_string())
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(categorize_error(output.status.code().unwrap_or(1), &stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_created_issue_json(&stdout)
    }

    /// Resolve the authenticated viewer's login (`gh api user --jq .login`).
    ///
    /// Used to self-assign an issue on send-to-agent (issue #186).
    pub fn viewer_login(&self) -> Result<String, GhError> {
        let stdout = Self::run_gh(&build_viewer_login_args())?;
        parse_viewer_login(&stdout)
    }

    /// Assign an issue to `assignee` via the assignees REST endpoint. Used for
    /// self-assignment on send-to-agent (issue #186); failures are non-blocking
    /// warnings at the caller, so this surfaces the raw `GhError`.
    pub fn assign_issue(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        assignee: &str,
    ) -> Result<(), GhError> {
        let args = build_assign_issue_args(owner, repo, number, assignee);
        Self::run_gh(&args)?;
        Ok(())
    }

    /// Create a new comment on an issue.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-011
    /// @pseudocode component-002 lines 44-48
    pub fn create_comment(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &str,
    ) -> Result<IssueComment, GhError> {
        let output = Command::new("gh")
            .args([
                "api",
                "--method",
                "POST",
                &format!("/repos/{owner}/{repo}/issues/{number}/comments"),
                "-f",
                &format!("body={body}"),
            ])
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    GhError::NotInstalled
                } else {
                    GhError::NetworkError(e.to_string())
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(categorize_error(output.status.code().unwrap_or(1), &stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_created_comment_json(&stdout)
    }

    /// Update an existing comment.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-011
    /// @pseudocode component-002 lines 49-56
    pub fn update_comment(
        &self,
        owner: &str,
        repo: &str,
        comment_id: u64,
        body: &str,
    ) -> Result<(), GhError> {
        let output = Command::new("gh")
            .args([
                "api",
                "--method",
                "PATCH",
                &format!("/repos/{owner}/{repo}/issues/comments/{comment_id}"),
                "-f",
                &format!("body={body}"),
            ])
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    GhError::NotInstalled
                } else {
                    GhError::NetworkError(e.to_string())
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(categorize_error(output.status.code().unwrap_or(1), &stderr));
        }

        Ok(())
    }

    /// Update an issue's title and body text.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-011
    /// @pseudocode component-002 lines 57-61
    pub fn update_issue(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        title: &str,
        body: &str,
    ) -> Result<(), GhError> {
        let output = Command::new("gh")
            .args([
                "issue",
                "edit",
                "--repo",
                &format!("{owner}/{repo}"),
                &number.to_string(),
                "--title",
                title,
                "--body",
                body,
            ])
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    GhError::NotInstalled
                } else {
                    GhError::NetworkError(e.to_string())
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(categorize_error(output.status.code().unwrap_or(1), &stderr));
        }

        Ok(())
    }

    /// Build a send-to-agent payload from current context.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-011
    /// @pseudocode component-002 lines 70-83
    #[must_use]
    pub fn build_send_payload(
        repo_slug: &str,
        detail: &IssueDetail,
        focused_comment: Option<&IssueComment>,
        issue_base_prompt: &str,
    ) -> SendPayload {
        let state_str = match detail.state {
            IssueState::Open => "open",
            IssueState::Closed => "closed",
        };

        SendPayload {
            repository: repo_slug.to_string(),
            issue_number: detail.number,
            issue_title: detail.title.clone(),
            issue_body: detail.body.clone(),
            issue_state: state_str.to_string(),
            issue_labels: detail.labels.clone(),
            issue_assignees: detail.assignees.clone(),
            focused_comment: focused_comment.map(|c| c.body.clone()),
            focused_comment_author: focused_comment.map(|c| c.author_login.clone()),
            issue_base_prompt: issue_base_prompt.to_string(),
        }
    }

    /// List pull requests for a repository with filtering and pagination.
    ///
    /// Builds the GraphQL search args, runs `gh`, parses the response, sorts
    /// by `updated_at` DESC, and returns the paginated response with the REAL
    /// `endCursor`/`hasNextPage`.
    ///
    /// @plan PLAN-20260624-PR-MODE.P08
    /// @requirement REQ-PR-006
    /// @pseudocode component-002 lines 22-34
    pub fn list_pull_requests(
        &self,
        owner: &str,
        name: &str,
        filter: &PrFilter,
        cursor: Option<&str>,
    ) -> Result<PrListResponse, GhError> {
        let args = build_pr_search_args(owner, name, filter, cursor, PR_LIST_PAGE_SIZE);
        let stdout = Self::run_gh(&args)?;
        let mut response = parse_pull_requests_json(&stdout)?;
        sort_pull_requests(&mut response.pull_requests);
        Ok(response)
    }

    /// Get full pull-request detail.
    ///
    /// Fetches metadata via `gh pr view --json` (the `--json` set OMITS
    /// `comments`), then sources the first comment page via a SEPARATE
    /// `list_pr_comments` call (mirroring `get_issue_detail`'s
    /// comments-sourcing, but via `repository.pullRequest` not
    /// `repository.issue`).
    ///
    /// @plan PLAN-20260624-PR-MODE.P08
    /// @requirement REQ-PR-009
    /// @pseudocode component-002 lines 74-101
    pub fn get_pull_request_detail(
        &self,
        owner: &str,
        name: &str,
        number: u64,
    ) -> Result<PullRequestDetail, GhError> {
        let args = vec![
            "pr".to_string(),
            "view".to_string(),
            number.to_string(),
            "--repo".to_string(),
            format!("{owner}/{name}"),
            "--json".to_string(),
            "number,title,state,mergedAt,author,createdAt,updatedAt,headRefName,baseRefName,isDraft,labels,assignees,milestone,body,url,reviewDecision,statusCheckRollup,reviews,mergeable,mergeStateStatus".to_string(),
        ];
        let repo = format!("{owner}/{name}");

        let (detail, comments, threads) = std::thread::scope(|s| {
            let detail_handle = s.spawn(|| {
                let stdout = Self::run_gh(&args)?;
                parse_pull_request_detail_json(&stdout, &repo)
            });
            let comments_handle = s.spawn(|| {
                self.list_pr_comments(owner, name, number, None, ISSUE_DETAIL_COMMENT_PAGE_SIZE)
            });
            let threads_handle = s.spawn(|| self.list_pr_review_threads(owner, name, number));

            let worker_panic = |what: &str| {
                GhError::ApiError(format!(
                    "{what} worker panicked for {owner}/{name}#{number}"
                ))
            };
            let detail = detail_handle
                .join()
                .map_err(|_| worker_panic("metadata fetch"));
            let comments = comments_handle
                .join()
                .map_err(|_| worker_panic("comments fetch"));
            let threads = threads_handle.join().unwrap_or_else(|_| {
                tracing::warn!(
                    "review-threads fetch worker panicked for {owner}/{name}#{number}; \
                     rendering detail without threads"
                );
                Vec::new()
            });
            Ok::<_, GhError>((detail??, comments??, threads))
        })?;

        let mut detail = detail;
        detail.comments = comments.comments;
        detail.comments_cursor = comments.cursor;
        detail.has_more_comments = comments.has_more;
        assign_threads_to_reviews(&mut detail.reviews, threads);
        Ok(detail)
    }

    /// List comments for a pull request with pagination (PR-specific GraphQL
    /// path querying `repository.pullRequest(number:).comments` — NOT
    /// `repository.issue`, which is NULL for a PR number; P00A §2d). Reuses
    /// `parse_comments_json` for nodes and the page-info helper for the cursor.
    ///
    /// @plan PLAN-20260624-PR-MODE.P08
    /// @requirement REQ-PR-010
    /// @pseudocode component-002 lines 102-107
    pub fn list_pr_comments(
        &self,
        owner: &str,
        name: &str,
        number: u64,
        cursor: Option<&str>,
        page_size: u32,
    ) -> Result<CommentsResponse, GhError> {
        let mut args = vec![
            "api".to_string(),
            "graphql".to_string(),
            "-f".to_string(),
            format!("query={}", build_pr_comments_query(cursor.is_some())),
            "-F".to_string(),
            format!("owner={owner}"),
            "-F".to_string(),
            format!("repo={name}"),
            "-F".to_string(),
            format!("number={number}"),
            "-F".to_string(),
            format!("first={page_size}"),
        ];
        if let Some(c) = cursor {
            args.push("-F".to_string());
            args.push(format!("after={c}"));
        }
        let stdout = Self::run_gh(&args)?;
        let (comments, end_cursor, has_more) = parse_comments_json(&stdout)?;
        Ok(CommentsResponse {
            comments,
            cursor: end_cursor,
            has_more,
        })
    }

    /// Create a new comment on a pull request (uses the issue comment REST
    /// endpoint `/repos/{owner}/{repo}/issues/{number}/comments`, which
    /// accepts a PR number). Reuses `parse_created_comment_json`.
    ///
    /// @plan PLAN-20260624-PR-MODE.P08
    /// @requirement REQ-PR-010
    /// @pseudocode component-002 lines 108-114
    pub fn create_pr_comment(
        &self,
        owner: &str,
        name: &str,
        number: u64,
        body: &str,
    ) -> Result<IssueComment, GhError> {
        let args = vec![
            "api".to_string(),
            "--method".to_string(),
            "POST".to_string(),
            format!("/repos/{owner}/{name}/issues/{number}/comments"),
            "-f".to_string(),
            format!("body={body}"),
        ];
        let stdout = Self::run_gh(&args)?;
        parse_created_comment_json(&stdout)
    }

    /// Open a pull request in the default browser via `gh pr view --web`.
    ///
    /// @plan PLAN-20260624-PR-MODE.P08
    /// @requirement REQ-PR-012
    /// @pseudocode component-002 lines 115-122
    pub fn open_pull_request_in_browser(
        &self,
        owner: &str,
        name: &str,
        number: u64,
    ) -> Result<(), GhError> {
        let args = vec![
            "pr".to_string(),
            "view".to_string(),
            number.to_string(),
            "--repo".to_string(),
            format!("{owner}/{name}"),
            "--web".to_string(),
        ];
        Self::run_gh(&args)?;
        Ok(())
    }

    /// Merge a pull request via `gh pr merge` with the chosen method.
    ///
    /// @plan PLAN-20260624-PR-MODE.P08
    /// @requirement REQ-PR-009
    /// @pseudocode component-002 lines 115-122
    pub fn merge_pull_request(
        &self,
        owner: &str,
        name: &str,
        number: u64,
        method: crate::domain::MergeMethod,
    ) -> Result<(), GhError> {
        let args = vec![
            "pr".to_string(),
            "merge".to_string(),
            number.to_string(),
            "--repo".to_string(),
            format!("{owner}/{name}"),
            method.gh_flag().to_string(),
        ];
        Self::run_gh(&args)?;
        Ok(())
    }

    /// Fetch the repo's allowed merge methods via `gh api repos/{owner}/{repo}`.
    ///
    /// Returns the subset of [`MergeMethod`] allowed by the repository's merge
    /// settings. On any error, returns an empty `Vec` (the chooser treats
    /// unknown as "all available" — graceful degradation).
    ///
    /// @plan PLAN-20260624-PR-MODE.P08
    /// @requirement REQ-PR-009
    /// @pseudocode component-002 lines 115-122
    pub fn get_repo_merge_methods(
        &self,
        owner: &str,
        name: &str,
    ) -> Result<Vec<crate::domain::MergeMethod>, GhError> {
        let args = vec![
            "api".to_string(),
            format!("repos/{owner}/{name}"),
            "--jq".to_string(),
            "{allow_merge_commit, allow_squash_merge, allow_rebase_merge}".to_string(),
        ];
        let stdout = Self::run_gh(&args)?;
        Ok(parse_repo_merge_methods(&stdout))
    }

    /// Build a send-to-agent payload from PR context (mirrors
    /// [`build_send_payload`]). Pure assembly; no I/O. Carries NO
    /// `prompt_markdown`/`work_dir`/`signature` — those come from the agent.
    ///
    /// @plan PLAN-20260624-PR-MODE.P08
    /// @requirement REQ-PR-011
    /// @pseudocode component-002 lines 123-136
    #[must_use]
    pub fn build_pr_send_payload(
        repo_slug: &str,
        pr_detail: &PullRequestDetail,
        focused_comment: Option<&IssueComment>,
        pr_base_prompt: &str,
    ) -> PrSendPayload {
        PrSendPayload {
            repository: repo_slug.to_string(),
            pr_number: pr_detail.number,
            pr_title: pr_detail.title.clone(),
            pr_body: pr_detail.body.clone(),
            pr_state: pr_state_str(pr_detail.state).to_string(),
            head_ref: pr_detail.head_ref.clone(),
            base_ref: pr_detail.base_ref.clone(),
            external_url: pr_detail.external_url.clone(),
            review_summary: summarize_pr_reviews(&pr_detail.reviews),
            check_summary: summarize_pr_checks(&pr_detail.checks),
            focused_comment: focused_comment.map(|c| c.body.clone()),
            focused_comment_author: focused_comment.map(|c| c.author_login.clone()),
            pr_base_prompt: pr_base_prompt.to_string(),
        }
    }

    /// Run `gh` with the given args, returning stdout on success. Encapsulates
    /// the established error idiom: `NotFound→NotInstalled` else
    /// `NetworkError`; non-zero exit → `categorize_error`.
    pub(super) fn run_gh(args: &[String]) -> Result<String, GhError> {
        let output = Command::new("gh").args(args).output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                GhError::NotInstalled
            } else {
                GhError::NetworkError(e.to_string())
            }
        })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(categorize_error(output.status.code().unwrap_or(1), &stderr));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

impl Default for GhClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Distribute fetched review threads onto the review structs.
///
/// GitHub's `reviewThreads` connection is on `PullRequest`, not on each
/// `Review`. Each thread carries the id of the review that opened it (from
/// its first comment's `pullRequestReview`), so threads are attached to THEIR
/// parent review — mirroring the github.com grouping where each review card
/// shows its own batch of inline comments. Threads whose parent review id is
/// missing or matches no fetched review fall back to the first review so
/// nothing is dropped. When there are no reviews at all, threads are dropped
/// (there is no review slot to hold them). The renderer flattens
/// `reviews.iter().flat_map(|r| &r.review_threads)` so this preserves all
/// threads for display, in per-review chronological order.
pub(crate) fn assign_threads_to_reviews(reviews: &mut [PrReview], threads: Vec<PrReviewThread>) {
    if threads.is_empty() || reviews.is_empty() {
        return;
    }
    // Build a review_id → index map once (O(N)) instead of a per-thread
    // linear scan (O(N×M)). Cloning the ids into owned Strings avoids holding
    // &str borrows of `reviews` while we mutably push into it below.
    let mut id_to_idx = std::collections::HashMap::with_capacity(reviews.len());
    for (i, r) in reviews.iter().enumerate() {
        if let Some(id) = &r.review_id {
            id_to_idx.insert(id.clone(), i);
        }
    }
    for thread in threads {
        let parent_idx = thread
            .review_id
            .as_deref()
            .and_then(|tid| id_to_idx.get(tid).copied())
            .unwrap_or(0);
        reviews[parent_idx].review_threads.push(thread);
    }
}

/// Map a [`PrState`] to its lowercase send-payload string.
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-011
/// @pseudocode component-002 lines 130-136
fn pr_state_str(state: PrState) -> &'static str {
    match state {
        PrState::Open => "open",
        PrState::Closed => "closed",
        PrState::Merged => "merged",
    }
}

/// Build the display-only review-summary strings for the send payload.
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-011
/// @pseudocode component-002 lines 130-136
fn summarize_pr_reviews(reviews: &[PrReview]) -> Vec<String> {
    reviews
        .iter()
        .map(|r| format!("{}: {}", r.author_login, review_state_str(r.state)))
        .collect()
}

/// Map a [`PrReviewState`] to a display label.
fn review_state_str(state: PrReviewState) -> &'static str {
    match state {
        PrReviewState::Approved => "approved",
        PrReviewState::ChangesRequested => "changes_requested",
        PrReviewState::Commented => "commented",
        PrReviewState::Pending => "pending",
        PrReviewState::Dismissed => "dismissed",
        PrReviewState::ReviewRequired => "review_required",
        PrReviewState::None => "none",
    }
}

/// Build the display-only check-summary strings for the send payload.
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-011
/// @pseudocode component-002 lines 130-136
fn summarize_pr_checks(checks: &[PrCheck]) -> Vec<String> {
    checks
        .iter()
        .map(|c| format!("{}: {}", c.name, c.conclusion))
        .collect()
}

fn fetch_issue_search_page(
    owner: &str,
    repo: &str,
    filter: &IssueFilter,
    cursor: Option<&str>,
    page_size: u32,
) -> Result<IssueListResponse, GhError> {
    if active_issue_type_filter(filter).is_some() && issue_type_requires_search_filter(filter) {
        return fetch_issue_search_filtered_pages(owner, repo, filter, cursor, page_size);
    }
    fetch_issue_search_raw_page(owner, repo, filter, cursor, page_size)
}

fn fetch_issue_search_filtered_pages(
    owner: &str,
    repo: &str,
    filter: &IssueFilter,
    cursor: Option<&str>,
    page_size: u32,
) -> Result<IssueListResponse, GhError> {
    let Some(issue_type) = active_issue_type_filter(filter) else {
        return fetch_issue_search_raw_page(owner, repo, filter, cursor, page_size);
    };
    let mut search_cursor = cursor.map(str::to_string);
    let mut collected = Vec::new();
    let mut response_cursor: Option<String>;
    let mut response_has_more: bool;

    loop {
        let response =
            fetch_issue_search_raw_page(owner, repo, filter, search_cursor.as_deref(), page_size)?;
        response_cursor = response.cursor.clone();
        response_has_more = response.has_more;
        collected.extend(
            response
                .issues
                .into_iter()
                .filter(|issue| issue.issue_type.eq_ignore_ascii_case(issue_type)),
        );
        if collected.len() > page_size as usize {
            response_has_more = true;
            break;
        }

        if collected.len() >= page_size as usize || !response_has_more {
            break;
        }
        if response.cursor == search_cursor {
            response_has_more = false;
            break;
        }
        search_cursor = response.cursor;
    }

    Ok(IssueListResponse {
        issues: collected,
        cursor: response_cursor,
        has_more: response_has_more,
    })
}

fn fetch_issue_search_raw_page(
    owner: &str,
    repo: &str,
    filter: &IssueFilter,
    cursor: Option<&str>,
    page_size: u32,
) -> Result<IssueListResponse, GhError> {
    let args = build_issue_search_args(owner, repo, filter, cursor, page_size);

    let output = Command::new("gh").args(&args).output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            GhError::NotInstalled
        } else {
            GhError::NetworkError(e.to_string())
        }
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(categorize_error(output.status.code().unwrap_or(1), &stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_issue_search_json(&stdout)
}
