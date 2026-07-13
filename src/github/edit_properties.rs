//! Property-editing `gh`/GraphQL argument builders and parsers (issue #175).
//!
//! All functions here are pure so the exact `gh` command shape can be unit
//! tested without a process boundary; the [`super::GhClient`] methods wrap
//! them with the actual `gh` subprocess invocation via `run_gh`.
//!
//! Mirrors the `build_*_args`/`parse_*` separation in `viewer.rs` and `parse.rs`.

use super::GhError;
use serde_json::Value;

/// Extract the next-page cursor from a GraphQL `pageInfo` object, returning
/// `Some(cursor)` only when `hasNextPage` is true and `endCursor` is present
/// (issue #175 F7).
fn next_page_cursor(connection: &Value) -> Option<String> {
    let page_info = connection.get("pageInfo")?;
    let has_next = page_info.get("hasNextPage").and_then(Value::as_bool)?;
    if !has_next {
        return None;
    }
    page_info
        .get("endCursor")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Maximum number of pages to fetch in a single `paginate` call. Guards
/// against runaway loops from a malformed `hasNextPage` (issue #175 F7).
const MAX_PAGINATION_PAGES: usize = 50;

/// Fetch all pages of a GraphQL connection by following `endCursor` until
/// `hasNextPage` is false (issue #175 F7).
///
/// - `build_args`: builds the `gh api graphql` arg vector for a page, given an
///   optional continuation cursor.
/// - `parse_page`: parses one page's JSON into `(items, next_cursor)`.
/// - `label`: a human-readable label for error messages (e.g. "labels").
///
/// # Errors
/// Propagates [`GhError`] from `run_gh` or the page parser.
fn paginate<T, F, P>(build_args: F, parse_page: P, label: &str) -> Result<Vec<T>, GhError>
where
    F: Fn(Option<&str>) -> Vec<String>,
    P: Fn(&str) -> Result<(Vec<T>, Option<String>), GhError>,
{
    let mut all = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGINATION_PAGES {
        let args = build_args(cursor.as_deref());
        let stdout = GhClient::run_gh(&args)?;
        let (mut items, next) = parse_page(&stdout)?;
        all.append(&mut items);
        match next {
            Some(next_cursor) if !next_cursor.is_empty() => cursor = Some(next_cursor),
            _ => return Ok(all),
        }
    }
    Err(GhError::ParseError(format!(
        "{label} pagination exceeded {MAX_PAGINATION_PAGES} pages"
    )))
}

/// Reject names containing a comma, returning a [`GhError`] naming the field
/// and the offending value (issue #175 F8). `gh`'s `--add-label`/`--add-assignee`
/// split each value on commas, so a name with a comma would be silently
/// misinterpreted as multiple values.
fn reject_comma_names(field: &str, names: &[String]) -> Result<(), GhError> {
    for name in names {
        if name.contains(',') {
            return Err(GhError::ParseError(format!(
                "{field} name cannot contain a comma: '{name}'"
            )));
        }
    }
    Ok(())
}

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

/// Build the GraphQL query to fetch the repo's available issue types (issue
/// #175 F7 pagination).
///
/// Returns the argument vector for `gh api graphql -f query=... -F owner=... -F name=...`.
#[must_use]
pub fn build_issue_types_query_args(owner: &str, name: &str, after: Option<&str>) -> Vec<String> {
    let query = if after.is_some() {
        "query($owner: String!, $name: String!, $after: String!) { repository(owner: $owner, name: $name) { issueTypes(first: 50, after: $after) { nodes { id name } pageInfo { hasNextPage endCursor } } } }"
    } else {
        "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { issueTypes(first: 50) { nodes { id name } pageInfo { hasNextPage endCursor } } } }"
    };
    let mut args = vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={query}"),
        "-F".to_string(),
        format!("owner={owner}"),
        "-F".to_string(),
        format!("name={name}"),
    ];
    if let Some(cursor) = after {
        args.push("-F".to_string());
        args.push(format!("after={cursor}"));
    }
    args
}

/// Parse the issue-types GraphQL response into `(id, name)` pairs.
///
/// # Errors
/// Returns [`GhError::ParseError`] if the JSON is malformed or missing expected fields.
pub fn parse_issue_types(json: &str) -> Result<Vec<(String, String)>, GhError> {
    Ok(parse_issue_types_page(json)?.0)
}

