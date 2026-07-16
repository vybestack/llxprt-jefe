#[cfg(test)]
mod tests {
    use crate::domain::{
        ActionsFilter, ListRequestId, Repository, RepositoryId, Workflow, WorkflowRun,
        WorkflowRunStatus,
    };
    use crate::messages::{ActionsMessage, NavDir, ScrollDir};
    use crate::state::{ActionsFocus, ActionsListIdentity, AppState, ModalState, ScreenMode};

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

    /// Helper: start a visible reload (page 1) so that a RunsLoaded message is
    /// accepted (not stale).
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

    /// Allocate a request id from the list, panicking on exhaustion (test
    /// setup where the state is controlled).
    fn alloc_req(
        list: &mut crate::state::pagination::PaginatedList<
            crate::domain::WorkflowRun,
            ActionsListIdentity,
        >,
    ) -> ListRequestId {
        let Ok(id) = list.next_request_id() else {
            panic!("request id allocation must succeed in test setup");
        };
        id
    }

    #[test]
    fn test_enter_exit_actions_mode() {
        let mut state = create_test_state();
        assert!(!state.actions_state.active);

        state.apply_actions_message(ActionsMessage::EnterMode);
        assert!(state.actions_state.active);
        assert_eq!(state.screen_mode, ScreenMode::DashboardActions);
        assert_eq!(state.actions_state.focus, ActionsFocus::RunList);
        assert!(
            state.actions_state.runs().is_empty(),
            "enter must clear the list"
        );

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
        state.actions_state.list.items_mut().clear();
        state
            .actions_state
            .list
            .items_mut()
            .extend_from_slice(&[run1.clone(), run2]);
        state.actions_state.list.set_selected_index(Some(0));

        state.apply_actions_message(ActionsMessage::Navigate(NavDir::Down));
        assert_eq!(state.actions_state.selected_run_index(), Some(1));
        assert!(!state.actions_state.loading.detail);
        assert!(state.actions_state.detail_pending.is_none());

        state.apply_actions_message(ActionsMessage::Navigate(NavDir::Up));
        assert_eq!(state.actions_state.selected_run_index(), Some(0));

        scroll_and_assert(&mut state, ScrollDir::Down, 0);
        scroll_and_assert(&mut state, ScrollDir::Up, 0);

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
        state.apply_actions_message(ActionsMessage::ScrollDetail(ScrollDir::PageDown));
        assert_eq!(
            state.actions_state.detail_scroll_offset,
            10.min(max),
            "PageDown advances by 10 then clamps at max"
        );
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
        let filter = ActionsFilter::default();

        start_reload(&mut state, 42);

        state.apply_actions_message(ActionsMessage::RunsLoadFailed {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            filter: Box::new(filter.clone()),
            page: 1,
            request_id: 42,
            error: "Failed".to_string(),
        });
        assert!(!state.actions_state.list_pending());
        assert_eq!(state.actions_state.error, Some("Failed".to_string()));

        // Success load
        start_reload(&mut state, 43);
        let run = make_run(1);
        state.apply_actions_message(ActionsMessage::RunsLoaded {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            filter: Box::new(filter),
            page: 1,
            request_id: 43,
            runs: vec![run],
            has_more: false,
        });
        assert!(!state.actions_state.list_pending());
        assert_eq!(state.actions_state.runs().len(), 1);
        assert_eq!(state.actions_state.selected_run_index(), Some(0));
    }

