//! Issues mode state operations.
//! @plan PLAN-20260329-ISSUES-MODE.P05

use super::{
    AgentChooserState, AppEvent, AppState, DetailSubfocus, ISSUE_FILTER_FIELD_COUNT, InlineState,
    IssueFocus, PaneFocus, PriorAgentFocus, ScreenMode,
};
use crate::domain::{IssueFilter, IssueFilterState};
use crate::messages::IssuesMessage;

impl AppState {
    /// Enter issues mode, saving prior focus state.
    fn enter_issues_mode(&mut self) {
        self.issues_state.prior_agent_focus = Some(PriorAgentFocus {
            pane_focus: self.pane_focus,
            selected_repository_index: self.selected_repository_index,
            selected_agent_index: self.selected_agent_index,
        });
        self.screen_mode = ScreenMode::DashboardIssues;
        self.issues_state.active = true;
        self.issues_state.issue_focus = IssueFocus::IssueList;
        self.issues_state.issues.clear();
        self.issues_state.selected_issue_index = None;
        self.issues_state.issue_detail = None;
        self.issues_state.list_cursor = None;
        self.issues_state.has_more_issues = false;
        self.issues_state.error = None;
        self.issues_state.loading.comments = false;
        self.issues_state.comments_page_pending = None;
        self.issues_state.list_reload_pending = None;
        self.issues_state.list_page_pending = None;
        self.issues_state.detail_pending = None;
        self.issues_state.inline_state = InlineState::None;
        self.issues_state.agent_chooser = None;
        self.issues_state.filter_ui.controls_open = false;
        self.issues_state.search_input_focused = false;
        self.issues_state.search_query.clear();
        self.issues_state.detail_subfocus = DetailSubfocus::Body;
        self.issues_state.draft_notice = None;
        self.issues_state.loading.list = true;
    }

