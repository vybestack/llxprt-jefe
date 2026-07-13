//! Issues mode load/result state operations.

use super::{
    AppEvent, AppState, DetailSubfocus, IssueCommentsPagePending, IssueDetailPending,
    IssueListPagePending, IssueListReloadPending,
};
use crate::domain::{Issue, IssueComment, IssueFilter, IssueFilterState, RepositoryId};

struct IssueListPageLoadedData {
    scope_repo_id: RepositoryId,
    filter: IssueFilter,
    request_id: u64,
    request_cursor: Option<String>,
    issues: Vec<Issue>,
    cursor: Option<String>,
    has_more: bool,
}

struct IssueListLoadedData {
    scope_repo_id: RepositoryId,
    filter: IssueFilter,
    request_id: u64,
    issues: Vec<Issue>,
    cursor: Option<String>,
    has_more: bool,
}

/// Payload for a silent background refresh (issue #175). Mirrors
/// `IssueListLoadedData` but the reducer preserves selection/scroll/detail.
struct IssueListSilentRefreshedData {
    scope_repo_id: RepositoryId,
    filter: IssueFilter,
    request_id: u64,
    issues: Vec<Issue>,
    cursor: Option<String>,
    has_more: bool,
}

struct IssueCommentsPageLoadedData {
    scope_repo_id: RepositoryId,
    issue_number: u64,
    request_id: u64,
    request_cursor: Option<String>,
    comments: Vec<IssueComment>,
    cursor: Option<String>,
    has_more: bool,
}

impl AppState {
    fn apply_issue_list_loaded(&mut self, list: IssueListLoadedData) {
        if self.list_reload_pending_matches(&list.scope_repo_id, &list.filter, list.request_id) {
            self.issues_state.error = None;
            self.issues_state.issues = list.issues;
            self.issues_state.list_cursor = list.cursor;
            self.issues_state.has_more_issues = list.has_more;
            self.issues_state.loading.list = false;
            self.issues_state.list_reload_pending = None;
            self.issues_state.list_page_pending = None;
            self.issues_state.detail_pending = None;
            if self.issues_state.issues.is_empty() {
                self.issues_state.selected_issue_index = None;
                self.issues_state.issue_detail = None;
            } else {
                self.issues_state.selected_issue_index = Some(0);
            }
        }
    }

    /// Apply a silent background refresh (issue #175). Mirrors
    /// `apply_issue_list_loaded` but preserves selection, scroll offset, and
    /// `issue_detail`, and does NOT set `loading.list` (no spinner flash).
    fn apply_issue_list_silent_refreshed(&mut self, data: IssueListSilentRefreshedData) {
        if !self.list_reload_pending_matches(&data.scope_repo_id, &data.filter, data.request_id) {
            return;
        }
        let selected_issue_number = self
            .issues_state
            .selected_issue_index
            .and_then(|idx| self.issues_state.issues.get(idx))
            .map(|issue| issue.number);
        self.issues_state.error = None;
        self.issues_state.issues = data.issues;
        self.issues_state.list_cursor = data.cursor;
        self.issues_state.has_more_issues = data.has_more;
        // Do NOT set loading.list — this is a silent background refresh.
        self.issues_state.list_reload_pending = None;
        self.issues_state.list_page_pending = None;
        self.preserve_issue_silent_refresh_selection(selected_issue_number);
    }

    /// Re-derive selection + scroll after a silent refresh (issue #175 helper).
    fn preserve_issue_silent_refresh_selection(&mut self, selected_issue_number: Option<u64>) {
        if self.issues_state.issues.is_empty() {
            self.issues_state.selected_issue_index = None;
            return;
        }
        let new_index = selected_issue_number
            .and_then(|num| {
                self.issues_state
                    .issues
                    .iter()
                    .position(|issue| issue.number == num)
            })
            .unwrap_or(0);
        self.issues_state.selected_issue_index = Some(new_index);
    }

    /// Apply a silent background refresh failure (issue #175). Clears the
    /// pending marker WITHOUT surfacing an error.
    fn apply_issue_list_silent_refresh_failed(
        &mut self,
        scope_repo_id: &RepositoryId,
        request_id: u64,
    ) {
        if self.list_reload_pending_matches_id(scope_repo_id, request_id) {
            self.issues_state.list_reload_pending = None;
        }
    }

