//! Behavioral tests for projection-time issue re-sorting (issue #473).
//!
//! Tests that the sort config drives the list order after a sort change, that
//! cycling sort config re-orders the list instantly, and that selection is
//! preserved by identity across re-sort.

use crate::domain::{Issue, IssueSortBy, IssueSortConfig, IssueState, SortOrder};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::transition::TransitionExt;
use crate::state::types::ScreenMode;

fn make_issue(number: u64, created_at: &str, updated_at: &str) -> Issue {
    Issue {
        number,
        node_id: String::new(),
        title: format!("Issue {number}"),
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
        priority: None,
        state_reason: None,
    }
}

fn issues_state_with_issues(issues: Vec<Issue>) -> AppState {
    let mut state = AppState {
        screen_mode: ScreenMode::DashboardIssues,
        ..AppState::default()
    };
    state.issues_state.active = true;
    state.issues_state.list.replace_items(issues);
    state.issues_state.list.set_selected_index(Some(0));
    state
}

fn selected_numbers(state: &AppState) -> Vec<u64> {
    state
        .issues_state
        .issues()
        .iter()
        .map(|i| i.number)
        .collect()
}

#[test]
fn default_sort_config_is_updated_desc() {
    let config = IssueSortConfig::default();
    assert_eq!(config.by, IssueSortBy::Updated);
    assert_eq!(config.order, SortOrder::Desc);
}

#[test]
fn cycle_sort_by_next_cycles_through_all_keys() {
    // Issues inserted in number order; re-sort only applies after a sort event.
    let state = issues_state_with_issues(vec![
        make_issue(1, "2026-01-01T00:00:00Z", "2026-03-01T00:00:00Z"),
        make_issue(2, "2026-02-01T00:00:00Z", "2026-01-01T00:00:00Z"),
        make_issue(3, "2026-03-01T00:00:00Z", "2026-02-01T00:00:00Z"),
    ]);

    // Cycle from Updated → Priority: Desc → no priorities, tie-break by number asc.
    let state = state.apply(AppEvent::CycleIssueSortByNext).committed_pure();
    assert_eq!(state.issues_state.sort_config.by, IssueSortBy::Priority);
    assert_eq!(selected_numbers(&state), vec![1, 2, 3]);

    // Cycle to Number: Desc → highest number first.
    let state = state.apply(AppEvent::CycleIssueSortByNext).committed_pure();
    assert_eq!(state.issues_state.sort_config.by, IssueSortBy::Number);
    assert_eq!(selected_numbers(&state), vec![3, 2, 1]);

    // Cycle to Created: Desc → issue 3 (created 2026-03) first.
    let state = state.apply(AppEvent::CycleIssueSortByNext).committed_pure();
    assert_eq!(state.issues_state.sort_config.by, IssueSortBy::Created);
    assert_eq!(selected_numbers(&state), vec![3, 2, 1]);

    // Cycle wraps back to Updated.
    let state = state.apply(AppEvent::CycleIssueSortByNext).committed_pure();
    assert_eq!(state.issues_state.sort_config.by, IssueSortBy::Updated);
}

#[test]
fn toggle_sort_order_flips_direction() {
    // Start in Updated/Desc. Issues: #1 updated 2026-03, #2 updated 2026-01.
    let state = issues_state_with_issues(vec![
        make_issue(1, "2026-01-01T00:00:00Z", "2026-03-01T00:00:00Z"),
        make_issue(2, "2026-02-01T00:00:00Z", "2026-01-01T00:00:00Z"),
    ]);

    // Toggle to Updated/Asc → issue 2 (updated earliest) first.
    let state = state.apply(AppEvent::ToggleIssueSortOrder).committed_pure();
    assert_eq!(state.issues_state.sort_config.order, SortOrder::Asc);
    assert_eq!(selected_numbers(&state), vec![2, 1]);
}

#[test]
fn sort_preserves_selection_by_identity() {
    let mut state = issues_state_with_issues(vec![
        make_issue(10, "2026-01-01T00:00:00Z", "2026-03-01T00:00:00Z"),
        make_issue(20, "2026-02-01T00:00:00Z", "2026-02-01T00:00:00Z"),
        make_issue(30, "2026-03-01T00:00:00Z", "2026-01-01T00:00:00Z"),
    ]);
    // Select issue #20 (middle row: 10, 20, 30).
    state.issues_state.list.set_selected_index(Some(1));

    // Cycle sort from Updated/Desc to Priority/Desc; #20 stays mid (no priority).
    let state = state.apply(AppEvent::CycleIssueSortByNext).committed_pure();

    // Selection should still be on issue #20.
    let selected_number = state
        .issues_state
        .selected_issue_index()
        .and_then(|idx| state.issues_state.issues().get(idx))
        .map(|i| i.number);
    assert_eq!(selected_number, Some(20));
}

#[test]
fn number_desc_sort_orders_highest_first() {
    let mut state = issues_state_with_issues(vec![
        make_issue(5, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
        make_issue(15, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
        make_issue(10, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
    ]);
    // Set Number/Asc, then toggle to Desc to verify highest issue number first.
    state.issues_state.sort_config = IssueSortConfig {
        by: IssueSortBy::Number,
        order: SortOrder::Asc,
    };
    let state = state.apply(AppEvent::ToggleIssueSortOrder).committed_pure();
    // Now Number/Desc → highest first.
    assert_eq!(selected_numbers(&state), vec![15, 10, 5]);
}

#[test]
fn created_asc_sort_orders_oldest_first() {
    let mut state = issues_state_with_issues(vec![
        make_issue(1, "2026-03-01T00:00:00Z", "2026-01-01T00:00:00Z"),
        make_issue(2, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
        make_issue(3, "2026-02-01T00:00:00Z", "2026-01-01T00:00:00Z"),
    ]);
    // Set sort to Created/Desc first, then toggle to Asc.
    state.issues_state.sort_config = IssueSortConfig {
        by: IssueSortBy::Created,
        order: SortOrder::Desc,
    };
    let state = state.apply(AppEvent::ToggleIssueSortOrder).committed_pure();
    // Now Created/Asc → oldest first.
    assert_eq!(selected_numbers(&state), vec![2, 3, 1]);
}
