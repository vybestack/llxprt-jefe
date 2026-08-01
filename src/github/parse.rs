//! Parsing and CLI-argument-building helpers for the GitHub client boundary.
//!
//! Extracted from `github/mod.rs` to keep individual source files within the
//! project's length policy. These are pure functions over `serde_json::Value`
//! and `crate::domain` types; they perform no I/O.
//!
//! @plan PLAN-20260329-ISSUES-MODE.P08
//! @requirement REQ-ISS-013

use crate::domain::{Issue, IssueComment, IssueDetail, IssueState, IssueStateReason};
use serde_json::Value;
use std::collections::HashSet;

use super::comment_pages::exhausted_comments;
use super::issue_sort::{issue_priority_from_item, sort_issues};
use super::{GhError, IssueListResponse};

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

    if not_authenticated_matcher(&stderr_lower) {
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

/// The single source of truth for recognizing a `gh` authentication failure
/// from a lowercased error/stderr string. Shared by [`categorize_error`]'s
/// `NotAuthenticated` arm and [`crate::github::is_not_authenticated_error`]
/// so the dispatch-layer auth-remediation trigger cannot drift from the
/// error categorizer (issue #244).
///
/// @must_use
#[must_use]
pub(super) fn not_authenticated_matcher(stderr_lower: &str) -> bool {
    stderr_lower.contains("401")
        || stderr_lower.contains("not logged in")
        || stderr_lower.contains("authentication")
        || stderr_lower.contains("not authenticated")
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

/// Parse JSON output from the GraphQL issue search query into a paginated response.
pub fn parse_issue_search_json(json_str: &str) -> Result<IssueListResponse, GhError> {
    let value: Value = serde_json::from_str(json_str)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;
    let data = value
        .get("data")
        .ok_or_else(|| GhError::ParseError("Missing issue search data".to_string()))?;
    let search = data
        .get("search")
        .or_else(|| data.get("repository").and_then(|repo| repo.get("issues")))
        .ok_or_else(|| {
            GhError::ParseError("Missing issue search or repository issues data".to_string())
        })?;
    let nodes = search
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| GhError::ParseError("Missing issue search nodes".to_string()))?;
    let page_info = search
        .get("pageInfo")
        .ok_or_else(|| GhError::ParseError("Missing pageInfo".to_string()))?;

    let mut issues = nodes
        .iter()
        .map(parse_issue_from_item)
        .collect::<Result<Vec<Issue>, GhError>>()?;
    sort_issues(&mut issues);
    let (cursor, has_more) = parse_page_info(page_info);

    Ok(IssueListResponse {
        issues,
        cursor,
        has_more,
    })
}

/// Build a single [`Issue`] from one JSON array element of `gh issue list`.
fn parse_issue_from_item(item: &Value) -> Result<Issue, GhError> {
    let number = item
        .get("number")
        .and_then(Value::as_u64)
        .ok_or_else(|| GhError::ParseError("Missing or invalid number".to_string()))?;

    let node_id = json_field_str(item, "id");
    let title = json_field_str(item, "title");
    let state = parse_issue_state(item);
    let state_reason = parse_issue_state_reason(item);
    let author_login = item
        .get("author")
        .and_then(|a| a.get("login"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let created_at = json_field_str(item, "createdAt");
    let updated_at = json_field_str(item, "updatedAt");

    let assignees = collect_nodes_field(item, "assignees");
    let labels = collect_nodes_field(item, "labels");
    let issue_type = parse_optional_string_field(item, "issueType", "name").unwrap_or_default();
    let milestone = parse_optional_string_field(item, "milestone", "title").unwrap_or_default();
    let module = module_from_labels(&labels);
    let assignee_summary = assignees.join(", ");
    let labels_summary = labels.join(", ");

    let comment_count = item
        .get("comments")
        .and_then(|c| c.get("totalCount"))
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let body = json_field_str(item, "body");
    let priority = issue_priority_from_item(item);
    let linked_pr_numbers = parse_linked_pr_numbers(item);

    Ok(Issue {
        number,
        node_id,
        title,
        state,
        author_login,
        created_at,
        updated_at,
        assignee_summary,
        labels_summary,
        assignees,
        labels,
        issue_type,
        milestone,
        module,
        comment_count,
        body,
        priority,
        state_reason,
        linked_pr_numbers,
    })
}

/// Extract linked pull-request numbers from an issue node's `timelineItems`
/// connection (issue #187).
///
/// GitHub surfaces linked PRs as `CROSS_REFERENCED_EVENT` timeline items whose
/// `source` is a `PullRequest`. This walks `timelineItems.nodes`, keeps only
/// events whose `source.__typename == "PullRequest"`, reads `source.number`,
/// and de-duplicates while preserving first-seen order. A TOTAL function:
/// missing/empty `timelineItems`, non-PR sources, and null `source` all yield
/// an empty vec without panicking.
fn parse_linked_pr_numbers(item: &Value) -> Vec<u64> {
    let Some(nodes) = item
        .get("timelineItems")
        .and_then(|t| t.get("nodes"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut seen = Vec::new();
    let mut seen_set = HashSet::new();
    for node in nodes {
        let is_cross_ref = node
            .get("__typename")
            .and_then(Value::as_str)
            .is_some_and(|t| t == "CrossReferencedEvent");
        if !is_cross_ref {
            continue;
        }
        let Some(source) = node.get("source") else {
            continue;
        };
        let is_pr = source
            .get("__typename")
            .and_then(Value::as_str)
            .is_some_and(|t| t == "PullRequest");
        if !is_pr {
            continue;
        }
        if let Some(number) = source.get("number").and_then(Value::as_u64)
            && seen_set.insert(number)
        {
            seen.push(number);
        }
    }
    seen
}

/// Read a top-level string field as `String`, defaulting to "".
fn json_field_str(item: &Value, field: &str) -> String {
    item.get(field)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn module_from_labels(labels: &[String]) -> String {
    labels
        .iter()
        .filter_map(|label| module_label_value(label))
        .find(|module| !module.is_empty())
        .map(str::to_string)
        .unwrap_or_default()
}

pub(super) fn module_label_value(label: &str) -> Option<&str> {
    split_case_insensitive_prefix(label.trim(), "module:").map(str::trim)
}

fn split_case_insensitive_prefix<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    let prefix_len = prefix.len();
    let candidate = value.get(..prefix_len)?;
    candidate
        .eq_ignore_ascii_case(prefix)
        .then_some(&value[prefix_len..])
}

/// Read `field.nodes[*].<key>` (defaulting to "login"/"name").
///
/// Supports two JSON shapes returned by the `gh` CLI:
/// - GraphQL style: `{"nodes": [{"login": ...}, ...]}`.
/// - REST/direct array style: `[{"login": ...}, ...]` (a bare array of objects).
fn collect_nodes_field(item: &Value, field: &str) -> Vec<String> {
    // `gh issue list` exposes label names under `name`; user-like nodes use `login`.
    let key = if field == "labels" { "name" } else { "login" };

    let nodes = item.get(field).and_then(|f| {
        // GraphQL shape: {"nodes": [...]}.
        if let Some(arr) = f.get("nodes").and_then(Value::as_array) {
            return Some(arr);
        }
        // REST/direct array shape: [...] itself.
        f.as_array()
    });

    nodes
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|n| n.get(key).and_then(Value::as_str))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
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

    let node_id = value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let title = json_field_str(&value, "title");
    let state = parse_issue_state(&value);
    let state_reason = parse_issue_state_reason(&value);
    let author_login = json_login_field(&value, "author");
    let created_at = json_field_str(&value, "createdAt");
    let updated_at = json_field_str(&value, "updatedAt");
    let labels = json_string_array(&value, "labels", "name");
    let assignees = json_string_array(&value, "assignees", "login");
    let milestone = parse_optional_string_field(&value, "milestone", "title");
    let body = json_field_str(&value, "body");
    let external_url = json_field_str(&value, "url");

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
                .map(parse_rest_comment)
                .collect::<Result<Vec<IssueComment>, GhError>>()
        })
        .transpose()?
        .unwrap_or_default();
    let comments = exhausted_comments(comments);

    Ok(IssueDetail {
        repo_owner_name,
        number,
        node_id,
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
        issue_type_name: None,
        state_reason,
    })
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

/// Parse the GraphQL `stateReason` / REST `state_reason` field into an
/// [`IssueStateReason`], returning `None` when missing, null, `REOPENED`, or
/// unknown (issue #204). Handles both the GraphQL spelling (`COMPLETED`) and
/// the REST spelling (`completed`) returned by `gh issue view --json`.
fn parse_issue_state_reason(value: &Value) -> Option<IssueStateReason> {
    value
        .get("stateReason")
        .or_else(|| value.get("state_reason"))
        .and_then(Value::as_str)
        .and_then(IssueStateReason::parse)
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

fn parse_comment_id(value: &Value) -> Result<u64, GhError> {
    if let Some(id) = value.get("databaseId").and_then(Value::as_u64) {
        return Ok(id);
    }
    if let Some(id) = value.get("id").and_then(Value::as_u64) {
        return Ok(id);
    }

    let id_str = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| GhError::ParseError("Missing comment id".to_string()))?;
    id_str
        .strip_prefix("IC_")
        .and_then(|rest| rest.parse::<u64>().ok())
        .or_else(|| id_str.parse::<u64>().ok())
        .or_else(|| parse_issuecomment_fragment(value))
        .ok_or_else(|| GhError::ParseError(format!("Invalid comment id: {id_str}")))
}

fn parse_issuecomment_fragment(value: &Value) -> Option<u64> {
    value
        .get("url")
        .or_else(|| value.get("html_url"))
        .and_then(Value::as_str)
        .and_then(|url| url.rsplit_once("#issuecomment-"))
        .and_then(|(_, id)| id.parse::<u64>().ok())
}

/// Helper to parse a REST API format comment
fn parse_rest_comment(value: &Value) -> Result<IssueComment, GhError> {
    let comment_id = parse_comment_id(value)?;

    let author_login = value
        .get("author")
        .or_else(|| value.get("user"))
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

    // Navigate to data.repository.<issue|pullRequest>.comments. PR comments
    // are served under `repository.pullRequest(number:).comments` (the issue
    // object is NULL for a PR number — P00A §2d), so both object paths are
    // accepted here to keep the node/pageInfo parser reusable.
    let comments_data = value
        .get("data")
        .and_then(|d| d.get("repository"))
        .and_then(|r| {
            r.get("issue")
                .and_then(|i| i.get("comments"))
                .or_else(|| r.get("pullRequest").and_then(|p| p.get("comments")))
        })
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
///
/// `pub(super)` so `parse_pr` can reuse it verbatim (the PR search and
/// `gh pr view` paths read the SAME `pageInfo { hasNextPage endCursor }`
/// shape). Kept in `parse.rs` to avoid duplicating page-info logic.
pub(super) fn parse_page_info(page_info: &Value) -> (Option<String>, bool) {
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

    let comment_id = parse_comment_id(&value)?;

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

#[cfg(test)]
mod linked_pr_parse_tests {
    use super::parse_linked_pr_numbers;
    use serde_json::json;

    #[test]
    fn extracts_single_linked_pr_from_cross_referenced_event() {
        let item = json!({
            "timelineItems": {
                "nodes": [
                    {
                        "__typename": "CrossReferencedEvent",
                        "source": {"__typename": "PullRequest", "number": 123}
                    }
                ]
            }
        });
        assert_eq!(parse_linked_pr_numbers(&item), vec![123]);
    }

    #[test]
    fn excludes_non_pull_request_cross_references() {
        let item = json!({
            "timelineItems": {
                "nodes": [
                    {
                        "__typename": "CrossReferencedEvent",
                        "source": {"__typename": "PullRequest", "number": 7}
                    },
                    {
                        "__typename": "CrossReferencedEvent",
                        "source": {"__typename": "Issue", "number": 99}
                    }
                ]
            }
        });
        assert_eq!(parse_linked_pr_numbers(&item), vec![7]);
    }

    #[test]
    fn deduplicates_repeated_pr_numbers_preserving_first_seen_order() {
        let item = json!({
            "timelineItems": {
                "nodes": [
                    {
                        "__typename": "CrossReferencedEvent",
                        "source": {"__typename": "PullRequest", "number": 5}
                    },
                    {
                        "__typename": "CrossReferencedEvent",
                        "source": {"__typename": "PullRequest", "number": 3}
                    },
                    {
                        "__typename": "CrossReferencedEvent",
                        "source": {"__typename": "PullRequest", "number": 5}
                    }
                ]
            }
        });
        assert_eq!(parse_linked_pr_numbers(&item), vec![5, 3]);
    }

    #[test]
    fn empty_when_timeline_items_absent() {
        let item = json!({"number": 1});
        assert!(parse_linked_pr_numbers(&item).is_empty());
    }

    #[test]
    fn empty_when_nodes_empty() {
        let item = json!({"timelineItems": {"nodes": []}});
        assert!(parse_linked_pr_numbers(&item).is_empty());
    }

    #[test]
    fn empty_when_source_is_null() {
        let item = json!({
            "timelineItems": {
                "nodes": [
                    {"__typename": "CrossReferencedEvent", "source": null}
                ]
            }
        });
        assert!(parse_linked_pr_numbers(&item).is_empty());
    }
}