    fn apply_issue_list_page_loaded(&mut self, page: IssueListPageLoadedData) {
        if self.list_page_pending_matches(
            &page.scope_repo_id,
            &page.filter,
            page.request_id,
            page.request_cursor.as_deref(),
        ) {
            self.issues_state.error = None;
            self.issues_state.issues.extend(page.issues);
            self.issues_state.list_cursor = page.cursor;
            self.issues_state.has_more_issues = page.has_more;
            self.issues_state.loading.list = false;
            self.issues_state.list_page_pending = None;
        }
    }

    fn apply_issue_detail_loaded(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        issue_number: u64,
        request_id: u64,
        detail: crate::domain::IssueDetail,
    ) {
        let current_repo_id = self.selected_repository_id().cloned();
        if current_repo_id.as_ref() == Some(&scope_repo_id)
            && self.detail_pending_matches(&scope_repo_id, issue_number, request_id)
        {
            self.issues_state.error = None;
            self.issues_state.issue_detail = Some(detail);
            self.issues_state.loading.detail = false;
            self.issues_state.loading.comments = false;
            self.issues_state.detail_pending = None;
            self.issues_state.comments_page_pending = None;
            self.issues_state.detail_subfocus = DetailSubfocus::Body;
            self.issues_state.detail_scroll_offset = 0;
        }
    }

    /// Apply a silent background detail refresh (issue #175). Mirrors
    /// `apply_issue_detail_loaded` but does NOT set `loading.detail`, does NOT
    /// reset `detail_subfocus` or `detail_scroll_offset`, and does NOT set an
    /// error. Preserves the user's scroll/focus position.
    fn apply_issue_detail_silent_refreshed(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        issue_number: u64,
        request_id: u64,
        detail: crate::domain::IssueDetail,
    ) {
        let current_repo_id = self.selected_repository_id().cloned();
        if current_repo_id.as_ref() == Some(&scope_repo_id)
            && self.detail_pending_matches(&scope_repo_id, issue_number, request_id)
        {
            self.issues_state.issue_detail = Some(detail);
            self.issues_state.detail_pending = None;
        }
    }

    /// Apply a silent background detail refresh failure (issue #175). Clears
    /// `detail_pending` silently WITHOUT setting an error.
    fn apply_issue_detail_silent_refresh_failed(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        issue_number: u64,
        request_id: u64,
    ) {
        if self.detail_pending_matches(scope_repo_id, issue_number, request_id) {
            self.issues_state.detail_pending = None;
        }
    }

    fn apply_issue_comments_page_loaded(&mut self, page: IssueCommentsPageLoadedData) {
        if self.comments_page_pending_matches(
            &page.scope_repo_id,
            page.issue_number,
            page.request_id,
            page.request_cursor.as_deref(),
        ) && let Some(detail) = &mut self.issues_state.issue_detail
            && detail.number == page.issue_number
        {
            detail.comments.extend(page.comments);
            detail.comments_cursor = page.cursor;
            detail.has_more_comments = page.has_more;
            self.issues_state.error = None;
            self.issues_state.loading.comments = false;
            self.issues_state.comments_page_pending = None;
        }
    }

    fn current_detail_matches(
        &self,
        scope_repo_id: &crate::domain::RepositoryId,
        issue_number: u64,
    ) -> bool {
        self.selected_repository_id() == Some(scope_repo_id)
            && self
                .issues_state
                .issue_detail
                .as_ref()
                .is_some_and(|detail| detail.number == issue_number)
    }

    pub fn mark_comments_page_loading(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        issue_number: u64,
        cursor: Option<String>,
    ) {
        self.mark_comments_page_loading_with_request_id(scope_repo_id, issue_number, cursor, 0);
    }

    pub fn next_comments_page_request_id(&mut self) -> u64 {
        let request_id = self
            .issues_state
            .next_comments_page_request_id
            .saturating_add(1);
        self.issues_state.next_comments_page_request_id = request_id;
        request_id
    }

    pub fn mark_comments_page_loading_with_request_id(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        issue_number: u64,
        cursor: Option<String>,
        request_id: u64,
    ) {
        self.issues_state.loading.comments = true;
        self.issues_state.comments_page_pending = Some(IssueCommentsPagePending {
            scope_repo_id,
            issue_number,
            cursor,
            request_id,
        });
    }

    pub fn mark_issue_list_page_loading(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        filter: IssueFilter,
        cursor: Option<String>,
    ) {
        self.mark_issue_list_page_loading_with_request_id(scope_repo_id, filter, cursor, 0);
    }

    pub fn mark_issue_list_page_loading_with_request_id(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        filter: IssueFilter,
        cursor: Option<String>,
        request_id: u64,
    ) {
        self.issues_state.loading.list = true;
        self.issues_state.list_reload_pending = None;
        self.issues_state.list_page_pending = Some(IssueListPagePending {
            scope_repo_id,
            filter,
            cursor,
            request_id,
        });
    }