/// Parse one page of the issue-types response, returning `(id, name)` pairs
/// and the next cursor when another page exists (issue #175 F7).
/// One page of issue-type results: `(id, name)` pairs and the next cursor.
pub type IssueTypesPage = (Vec<(String, String)>, Option<String>);

/// Parse one page of the issue-types response, returning `(id, name)` pairs
/// and the next cursor when another page exists (issue #175 F7).
///
/// # Errors
/// Returns [`GhError::ParseError`] if the JSON is malformed or missing expected fields.
pub fn parse_issue_types_page(json: &str) -> Result<IssueTypesPage, GhError> {
    let value: Value = serde_json::from_str(json)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;
    let connection = value
        .pointer("/data/repository/issueTypes")
        .ok_or_else(|| GhError::ParseError("Missing issueTypes in response".to_string()))?;
    let nodes = connection
        .get("nodes")
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
    Ok((result, next_page_cursor(connection)))
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
///
/// When `after` is `Some(cursor)`, the query continues from that cursor
/// (issue #175 F7 pagination).
#[must_use]
pub fn build_labels_query_args(owner: &str, name: &str, after: Option<&str>) -> Vec<String> {
    let query = if after.is_some() {
        "query($owner: String!, $name: String!, $after: String!) { repository(owner: $owner, name: $name) { labels(first: 100, after: $after, orderBy: {field: NAME, direction: ASC}) { nodes { name } pageInfo { hasNextPage endCursor } } } }"
    } else {
        "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { labels(first: 100, orderBy: {field: NAME, direction: ASC}) { nodes { name } pageInfo { hasNextPage endCursor } } } }"
    };
    let mut args = vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={query}"),
        "-F".to_string(),
        format!("owner={owner}"),
        "-F".to_string(),
        format!("name={name}"),
    ];
    if let Some(cursor) = after {
        args.push("-F".to_string());
        args.push(format!("after={cursor}"));
    }
    args
}

/// Parse the labels GraphQL response into a sorted list of label names plus
/// the next-page cursor (issue #175 F7).
///
/// # Errors
/// Returns [`GhError::ParseError`] if the JSON is malformed.
pub fn parse_label_names(json: &str) -> Result<Vec<String>, GhError> {
    Ok(parse_label_names_page(json)?.0)
}

/// Parse one page of the labels response, returning the names and the
/// next cursor when another page exists (issue #175 F7).
///
/// # Errors
/// Returns [`GhError::ParseError`] if the JSON is malformed.
pub fn parse_label_names_page(json: &str) -> Result<(Vec<String>, Option<String>), GhError> {
    let value: Value = serde_json::from_str(json)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;
    let connection = value
        .pointer("/data/repository/labels")
        .ok_or_else(|| GhError::ParseError("Missing labels in response".to_string()))?;
    let nodes = connection
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| GhError::ParseError("Missing labels.nodes in response".to_string()))?;
    let mut names: Vec<String> = nodes
        .iter()
        .filter_map(|n| n.get("name").and_then(Value::as_str).map(str::to_string))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    let next_cursor = next_page_cursor(connection);
    Ok((names, next_cursor))
}

/// Build the GraphQL query to fetch the repo's milestones (issue #175 F7).
#[must_use]
pub fn build_milestones_query_args(owner: &str, name: &str, after: Option<&str>) -> Vec<String> {
    let query = if after.is_some() {
        "query($owner: String!, $name: String!, $after: String!) { repository(owner: $owner, name: $name) { milestones(first: 50, after: $after, states: [OPEN], orderBy: {field: CREATED_AT, direction: DESC}) { nodes { title } pageInfo { hasNextPage endCursor } } } }"
    } else {
        "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { milestones(first: 50, states: [OPEN], orderBy: {field: CREATED_AT, direction: DESC}) { nodes { title } pageInfo { hasNextPage endCursor } } } }"
    };
    let mut args = vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={query}"),
        "-F".to_string(),
        format!("owner={owner}"),
        "-F".to_string(),
        format!("name={name}"),
    ];
    if let Some(cursor) = after {
        args.push("-F".to_string());
        args.push(format!("after={cursor}"));
    }
    args
}

/// Parse the milestones GraphQL response into a list of milestone titles.
///
/// # Errors
/// Returns [`GhError::ParseError`] if the JSON is malformed.
pub fn parse_milestone_titles(json: &str) -> Result<Vec<String>, GhError> {
    Ok(parse_milestone_titles_page(json)?.0)
}

