//! State-level coverage for Actions workflow run sorting (issue #473).

#[cfg(test)]
mod tests {
    use crate::domain::{
        ActionsFilter, ActionsSortBy, ActionsSortConfig, Repository, RepositoryId, SortOrder,
        WorkflowRun, WorkflowRunConclusion, WorkflowRunStatus,
    };
    use crate::messages::ActionsMessage;
    use crate::state::{ActionsListIdentity, AppState};

    fn create_test_state() -> AppState {
        let mut state = AppState::test_fixture();
        let repo = Repository::new(
            RepositoryId("test_repo".to_string()),
            crate::domain::shipped_agent_type(3),
            crate::domain::TypedMap::new(),
            "test_repo".to_string(),
            "test_repo".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        state.repositories.push(repo);
        state.selected_repository_index = Some(0);
        state
    }

    fn make_run(id: u64, run_number: u32, created_at: &str, updated_at: &str) -> WorkflowRun {
        WorkflowRun {
            id,
            name: format!("Run {run_number}"),
            head_branch: "main".to_string(),
            head_sha: format!("sha{id}"),
            run_number,
            event: "push".to_string(),
            status: WorkflowRunStatus::Completed,
            conclusion: Some(WorkflowRunConclusion::Success),
            workflow_name: "CI".to_string(),
            created_at: created_at.to_string(),
            updated_at: updated_at.to_string(),
        }
    }

    fn seed_runs(state: &mut AppState) {
        state.actions_state.list.items_mut().clear();
        state.actions_state.list.items_mut().extend_from_slice(&[
            make_run(1, 100, "2026-01-01T00:00:00Z", "2026-01-03T00:00:00Z"),
            make_run(2, 300, "2026-01-02T00:00:00Z", "2026-01-01T00:00:00Z"),
            make_run(3, 200, "2026-01-03T00:00:00Z", "2026-01-02T00:00:00Z"),
        ]);
        state.actions_state.list.set_selected_index(Some(0));
    }

    #[test]
    fn default_sort_config_is_created_desc() {
        let state = create_test_state();
        assert_eq!(
            state.actions_state.sort_config,
            ActionsSortConfig::default_sort()
        );
        assert_eq!(state.actions_state.sort_config.by, ActionsSortBy::Created);
        assert_eq!(state.actions_state.sort_config.order, SortOrder::Desc);
    }

    #[test]
    fn cycle_sort_by_next_wraps() {
        let mut state = create_test_state();
        state.apply_actions_message(ActionsMessage::CycleActionsSortByNext);
        assert_eq!(state.actions_state.sort_config.by, ActionsSortBy::Updated);
        state.apply_actions_message(ActionsMessage::CycleActionsSortByNext);
        assert_eq!(state.actions_state.sort_config.by, ActionsSortBy::Number);
        state.apply_actions_message(ActionsMessage::CycleActionsSortByNext);
        assert_eq!(state.actions_state.sort_config.by, ActionsSortBy::Created);
    }

    #[test]
    fn cycle_sort_by_prev_wraps() {
        let mut state = create_test_state();
        // Default is Created; prev of Created is Number.
        state.apply_actions_message(ActionsMessage::CycleActionsSortByPrev);
        assert_eq!(state.actions_state.sort_config.by, ActionsSortBy::Number);
        // Prev of Number is Updated.
        state.apply_actions_message(ActionsMessage::CycleActionsSortByPrev);
        assert_eq!(state.actions_state.sort_config.by, ActionsSortBy::Updated);
    }

    #[test]
    fn toggle_sort_order_flips_asc_desc() {
        let mut state = create_test_state();
        assert_eq!(state.actions_state.sort_config.order, SortOrder::Desc);
        state.apply_actions_message(ActionsMessage::ToggleActionsSortOrder);
        assert_eq!(state.actions_state.sort_config.order, SortOrder::Asc);
        state.apply_actions_message(ActionsMessage::ToggleActionsSortOrder);
        assert_eq!(state.actions_state.sort_config.order, SortOrder::Desc);
    }

    #[test]
    fn number_desc_sorts_highest_run_number_first() {
        let mut state = create_test_state();
        seed_runs(&mut state);
        // Cycle Created→Updated→Number to land on Number sort.
        state.apply_actions_message(ActionsMessage::CycleActionsSortByNext);
        state.apply_actions_message(ActionsMessage::CycleActionsSortByNext);
        assert_eq!(state.actions_state.sort_config.by, ActionsSortBy::Number);
        assert_eq!(state.actions_state.sort_config.order, SortOrder::Desc);
        let run_numbers: Vec<u32> = state
            .actions_state
            .list
            .items()
            .iter()
            .map(|r| r.run_number)
            .collect();
        assert_eq!(run_numbers, vec![300, 200, 100]);
    }

    #[test]
    fn created_asc_sorts_oldest_first() {
        let mut state = create_test_state();
        seed_runs(&mut state);
        // Start with Created/Desc (default), toggle to Created/Asc.
        assert_eq!(state.actions_state.sort_config.by, ActionsSortBy::Created);
        assert_eq!(state.actions_state.sort_config.order, SortOrder::Desc);
        state.apply_actions_message(ActionsMessage::ToggleActionsSortOrder);
        assert_eq!(state.actions_state.sort_config.order, SortOrder::Asc);
        let numbers: Vec<u32> = state
            .actions_state
            .list
            .items()
            .iter()
            .map(|r| r.run_number)
            .collect();
        // created_at: run100=2026-01-01, run300=2026-01-02, run200=2026-01-03
        assert_eq!(numbers, vec![100, 300, 200]);
    }

    #[test]
    fn sort_preserves_selection_by_identity() {
        let mut state = create_test_state();
        seed_runs(&mut state);
        // Select run #2 (id=2, run_number=300).
        state.actions_state.list.set_selected_index(Some(1));
        assert_eq!(state.actions_state.list.selected_index(), Some(1));

        // Cycle to number sort — run #2 (run_number=300) will move to index 0.
        state.apply_actions_message(ActionsMessage::CycleActionsSortByNext);
        state.apply_actions_message(ActionsMessage::CycleActionsSortByNext);

        // After number sort desc, the selected run (id=2) should be found by
        // identity, not by index.
        let selected = state
            .actions_state
            .list
            .selected_index()
            .and_then(|idx| state.actions_state.list.items().get(idx));
        assert_eq!(
            selected.map(|r| r.id),
            Some(2),
            "selection must follow the run by identity after re-sort"
        );
    }

    #[test]
    fn sort_does_not_change_identity_or_pagination() {
        let mut state = create_test_state();
        seed_runs(&mut state);

        let identity_before = state
            .actions_state
            .list
            .identity()
            .map(|i| i.scope_repo_id.clone());
        let selected_run_id_before = state
            .actions_state
            .list
            .selected_index()
            .and_then(|idx| state.actions_state.list.items().get(idx))
            .map(|run| run.id);
        let has_more_before = state.actions_state.list.has_more();

        state.apply_actions_message(ActionsMessage::CycleActionsSortByNext);

        let identity_after = state
            .actions_state
            .list
            .identity()
            .map(|i| i.scope_repo_id.clone());
        assert_eq!(
            identity_before, identity_after,
            "sort must not mutate the list identity"
        );
        assert_eq!(
            has_more_before,
            state.actions_state.list.has_more(),
            "sort must not change pagination state"
        );
        let selected_run_id_after = state
            .actions_state
            .list
            .selected_index()
            .and_then(|idx| state.actions_state.list.items().get(idx))
            .map(|run| run.id);
        assert_eq!(
            selected_run_id_before, selected_run_id_after,
            "sort must preserve selection by run identity"
        );
    }

    /// Verify that the configurable comparator produces the expected order
    /// under Number/Asc when applied to a reload result. The production path
    /// (`reload_runs`) calls `resort_actions_by_config` after `accept_loaded`;
    /// this test isolates the comparator itself by applying it directly.
    #[test]
    fn comparator_applies_number_asc_after_reload() {
        let mut state = create_test_state();

        // Set sort to Number/Asc BEFORE any data arrives.
        state.actions_state.sort_config = ActionsSortConfig {
            by: ActionsSortBy::Number,
            order: SortOrder::Asc,
        };

        let identity = ActionsListIdentity {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            filter: ActionsFilter::default(),
        };
        let Ok(request_id) = state.actions_state.list.next_request_id() else {
            panic!("request id allocation must succeed in test setup");
        };
        state.actions_state.list.begin_reload(identity, request_id);

        // Deliver runs in descending run_number order; after Number/Asc sort
        // they must come out ascending.
        let runs = vec![
            make_run(3, 300, "2026-01-03T00:00:00Z", "2026-01-03T00:00:00Z"),
            make_run(2, 200, "2026-01-02T00:00:00Z", "2026-01-02T00:00:00Z"),
            make_run(1, 100, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
        ];
        let result = crate::state::pagination::ReloadResult {
            identity: ActionsListIdentity {
                scope_repo_id: RepositoryId("test_repo".to_string()),
                filter: ActionsFilter::default(),
            },
            request_id,
            items: runs,
            next_page: crate::domain::PageToken::Done,
        };
        let _outcome = state.actions_state.list.accept_loaded(result);

        // Apply the comparator directly (mirrors resort_actions_by_config).
        let config = state.actions_state.sort_config;
        state
            .actions_state
            .list
            .sort_by(|a, b| crate::github::compare_workflow_runs(a, b, config));

        let run_numbers: Vec<u32> = state
            .actions_state
            .list
            .items()
            .iter()
            .map(|r| r.run_number)
            .collect();
        assert_eq!(
            run_numbers,
            vec![100, 200, 300],
            "active Number/Asc sort must produce ascending run numbers"
        );
    }
}
