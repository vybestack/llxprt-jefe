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
/// When there is nothing to add or remove, returns an empty `Vec` (no-op).
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
    if !to_add.is_empty() {
        let joined = to_add.join(",");
        args.push("--add-label".to_string());
        args.push(joined);
    }
    if !to_remove.is_empty() {
        let joined = to_remove.join(",");
        args.push("--remove-label".to_string());
        args.push(joined);
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
mod tests {
    use super::*;

    trait TestResultExt<T> {
        fn value_or_panic(self, context: &str) -> T;
    }

    impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
        fn value_or_panic(self, context: &str) -> T {
            match self {
                Ok(value) => value,
                Err(error) => panic!("{context}: {error:?}"),
            }
        }
    }

    // ── Label diff ──────────────────────────────────────────────────────

    #[test]
    fn label_diff_adds_new_labels() {
        let current = vec!["bug".to_string()];
        let desired = vec!["bug".to_string(), "enhancement".to_string()];
        let (add, remove) = compute_label_diff(&current, &desired);
        assert_eq!(add, vec!["enhancement"]);
        assert!(remove.is_empty());
    }

    #[test]
    fn label_diff_removes_dropped_labels() {
        let current = vec!["bug".to_string(), "wontfix".to_string()];
        let desired = vec!["bug".to_string()];
        let (add, remove) = compute_label_diff(&current, &desired);
        assert!(add.is_empty());
        assert_eq!(remove, vec!["wontfix"]);
    }

    #[test]
    fn label_diff_case_insensitive_match() {
        let current = vec!["Bug".to_string()];
        let desired = vec!["bug".to_string()];
        let (add, remove) = compute_label_diff(&current, &desired);
        assert!(add.is_empty());
        assert!(remove.is_empty());
    }

    #[test]
    fn label_diff_noop_when_identical() {
        let current = vec!["bug".to_string(), "enhancement".to_string()];
        let desired = current.clone();
        let (add, remove) = compute_label_diff(&current, &desired);
        assert!(add.is_empty());
        assert!(remove.is_empty());
    }

    // ── Assignee diff ───────────────────────────────────────────────────

    #[test]
    fn assignee_diff_add_and_remove() {
        let current = vec!["alice".to_string(), "bob".to_string()];
        let desired = vec!["bob".to_string(), "carol".to_string()];
        let (add, remove) = compute_assignee_diff(&current, &desired);
        assert_eq!(add, vec!["carol"]);
        assert_eq!(remove, vec!["alice"]);
    }

    // ── Arg builders ────────────────────────────────────────────────────

    #[test]
    fn build_edit_labels_args_issue_add_and_remove() {
        let args = build_edit_labels_args(
            PropertyEditTarget {
                owner: "owner",
                repo: "repo",
                number: 42,
                is_pr: false,
            },
            &["enhancement".to_string()],
            &["bug".to_string()],
        );
        assert_eq!(
            args,
            vec![
                "issue",
                "edit",
                "--repo",
                "owner/repo",
                "42",
                "--add-label",
                "enhancement",
                "--remove-label",
                "bug",
            ]
        );
    }

    #[test]
    fn build_edit_labels_args_pr() {
        let args = build_edit_labels_args(
            PropertyEditTarget {
                owner: "owner",
                repo: "repo",
                number: 7,
                is_pr: true,
            },
            &["x".to_string()],
            &[],
        );
        assert_eq!(
            args,
            vec![
                "pr",
                "edit",
                "--repo",
                "owner/repo",
                "7",
                "--add-label",
                "x"
            ]
        );
    }

    #[test]
    fn build_edit_labels_args_noop_returns_empty() {
        let args = build_edit_labels_args(
            PropertyEditTarget {
                owner: "owner",
                repo: "repo",
                number: 42,
                is_pr: false,
            },
            &[],
            &[],
        );
        assert!(args.is_empty());
    }

    #[test]
    fn build_edit_labels_args_multiple_joined_by_comma() {
        let args = build_edit_labels_args(
            PropertyEditTarget {
                owner: "o",
                repo: "r",
                number: 1,
                is_pr: false,
            },
            &["a".to_string(), "b".to_string()],
            &[],
        );
        assert_eq!(
            args,
            vec!["issue", "edit", "--repo", "o/r", "1", "--add-label", "a,b"]
        );
    }

    #[test]
    fn build_edit_assignees_args_repeatable_flags() {
        let args = build_edit_assignees_args(
            PropertyEditTarget {
                owner: "o",
                repo: "r",
                number: 1,
                is_pr: false,
            },
            &["alice".to_string(), "bob".to_string()],
            &["carol".to_string()],
        );
        assert_eq!(
            args,
            vec![
                "issue",
                "edit",
                "--repo",
                "o/r",
                "1",
                "--add-assignee",
                "alice",
                "--add-assignee",
                "bob",
                "--remove-assignee",
                "carol",
            ]
        );
    }

    #[test]
    fn build_set_milestone_args_issue() {
        let args = build_set_milestone_args("owner", "repo", 42, false, "v1.0");
        assert_eq!(
            args,
            vec![
                "issue",
                "edit",
                "--repo",
                "owner/repo",
                "42",
                "--milestone",
                "v1.0"
            ]
        );
    }

    #[test]
    fn build_clear_milestone_args_pr() {
        let args = build_clear_milestone_args("owner", "repo", 7, true);
        assert_eq!(
            args,
            vec![
                "pr",
                "edit",
                "--repo",
                "owner/repo",
                "7",
                "--remove-milestone"
            ]
        );
    }

    #[test]
    fn build_set_title_args_issue() {
        let args = build_set_title_args("owner", "repo", 42, false, "New Title");
        assert_eq!(
            args,
            vec![
                "issue",
                "edit",
                "--repo",
                "owner/repo",
                "42",
                "--title",
                "New Title"
            ]
        );
    }

    #[test]
    fn build_close_args_pr() {
        let args = build_close_args("owner", "repo", 7, true);
        assert_eq!(args, vec!["pr", "close", "7", "--repo", "owner/repo"]);
    }

    #[test]
    fn build_reopen_args_issue() {
        let args = build_reopen_args("owner", "repo", 42, false);
        assert_eq!(args, vec!["issue", "reopen", "42", "--repo", "owner/repo"]);
    }

    // ── Issue Type GraphQL ──────────────────────────────────────────────

    #[test]
    fn build_issue_types_query_args_shape() {
        let args = build_issue_types_query_args("owner", "repo");
        assert_eq!(args[0], "api");
        assert_eq!(args[1], "graphql");
        assert!(args[3].contains("issueTypes"));
        assert!(args.contains(&"-F".to_string()));
        assert!(args.contains(&"owner=owner".to_string()));
        assert!(args.contains(&"name=repo".to_string()));
    }

    #[test]
    fn parse_issue_types_extracts_id_name_pairs() {
        let json = r#"{"data":{"repository":{"issueTypes":{"nodes":[{"id":"T_1","name":"Bug"},{"id":"T_2","name":"Feature"}]}}}}"#;
        let types = parse_issue_types(json).value_or_panic("should parse");
        assert_eq!(
            types,
            vec![
                ("T_1".to_string(), "Bug".to_string()),
                ("T_2".to_string(), "Feature".to_string())
            ]
        );
    }

    #[test]
    fn parse_issue_types_empty_list() {
        let json = r#"{"data":{"repository":{"issueTypes":{"nodes":[]}}}}"#;
        let types = parse_issue_types(json).value_or_panic("should parse");
        assert!(types.is_empty());
    }

    #[test]
    fn parse_issue_types_missing_nodes_errors() {
        let json = r#"{"data":{"repository":{}}}"#;
        assert!(parse_issue_types(json).is_err());
    }

    #[test]
    fn build_issue_node_id_query_args_shape() {
        let args = build_issue_node_id_query_args("owner", "repo", 42);
        assert!(args[3].contains("issue(number:"));
        assert!(args.contains(&"number=42".to_string()));
    }

    #[test]
    fn parse_issue_node_info_with_type() {
        let json = r#"{"data":{"repository":{"issue":{"id":"I_123","issueType":{"id":"T_1","name":"Bug"}}}}}"#;
        let info = parse_issue_node_info(json).value_or_panic("should parse");
        assert_eq!(info.node_id, "I_123");
        assert_eq!(info.current_type_id, Some("T_1".to_string()));
    }

    #[test]
    fn parse_issue_node_info_without_type() {
        let json = r#"{"data":{"repository":{"issue":{"id":"I_456","issueType":null}}}}"#;
        let info = parse_issue_node_info(json).value_or_panic("should parse");
        assert_eq!(info.node_id, "I_456");
        assert_eq!(info.current_type_id, None);
    }

    #[test]
    fn build_update_issue_type_args_set() {
        let args = build_update_issue_type_args("I_123", Some("T_1"));
        assert!(args[3].contains("$type: ID!"));
        assert!(args.contains(&"id=I_123".to_string()));
        assert!(args.contains(&"type=T_1".to_string()));
    }

    #[test]
    fn build_update_issue_type_args_clear() {
        let args = build_update_issue_type_args("I_123", None);
        assert!(args[3].contains("issueTypeId: null"));
        assert!(args.contains(&"id=I_123".to_string()));
        assert!(!args.iter().any(|a| a.starts_with("type=")));
    }

    // ── Options-fetch queries ───────────────────────────────────────────

    #[test]
    fn parse_label_names_sorted() {
        let json = r#"{"data":{"repository":{"labels":{"nodes":[{"name":"zebra"},{"name":"apple"},{"name":"Bug"}]}}}}"#;
        let names = parse_label_names(json).value_or_panic("should parse");
        assert_eq!(names, vec!["apple", "Bug", "zebra"]);
    }

    #[test]
    fn test_parse_milestone_titles() {
        let json = r#"{"data":{"repository":{"milestones":{"nodes":[{"title":"v1.0"},{"title":"v2.0"}]}}}}"#;
        let titles = parse_milestone_titles(json).value_or_panic("should parse");
        assert_eq!(titles, vec!["v1.0", "v2.0"]);
    }

    #[test]
    fn test_parse_assignee_logins() {
        let json = r#"{"data":{"repository":{"assignees":{"nodes":[{"login":"alice"},{"login":"bob"}]}}}}"#;
        let logins = parse_assignee_logins(json).value_or_panic("should parse");
        assert_eq!(logins, vec!["alice", "bob"]);
    }

    #[test]
    fn parse_label_names_missing_path_errors() {
        let json = r#"{"data":{}}"#;
        assert!(parse_label_names(json).is_err());
    }
}
