//! Pull-request parsing and CLI-argument-building helpers for the GitHub
//! client boundary.
//!
//! P08 GREEN phase: real parsing/arg-building/error-mapping logic for the PR
//! search/list, detail, comments, rollup, review, check, state, and sort
//! helpers — replacing the P06 TOTAL-STUB bodies so all P07 RED tests pass.
//!
//! Boundary isolation: this module imports ONLY `crate::domain`, `serde_json`,
//! and sibling `crate::github` types — mirroring `src/github/parse.rs`. It does
//! NOT import `crate::ui`, `crate::state`, or `crate::app_input`.

use crate::domain::{
    ChecksFilter, IssueComment, PrCheck, PrCheckStatus, PrFilter, PrFilterState, PrReview,
    PrReviewState, PrReviewThread, PrState, PullRequest, PullRequestDetail, ReviewDecisionFilter,
};
use serde_json::Value;

use super::comment_pages::exhausted_comments;
use super::parse::parse_page_info;
use super::timestamp::cmp_rfc3339_newest_first;
use super::{GhError, PrListResponse};

/// Build the GraphQL query string for the PR comments fetch
/// (`repository.pullRequest(number:).comments`), so the query object path can
/// be tested in isolation from the I/O transport.
///
/// `with_cursor=true` includes the `$after` variable declaration and the
/// `after: $after` argument; `with_cursor=false` omits both. The object path
/// MUST be `repository(...) { pullRequest(number: $number) { comments(...) } }`
/// — `repository.issue(number:)` is NULL for a PR number (P00A §2d), so the
/// issue comments path cannot be reused for the fetch.
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-010
/// @pseudocode component-002 lines 102-107
#[must_use]
pub fn build_pr_comments_query(with_cursor: bool) -> String {
    if with_cursor {
        "query($owner: String!, $repo: String!, $number: Int!, $first: Int!, $after: String) { repository(owner: $owner, name: $repo) { pullRequest(number: $number) { comments(first: $first, after: $after) { nodes { id databaseId author { login } createdAt lastEditedAt body } pageInfo { hasNextPage endCursor } totalCount } } } }"
    } else {
        "query($owner: String!, $repo: String!, $number: Int!, $first: Int!) { repository(owner: $owner, name: $repo) { pullRequest(number: $number) { comments(first: $first) { nodes { id databaseId author { login } createdAt lastEditedAt body } pageInfo { hasNextPage endCursor } totalCount } } } }"
    }
    .to_owned()
}

/// Build the `gh api graphql` argument vector for the PR search query.
///
/// Mirrors `build_issue_search_args` but with a PR `... on PullRequest`
/// field selection and the PR search-qualifier string. The `search(type:
/// ISSUE, ...)` endpoint narrows to PRs via the `is:pr` qualifier. Real cursor
/// pagination: `after=` is emitted only when a cursor is supplied.
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-007
/// @pseudocode component-002 lines 35-58
#[must_use]
pub fn build_pr_search_args(
    owner: &str,
    name: &str,
    filter: &PrFilter,
    cursor: Option<&str>,
    page_size: u32,
) -> Vec<String> {
    let query = if cursor.is_some() {
        pr_search_query_with_after()
    } else {
        pr_search_query_first_page()
    };
    let mut args = vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={query}"),
        "-F".to_string(),
        format!("searchQuery={}", build_pr_search_query(owner, name, filter)),
        "-F".to_string(),
        format!("first={page_size}"),
    ];
    if let Some(c) = cursor {
        args.push("-F".to_string());
        args.push(format!("after={c}"));
    }
    args
}

/// GraphQL search query WITH the `$after` cursor variable (PR fields inlined).
fn pr_search_query_with_after() -> &'static str {
    "query($searchQuery: String!, $first: Int!, $after: String) { search(type: ISSUE, query: $searchQuery, first: $first, after: $after) { nodes { ... on PullRequest { number title state mergedAt author { login } updatedAt headRefName headRefOid baseRefName isDraft mergeable reviewDecision statusCheckRollup { contexts(first: 100) { nodes { __typename ... on CheckRun { name status conclusion detailsUrl } ... on StatusContext { context state targetUrl } } } } assignees(first: 10) { nodes { login } } labels(first: 20) { nodes { name } } comments { totalCount } body } } pageInfo { hasNextPage endCursor } } }"
}

