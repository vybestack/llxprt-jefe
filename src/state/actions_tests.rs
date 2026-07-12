#[cfg(test)]
mod tests {
    use crate::domain::{
        ActionsFilter, Repository, RepositoryId, Workflow, WorkflowRun, WorkflowRunStatus,
    };
    use crate::messages::{ActionsMessage, NavDir, ScrollDir};
    use crate::state::{ActionsFocus, AppState, ModalState, ScreenMode};

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

    #[test]
    fn test_enter_exit_actions_mode() {
        let mut state = create_test_state();
        assert!(!state.actions_state.active);

        // Enter
        state.apply_actions_message(ActionsMessage::EnterMode);
        assert!(state.actions_state.active);
        assert_eq!(state.screen_mode, ScreenMode::DashboardActions);
        assert_eq!(state.actions_state.focus, ActionsFocus::RunList);
        assert!(state.actions_state.loading.list);

        // Exit
        state.apply_actions_message(ActionsMessage::ExitMode);
        assert!(!state.actions_state.active);
        assert_eq!(state.screen_mode, ScreenMode::Dashboard);
    }

    #[test]
    fn test_cycle_focus() {
        let mut state = create_test_state();
        state.actions_state.focus = ActionsFocus::RunList;

        state.apply_actions_message(ActionsMessage::CycleFocus);
        assert_eq!(state.actions_state.focus, ActionsFocus::Detail);

        state.apply_actions_message(ActionsMessage::CycleFocusReverse);
        assert_eq!(state.actions_state.focus, ActionsFocus::RunList);
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

    #[test]
    fn test_navigation_and_scrolling() {
        let mut state = create_test_state();
        let run1 = make_run(1);
        let run2 = make_run(2);
        state.actions_state.runs = vec![run1.clone(), run2];
        state.actions_state.selected_run_index = Some(0);

        // Navigate Down
        state.apply_actions_message(ActionsMessage::Navigate(NavDir::Down));
        assert_eq!(state.actions_state.selected_run_index, Some(1));
        assert!(!state.actions_state.loading.detail);
        assert!(state.actions_state.detail_pending.is_none());

        // Navigate Up
        state.apply_actions_message(ActionsMessage::Navigate(NavDir::Up));
        assert_eq!(state.actions_state.selected_run_index, Some(0));

        // Scroll Detail clamps to 0 when no detail is loaded.
        scroll_and_assert(&mut state, ScrollDir::Down, 0);
        scroll_and_assert(&mut state, ScrollDir::Up, 0);

        // With a scrollable detail, verify scrolling advances and retreats.
        let detail = crate::domain::WorkflowRunDetail {
            run: run1.clone(),
            jobs: (0..20)
                .map(|i| crate::domain::WorkflowRunJob {
                    id: i,
                    name: format!("job-{i}"),
                    status: WorkflowRunStatus::Completed,
                    conclusion: None,
                    steps: Vec::new(),
                })
                .collect(),
        };
        state.actions_state.run_detail = Some(detail);
        state.actions_state.detail_viewport_rows = 5;
        let max = state.actions_max_detail_scroll_offset();
        assert!(
            max > 0,
            "a 20-job detail with a 5-row viewport is scrollable"
        );

        scroll_and_assert(&mut state, ScrollDir::Down, 1);
        scroll_and_assert(&mut state, ScrollDir::Up, 0);
        // PageDown advances by 10 then clamps at max.
        state.apply_actions_message(ActionsMessage::ScrollDetail(ScrollDir::PageDown));
        assert_eq!(
            state.actions_state.detail_scroll_offset,
            10.min(max),
            "PageDown advances by 10 then clamps at max"
        );
        // Repeated Down presses climb to max and then stop there.
        for _ in 0..(max + 5) {
            state.apply_actions_message(ActionsMessage::ScrollDetail(ScrollDir::Down));
        }
        assert_eq!(
            state.actions_state.detail_scroll_offset, max,
            "scroll never exceeds the max offset"
        );
    }

    fn scroll_and_assert(state: &mut AppState, dir: ScrollDir, expected: usize) {
        state.apply_actions_message(ActionsMessage::ScrollDetail(dir));
        assert_eq!(state.actions_state.detail_scroll_offset, expected);
    }

    #[test]
    fn test_reload_and_fail() {
        let mut state = create_test_state();
        let repo_id = RepositoryId("test_repo".to_string());
        let filter = ActionsFilter::default();
        let req_id = 42;

        state.actions_state.list_reload_pending = Some(crate::state::ActionsListReloadPending {
            scope_repo_id: repo_id.clone(),
            filter: filter.clone(),
            page: 1,
            request_id: req_id,
        });
        state.actions_state.loading.list = true;

        // Fail load
        state.apply_actions_message(ActionsMessage::RunsLoadFailed {
            scope_repo_id: repo_id.clone(),
            filter: Box::new(filter.clone()),
            page: 1,
            request_id: req_id,
            error: "Failed".to_string(),
        });
        assert!(!state.actions_state.loading.list);
        assert_eq!(state.actions_state.error, Some("Failed".to_string()));
        assert!(state.actions_state.list_reload_pending.is_none());

        // Success load
        state.actions_state.list_reload_pending = Some(crate::state::ActionsListReloadPending {
            scope_repo_id: repo_id.clone(),
            filter: filter.clone(),
            page: 1,
            request_id: req_id + 1,
        });
        state.actions_state.loading.list = true;
        let run = WorkflowRun {
            id: 1,
            name: "Run 1".to_string(),
            head_branch: "main".to_string(),
            head_sha: "sha1".to_string(),
            run_number: 1,
            event: "push".to_string(),
            status: WorkflowRunStatus::Completed,
            conclusion: None,
            workflow_name: "CI".to_string(),
            created_at: "time".to_string(),
            updated_at: "time".to_string(),
        };

        state.apply_actions_message(ActionsMessage::RunsLoaded {
            scope_repo_id: repo_id,
            filter: Box::new(filter),
            page: 1,
            request_id: req_id + 1,
            runs: vec![run],
            has_more: false,
        });
        assert!(!state.actions_state.loading.list);
        assert_eq!(state.actions_state.runs.len(), 1);
        assert_eq!(state.actions_state.selected_run_index, Some(0));
    }

    #[test]
    fn test_filters() {
        let mut state = create_test_state();
        state.actions_state.draft_filter.workflow = "draft_wf".to_string();

        state.apply_actions_message(ActionsMessage::ApplyFilter);
        assert_eq!(state.actions_state.committed_filter.workflow, "draft_wf");
        assert!(!state.actions_state.ui.filter_ui_open);
        assert!(state.actions_state.loading.list);

        state.apply_actions_message(ActionsMessage::ClearFilter);
        assert_eq!(state.actions_state.committed_filter.workflow, "");
        assert_eq!(state.actions_state.draft_filter.workflow, "");

        // Toggle UI open
        state.apply_actions_message(ActionsMessage::OpenFilterControls);
        assert!(state.actions_state.ui.filter_ui_open);
        state.apply_actions_message(ActionsMessage::CloseFilterControls);
        assert!(!state.actions_state.ui.filter_ui_open);
    }

    #[test]
    fn test_dispatch_success() {
        let mut state = create_test_state();
        let wf = Workflow {
            id: 1,
            name: "Workflow 1".to_string(),
            path: ".github/workflows/ci.yml".to_string(),
            state: "active".to_string(),
        };

        state.apply_actions_message(ActionsMessage::OpenWorkflowDispatch(wf));
        assert!(matches!(state.modal, ModalState::WorkflowDispatch { .. }));

        state.apply_actions_message(ActionsMessage::WorkflowDispatchSubmitted {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            workflow_id: "1".to_string(),
            ref_name: "main".to_string(),
            inputs: vec![],
        });
        assert!(state.actions_state.dispatch_pending());
        assert!(matches!(state.modal, ModalState::None));

        state.apply_actions_message(ActionsMessage::WorkflowDispatchSuccess {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            request_id: 1,
        });
        assert!(!state.actions_state.dispatch_pending());
    }

    #[test]
    fn test_dispatch_failed() {
        let mut state = create_test_state();
        let wf = Workflow {
            id: 1,
            name: "Workflow 1".to_string(),
            path: ".github/workflows/ci.yml".to_string(),
            state: "active".to_string(),
        };

        state.apply_actions_message(ActionsMessage::OpenWorkflowDispatch(wf));
        state.apply_actions_message(ActionsMessage::WorkflowDispatchSubmitted {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            workflow_id: "1".to_string(),
            ref_name: "main".to_string(),
            inputs: vec![],
        });
        assert!(state.actions_state.dispatch_pending());

        state.apply_actions_message(ActionsMessage::WorkflowDispatchFailed {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            request_id: 1,
            error: "Failed dispatch".to_string(),
        });
        assert!(!state.actions_state.dispatch_pending());
        assert_eq!(
            state.actions_state.error,
            Some("Failed dispatch".to_string())
        );
    }

    // ---- BUG 2: Workflow filter must use path (not display name) for API ----

    /// Cycling the workflow filter must set `workflow_path` to the workflow's
    /// file path (e.g. ".github/workflows/ci.yml"), NOT the display name ("CI").
    /// The GitHub API rejects display names with HTTP 404.
    #[test]
    fn cycle_workflow_filter_sets_path_not_display_name() {
        let mut state = create_test_state();
        state.actions_state.workflows = vec![
            Workflow {
                id: 100,
                name: "CI".to_string(),
                path: ".github/workflows/ci.yml".to_string(),
                state: "active".to_string(),
            },
            Workflow {
                id: 200,
                name: "Deploy".to_string(),
                path: ".github/workflows/deploy.yml".to_string(),
                state: "active".to_string(),
            },
        ];

        // CycleFilterStatus advances the active filter field. Select the
        // workflow field (index 0) so cycling walks the workflow list.
        state.actions_state.ui.filter_field_index = 0;

        // Cycle forward once: "all" → first workflow ("CI").
        state.apply_actions_message(ActionsMessage::CycleFilterStatus);
        assert_eq!(
            state.actions_state.draft_filter.workflow, "CI",
            "display name should be the user-friendly name"
        );
        assert_eq!(
            state.actions_state.draft_filter.workflow_path, ".github/workflows/ci.yml",
            "workflow_path must be the file path for the API call, not the display name"
        );

        // Cycle forward again: "CI" → "Deploy".
        state.apply_actions_message(ActionsMessage::CycleFilterStatus);
        assert_eq!(state.actions_state.draft_filter.workflow, "Deploy");
        assert_eq!(
            state.actions_state.draft_filter.workflow_path,
            ".github/workflows/deploy.yml"
        );

        // Cycle forward once more: "Deploy" → "all".
        state.apply_actions_message(ActionsMessage::CycleFilterStatus);
        assert!(state.actions_state.draft_filter.workflow.is_empty());
        assert!(state.actions_state.draft_filter.workflow_path.is_empty());
    }

    /// With a single workflow, cycling forward wraps "all" → "CI" and sets the
    /// path (not the display name) for the API call.
    #[test]
    fn cycle_workflow_filter_forward_uses_path() {
        let mut state = create_test_state();
        state.actions_state.workflows = vec![Workflow {
            id: 100,
            name: "CI".to_string(),
            path: ".github/workflows/ci.yml".to_string(),
            state: "active".to_string(),
        }];

        // With a single workflow, cycling forward wraps "all" → "CI".
        state.actions_state.ui.filter_field_index = 0;
        state.apply_actions_message(ActionsMessage::CycleFilterStatus);
        assert_eq!(state.actions_state.draft_filter.workflow, "CI");
        assert_eq!(
            state.actions_state.draft_filter.workflow_path,
            ".github/workflows/ci.yml"
        );
    }

    /// Cycling the workflow filter when no workflows are loaded yet must be a
    /// safe no-op (no panic, no state change).
    #[test]
    fn cycle_workflow_filter_empty_workflows_is_noop() {
        let mut state = create_test_state();
        // No workflows loaded (e.g. still loading on first entry).
        state.actions_state.workflows = Vec::new();
        state.actions_state.ui.filter_field_index = 0;
        state.apply_actions_message(ActionsMessage::CycleFilterStatus);
        assert!(
            state.actions_state.draft_filter.workflow.is_empty(),
            "workflow filter must stay empty with no loaded workflows"
        );
        assert!(state.actions_state.draft_filter.workflow_path.is_empty());
    }

    /// After applying a filter with a selected workflow, the committed filter's
    /// `workflow_path` is set (so the API call uses the path).
    #[test]
    fn apply_filter_commits_workflow_path() {
        let mut state = create_test_state();
        state.actions_state.workflows = vec![Workflow {
            id: 100,
            name: "CI".to_string(),
            path: ".github/workflows/ci.yml".to_string(),
            state: "active".to_string(),
        }];
        state.actions_state.ui.filter_field_index = 0;
        state.apply_actions_message(ActionsMessage::CycleFilterStatus);
        state.apply_actions_message(ActionsMessage::ApplyFilter);

        assert_eq!(
            state.actions_state.committed_filter.workflow, "CI",
            "committed display name"
        );
        assert_eq!(
            state.actions_state.committed_filter.workflow_path, ".github/workflows/ci.yml",
            "committed workflow_path must contain the file path"
        );
    }

    // ---- BUG 5: job expand/collapse state ----

    fn make_detail_with_jobs() -> crate::domain::WorkflowRunDetail {
        use crate::domain::{
            WorkflowRun, WorkflowRunConclusion, WorkflowRunDetail, WorkflowRunJob,
            WorkflowRunStatus, WorkflowRunStep,
        };
        WorkflowRunDetail {
            run: WorkflowRun {
                id: 1,
                name: "Run 1".to_string(),
                head_branch: "main".to_string(),
                head_sha: "abc".to_string(),
                run_number: 1,
                event: "push".to_string(),
                status: WorkflowRunStatus::Completed,
                conclusion: Some(WorkflowRunConclusion::Success),
                workflow_name: "CI".to_string(),
                created_at: "t".to_string(),
                updated_at: "t".to_string(),
            },
            jobs: vec![
                WorkflowRunJob {
                    id: 100,
                    name: "build".to_string(),
                    status: WorkflowRunStatus::Completed,
                    conclusion: Some(WorkflowRunConclusion::Success),
                    steps: vec![
                        WorkflowRunStep {
                            name: "checkout".to_string(),
                            status: WorkflowRunStatus::Completed,
                            conclusion: Some(WorkflowRunConclusion::Success),
                            number: 1,
                        },
                        WorkflowRunStep {
                            name: "compile".to_string(),
                            status: WorkflowRunStatus::Completed,
                            conclusion: Some(WorkflowRunConclusion::Success),
                            number: 2,
                        },
                    ],
                },
                WorkflowRunJob {
                    id: 200,
                    name: "test".to_string(),
                    status: WorkflowRunStatus::Completed,
                    conclusion: Some(WorkflowRunConclusion::Failure),
                    steps: vec![WorkflowRunStep {
                        name: "unit-tests".to_string(),
                        status: WorkflowRunStatus::Completed,
                        conclusion: Some(WorkflowRunConclusion::Failure),
                        number: 1,
                    }],
                },
            ],
        }
    }

    #[test]
    fn load_detail_resets_expanded_jobs_and_focuses_first() {
        let mut state = create_test_state();
        // Pre-populate expanded state from a prior detail.
        state.actions_state.expanded_jobs.insert(999);
        // Set up a matching detail_pending so the DetailLoaded handler fires.
        state.actions_state.detail_pending = Some(crate::state::ActionsDetailPending {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            run_id: 1,
            request_id: 0,
        });
        state.apply_actions_message(ActionsMessage::DetailLoaded {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            run_id: 1,
            request_id: 0,
            detail: Box::new(make_detail_with_jobs()),
        });
        assert!(
            state.actions_state.expanded_jobs.is_empty(),
            "expanded_jobs must be cleared on new detail load"
        );
        assert_eq!(
            state.actions_state.focused_job_index,
            Some(0),
            "first job must be focused on detail load"
        );
    }

    #[test]
    fn toggle_job_expand_adds_and_removes() {
        let mut state = create_test_state();
        state.actions_state.run_detail = Some(make_detail_with_jobs());
        state.actions_state.focused_job_index = Some(0);

        // Toggle expand on focused job (index 0, id 100).
        state.apply_actions_message(ActionsMessage::ToggleJobExpand);
        assert!(
            state.actions_state.expanded_jobs.contains(&100),
            "toggling must expand the focused job"
        );

        // Toggle again: collapses.
        state.apply_actions_message(ActionsMessage::ToggleJobExpand);
        assert!(
            !state.actions_state.expanded_jobs.contains(&100),
            "toggling again must collapse the focused job"
        );
    }

    #[test]
    fn navigate_job_moves_focus() {
        let mut state = create_test_state();
        state.actions_state.run_detail = Some(make_detail_with_jobs());
        state.actions_state.focused_job_index = Some(0);

        state.apply_actions_message(ActionsMessage::NavigateJob(NavDir::Down));
        assert_eq!(state.actions_state.focused_job_index, Some(1));

        // Down at last job clamps.
        state.apply_actions_message(ActionsMessage::NavigateJob(NavDir::Down));
        assert_eq!(state.actions_state.focused_job_index, Some(1));

        state.apply_actions_message(ActionsMessage::NavigateJob(NavDir::Up));
        assert_eq!(state.actions_state.focused_job_index, Some(0));

        // Up at first job clamps.
        state.apply_actions_message(ActionsMessage::NavigateJob(NavDir::Up));
        assert_eq!(state.actions_state.focused_job_index, Some(0));
    }

    #[test]
    fn collapse_job_removes_from_expanded() {
        let mut state = create_test_state();
        state.actions_state.run_detail = Some(make_detail_with_jobs());
        state.actions_state.focused_job_index = Some(0);

        // Expand job 0.
        state.apply_actions_message(ActionsMessage::ToggleJobExpand);
        assert!(state.actions_state.expanded_jobs.contains(&100));

        // Collapse it.
        state.apply_actions_message(ActionsMessage::CollapseJob);
        assert!(!state.actions_state.expanded_jobs.contains(&100));
    }
}
