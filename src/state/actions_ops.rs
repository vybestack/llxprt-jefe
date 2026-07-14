//! Actions mode reducer operations.
//!
//! All list-load correlation is delegated to `PaginatedList` — the reducer
//! only constructs identity/result values, delegates, and applies
//! screen-specific side effects (error, detail reset) based on the returned
//! `AcceptOutcome`.

use super::{
    ActionsFocus, AppState, ModalState, PaneFocus, PriorAgentFocus, ScreenMode,
    actions_load_ops::RunsLoadData,
};
use crate::domain::{ActionsFilter, RepositoryId};
use crate::messages::ActionsMessage;

/// Number of navigable fields in the Actions filter bar (workflow, status, pr).
/// Mirrors the field-count assumption used by `FilterNavigateNext/Prev`.
const ACTIONS_FILTER_FIELD_COUNT: usize = 3;

impl AppState {
    /// Enter actions mode, saving prior focus state.
    fn enter_actions_mode(&mut self) -> bool {
        self.actions_state.prior_agent_focus = Some(PriorAgentFocus {
            pane_focus: self.pane_focus,
            selected_repository_index: self.selected_repository_index,
            selected_agent_index: self.selected_agent_index,
        });
        self.screen_mode = ScreenMode::DashboardActions;
        self.actions_state.active = true;
        self.actions_state.focus = ActionsFocus::RunList;
        self.actions_state.list.clear();
        self.actions_state.run_detail = None;
        self.actions_state.workflows.clear();
        self.actions_state.committed_filter = ActionsFilter::default();
        self.actions_state.draft_filter = ActionsFilter::default();
        self.actions_state.ui.filter_ui_open = false;
        self.actions_state.search_query.clear();
        self.actions_state.ui.search_input_focused = false;
        self.actions_state.error = None;
        self.actions_state.loading.detail = false;
        self.actions_state.dispatch_pending = None;
        self.actions_state.detail_pending = None;
        self.actions_state.workflows_pending = None;
        self.actions_state.expanded_jobs.clear();
        self.actions_state.focused_job_index = None;
        true
    }

    /// Enter Actions mode with a PR filter pre-set (cross-mode action from
    /// PR mode — issue #205). The PR filter is applied to both the committed
    /// and draft filters so the initial load is immediately narrowed.
    fn enter_actions_mode_with_pr_filter(&mut self, pr_number: u64, head_sha: String) -> bool {
        self.enter_actions_mode();
        self.actions_state.committed_filter.pr_number = Some(pr_number);
        self.actions_state.committed_filter.head_sha = Some(head_sha.clone());
        self.actions_state.draft_filter.pr_number = Some(pr_number);
        self.actions_state.draft_filter.head_sha = Some(head_sha);
        true
    }

    /// Exit actions mode, restoring prior focus state.
    fn exit_actions_mode(&mut self) {
        self.screen_mode = ScreenMode::Dashboard;
        self.actions_state.active = false;
        if let Some(prior) = self.actions_state.prior_agent_focus.take() {
            self.pane_focus = prior.pane_focus;
            if let Some(idx) = prior.selected_agent_index {
                if idx < self.agents.len() {
                    self.selected_agent_index = Some(idx);
                } else {
                    self.pane_focus = PaneFocus::Agents;
                    self.selected_agent_index = if self.agents.is_empty() {
                        None
                    } else {
                        Some(0)
                    };
                }
            }
            if let Some(idx) = prior.selected_repository_index
                && idx < self.repositories.len()
            {
                self.selected_repository_index = Some(idx);
            }
        } else {
            self.pane_focus = PaneFocus::Agents;
        }
    }

    fn refocus_list(&mut self) -> bool {
        self.actions_state.focus = ActionsFocus::RunList;
        true
    }

