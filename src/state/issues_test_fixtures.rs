//! Shared `#[cfg(test)]` fixtures for Issues Mode reducer tests.
//!
//! Extracted so the Issues test modules (`issues_tests.rs`,
//! `issues_tests_components.rs`, `issues_tests_detail.rs`,
//! `issues_tests_detail_flow.rs`, `issues_tests_filter.rs`) share ONE copy of
//! the list-reload setup helper instead of drifting copies that must be
//! updated in lockstep when the pagination API changes.

use crate::domain::{IssueFilter, RepositoryId};
use crate::state::AppState;

/// Begin a fresh visible issue-list reload for the given scope/filter and
/// return the allocated request id, so tests can correlate the result event.
///
/// Mirrors what the dispatch layer does (`next_request_id` + `begin_reload`)
/// so a subsequent `IssueListLoaded`/`IssueListLoadFailed` with the returned
/// id is accepted rather than treated as stale.
pub(super) fn begin_issue_list_reload(
    state: &mut AppState,
    scope_repo_id: &str,
    filter: IssueFilter,
) -> u64 {
    let Ok(request_id) = state.issues_state.list.next_request_id() else {
        panic!("request id allocation must succeed in test setup");
    };
    let id = request_id.get();
    state.issues_state.list.begin_reload(
        crate::state::types::IssueListIdentity {
            scope_repo_id: RepositoryId(scope_repo_id.to_string()),
            filter,
        },
        request_id,
    );
    id
}
