//! Issue list pagination: raw and filtered page fetching (issue #573).
//!
//! Extracted from `mod.rs` so that module stays under the source-size policy.
//! These helpers drive the cursor-based search-page fetch, including the
//! client-side filter loop used when an issue-type filter must be applied on
//! top of a search query that cannot express it server-side. The fetch order
//! follows the active [`IssueSortConfig`] so the server returns issues in the
//! same direction the user wants to view them.

use super::issue_query::{
    active_issue_type_filter, build_issue_search_args_sorted, issue_type_requires_search_filter,
};
use super::parse::{categorize_error, parse_issue_search_json};
use super::{GhError, IssueListResponse, gh_command};
use crate::domain::{Issue, IssueFilter, IssueSortConfig};

/// Fetch one search page, dispatching to the filtered loop when an issue-type
/// filter needs client-side narrowing.
pub(super) fn fetch_issue_search_page(
    owner: &str,
    repo: &str,
    filter: &IssueFilter,
    cursor: Option<&str>,
    page_size: u32,
    sort: IssueSortConfig,
) -> Result<IssueListResponse, GhError> {
    if active_issue_type_filter(filter).is_some() && issue_type_requires_search_filter(filter) {
        return fetch_issue_search_filtered_pages(owner, repo, filter, cursor, page_size, sort);
    }
    fetch_issue_search_raw_page(owner, repo, filter, cursor, page_size, sort)
}

/// Fetch raw search pages until enough issues match the issue-type filter,
/// narrowing client-side because the search query cannot express it.
fn fetch_issue_search_filtered_pages(
    owner: &str,
    repo: &str,
    filter: &IssueFilter,
    cursor: Option<&str>,
    page_size: u32,
    sort: IssueSortConfig,
) -> Result<IssueListResponse, GhError> {
    let Some(issue_type) = active_issue_type_filter(filter) else {
        return fetch_issue_search_raw_page(owner, repo, filter, cursor, page_size, sort);
    };
    let mut search_cursor = cursor.map(str::to_string);
    let mut collected: Vec<Issue> = Vec::new();
    let mut response_cursor: Option<String>;
    let mut response_has_more: bool;

    loop {
        let response = fetch_issue_search_raw_page(
            owner,
            repo,
            filter,
            search_cursor.as_deref(),
            page_size,
            sort,
        )?;
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

/// Fetch a single raw search page via `gh api graphql`.
fn fetch_issue_search_raw_page(
    owner: &str,
    repo: &str,
    filter: &IssueFilter,
    cursor: Option<&str>,
    page_size: u32,
    sort: IssueSortConfig,
) -> Result<IssueListResponse, GhError> {
    let args = build_issue_search_args_sorted(owner, repo, filter, cursor, page_size, sort);

    let output = gh_command()?.args(&args).output().map_err(|e| {
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