    fn handle_navigation(&mut self, dir: crate::messages::NavDir) -> bool {
        if matches!(self.actions_state.focus, ActionsFocus::RepoList)
            && matches!(
                dir,
                crate::messages::NavDir::Up | crate::messages::NavDir::Down
            )
        {
            if self.move_repo_selection(dir) {
                self.reset_actions_for_repo_change();
            }
            return true;
        }
        let runs = self.actions_state.list.items();
        if runs.is_empty() {
            return true;
        }
        let current = self.actions_state.list.selected_index().unwrap_or(0);
        let last = runs.len() - 1;
        let new_idx = match dir {
            crate::messages::NavDir::Up => current.saturating_sub(1),
            crate::messages::NavDir::Down => (current + 1).min(last),
            crate::messages::NavDir::PageUp => current.saturating_sub(super::VIEWPORT_PAGE_JUMP),
            crate::messages::NavDir::PageDown => (current + super::VIEWPORT_PAGE_JUMP).min(last),
            crate::messages::NavDir::Home => 0,
            crate::messages::NavDir::End => last,
            crate::messages::NavDir::Next | crate::messages::NavDir::Prev => current,
        };
        self.actions_state.list.set_selected_index(Some(new_idx));
        self.actions_state.detail_scroll_offset = 0;
        self.actions_state.run_detail = None;
        self.actions_state.loading.detail = false;
        self.actions_state.detail_pending = None;
        self.actions_state.expanded_jobs.clear();
        self.actions_state.focused_job_index = None;
        true
    }

    /// Clear loaded Actions data after a repository change (RepoList focus
    /// navigation), mirroring `reset_issues_for_repo_change`. The next list
    /// load is queued via `trigger_list_reload`.
    fn reset_actions_for_repo_change(&mut self) {
        self.actions_state.list.clear();
        self.actions_state.run_detail = None;
        self.actions_state.detail_scroll_offset = 0;
        self.actions_state.expanded_jobs.clear();
        self.actions_state.focused_job_index = None;
        self.actions_state.workflows.clear();
        self.actions_state.committed_filter = ActionsFilter::default();
        self.actions_state.draft_filter = ActionsFilter::default();
        self.actions_state.search_query.clear();
        self.actions_state.error = None;
        self.actions_state.detail_pending = None;
        self.actions_state.workflows_pending = None;
        self.actions_state.loading.detail = false;
        self.trigger_list_reload();
    }

    fn handle_enter(&mut self) -> bool {
        if matches!(self.actions_state.focus, ActionsFocus::RunList)
            && self.actions_state.list.selected_index().is_some()
        {
            self.actions_state.focus = ActionsFocus::Detail;
        }
        true
    }

    fn cycle_focus(&mut self) -> bool {
        self.actions_state.focus = match self.actions_state.focus {
            ActionsFocus::RepoList => ActionsFocus::RunList,
            ActionsFocus::RunList => ActionsFocus::Detail,
            ActionsFocus::Detail => ActionsFocus::RepoList,
        };
        true
    }
    fn cycle_focus_reverse(&mut self) -> bool {
        self.actions_state.focus = match self.actions_state.focus {
            ActionsFocus::RepoList => ActionsFocus::Detail,
            ActionsFocus::RunList => ActionsFocus::RepoList,
            ActionsFocus::Detail => ActionsFocus::RunList,
        };
        true
    }

    /// Maximum scroll offset for the Actions detail pane, derived from the
    /// detail's line count (mirroring `issues_ops.rs` clamping logic).
    fn max_detail_scroll_offset(&self) -> usize {
        let Some(detail) = &self.actions_state.run_detail else {
            return 0;
        };
        let lines =
            crate::actions_view::detail_line_count(detail, &self.actions_state.expanded_jobs);
        let viewport = if self.actions_state.detail_viewport_rows == 0 {
            crate::layout::detail_viewport_rows(40)
        } else {
            self.actions_state.detail_viewport_rows
        };
        lines.saturating_sub(viewport)
    }