/// GraphQL search query WITHOUT the `$after` cursor variable (first page).
fn pr_search_query_first_page() -> &'static str {
    "query($searchQuery: String!, $first: Int!) { search(type: ISSUE, query: $searchQuery, first: $first) { nodes { ... on PullRequest { number title state mergedAt author { login } updatedAt headRefName headRefOid baseRefName isDraft mergeable reviewDecision statusCheckRollup { contexts(first: 100) { nodes { __typename ... on CheckRun { name status conclusion detailsUrl } ... on StatusContext { context state targetUrl } } } } assignees(first: 10) { nodes { login } } labels(first: 20) { nodes { name } } comments { totalCount } body } } pageInfo { hasNextPage endCursor } } }"
}

/// Build the GitHub search-qualifier string (incl. `is:pr`) for the PR query.
///
/// Deterministic qualifier order: state, labels, author, assignee, reviewer,
/// draft, review, checks, then the free-text query — so construction is stable
/// and testable. Mirrors `issue_search_query` (parse.rs).
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-007
/// @pseudocode component-002 lines 59-73
#[must_use]
pub fn build_pr_search_query(owner: &str, repo: &str, filter: &PrFilter) -> String {
    let mut terms = vec![format!("repo:{owner}/{repo}"), "is:pr".to_string()];
    if let Some(state) = pr_state_qualifier(filter.state) {
        terms.push(state);
    }
    terms.extend(filter.labels.iter().map(|label| format!("label:{label}")));
    push_non_empty_term(&mut terms, "author:", &filter.author);
    push_non_empty_term(&mut terms, "assignee:", &filter.assignee);
    push_non_empty_term(&mut terms, "review-requested:", &filter.reviewer);
    if let Some(draft) = filter.is_draft {
        terms.push(format!("draft:{draft}"));
    }
    if let Some(review) = review_decision_qualifier(filter.review_decision) {
        terms.push(review);
    }
    if let Some(checks) = checks_status_qualifier(filter.checks_status) {
        terms.push(checks);
    }
    if !filter.query_text.trim().is_empty() {
        terms.push(filter.query_text.trim().to_string());
    }
    terms.join(" ")
}

/// Map `PrFilterState` to its search qualifier (None for All/absent).
fn pr_state_qualifier(state: Option<PrFilterState>) -> Option<String> {
    match state.unwrap_or_default() {
        PrFilterState::Open => Some("is:open".to_string()),
        PrFilterState::Closed => Some("is:closed".to_string()),
        PrFilterState::Merged => Some("is:merged".to_string()),
        PrFilterState::All => None,
    }
}

/// Map `ReviewDecisionFilter` to its `review:` qualifier (None for Any).
fn review_decision_qualifier(filter: ReviewDecisionFilter) -> Option<String> {
    match filter {
        ReviewDecisionFilter::Approved => Some("review:approved".to_string()),
        ReviewDecisionFilter::ChangesRequested => Some("review:changes_requested".to_string()),
        ReviewDecisionFilter::ReviewRequired => Some("review:required".to_string()),
        ReviewDecisionFilter::None => Some("review:none".to_string()),
        ReviewDecisionFilter::Any => None,
    }
}

/// Map `ChecksFilter` to its `status:` qualifier (None for Any).
fn checks_status_qualifier(filter: ChecksFilter) -> Option<String> {
    match filter {
        ChecksFilter::Success => Some("status:success".to_string()),
        ChecksFilter::Failing => Some("status:failure".to_string()),
        ChecksFilter::Pending => Some("status:pending".to_string()),
        ChecksFilter::Any => None,
    }
}

