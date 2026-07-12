//! Property-editing `gh`/GraphQL argument builders and parsers (issue #175).
//!
//! All functions here are pure so the exact `gh` command shape can be unit
//! tested without a process boundary; the [`super::GhClient`] methods wrap
//! them with the actual `gh` subprocess invocation via `run_gh`.
//!
//! Mirrors the `build_*_args`/`parse_*` separation in `viewer.rs` and `parse.rs`.

use super::GhError;
use serde_json::Value;

/// Identifies the issue or PR being edited. Bundles the four identifying
/// fields so property-edit methods and arg builders stay under the argument
/// limit (issue #175).
#[derive(Debug, Clone, Copy)]
pub struct PropertyEditTarget<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    pub number: u64,
    pub is_pr: bool,
}

/// Compute the set of labels to add and remove (the "diff") given the current
/// set and the desired set. Order-independent and deterministic.
///
/// Returns `(to_add, to_remove)`.
#[must_use]
pub fn compute_label_diff(current: &[String], desired: &[String]) -> (Vec<String>, Vec<String>) {
    let to_add: Vec<String> = desired
        .iter()
        .filter(|d| !current.iter().any(|c| c.eq_ignore_ascii_case(d)))
        .cloned()
        .collect();
    let to_remove: Vec<String> = current
        .iter()
        .filter(|c| !desired.iter().any(|d| d.eq_ignore_ascii_case(c)))
        .cloned()
        .collect();
    (to_add, to_remove)
}

/// Compute the set of assignees to add and remove given current and desired.
///
/// Returns `(to_add, to_remove)`.
#[must_use]
pub fn compute_assignee_diff(current: &[String], desired: &[String]) -> (Vec<String>, Vec<String>) {
    let to_add: Vec<String> = desired
        .iter()
        .filter(|d| !current.iter().any(|c| c.eq_ignore_ascii_case(d)))
        .cloned()
        .collect();
    let to_remove: Vec<String> = current
        .iter()
        .filter(|c| !desired.iter().any(|d| d.eq_ignore_ascii_case(c)))
        .cloned()
        .collect();
    (to_add, to_remove)
}

/// Build `gh issue edit` / `gh pr edit` args for label changes.
///
/// Each label gets its own `--add-label` / `--remove-label` flag so label
/// names containing commas are not split (M9 fix). Returns empty `Vec` if
/// no-op.
#[must_use]
pub fn build_edit_labels_args(
    target: PropertyEditTarget,
    to_add: &[String],
    to_remove: &[String],
) -> Vec<String> {
    if to_add.is_empty() && to_remove.is_empty() {
        return Vec::new();
    }
    let entity = if target.is_pr { "pr" } else { "issue" };
    let mut args = vec![
        entity.to_string(),
        "edit".to_string(),
        "--repo".to_string(),
        format!("{}/{}", target.owner, target.repo),
        target.number.to_string(),
    ];
    for label in to_add {
        args.push("--add-label".to_string());
        args.push(label.clone());
    }
    for label in to_remove {
        args.push("--remove-label".to_string());
        args.push(label.clone());
    }
    args
}

/// Build `gh issue edit` / `gh pr edit` args for assignee changes.
///
/// Each assignee gets its own `--add-assignee` / `--remove-assignee` flag (gh
/// does not support comma-separated assignees). Returns empty `Vec` if no-op.
#[must_use]
pub fn build_edit_assignees_args(
    target: PropertyEditTarget,
    to_add: &[String],
    to_remove: &[String],
) -> Vec<String> {
    if to_add.is_empty() && to_remove.is_empty() {
        return Vec::new();
    }
    let entity = if target.is_pr { "pr" } else { "issue" };
    let mut args = vec![
        entity.to_string(),
        "edit".to_string(),
        "--repo".to_string(),
        format!("{}/{}", target.owner, target.repo),
        target.number.to_string(),
    ];
    for a in to_add {
        args.push("--add-assignee".to_string());
        args.push(a.clone());
    }
    for a in to_remove {
        args.push("--remove-assignee".to_string());
        args.push(a.clone());
    }
    args
}

/// Build `gh issue edit` / `gh pr edit` args to set a milestone.
#[must_use]
pub fn build_set_milestone_args(
    owner: &str,
    repo: &str,
    number: u64,
    is_pr: bool,
    milestone: &str,
) -> Vec<String> {
    let entity = if is_pr { "pr" } else { "issue" };
    vec![
        entity.to_string(),
        "edit".to_string(),
        "--repo".to_string(),
        format!("{owner}/{repo}"),
        number.to_string(),
        "--milestone".to_string(),
        milestone.to_string(),
    ]
}