    /// Public accessor for the Actions detail max scroll offset (used by mouse
    /// routing to clamp wheel-scrolling).
    #[must_use]
    pub fn actions_max_detail_scroll_offset(&self) -> usize {
        self.max_detail_scroll_offset()
    }

    fn handle_scroll_detail(&mut self, dir: crate::messages::ScrollDir) -> bool {
        let max = self.max_detail_scroll_offset();
        let current = self.actions_state.detail_scroll_offset.min(max);
        match dir {
            crate::messages::ScrollDir::Up => {
                self.actions_state.detail_scroll_offset = current.saturating_sub(1);
            }
            crate::messages::ScrollDir::Down => {
                self.actions_state.detail_scroll_offset = (current + 1).min(max);
            }
            crate::messages::ScrollDir::PageUp => {
                self.actions_state.detail_scroll_offset =
                    current.saturating_sub(super::VIEWPORT_PAGE_JUMP);
            }
            crate::messages::ScrollDir::PageDown => {
                self.actions_state.detail_scroll_offset =
                    (current + super::VIEWPORT_PAGE_JUMP).min(max);
            }
        }
        true
    }

    /// Toggle the expand/collapse state of the currently focused job.
    fn toggle_job_expand(&mut self) -> bool {
        let Some(detail) = &self.actions_state.run_detail else {
            return true;
        };
        let Some(idx) = self.actions_state.focused_job_index else {
            return true;
        };
        let Some(job) = detail.jobs.get(idx) else {
            return true;
        };
        if self.actions_state.expanded_jobs.contains(&job.id) {
            self.actions_state.expanded_jobs.remove(&job.id);
        } else {
            self.actions_state.expanded_jobs.insert(job.id);
        }
        true
    }

    /// Collapse the currently focused job (Left/Esc in detail pane).
    fn collapse_job(&mut self) -> bool {
        let Some(detail) = &self.actions_state.run_detail else {
            return true;
        };
        let Some(idx) = self.actions_state.focused_job_index else {
            return true;
        };
        if let Some(job) = detail.jobs.get(idx) {
            self.actions_state.expanded_jobs.remove(&job.id);
        }
        true
    }

    /// Move the focused job up/down within the detail pane.
    fn navigate_job(&mut self, dir: crate::messages::NavDir) -> bool {
        let Some(detail) = &self.actions_state.run_detail else {
            return true;
        };
        if detail.jobs.is_empty() {
            return true;
        }
        let current = self.actions_state.focused_job_index.unwrap_or(0);
        let new_idx = match dir {
            crate::messages::NavDir::Up => current.saturating_sub(1),
            crate::messages::NavDir::Down => (current + 1).min(detail.jobs.len() - 1),
            _ => current,
        };
        self.actions_state.focused_job_index = Some(new_idx);
        true
    }

    fn load_detail(
        &mut self,
        scope_repo_id: RepositoryId,
        run_id: u64,
        request_id: u64,
        detail: Box<crate::domain::WorkflowRunDetail>,
    ) -> bool {
        if let Some(pending) = &self.actions_state.detail_pending
            && pending.scope_repo_id == scope_repo_id
            && pending.run_id == run_id
            && pending.request_id == request_id
        {
            let job_count = detail.jobs.len();
            self.actions_state.run_detail = Some(*detail);
            self.actions_state.detail_scroll_offset = 0;
            self.actions_state.loading.detail = false;
            self.actions_state.detail_pending = None;
            self.actions_state.error = None;
            self.actions_state.expanded_jobs.clear();
            self.actions_state.focused_job_index = if job_count == 0 { None } else { Some(0) };
        }
        true
    }

    fn fail_detail_load(
        &mut self,
        scope_repo_id: RepositoryId,
        run_id: u64,
        request_id: u64,
        error: String,
    ) -> bool {
        if let Some(pending) = &self.actions_state.detail_pending
            && pending.scope_repo_id == scope_repo_id
            && pending.run_id == run_id
            && pending.request_id == request_id
        {
            self.actions_state.error = Some(error);
            self.actions_state.loading.detail = false;
            self.actions_state.detail_pending = None;
        }
        true
    }

