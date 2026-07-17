//! Pull Requests Mode state tests — enter/exit, default filter, clear filter,
//! persistence backward-compat, empty-list.
//!
//! @plan PLAN-20260624-PR-MODE.P04
//! @requirement REQ-PR-001
//! @requirement REQ-PR-005
//! @requirement REQ-PR-008
//! @requirement REQ-PR-014
//! @requirement REQ-PR-NFR-002

use crate::domain::{
    PrCheckStatus, PrFilter, PrFilterState, PrState, PullRequest, Repository, RepositoryId,
};
use crate::persistence::State as PersistedState;
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::types::{PaneFocus, PrFocus, PriorAgentFocus, PullRequestsState, ScreenMode};

use super::prs_test_fixtures::begin_pr_list_reload;

/// Helper: a Dashboard AppState with two repositories selected at index 0.
fn dashboard_state() -> AppState {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/repo1"),
    ));
    state.repositories.push(Repository::new(
        RepositoryId("repo-2".to_string()),
        "Repo 2".to_string(),
        "repo-2".to_string(),
        std::path::PathBuf::from("/tmp/repo2"),
    ));
    state.selected_repository_index = Some(0);
    state
}

trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

/// EnterPrsMode sets screen_mode=DashboardPullRequests, active=true, and pr_focus=PrList.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-001
/// @pseudocode component-001 lines 66-71
#[test]
fn test_enter_prs_mode_sets_active_and_saves_prior_focus() {
    let state = AppState {
        pane_focus: PaneFocus::Agents,
        selected_agent_index: Some(2),
        selected_repository_index: Some(1),
        ..dashboard_state()
    };
    let new_state = state.apply(AppEvent::EnterPrsMode);

    assert_eq!(new_state.screen_mode, ScreenMode::DashboardPullRequests);
    assert!(new_state.prs_state.active);
    assert_eq!(new_state.prs_state.pr_focus, PrFocus::PrList);

    // prior_agent_focus must be saved for restoration on exit.
    let saved = new_state
        .prs_state
        .prior_agent_focus
        .clone()
        .unwrap_or_else(|| panic!("prior_agent_focus should be Some"));
    assert_eq!(saved.pane_focus, PaneFocus::Agents);
    assert_eq!(saved.selected_agent_index, Some(2));
    assert_eq!(saved.selected_repository_index, Some(1));
}

/// After EnterPrsMode, committed_filter.state must be Some(Open) and all other
/// structured criteria must be unset/empty.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 74
#[test]
fn test_enter_prs_mode_default_committed_filter_is_open() {
    let state = dashboard_state();
    let new_state = state.apply(AppEvent::EnterPrsMode);

    assert_eq!(
        new_state.prs_state.committed_filter.state,
        Some(PrFilterState::Open)
    );
    assert!(new_state.prs_state.committed_filter.author.is_empty());
    assert!(new_state.prs_state.committed_filter.assignee.is_empty());
    assert!(new_state.prs_state.committed_filter.reviewer.is_empty());
    assert!(new_state.prs_state.committed_filter.is_draft.is_none());
    assert!(new_state.prs_state.committed_filter.labels.is_empty());
    assert!(new_state.prs_state.committed_filter.query_text.is_empty());
}

/// ClearFilter resets committed_filter to the default with state=Some(Open)
/// (not empty/None) and all criteria cleared.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 270-274
#[test]
fn test_clear_committed_filter_resets_state_to_open() {
    let mut state = dashboard_state();
    state.prs_state.active = true;
    // Dirty the committed filter with non-default values.
    state.prs_state.committed_filter.state = Some(PrFilterState::Closed);
    state.prs_state.committed_filter.author = "octocat".to_string();
    state.prs_state.committed_filter.query_text = "bug".to_string();

    let new_state = state.apply(AppEvent::PrClearFilter);

    assert_eq!(
        new_state.prs_state.committed_filter.state,
        Some(PrFilterState::Open)
    );
    assert!(new_state.prs_state.committed_filter.author.is_empty());
    assert!(new_state.prs_state.committed_filter.query_text.is_empty());
    assert!(new_state.prs_state.committed_filter.assignee.is_empty());
    assert!(new_state.prs_state.committed_filter.reviewer.is_empty());
    assert!(new_state.prs_state.committed_filter.is_draft.is_none());
    assert!(new_state.prs_state.committed_filter.labels.is_empty());
}

