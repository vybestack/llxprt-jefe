//! Pull Requests mode state operations.
//!
//! @plan PLAN-20260624-PR-MODE.P05
//! @requirement REQ-PR-001
//! @requirement REQ-PR-003
//! @requirement REQ-PR-005
//! @requirement REQ-PR-006
//! @requirement REQ-PR-008
//! @requirement REQ-PR-009
//! @requirement REQ-PR-010
//! @requirement REQ-PR-012
//! @requirement REQ-PR-013

use super::{
    AgentChooserState, AppEvent, AppState, InlineState, PaneFocus, PrFocus, PriorAgentFocus,
    ReadOnlyHintKind, ScreenMode,
};
use crate::domain::{PrFilter, PrFilterState};
use crate::messages::PullRequestsMessage;

use crate::state::PR_FILTER_FIELD_COUNT;

impl AppState {
    /// Enter PR mode: save prior focus, set active, clear data, set default filter.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-001
    /// @pseudocode component-001 lines 66-76
    fn enter_prs_mode(&mut self) {
        self.prs_state.prior_agent_focus = Some(PriorAgentFocus {
            pane_focus: self.pane_focus,
            selected_repository_index: self.selected_repository_index,
            selected_agent_index: self.selected_agent_index,
        });
        self.screen_mode = ScreenMode::DashboardPullRequests;
        self.prs_state.active = true;
        self.prs_state.pr_focus = PrFocus::PrList;
        self.prs_state.list.clear();
        self.prs_state.pr_detail = None;
        self.prs_state.error = None;
        self.prs_state.loading.detail = false;
        self.prs_state.loading.comments = false;
        self.prs_state.detail_pending = None;
        self.prs_state.comments_page_pending = None;
        self.prs_state.inline_state = InlineState::None;
        self.prs_state.agent_chooser = None;
        self.prs_state.merge_chooser = None;
        self.prs_state.merge_mutation_pending = None;
        self.prs_state.filter_ui.controls_open = false;
        self.prs_state.search_input_focused = false;
        self.restore_pr_preferences();
        self.prs_state.detail_subfocus = super::PrDetailSubfocus::Body;
        self.prs_state.draft_notice = None;
        self.prs_state.mutation_pending = None;
    }

    /// Restore the current repo's PR filter/search/field-index from per-repo
    /// preferences (issue #163). Falls back to Open defaults for unknown repos.
    fn restore_pr_preferences(&mut self) {
        let repo_id = self.current_repo_id();
        let prefs = match &repo_id {
            Some(id) => self.user_preferences.for_repo(id),
            None => crate::domain::RepoPreferences::default(),
        };
        self.prs_state.committed_filter = prefs.pr_filter;
        self.prs_state.draft_filter = self.prs_state.committed_filter.clone();
        self.prs_state.search_query = prefs.pr_search_query;
        // Clamp against the current field count so a stale/corrupted persisted
        // index cannot drive the cursor out of bounds (issue #163).
        self.prs_state.filter_ui.field_index = prefs
            .pr_filter_field_index
            .min(PR_FILTER_FIELD_COUNT.saturating_sub(1));
    }