    fn load_workflows(
        &mut self,
        scope_repo_id: RepositoryId,
        request_id: u64,
        workflows: Vec<crate::domain::Workflow>,
    ) -> bool {
        if let Some(pending) = &self.actions_state.workflows_pending
            && pending.scope_repo_id == scope_repo_id
            && pending.request_id == request_id
        {
            self.actions_state.workflows = workflows;
            self.actions_state.workflows_pending = None;
            self.actions_state.error = None;
        }
        true
    }

    fn fail_workflows_load(
        &mut self,
        scope_repo_id: RepositoryId,
        request_id: u64,
        error: String,
    ) -> bool {
        if let Some(pending) = &self.actions_state.workflows_pending
            && pending.scope_repo_id == scope_repo_id
            && pending.request_id == request_id
        {
            self.actions_state.error = Some(error);
            self.actions_state.workflows_pending = None;
        }
        true
    }

    fn apply_filter(&mut self) -> bool {
        self.actions_state.committed_filter = self.actions_state.draft_filter.clone();
        self.actions_state.ui.filter_ui_open = false;
        self.trigger_list_reload();
        true
    }

    fn clear_filter(&mut self) -> bool {
        self.actions_state.committed_filter = ActionsFilter::default();
        self.actions_state.draft_filter = ActionsFilter::default();
        self.actions_state.ui.filter_ui_open = false;
        self.trigger_list_reload();
        true
    }

    fn update_draft_filter(&mut self, field: super::ActionsFilterField, value: String) -> bool {
        match field {
            super::ActionsFilterField::Workflow => {
                self.actions_state.draft_filter.workflow = value;
            }
            super::ActionsFilterField::Status => {
                self.actions_state.draft_filter.status = value;
            }
            super::ActionsFilterField::Pr => {
                // PR filter is cycled, not typed — ClearCurrent sends empty string.
                if value.is_empty() {
                    self.actions_state.draft_filter.pr_number = None;
                    self.actions_state.draft_filter.head_sha = None;
                }
            }
        }
        true
    }

    /// Commit the search query into the committed filter and trigger a reload.
    fn apply_search(&mut self) -> bool {
        let query = self.actions_state.search_query.trim().to_string();
        self.actions_state.committed_filter.search = query;
        self.actions_state.ui.search_input_focused = false;
        self.trigger_list_reload()
    }

    /// Clear the committed search filter and trigger a reload.
    fn clear_search(&mut self) -> bool {
        self.actions_state.search_query.clear();
        self.actions_state.committed_filter.search.clear();
        self.actions_state.ui.search_input_focused = false;
        self.trigger_list_reload()
    }
    /// is active.
    fn cycle_status_filter(&mut self, forward: bool) -> bool {
        const ORDER: [&str; 5] = ["all", "completed", "failed", "in_progress", "queued"];
        let current = self.actions_state.draft_filter.status.as_str();
        let idx = ORDER.iter().position(|&s| s == current).unwrap_or(0);
        let len = ORDER.len();
        let new_idx = if forward {
            (idx + 1) % len
        } else {
            (idx + len - 1) % len
        };
        self.actions_state.draft_filter.status = ORDER[new_idx].to_string();
        true
    }

