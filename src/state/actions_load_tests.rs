//! Behavioral tests for the Actions async-load correlation logic.
//!
//! These tests exercise the reducer's request-ID / scope correlation guards
//! (stale-result rejection, wrong-scope rejection, error clearing) and the
//! search/filter state transitions. They complement the happy-path tests in
//! `actions_tests.rs`.

use crate::domain::{
    ActionsFilter, Repository, RepositoryId, Workflow, WorkflowRun, WorkflowRunDetail,
    WorkflowRunJob, WorkflowRunStatus,
};
use crate::messages::ActionsMessage;
use crate::state::AppState;

fn create_test_state() -> AppState {
    let mut state = AppState::default();
    let repo = Repository::new(
        RepositoryId("test_repo".to_string()),
        "test_repo".to_string(),
        "test_repo".to_string(),
        std::path::PathBuf::from("/tmp"),
    );
    state.repositories.push(repo);
    state.selected_repository_index = Some(0);
    state
}

fn make_run(id: u64) -> WorkflowRun {
    WorkflowRun {
        id,
        name: format!("Run {id}"),
        head_branch: "main".to_string(),
        head_sha: format!("sha{id}"),
        run_number: u32::try_from(id).unwrap_or_default(),
        event: "push".to_string(),
        status: WorkflowRunStatus::Completed,
        conclusion: None,
        workflow_name: "CI".to_string(),
        created_at: "time".to_string(),
        updated_at: "time".to_string(),
    }
}

fn make_detail(id: u64) -> WorkflowRunDetail {
    WorkflowRunDetail {
        run: make_run(id),
        jobs: Vec::new(),
    }
}

fn make_detail_with_jobs(id: u64, jobs: Vec<WorkflowRunJob>) -> WorkflowRunDetail {
    WorkflowRunDetail {
        run: make_run(id),
        jobs,
    }
}

/// A `RunsLoaded` whose `request_id` doesn't match the pending reload is
/// ignored entirely — runs stay unchanged and loading stays true.
#[test]
fn stale_runs_loaded_is_ignored() {
    let mut state = create_test_state();
    let repo_id = RepositoryId("test_repo".to_string());
    let filter = ActionsFilter::default();

    state.actions_state.list_reload_pending = Some(crate::state::ActionsListReloadPending {
        scope_repo_id: repo_id.clone(),
        filter: filter.clone(),
        page: 1,
        request_id: 100,
    });
    state.actions_state.loading.list = true;

    // Stale: request_id 999 != pending 100
    state.apply_actions_message(ActionsMessage::RunsLoaded {
        scope_repo_id: repo_id,
        filter: Box::new(filter),
        page: 1,
        request_id: 999,
        runs: vec![make_run(1)],
        has_more: false,
    });

    assert!(
        state.actions_state.runs.is_empty(),
        "stale result must not populate runs"
    );
    assert!(
        state.actions_state.loading.list,
        "stale result must not clear loading"
    );
    assert!(
        state.actions_state.list_reload_pending.is_some(),
        "stale result must not clear pending"
    );
}

/// A `DetailLoaded` with a wrong `request_id` is ignored.
#[test]
fn stale_detail_loaded_is_ignored() {
    let mut state = create_test_state();
    let repo_id = RepositoryId("test_repo".to_string());

    state.actions_state.detail_pending = Some(crate::state::ActionsDetailPending {
        scope_repo_id: repo_id.clone(),
        run_id: 1,
        request_id: 50,
    });
    state.actions_state.loading.detail = true;

    state.apply_actions_message(ActionsMessage::DetailLoaded {
        scope_repo_id: repo_id,
        run_id: 1,
        request_id: 999, // stale
        detail: Box::new(make_detail(1)),
    });

    assert!(
        state.actions_state.run_detail.is_none(),
        "stale detail must not populate run_detail"
    );
    assert!(
        state.actions_state.loading.detail,
        "stale detail must not clear loading"
    );
}

/// A `RunsLoaded` with a different `scope_repo_id` is ignored.
#[test]
fn wrong_scope_runs_loaded_is_ignored() {
    let mut state = create_test_state();
    let repo_id = RepositoryId("test_repo".to_string());
    let other_repo = RepositoryId("other_repo".to_string());
    let filter = ActionsFilter::default();

    state.actions_state.list_reload_pending = Some(crate::state::ActionsListReloadPending {
        scope_repo_id: repo_id,
        filter: filter.clone(),
        page: 1,
        request_id: 1,
    });
    state.actions_state.loading.list = true;

    state.apply_actions_message(ActionsMessage::RunsLoaded {
        scope_repo_id: other_repo, // wrong scope
        filter: Box::new(filter),
        page: 1,
        request_id: 1,
        runs: vec![make_run(1)],
        has_more: false,
    });

    assert!(
        state.actions_state.runs.is_empty(),
        "wrong-scope result must not populate runs"
    );
    assert!(state.actions_state.loading.list);
    assert!(
        state.actions_state.list_reload_pending.is_some(),
        "wrong-scope result must not clear pending"
    );
}

