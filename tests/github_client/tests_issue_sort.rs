//! Behavioral coverage for user-selectable issue sorting (issue #473).
//!
//! Tests the `compare_issues` comparator across all sort-by × sort-order
//! combinations, including priority ranking and tie-breaking behavior.

use jefe::domain::{Issue, IssueSortBy, IssueSortConfig, IssueState, SortOrder};
use jefe::github::{compare_issues, issue_priority_rank};

fn issue(number: u64, created_at: &str, updated_at: &str, priority: Option<&str>) -> Issue {
    Issue {
        number,
        node_id: String::new(),
        title: format!("issue {number}"),
        state: IssueState::Open,
        author_login: String::new(),
        created_at: created_at.to_string(),
        updated_at: updated_at.to_string(),
        assignee_summary: String::new(),
        labels_summary: String::new(),
        assignees: Vec::new(),
        labels: Vec::new(),
        issue_type: String::new(),
        milestone: String::new(),
        module: String::new(),
        comment_count: 0,
        body: String::new(),
        priority: priority.map(str::to_string),
        state_reason: None,
    }
}

fn sorted_numbers(mut issues: Vec<Issue>, config: IssueSortConfig) -> Vec<u64> {
    issues.sort_by(|a, b| compare_issues(a, b, config));
    issues.iter().map(|i| i.number).collect()
}

// ── Number sort ─────────────────────────────────────────────────────────────

#[test]
fn number_desc_sorts_highest_first() {
    let issues = vec![
        issue(3, "2026-01-01T00:00:00Z", "2026-03-01T00:00:00Z", None),
        issue(1, "2026-01-01T00:00:00Z", "2026-03-01T00:00:00Z", None),
        issue(2, "2026-01-01T00:00:00Z", "2026-03-01T00:00:00Z", None),
    ];
    let config = IssueSortConfig {
        by: IssueSortBy::Number,
        order: SortOrder::Desc,
    };
    assert_eq!(sorted_numbers(issues, config), vec![3, 2, 1]);
}

#[test]
fn number_asc_sorts_lowest_first() {
    let issues = vec![
        issue(3, "2026-01-01T00:00:00Z", "2026-03-01T00:00:00Z", None),
        issue(1, "2026-01-01T00:00:00Z", "2026-03-01T00:00:00Z", None),
        issue(2, "2026-01-01T00:00:00Z", "2026-03-01T00:00:00Z", None),
    ];
    let config = IssueSortConfig {
        by: IssueSortBy::Number,
        order: SortOrder::Asc,
    };
    assert_eq!(sorted_numbers(issues, config), vec![1, 2, 3]);
}

// ── Created sort ────────────────────────────────────────────────────────────

#[test]
fn created_desc_sorts_newest_first() {
    let issues = vec![
        issue(1, "2026-01-01T00:00:00Z", "2026-03-01T00:00:00Z", None),
        issue(2, "2026-02-01T00:00:00Z", "2026-03-01T00:00:00Z", None),
        issue(3, "2026-03-01T00:00:00Z", "2026-03-01T00:00:00Z", None),
    ];
    let config = IssueSortConfig {
        by: IssueSortBy::Created,
        order: SortOrder::Desc,
    };
    assert_eq!(sorted_numbers(issues, config), vec![3, 2, 1]);
}

#[test]
fn created_asc_sorts_oldest_first() {
    let issues = vec![
        issue(3, "2026-03-01T00:00:00Z", "2026-03-01T00:00:00Z", None),
        issue(1, "2026-01-01T00:00:00Z", "2026-03-01T00:00:00Z", None),
        issue(2, "2026-02-01T00:00:00Z", "2026-03-01T00:00:00Z", None),
    ];
    let config = IssueSortConfig {
        by: IssueSortBy::Created,
        order: SortOrder::Asc,
    };
    assert_eq!(sorted_numbers(issues, config), vec![1, 2, 3]);
}

// ── Updated sort (default) ──────────────────────────────────────────────────

