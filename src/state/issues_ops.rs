//! Issues mode state operations.
//!
//! Extracted from mod.rs to keep file sizes manageable.
//! @plan PLAN-20260329-ISSUES-MODE.P05

use super::{
    AgentChooserState, AppEvent, AppState, ComposerTarget, DetailSubfocus, EditorTarget,
    InlineState, IssueFocus, PaneFocus, PriorAgentFocus, ScreenMode, inline_cursor_vertical,
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

    fn active_inline_text(inline_state: &mut InlineState) -> Option<(&mut String, &mut usize)> {
        match inline_state {
            InlineState::Composer { text, cursor, .. }
            | InlineState::Editor { text, cursor, .. } => Some((text, cursor)),
            InlineState::None => None,
        }
    }

    fn insert_inline_char(inline_state: &mut InlineState, c: char) {
        if let Some((text, cursor)) = Self::active_inline_text(inline_state) {
            text.insert(*cursor, c);
            *cursor += c.len_utf8();
        }
    }

    fn delete_inline_previous_char(inline_state: &mut InlineState) {
        if let Some((text, cursor)) = Self::active_inline_text(inline_state)
            && *cursor > 0
        {
            let prev = text[..*cursor].chars().last().map_or(0, char::len_utf8);
            text.drain((*cursor - prev)..*cursor);
            *cursor -= prev;
        }
    }

    fn delete_inline_next_char(inline_state: &mut InlineState) {
        if let Some((text, cursor)) = Self::active_inline_text(inline_state)
            && *cursor < text.len()
        {
            let next = text[*cursor..].chars().next().map_or(0, char::len_utf8);
            text.drain(*cursor..(*cursor + next));
        }
    }

    fn move_inline_cursor_left(inline_state: &mut InlineState) {
        if let Some((text, cursor)) = Self::active_inline_text(inline_state)
            && *cursor > 0
        {
            let prev = text[..*cursor].chars().last().map_or(0, char::len_utf8);
            *cursor -= prev;
        }
    }

    fn move_inline_cursor_right(inline_state: &mut InlineState) {
        if let Some((text, cursor)) = Self::active_inline_text(inline_state)
            && *cursor < text.len()
        {
            let next = text[*cursor..].chars().next().map_or(0, char::len_utf8);
            *cursor += next;
        }
    }

    /// Handle inline editor/composer text manipulation events.
    /// Returns `true` if the event was handled.
    fn apply_inline_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::InlineChar(c) => {
                Self::insert_inline_char(&mut self.issues_state.inline_state, c);
            }
            AppEvent::InlineNewline => {
                Self::insert_inline_char(&mut self.issues_state.inline_state, char::from(0x0Au8));
            }
            AppEvent::InlineBackspace => {
                Self::delete_inline_previous_char(&mut self.issues_state.inline_state);
            }
            AppEvent::InlineDelete => {
                Self::delete_inline_next_char(&mut self.issues_state.inline_state);
            }
            AppEvent::InlineCursorLeft => {
                Self::move_inline_cursor_left(&mut self.issues_state.inline_state);
            }
            AppEvent::InlineCursorRight => {
                Self::move_inline_cursor_right(&mut self.issues_state.inline_state);
            }
            AppEvent::InlineCursorUp => {
                if let Some((text, cursor)) =
                    Self::active_inline_text(&mut self.issues_state.inline_state)
                {
                    inline_cursor_vertical(text, cursor, -1);
                }
            }
            AppEvent::InlineCursorDown => {
                if let Some((text, cursor)) =
                    Self::active_inline_text(&mut self.issues_state.inline_state)
                {
                    inline_cursor_vertical(text, cursor, 1);
                }
            }
            AppEvent::InlineCancelOrEsc => self.issues_state.inline_state = InlineState::None,
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
    }

    fn navigate_issue_list_up(&mut self) {
        if let Some(idx) = self.issues_state.selected_issue_index
            && idx > 0
        {
            self.issues_state.selected_issue_index = Some(idx - 1);
        }
    }

    fn navigate_issue_list_down(&mut self) {
        if let Some(idx) = self.issues_state.selected_issue_index
            && idx + 1 < self.issues_state.issues.len()
        {
            self.issues_state.selected_issue_index = Some(idx + 1);
        }
    }

    fn navigate_issue_list_page_up(&mut self) {
        if let Some(idx) = self.issues_state.selected_issue_index {
            self.issues_state.selected_issue_index = Some(idx.saturating_sub(10));
        }
    }

    fn navigate_issue_list_page_down(&mut self) {
        if let Some(idx) = self.issues_state.selected_issue_index {
            let max = self.issues_state.issues.len().saturating_sub(1);
            self.issues_state.selected_issue_index = Some((idx + 10).min(max));
        }
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
            AppEvent::IssuesCycleFocus => self.cycle_issues_focus(),
            AppEvent::IssuesCycleFocusReverse => self.cycle_issues_focus_reverse(),
            _ => {}
        }
    }

    fn apply_issue_list_loaded(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        issues: Vec<crate::domain::Issue>,
        cursor: Option<String>,
        has_more: bool,
    ) {
        let current_repo_id = self.selected_repository_id().cloned();
        if current_repo_id.as_ref() == Some(&scope_repo_id) {
            self.issues_state.error = None;
            self.issues_state.issues = issues;
            self.issues_state.list_cursor = cursor;
            self.issues_state.has_more_issues = has_more;
            self.issues_state.loading.list = false;
            if self.issues_state.issues.is_empty() {
                self.issues_state.selected_issue_index = None;
                self.issues_state.issue_detail = None;
            } else {
                self.issues_state.selected_issue_index = Some(0);
            }
        }
    }

    fn apply_issue_list_page_loaded(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        issues: Vec<crate::domain::Issue>,
        cursor: Option<String>,
        has_more: bool,
    ) {
        let current_repo_id = self.selected_repository_id().cloned();
        if current_repo_id.as_ref() == Some(&scope_repo_id) {
            self.issues_state.error = None;
            self.issues_state.issues.extend(issues);
            self.issues_state.list_cursor = cursor;
            self.issues_state.has_more_issues = has_more;
            self.issues_state.loading.list = false;
        }
    }

    fn apply_issue_detail_loaded(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        detail: crate::domain::IssueDetail,
    ) {
        let current_repo_id = self.selected_repository_id().cloned();
        if current_repo_id.as_ref() == Some(&scope_repo_id) {
            self.issues_state.error = None;
            self.issues_state.issue_detail = Some(detail);
            self.issues_state.loading.detail = false;
            self.issues_state.detail_subfocus = DetailSubfocus::Body;
            self.issues_state.detail_scroll_offset = 0;
        }
    }

    fn apply_issue_comments_page_loaded(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        issue_number: u64,
        comments: Vec<crate::domain::IssueComment>,
        cursor: Option<String>,
        has_more: bool,
    ) {
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
            self.issues_state.loading.comments = false;
        }
    }

    fn update_draft_filter_field(&mut self, field: String, value: String) {
        match field.as_str() {
            "author" => self.issues_state.draft_filter.author = value,
            "assignee" => self.issues_state.draft_filter.assignee = value,
            "mentioned" => self.issues_state.draft_filter.mentioned = value,
            "query_text" => self.issues_state.draft_filter.query_text = value,
            "labels" => {
                self.issues_state
                    .filter_ui
                    .draft_labels_text
                    .clone_from(&value);
                self.issues_state.draft_filter.labels = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            "updated_before" => self.issues_state.draft_filter.updated_before = value,
            "updated_after" => self.issues_state.draft_filter.updated_after = value,
            _ => {}
        }
    }

    /// Handle data-loaded events (issue lists, details, comments, search, filters).
    fn apply_issues_data(&mut self, event: AppEvent) {
        match event {
            AppEvent::IssueListLoaded {
                scope_repo_id,
                issues,
                cursor,
                has_more,
            } => self.apply_issue_list_loaded(scope_repo_id, issues, cursor, has_more),
            AppEvent::IssueListPageLoaded {
                scope_repo_id,
                issues,
                cursor,
                has_more,
            } => self.apply_issue_list_page_loaded(scope_repo_id, issues, cursor, has_more),
            AppEvent::IssueDetailLoaded {
                scope_repo_id,
                detail,
                ..
            } => self.apply_issue_detail_loaded(scope_repo_id, *detail),
            AppEvent::IssueCommentsPageLoaded {
                scope_repo_id,
                issue_number,
                comments,
                cursor,
                has_more,
            } => self.apply_issue_comments_page_loaded(
                scope_repo_id,
                issue_number,
                comments,
                cursor,
                has_more,
            ),
            AppEvent::SetSearchQuery { query } => self.issues_state.search_query = query,
            AppEvent::UpdateDraftFilter { field, value } => {
                self.update_draft_filter_field(field, value);
            }
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
                    self.issues_state.loading.list = false;
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
                    self.issues_state.loading.detail = false;
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
                    self.issues_state.loading.comments = false;
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
            AppEvent::FilterNavigateNext => {
                const FILTER_FIELD_COUNT: usize = 5;
                let idx = self.issues_state.filter_ui.field_index;
                self.issues_state.filter_ui.field_index = (idx + 1) % FILTER_FIELD_COUNT;
            }
            AppEvent::FilterNavigatePrev => {
                const FILTER_FIELD_COUNT: usize = 5;
                let idx = self.issues_state.filter_ui.field_index;
                self.issues_state.filter_ui.field_index =
                    (idx + FILTER_FIELD_COUNT - 1) % FILTER_FIELD_COUNT;
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

    fn apply_inline_open_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::OpenNewIssueComposer => {
                if self.issues_state.inline_state == InlineState::None {
                    self.issues_state.issue_focus = IssueFocus::IssueList;
                    self.issues_state.inline_state = InlineState::Composer {
                        target: ComposerTarget::NewIssue,
                        text: String::new(),
                        cursor: 0,
                    };
                }
            }
            AppEvent::OpenNewCommentComposer => {
                if self.issues_state.inline_state == InlineState::None {
                    self.issues_state.inline_state = InlineState::Composer {
                        target: ComposerTarget::NewComment,
                        text: String::new(),
                        cursor: 0,
                    };
                }
            }
            AppEvent::OpenReplyComposer { comment_index } => {
                self.open_reply_composer(comment_index);
            }
            AppEvent::OpenInlineEditor { target } => self.open_inline_editor(target),
            _ => return false,
        }
        true
    }

    fn apply_issue_mutation_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::CommentCreated { comment } => {
                if let Some(detail) = &mut self.issues_state.issue_detail {
                    detail.comments.push(comment);
                }
            }
            AppEvent::IssueBodyUpdated { body } => {
                if let Some(detail) = &mut self.issues_state.issue_detail {
                    detail.body = body;
                }
            }
            AppEvent::CommentUpdated {
                comment_index,
                body,
            } => {
                if let Some(detail) = &mut self.issues_state.issue_detail
                    && let Some(comment) = detail.comments.get_mut(comment_index)
                {
                    comment.body = body;
                }
            }
            _ => return false,
        }
        self.issues_state.error = None;
        self.issues_state.inline_state = InlineState::None;
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
        if self.apply_issue_scroll_event(&event)
            || self.apply_issue_lifecycle_event(event.clone())
            || self.apply_issue_filter_event(event.clone())
            || self.apply_inline_open_event(event.clone())
            || self.apply_inline_event(event.clone())
            || self.apply_issue_mutation_event(event.clone())
            || self.apply_agent_chooser_event(event.clone())
            || self.apply_issue_error_event(event)
        {
            return true;
        }
        false
    }
}