    /// Core workflow filter cycling logic shared by next/prev.
    ///
    /// Cycles the draft filter through "all" → wf1 → wf2 → … → "all".
    /// Sets both `workflow` (display name for UI) and `workflow_path` (the
    /// file path used for the GitHub API call — the display name would 404).
    fn cycle_workflow_filter(&mut self, forward: bool) -> bool {
        let workflows = &self.actions_state.workflows;
        if workflows.is_empty() {
            return true;
        }
        let current_path = self.actions_state.draft_filter.workflow_path.as_str();
        let paths: Vec<&str> = std::iter::once("all")
            .chain(workflows.iter().map(|w| w.path.as_str()))
            .collect();
        let idx = paths.iter().position(|&p| p == current_path).unwrap_or(0);
        let len = paths.len();
        let new_idx = if forward {
            (idx + 1) % len
        } else {
            (idx + len - 1) % len
        };
        if new_idx == 0 {
            self.actions_state.draft_filter.workflow.clear();
            self.actions_state.draft_filter.workflow_path.clear();
        } else {
            let wf = &workflows[new_idx - 1];
            self.actions_state.draft_filter.workflow = wf.name.clone();
            self.actions_state.draft_filter.workflow_path = wf.path.clone();
        }
        true
    }

    /// Core PR filter cycling logic shared by next/prev.
    ///
    /// Cycles the draft filter through None → PR1 → PR2 → … → None (issue #205).
    /// Sets both `pr_number` (display) and `head_sha` (API head_sha= param) so
    /// the runs query narrows to the selected PR's head commit.
    fn cycle_pr_filter(&mut self, forward: bool) -> bool {
        let prs = self.prs_state.pull_requests();
        if prs.is_empty() {
            return true;
        }
        let current = self.actions_state.draft_filter.pr_number;
        let numbers: Vec<Option<u64>> = std::iter::once(None)
            .chain(prs.iter().map(|pr| Some(pr.number)))
            .collect();
        let idx = numbers.iter().position(|&n| n == current).unwrap_or(0);
        let len = numbers.len();
        let new_idx = if forward {
            (idx + 1) % len
        } else {
            (idx + len - 1) % len
        };
        if new_idx == 0 {
            self.actions_state.draft_filter.pr_number = None;
            self.actions_state.draft_filter.head_sha = None;
        } else {
            let pr = &prs[new_idx - 1];
            self.actions_state.draft_filter.pr_number = Some(pr.number);
            self.actions_state.draft_filter.head_sha = Some(pr.head_sha.clone());
        }
        true
    }

    fn open_workflow_dispatch(&mut self, workflow: crate::domain::Workflow) -> bool {
        let ref_name = if let Some(detail) = &self.actions_state.run_detail {
            detail.run.head_branch.clone()
        } else if let Some(idx) = self.actions_state.list.selected_index()
            && idx < self.actions_state.list.items().len()
        {
            self.actions_state.list.items()[idx].head_branch.clone()
        } else {
            String::new()
        };

        self.modal = ModalState::WorkflowDispatch {
            workflow,
            fields: super::WorkflowDispatchFormFields {
                ref_name,
                inputs: String::new(),
            },
            focus: super::WorkflowDispatchFormFocus::default(),
            cursor: super::WorkflowDispatchFormCursor::default(),
        };
        true
    }

    fn workflow_dispatch_submitted(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        workflow_id: String,
    ) -> bool {
        self.modal = ModalState::None;
        let request_id = self
            .actions_state
            .next_dispatch_request_id
            .saturating_add(1);
        self.actions_state.next_dispatch_request_id = request_id;
        self.actions_state.dispatch_pending = Some(super::ActionsDispatchPending {
            scope_repo_id,
            workflow_id,
            request_id,
        });
        true
    }

    fn workflow_dispatch_success(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        request_id: u64,
    ) -> bool {
        if let Some(pending) = &self.actions_state.dispatch_pending
            && pending.scope_repo_id == scope_repo_id
            && pending.request_id == request_id
        {
            self.actions_state.dispatch_pending = None;
            self.begin_actions_reload(scope_repo_id);
        }
        true
    }

    fn workflow_dispatch_failed(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        request_id: u64,
        error: String,
    ) -> bool {
        if let Some(pending) = &self.actions_state.dispatch_pending
            && pending.scope_repo_id == scope_repo_id
            && pending.request_id == request_id
        {
            self.actions_state.dispatch_pending = None;
            self.actions_state.error = Some(error);
        }
        true
    }

