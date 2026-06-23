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

use crate::domain::{Issue, IssueComment, IssueDetail, IssueFilter, IssueState};
use std::process::Command;

mod create_issue;
pub use create_issue::{CreatedIssue, parse_created_issue_json};

mod parse;
use parse::build_issue_search_args;
pub use parse::{
    build_list_issues_args, categorize_error, parse_comments_json, parse_created_comment_json,
    parse_issue_detail_json, parse_issue_search_json, parse_issues_json, sort_issues,
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

/// Response from listing comments.
pub struct CommentsResponse {
    pub comments: Vec<IssueComment>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

const ISSUE_DETAIL_COMMENT_PAGE_SIZE: u32 = 30;

/// Payload for sending issue context to an agent.
///
/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-011
/// @pseudocode component-002 lines 70-83
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
                "number,title,state,author,createdAt,updatedAt,labels,assignees,milestone,body,url,comments",
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

    /// Update an issue's body text.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-011
    /// @pseudocode component-002 lines 57-61
    pub fn update_issue_body(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &str,
    ) -> Result<(), GhError> {
        let output = Command::new("gh")
            .args([
                "issue",
                "edit",
                "--repo",
                &format!("{owner}/{repo}"),
                &number.to_string(),
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
}

impl Default for GhClient {
    fn default() -> Self {
        Self::new()
    }
}