/// After a failed load sets `error`, a subsequent accepted successful load
/// clears `error` (SHOULD-FIX E).
#[test]
fn error_cleared_on_accepted_success() {
    let mut state = create_test_state();
    let repo_id = RepositoryId("test_repo".to_string());
    let filter = ActionsFilter::default();

    // First: a failed load sets error.
    state.actions_state.list_reload_pending = Some(crate::state::ActionsListReloadPending {
        scope_repo_id: repo_id.clone(),
        filter: filter.clone(),
        page: 1,
        request_id: 1,
    });
    state.actions_state.loading.list = true;

    state.apply_actions_message(ActionsMessage::RunsLoadFailed {
        scope_repo_id: repo_id.clone(),
        filter: Box::new(filter.clone()),
        page: 1,
        request_id: 1,
        error: "network error".to_string(),
    });
    assert_eq!(state.actions_state.error, Some("network error".to_string()));

    // Second: a successful load with a fresh pending clears error.
    state.actions_state.list_reload_pending = Some(crate::state::ActionsListReloadPending {
        scope_repo_id: repo_id,
        filter: filter.clone(),
        page: 1,
        request_id: 2,
    });
    state.actions_state.loading.list = true;

    state.apply_actions_message(ActionsMessage::RunsLoaded {
        scope_repo_id: RepositoryId("test_repo".to_string()),
        filter: Box::new(filter),
        page: 1,
        request_id: 2,
        runs: vec![make_run(1)],
        has_more: false,
    });

    assert!(
        state.actions_state.error.is_none(),
        "accepted success must clear error"
    );
}

/// `DetailLoadFailed` with matching request_id sets error and clears
/// loading.detail.
#[test]
fn detail_failure_clears_loading_detail() {
    let mut state = create_test_state();
    let repo_id = RepositoryId("test_repo".to_string());

    state.actions_state.detail_pending = Some(crate::state::ActionsDetailPending {
        scope_repo_id: repo_id.clone(),
        run_id: 1,
        request_id: 10,
    });
    state.actions_state.loading.detail = true;

    state.apply_actions_message(ActionsMessage::DetailLoadFailed {
        scope_repo_id: repo_id,
        run_id: 1,
        request_id: 10,
        error: "detail fetch failed".to_string(),
    });

    assert_eq!(
        state.actions_state.error,
        Some("detail fetch failed".to_string())
    );
    assert!(
        !state.actions_state.loading.detail,
        "detail failure must clear loading.detail"
    );
    assert!(state.actions_state.detail_pending.is_none());
}

/// Page-1 success with empty `runs` leaves `selected_run_index = None`, does
/// NOT set `loading.detail`, and has no `detail_pending`.
#[test]
fn empty_list_load_page1() {
    let mut state = create_test_state();
    let repo_id = RepositoryId("test_repo".to_string());
    let filter = ActionsFilter::default();

    state.actions_state.list_reload_pending = Some(crate::state::ActionsListReloadPending {
        scope_repo_id: repo_id.clone(),
        filter: filter.clone(),
        page: 1,
        request_id: 1,
    });
    state.actions_state.loading.list = true;

    state.apply_actions_message(ActionsMessage::RunsLoaded {
        scope_repo_id: repo_id,
        filter: Box::new(filter),
        page: 1,
        request_id: 1,
        runs: Vec::new(),
        has_more: false,
    });

    assert!(state.actions_state.runs.is_empty());
    assert_eq!(
        state.actions_state.selected_run_index, None,
        "empty list must leave selected_run_index as None"
    );
    assert!(
        !state.actions_state.loading.detail,
        "empty list must not set loading.detail"
    );
    assert!(
        state.actions_state.detail_pending.is_none(),
        "empty list must not create detail_pending"
    );
    assert!(!state.actions_state.loading.list);
}

// ---- Workflow dispatch correlation (SHOULD-FIX G) ----

fn setup_dispatch_pending(state: &mut AppState, request_id: u64) {
    let repo_id = RepositoryId("test_repo".to_string());
    state.actions_state.dispatch_pending = Some(crate::state::ActionsDispatchPending {
        scope_repo_id: repo_id,
        workflow_id: "1".to_string(),
        request_id,
    });
}

