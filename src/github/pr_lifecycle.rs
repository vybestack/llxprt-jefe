//! Pull-request lifecycle `gh` calls: head-branch deletion and PR creation
//! (issue #183).
//!
//! Closing and reopening a pull request already have exactly one home
//! (`edit_properties::close_item` / `reopen_item`) and are deliberately not
//! duplicated here. What this module adds is the part GitHub exposes nowhere
//! else in jefe: resolving and deleting a head branch, listing the branches a
//! new pull request can be opened from, and opening it.

use serde_json::Value;

use super::{GhClient, GhError, graphql_errors};

/// Resolve a branch to its GraphQL ref node id.
const BRANCH_REF_ID_QUERY: &str = "query($owner: String!, $name: String!, $qualified: String!) { \
     repository(owner: $owner, name: $name) { ref(qualifiedName: $qualified) { id } } }";

/// Delete a ref by node id.
const DELETE_REF_QUERY: &str =
    "mutation($id: ID!) { deleteRef(input: {refId: $id}) { clientMutationId } }";

/// One page of the repository's branches, plus its default branch.
const BRANCHES_QUERY: &str = "query($owner: String!, $name: String!) { \
     repository(owner: $owner, name: $name) { defaultBranchRef { name } \
     refs(refPrefix: \"refs/heads/\", first: 100, \
     orderBy: {field: ALPHABETICAL, direction: ASC}) { nodes { name } \
     pageInfo { hasNextPage endCursor } } } }";

/// The continuation form of [`BRANCHES_QUERY`].
const BRANCHES_QUERY_AFTER: &str = "query($owner: String!, $name: String!, $after: String!) { \
     repository(owner: $owner, name: $name) { defaultBranchRef { name } \
     refs(refPrefix: \"refs/heads/\", first: 100, after: $after, \
     orderBy: {field: ALPHABETICAL, direction: ASC}) { nodes { name } \
     pageInfo { hasNextPage endCursor } } } }";

/// Upper bound on branch pages fetched for one composer open. Guards against a
/// runaway loop from a malformed `hasNextPage`.
const MAX_BRANCH_PAGES: usize = 20;

/// The repository's branches as the New PR composer sees them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryBranches {
    /// Branch names, alphabetically ascending.
    pub names: Vec<String>,
    /// The repository's default branch, when it has one.
    pub default_branch: Option<String>,
}

impl GhClient {
    /// Resolve a branch name to the GraphQL ref node id `deleteRef` needs.
    ///
    /// # Errors
    /// [`GhError::ApiError`] when the branch does not exist in this repository
    /// (which is what a fork-headed pull request looks like from here), and the
    /// usual transport errors from `gh`.
    pub fn resolve_branch_ref_id(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<String, GhError> {
        let stdout = Self::run_gh(&build_branch_ref_id_args(owner, repo, branch))?;
        parse_branch_ref_id(&stdout)
    }

    /// Delete a branch by its ref node id.
    ///
    /// # Errors
    /// [`GhError::ApiError`] when GitHub refuses the deletion (a protected
    /// branch, or one the token cannot write).
    pub fn delete_branch_ref(&self, ref_id: &str) -> Result<(), GhError> {
        let stdout = Self::run_gh(&build_delete_ref_args(ref_id))?;
        graphql_errors::reject_mutation_errors(&stdout, "deleteRef")
    }

    /// List the repository's branches and its default branch.
    ///
    /// # Errors
    /// [`GhError::ParseError`] when the paging never terminates, plus the usual
    /// transport and envelope errors.
    pub fn fetch_repository_branches(
        &self,
        owner: &str,
        name: &str,
    ) -> Result<RepositoryBranches, GhError> {
        let mut names = Vec::new();
        let mut default_branch = None;
        let mut cursor: Option<String> = None;
        for page in 0..MAX_BRANCH_PAGES {
            let stdout = Self::run_gh(&build_branches_query_args(owner, name, cursor.as_deref()))?;
            // Parse the page first: it rejects a malformed body, so reading the
            // default branch afterwards cannot mistake a corrupt answer for a
            // repository that simply has no default branch.
            let (mut page_names, next) = parse_branches_page(&stdout)?;
            if page == 0 {
                default_branch = parse_default_branch_name(&stdout);
            }
            names.append(&mut page_names);
            match next {
                Some(next_cursor) if !next_cursor.is_empty() => cursor = Some(next_cursor),
                _ => {
                    return Ok(RepositoryBranches {
                        names,
                        default_branch,
                    });
                }
            }
        }
        Err(GhError::ParseError(format!(
            "branch pagination exceeded {MAX_BRANCH_PAGES} pages"
        )))
    }

    /// Open a pull request and return its number.
    ///
    /// # Errors
    /// The usual transport errors, plus [`GhError::ParseError`] when the create
    /// response carries no pull-request number.
    pub fn create_pull_request(&self, target: CreatePullRequest<'_>) -> Result<u64, GhError> {
        let stdout = Self::run_gh(&build_create_pr_args(
            target.owner,
            target.repo,
            target.head,
            target.base,
            target.title,
            target.body,
        ))?;
        parse_created_pr_number(&stdout)
    }
}

/// Everything one pull-request creation needs, so the call site reads as a
/// request rather than as six positional strings.
#[derive(Debug, Clone, Copy)]
pub struct CreatePullRequest<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    pub head: &'a str,
    pub base: &'a str,
    pub title: &'a str,
    pub body: &'a str,
}