    /// Exit issues mode, restoring prior focus state.
    fn exit_issues_mode(&mut self) {
        self.screen_mode = ScreenMode::Dashboard;
        self.issues_state.active = false;
        if self.issues_state.inline_state != InlineState::None {
            self.issues_state.draft_notice = Some("Unsent draft discarded".to_string());
            self.issues_state.inline_state = InlineState::None;
        }
        if let Some(prior) = self.issues_state.prior_agent_focus.take() {
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

    fn detail_subfocus_next(&mut self) {
        if let Some(detail) = &self.issues_state.issue_detail {
            let has_comments = !detail.comments.is_empty();
            let comment_count = detail.comments.len();
            self.issues_state.detail_subfocus = match self.issues_state.detail_subfocus {
                DetailSubfocus::Body => {
                    if has_comments {
                        DetailSubfocus::Comment(0)
                    } else {
                        DetailSubfocus::NewComment
                    }
                }
                DetailSubfocus::Comment(i) => {
                    if i + 1 < comment_count {
                        DetailSubfocus::Comment(i + 1)
                    } else {
                        DetailSubfocus::NewComment
                    }
                }
                DetailSubfocus::NewComment => DetailSubfocus::Body,
            };
        }
    }

    fn detail_subfocus_prev(&mut self) {
        if let Some(detail) = &self.issues_state.issue_detail {
            let has_comments = !detail.comments.is_empty();
            let comment_count = detail.comments.len();
            self.issues_state.detail_subfocus = match self.issues_state.detail_subfocus {
                DetailSubfocus::Body => DetailSubfocus::NewComment,
                DetailSubfocus::Comment(0) => DetailSubfocus::Body,
                DetailSubfocus::Comment(i) => DetailSubfocus::Comment(i - 1),
                DetailSubfocus::NewComment => {
                    if has_comments {
                        DetailSubfocus::Comment(comment_count - 1)
                    } else {
                        DetailSubfocus::Body
                    }
                }
            };
        }
    }

    /// Navigate to the previous repository in issues mode.
    ///
    /// Thin wrapper over the shared `move_repo_selection` helper (Finding 5);
    /// independent of `pane_focus` (#47).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @pseudocode component-001 lines 152-153
    fn navigate_repo_up_in_issues_mode(&mut self) {
        if self.move_repo_selection(crate::messages::NavDir::Up) {
            self.reset_issues_for_repo_change();
        }
    }

    /// Navigate to the next repository in issues mode.
    ///
    /// Thin wrapper over the shared `move_repo_selection` helper (Finding 5);
    /// independent of `pane_focus` (#47).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @pseudocode component-001 lines 152-153
    fn navigate_repo_down_in_issues_mode(&mut self) {
        if self.move_repo_selection(crate::messages::NavDir::Down) {
            self.reset_issues_for_repo_change();
        }
    }

    /// Clear loaded issues data after a repo change in issues mode.
    pub(super) fn reset_issues_for_repo_change(&mut self) {
        if self.issues_state.inline_state != InlineState::None {
            self.issues_state.draft_notice = Some("Unsent draft discarded".to_string());
            self.issues_state.inline_state = InlineState::None;
        }
        self.issues_state.issues.clear();
        self.issues_state.selected_issue_index = None;
        self.issues_state.issue_detail = None;
        self.issues_state.list_cursor = None;
        self.issues_state.has_more_issues = false;
        self.issues_state.error = None;
        self.issues_state.loading.list = true;
        self.issues_state.loading.detail = false;
        self.issues_state.loading.comments = false;
        self.issues_state.list_reload_pending = None;
        self.issues_state.list_page_pending = None;
        self.issues_state.detail_pending = None;
        self.issues_state.comments_page_pending = None;
    }

    fn navigate_issue_list_up(&mut self) {
        let previous = self.issues_state.selected_issue_index;
        if let Some(idx) = previous
            && idx > 0
        {
            self.issues_state.selected_issue_index = Some(idx - 1);
        }
        self.invalidate_detail_requests_if_issue_selection_changed(previous);
    }

    fn navigate_issue_list_down(&mut self) {
        let previous = self.issues_state.selected_issue_index;
        if let Some(idx) = previous
            && idx + 1 < self.issues_state.issues.len()
        {
            self.issues_state.selected_issue_index = Some(idx + 1);
        }
        self.invalidate_detail_requests_if_issue_selection_changed(previous);
    }

    fn navigate_issue_list_page_up(&mut self) {
        let previous = self.issues_state.selected_issue_index;
        if let Some(idx) = previous {
            self.issues_state.selected_issue_index = Some(idx.saturating_sub(10));
        }
        self.invalidate_detail_requests_if_issue_selection_changed(previous);
    }

    fn navigate_issue_list_page_down(&mut self) {
        let previous = self.issues_state.selected_issue_index;
        if let Some(idx) = previous {
            let max = self.issues_state.issues.len().saturating_sub(1);
            self.issues_state.selected_issue_index = Some((idx + 10).min(max));
        }
        self.invalidate_detail_requests_if_issue_selection_changed(previous);
    }

    fn navigate_issue_list_home(&mut self) {
        let previous = self.issues_state.selected_issue_index;
        if !self.issues_state.issues.is_empty() {
            self.issues_state.selected_issue_index = Some(0);
        }
        self.invalidate_detail_requests_if_issue_selection_changed(previous);
    }

    fn navigate_issue_list_end(&mut self) {
        let previous = self.issues_state.selected_issue_index;
        if !self.issues_state.issues.is_empty() {
            self.issues_state.selected_issue_index = Some(self.issues_state.issues.len() - 1);
        }
        self.invalidate_detail_requests_if_issue_selection_changed(previous);
    }

    fn invalidate_detail_requests_if_issue_selection_changed(&mut self, previous: Option<usize>) {
        if self.issues_state.selected_issue_index == previous {
            return;
        }
        self.issues_state.loading.detail = false;
        self.issues_state.loading.comments = false;
        self.issues_state.detail_pending = None;
        self.issues_state.comments_page_pending = None;
        self.issues_state.detail_scroll_offset = 0;
    }

    fn cycle_issues_focus(&mut self) {
        self.issues_state.issue_focus = match self.issues_state.issue_focus {
            IssueFocus::RepoList => IssueFocus::IssueList,
            IssueFocus::IssueList => IssueFocus::IssueDetail,
            IssueFocus::IssueDetail => IssueFocus::RepoList,
        };
    }

    fn cycle_issues_focus_reverse(&mut self) {
        self.issues_state.issue_focus = match self.issues_state.issue_focus {
            IssueFocus::RepoList => IssueFocus::IssueDetail,
            IssueFocus::IssueList => IssueFocus::RepoList,
            IssueFocus::IssueDetail => IssueFocus::IssueList,
        };
    }

    /// Handle issues navigation and focus events.
    fn apply_issues_navigation(&mut self, event: AppEvent) {
        match event {
            AppEvent::IssuesNavigateUp => match self.issues_state.issue_focus {
                IssueFocus::IssueList => self.navigate_issue_list_up(),
                IssueFocus::RepoList => self.navigate_repo_up_in_issues_mode(),
                IssueFocus::IssueDetail => {}
            },
            AppEvent::IssuesNavigateDown => match self.issues_state.issue_focus {
                IssueFocus::IssueList => self.navigate_issue_list_down(),
                IssueFocus::RepoList => self.navigate_repo_down_in_issues_mode(),
                IssueFocus::IssueDetail => {}
            },
            AppEvent::IssuesNavigatePageUp => self.navigate_issue_list_page_up(),
            AppEvent::IssuesNavigatePageDown => self.navigate_issue_list_page_down(),
            AppEvent::IssuesNavigateHome => self.navigate_issue_list_home(),
            AppEvent::IssuesNavigateEnd => self.navigate_issue_list_end(),
            AppEvent::IssuesEnter => {
                if self.issues_state.issue_focus == IssueFocus::IssueList
                    && self.issues_state.selected_issue_index.is_some()
                {
                    self.issues_state.issue_focus = IssueFocus::IssueDetail;
                }
            }
            AppEvent::IssuesCycleFocus => self.cycle_issues_focus(),
            AppEvent::IssuesCycleFocusReverse => self.cycle_issues_focus_reverse(),
            _ => {}
        }
    }

    pub(crate) fn apply_issue_mutation_error(&mut self, event: AppEvent) {
        match event {
            AppEvent::CommentCreateFailed {
                scope_repo_id,
                issue_number,
                mutation_id,
                error,
            } => self.apply_scoped_mutation_error(
                &scope_repo_id,
                Some(issue_number),
                Some(mutation_id),
                error,
            ),
            AppEvent::MutationFailed {
                scope_repo_id,
                issue_number,
                mutation_id,
                error,
            } => self.apply_scoped_mutation_error(&scope_repo_id, issue_number, mutation_id, error),
            _ => {}
        }
    }

    /// Handle all issues-mode `AppEvent` variants.
    ///
    /// Returns `true` if the event was handled, `false` if it was not an
    /// issues-mode event (caller should handle it).
    pub(super) fn apply_issues_message(&mut self, message: IssuesMessage) -> bool {
        match message {
            IssuesMessage::ApplySearch => {
                self.issues_state.committed_filter.query_text =
                    self.issues_state.search_query.trim().to_string();
                self.issues_state.search_input_focused = false;
                self.issues_state.issues.clear();
                self.issues_state.selected_issue_index = None;
                self.issues_state.issue_detail = None;
                self.issues_state.list_cursor = None;
                self.issues_state.has_more_issues = false;
                self.issues_state.error = None;
                self.issues_state.loading.list = true;
                self.issues_state.loading.detail = false;
                self.issues_state.loading.comments = false;
                self.issues_state.detail_pending = None;
                self.issues_state.comments_page_pending = None;
                self.issues_state.list_reload_pending = None;
                self.issues_state.list_page_pending = None;
                self.issues_state.mutation_pending = None;
                self.issues_state.inline_state = InlineState::None;
                true
            }
            message => self.apply_issues_event(message.into()),
        }
    }

    fn apply_issue_scroll_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::IssuesScrollDetailUp => {
                self.issues_state.detail_scroll_offset =
                    self.issues_state.detail_scroll_offset.saturating_sub(1);
            }
            AppEvent::IssuesScrollDetailDown => {
                let max = self.issues_state.max_detail_scroll_offset();
                if self.issues_state.detail_scroll_offset < max {
                    self.issues_state.detail_scroll_offset += 1;
                }
            }
            AppEvent::IssuesScrollDetailPageUp => {
                self.issues_state.detail_scroll_offset =
                    self.issues_state.detail_scroll_offset.saturating_sub(10);
            }
            AppEvent::IssuesScrollDetailPageDown => {
                let max = self.issues_state.max_detail_scroll_offset();
                self.issues_state.detail_scroll_offset =
                    (self.issues_state.detail_scroll_offset + 10).min(max);
            }
            _ => return false,
        }
        true
    }