    pub fn mark_issue_list_reload_loading(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        filter: IssueFilter,
        request_id: u64,
    ) {
        self.issues_state.loading.list = true;
        self.issues_state.list_page_pending = None;
        self.issues_state.detail_pending = None;
        self.issues_state.list_reload_pending = Some(IssueListReloadPending {
            scope_repo_id,
            filter,
            request_id,
        });
    }

    /// Mark a silent background refresh as pending (issue #175). Does NOT set
    /// `loading.list` (no spinner flash) and does NOT clear `detail_pending`.
    pub fn mark_issue_list_silent_refresh_loading(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        filter: IssueFilter,
        request_id: u64,
    ) {
        self.issues_state.list_page_pending = None;
        self.issues_state.list_reload_pending = Some(IssueListReloadPending {
            scope_repo_id,
            filter,
            request_id,
        });
    }

    pub fn mark_issue_detail_loading(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        issue_number: u64,
    ) {
        self.mark_issue_detail_loading_with_request_id(scope_repo_id, issue_number, 0);
    }

    pub fn next_issue_detail_request_id(&mut self) -> u64 {
        let request_id = self
            .issues_state
            .next_issue_detail_request_id
            .saturating_add(1);
        self.issues_state.next_issue_detail_request_id = request_id;
        request_id
    }

    pub fn mark_issue_detail_loading_with_request_id(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        issue_number: u64,
        request_id: u64,
    ) {
        self.issues_state.loading.detail = true;
        self.issues_state.detail_pending = Some(IssueDetailPending {
            scope_repo_id,
            issue_number,
            request_id,
        });
    }

    /// Mark an issue detail silent refresh as pending (issue #175). Sets
    /// `detail_pending` for staleness tracking but does NOT set
    /// `loading.detail` (no spinner flash).
    pub fn mark_issue_detail_silent_loading(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        issue_number: u64,
        request_id: u64,
    ) {
        self.issues_state.detail_pending = Some(IssueDetailPending {
            scope_repo_id,
            issue_number,
            request_id,
        });
    }

    fn list_page_pending_matches(
        &self,
        scope_repo_id: &crate::domain::RepositoryId,
        filter: &IssueFilter,
        request_id: u64,
        cursor: Option<&str>,
    ) -> bool {
        self.selected_repository_id() == Some(scope_repo_id)
            && self
                .issues_state
                .list_page_pending
                .as_ref()
                .is_some_and(|pending| {
                    pending.scope_repo_id == *scope_repo_id
                        && pending.filter == *filter
                        && pending.request_id == request_id
                        && pending.cursor.as_deref() == cursor
                })
    }

    fn list_reload_pending_matches(
        &self,
        scope_repo_id: &crate::domain::RepositoryId,
        filter: &IssueFilter,
        request_id: u64,
    ) -> bool {
        if request_id == 0 {
            return self.selected_repository_id() == Some(scope_repo_id)
                && self.issues_state.committed_filter == *filter;
        }
        self.selected_repository_id() == Some(scope_repo_id)
            && self.issues_state.committed_filter == *filter
            && self
                .issues_state
                .list_reload_pending
                .as_ref()
                .is_some_and(|pending| {
                    pending.scope_repo_id == *scope_repo_id
                        && pending.filter == *filter
                        && pending.request_id == request_id
                })
    }

    /// Check if a pending list-reload matches scope + request_id (issue #175).
    fn list_reload_pending_matches_id(
        &self,
        scope_repo_id: &crate::domain::RepositoryId,
        request_id: u64,
    ) -> bool {
        if request_id == 0 {
            return self.selected_repository_id() == Some(scope_repo_id);
        }
        self.selected_repository_id() == Some(scope_repo_id)
            && self
                .issues_state
                .list_reload_pending
                .as_ref()
                .is_some_and(|pending| {
                    pending.scope_repo_id == *scope_repo_id && pending.request_id == request_id
                })
    }

    fn detail_pending_matches(
        &self,
        scope_repo_id: &crate::domain::RepositoryId,
        issue_number: u64,
        request_id: u64,
    ) -> bool {
        self.issues_state
            .detail_pending
            .as_ref()
            .is_some_and(|pending| {
                pending.scope_repo_id == *scope_repo_id
                    && pending.issue_number == issue_number
                    && pending.request_id == request_id
            })
    }

