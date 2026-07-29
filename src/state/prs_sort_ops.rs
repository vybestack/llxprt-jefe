//! PR list sort projection helpers (issue #473).
//!
//! Extracted from `prs_load_ops.rs` to keep that file under the 850-line
//! architecture boundary limit.

use super::types::PullRequestsState;

/// Re-sort the loaded PR list with the active sort config, preserving
/// selection by PR number (issue #473). Called after every load/append and
/// after a sort config change.
pub fn resort_prs_preserving_selection(prs_state: &mut PullRequestsState) {
    let selected_number = prs_state
        .list
        .selected_index()
        .and_then(|idx| prs_state.list.items().get(idx).map(|pr| pr.number));
    let config = prs_state.sort_config;
    prs_state
        .list
        .sort_by(|a, b| crate::github::compare_pull_requests(a, b, config));
    if let Some(number) = selected_number {
        prs_state.list.set_selected_index(
            prs_state
                .list
                .items()
                .iter()
                .position(|pr| pr.number == number),
        );
    }
}