    fn handle_mode_message(&mut self, message: &ActionsMessage) -> bool {
        match message {
            ActionsMessage::EnterMode => self.enter_actions_mode(),
            ActionsMessage::EnterModeWithPrFilter {
                pr_number,
                head_sha,
            } => self.enter_actions_mode_with_pr_filter(*pr_number, head_sha.clone()),
            ActionsMessage::ExitMode => {
                self.exit_actions_mode();
                true
            }
            ActionsMessage::Reload => {
                self.actions_state.error = None;
                true
            }
            ActionsMessage::RefocusList => self.refocus_list(),
            ActionsMessage::Navigate(dir) => self.handle_navigation(*dir),
            ActionsMessage::Enter => self.handle_enter(),
            ActionsMessage::CycleFocus => self.cycle_focus(),
            ActionsMessage::CycleFocusReverse => self.cycle_focus_reverse(),
            ActionsMessage::ScrollDetail(dir) => self.handle_scroll_detail(*dir),
            ActionsMessage::ToggleJobExpand => self.toggle_job_expand(),
            ActionsMessage::CollapseJob => self.collapse_job(),
            ActionsMessage::NavigateJob(dir) => self.navigate_job(*dir),
            _ => false,
        }
    }

    fn handle_load_message(&mut self, message: &ActionsMessage) -> bool {
        if self.handle_runs_message(message) {
            return true;
        }
        if self.handle_detail_message(message) {
            return true;
        }
        false
    }

    /// Handle runs list load/failure messages (reload, page, failures).
    fn handle_runs_message(&mut self, message: &ActionsMessage) -> bool {
        match message {
            ActionsMessage::RunsLoaded {
                scope_repo_id,
                filter,
                page,
                request_id,
                runs,
                has_more,
            } => self.reload_runs(RunsLoadData {
                scope_repo_id: scope_repo_id.clone(),
                filter: (**filter).clone(),
                page: *page,
                request_id: *request_id,
                runs: runs.clone(),
                has_more: *has_more,
            }),
            ActionsMessage::RunsLoadFailed {
                scope_repo_id,
                filter,
                request_id,
                error,
                ..
            } => self.fail_runs_load(
                scope_repo_id.clone(),
                (**filter).clone(),
                *request_id,
                error.clone(),
            ),
            ActionsMessage::RunsPageLoaded {
                scope_repo_id,
                filter,
                page,
                request_id,
                runs,
                has_more,
            } => self.apply_runs_page_loaded(RunsLoadData {
                scope_repo_id: scope_repo_id.clone(),
                filter: (**filter).clone(),
                page: *page,
                request_id: *request_id,
                runs: runs.clone(),
                has_more: *has_more,
            }),
            ActionsMessage::RunsPageLoadFailed {
                scope_repo_id,
                filter,
                page,
                request_id,
                error,
            } => self.fail_runs_page_load(
                scope_repo_id.clone(),
                (**filter).clone(),
                *page,
                *request_id,
                error.clone(),
            ),
            _ => false,
        }
    }

    /// Handle detail and workflow load/failure messages.
    fn handle_detail_message(&mut self, message: &ActionsMessage) -> bool {
        match message {
            ActionsMessage::DetailLoaded {
                scope_repo_id,
                run_id,
                request_id,
                detail,
            } => self.load_detail(scope_repo_id.clone(), *run_id, *request_id, detail.clone()),
            ActionsMessage::DetailLoadFailed {
                scope_repo_id,
                run_id,
                request_id,
                error,
            } => self.fail_detail_load(scope_repo_id.clone(), *run_id, *request_id, error.clone()),
            ActionsMessage::WorkflowsLoaded {
                scope_repo_id,
                request_id,
                workflows,
            } => self.load_workflows(scope_repo_id.clone(), *request_id, workflows.clone()),
            ActionsMessage::WorkflowsLoadFailed {
                scope_repo_id,
                request_id,
                error,
            } => self.fail_workflows_load(scope_repo_id.clone(), *request_id, error.clone()),
            _ => false,
        }
    }

