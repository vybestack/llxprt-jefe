//! Issues mode state operations.
//! @plan PLAN-20260329-ISSUES-MODE.P05

use super::{
    AgentChooserState, AppEvent, AppState, ComposerTarget, DetailSubfocus,
    ISSUE_FILTER_FIELD_COUNT, InlineState, IssueFocus, PaneFocus, PrFocus, PriorAgentFocus,
    ScreenMode,
};
use crate::domain::{IssueFilter, IssueFilterState};
use crate::messages::IssuesMessage;

impl AppState {
    /// Enter issues mode, saving prior focus state.
    ///
    /// When entering from PR mode (cross-mode `i` key, issue #164), the PR
    /// mode is deactivated in-place: its per-repo preferences are snapshot
    /// and its overlays cleared, but `prior_agent_focus` is NOT restored to
    /// Dashboard (that would bounce the user out of list-mode UX). The
    /// exclusivity invariant holds: at most one of `issues_state.active` /
    /// `prs_state.active` is true after this call.
    fn enter_issues_mode(&mut self) {
        // Finding 1: deactivate PR mode if active so both list modes are
        // never simultaneously active (which would corrupt per-repo
        // preferences on a repo change).
        if self.prs_state.active {
            self.remember_pr_preferences();
            self.prs_state.active = false;
            self.prs_state.pr_focus = PrFocus::PrList;
            self.prs_state.inline_state = InlineState::None;
            self.prs_state.agent_chooser = None;
            self.prs_state.merge_chooser = None;
            self.prs_state.filter_ui.controls_open = false;
            self.prs_state.search_input_focused = false;
        }
        // Finding 1: only save prior_agent_focus if none exists yet, so a
        // Dashboard → Issues → PRs → Issues round-trip does not clobber the
        // original saved focus.
        if self.issues_state.prior_agent_focus.is_none() {
            self.issues_state.prior_agent_focus = Some(PriorAgentFocus {
                pane_focus: self.pane_focus,
                selected_repository_index: self.selected_repository_index,
                selected_agent_index: self.selected_agent_index,
            });
        }
        // Finding 2: normalize terminal-focus state so a cross-mode switch
        // from a terminal-focused PR view does not leave terminal capture
        // active in a list-mode render.
        self.terminal_focused = false;
        self.pane_focus = PaneFocus::Agents;
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
        self.restore_issue_preferences();
        self.issues_state.detail_subfocus = DetailSubfocus::Body;
        self.issues_state.draft_notice = None;
        self.issues_state.loading.list = true;
    }