/// Build the `gh api graphql` args that resolve a branch's ref node id.
#[must_use]
pub fn build_branch_ref_id_args(owner: &str, repo: &str, branch: &str) -> Vec<String> {
    vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={BRANCH_REF_ID_QUERY}"),
        "-F".to_string(),
        format!("owner={owner}"),
        "-F".to_string(),
        format!("name={repo}"),
        "-F".to_string(),
        format!("qualified=refs/heads/{branch}"),
    ]
}

/// Read the ref node id out of a branch-resolution response.
///
/// # Errors
/// [`GhError::ApiError`] for a reported GraphQL error or a branch this
/// repository does not have, and [`GhError::ParseError`] for a body that is not
/// the expected shape.
pub fn parse_branch_ref_id(json: &str) -> Result<String, GhError> {
    let value: Value = serde_json::from_str(json.trim())
        .map_err(|e| GhError::ParseError(format!("invalid JSON resolving branch ref: {e}")))?;
    if let Some(messages) = graphql_errors::error_messages(&value) {
        return Err(GhError::ApiError(format!(
            "GraphQL error resolving branch ref: {}",
            messages.join("; ")
        )));
    }
    let id = value
        .pointer("/data/repository/ref/id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty());
    id.map(str::to_string).ok_or_else(|| {
        GhError::ApiError(
            "branch not found in this repository (a pull request from a fork keeps its head \
             branch in the fork)"
                .to_string(),
        )
    })
}

/// Build the `gh api graphql` args for the `deleteRef` mutation.
#[must_use]
pub fn build_delete_ref_args(ref_id: &str) -> Vec<String> {
    vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={DELETE_REF_QUERY}"),
        "-F".to_string(),
        format!("id={ref_id}"),
    ]
}

/// Build the `gh api graphql` args for one page of the repository's branches.
#[must_use]
pub fn build_branches_query_args(owner: &str, name: &str, after: Option<&str>) -> Vec<String> {
    let query = if after.is_some() {
        BRANCHES_QUERY_AFTER
    } else {
        BRANCHES_QUERY
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

/// Read one page of branch names plus the cursor for the page after it.
///
/// # Errors
/// [`GhError::ApiError`] for a reported GraphQL error and
/// [`GhError::ParseError`] for a body that is not the expected shape.
pub fn parse_branches_page(json: &str) -> Result<(Vec<String>, Option<String>), GhError> {
    let value: Value = serde_json::from_str(json.trim())
        .map_err(|e| GhError::ParseError(format!("invalid JSON listing branches: {e}")))?;
    if let Some(messages) = graphql_errors::error_messages(&value) {
        return Err(GhError::ApiError(format!(
            "GraphQL error listing branches: {}",
            messages.join("; ")
        )));
    }
    let connection = value
        .pointer("/data/repository/refs")
        .ok_or_else(|| GhError::ParseError("missing refs in branch response".to_string()))?;
    let nodes = connection
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| GhError::ParseError("missing refs.nodes in branch response".to_string()))?;
    let names = nodes
        .iter()
        .filter_map(|node| node.get("name").and_then(Value::as_str).map(str::to_string))
        .collect();
    Ok((names, next_branch_cursor(connection)))
}

/// Read the repository's default branch from a branch page, when it has one.
///
/// `None` means the repository has no default branch. Call this only on a body
/// [`parse_branches_page`] has already accepted, so a malformed answer has
/// been rejected before it can be mistaken for an absent default branch.
#[must_use]
pub fn parse_default_branch_name(json: &str) -> Option<String> {
    let value: Value = serde_json::from_str(json.trim()).ok()?;
    value
        .pointer("/data/repository/defaultBranchRef/name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

/// The cursor for the next branch page, or `None` when this page is the last.
fn next_branch_cursor(connection: &Value) -> Option<String> {
    let page_info = connection.get("pageInfo")?;
    if !page_info.get("hasNextPage").and_then(Value::as_bool)? {
        return None;
    }
    page_info
        .get("endCursor")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Build the `gh api` args that open a pull request.
///
/// The REST create endpoint is used rather than `gh pr create` because it
/// answers with JSON, so the new pull request's number is read rather than
/// scraped out of a URL.
#[must_use]
pub fn build_create_pr_args(
    owner: &str,
    repo: &str,
    head: &str,
    base: &str,
    title: &str,
    body: &str,
) -> Vec<String> {
    vec![
        "api".to_string(),
        "--method".to_string(),
        "POST".to_string(),
        format!("/repos/{owner}/{repo}/pulls"),
        "-f".to_string(),
        format!("title={title}"),
        "-f".to_string(),
        format!("body={body}"),
        "-f".to_string(),
        format!("head={head}"),
        "-f".to_string(),
        format!("base={base}"),
    ]
}

/// Read the new pull request's number out of the create response.
///
/// A REST body that carries a `message` instead of a number is GitHub
/// explaining a refusal — "No commits between main and topic", say — and that
/// sentence is far more useful than "carried no PR number", so it is surfaced.
///
/// # Errors
/// [`GhError::ApiError`] when GitHub explained a refusal, and
/// [`GhError::ParseError`] when the body is not JSON or carries no number.
pub fn parse_created_pr_number(json: &str) -> Result<u64, GhError> {
    let value: Value = serde_json::from_str(json.trim())
        .map_err(|e| GhError::ParseError(format!("invalid JSON creating pull request: {e}")))?;
    if let Some(number) = value.get("number").and_then(Value::as_u64) {
        return Ok(number);
    }
    match value.get("message").and_then(Value::as_str) {
        Some(message) if !message.is_empty() => Err(GhError::ApiError(message.to_string())),
        _ => Err(GhError::ParseError(
            "create response carried no PR number".to_string(),
        )),
    }
}

#[cfg(test)]
#[path = "pr_lifecycle_tests.rs"]
mod tests;
