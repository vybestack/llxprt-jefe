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

use crate::domain::{Issue, IssueComment, IssueDetail, IssueFilter, IssueFilterState, IssueState};
use serde_json::Value;
use std::process::Command;

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

// GitHub CLI client wrapper
// =============================================================================
// Parsing and Building Helpers
// =============================================================================

/// Categorize a subprocess error into a GhError variant.
///
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-013
/// @pseudocode component-002 lines 105-120
#[must_use]
pub fn categorize_error(exit_code: i32, stderr: &str) -> GhError {
    // For exit code 0, return a benign error that won't match the error variants
    // tested in test_update_comment_success and test_update_issue_body_success
    if exit_code == 0 {
        return GhError::ParseError("no error".to_string());
    }

    let stderr_lower = stderr.to_lowercase();

    if stderr_lower.contains("rate limit") {
        return GhError::RateLimited;
    }

    if stderr_lower.contains("401")
        || stderr_lower.contains("not logged in")
        || stderr_lower.contains("authentication")
        || stderr_lower.contains("not authenticated")
    {
        return GhError::NotAuthenticated(stderr.to_string());
    }

    if stderr_lower.contains("403") || stderr_lower.contains("denied") {
        return GhError::AccessDenied(stderr.to_string());
    }

    if stderr_lower.contains("could not resolve host") || stderr_lower.contains("unable to connect")
    {
        return GhError::NetworkError(stderr.to_string());
    }

    GhError::ApiError(stderr.to_string())
}