    /// Restore the current repo's issue filter/search/field-index from
    /// per-repo preferences (issue #163). Falls back to Open defaults.
    fn restore_issue_preferences(&mut self) {
        let repo_id = self.current_repo_id();
        let prefs = match &repo_id {
            Some(id) => self.user_preferences.for_repo(id),
            None => crate::domain::RepoPreferences::default(),
        };
        self.issues_state.committed_filter = prefs.issue_filter;
        self.issues_state.draft_filter = self.issues_state.committed_filter.clone();
        self.issues_state.search_query = prefs.issue_search_query;
        // Clamp against the current field count so a stale/corrupted persisted
        // index cannot drive the cursor out of bounds (issue #163).
        self.issues_state.filter_ui.field_index = prefs
            .issue_filter_field_index
            .min(ISSUE_FILTER_FIELD_COUNT.saturating_sub(1));
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

    /// Scroll the issue detail pane so the currently-focused subfocus item is
    /// visible, using the pure `reveal_range_scroll_offset` helper and the
    /// fresh viewport row count. Only fires on a subfocus *change* (Tab/j/k),
    /// never on manual scroll ticks (#151).
    fn scroll_issue_detail_to_subfocus(&mut self) {
        let Some(detail) = &self.issues_state.issue_detail else {
            return;
        };
        let Some((item_start, item_end)) = crate::issue_detail_content::issue_subfocus_line_range(
            detail,
            self.issues_state.detail_subfocus,
            &self.issues_state.inline_state,
            self.issues_state.loading.comments,
        ) else {
            return;
        };
        let viewport = self.issues_detail_scroll_viewport_rows();
        let desired = crate::layout::reveal_range_scroll_offset(
            item_start,
            item_end,
            self.issues_state.detail_scroll_offset,
            viewport,
        );
        let max = self.issues_state.max_detail_scroll_offset();
        self.issues_state.detail_scroll_offset = desired.min(max);
    }

    /// Rows available to the read-only issue detail document after an embedded
    /// composer reserves rows. Mirrors the PR helper so the scroll-into-view
    /// clamp bound stays fresh.
    fn issues_detail_scroll_viewport_rows(&self) -> usize {
        let composer_active = matches!(
            self.issues_state.inline_state,
            InlineState::Composer {
                target: ComposerTarget::NewComment | ComposerTarget::Reply { .. },
                ..
            }
        );
        crate::layout::issue_detail_document_viewport_rows(
            self.issues_state.detail_viewport_rows,
            composer_active,
        )
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
        // Persist the OLD repo's filter/search/cursor before move_repo_selection
        // changes the selected index (issue #163). Idempotent on a no-op move.
        self.remember_issue_preferences();
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
        // Persist the OLD repo's filter/search/cursor before move_repo_selection
        // changes the selected index (issue #163). Idempotent on a no-op move.
        self.remember_issue_preferences();
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
        self.issues_state.delete_confirm = None;
        self.issues_state.close_mutation_pending = None;
        self.issues_state.delete_mutation_pending = None;
        self.restore_issue_preferences();
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
            AppEvent::IssuesEnter
                if self.issues_state.issue_focus == IssueFocus::IssueList
                    && self.issues_state.selected_issue_index.is_some() =>
            {
                self.issues_state.issue_focus = IssueFocus::IssueDetail;
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
                self.remember_issue_preferences();
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
        // A filter change reloads the list; dismiss the transient delete-confirm
        // overlay (it targets a specific list row that may no longer be present).
        // In-flight close/delete mutations are intentionally KEPT — their result
        // still carries the original scope+issue and matches the pending.
        self.issues_state.delete_confirm = None;
        self.issues_state.loading.list = true;
    }

    fn apply_issue_filter_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::OpenFilterControls => {
                self.issues_state.filter_ui.controls_open = true;
                // field_index is kept in sync with per-repo prefs by
                // restore_issue_preferences (mode entry) and
                // remember_issue_filter_field_index (navigation), so the live
                // value is already the persisted value; just clamp it.
                self.issues_state.filter_ui.field_index = self
                    .issues_state
                    .filter_ui
                    .field_index
                    .min(ISSUE_FILTER_FIELD_COUNT.saturating_sub(1));
                self.issues_state.filter_ui.draft_labels_text =
                    self.issues_state.draft_filter.labels.join(",");
            }
            AppEvent::CloseFilterControls => {
                self.issues_state.filter_ui.controls_open = false;
                self.remember_issue_filter_field_index();
            }
            AppEvent::ApplyFilter => {
                self.issues_state.committed_filter = self.issues_state.draft_filter.clone();
                self.issues_state.filter_ui.controls_open = false;
                self.reload_issue_list_for_filter_change();
                self.remember_issue_preferences();
            }
            AppEvent::ClearFilter => self.clear_issue_filter(),
            AppEvent::ClearDraftFilter => {
                self.issues_state.draft_filter = IssueFilter {
                    state: Some(IssueFilterState::Open),
                    ..IssueFilter::default()
                };
                self.issues_state.filter_ui.draft_labels_text.clear();
            }
            AppEvent::FilterNavigateNext => {
                let idx = self.issues_state.filter_ui.field_index;
                self.issues_state.filter_ui.field_index = (idx + 1) % ISSUE_FILTER_FIELD_COUNT;
                self.remember_issue_filter_field_index();
            }
            AppEvent::FilterNavigatePrev => {
                let idx = self.issues_state.filter_ui.field_index;
                self.issues_state.filter_ui.field_index =
                    (idx + ISSUE_FILTER_FIELD_COUNT - 1) % ISSUE_FILTER_FIELD_COUNT;
                self.remember_issue_filter_field_index();
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

    /// Reset the committed/draft filters to the Open default, clear the search
    /// query, and persist the result (issue #163).
    fn clear_issue_filter(&mut self) {
        self.issues_state.committed_filter = IssueFilter {
            state: Some(IssueFilterState::Open),
            ..IssueFilter::default()
        };
        self.issues_state.draft_filter = self.issues_state.committed_filter.clone();
        self.issues_state.filter_ui.draft_labels_text.clear();
        self.issues_state.filter_ui.controls_open = false;
        // Clearing all filters also clears the search query so the persisted
        // state stays consistent.
        self.issues_state.search_query.clear();
        self.issues_state.search_input_focused = false;
        self.reload_issue_list_for_filter_change();
        self.remember_issue_preferences();
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
        let agents = self.chooser_agents_for_repository(repo_id.as_ref());
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
            AppEvent::IssueDetailSubfocusNext => {
                self.detail_subfocus_next();
                self.scroll_issue_detail_to_subfocus();
            }
            AppEvent::IssueDetailSubfocusPrev => {
                self.detail_subfocus_prev();
                self.scroll_issue_detail_to_subfocus();
            }
            AppEvent::FocusSearchInput => self.issues_state.search_input_focused = true,
            AppEvent::BlurSearchInput => self.issues_state.search_input_focused = false,
            AppEvent::ClearSearch => {
                self.issues_state.search_query.clear();
                self.remember_issue_preferences();
            }
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
            | AppEvent::SendToAgentFailed { .. }
            | AppEvent::IssueSelfAssignmentFailed { .. } => self.apply_issues_error(event),
            _ => return false,
        }
        true
    }

    pub(super) fn apply_issues_event(&mut self, event: AppEvent) -> bool {
        self.apply_issue_scroll_event(&event)
            || self.apply_issue_lifecycle_event(event.clone())
            || self.apply_issue_close_delete_event(&event)
            || self.apply_issue_filter_event(event.clone())
            || self.apply_inline_open_event(event.clone())
            || self.apply_inline_event(event.clone())
            || self.apply_issue_mutation_event(event.clone())
            || self.apply_agent_chooser_event(event.clone())
            || self.apply_issue_error_event(event)
    }
}