#[test]
fn updated_desc_sorts_most_recently_updated_first() {
    let issues = vec![
        issue(1, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", None),
        issue(2, "2026-01-01T00:00:00Z", "2026-03-01T00:00:00Z", None),
        issue(3, "2026-01-01T00:00:00Z", "2026-02-01T00:00:00Z", None),
    ];
    let config = IssueSortConfig {
        by: IssueSortBy::Updated,
        order: SortOrder::Desc,
    };
    assert_eq!(sorted_numbers(issues, config), vec![2, 3, 1]);
}

#[test]
fn updated_asc_sorts_least_recently_updated_first() {
    let issues = vec![
        issue(1, "2026-01-01T00:00:00Z", "2026-02-01T00:00:00Z", None),
        issue(2, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", None),
        issue(3, "2026-01-01T00:00:00Z", "2026-03-01T00:00:00Z", None),
    ];
    let config = IssueSortConfig {
        by: IssueSortBy::Updated,
        order: SortOrder::Asc,
    };
    assert_eq!(sorted_numbers(issues, config), vec![2, 1, 3]);
}

#[test]
fn updated_desc_with_equal_timestamps_breaks_tie_by_number_asc() {
    let issues = vec![
        issue(3, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", None),
        issue(1, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", None),
        issue(2, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", None),
    ];
    let config = IssueSortConfig {
        by: IssueSortBy::Updated,
        order: SortOrder::Desc,
    };
    assert_eq!(sorted_numbers(issues, config), vec![1, 2, 3]);
}

// ── Priority sort ───────────────────────────────────────────────────────────

#[test]
fn priority_desc_sorts_highest_priority_first() {
    let issues = vec![
        issue(
            1,
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z",
            Some("Low"),
        ),
        issue(
            2,
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z",
            Some("Critical"),
        ),
        issue(
            3,
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z",
            Some("Medium"),
        ),
        issue(
            4,
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z",
            Some("High"),
        ),
    ];
    let config = IssueSortConfig {
        by: IssueSortBy::Priority,
        order: SortOrder::Desc,
    };
    assert_eq!(sorted_numbers(issues, config), vec![2, 4, 3, 1]);
}

#[test]
fn priority_asc_sorts_lowest_priority_first() {
    let issues = vec![
        issue(
            1,
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z",
            Some("Critical"),
        ),
        issue(
            2,
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z",
            Some("Low"),
        ),
        issue(
            3,
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z",
            Some("Medium"),
        ),
        issue(
            4,
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z",
            Some("High"),
        ),
    ];
    let config = IssueSortConfig {
        by: IssueSortBy::Priority,
        order: SortOrder::Asc,
    };
    assert_eq!(sorted_numbers(issues, config), vec![2, 3, 4, 1]);
}

#[test]
fn priority_desc_with_none_sorts_missing_last() {
    let issues = vec![
        issue(1, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", None),
        issue(
            2,
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z",
            Some("High"),
        ),
        issue(3, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", None),
    ];
    let config = IssueSortConfig {
        by: IssueSortBy::Priority,
        order: SortOrder::Desc,
    };
    // Some always beats None regardless of direction. Issues with priority
    // come first (ordered by number asc within the tie), then None issues.
    assert_eq!(sorted_numbers(issues, config), vec![2, 1, 3]);
}

#[test]
fn priority_asc_with_none_still_sorts_missing_last() {
    let issues = vec![
        issue(
            1,
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z",
            Some("Low"),
        ),
        issue(2, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", None),
        issue(
            3,
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z",
            Some("High"),
        ),
    ];
    let config = IssueSortConfig {
        by: IssueSortBy::Priority,
        order: SortOrder::Asc,
    };
    // Ascending: Low(1) < High(3), then None(2) last.
    assert_eq!(sorted_numbers(issues, config), vec![1, 3, 2]);
}

#[test]
fn priority_desc_equal_rank_breaks_tie_by_number_asc() {
    let issues = vec![
        issue(
            3,
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z",
            Some("High"),
        ),
        issue(
            1,
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z",
            Some("High"),
        ),
        issue(
            2,
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z",
            Some("High"),
        ),
    ];
    let config = IssueSortConfig {
        by: IssueSortBy::Priority,
        order: SortOrder::Desc,
    };
    assert_eq!(sorted_numbers(issues, config), vec![1, 2, 3]);
}

// ── Priority rank mapping ───────────────────────────────────────────────────

#[test]
fn priority_rank_critical_is_highest() {
    assert_eq!(issue_priority_rank("Critical"), 4);
}

#[test]
fn priority_rank_high() {
    assert_eq!(issue_priority_rank("High"), 3);
}

#[test]
fn priority_rank_medium_and_med_alias() {
    assert_eq!(issue_priority_rank("Medium"), 2);
    assert_eq!(issue_priority_rank("Med"), 2);
}

#[test]
fn priority_rank_low() {
    assert_eq!(issue_priority_rank("Low"), 1);
}

#[test]
fn priority_rank_unknown_is_zero() {
    assert_eq!(issue_priority_rank("Urgent"), 0);
    assert_eq!(issue_priority_rank(""), 0);
}

#[test]
fn priority_rank_is_case_insensitive() {
    assert_eq!(issue_priority_rank("CRITICAL"), 4);
    assert_eq!(issue_priority_rank("high"), 3);
    assert_eq!(issue_priority_rank("  medium  "), 2);
}

// ── Priority parsing from GraphQL JSON ──────────────────────────────────────

#[test]
fn parse_issue_extracts_priority_from_issue_field_values() {
    use jefe::github::parse_issues_json;
    let json = r#"[{
        "number": 1,
        "id": "I_123",
        "title": "Test issue",
        "state": "OPEN",
        "author": {"login": "octocat"},
        "createdAt": "2026-01-01T00:00:00Z",
        "updatedAt": "2026-01-02T00:00:00Z",
        "assignees": {"nodes": []},
        "labels": {"nodes": []},
        "issueType": null,
        "milestone": null,
        "comments": {"totalCount": 0},
        "issueFieldValues": {
            "nodes": [
                {
                    "__typename": "IssueFieldSingleSelectValue",
                    "name": "High",
                    "field": {"name": "Priority"}
                }
            ]
        }
    }]"#;
    let issues = parse_issues_json(json).unwrap_or_else(|_| Vec::new());
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].priority.as_deref(), Some("High"));
}

#[test]
fn parse_issue_returns_none_priority_when_not_set() {
    use jefe::github::parse_issues_json;
    let json = r#"[{
        "number": 1,
        "id": "I_123",
        "title": "Test issue",
        "state": "OPEN",
        "author": {"login": "octocat"},
        "createdAt": "2026-01-01T00:00:00Z",
        "updatedAt": "2026-01-02T00:00:00Z",
        "assignees": {"nodes": []},
        "labels": {"nodes": []},
        "issueType": null,
        "milestone": null,
        "comments": {"totalCount": 0},
        "issueFieldValues": {"nodes": []}
    }]"#;
    let issues = parse_issues_json(json).unwrap_or_else(|_| Vec::new());
    assert_eq!(issues[0].priority, None);
}

#[test]
fn parse_issue_returns_none_priority_when_field_absent() {
    use jefe::github::parse_issues_json;
    let json = r#"[{
        "number": 1,
        "id": "I_123",
        "title": "Test issue",
        "state": "OPEN",
        "author": {"login": "octocat"},
        "createdAt": "2026-01-01T00:00:00Z",
        "updatedAt": "2026-01-02T00:00:00Z",
        "assignees": {"nodes": []},
        "labels": {"nodes": []},
        "issueType": null,
        "milestone": null,
        "comments": {"totalCount": 0}
    }]"#;
    let issues = parse_issues_json(json).unwrap_or_else(|_| Vec::new());
    assert_eq!(issues[0].priority, None);
}

