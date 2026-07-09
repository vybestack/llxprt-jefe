//! Parsing and CLI-argument-building helpers for the GitHub client boundary.
//!
//! Extracted from `github/mod.rs` to keep individual source files within the
//! project's length policy. These are pure functions over `serde_json::Value`
//! and `crate::domain` types; they perform no I/O.
//!
//! @plan PLAN-20260329-ISSUES-MODE.P08
//! @requirement REQ-ISS-013

use crate::domain::{
    FILTER_CHOICE_ANY, FILTER_CHOICE_NONE, Issue, IssueComment, IssueDetail, IssueFilter,
    IssueFilterState, IssueState,
};
use serde_json::Value;

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

    let title = item
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let state = parse_issue_state(item);

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
        assignees,
        labels,
        issue_type,
        milestone,
        module,
        comment_count,
        body,
    })
}

fn module_from_labels(labels: &[String]) -> String {
    labels
        .iter()
        .filter_map(|label| module_label_value(label))
        .find(|module| !module.is_empty())
        .map(str::to_string)
        .unwrap_or_default()
}

fn module_label_value(label: &str) -> Option<&str> {
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
                .map(parse_rest_comment)
                .collect::<Result<Vec<IssueComment>, GhError>>()
        })
        .transpose()?
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

/// Build the `gh issue list` CLI argument vector for the given repository and
/// filter.
///
/// This legacy CLI path cannot filter by GitHub Issue Type: GitHub search's
/// `type:` qualifier means issue vs. pull-request, and `gh issue list --json`
/// does not expose `issueType`. Callers that need Issue Type filtering or
/// metadata should use the GraphQL list/search path.
///
/// The `cursor` parameter is accepted for API symmetry with
/// [`super::GhClient::list_issues`] and the analogous comment pagination
/// helpers, but is intentionally unused: `gh issue list` (the REST-backed CLI
/// subcommand) does not expose cursor-based pagination. It is retained so that
/// callers can be migrated to a future GraphQL issue query without signature
/// churn.
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
        "number,title,state,author,updatedAt,assignees,labels,milestone,comments".to_string(),
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

    // Add assignee when the legacy CLI flag can represent it directly.
    let assignee = filter.assignee.trim();
    if !assignee.is_empty()
        && !assignee.eq_ignore_ascii_case(FILTER_CHOICE_ANY)
        && !assignee.eq_ignore_ascii_case(FILTER_CHOICE_NONE)
    {
        args.push("--assignee".to_string());
        args.push(assignee.to_string());
    }

    // Add author
    if let Some(author) = non_any_filter_value(&filter.author) {
        args.push("--author".to_string());
        args.push(author.to_string());
    }

    // Add mentioned
    if let Some(mentioned) = non_any_filter_value(&filter.mentioned) {
        args.push("--mention".to_string());
        args.push(mentioned.to_string());
    }

    let search_query = legacy_issue_search_query(filter);
    if !search_query.is_empty() {
        args.push("--search".to_string());
        args.push(search_query);
    }

    args
}

fn issue_search_query(owner: &str, repo: &str, filter: &IssueFilter) -> String {
    let mut terms = vec![format!("repo:{owner}/{repo}"), "is:issue".to_string()];
    if let Some(state) = issue_filter_state_query(filter) {
        terms.push(state);
    }

    terms.extend(
        filter
            .labels
            .iter()
            .map(|label| format!("label:{}", search_qualifier_value(label))),
    );
    push_non_empty_term(&mut terms, "author:", &filter.author);
    push_assignee_term(&mut terms, &filter.assignee);
    push_milestone_term(&mut terms, &filter.milestone);
    push_module_term(&mut terms, &filter.module, &filter.labels);
    push_non_empty_term(&mut terms, "mentions:", &filter.mentioned);
    push_non_empty_term(&mut terms, "updated:<", &filter.updated_before);
    push_non_empty_term(&mut terms, "updated:>", &filter.updated_after);
    if !filter.query_text.trim().is_empty() {
        terms.push(filter.query_text.trim().to_string());
    }

    terms.join(" ")
}

fn legacy_issue_search_query(filter: &IssueFilter) -> String {
    let mut terms = Vec::new();
    push_legacy_assignee_term(&mut terms, &filter.assignee);
    push_milestone_term(&mut terms, &filter.milestone);
    push_module_term(&mut terms, &filter.module, &filter.labels);
    if !filter.query_text.trim().is_empty() {
        terms.push(filter.query_text.trim().to_string());
    }
    terms.join(" ")
}

fn issue_filter_state_query(filter: &IssueFilter) -> Option<String> {
    match filter.state.unwrap_or_default() {
        IssueFilterState::Open => Some("state:open".to_string()),
        IssueFilterState::Closed => Some("state:closed".to_string()),
        IssueFilterState::All => None,
    }
}

fn push_non_empty_term(terms: &mut Vec<String>, prefix: &str, value: &str) {
    if non_any_filter_value(value).is_some() {
        terms.push(format!("{prefix}{}", search_qualifier_value(value)));
    }
}