    fn reload_issue_list_for_filter_change(&mut self) {
        self.issues_state.issues.clear();
        self.issues_state.selected_issue_index = None;
        self.issues_state.issue_detail = None;
        self.issues_state.list_cursor = None;
        self.issues_state.has_more_issues = false;
        self.issues_state.loading.detail = false;
        self.issues_state.loading.comments = false;
        self.issues_state.detail_pending = None;
        self.issues_state.comments_page_pending = None;
        self.issues_state.list_reload_pending = None;
        self.issues_state.list_page_pending = None;
        self.issues_state.loading.list = true;
    }

    fn apply_issue_filter_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::OpenFilterControls => {
                self.issues_state.filter_ui.controls_open = true;
                self.issues_state.filter_ui.field_index = 0;
                self.issues_state.filter_ui.draft_labels_text =
                    self.issues_state.draft_filter.labels.join(",");
            }
            AppEvent::CloseFilterControls => self.issues_state.filter_ui.controls_open = false,
            AppEvent::ApplyFilter => {
                self.issues_state.committed_filter = self.issues_state.draft_filter.clone();
                self.issues_state.filter_ui.controls_open = false;
                self.reload_issue_list_for_filter_change();
            }
            AppEvent::ClearFilter => {
                self.issues_state.committed_filter = IssueFilter::default();
                self.issues_state.draft_filter = IssueFilter::default();
                self.issues_state.filter_ui.draft_labels_text.clear();
                self.issues_state.filter_ui.controls_open = false;
                self.reload_issue_list_for_filter_change();
            }
            AppEvent::ClearDraftFilter => {
                self.issues_state.draft_filter = IssueFilter::default();
                self.issues_state.filter_ui.draft_labels_text.clear();
            }
            AppEvent::FilterNavigateNext => {
                let idx = self.issues_state.filter_ui.field_index;
                self.issues_state.filter_ui.field_index = (idx + 1) % ISSUE_FILTER_FIELD_COUNT;
            }
            AppEvent::FilterNavigatePrev => {
                let idx = self.issues_state.filter_ui.field_index;
                self.issues_state.filter_ui.field_index =
                    (idx + ISSUE_FILTER_FIELD_COUNT - 1) % ISSUE_FILTER_FIELD_COUNT;
            }
            AppEvent::CycleFilterState => {
                let current = self.issues_state.draft_filter.state;
                self.issues_state.draft_filter.state = Some(match current {
                    Some(IssueFilterState::Open) | None => IssueFilterState::Closed,
                    Some(IssueFilterState::Closed) => IssueFilterState::All,
                    Some(IssueFilterState::All) => IssueFilterState::Open,
                });
            }
            _ => return false,
        }
        true
    }

    fn apply_agent_chooser_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::OpenAgentChooser => self.open_agent_chooser(),
            AppEvent::AgentChooserNavigateUp => {
                if let Some(chooser) = &mut self.issues_state.agent_chooser
                    && chooser.selected_index > 0
                {
                    chooser.selected_index -= 1;
                }
            }
            AppEvent::AgentChooserNavigateDown => {
                if let Some(chooser) = &mut self.issues_state.agent_chooser
                    && chooser.selected_index + 1 < chooser.agents.len()
                {
                    chooser.selected_index += 1;
                }
            }
            AppEvent::AgentChooserConfirm
            | AppEvent::AgentChooserCancel
            | AppEvent::SendToAgentCompleted => self.issues_state.agent_chooser = None,
            _ => return false,
        }
        true
    }

    fn open_agent_chooser(&mut self) {
        let repo_id = self.selected_repository_id().cloned();
        let agents: Vec<_> = self
            .agents
            .iter()
            .filter(|a| {
                repo_id.as_ref().is_some_and(|rid| a.repository_id == *rid) && !a.is_running()
            })
            .map(|a| (a.id.clone(), a.name.clone()))
            .collect();
        if !agents.is_empty() {
            self.issues_state.agent_chooser = Some(AgentChooserState {
                selected_index: 0,
                agents,
            });
        }
    }

    fn apply_issue_lifecycle_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::EnterIssuesMode => self.enter_issues_mode(),
            AppEvent::ExitIssuesMode => self.exit_issues_mode(),
            AppEvent::RefocusIssueList => self.issues_state.issue_focus = IssueFocus::IssueList,
            AppEvent::IssuesNavigateUp
            | AppEvent::IssuesNavigateDown
            | AppEvent::IssuesNavigatePageUp
            | AppEvent::IssuesNavigatePageDown
            | AppEvent::IssuesNavigateHome
            | AppEvent::IssuesNavigateEnd
            | AppEvent::IssuesEnter
            | AppEvent::IssuesCycleFocus
            | AppEvent::IssuesCycleFocusReverse => self.apply_issues_navigation(event),
            AppEvent::IssueDetailSubfocusNext => self.detail_subfocus_next(),
            AppEvent::IssueDetailSubfocusPrev => self.detail_subfocus_prev(),
            AppEvent::FocusSearchInput => self.issues_state.search_input_focused = true,
            AppEvent::BlurSearchInput => self.issues_state.search_input_focused = false,
            AppEvent::ClearSearch => self.issues_state.search_query.clear(),
            AppEvent::IssueListLoaded { .. }
            | AppEvent::IssueListPageLoaded { .. }
            | AppEvent::IssueDetailLoaded { .. }
            | AppEvent::IssueCommentsPageLoaded { .. }
            | AppEvent::SetSearchQuery { .. }
            | AppEvent::UpdateDraftFilter { .. } => self.apply_issues_data(event),
            _ => return false,
        }
        true
    }

    fn apply_issue_error_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::IssueListLoadFailed { .. }
            | AppEvent::IssueDetailLoadFailed { .. }
            | AppEvent::IssueCommentsPageFailed { .. }
            | AppEvent::CommentCreateFailed { .. }
            | AppEvent::MutationFailed { .. }
            | AppEvent::SendToAgentFailed { .. } => self.apply_issues_error(event),
            _ => return false,
        }
        true
    }

    pub(super) fn apply_issues_event(&mut self, event: AppEvent) -> bool {
        self.apply_issue_scroll_event(&event)
            || self.apply_issue_lifecycle_event(event.clone())
            || self.apply_issue_filter_event(event.clone())
            || self.apply_inline_open_event(event.clone())
            || self.apply_inline_event(event.clone())
            || self.apply_issue_mutation_event(event.clone())
            || self.apply_agent_chooser_event(event.clone())
            || self.apply_issue_error_event(event)
    }
}