/// ExitPrsMode restores prior focus with bounds fallback.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-005
/// @pseudocode component-001 lines 77-87
#[test]
fn test_exit_prs_mode_restores_prior_focus_with_bounds_fallback() {
    let mut state = AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        ..dashboard_state()
    };
    state.prs_state.active = true;
    // Prior focus points to an agent index that no longer exists (out of bounds).
    state.prs_state.prior_agent_focus = Some(PriorAgentFocus {
        pane_focus: PaneFocus::Agents,
        selected_repository_index: Some(0),
        selected_agent_index: Some(99),
    });

    let new_state = state.apply(AppEvent::ExitPrsMode);

    assert_eq!(new_state.screen_mode, ScreenMode::Dashboard);
    assert!(!new_state.prs_state.active);
    // Fallback: must be Agents pane, index clamped to a valid value or None.
    assert_eq!(new_state.pane_focus, PaneFocus::Agents);
    assert!(new_state.selected_agent_index == Some(0) || new_state.selected_agent_index.is_none());
}

/// A legacy state.json without any PR fields must still deserialize successfully
/// with all prior fields intact.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-NFR-002
/// @pseudocode component-001 lines 66-76
#[test]
fn test_pre_pr_persisted_state_deserializes_without_pr_fields() {
    let legacy_json = serde_json::json!({
        "schema_version": 1,
        "repositories": [
            {
                "id": "repo-legacy",
                "name": "Legacy Repo",
                "slug": "legacy-repo",
                "base_dir": "/tmp/legacy-repo",
                "default_profile": "",
                "agent_ids": []
            }
        ],
        "agents": [],
        "selected_repository_index": 0,
        "selected_agent_index": null,
        "hide_idle_repositories": false,
        "last_selected_agent_by_repo": []
    });

    let state: PersistedState =
        serde_json::from_value(legacy_json).value_or_panic("legacy JSON should deserialize");

    assert_eq!(state.repositories.len(), 1);
    assert_eq!(state.selected_repository_index, Some(0));
    assert!(state.selected_agent_index.is_none());
    assert!(!state.hide_idle_repositories);
}

/// AppState::default() must have inactive prs_state (active=false, empty
/// list/detail, default filter).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-NFR-002
/// @pseudocode component-001 lines 66-76
#[test]
fn test_app_state_default_has_inactive_prs_state() {
    let state = AppState::default();

    assert!(matches!(
        state.prs_state,
        PullRequestsState { active: false, .. }
    ));
    assert!(state.prs_state.pull_requests().is_empty());
    assert!(state.prs_state.selected_pr_index().is_none());
    assert!(state.prs_state.pr_detail.is_none());
    assert!(!state.prs_state.active);
}

/// An empty PR list (loaded result is empty) must not panic and must CLEAR a
/// previously-non-empty list: the reducer must reset selected_pr_index to None,
/// clear pr_detail, and leave an empty-state (no panic). RED now because the
/// P03 no-op stub leaves the seeded list intact; GREEN in P05 once
/// apply_pr_list_loaded clears the list on empty result.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-014
/// @pseudocode component-001 lines 218-220
#[test]
fn test_empty_pr_list_shows_empty_state_not_panic() {
    let mut state = AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        ..dashboard_state()
    };
    state.prs_state.active = true;
    // Seed with a NON-empty list + selection + detail so the no-op stub would
    // leave them intact (and the test would FAIL). Only a real reducer that
    // clears the list on an empty loaded result will pass.
    state.prs_state.list.replace_items(vec![PullRequest {
        number: 42,
        title: "Seeded PR #42".to_string(),
        state: PrState::Open,
        author_login: "seeduser".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        head_sha: "sha123".to_string(),
        base_ref: "main".to_string(),
        is_draft: false,
        review_decision: None,
        checks_status: PrCheckStatus::None,
        mergeable: None,
        assignee_summary: String::new(),
        labels_summary: String::new(),
        comment_count: 0,
    }]);
    state.prs_state.list.set_selected_index(Some(0));

    // Begin a real list reload so the subsequent PrListLoaded is accepted.
    let request_id = begin_pr_list_reload(&mut state, "repo-1", PrFilter::default());

    // Dispatch the "list loaded with EMPTY result for the current scope" event.
    let new_state = state.apply(AppEvent::PrListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(PrFilter::default()),
        request_id,
        pull_requests: vec![],
        cursor: None,
        has_more: false,
    });

    // The reducer must CLEAR the seeded list and show the empty-state.
    assert!(
        new_state.prs_state.pull_requests().is_empty(),
        "empty loaded result must clear the previously-seeded list"
    );
    assert_eq!(
        new_state.prs_state.selected_pr_index(),
        None,
        "empty loaded result must reset selected_pr_index to None"
    );
    assert!(
        new_state.prs_state.pr_detail.is_none(),
        "empty loaded result must clear pr_detail"
    );
}