    #[test]
    fn test_filters() {
        let mut state = create_test_state();
        state.actions_state.draft_filter.workflow = "draft_wf".to_string();

        state.apply_actions_message(ActionsMessage::ApplyFilter);
        assert_eq!(state.actions_state.committed_filter.workflow, "draft_wf");
        assert!(!state.actions_state.ui.filter_ui_open);
        assert!(state.actions_state.list_pending());

        state.apply_actions_message(ActionsMessage::ClearFilter);
        assert_eq!(state.actions_state.committed_filter.workflow, "");
        assert_eq!(state.actions_state.draft_filter.workflow, "");

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

        state.actions_state.ui.filter_field_index = 0;
        state.apply_actions_message(ActionsMessage::CycleFilterStatus);
        assert_eq!(state.actions_state.draft_filter.workflow, "CI");
        assert_eq!(
            state.actions_state.draft_filter.workflow_path,
            ".github/workflows/ci.yml"
        );

        state.apply_actions_message(ActionsMessage::CycleFilterStatus);
        assert_eq!(state.actions_state.draft_filter.workflow, "Deploy");
        assert_eq!(
            state.actions_state.draft_filter.workflow_path,
            ".github/workflows/deploy.yml"
        );

        state.apply_actions_message(ActionsMessage::CycleFilterStatus);
        assert!(state.actions_state.draft_filter.workflow.is_empty());
        assert!(state.actions_state.draft_filter.workflow_path.is_empty());
    }

    #[test]
    fn cycle_workflow_filter_forward_uses_path() {
        let mut state = create_test_state();
        state.actions_state.workflows = vec![Workflow {
            id: 100,
            name: "CI".to_string(),
            path: ".github/workflows/ci.yml".to_string(),
            state: "active".to_string(),
        }];

        state.actions_state.ui.filter_field_index = 0;
        state.apply_actions_message(ActionsMessage::CycleFilterStatus);
        assert_eq!(state.actions_state.draft_filter.workflow, "CI");
        assert_eq!(
            state.actions_state.draft_filter.workflow_path,
            ".github/workflows/ci.yml"
        );
    }

    #[test]
    fn cycle_workflow_filter_empty_workflows_is_noop() {
        let mut state = create_test_state();
        state.actions_state.workflows = Vec::new();
        state.actions_state.ui.filter_field_index = 0;
        state.apply_actions_message(ActionsMessage::CycleFilterStatus);
        assert!(state.actions_state.draft_filter.workflow.is_empty());
        assert!(state.actions_state.draft_filter.workflow_path.is_empty());
    }

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

