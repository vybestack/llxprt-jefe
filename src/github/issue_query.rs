//! Issue GraphQL/CLI query-building helpers (issue #473 extraction).
//!
//! Extracted from `parse.rs` to keep that file within the source-size policy.
//! These are pure functions over `crate::domain::IssueFilter` that produce
//! `gh` CLI argument vectors.

use crate::domain::{FILTER_CHOICE_ANY, FILTER_CHOICE_NONE, IssueFilter, IssueFilterState};

/// HTTP header enabling the `issueFieldValues` schema preview (issue #473).
///
/// Priority is an organization-level issue field exposed only when this header
/// is present on the GraphQL request. `gh api graphql` passes it through as
/// `-H "GraphQL-Features: issue_fields"`.
const ISSUE_FIELDS_HEADER: &str = "GraphQL-Features: issue_fields";

/// Shared GraphQL selection for issue list nodes (issue #473).
///
/// Both the `repository.issues` and `search` queries select identical fields
/// so parse_issue_from_item handles both uniformly. Includes `createdAt`
/// (needed for the created-date sort) and `issueFieldValues` (needed for the
/// priority sort) in addition to the pre-existing fields.
fn issue_list_node_selection() -> &'static str {
    "id number title state stateReason author { login } createdAt updatedAt \
     assignees(first: 10) { nodes { login } } \
     labels(first: 20) { nodes { name } } \
     issueType { name } milestone { title } comments { totalCount } \
     issueFieldValues(first: 10) { nodes { __typename \
       ... on IssueFieldSingleSelectValue { name \
         field { ... on IssueFieldSingleSelect { name } } } } }"
}

/// Build the `gh issue list` CLI argument vector for the given repository and
/// filter.
///
/// This legacy CLI path cannot filter by GitHub Issue Type: GitHub search's
/// `type:` qualifier means issue vs. pull-request, and `gh issue list --json`
/// does not expose `issueType`. Callers that need Issue Type filtering or
/// metadata should use the GraphQL list/search path.
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

    if let Some(state) = &filter.state {
        let state_arg = match state {
            IssueFilterState::Open => "open",
            IssueFilterState::Closed => "closed",
            IssueFilterState::All => "all",
        };
        args.push("--state".to_string());
        args.push(state_arg.to_string());
    }

    for label in &filter.labels {
        args.push("--label".to_string());
        args.push(label.clone());
    }

    let assignee = filter.assignee.trim();
    if !assignee.is_empty()
        && !assignee.eq_ignore_ascii_case(FILTER_CHOICE_ANY)
        && !assignee.eq_ignore_ascii_case(FILTER_CHOICE_NONE)
    {
        args.push("--assignee".to_string());
        args.push(assignee.to_string());
    }

    if let Some(author) = non_any_filter_value(&filter.author) {
        args.push("--author".to_string());
        args.push(author.to_string());
    }

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

/// Build the GraphQL search query argument vector for the given repository and
/// filter (issue #473).
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

    let selection = issue_list_node_selection();
    let query = if cursor.is_some() {
        format!(
            "query($searchQuery: String!, $first: Int!, $after: String) {{ search(type: ISSUE, query: $searchQuery, first: $first, after: $after) {{ nodes {{ ... on Issue {{ {selection} }} }} pageInfo {{ hasNextPage endCursor }} }} }}"
        )
    } else {
        format!(
            "query($searchQuery: String!, $first: Int!) {{ search(type: ISSUE, query: $searchQuery, first: $first) {{ nodes {{ ... on Issue {{ {selection} }} }} pageInfo {{ hasNextPage endCursor }} }} }}"
        )
    };
    let mut args = vec![
        "api".to_string(),
        "graphql".to_string(),
        "-H".to_string(),
        ISSUE_FIELDS_HEADER.to_string(),
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
    super::parse::module_label_value(label).is_some_and(|value| value.eq_ignore_ascii_case(module))
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
        "query({}) {{ repository(owner: $owner, name: $repo) {{ issues(first: $first{after_arg}, filterBy: {{ {} }}, orderBy: {{ field: UPDATED_AT, direction: DESC }}) {{ nodes {{ {} }} pageInfo {{ hasNextPage endCursor }} }} }} }}",
        variable_defs.join(", "),
        filters.join(", "),
        issue_list_node_selection(),
    );

    args.splice(2..2, ["-f".to_string(), format!("query={query}")]);
    // Priority lives under `issueFieldValues`, which requires the issue_fields
    // preview header (issue #473). Without it the server returns a schema error.
    args.splice(2..2, ["-H".to_string(), ISSUE_FIELDS_HEADER.to_string()]);
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