/// Build `gh issue edit` / `gh pr edit` args to clear the milestone.
#[must_use]
pub fn build_clear_milestone_args(
    owner: &str,
    repo: &str,
    number: u64,
    is_pr: bool,
) -> Vec<String> {
    let entity = if is_pr { "pr" } else { "issue" };
    vec![
        entity.to_string(),
        "edit".to_string(),
        "--repo".to_string(),
        format!("{owner}/{repo}"),
        number.to_string(),
        "--remove-milestone".to_string(),
    ]
}

/// Build `gh issue edit` / `gh pr edit` args to update the title.
#[must_use]
pub fn build_set_title_args(
    owner: &str,
    repo: &str,
    number: u64,
    is_pr: bool,
    title: &str,
) -> Vec<String> {
    let entity = if is_pr { "pr" } else { "issue" };
    vec![
        entity.to_string(),
        "edit".to_string(),
        "--repo".to_string(),
        format!("{owner}/{repo}"),
        number.to_string(),
        "--title".to_string(),
        title.to_string(),
    ]
}

/// Build `gh issue close` or `gh pr close` args.
#[must_use]
pub fn build_close_args(owner: &str, repo: &str, number: u64, is_pr: bool) -> Vec<String> {
    let entity = if is_pr { "pr" } else { "issue" };
    vec![
        entity.to_string(),
        "close".to_string(),
        number.to_string(),
        "--repo".to_string(),
        format!("{owner}/{repo}"),
    ]
}

/// Build `gh issue reopen` or `gh pr reopen` args.
#[must_use]
pub fn build_reopen_args(owner: &str, repo: &str, number: u64, is_pr: bool) -> Vec<String> {
    let entity = if is_pr { "pr" } else { "issue" };
    vec![
        entity.to_string(),
        "reopen".to_string(),
        number.to_string(),
        "--repo".to_string(),
        format!("{owner}/{repo}"),
    ]
}

// ── Issue Type (GraphQL) ───────────────────────────────────────────────────

/// Build the GraphQL query to fetch the repo's available issue types.
///
/// Returns the argument vector for `gh api graphql -f query=... -F owner=... -F name=...`.
#[must_use]
pub fn build_issue_types_query_args(owner: &str, name: &str) -> Vec<String> {
    let query = "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { issueTypes(first: 50) { nodes { id name } } } }";
    vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={query}"),
        "-F".to_string(),
        format!("owner={owner}"),
        "-F".to_string(),
        format!("name={name}"),
    ]
}

/// Parse the issue-types GraphQL response into `(id, name)` pairs.
///
/// # Errors
/// Returns [`GhError::ParseError`] if the JSON is malformed or missing expected fields.
pub fn parse_issue_types(json: &str) -> Result<Vec<(String, String)>, GhError> {
    let value: Value = serde_json::from_str(json)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;
    let nodes = value
        .pointer("/data/repository/issueTypes/nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| GhError::ParseError("Missing issueTypes.nodes in response".to_string()))?;
    let mut result = Vec::new();
    for node in nodes {
        let id = node
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| GhError::ParseError("Issue type node missing id".to_string()))?;
        let name = node
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| GhError::ParseError("Issue type node missing name".to_string()))?;
        result.push((id.to_string(), name.to_string()));
    }
    Ok(result)
}

/// Build the GraphQL query to fetch an issue's node id and current issue type.
///
/// Returns the argument vector for `gh api graphql`.
#[must_use]
pub fn build_issue_node_id_query_args(owner: &str, name: &str, number: u64) -> Vec<String> {
    let query = "query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { issue(number: $number) { id issueType { id name } } } }";
    vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={query}"),
        "-F".to_string(),
        format!("owner={owner}"),
        "-F".to_string(),
        format!("name={name}"),
        "-F".to_string(),
        format!("number={number}"),
    ]
}

/// The result of parsing an issue node-id query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueNodeInfo {
    pub node_id: String,
    pub current_type_id: Option<String>,
}

/// Parse the issue node-id GraphQL response.
///
/// # Errors
/// Returns [`GhError::ParseError`] if the JSON is malformed or missing the node id.
pub fn parse_issue_node_info(json: &str) -> Result<IssueNodeInfo, GhError> {
    let value: Value = serde_json::from_str(json)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;
    let issue = value
        .pointer("/data/repository/issue")
        .ok_or_else(|| GhError::ParseError("Missing repository.issue in response".to_string()))?;
    let node_id = issue
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| GhError::ParseError("Issue node missing id".to_string()))?;
    let current_type_id = issue
        .pointer("/issueType/id")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(IssueNodeInfo {
        node_id: node_id.to_string(),
        current_type_id,
    })
}