/// A `WorkflowDispatchFailed` with a STALE request_id does NOT clear
/// `dispatch_pending` and does NOT set error.
#[test]
fn stale_dispatch_failed_does_not_clear_pending() {
    let mut state = create_test_state();
    setup_dispatch_pending(&mut state, 5);

    state.apply_actions_message(ActionsMessage::WorkflowDispatchFailed {
        scope_repo_id: RepositoryId("test_repo".to_string()),
        request_id: 999, // stale
        error: "failed".to_string(),
    });

    assert!(
        state.actions_state.dispatch_pending(),
        "stale dispatch failure must not clear dispatch_pending"
    );
    assert!(
        state.actions_state.error.is_none(),
        "stale dispatch failure must not set error"
    );
}

/// A `WorkflowDispatchFailed` with a MATCHING request_id clears
/// `dispatch_pending` and sets error.
#[test]
fn matching_dispatch_failed_clears_pending_and_sets_error() {
    let mut state = create_test_state();
    setup_dispatch_pending(&mut state, 5);

    state.apply_actions_message(ActionsMessage::WorkflowDispatchFailed {
        scope_repo_id: RepositoryId("test_repo".to_string()),
        request_id: 5, // matching
        error: "boom".to_string(),
    });

    assert!(!state.actions_state.dispatch_pending());
    assert_eq!(state.actions_state.error, Some("boom".to_string()));
}

/// A `WorkflowDispatchSuccess` with a STALE request_id does NOT clear
/// `dispatch_pending` and does NOT trigger a list reload.
#[test]
fn stale_dispatch_success_does_not_trigger_reload() {
    let mut state = create_test_state();
    setup_dispatch_pending(&mut state, 5);
    state.actions_state.list_reload_pending = None;

    state.apply_actions_message(ActionsMessage::WorkflowDispatchSuccess {
        scope_repo_id: RepositoryId("test_repo".to_string()),
        request_id: 999, // stale
    });

    assert!(
        state.actions_state.dispatch_pending(),
        "stale success must not clear dispatch_pending"
    );
    assert!(
        state.actions_state.list_reload_pending.is_none(),
        "stale success must not trigger a list reload"
    );
}

/// A `WorkflowDispatchSuccess` with a MATCHING request_id clears
/// `dispatch_pending` and triggers a list reload.
#[test]
fn matching_dispatch_success_clears_and_reloads() {
    let mut state = create_test_state();
    setup_dispatch_pending(&mut state, 5);

    state.apply_actions_message(ActionsMessage::WorkflowDispatchSuccess {
        scope_repo_id: RepositoryId("test_repo".to_string()),
        request_id: 5, // matching
    });

    assert!(!state.actions_state.dispatch_pending());
    assert!(
        state.actions_state.list_reload_pending.is_some(),
        "matching success must trigger a list reload"
    );
}

// ---- Search commit (Blocker 3) ----

/// `SetSearchQuery` then `ApplySearch` copies the trimmed query into
/// `committed_filter.search`; `ClearSearch` clears it.
#[test]
fn search_commit_and_clear() {
    let mut state = create_test_state();

    // Enter actions mode first so there's a selected repo for reload.
    state.apply_actions_message(ActionsMessage::EnterMode);
    state.actions_state.list_reload_pending = None; // clear the EnterMode reload

    state.apply_actions_message(ActionsMessage::SetSearchQuery {
        query: "  my search  ".to_string(),
    });
    assert_eq!(state.actions_state.search_query, "  my search  ");
    assert!(
        state.actions_state.committed_filter.search.is_empty(),
        "committed search is empty until ApplySearch"
    );

    state.apply_actions_message(ActionsMessage::ApplySearch);
    assert_eq!(
        state.actions_state.committed_filter.search, "my search",
        "ApplySearch trims and commits the query"
    );
    assert!(
        state.actions_state.loading.list,
        "ApplySearch must trigger a list reload"
    );
    assert!(
        state.actions_state.list_reload_pending.is_some(),
        "ApplySearch must set list_reload_pending"
    );
    // Clear the pending state so ClearSearch starts from a clean slate.
    state.actions_state.loading.list = false;
    state.actions_state.list_reload_pending = None;

    state.apply_actions_message(ActionsMessage::ClearSearch);
    assert!(
        state.actions_state.committed_filter.search.is_empty(),
        "ClearSearch resets committed search"
    );
    assert!(state.actions_state.search_query.is_empty());
    assert!(
        state.actions_state.loading.list,
        "ClearSearch must trigger a list reload"
    );
}

// ---- Filter apply/clear ----

/// `ApplyFilter` copies draft→committed and clears runs/selection.
#[test]
fn apply_filter_copies_draft_and_clears() {
    let mut state = create_test_state();
    state.actions_state.draft_filter.workflow = "deploy".to_string();
    // Pre-populate runs so we can verify they get cleared.
    state.actions_state.runs = vec![make_run(1), make_run(2)];
    state.actions_state.selected_run_index = Some(0);

    state.apply_actions_message(ActionsMessage::ApplyFilter);

    assert_eq!(
        state.actions_state.committed_filter.workflow, "deploy",
        "ApplyFilter copies draft to committed"
    );
    assert!(
        state.actions_state.runs.is_empty(),
        "ApplyFilter clears runs"
    );
    assert_eq!(
        state.actions_state.selected_run_index, None,
        "ApplyFilter clears selection"
    );
    assert!(
        state.actions_state.loading.list,
        "ApplyFilter triggers a reload"
    );
}