    /// Exit PR mode: restore prior focus with bounds fallback.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-005
    /// @pseudocode component-001 lines 77-87
    fn exit_prs_mode(&mut self) {
        self.screen_mode = ScreenMode::Dashboard;
        self.prs_state.active = false;
        if self.prs_state.inline_state != InlineState::None {
            self.prs_state.draft_notice = Some("Unsent draft discarded".to_string());
            self.prs_state.inline_state = InlineState::None;
        }
        if let Some(prior) = self.prs_state.prior_agent_focus.take() {
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

    /// Clear loaded PR data after a repo change (staleness invalidation).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @requirement REQ-PR-NFR-002
    /// @pseudocode component-001 lines 88-98
    pub(super) fn reset_prs_for_repo_change(&mut self) {
        if self.prs_state.inline_state != InlineState::None {
            self.prs_state.draft_notice = Some("Draft discarded (repo changed)".to_string());
            self.prs_state.inline_state = InlineState::None;
        }
        self.prs_state.list.clear();
        self.prs_state.pr_detail = None;
        self.prs_state.error = None;
        self.prs_state.loading.detail = false;
        self.prs_state.loading.comments = false;
        self.prs_state.detail_pending = None;
        self.prs_state.comments_page_pending = None;
        self.prs_state.detail_scroll_offset = 0;
        self.prs_state.detail_subfocus = super::PrDetailSubfocus::Body;
        self.prs_state.mutation_pending = None;
        self.prs_state.merge_chooser = None;
        self.prs_state.merge_mutation_pending = None;
        self.restore_pr_preferences();
        // Begin a fresh reload so `list_pending()` is observable before the
        // dispatch layer spawns the actual fetch (mirrors Actions).
        if let Some(repo_id) = self.selected_repository().map(|r| r.id.clone()) {
            self.begin_prs_reload(repo_id);
        }
    }

    // ---- Filter controls ----

    /// Apply filter-control events.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-008
    /// @pseudocode component-001 lines 249-274
    fn apply_pr_filter_event(&mut self, event: &AppEvent) -> bool {
        if self.apply_pr_filter_controls_event(event) {
            return true;
        }
        if self.apply_pr_filter_cycle_event(event) {
            return true;
        }
        if self.apply_pr_filter_draft_event(event) {
            return true;
        }
        self.apply_pr_filter_search_event(event)
    }

    /// Handle filter UI open/close/navigate events.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-008
    /// @pseudocode component-001 lines 249-274
    fn apply_pr_filter_controls_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::PrOpenFilterControls => {
                self.prs_state.filter_ui.controls_open = true;
                // field_index is kept in sync with per-repo prefs by
                // restore_pr_preferences (mode entry) and
                // remember_pr_filter_field_index (navigation), so the live
                // value is already the persisted value; just clamp it.
                self.prs_state.filter_ui.field_index = self
                    .prs_state
                    .filter_ui
                    .field_index
                    .min(PR_FILTER_FIELD_COUNT.saturating_sub(1));
                self.prs_state.draft_filter = self.prs_state.committed_filter.clone();
                true
            }
            AppEvent::PrCloseFilterControls => {
                self.prs_state.filter_ui.controls_open = false;
                self.remember_pr_filter_field_index();
                true
            }
            AppEvent::PrFilterNavigateNext => {
                self.prs_state.filter_ui.field_index =
                    (self.prs_state.filter_ui.field_index + 1) % PR_FILTER_FIELD_COUNT;
                self.remember_pr_filter_field_index();
                true
            }
            AppEvent::PrFilterNavigatePrev => {
                self.prs_state.filter_ui.field_index = if self.prs_state.filter_ui.field_index == 0
                {
                    PR_FILTER_FIELD_COUNT - 1
                } else {
                    self.prs_state.filter_ui.field_index - 1
                };
                self.remember_pr_filter_field_index();
                true
            }
            _ => false,
        }
    }