#[test]
fn parse_issue_ignores_non_priority_field_values() {
    use jefe::github::parse_issues_json;
    let json = r#"[{
        "number": 1,
        "id": "I_123",
        "title": "Test issue",
        "state": "OPEN",
        "author": {"login": "octocat"},
        "createdAt": "2026-01-01T00:00:00Z",
        "updatedAt": "2026-01-02T00:00:00Z",
        "assignees": {"nodes": []},
        "labels": {"nodes": []},
        "issueType": null,
        "milestone": null,
        "comments": {"totalCount": 0},
        "issueFieldValues": {
            "nodes": [
                {
                    "__typename": "IssueFieldSingleSelectValue",
                    "name": "Backend",
                    "field": {"name": "Module"}
                }
            ]
        }
    }]"#;
    let issues = parse_issues_json(json).unwrap_or_else(|_| Vec::new());
    assert_eq!(issues[0].priority, None);
}

#[test]
fn parse_issue_extracts_created_at() {
    use jefe::github::parse_issues_json;
    let json = r#"[{
        "number": 1,
        "id": "I_123",
        "title": "Test issue",
        "state": "OPEN",
        "author": {"login": "octocat"},
        "createdAt": "2026-06-15T12:30:00Z",
        "updatedAt": "2026-07-02T00:00:00Z",
        "assignees": {"nodes": []},
        "labels": {"nodes": []},
        "issueType": null,
        "milestone": null,
        "comments": {"totalCount": 0}
    }]"#;
    let issues = parse_issues_json(json).unwrap_or_else(|_| Vec::new());
    assert_eq!(issues[0].created_at, "2026-06-15T12:30:00Z");
}
