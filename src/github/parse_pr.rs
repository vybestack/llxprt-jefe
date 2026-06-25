//! Pull-request parsing and CLI-argument-building helpers for the GitHub
//! client boundary.
//!
//! TOTAL-STUB phase (P06): every function body returns a deterministic
//! WRONG/empty/default value so that P07 RED tests can target these
//! signatures and fail by behavioral assertion, never by panic. Real parsing
//! and transport logic arrives in P08.
//!
//! Boundary isolation: this module imports ONLY `crate::domain`, `serde_json`,
//! and sibling `crate::github` types — mirroring `src/github/parse.rs`. It does
//! NOT import `crate::ui`, `crate::state`, or `crate::app_input`.

use crate::domain::{
    PrCheck, PrCheckStatus, PrFilter, PrReview, PrReviewState, PrState, PullRequest,
    PullRequestDetail,
};
use serde_json::Value;

use super::{GhError, PrListResponse};

/// Build the `gh api graphql` argument vector for the PR search query.
///
/// TOTAL STUB: returns an empty vector. The real GraphQL `search(type: ISSUE,
/// query, first, after)` argument construction (mirroring
/// `build_issue_search_args`) arrives in P08.
///
/// @plan PLAN-20260624-PR-MODE.P06
/// @requirement REQ-PR-007
/// @pseudocode component-002 lines 35-58
#[must_use]
pub fn build_pr_search_args(
    _owner: &str,
    _name: &str,
    _filter: &PrFilter,
    _cursor: Option<&str>,
    _page_size: u32,
) -> Vec<String> {
    Vec::new()
}

/// Build the GitHub search-qualifier string (incl. `is:pr`) for the PR query.
///
/// TOTAL STUB: returns an empty string. The real qualifier-string construction
/// (state, labels, author, assignee, reviewer, draft, review, checks, free
/// text) arrives in P08.
///
/// @plan PLAN-20260624-PR-MODE.P06
/// @requirement REQ-PR-007
/// @pseudocode component-002 lines 59-73
#[must_use]
pub fn build_pr_search_query(_owner: &str, _repo: &str, _filter: &PrFilter) -> String {
    String::new()
}

/// Parse JSON output from the GraphQL PR search query into a paginated
/// response.
///
/// TOTAL STUB: returns `Ok(PrListResponse::default())`. Real parsing (reading
/// `data.search.nodes`, mapping each node to a `PullRequest`, and extracting
/// the real `endCursor`/`hasNextPage` via `parse_page_info`) arrives in P08.
///
/// @plan PLAN-20260624-PR-MODE.P06
/// @requirement REQ-PR-006
/// @pseudocode component-002 lines 138-156
pub fn parse_pull_requests_json(_json_str: &str) -> Result<PrListResponse, GhError> {
    Ok(PrListResponse::default())
}

/// Parse JSON output from `gh pr view --json` into a [`PullRequestDetail`].
///
/// TOTAL STUB: `PullRequestDetail` does not derive `Default`, so this returns a
/// deterministic `Err` rather than fabricating a struct or panicking. The real
/// parsing (state mapping, reviews/checks rollup, external_url) arrives in P08.
///
/// @plan PLAN-20260624-PR-MODE.P06
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 157-166
pub fn parse_pull_request_detail_json(
    _json_str: &str,
    _owner_name: &str,
) -> Result<PullRequestDetail, GhError> {
    Err(GhError::ParseError("stub".to_string()))
}

/// Parse a single review node into a [`PrReview`].
///
/// TOTAL STUB: returns a default/degraded `PrReview`. This is a TOTAL function
/// (never drops an entry); the real field extraction arrives in P08.
///
/// @plan PLAN-20260624-PR-MODE.P06
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 174-180
#[must_use]
pub fn parse_pr_review(_node: &Value) -> PrReview {
    PrReview {
        author_login: String::new(),
        state: PrReviewState::Commented,
        submitted_at: String::new(),
        body: None,
    }
}

/// Parse a single statusCheckRollup node into a [`PrCheck`].
///
/// TOTAL STUB: returns a default/degraded `PrCheck`. Handles both `CheckRun`
/// and `StatusContext` shapes in the real P08 implementation.
///
/// @plan PLAN-20260624-PR-MODE.P06
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 181-193
#[must_use]
pub fn parse_pr_check(_node: &Value) -> PrCheck {
    PrCheck {
        name: String::new(),
        status: PrCheckStatus::Pending,
        conclusion: String::new(),
        url: None,
    }
}

/// Parse the `reviewDecision` field into an optional [`PrReviewState`].
///
/// TOTAL STUB: returns `None`.
///
/// @plan PLAN-20260624-PR-MODE.P06
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 202-204
#[must_use]
pub fn parse_review_decision(_value: &Value) -> Option<PrReviewState> {
    None
}

/// Map a raw status/conclusion/state token to a [`PrCheckStatus`].
///
/// TOTAL STUB: returns `PrCheckStatus::Pending`. The real union mapping of
/// CheckRun-conclusion, CheckRun-status, and StatusContext-state tokens arrives
/// in P08.
///
/// @plan PLAN-20260624-PR-MODE.P06
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 205-215
#[must_use]
pub fn parse_check_status(_raw_status: &str) -> PrCheckStatus {
    PrCheckStatus::Pending
}

/// Aggregate the per-node check statuses into a single rollup status.
///
/// TOTAL STUB: returns `PrCheckStatus::None` (the empty-rollup result). The
/// real precedence logic (any Failure -> Failure, any Pending -> Pending, all
/// Success -> Success, else Neutral) arrives in P08.
///
/// @plan PLAN-20260624-PR-MODE.P06
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 216-222
#[must_use]
pub fn parse_checks_rollup(_nodes: &[Value]) -> PrCheckStatus {
    PrCheckStatus::None
}

/// Map the GraphQL `state` + `mergedAt` fields to a [`PrState`].
///
/// TOTAL STUB: returns `PrState::Open`. The real mapping (MERGED or non-null
/// mergedAt -> Merged, CLOSED -> Closed, else Open) arrives in P08.
///
/// @plan PLAN-20260624-PR-MODE.P06
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 197-201
#[must_use]
pub fn parse_pr_state(_state: &Value, _merged_at: &Value) -> PrState {
    PrState::Open
}

/// Sort pull requests by `updated_at` descending, then `number` ascending.
///
/// TOTAL STUB: no-op (empty body). The real sort arrives in P08.
///
/// @plan PLAN-20260624-PR-MODE.P06
/// @requirement REQ-PR-006
/// @pseudocode component-002 lines 194-196
pub fn sort_pull_requests(_items: &mut Vec<PullRequest>) {}