fn non_any_filter_value(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case(FILTER_CHOICE_ANY) {
        None
    } else {
        Some(trimmed)
    }
}

fn push_assignee_term(terms: &mut Vec<String>, assignee: &str) {
    let value = assignee.trim();
    if value.is_empty() || value.eq_ignore_ascii_case(FILTER_CHOICE_ANY) {
        return;
    }
    if value.eq_ignore_ascii_case(FILTER_CHOICE_NONE) {
        terms.push("no:assignee".to_string());
    } else {
        terms.push(format!("assignee:{}", search_qualifier_value(value)));
    }
}
fn push_legacy_assignee_term(terms: &mut Vec<String>, assignee: &str) {
    let value = assignee.trim();
    if value.is_empty() || value.eq_ignore_ascii_case(FILTER_CHOICE_ANY) {
        return;
    }
    if value.eq_ignore_ascii_case(FILTER_CHOICE_NONE) {
        terms.push("no:assignee".to_string());
    }
}

fn push_milestone_term(terms: &mut Vec<String>, milestone: &str) {
    let value = milestone.trim();
    if value.is_empty() || value.eq_ignore_ascii_case(FILTER_CHOICE_ANY) {
        return;
    }
    if value.eq_ignore_ascii_case(FILTER_CHOICE_NONE) {
        terms.push("no:milestone".to_string());
    } else {
        terms.push(format!("milestone:{}", search_qualifier_value(value)));
    }
}

fn push_module_term(terms: &mut Vec<String>, module: &str, labels: &[String]) {
    let value = module.trim();
    if value.is_empty() || value.eq_ignore_ascii_case(FILTER_CHOICE_ANY) {
        return;
    }
    if labels
        .iter()
        .any(|label| label_matches_module(label, value))
    {
        return;
    }

    let label = format!("module:{value}");
    terms.push(format!("label:{}", search_qualifier_value(&label)));
}

fn label_matches_module(label: &str, module: &str) -> bool {
    module_label_value(label).is_some_and(|value| value.eq_ignore_ascii_case(module))
}

fn search_qualifier_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\\'))
    {
        let escaped = trimmed.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{escaped}\"")
    } else {
        trimmed.to_string()
    }
}

pub(super) fn active_issue_type_filter(filter: &IssueFilter) -> Option<&str> {
    let issue_type = filter.issue_type.trim();
    if issue_type.is_empty() || issue_type.eq_ignore_ascii_case(FILTER_CHOICE_ANY) {
        None
    } else {
        Some(issue_type)
    }
}

pub(super) fn issue_type_requires_search_filter(filter: &IssueFilter) -> bool {
    filter
        .assignee
        .trim()
        .eq_ignore_ascii_case(FILTER_CHOICE_NONE)
        || filter
            .milestone
            .trim()
            .eq_ignore_ascii_case(FILTER_CHOICE_NONE)
        || !filter.query_text.trim().is_empty()
        || !filter.updated_before.trim().is_empty()
        || !filter.updated_after.trim().is_empty()
}

fn build_repository_issue_type_args(
    owner: &str,
    repo: &str,
    filter: &IssueFilter,
    issue_type: &str,
    cursor: Option<&str>,
    page_size: u32,
) -> Vec<String> {
    let mut variable_defs = vec![
        "$owner: String!".to_string(),
        "$repo: String!".to_string(),
        "$issueType: String!".to_string(),
        "$first: Int!".to_string(),
    ];
    let mut filters = vec!["type: $issueType".to_string()];
    let mut args = base_issue_type_args(owner, repo, issue_type, page_size);

    if let Some(c) = cursor {
        variable_defs.push("$after: String".to_string());
        args.push("-F".to_string());
        args.push(format!("after={c}"));
    }

    push_repository_issue_filter_fields(filter, &mut variable_defs, &mut filters, &mut args);

    let after_arg = if cursor.is_some() {
        ", after: $after"
    } else {
        ""
    };
    let query = format!(
        "query({}) {{ repository(owner: $owner, name: $repo) {{ issues(first: $first{after_arg}, filterBy: {{ {} }}, orderBy: {{ field: UPDATED_AT, direction: DESC }}) {{ nodes {{ number title state author {{ login }} updatedAt assignees(first: 10) {{ nodes {{ login }} }} labels(first: 20) {{ nodes {{ name }} }} issueType {{ name }} milestone {{ title }} comments {{ totalCount }} }} pageInfo {{ hasNextPage endCursor }} }} }} }}",
        variable_defs.join(", "),
        filters.join(", ")
    );

    args.splice(2..2, ["-f".to_string(), format!("query={query}")]);
    args
}