/// Build the GraphQL mutation to set or clear an issue's type.
///
/// When `type_id` is `None`, passes `null` for `issueTypeId` (clears the type).
#[must_use]
pub fn build_update_issue_type_args(node_id: &str, type_id: Option<&str>) -> Vec<String> {
    let query = if type_id.is_some() {
        "mutation($id: ID!, $type: ID!) { updateIssue(input: {id: $id, issueTypeId: $type}) { issue { id } } }"
    } else {
        "mutation($id: ID!) { updateIssue(input: {id: $id, issueTypeId: null}) { issue { id } } }"
    };
    let mut args = vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={query}"),
        "-F".to_string(),
        format!("id={node_id}"),
    ];
    if let Some(tid) = type_id {
        args.push("-F".to_string());
        args.push(format!("type={tid}"));
    }
    args
}

// ── Options-fetching queries (labels, assignees, milestones) ───────────────

/// Build the GraphQL query to fetch the repo's labels for the property editor.
#[must_use]
pub fn build_labels_query_args(owner: &str, name: &str) -> Vec<String> {
    let query = "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { labels(first: 100, orderBy: {field: NAME, direction: ASC}) { nodes { name } } } }";
    vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={query}"),
        "-F".to_string(),
        format!("owner={owner}"),
        "-F".to_string(),
        format!("name={name}"),
    ]
}

/// Parse the labels GraphQL response into a sorted list of label names.
///
/// # Errors
/// Returns [`GhError::ParseError`] if the JSON is malformed.
pub fn parse_label_names(json: &str) -> Result<Vec<String>, GhError> {
    let value: Value = serde_json::from_str(json)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;
    let nodes = value
        .pointer("/data/repository/labels/nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| GhError::ParseError("Missing labels.nodes in response".to_string()))?;
    let mut names: Vec<String> = nodes
        .iter()
        .filter_map(|n| n.get("name").and_then(Value::as_str).map(str::to_string))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    Ok(names)
}

/// Build the GraphQL query to fetch the repo's milestones.
#[must_use]
pub fn build_milestones_query_args(owner: &str, name: &str) -> Vec<String> {
    let query = "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { milestones(first: 50, states: [OPEN], orderBy: {field: CREATED_AT, direction: DESC}) { nodes { title } } } }";
    vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={query}"),
        "-F".to_string(),
        format!("owner={owner}"),
        "-F".to_string(),
        format!("name={name}"),
    ]
}

/// Parse the milestones GraphQL response into a list of milestone titles.
///
/// # Errors
/// Returns [`GhError::ParseError`] if the JSON is malformed.
pub fn parse_milestone_titles(json: &str) -> Result<Vec<String>, GhError> {
    let value: Value = serde_json::from_str(json)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;
    let nodes = value
        .pointer("/data/repository/milestones/nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| GhError::ParseError("Missing milestones.nodes in response".to_string()))?;
    let titles: Vec<String> = nodes
        .iter()
        .filter_map(|n| n.get("title").and_then(Value::as_str).map(str::to_string))
        .collect();
    Ok(titles)
}

/// Build the GraphQL query to fetch the repo's assignable users (assignees).
#[must_use]
pub fn build_assignees_query_args(owner: &str, name: &str) -> Vec<String> {
    let query = "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { assignees(first: 100) { nodes { login } } } }";
    vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={query}"),
        "-F".to_string(),
        format!("owner={owner}"),
        "-F".to_string(),
        format!("name={name}"),
    ]
}

