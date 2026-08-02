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
    collect_issue_type_matches(issue_type, cursor, page_size, |page_cursor| {
        fetch_issue_search_raw_page(owner, repo, filter, page_cursor, page_size, sort)
    })
}

/// Accumulate issue-type matches across raw pages until a page is ready
/// (issue #579).
///
/// A raw page is emitted whole or deferred whole, so the returned cursor is
/// always the end cursor of the last page every match of which was emitted.
/// While the underlying search results are stable, resuming from that cursor
/// therefore neither skips nor repeats an issue.
///
/// A raw page is fetched with `first = page_size` and filtering only removes
/// issues, so one page can never contribute more than `page_size` matches, and
/// deferring a page always leaves at least one match emitted. A response that
/// breaks that bound would leave the caller holding an empty page and the very
/// cursor it just used, so it is reported as an API error rather than an
/// invitation to request the same page forever.
///
/// The raw fetch is a parameter so the accumulation rules can be exercised
/// directly against scripted page sequences.
fn collect_issue_type_matches<F>(
    issue_type: &str,
    cursor: Option<&str>,
    page_size: u32,
    mut fetch_page: F,
) -> Result<IssueListResponse, GhError>
where
    F: FnMut(Option<&str>) -> Result<IssueListResponse, GhError>,
{
    let page_size = page_size as usize;
    let mut search_cursor = cursor.map(str::to_string);
    let mut collected: Vec<Issue> = Vec::new();

    loop {
        let response = fetch_page(search_cursor.as_deref())?;
        let matches = response
            .issues
            .into_iter()
            .filter(|issue| issue.issue_type.eq_ignore_ascii_case(issue_type))
            .collect::<Vec<Issue>>();
        if collected.len() + matches.len() > page_size {
            if collected.is_empty() {
                return Err(GhError::ApiError(format!(
                    "issue search returned {} matching issues for a page of {page_size}",
                    matches.len()
                )));
            }
            return Ok(IssueListResponse {
                issues: collected,
                cursor: search_cursor,
                has_more: true,
            });
        }
        collected.extend(matches);
        if collected.len() >= page_size || !response.has_more {
            return Ok(IssueListResponse {
                issues: collected,
                cursor: response.cursor,
                has_more: response.has_more,
            });
        }
        // A page promising more results without handing back a usable next
        // cursor cannot be followed: reusing the current cursor would refetch
        // the same page, and dropping it would restart from the first page.
        if response.cursor.is_none() || response.cursor == search_cursor {
            return Ok(IssueListResponse {
                issues: collected,
                cursor: response.cursor,
                has_more: false,
            });
        }
        search_cursor = response.cursor;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::IssueState;

    /// A scripted raw search page: the issues the server returns, the end
    /// cursor it reports, and whether another page follows.
    struct RawPage {
        issues: Vec<Issue>,
        end_cursor: &'static str,
        has_next: bool,
    }

    /// Serves scripted pages the way the search connection does: a request
    /// without a cursor gets the first page, and a request carrying page N's
    /// end cursor gets page N+1.
    struct ScriptedSearch {
        pages: Vec<RawPage>,
    }

    impl ScriptedSearch {
        fn page_for(&self, cursor: Option<&str>) -> IssueListResponse {
            let index = match cursor {
                None => 0,
                Some(requested) => {
                    let Some(previous) = self
                        .pages
                        .iter()
                        .position(|page| page.end_cursor == requested)
                    else {
                        panic!("scripted search received an unknown cursor: {requested}");
                    };
                    previous + 1
                }
            };
            let Some(page) = self.pages.get(index) else {
                panic!("scripted search was asked for a page it does not have");
            };
            IssueListResponse {
                issues: page.issues.clone(),
                cursor: Some(page.end_cursor.to_string()),
                has_more: page.has_next,
            }
        }
    }

    fn issue(number: u64, issue_type: &str) -> Issue {
        Issue {
            number,
            node_id: format!("I_{number}"),
            title: format!("Issue {number}"),
            state: IssueState::Open,
            author_login: "octocat".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            assignee_summary: String::new(),
            labels_summary: String::new(),
            assignees: Vec::new(),
            labels: Vec::new(),
            issue_type: issue_type.to_string(),
            milestone: String::new(),
            module: String::new(),
            comment_count: 0,
            body: String::new(),
            priority: None,
            state_reason: None,
            linked_pr_numbers: Vec::new(),
        }
    }

    fn page(issues: &[(u64, &str)], end_cursor: &'static str, has_next: bool) -> RawPage {
        RawPage {
            issues: issues
                .iter()
                .map(|&(number, issue_type)| issue(number, issue_type))
                .collect(),
            end_cursor,
            has_next,
        }
    }

    fn numbers(response: &IssueListResponse) -> Vec<u64> {
        response.issues.iter().map(|issue| issue.number).collect()
    }

    /// Three pages whose bug matches (1, 3, 4, 5, 6, 8, 9) straddle every page
    /// boundary, so a page of four cannot be filled from whole pages alone.
    fn straddling_search() -> ScriptedSearch {
        ScriptedSearch {
            pages: vec![
                page(
                    &[(1, "Bug"), (2, "Feature"), (3, "Bug"), (4, "Bug")],
                    "cursor-a",
                    true,
                ),
                page(
                    &[(5, "Bug"), (6, "Bug"), (7, "Feature"), (8, "Bug")],
                    "cursor-b",
                    true,
                ),
                page(&[(9, "Bug")], "cursor-c", false),
            ],
        }
    }

    #[test]
    fn filtered_page_never_returns_more_issues_than_requested() {
        let script = straddling_search();
        let Ok(response) =
            collect_issue_type_matches("Bug", None, 4, |cursor| Ok(script.page_for(cursor)))
        else {
            panic!("scripted search must not fail");
        };
        assert!(
            response.issues.len() <= 4,
            "a page of four must not carry {} issues: {:?}",
            response.issues.len(),
            numbers(&response)
        );
    }

    #[test]
    fn filtered_page_resumes_at_the_last_fully_emitted_page() {
        let script = straddling_search();
        let Ok(response) =
            collect_issue_type_matches("Bug", None, 4, |cursor| Ok(script.page_for(cursor)))
        else {
            panic!("scripted search must not fail");
        };
        assert_eq!(numbers(&response), vec![1, 3, 4]);
        assert_eq!(
            response.cursor.as_deref(),
            Some("cursor-a"),
            "the continuation cursor must mark the page whose matches were all emitted"
        );
        assert!(response.has_more, "more matching issues remain");
    }

    #[test]
    fn resuming_from_the_returned_cursor_skips_and_repeats_nothing() {
        let script = straddling_search();
        let mut seen: Vec<u64> = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let Ok(response) =
                collect_issue_type_matches("Bug", cursor.as_deref(), 4, |page_cursor| {
                    Ok(script.page_for(page_cursor))
                })
            else {
                panic!("scripted search must not fail");
            };
            assert!(
                response.issues.len() <= 4,
                "every emitted page must respect the requested size"
            );
            seen.extend(numbers(&response));
            if !response.has_more {
                break;
            }
            cursor = response.cursor;
        }
        assert_eq!(
            seen,
            vec![1, 3, 4, 5, 6, 8, 9],
            "walking the cursors must yield every match exactly once, in order"
        );
    }

    #[test]
    fn a_page_filled_exactly_reports_its_own_cursor() {
        let script = ScriptedSearch {
            pages: vec![
                page(&[(1, "Bug"), (3, "Bug")], "cursor-a", true),
                page(&[(4, "Bug")], "cursor-b", true),
            ],
        };
        let Ok(response) =
            collect_issue_type_matches("Bug", None, 2, |cursor| Ok(script.page_for(cursor)))
        else {
            panic!("scripted search must not fail");
        };
        assert_eq!(numbers(&response), vec![1, 3]);
        assert_eq!(response.cursor.as_deref(), Some("cursor-a"));
        assert!(response.has_more);
    }

    #[test]
    fn exhausted_pages_return_every_match_and_report_no_more() {
        let script = ScriptedSearch {
            pages: vec![
                page(&[(1, "Bug"), (2, "Feature")], "cursor-a", true),
                page(&[(3, "Bug")], "cursor-b", false),
            ],
        };
        let Ok(response) =
            collect_issue_type_matches("Bug", None, 10, |cursor| Ok(script.page_for(cursor)))
        else {
            panic!("scripted search must not fail");
        };
        assert_eq!(numbers(&response), vec![1, 3]);
        assert_eq!(response.cursor.as_deref(), Some("cursor-b"));
        assert!(!response.has_more);
    }

    #[test]
    fn pages_without_matches_are_skipped_until_the_page_fills() {
        let script = ScriptedSearch {
            pages: vec![
                page(&[(1, "Feature"), (2, "Feature")], "cursor-a", true),
                page(&[(3, "Bug"), (4, "Bug")], "cursor-b", true),
            ],
        };
        let Ok(response) =
            collect_issue_type_matches("Bug", None, 2, |cursor| Ok(script.page_for(cursor)))
        else {
            panic!("scripted search must not fail");
        };
        assert_eq!(numbers(&response), vec![3, 4]);
        assert_eq!(response.cursor.as_deref(), Some("cursor-b"));
    }

    #[test]
    fn a_cursor_that_never_advances_stops_the_loop() {
        let mut fetches = 0_u32;
        let Ok(response) = collect_issue_type_matches("Bug", Some("stuck"), 5, |cursor| {
            fetches += 1;
            assert_eq!(cursor, Some("stuck"), "the loop must not invent a cursor");
            Ok(IssueListResponse {
                issues: vec![issue(1, "Bug")],
                cursor: Some("stuck".to_string()),
                has_more: true,
            })
        }) else {
            panic!("scripted search must not fail");
        };
        assert_eq!(fetches, 1, "a stalled cursor must not be fetched again");
        assert_eq!(numbers(&response), vec![1]);
        assert!(
            !response.has_more,
            "a server that cannot advance must not invite another request"
        );
    }

    #[test]
    fn a_page_promising_more_without_a_cursor_stops_the_loop() {
        let mut fetches = 0_u32;
        let Ok(response) = collect_issue_type_matches("Bug", Some("cursor-a"), 5, |_| {
            fetches += 1;
            Ok(IssueListResponse {
                issues: vec![issue(1, "Bug")],
                cursor: None,
                has_more: true,
            })
        }) else {
            panic!("scripted search must not fail");
        };
        assert_eq!(
            fetches, 1,
            "a page without a next cursor must not restart pagination"
        );
        assert_eq!(numbers(&response), vec![1]);
        assert!(!response.has_more);
    }

    #[test]
    fn deferring_the_last_page_still_resumes_onto_it() {
        let script = ScriptedSearch {
            pages: vec![
                page(&[(1, "Bug"), (2, "Bug"), (3, "Feature")], "cursor-a", true),
                page(&[(4, "Bug"), (5, "Bug")], "cursor-b", false),
            ],
        };
        let Ok(first) =
            collect_issue_type_matches("Bug", None, 3, |cursor| Ok(script.page_for(cursor)))
        else {
            panic!("scripted search must not fail");
        };
        assert_eq!(numbers(&first), vec![1, 2]);
        assert!(
            first.has_more,
            "the deferred final page still holds matching issues"
        );
        let Ok(second) = collect_issue_type_matches("Bug", first.cursor.as_deref(), 3, |cursor| {
            Ok(script.page_for(cursor))
        }) else {
            panic!("scripted search must not fail");
        };
        assert_eq!(numbers(&second), vec![4, 5]);
        assert!(!second.has_more);
    }

    #[test]
    fn a_page_larger_than_requested_is_reported_instead_of_stalling() {
        let error = collect_issue_type_matches("Bug", None, 1, |_| {
            Ok(IssueListResponse {
                issues: vec![issue(1, "Bug"), issue(2, "Bug")],
                cursor: Some("cursor-a".to_string()),
                has_more: true,
            })
        });
        let Err(GhError::ApiError(message)) = error else {
            panic!("a page that cannot be emitted or deferred must be an API error");
        };
        assert!(
            message.contains("2 matching issues"),
            "the error must name the oversized page: {message}"
        );
    }

    #[test]
    fn a_failed_fetch_is_reported_even_after_partial_accumulation() {
        let mut fetches = 0_u32;
        let result = collect_issue_type_matches("Bug", None, 5, |_| {
            fetches += 1;
            if fetches == 1 {
                return Ok(IssueListResponse {
                    issues: vec![issue(1, "Bug")],
                    cursor: Some("cursor-a".to_string()),
                    has_more: true,
                });
            }
            Err(GhError::RateLimited)
        });
        assert!(
            matches!(result, Err(GhError::RateLimited)),
            "a mid-accumulation failure must not be reported as a short page"
        );
    }
}
