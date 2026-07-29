//! Behavioral coverage for user-selectable PR sorting (issue #473).
//!
//! Tests the `compare_pull_requests` comparator across all sort-by ×
//! sort-order combinations and tie-breaking behavior.

use jefe::domain::{PrCheckStatus, PrSortBy, PrSortConfig, PrState, PullRequest, SortOrder};
use jefe::github::compare_pull_requests;

fn pull_request(number: u64, created_at: &str, updated_at: &str) -> PullRequest {
    PullRequest {
        number,
        title: format!("pr {number}"),
        state: PrState::Open,
        author_login: String::new(),
        created_at: created_at.to_string(),
        updated_at: updated_at.to_string(),
        head_ref: String::new(),
        head_sha: String::new(),
        base_ref: String::new(),
        is_draft: false,
        review_decision: None,
        checks_status: PrCheckStatus::None,
        assignee_summary: String::new(),
        labels_summary: String::new(),
        comment_count: 0,
        mergeable: None,
    }
}

fn sorted_numbers(mut prs: Vec<PullRequest>, config: PrSortConfig) -> Vec<u64> {
    prs.sort_by(|a, b| compare_pull_requests(a, b, config));
    prs.iter().map(|pr| pr.number).collect()
}

// ── Number sort ─────────────────────────────────────────────────────────────

#[test]
fn number_desc_sorts_highest_first() {
    let prs = vec![
        pull_request(3, "2026-07-01T00:00:00Z", "2026-07-03T00:00:00Z"),
        pull_request(1, "2026-07-01T00:00:00Z", "2026-07-01T00:00:00Z"),
        pull_request(2, "2026-07-01T00:00:00Z", "2026-07-02T00:00:00Z"),
    ];
    let config = PrSortConfig {
        by: PrSortBy::Number,
        order: SortOrder::Desc,
    };
    assert_eq!(sorted_numbers(prs, config), vec![3, 2, 1]);
}

#[test]
fn number_asc_sorts_lowest_first() {
    let prs = vec![
        pull_request(3, "2026-07-01T00:00:00Z", "2026-07-03T00:00:00Z"),
        pull_request(1, "2026-07-01T00:00:00Z", "2026-07-01T00:00:00Z"),
        pull_request(2, "2026-07-01T00:00:00Z", "2026-07-02T00:00:00Z"),
    ];
    let config = PrSortConfig {
        by: PrSortBy::Number,
        order: SortOrder::Asc,
    };
    assert_eq!(sorted_numbers(prs, config), vec![1, 2, 3]);
}

// ── Created sort ────────────────────────────────────────────────────────────

#[test]
fn created_desc_sorts_newest_first() {
    let prs = vec![
        pull_request(1, "2026-07-01T00:00:00Z", "2026-07-05T00:00:00Z"),
        pull_request(2, "2026-07-03T00:00:00Z", "2026-07-02T00:00:00Z"),
        pull_request(3, "2026-07-02T00:00:00Z", "2026-07-04T00:00:00Z"),
    ];
    let config = PrSortConfig {
        by: PrSortBy::Created,
        order: SortOrder::Desc,
    };
    assert_eq!(sorted_numbers(prs, config), vec![2, 3, 1]);
}

#[test]
fn created_asc_sorts_oldest_first() {
    let prs = vec![
        pull_request(1, "2026-07-01T00:00:00Z", "2026-07-05T00:00:00Z"),
        pull_request(2, "2026-07-03T00:00:00Z", "2026-07-02T00:00:00Z"),
        pull_request(3, "2026-07-02T00:00:00Z", "2026-07-04T00:00:00Z"),
    ];
    let config = PrSortConfig {
        by: PrSortBy::Created,
        order: SortOrder::Asc,
    };
    assert_eq!(sorted_numbers(prs, config), vec![1, 3, 2]);
}

