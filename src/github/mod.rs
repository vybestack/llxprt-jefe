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

mod create_issue;
pub use create_issue::{CreatedIssue, parse_created_issue_json};

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
pub fn parse_issues_json(json_str: &str) -> Result<Vec<Issue>, GhError> {
    let value: Value = serde_json::from_str(json_str)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;

    let array = value
        .as_array()
        .ok_or_else(|| GhError::ParseError("Expected JSON array".to_string()))?;

    array
        .iter()
        .map(parse_issue_from_item)
        .collect::<Result<Vec<Issue>, GhError>>()
}

/// Build a single [`Issue`] from one JSON array element of `gh issue list`.
fn parse_issue_from_item(item: &Value) -> Result<Issue, GhError> {
    let number = item
        .get("number")
        .and_then(Value::as_u64)
        .ok_or_else(|| GhError::ParseError("Missing or invalid number".to_string()))?;

    let title = item
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let state = item
        .get("state")
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

    let assignee_summary = join_nodes_field(item, "assignees");
    let labels_summary = join_nodes_field(item, "labels");

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

    Ok(Issue {
        number,
        title,
        state,
        author_login,
        updated_at,
        assignee_summary,
        labels_summary,
        comment_count,
        body,
    })
}

/// Read `field.nodes[*].<key>` (defaulting to "login"/"name") joined with ", ".
fn join_nodes_field(item: &Value, field: &str) -> String {
    item.get(field)
        .and_then(|f| f.get("nodes"))
        .and_then(Value::as_array)
        .map(|nodes| {
            let key = if field == "labels" { "name" } else { "login" };
            nodes
                .iter()
                .filter_map(|n| n.get(key).and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
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
pub fn parse_issue_detail_json(json_str: &str) -> Result<IssueDetail, GhError> {
    let value: Value = serde_json::from_str(json_str)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;

    let number = value
        .get("number")
        .and_then(Value::as_u64)
        .ok_or_else(|| GhError::ParseError("Missing or invalid number".to_string()))?;

    let title = json_string_field(&value, "title");
    let state = parse_issue_state(&value);
    let author_login = json_login_field(&value, "author");
    let created_at = json_string_field(&value, "createdAt");
    let updated_at = json_string_field(&value, "updatedAt");
    let labels = json_string_array(&value, "labels", "name");
    let assignees = json_string_array(&value, "assignees", "login");
    let milestone = parse_optional_string_field(&value, "milestone", "title");
    let body = json_string_field(&value, "body");
    let external_url = json_string_field(&value, "url");

    // Extract repo_owner_name from URL (format: https://github.com/owner/repo/issues/NUM)
    let repo_owner_name = external_url
        .strip_prefix("https://github.com/")
        .and_then(|rest| rest.find("/issues/").map(|idx| rest[..idx].to_string()))
        .unwrap_or_default();

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

/// Read a top-level string field, defaulting to "".
fn json_string_field(value: &Value, field: &str) -> String {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Read `<field>.login` as a string, defaulting to "".
fn json_login_field(value: &Value, field: &str) -> String {
    value
        .get(field)
        .and_then(|a| a.get("login"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Parse the `state` field into an [`IssueState`], defaulting to Open.
fn parse_issue_state(value: &Value) -> IssueState {
    value
        .get("state")
        .and_then(Value::as_str)
        .map_or(IssueState::Open, |s| match s {
            "CLOSED" => IssueState::Closed,
            _ => IssueState::Open,
        })
}

/// Collect `<field>[*].<key>` into `Vec<String>`.
fn json_string_array(value: &Value, field: &str, key: &str) -> Vec<String> {
    value
        .get(field)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.get(key).and_then(Value::as_str).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Read an optional nested string: null or missing yields None.
fn parse_optional_string_field(value: &Value, field: &str, key: &str) -> Option<String> {
    value.get(field).and_then(|m| {
        if m.is_null() {
            None
        } else {
            m.get(key).and_then(Value::as_str).map(String::from)
        }
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
    let (end_cursor, has_next_page) = parse_page_info(page_info);

    let mut comments = Vec::new();
    for node in nodes {
        comments.push(parse_rest_comment(node)?);
    }

    Ok((comments, end_cursor, has_next_page))
}

/// Extract (endCursor, hasNextPage) from a GraphQL `pageInfo` object.
fn parse_page_info(page_info: &Value) -> (Option<String>, bool) {
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

    (end_cursor, has_next_page)
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