/// Parse the assignees GraphQL response into a list of login names.
///
/// # Errors
/// Returns [`GhError::ParseError`] if the JSON is malformed.
pub fn parse_assignee_logins(json: &str) -> Result<Vec<String>, GhError> {
    let value: Value = serde_json::from_str(json)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;
    let nodes = value
        .pointer("/data/repository/assignees/nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| GhError::ParseError("Missing assignees.nodes in response".to_string()))?;
    let logins: Vec<String> = nodes
        .iter()
        .filter_map(|n| n.get("login").and_then(Value::as_str).map(str::to_string))
        .collect();
    Ok(logins)
}

// ── GhClient method wrappers (issue #175) ──────────────────────────────────
//
// These are defined in a separate `impl GhClient` block here (Rust allows
// multiple impl blocks) to keep `mod.rs` under the source-file-size limit.
// They delegate to the pure arg-builder/parse functions above and call
// `GhClient::run_gh` for the actual subprocess invocation.

use super::GhClient;

impl GhClient {
    /// Edit labels on an issue or PR by diffing current vs desired.
    pub fn edit_labels(
        &self,
        target: PropertyEditTarget,
        to_add: &[String],
        to_remove: &[String],
    ) -> Result<(), GhError> {
        let args = build_edit_labels_args(target, to_add, to_remove);
        if args.is_empty() {
            return Ok(());
        }
        Self::run_gh(&args)?;
        Ok(())
    }

    /// Edit assignees on an issue or PR by diffing current vs desired.
    pub fn edit_assignees(
        &self,
        target: PropertyEditTarget,
        to_add: &[String],
        to_remove: &[String],
    ) -> Result<(), GhError> {
        let args = build_edit_assignees_args(target, to_add, to_remove);
        if args.is_empty() {
            return Ok(());
        }
        Self::run_gh(&args)?;
        Ok(())
    }

    /// Set the milestone on an issue or PR.
    pub fn set_milestone(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        is_pr: bool,
        milestone: &str,
    ) -> Result<(), GhError> {
        let args = build_set_milestone_args(owner, repo, number, is_pr, milestone);
        Self::run_gh(&args)?;
        Ok(())
    }

    /// Clear the milestone on an issue or PR.
    pub fn clear_milestone(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        is_pr: bool,
    ) -> Result<(), GhError> {
        let args = build_clear_milestone_args(owner, repo, number, is_pr);
        Self::run_gh(&args)?;
        Ok(())
    }

    /// Update the title of an issue or PR.
    pub fn set_title(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        is_pr: bool,
        title: &str,
    ) -> Result<(), GhError> {
        let args = build_set_title_args(owner, repo, number, is_pr, title);
        Self::run_gh(&args)?;
        Ok(())
    }

    /// Close an issue or PR.
    pub fn close_item(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        is_pr: bool,
    ) -> Result<(), GhError> {
        let args = build_close_args(owner, repo, number, is_pr);
        Self::run_gh(&args)?;
        Ok(())
    }

    /// Reopen an issue or PR.
    pub fn reopen_item(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        is_pr: bool,
    ) -> Result<(), GhError> {
        let args = build_reopen_args(owner, repo, number, is_pr);
        Self::run_gh(&args)?;
        Ok(())
    }

    /// Fetch the repo's available issue types via GraphQL.
    pub fn fetch_issue_types(
        &self,
        owner: &str,
        name: &str,
    ) -> Result<Vec<(String, String)>, GhError> {
        let args = build_issue_types_query_args(owner, name);
        let stdout = Self::run_gh(&args)?;
        parse_issue_types(&stdout)
    }

    /// Fetch an issue's node id and current issue type via GraphQL.
    pub fn fetch_issue_node_info(
        &self,
        owner: &str,
        name: &str,
        number: u64,
    ) -> Result<IssueNodeInfo, GhError> {
        let args = build_issue_node_id_query_args(owner, name, number);
        let stdout = Self::run_gh(&args)?;
        parse_issue_node_info(&stdout)
    }

    /// Set or clear an issue's type via the GraphQL `updateIssue` mutation.
    pub fn set_issue_type(&self, node_id: &str, type_id: Option<&str>) -> Result<(), GhError> {
        let args = build_update_issue_type_args(node_id, type_id);
        Self::run_gh(&args)?;
        Ok(())
    }

    /// Fetch the repo's labels for the property editor.
    pub fn fetch_label_names(&self, owner: &str, name: &str) -> Result<Vec<String>, GhError> {
        let args = build_labels_query_args(owner, name);
        let stdout = Self::run_gh(&args)?;
        parse_label_names(&stdout)
    }

    /// Fetch the repo's open milestones for the property editor.
    pub fn fetch_milestone_titles(&self, owner: &str, name: &str) -> Result<Vec<String>, GhError> {
        let args = build_milestones_query_args(owner, name);
        let stdout = Self::run_gh(&args)?;
        parse_milestone_titles(&stdout)
    }

    /// Fetch the repo's assignable users for the property editor.
    pub fn fetch_assignee_logins(&self, owner: &str, name: &str) -> Result<Vec<String>, GhError> {
        let args = build_assignees_query_args(owner, name);
        let stdout = Self::run_gh(&args)?;
        parse_assignee_logins(&stdout)
    }
}

#[cfg(test)]
#[path = "edit_properties_tests.rs"]
mod tests;