/// `ClearFilter` resets to default.
#[test]
fn clear_filter_resets_to_default() {
    let mut state = create_test_state();
    state.actions_state.committed_filter.workflow = "deploy".to_string();
    state.actions_state.draft_filter.workflow = "deploy".to_string();
    state.actions_state.committed_filter.status = "failed".to_string();
    state.actions_state.draft_filter.status = "failed".to_string();

    state.apply_actions_message(ActionsMessage::ClearFilter);

    assert_eq!(
        state.actions_state.committed_filter,
        ActionsFilter::default(),
        "ClearFilter resets committed to default"
    );
    assert_eq!(
        state.actions_state.draft_filter,
        ActionsFilter::default(),
        "ClearFilter resets draft to default"
    );
    assert!(
        state.actions_state.loading.list,
        "ClearFilter must trigger a reload"
    );
}

// ---- CycleFilterStatus ----

/// `CycleFilterStatus` cycles through the status values.
#[test]
fn cycle_filter_status_cycles() {
    let mut state = create_test_state();

    // CycleFilterStatus advances the field that is currently active in the
    // filter bar. Select the status field (index 1) so the status value cycles.
    state.actions_state.ui.filter_field_index = 1;

    // Default is "" which maps to "all" branch
    assert_eq!(state.actions_state.draft_filter.status, "");
    state.apply_actions_message(ActionsMessage::CycleFilterStatus);
    assert_eq!(state.actions_state.draft_filter.status, "completed");

    state.apply_actions_message(ActionsMessage::CycleFilterStatus);
    assert_eq!(state.actions_state.draft_filter.status, "failed");

    state.apply_actions_message(ActionsMessage::CycleFilterStatus);
    assert_eq!(state.actions_state.draft_filter.status, "in_progress");

    state.apply_actions_message(ActionsMessage::CycleFilterStatus);
    assert_eq!(state.actions_state.draft_filter.status, "queued");

    state.apply_actions_message(ActionsMessage::CycleFilterStatus);
    assert_eq!(state.actions_state.draft_filter.status, "all");

    state.apply_actions_message(ActionsMessage::CycleFilterStatus);
    assert_eq!(state.actions_state.draft_filter.status, "completed");
}

/// Detail success clears loading.detail and sets run_detail.
#[test]
fn detail_loaded_clears_loading_and_sets_detail() {
    let mut state = create_test_state();
    let repo_id = RepositoryId("test_repo".to_string());
    let detail = make_detail_with_jobs(
        1,
        vec![WorkflowRunJob {
            id: 0,
            name: "build".to_string(),
            status: WorkflowRunStatus::Completed,
            conclusion: None,
            steps: Vec::new(),
        }],
    );

    state.actions_state.detail_pending = Some(crate::state::ActionsDetailPending {
        scope_repo_id: repo_id.clone(),
        run_id: 1,
        request_id: 10,
    });
    state.actions_state.loading.detail = true;

    state.apply_actions_message(ActionsMessage::DetailLoaded {
        scope_repo_id: repo_id,
        run_id: 1,
        request_id: 10,
        detail: Box::new(detail),
    });

    assert!(
        state.actions_state.run_detail.is_some(),
        "matching detail must populate run_detail"
    );
    assert!(
        !state.actions_state.loading.detail,
        "matching detail must clear loading.detail"
    );
    assert!(state.actions_state.detail_pending.is_none());
    assert!(
        state.actions_state.error.is_none(),
        "matching detail must clear error"
    );
}

/// WorkflowsLoaded with matching request populates workflows.
#[test]
fn workflows_loaded_populates_workflows() {
    let mut state = create_test_state();
    let repo_id = RepositoryId("test_repo".to_string());

    state.actions_state.workflows_pending = Some(crate::state::WorkflowsPending {
        scope_repo_id: repo_id.clone(),
        request_id: 1,
    });

    let wfs = vec![Workflow {
        id: 1,
        name: "CI".to_string(),
        path: ".github/workflows/ci.yml".to_string(),
        state: "active".to_string(),
    }];

    state.apply_actions_message(ActionsMessage::WorkflowsLoaded {
        scope_repo_id: repo_id,
        request_id: 1,
        workflows: wfs,
    });

    assert_eq!(state.actions_state.workflows.len(), 1);
    assert!(state.actions_state.workflows_pending.is_none());
}