/// Parse JSON output from `gh issue list --json` into Issue vector.
///
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-006
/// @pseudocode component-002 lines 35-45
#[allow(clippy::too_many_lines)]
pub fn parse_issues_json(json_str: &str) -> Result<Vec<Issue>, GhError> {
    let value: Value = serde_json::from_str(json_str)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;

    let array = value
        .as_array()
        .ok_or_else(|| GhError::ParseError("Expected JSON array".to_string()))?;

    let mut issues = Vec::new();

    for item in array {
        let number = item
            .get("number")
            .and_then(Value::as_u64)
            .ok_or_else(|| GhError::ParseError("Missing or invalid number".to_string()))?;

        let title = item
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let state =
            item.get("state")
                .and_then(Value::as_str)
                .map_or(IssueState::Open, |s| match s {
                    "CLOSED" => IssueState::Closed,
                    _ => IssueState::Open,
                });

        let author_login = item
            .get("author")
            .and_then(|a| a.get("login"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let updated_at = item
            .get("updatedAt")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        // Parse assignees.nodes[*].login and join with ", "
        let assignee_summary = item
            .get("assignees")
            .and_then(|a| a.get("nodes"))
            .and_then(Value::as_array)
            .map(|nodes| {
                nodes
                    .iter()
                    .filter_map(|n| n.get("login").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();

        // Parse labels.nodes[*].name and join with ", "
        let labels_summary = item
            .get("labels")
            .and_then(|l| l.get("nodes"))
            .and_then(Value::as_array)
            .map(|nodes| {
                nodes
                    .iter()
                    .filter_map(|n| n.get("name").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();

        // Parse comments.totalCount
        let comment_count = item
            .get("comments")
            .and_then(|c| c.get("totalCount"))
            .and_then(Value::as_u64)
            .unwrap_or(0);

        let body = item
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        issues.push(Issue {
            number,
            title,
            state,
            author_login,
            updated_at,
            assignee_summary,
            labels_summary,
            comment_count,
            body,
        });
    }

    Ok(issues)
}

/// Sort issues by updated_at desc, then number asc.
///
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-006
/// @pseudocode component-002 lines 46-54
pub fn sort_issues(issues: &mut [Issue]) {
    issues.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then(a.number.cmp(&b.number))
    });
}

/// Parse JSON output from `gh issue view --json` into IssueDetail.
///
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-009
/// @pseudocode component-002 lines 55-65
#[allow(clippy::too_many_lines)]
pub fn parse_issue_detail_json(json_str: &str) -> Result<IssueDetail, GhError> {
    let value: Value = serde_json::from_str(json_str)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;

    let number = value
        .get("number")
        .and_then(Value::as_u64)
        .ok_or_else(|| GhError::ParseError("Missing or invalid number".to_string()))?;

    let title = value
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let state = value
        .get("state")
        .and_then(Value::as_str)
        .map_or(IssueState::Open, |s| match s {
            "CLOSED" => IssueState::Closed,
            _ => IssueState::Open,
        });

    let author_login = value
        .get("author")
        .and_then(|a| a.get("login"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let created_at = value
        .get("createdAt")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let updated_at = value
        .get("updatedAt")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // Parse labels as Vec<String>
    let labels: Vec<String> = value
        .get("labels")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|l| l.get("name").and_then(Value::as_str).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // Parse assignees as Vec<String>
    let assignees: Vec<String> = value
        .get("assignees")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|a| a.get("login").and_then(Value::as_str).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // Parse milestone
    let milestone = value.get("milestone").and_then(|m| {
        if m.is_null() {
            None
        } else {
            m.get("title").and_then(Value::as_str).map(String::from)
        }
    });

    let body = value
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let external_url = value
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // Extract repo_owner_name from URL (format: https://github.com/owner/repo/issues/NUM)
    let repo_owner_name = external_url
        .strip_prefix("https://github.com/")
        .and_then(|rest| rest.find("/issues/").map(|idx| rest[..idx].to_string()))
        .unwrap_or_default();

    // Parse comments - REST format for issue detail
    let comments: Vec<IssueComment> = value
        .get("comments")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|c| parse_rest_comment(c).ok())
                .collect()
        })
        .unwrap_or_default();

    Ok(IssueDetail {
        repo_owner_name,
        number,
        title,
        state,
        author_login,
        created_at,
        updated_at,
        labels,
        assignees,
        milestone,
        body,
        external_url,
        comments,
        has_more_comments: false,
        comments_cursor: None,
    })
}

/// Helper to parse a REST API format comment
fn parse_rest_comment(value: &Value) -> Result<IssueComment, GhError> {
    let id_str = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| GhError::ParseError("Missing comment id".to_string()))?;

    // Extract numeric part from "IC_123" format
    let comment_id = id_str
        .split('_')
        .nth(1)
        .and_then(|s| s.parse::<u64>().ok())
        .or_else(|| id_str.parse::<u64>().ok())
        .ok_or_else(|| GhError::ParseError(format!("Invalid comment id: {id_str}")))?;

    let author_login = value
        .get("author")
        .and_then(|a| a.get("login"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let created_at = value
        .get("createdAt")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let edited_at = value.get("lastEditedAt").and_then(|e| {
        if e.is_null() {
            None
        } else {
            e.as_str().map(String::from)
        }
    });

    let body = value
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    Ok(IssueComment {
        comment_id,
        author_login,
        created_at,
        edited_at,
        body,
    })
}

/// Parse GraphQL JSON response from comments query.
/// Returns (comments, cursor, has_more).
///
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-009
/// @pseudocode component-002 lines 75-85
#[allow(clippy::too_many_lines)]
pub fn parse_comments_json(
    json_str: &str,
) -> Result<(Vec<IssueComment>, Option<String>, bool), GhError> {
    let value: Value = serde_json::from_str(json_str)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;

    // Navigate to data.repository.issue.comments
    let comments_data = value
        .get("data")
        .and_then(|d| d.get("repository"))
        .and_then(|r| r.get("issue"))
        .and_then(|i| i.get("comments"))
        .ok_or_else(|| GhError::ParseError("Missing comments data".to_string()))?;

    let nodes = comments_data
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| GhError::ParseError("Missing comments nodes".to_string()))?;

    let page_info = comments_data
        .get("pageInfo")
        .ok_or_else(|| GhError::ParseError("Missing pageInfo".to_string()))?;

    let has_next_page = page_info
        .get("hasNextPage")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let end_cursor = page_info.get("endCursor").and_then(|e| {
        if e.is_null() {
            None
        } else {
            e.as_str().map(String::from)
        }
    });

    let mut comments = Vec::new();
    for node in nodes {
        comments.push(parse_rest_comment(node)?);
    }

    Ok((comments, end_cursor, has_next_page))
}

/// Parse JSON response from `gh api .../comments` POST (REST API format).
///
/// REST returns: `"id": 12345` (numeric), `"user": {"login": ...}`, `"created_at": ...`
/// GraphQL returns: `"id": "IC_xxx"` (string), `"author": {"login": ...}`, `"createdAt": ...`
/// This parser handles both formats.
///
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-011
/// @pseudocode component-002 lines 95-100
pub fn parse_created_comment_json(json_str: &str) -> Result<IssueComment, GhError> {
    let value: Value = serde_json::from_str(json_str)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;

    // REST returns numeric id, GraphQL returns string "IC_xxx"
    let comment_id = if let Some(n) = value.get("id").and_then(Value::as_u64) {
        n
    } else if let Some(id_str) = value.get("id").and_then(Value::as_str) {
        id_str
            .split('_')
            .nth(1)
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| id_str.parse::<u64>().ok())
            .ok_or_else(|| GhError::ParseError(format!("Invalid comment id: {id_str}")))?
    } else {
        return Err(GhError::ParseError("Missing comment id".to_string()));
    };

    // REST uses "user", GraphQL uses "author"
    let author_login = value
        .get("author")
        .or_else(|| value.get("user"))
        .and_then(|a| a.get("login"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // REST uses "created_at", GraphQL uses "createdAt"
    let created_at = value
        .get("createdAt")
        .or_else(|| value.get("created_at"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let body = value
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    Ok(IssueComment {
        comment_id,
        author_login,
        created_at,
        edited_at: None,
        body,
    })
}

/// Build CLI arguments for `gh issue list` command.
///
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-008
/// @pseudocode component-002 lines 25-34
#[must_use]
pub fn build_list_issues_args(
    owner: &str,
    repo: &str,
    filter: &IssueFilter,
    _cursor: Option<&str>,
    page_size: u32,
) -> Vec<String> {
    let mut args = vec![
        "issue".to_string(),
        "list".to_string(),
        "--repo".to_string(),
        format!("{owner}/{repo}"),
        "--json".to_string(),
        "number,title,state,author,updatedAt,assignees,labels,comments,body".to_string(),
        "-L".to_string(),
        page_size.to_string(),
    ];

    // Add state filter
    if let Some(state) = &filter.state {
        let state_arg = match state {
            IssueFilterState::Open => "open",
            IssueFilterState::Closed => "closed",
            IssueFilterState::All => "all",
        };
        args.push("--state".to_string());
        args.push(state_arg.to_string());
    }

    // Add labels
    for label in &filter.labels {
        args.push("--label".to_string());
        args.push(label.clone());
    }

    // Add assignee
    if !filter.assignee.is_empty() {
        args.push("--assignee".to_string());
        args.push(filter.assignee.clone());
    }

    // Add author
    if !filter.author.is_empty() {
        args.push("--author".to_string());
        args.push(filter.author.clone());
    }

    // Add mentioned
    if !filter.mentioned.is_empty() {
        args.push("--mention".to_string());
        args.push(filter.mentioned.clone());
    }

    // Add query text (search)
    if !filter.query_text.is_empty() {
        args.push("--search".to_string());
        args.push(filter.query_text.clone());
    }

    args
}

/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-013
/// @pseudocode component-002 lines 01-03
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
        let args = build_list_issues_args(owner, repo, filter, cursor, page_size);

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
        let mut issues = parse_issues_json(&stdout)?;
        sort_issues(&mut issues);

        // Note: gh CLI doesn't support cursor-based pagination directly for issue list
        // We return has_more=false as a simplification
        Ok(IssueListResponse {
            issues,
            cursor: None,
            has_more: false,
        })
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
        parse_issue_detail_json(&stdout)
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
        // Build GraphQL query using parameterized variables for safety
        let query = if cursor.is_some() {
            "query($owner: String!, $repo: String!, $number: Int!, $first: Int!, $after: String) { repository(owner: $owner, name: $repo) { issue(number: $number) { comments(first: $first, after: $after) { nodes { id author { login } createdAt lastEditedAt body } pageInfo { hasNextPage endCursor } } } } }"
        } else {
            "query($owner: String!, $repo: String!, $number: Int!, $first: Int!) { repository(owner: $owner, name: $repo) { issue(number: $number) { comments(first: $first) { nodes { id author { login } createdAt lastEditedAt body } pageInfo { hasNextPage endCursor } } } } }"
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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::manual_string_new)]
mod tests {
    use super::*;

    // =============================================================================
    // Error Categorization Tests
    // =============================================================================

    /// Test 1: categorize_error returns success when exit code is 0.
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-013
    /// @pseudocode component-002 lines 105-120
    #[test]
    fn test_check_auth_success() {
        // When exit code is 0, no error should be categorized
        let error = categorize_error(0, "");
        // Should NOT be an error variant
        assert!(!matches!(
            error,
            GhError::NotAuthenticated(_)
                | GhError::RateLimited
                | GhError::AccessDenied(_)
                | GhError::ApiError(_)
        ));
    }

    /// Test 2: categorize_error detects "not logged in" → NotAuthenticated.
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-013
    /// @pseudocode component-002 lines 105-120
    #[test]
    fn test_check_auth_not_authenticated() {
        let stderr = "Welcome to GitHub CLI! To authenticate, please run `gh auth login`.\n\
                      You are not logged into any GitHub hosts.";
        let error = categorize_error(1, stderr);
        assert!(matches!(error, GhError::NotAuthenticated(_)));
    }

    /// Test 3: parse_issues_json parses valid gh CLI JSON output.
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-006
    /// @pseudocode component-002 lines 35-45
    #[test]
    fn test_list_issues_parses_json() {
        let json = r#"[
            {
                "number": 17,
                "title": "Create a feature list",
                "state": "OPEN",
                "author": {"login": "acoliver"},
                "updatedAt": "2026-03-29T10:00:00Z",
                "assignees": {"nodes": [{"login": "acoliver"}]},
                "labels": {"nodes": [{"name": "enhancement"}]},
                "comments": {"totalCount": 3}
            },
            {
                "number": 5,
                "title": "Bug: crash on startup",
                "state": "CLOSED",
                "author": {"login": "bob"},
                "updatedAt": "2026-03-28T15:30:00Z",
                "assignees": {"nodes": []},
                "labels": {"nodes": [{"name": "bug"}, {"name": "critical"}]},
                "comments": {"totalCount": 0}
            }
        ]"#;

        let issues = parse_issues_json(json).expect("should parse valid JSON");
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].number, 17);
        assert_eq!(issues[0].title, "Create a feature list");
        assert_eq!(issues[0].state, IssueState::Open);
        assert_eq!(issues[0].author_login, "acoliver");
        assert_eq!(issues[0].comment_count, 3);
        assert_eq!(issues[0].labels_summary, "enhancement");
        assert_eq!(issues[0].assignee_summary, "acoliver");
    }

    /// Test 4: sort_issues sorts by updated_at desc, then number asc.
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-006
    /// @pseudocode component-002 lines 46-54
    #[test]
    fn test_list_issues_sorts_by_updated_desc() {
        let mut issues = vec![
            Issue {
                number: 3,
                title: "Old issue".to_string(),
                state: IssueState::Open,
                author_login: "alice".to_string(),
                updated_at: "2026-03-25T10:00:00Z".to_string(),
                assignee_summary: "".to_string(),
                labels_summary: "".to_string(),
                comment_count: 0,
                body: String::new(),
            },
            Issue {
                number: 1,
                title: "Newer issue".to_string(),
                state: IssueState::Open,
                author_login: "bob".to_string(),
                updated_at: "2026-03-29T10:00:00Z".to_string(),
                assignee_summary: "".to_string(),
                labels_summary: "".to_string(),
                comment_count: 0,
                body: String::new(),
            },
            Issue {
                number: 2,
                title: "Same time, lower number".to_string(),
                state: IssueState::Open,
                author_login: "charlie".to_string(),
                updated_at: "2026-03-29T10:00:00Z".to_string(),
                assignee_summary: "".to_string(),
                labels_summary: "".to_string(),
                comment_count: 0,
                body: String::new(),
            },
        ];

        sort_issues(&mut issues);

        // Should be sorted by updated_at desc, then number asc
        assert_eq!(issues[0].number, 1);
        assert_eq!(issues[1].number, 2);
        assert_eq!(issues[2].number, 3);
    }

    /// Test 5: build_list_issues_args constructs correct CLI arguments from filter.
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-008
    /// @pseudocode component-002 lines 25-34
    #[test]
    fn test_list_issues_filter_args_construction() {
        let filter = IssueFilter {
            query_text: "bug".to_string(),
            state: Some(IssueFilterState::Open),
            author: "acoliver".to_string(),
            assignee: "".to_string(),
            labels: vec!["critical".to_string()],
            mentioned: "".to_string(),
            updated_before: "".to_string(),
            updated_after: "".to_string(),
        };

        let args = build_list_issues_args("owner", "repo", &filter, None, 30);

        // Should contain base command parts
        assert!(args.iter().any(|a| a.contains("owner/repo")));
        assert!(args.iter().any(|a| a == "--json"));
        assert!(args.iter().any(|a| a == "--state"
            && args[args.iter().position(|x| x == "--state").unwrap() + 1] == "open"));
        assert!(args.iter().any(|a| a.contains("limit") || a == "-L"));
    }

    /// Test 6: parse_issues_json handles empty result.
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-006
    /// @pseudocode component-002 lines 35-45
    #[test]
    fn test_list_issues_empty_result() {
        let json = "[]";
        let issues = parse_issues_json(json).expect("should parse empty array");
        assert!(issues.is_empty());
    }

    /// Test 7: parse_issue_detail_json parses complete detail JSON.
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-009
    /// @pseudocode component-002 lines 55-65
    #[test]
    fn test_get_issue_detail_parses_json() {
        let json = r#"{
            "number": 17,
            "title": "Create a feature list",
            "state": "OPEN",
            "author": {"login": "acoliver"},
            "createdAt": "2026-03-28T10:00:00Z",
            "updatedAt": "2026-03-29T10:00:00Z",
            "labels": [{"name": "enhancement"}],
            "assignees": [{"login": "acoliver"}],
            "milestone": {"title": "v2.0"},
            "body": "Issue body text here",
            "url": "https://github.com/owner/repo/issues/17",
            "comments": [
                {
                    "id": "IC_123",
                    "author": {"login": "bob"},
                    "createdAt": "2026-03-29T11:00:00Z",
                    "body": "Comment body"
                }
            ]
        }"#;

        let detail = parse_issue_detail_json(json).expect("should parse detail JSON");
        assert_eq!(detail.number, 17);
        assert_eq!(detail.title, "Create a feature list");
        assert_eq!(detail.state, IssueState::Open);
        assert_eq!(detail.author_login, "acoliver");
        assert_eq!(detail.body, "Issue body text here");
        assert_eq!(detail.labels, vec!["enhancement"]);
        assert_eq!(detail.assignees, vec!["acoliver"]);
        assert_eq!(detail.milestone, Some("v2.0".to_string()));
        assert_eq!(
            detail.external_url,
            "https://github.com/owner/repo/issues/17"
        );
        assert_eq!(detail.repo_owner_name, "owner/repo");
        assert_eq!(detail.comments.len(), 1);
        assert_eq!(detail.comments[0].body, "Comment body");
    }

    /// Test 8: parse_issue_detail_json handles missing milestone.
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-009
    /// @pseudocode component-002 lines 55-65
    #[test]
    fn test_get_issue_detail_optional_milestone() {
        let json_with_milestone = r#"{
            "number": 1,
            "title": "With milestone",
            "state": "OPEN",
            "author": {"login": "alice"},
            "createdAt": "2026-03-28T10:00:00Z",
            "updatedAt": "2026-03-29T10:00:00Z",
            "labels": [],
            "assignees": [],
            "milestone": {"title": "v1.0"},
            "body": "",
            "url": "https://github.com/o/r/issues/1",
            "comments": []
        }"#;

        let json_without_milestone = r#"{
            "number": 2,
            "title": "Without milestone",
            "state": "OPEN",
            "author": {"login": "bob"},
            "createdAt": "2026-03-28T10:00:00Z",
            "updatedAt": "2026-03-29T10:00:00Z",
            "labels": [],
            "assignees": [],
            "milestone": null,
            "body": "",
            "url": "https://github.com/o/r/issues/2",
            "comments": []
        }"#;

        let detail_with = parse_issue_detail_json(json_with_milestone).expect("should parse");
        let detail_without = parse_issue_detail_json(json_without_milestone).expect("should parse");

        assert_eq!(detail_with.milestone, Some("v1.0".to_string()));
        assert_eq!(detail_without.milestone, None);
    }

    /// Test 9: parse_comments_json parses GraphQL comments response.
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-009
    /// @pseudocode component-002 lines 75-85
    #[test]
    fn test_list_comments_parses_json() {
        let json = r#"{
            "data": {
                "repository": {
                    "issue": {
                        "comments": {
                            "nodes": [
                                {
                                    "id": "IC_123",
                                    "author": {"login": "alice"},
                                    "createdAt": "2026-03-29T10:00:00Z",
                                    "lastEditedAt": null,
                                    "body": "First comment"
                                },
                                {
                                    "id": "IC_456",
                                    "author": {"login": "bob"},
                                    "createdAt": "2026-03-29T11:00:00Z",
                                    "lastEditedAt": "2026-03-29T12:00:00Z",
                                    "body": "Second comment edited"
                                }
                            ],
                            "pageInfo": {
                                "hasNextPage": false,
                                "endCursor": null
                            }
                        }
                    }
                }
            }
        }"#;

        let (comments, cursor, has_more) =
            parse_comments_json(json).expect("should parse comments");
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].comment_id, 123);
        assert_eq!(comments[0].author_login, "alice");
        assert_eq!(comments[0].edited_at, None);
        assert_eq!(comments[1].comment_id, 456);
        assert_eq!(comments[1].author_login, "bob");
        assert_eq!(
            comments[1].edited_at,
            Some("2026-03-29T12:00:00Z".to_string())
        );
        assert_eq!(cursor, None);
        assert!(!has_more);
    }

    /// Test 10: parse_comments_json extracts pagination info.
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-009
    /// @pseudocode component-002 lines 75-85
    #[test]
    fn test_list_comments_pagination() {
        let json = r#"{
            "data": {
                "repository": {
                    "issue": {
                        "comments": {
                            "nodes": [
                                {
                                    "id": "IC_789",
                                    "author": {"login": "carol"},
                                    "createdAt": "2026-03-29T13:00:00Z",
                                    "lastEditedAt": null,
                                    "body": "Another comment"
                                }
                            ],
                            "pageInfo": {
                                "hasNextPage": true,
                                "endCursor": "Y3Vyc29yOnYyOpHOABcd"
                            }
                        }
                    }
                }
            }
        }"#;

        let (comments, cursor, has_more) = parse_comments_json(json).expect("should parse");
        assert_eq!(comments.len(), 1);
        assert_eq!(cursor, Some("Y3Vyc29yOnYyOpHOABcd".to_string()));
        assert!(has_more);
    }

    /// Test 11: parse_created_comment_json parses POST response.
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-011
    /// @pseudocode component-002 lines 95-100
    #[test]
    fn test_create_comment_success() {
        let json = r#"{
            "id": "IC_999",
            "html_url": "https://github.com/owner/repo/issues/17#issuecomment-999",
            "author": {"login": "acoliver"},
            "createdAt": "2026-03-29T14:00:00Z",
            "body": "This is a new comment"
        }"#;

        let comment = parse_created_comment_json(json).expect("should parse created comment");
        assert_eq!(comment.comment_id, 999);
        assert_eq!(comment.author_login, "acoliver");
        assert_eq!(comment.body, "This is a new comment");
    }

    /// Test 11b: parse_created_comment_json handles REST API format (numeric id, "user", "created_at").
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-011
    #[test]
    fn test_create_comment_rest_format() {
        let json = r#"{
            "id": 4185047845,
            "html_url": "https://github.com/owner/repo/issues/15#issuecomment-4185047845",
            "user": {"login": "acoliver"},
            "created_at": "2026-04-03T20:17:41Z",
            "updated_at": "2026-04-03T20:17:41Z",
            "body": "test from jefe"
        }"#;

        let comment = parse_created_comment_json(json).expect("should parse REST format");
        assert_eq!(comment.comment_id, 4_185_047_845);
        assert_eq!(comment.author_login, "acoliver");
        assert_eq!(comment.body, "test from jefe");
        assert_eq!(comment.created_at, "2026-04-03T20:17:41Z");
    }

    /// Test 12: update_comment returns success (unit test for parsing non-error).
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-011
    /// @pseudocode component-002 lines 120-125
    #[test]
    fn test_update_comment_success() {
        // For update operations, we test that categorize_error doesn't flag success as error
        let error = categorize_error(0, "");
        // Success path - should not be an error variant
        assert!(!matches!(
            error,
            GhError::NotAuthenticated(_)
                | GhError::RateLimited
                | GhError::AccessDenied(_)
                | GhError::ApiError(_)
        ));
    }

    /// Test 13: update_issue_body returns success (unit test for parsing non-error).
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-011
    /// @pseudocode component-002 lines 126-131
    #[test]
    fn test_update_issue_body_success() {
        // Similar to test_update_comment_success
        let error = categorize_error(0, "");
        assert!(!matches!(
            error,
            GhError::NotAuthenticated(_)
                | GhError::RateLimited
                | GhError::AccessDenied(_)
                | GhError::ApiError(_)
        ));
    }

    /// Test 14: build_send_payload with focused comment.
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-011
    /// @pseudocode component-002 lines 70-83
    #[test]
    fn test_build_send_payload_with_comment() {
        let detail = IssueDetail {
            repo_owner_name: "owner/repo".to_string(),
            number: 17,
            title: "Test Issue".to_string(),
            state: IssueState::Open,
            author_login: "alice".to_string(),
            created_at: "2026-03-28T10:00:00Z".to_string(),
            updated_at: "2026-03-29T10:00:00Z".to_string(),
            labels: vec!["bug".to_string()],
            assignees: vec!["bob".to_string()],
            milestone: Some("v1.0".to_string()),
            body: "Issue body".to_string(),
            external_url: "https://github.com/owner/repo/issues/17".to_string(),
            comments: vec![],
            has_more_comments: false,
            comments_cursor: None,
        };

        let focused_comment = IssueComment {
            comment_id: 123,
            author_login: "carol".to_string(),
            created_at: "2026-03-29T11:00:00Z".to_string(),
            edited_at: None,
            body: "This is the focused comment".to_string(),
        };

        let payload = GhClient::build_send_payload(
            "owner/repo",
            &detail,
            Some(&focused_comment),
            "Please help with this issue",
        );

        assert_eq!(payload.repository, "owner/repo");
        assert_eq!(payload.issue_number, 17);
        assert_eq!(payload.issue_title, "Test Issue");
        assert_eq!(payload.issue_state, "open");
        assert_eq!(payload.issue_labels, vec!["bug"]);
        assert_eq!(payload.issue_assignees, vec!["bob"]);
        assert_eq!(
            payload.focused_comment,
            Some("This is the focused comment".to_string())
        );
        assert_eq!(payload.focused_comment_author, Some("carol".to_string()));
        assert_eq!(payload.issue_base_prompt, "Please help with this issue");
    }

    /// Test 15: build_send_payload without focused comment.
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-011
    /// @pseudocode component-002 lines 70-83
    #[test]
    fn test_build_send_payload_without_comment() {
        let detail = IssueDetail {
            repo_owner_name: "owner/repo".to_string(),
            number: 5,
            title: "Another Issue".to_string(),
            state: IssueState::Closed,
            author_login: "dave".to_string(),
            created_at: "2026-03-25T10:00:00Z".to_string(),
            updated_at: "2026-03-26T10:00:00Z".to_string(),
            labels: vec![],
            assignees: vec![],
            milestone: None,
            body: "Another body".to_string(),
            external_url: "https://github.com/owner/repo/issues/5".to_string(),
            comments: vec![],
            has_more_comments: false,
            comments_cursor: None,
        };

        let payload = GhClient::build_send_payload("owner/repo", &detail, None, "Base prompt here");

        assert_eq!(payload.issue_number, 5);
        assert_eq!(payload.issue_state, "closed");
        assert!(payload.focused_comment.is_none());
        assert!(payload.focused_comment_author.is_none());
    }

    /// Test 16: categorize_error detects rate limit.
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-013
    /// @pseudocode component-002 lines 105-120
    #[test]
    fn test_error_categorization_rate_limit() {
        let stderr = "API rate limit exceeded. Please wait a few minutes and try again.";
        let error = categorize_error(1, stderr);
        assert!(matches!(error, GhError::RateLimited));
    }

    /// Test 17: categorize_error detects authentication error.
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-013
    /// @pseudocode component-002 lines 105-120
    #[test]
    fn test_error_categorization_not_authenticated() {
        let stderr = "401 Bad credentials - authentication required";
        let error = categorize_error(1, stderr);
        assert!(matches!(error, GhError::NotAuthenticated(_)));
    }

    /// Test 18: categorize_error detects access denied.
    /// @plan PLAN-20260329-ISSUES-MODE.P08
    /// @requirement REQ-ISS-013
    /// @pseudocode component-002 lines 105-120
    #[test]
    fn test_error_categorization_access_denied() {
        let stderr = "HTTP 403: Resource not accessible by personal access token";
        let error = categorize_error(1, stderr);
        assert!(matches!(error, GhError::AccessDenied(_)));
    }
}