/// Push `prefix + value` onto `terms` only when `value` is non-empty.
fn push_non_empty_term(terms: &mut Vec<String>, prefix: &str, value: &str) {
    if !value.trim().is_empty() {
        terms.push(format!("{prefix}{}", value.trim()));
    }
}

/// Parse JSON output from the GraphQL PR search query into a paginated
/// response.
///
/// Reads `data.search.nodes`, maps each node to a `PullRequest`, and extracts
/// the real `endCursor`/`hasNextPage` via the REUSED `parse_page_info`. Does
/// NOT sort (the caller sorts after). Malformed top-level JSON yields
/// `GhError::ParseError`.
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-006
/// @pseudocode component-002 lines 138-156
pub fn parse_pull_requests_json(json_str: &str) -> Result<PrListResponse, GhError> {
    let value: Value = serde_json::from_str(json_str)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;
    let search = value
        .get("data")
        .and_then(|data| data.get("search"))
        .ok_or_else(|| GhError::ParseError("Missing PR search data".to_string()))?;
    let nodes = search
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| GhError::ParseError("Missing PR search nodes".to_string()))?;
    let page_info = search
        .get("pageInfo")
        .ok_or_else(|| GhError::ParseError("Missing pageInfo".to_string()))?;

    let pull_requests = nodes.iter().map(parse_pr_from_node).collect();
    let (cursor, has_more) = parse_page_info(page_info);

    Ok(PrListResponse {
        pull_requests,
        cursor,
        has_more,
    })
}