        assert_eq!(state.actions_state.committed_filter.workflow, "CI");
        assert_eq!(
            state.actions_state.committed_filter.workflow_path,
            ".github/workflows/ci.yml"
        );
    }

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
    fn run_list_enter_focuses_detail_when_a_run_is_selected() {
        let mut state = create_test_state();
        state.actions_state.list.items_mut().push(make_run(1));
        state.actions_state.list.set_selected_index(Some(0));
        state.actions_state.focus = ActionsFocus::RunList;

        state.apply_actions_message(ActionsMessage::Enter);

        assert_eq!(state.actions_state.focus, ActionsFocus::Detail);
    }

    #[test]
    fn load_detail_resets_expanded_jobs_and_focuses_first() {
        let mut state = create_test_state();
        state.actions_state.expanded_jobs.insert(999);
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
        assert!(state.actions_state.expanded_jobs.is_empty());
        assert_eq!(state.actions_state.focused_job_index, Some(0));
    }

    #[test]
    fn expand_job_is_idempotent() {
        let mut state = create_test_state();
        state.actions_state.run_detail = Some(make_detail_with_jobs());
        state.actions_state.focused_job_index = Some(0);

        state.apply_actions_message(ActionsMessage::ExpandJob);
        state.apply_actions_message(ActionsMessage::ExpandJob);

        assert!(state.actions_state.expanded_jobs.contains(&100));
    }

    #[test]
    fn navigate_job_moves_focus_and_scroll_follows_rendered_row() {
        let mut state = create_test_state();
        state.actions_state.run_detail = Some(make_detail_with_jobs());
        state.actions_state.focused_job_index = Some(0);
        state.actions_state.detail_viewport_rows = 2;
        state.actions_state.expanded_jobs.insert(100);

        state.apply_actions_message(ActionsMessage::NavigateJob(NavDir::Down));
        assert_eq!(state.actions_state.focused_job_index, Some(1));
        assert_eq!(
            state.actions_state.detail_scroll_offset, 4,
            "second job follows the two rendered steps of the expanded first job"
        );

        state.apply_actions_message(ActionsMessage::NavigateJob(NavDir::Down));
        assert_eq!(state.actions_state.focused_job_index, Some(1));

        state.apply_actions_message(ActionsMessage::NavigateJob(NavDir::Up));
        assert_eq!(state.actions_state.focused_job_index, Some(0));
        assert_eq!(state.actions_state.detail_scroll_offset, 2);
    }

    #[test]
    fn collapse_job_removes_from_expanded() {
        let mut state = create_test_state();
        state.actions_state.run_detail = Some(make_detail_with_jobs());
        state.actions_state.focused_job_index = Some(0);

        state.apply_actions_message(ActionsMessage::ExpandJob);
        state.apply_actions_message(ActionsMessage::CollapseJob);

        assert!(!state.actions_state.expanded_jobs.contains(&100));
    }

    #[test]
    fn detail_escape_collapses_then_refocuses_run_list() {
        let mut state = create_test_state();
        state.actions_state.focus = ActionsFocus::Detail;
        state.actions_state.run_detail = Some(make_detail_with_jobs());
        state.actions_state.focused_job_index = Some(0);
        state.actions_state.expanded_jobs.insert(100);

        state.apply_actions_message(ActionsMessage::DetailEscape);
        assert!(!state.actions_state.expanded_jobs.contains(&100));
        assert_eq!(state.actions_state.focus, ActionsFocus::Detail);

        state.apply_actions_message(ActionsMessage::DetailEscape);
        assert_eq!(state.actions_state.focus, ActionsFocus::RunList);
    }

    // ---- NEW: load-more reducer tests (issue #202) ----

    /// Page-1 reload replaces runs, selects first, and resets detail.
    #[test]
    fn page1_reload_replaces_and_selects_first() {
        let mut state = create_test_state();
        start_reload(&mut state, 1);
        state.apply_actions_message(ActionsMessage::RunsLoaded {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            filter: Box::new(ActionsFilter::default()),
            page: 1,
            request_id: 1,
            runs: vec![make_run(1), make_run(2)],
            has_more: true,
        });
        assert_eq!(state.actions_state.runs().len(), 2);
        assert_eq!(state.actions_state.selected_run_index(), Some(0));
        assert!(state.actions_state.run_detail.is_none());
        assert!(state.actions_state.has_more());
    }

    /// Page-2 (page-loaded) appends items.
    #[test]
    fn page2_append_grows_list() {
        let mut state = create_test_state();
        start_reload(&mut state, 1);
        state.apply_actions_message(ActionsMessage::RunsLoaded {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            filter: Box::new(ActionsFilter::default()),
            page: 1,
            request_id: 1,
            runs: vec![make_run(1)],
            has_more: true,
        });
        assert_eq!(state.actions_state.runs().len(), 1);

        // Begin a page load for page 2. The identity was set by the reload.
        let token = state.actions_state.list.next_page().clone();
        let req2 = alloc_req(&mut state.actions_state.list);
        state.actions_state.list.begin_page(token, req2);

        state.apply_actions_message(ActionsMessage::RunsPageLoaded {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            filter: Box::new(ActionsFilter::default()),
            page: 2,
            request_id: req2.get(),
            runs: vec![make_run(2), make_run(3)],
            has_more: false,
        });
        assert_eq!(state.actions_state.runs().len(), 3);
        assert!(!state.actions_state.has_more());
    }

    /// Stale page-2 is ignored.
    #[test]
    fn stale_page2_ignored() {
        let mut state = create_test_state();
        start_reload(&mut state, 1);
        state.apply_actions_message(ActionsMessage::RunsLoaded {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            filter: Box::new(ActionsFilter::default()),
            page: 1,
            request_id: 1,
            runs: vec![make_run(1)],
            has_more: true,
        });

        // Page-2 result with wrong request_id (stale).
        state.apply_actions_message(ActionsMessage::RunsPageLoaded {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            filter: Box::new(ActionsFilter::default()),
            page: 2,
            request_id: 999, // stale
            runs: vec![make_run(2)],
            has_more: false,
        });
        assert_eq!(
            state.actions_state.runs().len(),
            1,
            "stale page result must not append"
        );
    }

    /// Page-2 failure clears pending and permits retry.
    #[test]
    fn page2_failure_clears_pending_permits_retry() {
        let mut state = create_test_state();
        start_reload(&mut state, 1);
        state.apply_actions_message(ActionsMessage::RunsLoaded {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            filter: Box::new(ActionsFilter::default()),
            page: 1,
            request_id: 1,
            runs: vec![make_run(1)],
            has_more: true,
        });

        // Begin a page load.
        let token = state.actions_state.list.next_page().clone();
        let req2 = alloc_req(&mut state.actions_state.list);
        state.actions_state.list.begin_page(token, req2);
        assert!(state.actions_state.list_pending());

        state.apply_actions_message(ActionsMessage::RunsPageLoadFailed {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            filter: Box::new(ActionsFilter::default()),
            page: 2,
            request_id: req2.get(),
            error: "timeout".to_string(),
        });
        assert!(
            !state.actions_state.list_pending(),
            "page failure must clear pending"
        );
        assert!(state.actions_state.has_more(), "continuation preserved");
        assert_eq!(state.actions_state.runs().len(), 1, "rows preserved");
    }

    /// Terminal page stores Done — navigation at end does NOT re-dispatch.
    #[test]
    fn terminal_page_stores_done() {
        let mut state = create_test_state();
        start_reload(&mut state, 1);
        state.apply_actions_message(ActionsMessage::RunsLoaded {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            filter: Box::new(ActionsFilter::default()),
            page: 1,
            request_id: 1,
            runs: vec![make_run(1)],
            has_more: false,
        });
        assert!(!state.actions_state.has_more());
        let selected = state.actions_state.selected_run_index();
        assert!(!state.actions_state.list.should_load_more(selected));
    }

    /// Visible reload while page pending invalidates the page.
    #[test]
    fn visible_reload_supersedes_pending_page() {
        let mut state = create_test_state();
        start_reload(&mut state, 1);
        state.apply_actions_message(ActionsMessage::RunsLoaded {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            filter: Box::new(ActionsFilter::default()),
            page: 1,
            request_id: 1,
            runs: vec![make_run(1)],
            has_more: true,
        });

        // Begin a page load (page 2).
        let token = state.actions_state.list.next_page().clone();
        let req2 = alloc_req(&mut state.actions_state.list);
        state.actions_state.list.begin_page(token, req2);

        // A new visible reload (page 1) supersedes the pending page.
        let identity = ActionsListIdentity {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            filter: ActionsFilter::default(),
        };
        let req3 = alloc_req(&mut state.actions_state.list);
        state.actions_state.list.begin_reload(identity, req3);

        // The stale page-2 result is now rejected.
        state.apply_actions_message(ActionsMessage::RunsPageLoaded {
            scope_repo_id: RepositoryId("test_repo".to_string()),
            filter: Box::new(ActionsFilter::default()),
            page: 2,
            request_id: req2.get(),
            runs: vec![make_run(99)],
            has_more: false,
        });
        assert_eq!(
            state.actions_state.runs().len(),
            1,
            "stale page after reload must not append"
        );
    }

    // ── PR filter (issue #205) ────────────────────────────────────────────

    use crate::domain::{PrCheckStatus, PrState, PullRequest};

    fn make_pr(number: u64, head_sha: &str) -> PullRequest {
        PullRequest {
            number,
            title: format!("PR #{number}"),
            state: PrState::Open,
            author_login: "testuser".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            head_ref: "feature".to_string(),
            head_sha: head_sha.to_string(),
            base_ref: "main".to_string(),
            is_draft: false,
            review_decision: None,
            checks_status: PrCheckStatus::None,
            assignee_summary: String::new(),
            labels_summary: String::new(),
            comment_count: 0,
        }
    }

    fn state_with_prs(prs: Vec<PullRequest>) -> AppState {
        let mut state = create_test_state();
        state.prs_state.list.replace_items(prs);
        state.prs_state.list.set_selected_index(Some(0));
        state
    }

    #[test]
    fn enter_mode_with_pr_filter_sets_committed_and_draft() {
        let mut state = create_test_state();
        state.apply_actions_message(ActionsMessage::EnterModeWithPrFilter {
            pr_number: 42,
            head_sha: "abc123".to_string(),
        });
        assert!(state.actions_state.active);
        assert_eq!(state.actions_state.committed_filter.pr_number, Some(42));
        assert_eq!(
            state.actions_state.committed_filter.head_sha.as_deref(),
            Some("abc123")
        );
        assert_eq!(state.actions_state.draft_filter.pr_number, Some(42));
        assert_eq!(
            state.actions_state.draft_filter.head_sha.as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn cycle_pr_filter_forward_through_prs() {
        let mut state = state_with_prs(vec![
            make_pr(1, "sha1"),
            make_pr(2, "sha2"),
            make_pr(3, "sha3"),
        ]);
        state.actions_state.ui.filter_ui_open = true;
        state.actions_state.ui.filter_field_index = 2;

        // None → PR 1
        state.apply_actions_message(ActionsMessage::CycleFilterStatus);
        assert_eq!(state.actions_state.draft_filter.pr_number, Some(1));
        assert_eq!(
            state.actions_state.draft_filter.head_sha.as_deref(),
            Some("sha1")
        );

        // PR 1 → PR 2
        state.apply_actions_message(ActionsMessage::CycleFilterStatus);
        assert_eq!(state.actions_state.draft_filter.pr_number, Some(2));
        assert_eq!(
            state.actions_state.draft_filter.head_sha.as_deref(),
            Some("sha2")
        );

        // PR 2 → PR 3
        state.apply_actions_message(ActionsMessage::CycleFilterStatus);
        assert_eq!(state.actions_state.draft_filter.pr_number, Some(3));
        assert_eq!(
            state.actions_state.draft_filter.head_sha.as_deref(),
            Some("sha3")
        );

        // PR 3 → None (wraps)
        state.apply_actions_message(ActionsMessage::CycleFilterStatus);
        assert_eq!(state.actions_state.draft_filter.pr_number, None);
        assert!(state.actions_state.draft_filter.head_sha.is_none());
    }

    #[test]
    fn cycle_pr_filter_empty_prs_is_noop() {
        let mut state = create_test_state();
        state.actions_state.ui.filter_ui_open = true;
        state.actions_state.ui.filter_field_index = 2;

        state.apply_actions_message(ActionsMessage::CycleFilterStatus);
        assert_eq!(state.actions_state.draft_filter.pr_number, None);
    }

    #[test]
    fn clear_pr_filter_field() {
        let mut state = state_with_prs(vec![make_pr(1, "sha1")]);
        state.actions_state.ui.filter_ui_open = true;
        state.actions_state.ui.filter_field_index = 2;
        state.actions_state.draft_filter.pr_number = Some(1);
        state.actions_state.draft_filter.head_sha = Some("sha1".to_string());

        state.apply_actions_message(ActionsMessage::UpdateDraftFilter {
            field: crate::state::ActionsFilterField::Pr,
            value: String::new(),
        });

        assert_eq!(state.actions_state.draft_filter.pr_number, None);
        assert!(state.actions_state.draft_filter.head_sha.is_none());
    }

    #[test]
    fn filter_navigate_wraps_across_three_fields() {
        let mut state = create_test_state();
        state.actions_state.ui.filter_ui_open = true;
        state.actions_state.ui.filter_field_index = 2;

        // At index 2 (Pr), next wraps back to 0.
        state.apply_actions_message(ActionsMessage::FilterNavigateNext);
        assert_eq!(state.actions_state.ui.filter_field_index, 0);

        // From 0, prev wraps to 2.
        state.apply_actions_message(ActionsMessage::FilterNavigatePrev);
        assert_eq!(state.actions_state.ui.filter_field_index, 2);
    }
}
