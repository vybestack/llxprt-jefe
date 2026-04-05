//! Issues mode state operations.
//!
//! Extracted from mod.rs to keep file sizes manageable.
//! @plan PLAN-20260329-ISSUES-MODE.P05

use super::{
    AgentChooserState, AppEvent, AppState, ComposerTarget, DetailSubfocus, EditorTarget,
    InlineState, IssueFocus, PaneFocus, PriorAgentFocus, ScreenMode, inline_cursor_vertical,
};
use crate::domain::IssueFilter;

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
        self.issues_state.inline_state = InlineState::None;
        self.issues_state.agent_chooser = None;
        self.issues_state.filter_controls_open = false;
        self.issues_state.search_input_focused = false;
        self.issues_state.search_query.clear();
        self.issues_state.detail_subfocus = DetailSubfocus::Body;
        self.issues_state.draft_notice = None;
        self.issues_state.list_loading = true;
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

    /// Handle inline editor/composer text manipulation events.
    /// Returns `true` if the event was handled.
    #[allow(clippy::too_many_lines)]
    fn apply_inline_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::InlineChar(c) => match &mut self.issues_state.inline_state {
                InlineState::Composer { text, cursor, .. }
                | InlineState::Editor { text, cursor, .. } => {
                    text.insert(*cursor, c);
                    *cursor += c.len_utf8();
                }
                InlineState::None => {}
            },
            AppEvent::InlineNewline => match &mut self.issues_state.inline_state {
                InlineState::Composer { text, cursor, .. }
                | InlineState::Editor { text, cursor, .. } => {
                    text.insert(*cursor, char::from(0x0Au8));
                    *cursor += 1;
                }
                InlineState::None => {}
            },
            AppEvent::InlineBackspace => match &mut self.issues_state.inline_state {
                InlineState::Composer { text, cursor, .. }
                | InlineState::Editor { text, cursor, .. } => {
                    if *cursor > 0 {
                        let prev = text[..*cursor].chars().last().map_or(0, char::len_utf8);
                        text.drain((*cursor - prev)..*cursor);
                        *cursor -= prev;
                    }
                }
                InlineState::None => {}
            },
            AppEvent::InlineDelete => match &mut self.issues_state.inline_state {
                InlineState::Composer { text, cursor, .. }
                | InlineState::Editor { text, cursor, .. } => {
                    if *cursor < text.len() {
                        let next = text[*cursor..].chars().next().map_or(0, char::len_utf8);
                        text.drain(*cursor..(*cursor + next));
                    }
                }
                InlineState::None => {}
            },
            AppEvent::InlineCursorLeft => match &mut self.issues_state.inline_state {
                InlineState::Composer { text, cursor, .. }
                | InlineState::Editor { text, cursor, .. } => {
                    if *cursor > 0 {
                        let prev = text[..*cursor].chars().last().map_or(0, char::len_utf8);
                        *cursor -= prev;
                    }
                }
                InlineState::None => {}
            },
            AppEvent::InlineCursorRight => match &mut self.issues_state.inline_state {
                InlineState::Composer { text, cursor, .. }
                | InlineState::Editor { text, cursor, .. } => {
                    if *cursor < text.len() {
                        let next = text[*cursor..].chars().next().map_or(0, char::len_utf8);
                        *cursor += next;
                    }
                }
                InlineState::None => {}
            },
            AppEvent::InlineCursorUp => match &mut self.issues_state.inline_state {
                InlineState::Composer { text, cursor, .. }
                | InlineState::Editor { text, cursor, .. } => {
                    inline_cursor_vertical(text, cursor, -1);
                }
                InlineState::None => {}
            },
            AppEvent::InlineCursorDown => match &mut self.issues_state.inline_state {
                InlineState::Composer { text, cursor, .. }
                | InlineState::Editor { text, cursor, .. } => {
                    inline_cursor_vertical(text, cursor, 1);
                }
                InlineState::None => {}
            },
            AppEvent::InlineCancelOrEsc => {
                self.issues_state.inline_state = InlineState::None;
            }
            _ => return false,
        }
        true
    }

    /// Open a reply composer for the given comment.
    fn open_reply_composer(&mut self, comment_index: usize) {
        if self.issues_state.inline_state == InlineState::None {
            let author = self
                .issues_state
                .issue_detail
                .as_ref()
                .and_then(|d| d.comments.get(comment_index))
                .map(|c| format!("@{} ", c.author_login))
                .unwrap_or_default();
            let cursor = author.len();
            self.issues_state.inline_state = InlineState::Composer {
                target: ComposerTarget::Reply {
                    comment_index,
                    author: author.clone(),
                },
                text: author,
                cursor,
            };
        }
    }

    /// Open an inline editor for the given target (issue body or comment).
    fn open_inline_editor(&mut self, target: EditorTarget) {
        if self.issues_state.inline_state == InlineState::None {
            let text = match &target {
                EditorTarget::IssueBody => self
                    .issues_state
                    .issue_detail
                    .as_ref()
                    .map(|d| d.body.clone())
                    .unwrap_or_default(),
                EditorTarget::Comment { comment_index } => self
                    .issues_state
                    .issue_detail
                    .as_ref()
                    .and_then(|d| d.comments.get(*comment_index))
                    .map(|c| c.body.clone())
                    .unwrap_or_default(),
            };
            let cursor = text.len();
            self.issues_state.inline_state = InlineState::Editor {
                target,
                text,
                cursor,
            };
        }
    }

    /// Advance detail subfocus to the next element.
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

    /// Move detail subfocus to the previous element.
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
    /// Unlike `handle_navigate_up` (which checks `pane_focus`), this always
    /// navigates the repo list and resets issues state for the new scope.
    fn navigate_repo_up_in_issues_mode(&mut self) {
        let visible_repo_indices = self.visible_repository_indices();
        let selected_visible_idx = self.selected_repository_visible_index();
        if let Some(visible_idx) = selected_visible_idx.filter(|&idx| idx > 0) {
            self.remember_selected_agent_for_current_repo();
            self.selected_repository_index = Some(visible_repo_indices[visible_idx - 1]);
            self.restore_selected_agent_for_current_repo();
            self.reset_issues_for_repo_change();
        }
    }

    /// Navigate to the next repository in issues mode.
    ///
    /// Unlike `handle_navigate_down` (which checks `pane_focus`), this always
    /// navigates the repo list and resets issues state for the new scope.
    fn navigate_repo_down_in_issues_mode(&mut self) {
        let visible_repo_indices = self.visible_repository_indices();
        let selected_visible_idx = self.selected_repository_visible_index();
        if let Some(visible_idx) = selected_visible_idx
            && visible_idx + 1 < visible_repo_indices.len()
        {
            self.remember_selected_agent_for_current_repo();
            self.selected_repository_index = Some(visible_repo_indices[visible_idx + 1]);
            self.restore_selected_agent_for_current_repo();
            self.reset_issues_for_repo_change();
        }
    }

    /// Clear loaded issues data after a repo change in issues mode.
    fn reset_issues_for_repo_change(&mut self) {
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
        self.issues_state.list_loading = true;
    }

    /// Handle issues navigation and focus events.
    #[allow(clippy::too_many_lines)]
    fn apply_issues_navigation(&mut self, event: AppEvent) {
        match event {
            AppEvent::IssuesNavigateUp => match self.issues_state.issue_focus {
                IssueFocus::IssueList => {
                    if let Some(idx) = self.issues_state.selected_issue_index
                        && idx > 0
                    {
                        self.issues_state.selected_issue_index = Some(idx - 1);
                    }
                }
                IssueFocus::RepoList => self.navigate_repo_up_in_issues_mode(),
                IssueFocus::IssueDetail => {}
            },
            AppEvent::IssuesNavigateDown => match self.issues_state.issue_focus {
                IssueFocus::IssueList => {
                    if let Some(idx) = self.issues_state.selected_issue_index
                        && idx + 1 < self.issues_state.issues.len()
                    {
                        self.issues_state.selected_issue_index = Some(idx + 1);
                    }
                }
                IssueFocus::RepoList => self.navigate_repo_down_in_issues_mode(),
                IssueFocus::IssueDetail => {}
            },
            AppEvent::IssuesNavigatePageUp => {
                if let Some(idx) = self.issues_state.selected_issue_index {
                    self.issues_state.selected_issue_index = Some(idx.saturating_sub(10));
                }
            }
            AppEvent::IssuesNavigatePageDown => {
                if let Some(idx) = self.issues_state.selected_issue_index {
                    let max = self.issues_state.issues.len().saturating_sub(1);
                    self.issues_state.selected_issue_index = Some((idx + 10).min(max));
                }
            }
            AppEvent::IssuesNavigateHome => {
                if !self.issues_state.issues.is_empty() {
                    self.issues_state.selected_issue_index = Some(0);
                }
            }
            AppEvent::IssuesNavigateEnd => {
                if !self.issues_state.issues.is_empty() {
                    self.issues_state.selected_issue_index =
                        Some(self.issues_state.issues.len() - 1);
                }
            }
            AppEvent::IssuesEnter => {
                if self.issues_state.issue_focus == IssueFocus::IssueList
                    && self.issues_state.selected_issue_index.is_some()
                {
                    self.issues_state.issue_focus = IssueFocus::IssueDetail;
                }
            }
            AppEvent::IssuesCycleFocus => {
                self.issues_state.issue_focus = match self.issues_state.issue_focus {
                    IssueFocus::RepoList => IssueFocus::IssueList,
                    IssueFocus::IssueList => IssueFocus::IssueDetail,
                    IssueFocus::IssueDetail => IssueFocus::RepoList,
                };
            }
            AppEvent::IssuesCycleFocusReverse => {
                self.issues_state.issue_focus = match self.issues_state.issue_focus {
                    IssueFocus::RepoList => IssueFocus::IssueDetail,
                    IssueFocus::IssueList => IssueFocus::RepoList,
                    IssueFocus::IssueDetail => IssueFocus::IssueList,
                };
            }
            _ => {}
        }
    }

    /// Handle data-loaded events (issue lists, details, comments, search, filters).
    #[allow(clippy::too_many_lines)]
    fn apply_issues_data(&mut self, event: AppEvent) {
        match event {
            AppEvent::IssueListLoaded {
                scope_repo_id,
                issues,
                cursor,
                has_more,
            } => {
                let current_repo_id = self.selected_repository_id().cloned();
                if current_repo_id.as_ref() == Some(&scope_repo_id) {
                    self.issues_state.error = None;
                    self.issues_state.issues = issues;
                    self.issues_state.list_cursor = cursor;
                    self.issues_state.has_more_issues = has_more;
                    self.issues_state.list_loading = false;
                    if self.issues_state.issues.is_empty() {
                        self.issues_state.selected_issue_index = None;
                        self.issues_state.issue_detail = None;
                    } else {
                        self.issues_state.selected_issue_index = Some(0);
                    }
                }
            }
            AppEvent::IssueListPageLoaded {
                scope_repo_id,
                issues,
                cursor,
                has_more,
            } => {
                let current_repo_id = self.selected_repository_id().cloned();
                if current_repo_id.as_ref() == Some(&scope_repo_id) {
                    self.issues_state.error = None;
                    self.issues_state.issues.extend(issues);
                    self.issues_state.list_cursor = cursor;
                    self.issues_state.has_more_issues = has_more;
                    self.issues_state.list_loading = false;
                }
            }
            AppEvent::IssueDetailLoaded {
                scope_repo_id,
                detail,
                ..
            } => {
                let current_repo_id = self.selected_repository_id().cloned();
                if current_repo_id.as_ref() == Some(&scope_repo_id) {
                    self.issues_state.error = None;
                    self.issues_state.issue_detail = Some(*detail);
                    self.issues_state.detail_loading = false;
                    self.issues_state.detail_subfocus = DetailSubfocus::Body;
                    self.issues_state.detail_scroll_offset = 0;
                }
            }
            AppEvent::IssueCommentsPageLoaded {
                scope_repo_id,
                issue_number,
                comments,
                cursor,
                has_more,
            } => {
                let current_repo_id = self.selected_repository_id().cloned();
                if current_repo_id.as_ref() == Some(&scope_repo_id) {
                    if let Some(detail) = &mut self.issues_state.issue_detail
                        && detail.number == issue_number
                    {
                        detail.comments.extend(comments);
                        detail.comments_cursor = cursor;
                        detail.has_more_comments = has_more;
                    }
                    self.issues_state.error = None;
                    self.issues_state.comments_loading = false;
                }
            }
            AppEvent::SetSearchQuery { query } => {
                self.issues_state.search_query = query;
            }
            AppEvent::UpdateDraftFilter { field, value } => match field.as_str() {
                "author" => self.issues_state.draft_filter.author = value,
                "assignee" => self.issues_state.draft_filter.assignee = value,
                "mentioned" => self.issues_state.draft_filter.mentioned = value,
                "query_text" => self.issues_state.draft_filter.query_text = value,
                "updated_before" => self.issues_state.draft_filter.updated_before = value,
                "updated_after" => self.issues_state.draft_filter.updated_after = value,
                _ => {}
            },
            _ => {}
        }
    }

    /// Handle error events.
    fn apply_issues_error(&mut self, event: AppEvent) {
        match event {
            AppEvent::IssueListLoadFailed {
                scope_repo_id,
                error,
            } => {
                let current_repo_id = self.selected_repository_id().cloned();
                if current_repo_id.as_ref() == Some(&scope_repo_id) {
                    self.issues_state.list_loading = false;
                    self.issues_state.error = Some(error);
                }
            }
            AppEvent::IssueDetailLoadFailed {
                scope_repo_id,
                error,
                ..
            } => {
                let current_repo_id = self.selected_repository_id().cloned();
                if current_repo_id.as_ref() == Some(&scope_repo_id) {
                    self.issues_state.detail_loading = false;
                    self.issues_state.error = Some(error);
                }
            }
            AppEvent::IssueCommentsPageFailed {
                scope_repo_id,
                error,
                ..
            } => {
                let current_repo_id = self.selected_repository_id().cloned();
                if current_repo_id.as_ref() == Some(&scope_repo_id) {
                    self.issues_state.comments_loading = false;
                    self.issues_state.error = Some(error);
                }
            }
            AppEvent::CommentCreateFailed { error } | AppEvent::MutationFailed { error } => {
                self.issues_state.error = Some(error);
                self.issues_state.inline_state = InlineState::None;
            }
            AppEvent::SendToAgentFailed { error } => {
                self.issues_state.error = Some(error);
            }
            _ => {}
        }
    }

    /// Handle all issues-mode `AppEvent` variants.
    ///
    /// Returns `true` if the event was handled, `false` if it was not an
    /// issues-mode event (caller should handle it).
    #[allow(clippy::too_many_lines)]
    pub(super) fn apply_issues_event(&mut self, event: AppEvent) -> bool {
        match event {
            // Scroll detail pane viewport (clamped to content length)
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

            // Issues Mode events — P05 Domain + State Implementation
            // @plan PLAN-20260329-ISSUES-MODE.P05
            // @requirement REQ-ISS-001, REQ-ISS-005
            AppEvent::EnterIssuesMode => self.enter_issues_mode(),

            // @requirement REQ-ISS-001, REQ-ISS-005
            AppEvent::ExitIssuesMode => self.exit_issues_mode(),

            // @requirement REQ-ISS-002
            AppEvent::RefocusIssueList => {
                self.issues_state.issue_focus = IssueFocus::IssueList;
            }

            // @requirement REQ-ISS-004, REQ-ISS-002
            // Navigation and focus events — delegated
            AppEvent::IssuesNavigateUp
            | AppEvent::IssuesNavigateDown
            | AppEvent::IssuesNavigatePageUp
            | AppEvent::IssuesNavigatePageDown
            | AppEvent::IssuesNavigateHome
            | AppEvent::IssuesNavigateEnd
            | AppEvent::IssuesEnter
            | AppEvent::IssuesCycleFocus
            | AppEvent::IssuesCycleFocusReverse => {
                self.apply_issues_navigation(event);
            }

            // @requirement REQ-ISS-003
            AppEvent::IssueDetailSubfocusNext => self.detail_subfocus_next(),

            // @requirement REQ-ISS-003
            AppEvent::IssueDetailSubfocusPrev => self.detail_subfocus_prev(),

            // @requirement REQ-ISS-008
            AppEvent::OpenFilterControls => {
                self.issues_state.filter_controls_open = true;
            }

            // @requirement REQ-ISS-008
            AppEvent::CloseFilterControls => {
                self.issues_state.filter_controls_open = false;
            }

            // @requirement REQ-ISS-008
            AppEvent::ApplyFilter => {
                // Commit draft filter to committed filter
                self.issues_state.committed_filter = self.issues_state.draft_filter.clone();
                self.issues_state.filter_controls_open = false;
            }

            // @requirement REQ-ISS-008
            AppEvent::ClearFilter => {
                self.issues_state.committed_filter = IssueFilter::default();
                self.issues_state.draft_filter = IssueFilter::default();
            }

            // @requirement REQ-ISS-007
            AppEvent::FocusSearchInput => {
                self.issues_state.search_input_focused = true;
            }

            // @requirement REQ-ISS-007
            AppEvent::BlurSearchInput => {
                self.issues_state.search_input_focused = false;
            }

            // @requirement REQ-ISS-007
            AppEvent::ClearSearch => {
                self.issues_state.search_query.clear();
            }

            // @requirement REQ-ISS-006, REQ-ISS-009, REQ-ISS-012
            // Data events — delegated
            AppEvent::IssueListLoaded { .. }
            | AppEvent::IssueListPageLoaded { .. }
            | AppEvent::IssueDetailLoaded { .. }
            | AppEvent::IssueCommentsPageLoaded { .. }
            | AppEvent::SetSearchQuery { .. }
            | AppEvent::UpdateDraftFilter { .. } => {
                self.apply_issues_data(event);
            }

            // @requirement REQ-ISS-010
            // @pseudocode component-001 lines 190-197
            AppEvent::OpenNewCommentComposer => {
                if self.issues_state.inline_state == InlineState::None {
                    self.issues_state.inline_state = InlineState::Composer {
                        target: ComposerTarget::NewComment,
                        text: String::new(),
                        cursor: 0,
                    };
                }
            }

            // @requirement REQ-ISS-010
            AppEvent::OpenReplyComposer { comment_index } => {
                self.open_reply_composer(comment_index);
            }

            // @requirement REQ-ISS-010
            AppEvent::OpenInlineEditor { target } => {
                self.open_inline_editor(target);
            }

            // @requirement REQ-ISS-010
            // Inline editor/composer text manipulation — delegated to apply_inline_event
            AppEvent::InlineChar(_)
            | AppEvent::InlineNewline
            | AppEvent::InlineBackspace
            | AppEvent::InlineDelete
            | AppEvent::InlineCursorLeft
            | AppEvent::InlineCursorRight
            | AppEvent::InlineCursorUp
            | AppEvent::InlineCursorDown
            | AppEvent::InlineCancelOrEsc => {
                self.apply_inline_event(event);
            }

            // @requirement REQ-ISS-010
            AppEvent::CommentCreated { comment } => {
                if let Some(detail) = &mut self.issues_state.issue_detail {
                    detail.comments.push(comment);
                }
                self.issues_state.error = None;
                self.issues_state.inline_state = InlineState::None;
            }

            // @requirement REQ-ISS-010
            AppEvent::IssueBodyUpdated { body } => {
                if let Some(detail) = &mut self.issues_state.issue_detail {
                    detail.body = body;
                }
                self.issues_state.error = None;
                self.issues_state.inline_state = InlineState::None;
            }

            // @requirement REQ-ISS-010
            AppEvent::CommentUpdated {
                comment_index,
                body,
            } => {
                if let Some(detail) = &mut self.issues_state.issue_detail
                    && let Some(comment) = detail.comments.get_mut(comment_index)
                {
                    comment.body = body;
                }
                self.issues_state.error = None;
                self.issues_state.inline_state = InlineState::None;
            }

            // @requirement REQ-ISS-011
            AppEvent::OpenAgentChooser => {
                // Only show non-running agents belonging to the selected repository
                let repo_id = self.selected_repository_id().cloned();
                let agents: Vec<_> = self
                    .agents
                    .iter()
                    .filter(|a| {
                        repo_id.as_ref().is_some_and(|rid| a.repository_id == *rid)
                            && !a.is_running()
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

            // @requirement REQ-ISS-011
            AppEvent::AgentChooserNavigateUp => {
                if let Some(chooser) = &mut self.issues_state.agent_chooser
                    && chooser.selected_index > 0
                {
                    chooser.selected_index -= 1;
                }
            }

            // @requirement REQ-ISS-011
            AppEvent::AgentChooserNavigateDown => {
                if let Some(chooser) = &mut self.issues_state.agent_chooser
                    && chooser.selected_index + 1 < chooser.agents.len()
                {
                    chooser.selected_index += 1;
                }
            }

            // @requirement REQ-ISS-011
            // AgentChooserConfirm: actual send is handled by dispatch_app_event
            AppEvent::AgentChooserConfirm
            | AppEvent::AgentChooserCancel
            | AppEvent::SendToAgentCompleted => {
                self.issues_state.agent_chooser = None;
            }

            // @requirement REQ-ISS-012
            // Error events — delegated
            AppEvent::IssueListLoadFailed { .. }
            | AppEvent::IssueDetailLoadFailed { .. }
            | AppEvent::IssueCommentsPageFailed { .. }
            | AppEvent::CommentCreateFailed { .. }
            | AppEvent::MutationFailed { .. }
            | AppEvent::SendToAgentFailed { .. } => {
                self.apply_issues_error(event);
            }

            // Not an issues event
            _ => return false,
        }
        true
    }
}