    /// Handle filter-value cycling and apply/clear events.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-008
    /// @pseudocode component-001 lines 249-274
    fn apply_pr_filter_cycle_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::PrCycleFilterState => {
                self.prs_state.draft_filter.state = Some(match self.prs_state.draft_filter.state {
                    Some(PrFilterState::Open) => PrFilterState::Closed,
                    Some(PrFilterState::Closed) => PrFilterState::Merged,
                    Some(PrFilterState::Merged) => PrFilterState::All,
                    Some(PrFilterState::All) | None => PrFilterState::Open,
                });
                true
            }
            AppEvent::PrCycleDraftFilter => {
                self.prs_state.draft_filter.is_draft = match self.prs_state.draft_filter.is_draft {
                    None => Some(true),
                    Some(true) => Some(false),
                    Some(false) => None,
                };
                true
            }
            AppEvent::PrCycleReviewFilter => {
                use crate::domain::ReviewDecisionFilter::{
                    Any, Approved, ChangesRequested, None, ReviewRequired,
                };
                self.prs_state.draft_filter.review_decision =
                    match self.prs_state.draft_filter.review_decision {
                        Any => Approved,
                        Approved => ChangesRequested,
                        ChangesRequested => ReviewRequired,
                        ReviewRequired => None,
                        None => Any,
                    };
                true
            }
            AppEvent::PrCycleChecksFilter => {
                use crate::domain::ChecksFilter::{Any, Failing, Pending, Success};
                self.prs_state.draft_filter.checks_status =
                    match self.prs_state.draft_filter.checks_status {
                        Any => Success,
                        Success => Failing,
                        Failing => Pending,
                        Pending => Any,
                    };
                true
            }
            AppEvent::PrApplyFilter => {
                self.prs_state.committed_filter = self.prs_state.draft_filter.clone();
                self.prs_state.filter_ui.controls_open = false;
                self.reload_pr_list_for_filter_change();
                self.remember_pr_preferences();
                true
            }
            AppEvent::PrClearFilter => {
                self.clear_pr_filter();
                true
            }
            _ => false,
        }
    }

    /// Reset the committed/draft PR filters to the Open default, clear the
    /// search query, and persist the result (issue #163).
    fn clear_pr_filter(&mut self) {
        self.prs_state.committed_filter = PrFilter::default();
        self.prs_state.committed_filter.state = Some(PrFilterState::Open);
        self.prs_state.draft_filter = self.prs_state.committed_filter.clone();
        // Clearing all filters also clears the search query so the persisted
        // state stays consistent.
        self.prs_state.search_query.clear();
        self.prs_state.search_input_focused = false;
        self.reload_pr_list_for_filter_change();
        self.remember_pr_preferences();
    }

    /// Handle draft filter field updates.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-008
    /// @pseudocode component-001 lines 249-274
    fn apply_pr_filter_draft_event(&mut self, event: &AppEvent) -> bool {
        if let AppEvent::PrUpdateDraftFilter { field, value } = event {
            let field = crate::messages::PrFilterField::from_string(field);
            match field {
                crate::messages::PrFilterField::Author => {
                    self.prs_state.draft_filter.author.clone_from(value);
                }
                crate::messages::PrFilterField::Assignee => {
                    self.prs_state.draft_filter.assignee.clone_from(value);
                }
                crate::messages::PrFilterField::Reviewer => {
                    self.prs_state.draft_filter.reviewer.clone_from(value);
                }
                crate::messages::PrFilterField::Labels => {
                    self.prs_state.draft_filter.labels = value
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                crate::messages::PrFilterField::Query => {
                    self.prs_state.draft_filter.query_text.clone_from(value);
                }
            }
            true
        } else {
            false
        }
    }

    /// Handle search input focus/query/apply/clear events.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-008
    /// @pseudocode component-001 lines 282-291
    fn apply_pr_filter_search_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::PrFocusSearchInput => {
                self.prs_state.search_input_focused = true;
                true
            }
            AppEvent::PrBlurSearchInput => {
                self.prs_state.search_input_focused = false;
                true
            }
            AppEvent::PrSetSearchQuery { query } => {
                self.prs_state.search_query.clone_from(query);
                true
            }
            AppEvent::PrApplySearch => {
                let trimmed = self.prs_state.search_query.trim().to_string();
                self.prs_state.committed_filter.query_text = trimmed;
                self.prs_state.search_input_focused = false;
                self.reload_pr_list_for_filter_change();
                self.remember_pr_preferences();
                true
            }
            AppEvent::PrClearSearch => {
                self.prs_state.search_query.clear();
                self.prs_state.committed_filter.query_text.clear();
                self.prs_state.search_input_focused = false;
                self.reload_pr_list_for_filter_change();
                self.remember_pr_preferences();
                true
            }
            _ => false,
        }
    }

    /// Reload PR list for a filter change: clear cursor + mark loading.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-008
    /// @pseudocode component-001 lines 275-281
    fn reload_pr_list_for_filter_change(&mut self) {
        self.prs_state.list.clear();
        // Begin a fresh reload so `list_pending()` is observable before the
        // dispatch layer spawns the actual fetch (mirrors Actions).
        if let Some(repo_id) = self.selected_repository().map(|r| r.id.clone()) {
            self.begin_prs_reload(repo_id);
        }
    }

    // ---- Agent chooser ----

    /// Apply agent-chooser events.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-011
    /// @pseudocode component-001 lines 331-340
    fn apply_pr_agent_chooser_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::PrOpenAgentChooser => {
                self.open_pr_agent_chooser();
                true
            }
            AppEvent::PrAgentChooserNavigateUp => {
                if let Some(chooser) = &mut self.prs_state.agent_chooser
                    && chooser.selected_index > 0
                {
                    chooser.selected_index -= 1;
                }
                true
            }
            AppEvent::PrAgentChooserNavigateDown => {
                if let Some(chooser) = &mut self.prs_state.agent_chooser {
                    let max = chooser.agents.len().saturating_sub(1);
                    if chooser.selected_index < max {
                        chooser.selected_index += 1;
                    }
                }
                true
            }
            AppEvent::PrAgentChooserConfirm | AppEvent::PrAgentChooserCancel => {
                self.prs_state.agent_chooser = None;
                true
            }
            AppEvent::PrSendToAgentCompleted => true,
            _ => false,
        }
    }

    /// Open the PR agent chooser (precondition: detail + no composer).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-011
    /// @pseudocode component-001 lines 331-340
    fn open_pr_agent_chooser(&mut self) {
        if self.prs_state.pr_focus != PrFocus::PrDetail
            || self.prs_state.inline_state != InlineState::None
        {
            return;
        }
        let repo_id = self.selected_repository_id().cloned();
        let agents = self.chooser_agents_for_repository(repo_id.as_ref());
        if agents.is_empty() {
            self.prs_state.draft_notice = Some("No agents available".to_string());
            return;
        }
        self.prs_state.agent_chooser = Some(AgentChooserState {
            selected_index: 0,
            agents,
        });
    }

    // ---- Read-only notice / open-in-browser ----

    /// Apply a PR notice event: set draft_notice from the hint kind.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @requirement REQ-PR-013
    /// @pseudocode component-001 lines 344-348
    fn apply_pr_notice_event(&mut self, event: &AppEvent) -> bool {
        if let AppEvent::PrShowNotice(kind) = event {
            self.apply_pr_show_notice(*kind);
            true
        } else {
            false
        }
    }

    /// Set draft_notice text for a read-only hint kind (no silent None).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @requirement REQ-PR-013
    /// @pseudocode component-001 lines 344-348
    pub(super) fn apply_pr_show_notice(&mut self, kind: ReadOnlyHintKind) {
        let text = match kind {
            ReadOnlyHintKind::ReadOnlyReplyOnComment => {
                "Select a comment to reply (read-only context)".to_string()
            }
            ReadOnlyHintKind::ReadOnlyNoComment => "No comments to reply to".to_string(),
            ReadOnlyHintKind::ReadOnlyNotEditable => "This section is read-only".to_string(),
            ReadOnlyHintKind::NoSelectionToOpen => "No pull request selected to open".to_string(),
            ReadOnlyHintKind::NoPrToMerge => "No pull request loaded to merge".to_string(),
            ReadOnlyHintKind::PrNotMergeable => {
                "Pull request is not mergeable (closed/merged)".to_string()
            }
            ReadOnlyHintKind::ReadOnlyResolveOnThread => {
                "Select a review thread to resolve (read-only context)".to_string()
            }
        };
        self.prs_state.draft_notice = Some(text);
    }

    /// Apply open-in-browser reducer half (pure: sets notice, no I/O).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-012
    /// @pseudocode component-001 lines 349-357,362-365
    fn apply_pr_open_browser_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::PrOpenInBrowser => {
                if self.prs_state.selected_pr_index().is_none() {
                    self.prs_state.draft_notice =
                        Some("No pull request selected to open".to_string());
                } else {
                    self.prs_state.draft_notice =
                        Some("Opening pull request in browser...".to_string());
                }
                true
            }
            AppEvent::PrOpenedInBrowser {
                scope_repo_id,
                pr_number,
            } => {
                if self.scope_repo_id_matches(scope_repo_id) {
                    self.prs_state.draft_notice =
                        Some(format!("Opened PR #{pr_number} in browser"));
                }
                true
            }
            AppEvent::PrOpenInBrowserFailed {
                scope_repo_id,
                pr_number: _,
                error,
            } => {
                if self.scope_repo_id_matches(scope_repo_id) {
                    self.prs_state.error = Some(format!("Failed to open PR in browser: {error}"));
                }
                true
            }
            _ => false,
        }
    }

    /// Check if a scope_repo_id matches the currently-selected repository.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-012
    /// @pseudocode component-001 lines 349-365
    fn scope_repo_id_matches(&self, scope_repo_id: &crate::domain::RepositoryId) -> bool {
        self.selected_repository_index
            .and_then(|idx| self.repositories.get(idx))
            .is_some_and(|repo| &repo.id == scope_repo_id)
    }

    // ---- Lifecycle events ----

    /// Apply lifecycle events (enter/exit/refocus).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-001
    /// @requirement REQ-PR-005
    /// @pseudocode component-001 lines 66-87
    fn apply_pr_lifecycle_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::EnterPrsMode => {
                self.enter_prs_mode();
                true
            }
            AppEvent::ExitPrsMode => {
                self.exit_prs_mode();
                true
            }
            AppEvent::RefocusPrList => {
                self.prs_state.pr_focus = PrFocus::PrList;
                true
            }
            _ => false,
        }
    }

    // ---- Message hubs ----

    /// Apply a PullRequestsMessage (mutating &mut self, returns handled).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-002
    /// @pseudocode component-001 lines 366-372, component-004 lines 70-83
    pub fn apply_prs_message(&mut self, message: PullRequestsMessage) -> bool {
        // ApplySearch is special-cased: it needs the state's search_query before conversion.
        if matches!(message, PullRequestsMessage::ApplySearch) {
            let trimmed = self.prs_state.search_query.trim().to_string();
            self.prs_state.committed_filter.query_text = trimmed;
            self.prs_state.search_input_focused = false;
            self.reload_pr_list_for_filter_change();
            self.remember_pr_preferences();
            return true;
        }
        let event: AppEvent = message.into();
        self.apply_prs_event(event)
    }

    /// Apply an AppEvent through the PR reducer chain (chained-OR).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-002
    /// @pseudocode component-001 lines 373-385
    pub fn apply_prs_event(&mut self, event: AppEvent) -> bool {
        self.apply_pr_navigation_event(&event)
            || self.apply_pr_lifecycle_event(&event)
            || self.apply_pr_filter_event(&event)
            || self.apply_pr_inline_open_event(event.clone())
            || self.apply_pr_inline_dispatch(&event)
            || self.apply_pr_mutation_event(event.clone())
            || self.apply_pr_agent_chooser_event(&event)
            || self.apply_pr_merge_event(&event)
            || self.apply_prs_data_wrapper(&event)
            || self.apply_prs_load_error_wrapper(&event)
            || self.apply_pr_thread_event(&event)
            || self.apply_pr_notice_event(&event)
            || self.apply_pr_open_browser_event(&event)
            || self.apply_pr_error_event(event)
    }

    /// Dispatch an inline event from its AppEvent form to PrInlineMsg.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @pseudocode component-001 lines 44-50
    fn apply_pr_inline_dispatch(&mut self, event: &AppEvent) -> bool {
        use crate::messages::PrInlineMsg;
        let msg = match event {
            AppEvent::PrInlineChar(c) => PrInlineMsg::Char(*c),
            AppEvent::PrInlineNewline => PrInlineMsg::Newline,
            AppEvent::PrInlineBackspace => PrInlineMsg::Backspace,
            AppEvent::PrInlineDelete => PrInlineMsg::Delete,
            AppEvent::PrInlineCursorLeft => PrInlineMsg::CursorLeft,
            AppEvent::PrInlineCursorRight => PrInlineMsg::CursorRight,
            AppEvent::PrInlineCursorUp => PrInlineMsg::CursorUp,
            AppEvent::PrInlineCursorDown => PrInlineMsg::CursorDown,
            AppEvent::PrInlineSubmit => PrInlineMsg::Submit,
            AppEvent::PrInlineCancelOrEsc => PrInlineMsg::CancelOrEsc,
            _ => return false,
        };
        self.apply_pr_inline_event(msg)
    }

    /// Wrapper for data events that returns bool for the dispatch chain.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-007
    /// @pseudocode component-001 lines 21-27,209-241
    fn apply_prs_data_wrapper(&mut self, event: &AppEvent) -> bool {
        let handled = matches!(
            event,
            AppEvent::PrListLoaded { .. }
                | AppEvent::PrListPageLoaded { .. }
                | AppEvent::PrListSilentRefreshed { .. }
                | AppEvent::PrDetailLoaded { .. }
                | AppEvent::PrDetailSilentRefreshed { .. }
                | AppEvent::PrCommentsPageLoaded { .. }
        );
        if handled {
            self.apply_prs_data(event.clone());
        }
        handled
    }

    /// Wrapper for load-error events that returns bool for the dispatch chain.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-002
    /// @pseudocode component-001 lines 242-247
    fn apply_prs_load_error_wrapper(&mut self, event: &AppEvent) -> bool {
        let handled = matches!(
            event,
            AppEvent::PrListLoadFailed { .. }
                | AppEvent::PrListSilentRefreshFailed { .. }
                | AppEvent::PrDetailLoadFailed { .. }
                | AppEvent::PrDetailSilentRefreshFailed { .. }
                | AppEvent::PrCommentsPageFailed { .. }
        );
        if handled {
            self.apply_prs_load_error(event.clone());
        }
        handled
    }
}
