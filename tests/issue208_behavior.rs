//! Behavioral coverage for issue 208 Actions run ordering across reloads and pagination.

use jefe::domain::{
    ListRequestId, PageToken, Repository, RepositoryId, WorkflowRun, WorkflowRunStatus,
};
use jefe::messages::{ActionsMessage, AppMessage};
use jefe::state::{ActionsListIdentity, AppState};

fn create_test_state() -> AppState {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId("test_repo".to_string()),
        "test_repo".to_string(),
        "test_repo".to_string(),
        std::path::PathBuf::from("/tmp"),
    ));
    state.selected_repository_index = Some(0);
    state
}

fn start_reload(state: &mut AppState, request_id: u64) {
    let identity = ActionsListIdentity {
        scope_repo_id: RepositoryId("test_repo".to_string()),
        filter: state.actions_state.committed_filter.clone(),
    };
    state
        .actions_state
        .list
        .begin_reload(identity, ListRequestId::from_raw(request_id));
}

fn alloc_request_id(state: &mut AppState) -> ListRequestId {
    let Ok(id) = state.actions_state.list.next_request_id() else {
        panic!("request id allocation must succeed in test setup");
    };
    id
}

fn make_run_at(id: u64, created_at: &str) -> WorkflowRun {
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
        created_at: created_at.to_string(),
        updated_at: created_at.to_string(),
    }
}

fn apply_reload(state: &mut AppState, runs: Vec<WorkflowRun>, has_more: bool) {
    start_reload(state, 1);
    let filter = state.actions_state.committed_filter.clone();
    *state = std::mem::take(state).apply_message(AppMessage::Actions(ActionsMessage::RunsLoaded {
        scope_repo_id: RepositoryId("test_repo".to_string()),
        filter: Box::new(filter),
        page: 1,
        request_id: 1,
        runs,
        has_more,
    }));
}

fn apply_page(state: &mut AppState, runs: Vec<WorkflowRun>) {
    let request_id = alloc_request_id(state);
    let filter = state.actions_state.committed_filter.clone();
    state
        .actions_state
        .list
        .begin_page(PageToken::PageNumber(2), request_id);
    *state =
        std::mem::take(state).apply_message(AppMessage::Actions(ActionsMessage::RunsPageLoaded {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            filter: Box::new(filter),
            page: 2,
            request_id: request_id.get(),
            runs,
            has_more: false,
        }));
}

fn run_ids(state: &AppState) -> Vec<u64> {
    state
        .actions_state
        .runs()
        .iter()
        .map(|run| run.id)
        .collect()
}

#[test]
fn runs_loaded_sorts_newest_created_at_first_and_empty_timestamps_last() {
    let mut state = create_test_state();
    apply_reload(
        &mut state,
        vec![
            make_run_at(1, "2026-07-01T10:00:00Z"),
            make_run_at(4, ""),
            make_run_at(2, "2026-07-03T10:00:00Z"),
            make_run_at(3, "2026-07-02T10:00:00Z"),
        ],
        false,
    );

    assert_eq!(run_ids(&state), vec![2, 3, 1, 4]);
    assert_eq!(
        state.actions_state.selected_run().map(|run| run.id),
        Some(2),
        "visible reload selects the newest run at index 0"
    );
}

#[test]
fn runs_load_paths_break_equal_created_at_ties_by_id_desc() {
    let mut state = create_test_state();
    apply_reload(
        &mut state,
        vec![make_run_at(1, "t"), make_run_at(2, "t")],
        true,
    );
    assert_eq!(run_ids(&state), vec![2, 1]);
    assert_eq!(
        state.actions_state.selected_run().map(|run| run.id),
        Some(2),
        "visible reload selects the id-desc winner at index 0"
    );

    state.actions_state.list.set_selected_index(Some(1));
    apply_page(&mut state, vec![make_run_at(4, "t"), make_run_at(3, "t")]);

    assert_eq!(run_ids(&state), vec![4, 3, 2, 1]);
    assert_eq!(
        state.actions_state.selected_run().map(|run| run.id),
        Some(1),
        "selection follows the same run id across a tied-timestamp resort"
    );
}

#[test]
fn runs_page_loaded_resorts_interleaved_appends_and_preserves_selection() {
    let mut state = create_test_state();
    apply_reload(
        &mut state,
        vec![
            make_run_at(1, "2026-07-02T10:00:00Z"),
            make_run_at(10, "2026-07-04T10:00:00Z"),
        ],
        true,
    );
    assert_eq!(run_ids(&state), vec![10, 1]);

    state.actions_state.list.set_selected_index(Some(1));
    apply_page(
        &mut state,
        vec![
            make_run_at(3, "2026-07-01T10:00:00Z"),
            make_run_at(2, "2026-07-03T10:00:00Z"),
        ],
    );

    assert_eq!(run_ids(&state), vec![10, 2, 1, 3]);
    assert_eq!(
        state.actions_state.selected_run().map(|run| run.id),
        Some(1),
        "selection follows the same run id across resort"
    );
}
