//! Behavioral tests for PR list sort cycling and projection-time re-sort
//! (issue #473).

use crate::domain::{PrCheckStatus, PrSortBy, PrSortConfig, PrState, PullRequest, SortOrder};
use crate::state::AppEvent;
use crate::state::transition::TransitionExt;
use crate::state::types::AppState;

fn pr(number: u64, created_at: &str, updated_at: &str) -> PullRequest {
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

fn state_with_prs(prs: Vec<PullRequest>) -> AppState {
    let mut state = AppState::default();
    state.prs_state.list.replace_items(prs);
    state.prs_state.list.set_selected_index(Some(0));
    state
}

fn sorted_numbers(state: &AppState) -> Vec<u64> {
    state
        .prs_state
        .pull_requests()
        .iter()
        .map(|pr| pr.number)
        .collect()
}

#[test]
fn default_sort_config_is_updated_desc() {
    let config = PrSortConfig::default();
    assert_eq!(config.by, PrSortBy::Updated);
    assert_eq!(config.order, SortOrder::Desc);
}

#[test]
fn cycle_sort_by_next_cycles_through_all_keys() {
    let state = state_with_prs(vec![pr(1, "2026-07-01T00:00:00Z", "2026-07-01T00:00:00Z")]);
    // Default is Updated; cycle_next: Updated → Number
    let state = state.apply(AppEvent::PrCycleSortByNext).committed_pure();
    assert_eq!(state.prs_state.sort_config.by, PrSortBy::Number);

    // Number → Created
    let state = state.apply(AppEvent::PrCycleSortByNext).committed_pure();
    assert_eq!(state.prs_state.sort_config.by, PrSortBy::Created);

    // Created → Updated
    let state = state.apply(AppEvent::PrCycleSortByNext).committed_pure();
    assert_eq!(state.prs_state.sort_config.by, PrSortBy::Updated);
}

#[test]
fn toggle_sort_order_flips_direction() {
    let state = state_with_prs(vec![pr(1, "2026-07-01T00:00:00Z", "2026-07-01T00:00:00Z")]);
    assert_eq!(state.prs_state.sort_config.order, SortOrder::Desc);
    let state = state.apply(AppEvent::PrToggleSortOrder).committed_pure();
    assert_eq!(state.prs_state.sort_config.order, SortOrder::Asc);
    let state = state.apply(AppEvent::PrToggleSortOrder).committed_pure();
    assert_eq!(state.prs_state.sort_config.order, SortOrder::Desc);
}

#[test]
fn number_desc_sort_orders_highest_first() {
    let state = state_with_prs(vec![
        pr(1, "2026-07-01T00:00:00Z", "2026-07-01T00:00:00Z"),
        pr(3, "2026-07-01T00:00:00Z", "2026-07-01T00:00:00Z"),
        pr(2, "2026-07-01T00:00:00Z", "2026-07-01T00:00:00Z"),
    ]);
    // Cycle to Number: Updated → Number (one cycle_next)
    let state = state.apply(AppEvent::PrCycleSortByNext).committed_pure();
    // Default order is Desc, so highest number first
    assert_eq!(sorted_numbers(&state), vec![3, 2, 1]);
}

#[test]
fn created_asc_sort_orders_oldest_first() {
    let state = state_with_prs(vec![
        pr(1, "2026-07-03T00:00:00Z", "2026-07-01T00:00:00Z"),
        pr(2, "2026-07-01T00:00:00Z", "2026-07-02T00:00:00Z"),
        pr(3, "2026-07-02T00:00:00Z", "2026-07-03T00:00:00Z"),
    ]);
    // Cycle to Created: Updated → Number → Created (two cycle_nexts)
    let state = state.apply(AppEvent::PrCycleSortByNext).committed_pure();
    let state = state.apply(AppEvent::PrCycleSortByNext).committed_pure();
    // Toggle to Asc
    let state = state.apply(AppEvent::PrToggleSortOrder).committed_pure();
    assert_eq!(sorted_numbers(&state), vec![2, 3, 1]);
}

#[test]
fn sort_preserves_selection_by_identity() {
    let mut state = state_with_prs(vec![
        pr(1, "2026-07-03T00:00:00Z", "2026-07-01T00:00:00Z"),
        pr(2, "2026-07-01T00:00:00Z", "2026-07-02T00:00:00Z"),
        pr(3, "2026-07-02T00:00:00Z", "2026-07-03T00:00:00Z"),
    ]);
    // Select PR #1 (at index 0, will move to index 2 after Number/Desc sort)
    state.prs_state.list.set_selected_index(Some(0));
    assert_eq!(state.prs_state.selected_pr_index(), Some(0));

    // Cycle to Number (highest first by default): Updated → Number
    let state = state.apply(AppEvent::PrCycleSortByNext).committed_pure();

    // PR #1 should still be selected, now at index 2 (not 0)
    let selected_index = state.prs_state.selected_pr_index();
    assert_eq!(selected_index, Some(2));
    let selected_number = state
        .prs_state
        .selected_pr_index()
        .and_then(|idx| state.prs_state.pull_requests().get(idx))
        .map(|pr| pr.number);
    assert_eq!(selected_number, Some(1));
}