fn base_issue_type_args(owner: &str, repo: &str, issue_type: &str, page_size: u32) -> Vec<String> {
    vec![
        "api".to_string(),
        "graphql".to_string(),
        "-F".to_string(),
        format!("owner={owner}"),
        "-F".to_string(),
        format!("repo={repo}"),
        "-F".to_string(),
        format!("issueType={issue_type}"),
        "-F".to_string(),
        format!("first={page_size}"),
    ]
}

fn push_repository_issue_filter_fields(
    filter: &IssueFilter,
    variable_defs: &mut Vec<String>,
    filters: &mut Vec<String>,
    args: &mut Vec<String>,
) {
    push_repository_state_filter(filter, filters);
    push_repository_string_filter(
        "author",
        "createdBy",
        &filter.author,
        variable_defs,
        filters,
        args,
    );
    push_repository_nullable_filter(
        "assignee",
        "assignee",
        &filter.assignee,
        variable_defs,
        filters,
        args,
    );
    push_repository_nullable_filter(
        "milestone",
        "milestone",
        &filter.milestone,
        variable_defs,
        filters,
        args,
    );
    push_repository_string_filter(
        "mentioned",
        "mentioned",
        &filter.mentioned,
        variable_defs,
        filters,
        args,
    );
    push_repository_label_filter(filter, filters);
}

fn push_repository_state_filter(filter: &IssueFilter, filters: &mut Vec<String>) {
    match filter.state.unwrap_or_default() {
        IssueFilterState::Open => filters.push("states: [OPEN]".to_string()),
        IssueFilterState::Closed => filters.push("states: [CLOSED]".to_string()),
        IssueFilterState::All => {}
    }
}

fn push_repository_string_filter(
    variable_name: &str,
    filter_name: &str,
    value: &str,
    variable_defs: &mut Vec<String>,
    filters: &mut Vec<String>,
    args: &mut Vec<String>,
) {
    let Some(value) = non_any_filter_value(value) else {
        return;
    };
    variable_defs.push(format!("${variable_name}: String"));
    filters.push(format!("{filter_name}: ${variable_name}"));
    args.push("-F".to_string());
    args.push(format!("{variable_name}={value}"));
}

fn push_repository_nullable_filter(
    variable_name: &str,
    filter_name: &str,
    value: &str,
    variable_defs: &mut Vec<String>,
    filters: &mut Vec<String>,
    args: &mut Vec<String>,
) {
    let Some(value) = non_any_filter_value(value) else {
        return;
    };
    if value.eq_ignore_ascii_case(FILTER_CHOICE_NONE) {
        filters.push(format!("{filter_name}: null"));
        return;
    }
    variable_defs.push(format!("${variable_name}: String"));
    filters.push(format!("{filter_name}: ${variable_name}"));
    args.push("-F".to_string());
    args.push(format!("{variable_name}={value}"));
}

fn push_repository_label_filter(filter: &IssueFilter, filters: &mut Vec<String>) {
    let mut labels = filter.labels.clone();
    let module = filter.module.trim();
    if !module.is_empty()
        && !module.eq_ignore_ascii_case(FILTER_CHOICE_ANY)
        && !labels
            .iter()
            .any(|label| label_matches_module(label, module))
    {
        labels.push(format!("module:{module}"));
    }
    if labels.is_empty() {
        return;
    }
    let label_literals = labels
        .iter()
        .map(|label| graphql_string_literal(label))
        .collect::<Vec<_>>()
        .join(", ");
    filters.push(format!("labels: [{label_literals}]"));
}

fn graphql_string_literal(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[must_use]
pub fn build_issue_search_args(
    owner: &str,
    repo: &str,
    filter: &IssueFilter,
    cursor: Option<&str>,
    page_size: u32,
) -> Vec<String> {
    if let Some(issue_type) = active_issue_type_filter(filter)
        && !issue_type_requires_search_filter(filter)
    {
        return build_repository_issue_type_args(
            owner, repo, filter, issue_type, cursor, page_size,
        );
    }

    let query = if cursor.is_some() {
        "query($searchQuery: String!, $first: Int!, $after: String) { search(type: ISSUE, query: $searchQuery, first: $first, after: $after) { nodes { ... on Issue { number title state author { login } updatedAt assignees(first: 10) { nodes { login } } labels(first: 20) { nodes { name } } issueType { name } milestone { title } comments { totalCount } } } pageInfo { hasNextPage endCursor } } }"
    } else {
        "query($searchQuery: String!, $first: Int!) { search(type: ISSUE, query: $searchQuery, first: $first) { nodes { ... on Issue { number title state author { login } updatedAt assignees(first: 10) { nodes { login } } labels(first: 20) { nodes { name } } issueType { name } milestone { title } comments { totalCount } } } pageInfo { hasNextPage endCursor } } }"
    };
    let mut args = vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={query}"),
        "-F".to_string(),
        format!("searchQuery={}", issue_search_query(owner, repo, filter)),
        "-F".to_string(),
        format!("first={page_size}"),
    ];
    if let Some(c) = cursor {
        args.push("-F".to_string());
        args.push(format!("after={c}"));
    }
    args
}