    fn comments_page_pending_matches(
        &self,
        scope_repo_id: &crate::domain::RepositoryId,
        issue_number: u64,
        request_id: u64,
        cursor: Option<&str>,
    ) -> bool {
        self.selected_repository_id() == Some(scope_repo_id)
            && self
                .issues_state
                .comments_page_pending
                .as_ref()
                .is_some_and(|pending| {
                    pending.scope_repo_id == *scope_repo_id
                        && pending.issue_number == issue_number
                        && pending.request_id == request_id
                        && pending.cursor.as_deref() == cursor
                })
    }

    fn update_draft_filter_field(&mut self, field: String, value: String) {
        match field.as_str() {
            "state" => match value.as_str() {
                "open" => self.issues_state.draft_filter.state = Some(IssueFilterState::Open),
                "closed" => self.issues_state.draft_filter.state = Some(IssueFilterState::Closed),
                "all" => self.issues_state.draft_filter.state = Some(IssueFilterState::All),
                "" => self.issues_state.draft_filter.state = None,
                _ => {}
            },
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
            "issue_type" => self.issues_state.draft_filter.issue_type = value,
            "milestone" => self.issues_state.draft_filter.milestone = value,
            "module" => self.issues_state.draft_filter.module = value,
            "updated_before" => self.issues_state.draft_filter.updated_before = value,
            "updated_after" => self.issues_state.draft_filter.updated_after = value,
            _ => {}
        }
    }

    /// Handle data-loaded events (issue lists, details, comments, search, filters).
    pub(crate) fn apply_issues_data(&mut self, event: AppEvent) {
        match event {
            AppEvent::IssueListLoaded { .. }
            | AppEvent::IssueListPageLoaded { .. }
            | AppEvent::IssueListSilentRefreshed { .. } => self.apply_issue_list_data(event),
            detail_event @ (AppEvent::IssueDetailLoaded { .. }
            | AppEvent::IssueDetailSilentRefreshed { .. }) => {
                self.apply_issue_detail_data(detail_event);
            }
            AppEvent::IssueCommentsPageLoaded {
                scope_repo_id,
                issue_number,
                request_id,
                request_cursor,
                comments,
                cursor,
                has_more,
            } => self.apply_issue_comments_page_loaded(IssueCommentsPageLoadedData {
                scope_repo_id,
                issue_number,
                request_id,
                request_cursor,
                comments,
                cursor,
                has_more,
            }),
            AppEvent::SetSearchQuery { query } => self.issues_state.search_query = query,
            AppEvent::UpdateDraftFilter { field, value } => {
                self.update_draft_filter_field(field, value);
            }
            _ => {}
        }
    }

    /// Apply a detail data event (loud or silent, issue #175).
    fn apply_issue_detail_data(&mut self, event: AppEvent) {
        match event {
            AppEvent::IssueDetailLoaded {
                scope_repo_id,
                issue_number,
                request_id,
                detail,
            } => self.apply_issue_detail_loaded(scope_repo_id, issue_number, request_id, *detail),
            AppEvent::IssueDetailSilentRefreshed {
                scope_repo_id,
                issue_number,
                request_id,
                detail,
            } => {
                self.apply_issue_detail_silent_refreshed(
                    scope_repo_id,
                    issue_number,
                    request_id,
                    *detail,
                );
            }
            _ => {}
        }
    }

    fn apply_issue_list_data(&mut self, event: AppEvent) {
        match event {
            AppEvent::IssueListLoaded {
                scope_repo_id,
                filter,
                request_id,
                issues,
                cursor,
                has_more,
            } => self.apply_issue_list_loaded(IssueListLoadedData {
                scope_repo_id,
                filter: *filter,
                request_id,
                issues,
                cursor,
                has_more,
            }),
            AppEvent::IssueListPageLoaded {
                scope_repo_id,
                filter,
                request_id,
                request_cursor,
                issues,
                cursor,
                has_more,
            } => self.apply_issue_list_page_loaded(IssueListPageLoadedData {
                scope_repo_id,
                filter: *filter,
                request_id,
                request_cursor,
                issues,
                cursor,
                has_more,
            }),
            AppEvent::IssueListSilentRefreshed {
                scope_repo_id,
                filter,
                request_id,
                issues,
                cursor,
                has_more,
            } => self.apply_issue_list_silent_refreshed(IssueListSilentRefreshedData {
                scope_repo_id,
                filter: *filter,
                request_id,
                issues,
                cursor,
                has_more,
            }),
            _ => {}
        }
    }