/// Parse one page of the milestones response, returning titles and the next
/// cursor when another page exists (issue #175 F7).
///
/// # Errors
/// Returns [`GhError::ParseError`] if the JSON is malformed.
pub fn parse_milestone_titles_page(json: &str) -> Result<(Vec<String>, Option<String>), GhError> {
    let value: Value = serde_json::from_str(json)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;
    let connection = value
        .pointer("/data/repository/milestones")
        .ok_or_else(|| GhError::ParseError("Missing milestones in response".to_string()))?;
    let nodes = connection
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| GhError::ParseError("Missing milestones.nodes in response".to_string()))?;
    let titles: Vec<String> = nodes
        .iter()
        .filter_map(|n| n.get("title").and_then(Value::as_str).map(str::to_string))
        .collect();
    Ok((titles, next_page_cursor(connection)))
}

/// Build the GraphQL query to fetch the repo's assignable users (issue #175 F7).
#[must_use]
pub fn build_assignees_query_args(owner: &str, name: &str, after: Option<&str>) -> Vec<String> {
    let query = if after.is_some() {
        "query($owner: String!, $name: String!, $after: String!) { repository(owner: $owner, name: $name) { assignees(first: 100, after: $after) { nodes { login } pageInfo { hasNextPage endCursor } } } }"
    } else {
        "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { assignees(first: 100) { nodes { login } pageInfo { hasNextPage endCursor } } } }"
    };
    let mut args = vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={query}"),
        "-F".to_string(),
        format!("owner={owner}"),
        "-F".to_string(),
        format!("name={name}"),
    ];
    if let Some(cursor) = after {
        args.push("-F".to_string());
        args.push(format!("after={cursor}"));
    }
    args
}

/// Parse the assignees GraphQL response into a list of login names.
///
/// # Errors
/// Returns [`GhError::ParseError`] if the JSON is malformed.
pub fn parse_assignee_logins(json: &str) -> Result<Vec<String>, GhError> {
    Ok(parse_assignee_logins_page(json)?.0)
}

/// Parse one page of the assignees response, returning logins and the next
/// cursor when another page exists (issue #175 F7).
///
/// # Errors
/// Returns [`GhError::ParseError`] if the JSON is malformed.
pub fn parse_assignee_logins_page(json: &str) -> Result<(Vec<String>, Option<String>), GhError> {
    let value: Value = serde_json::from_str(json)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;
    let connection = value
        .pointer("/data/repository/assignees")
        .ok_or_else(|| GhError::ParseError("Missing assignees in response".to_string()))?;
    let nodes = connection
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| GhError::ParseError("Missing assignees.nodes in response".to_string()))?;
    let logins: Vec<String> = nodes
        .iter()
        .filter_map(|n| n.get("login").and_then(Value::as_str).map(str::to_string))
        .collect();
    Ok((logins, next_page_cursor(connection)))
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
        // F8: gh's --add-label/--remove-label split each value on commas, so a
        // label name containing a comma would be silently broken into multiple
        // labels. Reject such names with a clear error rather than corrupting
        // the label set. A full GraphQL labelIds migration would remove this
        // limitation.
        reject_comma_names("label", to_add)?;
        reject_comma_names("label", to_remove)?;
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
        paginate(
            |cursor| build_issue_types_query_args(owner, name, cursor),
            parse_issue_types_page,
            "issue types",
        )
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

    /// Fetch the repo's labels for the property editor, paginating through all
    /// pages (issue #175 F7).
    pub fn fetch_label_names(&self, owner: &str, name: &str) -> Result<Vec<String>, GhError> {
        paginate(
            |cursor| build_labels_query_args(owner, name, cursor),
            parse_label_names_page,
            "labels",
        )
    }

    /// Fetch the repo's open milestones for the property editor, paginating
    /// through all pages (issue #175 F7).
    pub fn fetch_milestone_titles(&self, owner: &str, name: &str) -> Result<Vec<String>, GhError> {
        paginate(
            |cursor| build_milestones_query_args(owner, name, cursor),
            parse_milestone_titles_page,
            "milestones",
        )
    }

    /// Fetch the repo's assignable users for the property editor, paginating
    /// through all pages (issue #175 F7).
    pub fn fetch_assignee_logins(&self, owner: &str, name: &str) -> Result<Vec<String>, GhError> {
        paginate(
            |cursor| build_assignees_query_args(owner, name, cursor),
            parse_assignee_logins_page,
            "assignees",
        )
    }
}

#[cfg(test)]
#[path = "edit_properties_tests.rs"]
mod tests;