#[test]
fn created_desc_breaks_ties_by_number_ascending() {
    // All PRs share the same created_at; distinct updated_at values ensure
    // that a wrong tiebreaker (e.g. updated_desc) would give a different result.
    let prs = vec![
        pull_request(3, "2026-07-01T00:00:00Z", "2026-07-05T00:00:00Z"),
        pull_request(1, "2026-07-01T00:00:00Z", "2026-07-02T00:00:00Z"),
        pull_request(2, "2026-07-01T00:00:00Z", "2026-07-04T00:00:00Z"),
    ];
    let config = PrSortConfig {
        by: PrSortBy::Created,
        order: SortOrder::Desc,
    };
    assert_eq!(sorted_numbers(prs, config), vec![1, 2, 3]);
}

// ── Updated sort ────────────────────────────────────────────────────────────

#[test]
fn updated_desc_sorts_newest_first() {
    let prs = vec![
        pull_request(1, "2026-07-01T00:00:00Z", "2026-07-01T00:00:00Z"),
        pull_request(2, "2026-07-01T00:00:00Z", "2026-07-03T00:00:00Z"),
        pull_request(3, "2026-07-01T00:00:00Z", "2026-07-02T00:00:00Z"),
    ];
    let config = PrSortConfig {
        by: PrSortBy::Updated,
        order: SortOrder::Desc,
    };
    assert_eq!(sorted_numbers(prs, config), vec![2, 3, 1]);
}

#[test]
fn updated_asc_sorts_oldest_first() {
    let prs = vec![
        pull_request(1, "2026-07-01T00:00:00Z", "2026-07-01T00:00:00Z"),
        pull_request(2, "2026-07-01T00:00:00Z", "2026-07-03T00:00:00Z"),
        pull_request(3, "2026-07-01T00:00:00Z", "2026-07-02T00:00:00Z"),
    ];
    let config = PrSortConfig {
        by: PrSortBy::Updated,
        order: SortOrder::Asc,
    };
    assert_eq!(sorted_numbers(prs, config), vec![1, 3, 2]);
}

#[test]
fn updated_desc_breaks_ties_by_number_ascending() {
    // All PRs share the same updated_at; distinct created_at values ensure
    // that a wrong tiebreaker (e.g. created_desc) would give a different result.
    let prs = vec![
        pull_request(3, "2026-07-05T00:00:00Z", "2026-07-01T00:00:00Z"),
        pull_request(1, "2026-07-02T00:00:00Z", "2026-07-01T00:00:00Z"),
        pull_request(2, "2026-07-04T00:00:00Z", "2026-07-01T00:00:00Z"),
    ];
    let config = PrSortConfig {
        by: PrSortBy::Updated,
        order: SortOrder::Desc,
    };
    assert_eq!(sorted_numbers(prs, config), vec![1, 2, 3]);
}

// ── Default sort ────────────────────────────────────────────────────────────

#[test]
fn default_sort_is_updated_desc() {
    let config = PrSortConfig::default();
    assert_eq!(config.by, PrSortBy::Updated);
    assert_eq!(config.order, SortOrder::Desc);
}

// ── Cycle ───────────────────────────────────────────────────────────────────

#[test]
fn cycle_next_wraps_around() {
    // PrSortBy cycle_next order: Number → Created → Updated → Number
    assert_eq!(PrSortBy::Number.cycle_next(), PrSortBy::Created);
    assert_eq!(PrSortBy::Created.cycle_next(), PrSortBy::Updated);
    assert_eq!(PrSortBy::Updated.cycle_next(), PrSortBy::Number);
}

#[test]
fn cycle_prev_wraps_around() {
    assert_eq!(PrSortBy::Number.cycle_prev(), PrSortBy::Updated);
    assert_eq!(PrSortBy::Created.cycle_prev(), PrSortBy::Number);
    assert_eq!(PrSortBy::Updated.cycle_prev(), PrSortBy::Created);
}

#[test]
fn labels_are_human_readable() {
    assert_eq!(PrSortBy::Number.label(), "number");
    assert_eq!(PrSortBy::Created.label(), "created");
    assert_eq!(PrSortBy::Updated.label(), "updated");
}

#[test]
fn sort_order_toggle() {
    assert_eq!(SortOrder::Asc.toggle(), SortOrder::Desc);
    assert_eq!(SortOrder::Desc.toggle(), SortOrder::Asc);
}

#[test]
fn sort_order_labels() {
    assert_eq!(SortOrder::Asc.label(), "asc");
    assert_eq!(SortOrder::Desc.label(), "desc");
}