/// Map one GraphQL search node to a [`PullRequest`] (never drops; degraded
/// defaults for missing fields).
fn parse_pr_from_node(node: &Value) -> PullRequest {
    let number = node.get("number").and_then(Value::as_u64).unwrap_or(0);
    let title = str_field(node, "title");
    let state = parse_pr_state(
        node.get("state").unwrap_or(&Value::Null),
        node.get("mergedAt").unwrap_or(&Value::Null),
    );
    let author_login = login_field(node, "author");
    let updated_at = str_field(node, "updatedAt");
    let head_ref = str_field(node, "headRefName");
    let head_sha = str_field(node, "headRefOid");
    let base_ref = str_field(node, "baseRefName");
    let is_draft = node
        .get("isDraft")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let review_decision = node.get("reviewDecision").and_then(parse_review_decision);
    let checks_status = parse_checks_rollup(&rollup_nodes(node.get("statusCheckRollup")));
    let mergeable = parse_mergeable_enum(node.get("mergeable"));
    PullRequest {
        number,
        title,
        state,
        author_login,
        updated_at,
        head_ref,
        head_sha,
        base_ref,
        is_draft,
        review_decision,
        checks_status,
        mergeable,
        assignee_summary: join_pr_nodes_field(node, "assignees", "login"),
        labels_summary: join_pr_nodes_field(node, "labels", "name"),
        comment_count: node
            .get("comments")
            .and_then(|c| c.get("totalCount"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
    }
}

/// Normalize a `statusCheckRollup` value into a slice of context nodes,
/// accepting BOTH transports:
/// - `gh pr view --json`: a flat array of CheckRun/StatusContext entries.
/// - `gh api graphql`: a `contexts.nodes` connection.
pub fn rollup_nodes(rollup: Option<&Value>) -> Vec<Value> {
    let Some(value) = rollup else {
        return Vec::new();
    };
    if let Some(arr) = value.as_array() {
        return arr.clone();
    }
    if let Some(nodes) = value
        .get("contexts")
        .and_then(|c| c.get("nodes"))
        .and_then(Value::as_array)
    {
        return nodes.clone();
    }
    Vec::new()
}

/// Read a top-level string field, defaulting to "".
fn str_field(value: &Value, field: &str) -> String {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Parse the GraphQL `mergeable` enum (`MERGEABLE`/`CONFLICTING`/`UNKNOWN`)
/// into a tri-state bool: `MERGEABLE`→`Some(true)`,
/// `CONFLICTING`→`Some(false)`, anything else (incl. `UNKNOWN` and missing)→
/// `None`. Used by the list parser so the PR list can show a mergeable/conflict
/// indicator without a separate detail fetch (issue #314).
#[must_use]
pub fn parse_mergeable_enum(value: Option<&Value>) -> Option<bool> {
    let token = value.and_then(Value::as_str)?;
    match token {
        "MERGEABLE" => Some(true),
        "CONFLICTING" => Some(false),
        _ => None,
    }
}

/// Read `<field>.login` as a string, defaulting to "".
fn login_field(value: &Value, field: &str) -> String {
    value
        .get(field)
        .and_then(|a| a.get("login"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Read `<field>[*].<key>` joined with ", ", accepting both
/// `{nodes:[...]}` and bare-array shapes.
fn join_pr_nodes_field(item: &Value, field: &str, key: &str) -> String {
    let nodes = item.get(field).and_then(|f| {
        if let Some(arr) = f.get("nodes").and_then(Value::as_array) {
            return Some(arr);
        }
        f.as_array()
    });
    nodes
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|n| n.get(key).and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

/// Parse JSON output from `gh pr view --json` into a [`PullRequestDetail`].
///
/// Reads the SINGLE-object `gh pr view` shape (NOT a search node). Comments
/// are initialized as an exhausted empty paginated list; they are sourced
/// separately by `list_pr_comments`. Malformed JSON yields `GhError::ParseError`.
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 157-166
pub fn parse_pull_request_detail_json(
    json_str: &str,
    owner_name: &str,
) -> Result<PullRequestDetail, GhError> {
    let value: Value = serde_json::from_str(json_str)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;
    let number = value
        .get("number")
        .and_then(Value::as_u64)
        .ok_or_else(|| GhError::ParseError("Missing or invalid number".to_string()))?;

    let rollup = rollup_nodes(value.get("statusCheckRollup"));
    let reviews = value
        .get("reviews")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().map(parse_pr_review).collect())
        .unwrap_or_default();
    let checks = rollup.iter().map(parse_pr_check).collect();

    Ok(PullRequestDetail {
        repo_owner_name: owner_name.to_string(),
        number,
        title: str_field(&value, "title"),
        state: parse_pr_state(
            value.get("state").unwrap_or(&Value::Null),
            value.get("mergedAt").unwrap_or(&Value::Null),
        ),
        is_draft: value
            .get("isDraft")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        author_login: login_field(&value, "author"),
        created_at: str_field(&value, "createdAt"),
        updated_at: str_field(&value, "updatedAt"),
        head_ref: str_field(&value, "headRefName"),
        head_sha: str_field(&value, "headRefOid"),
        base_ref: str_field(&value, "baseRefName"),
        labels: pr_string_array(&value, "labels", "name"),
        assignees: pr_string_array(&value, "assignees", "login"),
        milestone: value.get("milestone").and_then(|m| {
            if m.is_null() {
                None
            } else {
                m.get("title").and_then(Value::as_str).map(String::from)
            }
        }),
        body: str_field(&value, "body"),
        external_url: str_field(&value, "url"),
        review_decision: value.get("reviewDecision").and_then(parse_review_decision),
        // Reuse the already-extracted `rollup` nodes (computed once above) for
        // the aggregate status, instead of recomputing `rollup_nodes` a second
        // time — keeping `checks` and `checks_status` over the SAME source so
        // they can never drift apart (MED-4).
        checks_status: parse_checks_rollup(&rollup),
        reviews,
        checks,
        comments: exhausted_comments(Vec::new()),
        mergeable: value.get("mergeable").and_then(Value::as_bool),
        merge_state_status: value
            .get("mergeStateStatus")
            .and_then(Value::as_str)
            .map(String::from),
    })
}

/// Collect `<field>[*].<key>` into `Vec<String>` from a bare-array field.
fn pr_string_array(value: &Value, field: &str, key: &str) -> Vec<String> {
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

/// Parse a single review node into a [`PrReview`].
///
/// TOTAL function — never drops an entry. A malformed/missing field yields a
/// displayable degraded record (`"(unknown reviewer)"`, `Commented`), so
/// counts and rendering still include it (REQ-PR-013).
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 174-180
#[must_use]
pub fn parse_pr_review(node: &Value) -> PrReview {
    let review_id = node
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let author_login = node
        .get("author")
        .and_then(|a| a.get("login"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("(unknown reviewer)")
        .to_string();
    let state = node
        .get("state")
        .and_then(Value::as_str)
        .map_or(PrReviewState::Commented, parse_review_state_token);
    let submitted_at = str_field(node, "submittedAt");
    let body = node.get("body").and_then(Value::as_str).map(String::from);
    PrReview {
        review_id,
        author_login,
        state,
        submitted_at,
        body,
        review_threads: Vec::new(),
    }
}

/// Map a review-state token to a [`PrReviewState`] (default Commented).
fn parse_review_state_token(token: &str) -> PrReviewState {
    match token {
        "APPROVED" => PrReviewState::Approved,
        "CHANGES_REQUESTED" => PrReviewState::ChangesRequested,
        "PENDING" => PrReviewState::Pending,
        "DISMISSED" => PrReviewState::Dismissed,
        "REVIEW_REQUIRED" => PrReviewState::ReviewRequired,
        _ => PrReviewState::Commented,
    }
}

/// Parse a single statusCheckRollup node into a [`PrCheck`].
///
/// TOTAL function — never drops an entry. Handles BOTH `CheckRun`
/// (`name`/`status`/`conclusion`/`detailsUrl`) and `StatusContext`
/// (`context`/`state`/`targetUrl`) shapes, discriminating on `__typename`
/// with field-presence fallback (REQ-PR-013).
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 181-193
#[must_use]
pub fn parse_pr_check(node: &Value) -> PrCheck {
    let name = node
        .get("name")
        .or_else(|| node.get("context"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("(unparseable check)")
        .to_string();
    let raw_status = node
        .get("conclusion")
        .or_else(|| node.get("state"))
        .or_else(|| node.get("status"))
        .and_then(Value::as_str);
    let status = raw_status.map_or(PrCheckStatus::Pending, parse_check_status);
    let conclusion = raw_status.unwrap_or("unknown").to_string();
    let url = node
        .get("detailsUrl")
        .or_else(|| node.get("targetUrl"))
        .and_then(Value::as_str)
        .map(String::from);
    PrCheck {
        name,
        status,
        conclusion,
        url,
    }
}

/// Parse the `reviewDecision` field into an optional [`PrReviewState`].
///
/// `APPROVED`/`CHANGES_REQUESTED`/`REVIEW_REQUIRED` map to their variants;
/// null/empty map to `None`.
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 202-204
#[must_use]
pub fn parse_review_decision(value: &Value) -> Option<PrReviewState> {
    let token = value.as_str()?;
    match token {
        "APPROVED" => Some(PrReviewState::Approved),
        "CHANGES_REQUESTED" => Some(PrReviewState::ChangesRequested),
        "REVIEW_REQUIRED" => Some(PrReviewState::ReviewRequired),
        _ => None,
    }
}

/// Map a raw status/conclusion/state token to a [`PrCheckStatus`] (union of
/// CheckRun-conclusion, CheckRun-status, and StatusContext-state tokens).
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 205-215
#[must_use]
pub fn parse_check_status(raw_status: &str) -> PrCheckStatus {
    match raw_status {
        "SUCCESS" => PrCheckStatus::Success,
        "FAILURE" | "ERROR" | "TIMED_OUT" | "STARTUP_FAILURE" | "ACTION_REQUIRED" | "CANCELLED" => {
            PrCheckStatus::Failure
        }
        "PENDING" | "EXPECTED" | "QUEUED" | "IN_PROGRESS" | "WAITING" | "REQUESTED"
        | "COMPLETED" | "" => PrCheckStatus::Pending,
        _ => PrCheckStatus::Neutral,
    }
}

/// Aggregate the per-node check statuses into a single rollup status.
///
/// Precedence: empty→None; any Failure→Failure; any Pending→Pending; all
/// Success→Success; else Neutral.
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 216-222
#[must_use]
pub fn parse_checks_rollup(nodes: &[Value]) -> PrCheckStatus {
    if nodes.is_empty() {
        return PrCheckStatus::None;
    }
    let mut has_failure = false;
    let mut has_pending = false;
    let mut all_success = true;
    for node in nodes {
        let status = node
            .get("conclusion")
            .or_else(|| node.get("state"))
            .or_else(|| node.get("status"))
            .and_then(Value::as_str)
            .map_or(PrCheckStatus::Pending, parse_check_status);
        match status {
            PrCheckStatus::Failure => has_failure = true,
            PrCheckStatus::Pending => has_pending = true,
            PrCheckStatus::Success => {}
            _ => all_success = false,
        }
    }
    if has_failure {
        PrCheckStatus::Failure
    } else if has_pending {
        PrCheckStatus::Pending
    } else if all_success {
        PrCheckStatus::Success
    } else {
        PrCheckStatus::Neutral
    }
}

/// Map the GraphQL `state` + `mergedAt` fields to a [`PrState`].
///
/// `state == "MERGED"` OR a non-null `mergedAt` → `Merged`; `state ==
/// "CLOSED"` → `Closed`; else `Open`.
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 197-201
#[must_use]
pub fn parse_pr_state(state: &Value, merged_at: &Value) -> PrState {
    if state.as_str() == Some("MERGED") || merged_at_non_empty(merged_at) {
        return PrState::Merged;
    }
    if state.as_str() == Some("CLOSED") {
        return PrState::Closed;
    }
    PrState::Open
}

/// True when `mergedAt` is a non-null, non-empty value.
fn merged_at_non_empty(merged_at: &Value) -> bool {
    merged_at.as_str().is_some_and(|s| !s.is_empty())
}

/// Sort pull requests by `updated_at` descending, then `number` ascending.
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-006
/// @pseudocode component-002 lines 194-196
pub fn sort_pull_requests(items: &mut [PullRequest]) {
    items.sort_by(|a, b| {
        cmp_rfc3339_newest_first(&a.updated_at, &b.updated_at).then(a.number.cmp(&b.number))
    });
}

/// Sort PR reviews newest-first by `submitted_at` (issue #238).
///
/// Review groups keep their attached `review_threads` while the parent
/// ordering flips to most-recent → least-recent. Equal or missing timestamps
/// break ties by `review_id` descending, then `author_login` ascending for a
/// deterministic stable fallback.
pub fn sort_pr_reviews(reviews: &mut [crate::domain::PrReview]) {
    reviews.sort_by(cmp_pr_reviews_newest_first);
}

fn cmp_pr_reviews_newest_first(
    a: &crate::domain::PrReview,
    b: &crate::domain::PrReview,
) -> std::cmp::Ordering {
    cmp_rfc3339_newest_first(&a.submitted_at, &b.submitted_at)
        .then_with(|| b.review_id.cmp(&a.review_id))
        .then_with(|| a.author_login.cmp(&b.author_login))
}

// =============================================================================
// PR review threads (issue #119)
// =============================================================================

/// Build the GraphQL query string for the PR review-threads fetch.
///
/// Selects `repository.pullRequest(number:).reviewThreads(first: N) { nodes
/// { id isResolved isOutdated path line comments(first: 50) { nodes {
/// databaseId author { login } createdAt lastEditedAt body pullRequestReview
/// { id } } } } pageInfo { hasNextPage endCursor } }`. Threads are a direct
/// connection on `PullRequest` (not nested under each `Review`); the
/// `pullRequestReview { id }` on each comment lets the client attach threads
/// to their parent review, and `pageInfo` drives cursor pagination so PRs
/// with more threads than one page lose nothing (issue #155 follow-up).
///
/// The per-thread `comments(first: 50)` cap is an intentional trade-off:
/// threads (not their comments) are the paginated axis, and a single review
/// thread exceeding 50 back-and-forth comments is pathological. Comments
/// past the cap are not fetched.
///
/// `with_after=true` includes the `$after` variable declaration and the
/// `after: $after` argument on the reviewThreads connection;
/// `with_after=false` omits both.
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
#[must_use]
pub fn build_pr_review_threads_query(with_after: bool) -> String {
    // Single source of truth for the thread selection set: the cursor-bearing
    // and first-page variants differ ONLY in the `$after` declaration and the
    // `after:` argument, so they are interpolated into one template (any
    // schema change applies to both branches automatically).
    let (after_decl, after_arg) = if with_after {
        (", $after: String", ", after: $after")
    } else {
        ("", "")
    };
    format!(
        "query($owner: String!, $repo: String!, $number: Int!, $first: Int!{after_decl}) \
         {{ repository(owner: $owner, name: $repo) {{ pullRequest(number: $number) \
         {{ reviewThreads(first: $first{after_arg}) \
         {{ nodes {{ id isResolved isOutdated path line \
         comments(first: 50) {{ nodes {{ databaseId author {{ login }} createdAt lastEditedAt body \
         pullRequestReview {{ id }} }} }} }} \
         pageInfo {{ hasNextPage endCursor }} }} }} }} }}"
    )
}

/// Extract the `reviewThreads.pageInfo` cursor from a thread-page response.
///
/// Returns `Some(end_cursor)` when `hasNextPage` is true and `endCursor` is
/// a non-empty string, `None` otherwise (last page or malformed page info).
///
/// @requirement REQ-PR-009
#[must_use]
pub fn parse_pr_review_threads_cursor(json: &Value) -> Option<String> {
    let page_info = json
        .get("data")?
        .get("repository")?
        .get("pullRequest")?
        .get("reviewThreads")?
        .get("pageInfo")?;
    if !page_info
        .get("hasNextPage")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    let cursor = page_info
        .get("endCursor")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(String::from);
    if cursor.is_none() {
        // hasNextPage=true without a usable endCursor means pagination stops
        // early and threads are silently truncated — surface it in the log so
        // the failure mode is diagnosable (thread fetch itself must not fail).
        tracing::warn!(
            "review-threads pageInfo has hasNextPage=true but no endCursor; \
             stopping pagination early (results may be truncated)"
        );
    }
    cursor
}

/// Parse review threads from a GraphQL response JSON value.
///
/// Navigates `data.repository.pullRequest.reviewThreads.nodes` and maps each
/// thread node to a [`PrReviewThread`]. Also accepts the legacy nested shape
/// `reviews.nodes[*].reviewThreads.nodes` for backward compatibility.
/// Malformed threads yield degraded entries (never dropped — REQ-PR-013).
/// Missing data yields an empty vec (graceful degradation).
///
/// @plan PLAN-20260624-PR-MODE.P08
/// @requirement REQ-PR-009
/// @requirement REQ-PR-013
#[must_use]
pub fn parse_pr_review_threads(json: &Value) -> Vec<PrReviewThread> {
    let pull_request = json
        .get("data")
        .and_then(|d| d.get("repository"))
        .and_then(|r| r.get("pullRequest"));

    let Some(pull_request) = pull_request else {
        return Vec::new();
    };

    // Primary path: pullRequest.reviewThreads.nodes (correct GitHub API).
    if let Some(thread_nodes) = pull_request
        .get("reviewThreads")
        .and_then(|rt| rt.get("nodes"))
        .and_then(Value::as_array)
    {
        return thread_nodes
            .iter()
            .map(parse_single_review_thread)
            .collect();
    }

    // Legacy/fallback path: pullRequest.reviews.nodes[*].reviewThreads.nodes.
    parse_nested_review_threads(pull_request)
}

/// Parse threads nested under `reviews.nodes[*].reviewThreads` (fallback).
fn parse_nested_review_threads(pull_request: &Value) -> Vec<PrReviewThread> {
    let Some(review_nodes) = pull_request
        .get("reviews")
        .and_then(|r| r.get("nodes"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    let mut threads = Vec::new();
    for review_node in review_nodes {
        let Some(thread_nodes) = review_node
            .get("reviewThreads")
            .and_then(|rt| rt.get("nodes"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for thread_node in thread_nodes {
            threads.push(parse_single_review_thread(thread_node));
        }
    }
    threads
}

/// Parse a single review-thread node into a [`PrReviewThread`].
///
/// TOTAL function — a malformed thread node still yields a degraded
/// [`PrReviewThread`] with a placeholder id so it is not dropped.
fn parse_single_review_thread(node: &Value) -> PrReviewThread {
    let thread_id = node
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("(unknown thread)")
        .to_string();
    let is_resolved = node
        .get("isResolved")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_outdated = node
        .get("isOutdated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let review_id = parse_thread_review_id(node);
    let path = node
        .get("path")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let line = node
        .get("line")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok());
    let comments = parse_thread_comments(node);
    PrReviewThread {
        thread_id,
        is_resolved,
        is_outdated,
        review_id,
        path,
        line,
        comments,
    }
}

/// Extract the parent-review id (`comments.nodes[0].pullRequestReview.id`)
/// from a thread node. The FIRST comment of a thread is the one created by
/// the review that opened the thread, so its `pullRequestReview` id is the
/// thread's parent review. `None` when unavailable (degraded thread).
fn parse_thread_review_id(thread_node: &Value) -> Option<String> {
    let review_id = thread_node
        .get("comments")
        .and_then(|c| c.get("nodes"))
        .and_then(Value::as_array)
        .and_then(|nodes| nodes.first())
        .and_then(|first| first.get("pullRequestReview"))
        .and_then(|review| review.get("id"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from);
    if review_id.is_none() {
        // Surface the data-quality gap: an ungroupable thread falls back to
        // the first review in the UI, which is wrong grouping — make that
        // diagnosable instead of silent.
        let thread_id = thread_node
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("(unknown)");
        tracing::warn!(
            thread_id,
            "review thread missing parent review id; grouping will use the fallback review"
        );
    }
    review_id
}

/// Parse the `comments.nodes` array of a thread node into [`IssueComment`]s.
fn parse_thread_comments(thread_node: &Value) -> Vec<IssueComment> {
    let Some(nodes) = thread_node
        .get("comments")
        .and_then(|c| c.get("nodes"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    nodes.iter().map(parse_thread_comment_node).collect()
}

/// Parse a single thread comment node into an [`IssueComment`].
///
/// Mirrors the GraphQL comment node shape: `databaseId`, `author { login }`,
/// `createdAt`, `lastEditedAt`, `body`.
fn parse_thread_comment_node(node: &Value) -> IssueComment {
    let comment_id = node.get("databaseId").and_then(Value::as_u64).unwrap_or(0);
    let author_login = node
        .get("author")
        .and_then(|a| a.get("login"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("(unknown)")
        .to_string();
    let created_at = node
        .get("createdAt")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let edited_at = node
        .get("lastEditedAt")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let body = node
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    IssueComment {
        comment_id,
        author_login,
        created_at,
        edited_at,
        body,
    }
}

/// Parse the `addPullRequestReviewThreadReply` mutation response into an
/// [`IssueComment`]. The GraphQL path is
/// `data.addPullRequestReviewThreadReply.comment`.
///
/// @requirement REQ-PR-009
pub fn parse_thread_reply_json(stdout: &str) -> Result<IssueComment, GhError> {
    let json: Value = serde_json::from_str(stdout)
        .map_err(|e| GhError::ParseError(format!("thread reply JSON: {e}")))?;
    let comment_node = json
        .get("data")
        .and_then(|d| d.get("addPullRequestReviewThreadReply"))
        .and_then(|r| r.get("comment"))
        .ok_or_else(|| GhError::ParseError("missing thread reply comment node".to_string()))?;
    Ok(parse_thread_comment_node(comment_node))
}