    /// Handle error events.
    pub(crate) fn apply_issues_error(&mut self, event: AppEvent) {
        match event {
            AppEvent::IssueListLoadFailed { .. }
            | AppEvent::IssueDetailLoadFailed { .. }
            | AppEvent::IssueCommentsPageFailed { .. }
            | AppEvent::IssueListSilentRefreshFailed { .. }
            | AppEvent::IssueDetailSilentRefreshFailed { .. } => self.apply_issue_load_error(event),
            AppEvent::CommentCreateFailed { .. } | AppEvent::MutationFailed { .. } => {
                self.apply_issue_mutation_error(event);
            }
            AppEvent::SendToAgentFailed { error } => {
                self.issues_state.error = Some(error);
            }
            // Self-assignment is a non-blocking follow-up to a successful
            // send (issue #186): its failure surfaces a warning without
            // affecting the launch or the issues error state.
            AppEvent::IssueSelfAssignmentFailed {
                owner_repo,
                issue_number,
                error,
            } => {
                self.warning_message = Some(format!(
                    "Issue {owner_repo}#{issue_number} sent but could not be assigned: {error}"
                ));
            }
            _ => {}
        }
    }

    fn apply_issue_load_error(&mut self, event: AppEvent) {
        match event {
            AppEvent::IssueListLoadFailed {
                scope_repo_id,
                filter,
                request_id,
                request_cursor,
                error,
            } => self.apply_issue_list_load_failed(
                &scope_repo_id,
                &filter,
                request_id,
                request_cursor,
                error,
            ),
            AppEvent::IssueListSilentRefreshFailed {
                scope_repo_id,
                request_id,
            } => self.apply_issue_list_silent_refresh_failed(&scope_repo_id, request_id),
            AppEvent::IssueDetailLoadFailed {
                scope_repo_id,
                issue_number,
                request_id,
                error,
            } => {
                self.apply_issue_detail_load_failed(
                    &scope_repo_id,
                    issue_number,
                    request_id,
                    error,
                );
            }
            AppEvent::IssueDetailSilentRefreshFailed {
                scope_repo_id,
                issue_number,
                request_id,
            } => self.apply_issue_detail_silent_refresh_failed(
                &scope_repo_id,
                issue_number,
                request_id,
            ),
            AppEvent::IssueCommentsPageFailed {
                scope_repo_id,
                issue_number,
                request_id,
                request_cursor,
                error,
            } if self.comments_page_pending_matches(
                &scope_repo_id,
                issue_number,
                request_id,
                request_cursor.as_deref(),
            ) && self.current_detail_matches(&scope_repo_id, issue_number) =>
            {
                self.apply_issue_comments_page_failed(error);
            }
            _ => {}
        }
    }

    /// Apply a loud comments-page failure (clears the pending marker + error).
    fn apply_issue_comments_page_failed(&mut self, error: String) {
        self.issues_state.loading.comments = false;
        self.issues_state.comments_page_pending = None;
        self.issues_state.error = Some(error);
    }

    /// Apply a loud list-load failure (issue #175). The pending matchers
    /// already disambiguate a fresh reload from a page fetch (the two pendings
    /// are mutually exclusive and the page matcher compares the cursor).
    /// Gating on `request_cursor` presence would wrongly skip a page failure
    /// whose cursor is `None` (which happens when GitHub reports
    /// `has_more = true` with a null cursor), leaving `loading.list` stuck.
    fn apply_issue_list_load_failed(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        filter: &IssueFilter,
        request_id: u64,
        request_cursor: Option<String>,
        error: String,
    ) {
        let fresh_failure = self.list_reload_pending_matches(scope_repo_id, filter, request_id);
        let page_failure = self.list_page_pending_matches(
            scope_repo_id,
            filter,
            request_id,
            request_cursor.as_deref(),
        );
        if fresh_failure || page_failure {
            self.issues_state.loading.list = false;
            self.issues_state.list_reload_pending = None;
            self.issues_state.list_page_pending = None;
            self.issues_state.error = Some(error);
        }
    }

    /// Apply a loud detail-load failure (issue #175).
    fn apply_issue_detail_load_failed(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        issue_number: u64,
        request_id: u64,
        error: String,
    ) {
        let current_repo_id = self.selected_repository_id().cloned();
        if current_repo_id.as_ref() == Some(scope_repo_id)
            && self.detail_pending_matches(scope_repo_id, issue_number, request_id)
        {
            self.issues_state.loading.detail = false;
            self.issues_state.detail_pending = None;
            self.issues_state.error = Some(error);
        }
    }
}