    fn handle_filter_message(&mut self, message: &ActionsMessage) -> bool {
        match message {
            ActionsMessage::OpenFilterControls => {
                self.actions_state.ui.filter_ui_open = true;
                self.actions_state.ui.filter_field_index = 0;
                true
            }
            ActionsMessage::CloseFilterControls => {
                self.actions_state.ui.filter_ui_open = false;
                true
            }
            ActionsMessage::ApplyFilter => self.apply_filter(),
            ActionsMessage::ClearFilter => self.clear_filter(),
            ActionsMessage::ClearDraftFilter => {
                self.actions_state.draft_filter = self.actions_state.committed_filter.clone();
                true
            }
            ActionsMessage::FilterNavigateNext => {
                let idx = self.actions_state.ui.filter_field_index;
                self.actions_state.ui.filter_field_index = (idx + 1) % ACTIONS_FILTER_FIELD_COUNT;
                true
            }
            ActionsMessage::FilterNavigatePrev => {
                let idx = self.actions_state.ui.filter_field_index;
                self.actions_state.ui.filter_field_index =
                    (idx + ACTIONS_FILTER_FIELD_COUNT - 1) % ACTIONS_FILTER_FIELD_COUNT;
                true
            }
            ActionsMessage::CycleFilterStatus => {
                if self.actions_state.ui.filter_field_index == 0 {
                    self.cycle_workflow_filter(true)
                } else if self.actions_state.ui.filter_field_index == 1 {
                    self.cycle_status_filter(true)
                } else {
                    self.cycle_pr_filter(true)
                }
            }
            ActionsMessage::FocusSearchInput => {
                self.actions_state.ui.search_input_focused = true;
                true
            }
            ActionsMessage::BlurSearchInput => {
                self.actions_state.ui.search_input_focused = false;
                true
            }
            ActionsMessage::ApplySearch => self.apply_search(),
            ActionsMessage::SetSearchQuery { query } => {
                self.actions_state.search_query.clone_from(query);
                true
            }
            ActionsMessage::ClearSearch => self.clear_search(),
            ActionsMessage::UpdateDraftFilter { field, value } => {
                self.update_draft_filter(*field, value.clone())
            }
            _ => false,
        }
    }

    fn handle_dispatch_message(&mut self, message: ActionsMessage) -> bool {
        match message {
            ActionsMessage::OpenWorkflowDispatch(workflow) => self.open_workflow_dispatch(workflow),
            ActionsMessage::CloseWorkflowDispatch => {
                self.modal = ModalState::None;
                true
            }
            ActionsMessage::WorkflowDispatchSubmitted {
                scope_repo_id,
                workflow_id,
                ..
            } => self.workflow_dispatch_submitted(scope_repo_id.clone(), workflow_id.clone()),
            ActionsMessage::WorkflowDispatchSuccess {
                scope_repo_id,
                request_id,
            } => self.workflow_dispatch_success(scope_repo_id.clone(), request_id),
            ActionsMessage::WorkflowDispatchFailed {
                scope_repo_id,
                request_id,
                error,
            } => self.workflow_dispatch_failed(scope_repo_id.clone(), request_id, error.clone()),
            _ => false,
        }
    }

    /// Handle all Actions events.
    pub(super) fn apply_actions_message(&mut self, message: ActionsMessage) -> bool {
        if self.handle_mode_message(&message) {
            return true;
        }
        if self.handle_load_message(&message) {
            return true;
        }
        if self.handle_filter_message(&message) {
            return true;
        }
        self.handle_dispatch_message(message)
    }
}
