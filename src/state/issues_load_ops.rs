//! Issues mode load/result state operations.

use super::{AppEvent, AppState, DetailSubfocus, IssueDetailPending, IssueListIdentity};
use crate::domain::{
    CommentDetailIdentity, Issue, IssueComment, IssueFilter, IssueFilterState, ListRequestId,
    PageToken, RepositoryId,
};
use crate::state::pagination::{AcceptOutcome, LoadCorrelation, PageResult, ReloadResult};

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

struct IssueCommentsPageLoadedData {
    scope_repo_id: RepositoryId,
    issue_number: u64,
    request_id: u64,
    request_cursor: Option<String>,
    comments: Vec<IssueComment>,
    cursor: Option<String>,
    has_more: bool,
}

/// Build the cursor-based [`PageToken`] used to request a page from the cursor
/// the dispatch layer intends to fetch. A `None` cursor collapses to `Done`
/// (matching `PageToken::from_cursor`): a backend claiming more pages with no
/// cursor is treated as exhausted so the UI never wedges on a load-more that
/// can't fire.
fn issue_page_token(cursor: Option<String>) -> PageToken {
    cursor.map_or(PageToken::Done, PageToken::Cursor)
}

impl AppState {
    fn apply_issue_list_loaded(&mut self, list: IssueListLoadedData) {
        let identity = IssueListIdentity {
            scope_repo_id: list.scope_repo_id,
            filter: list.filter,
        };
        let result = ReloadResult {
            identity,
            request_id: ListRequestId::from_raw(list.request_id),
            items: list.issues,
            next_page: PageToken::from_cursor(list.cursor, list.has_more),
        };
        let outcome = self.issues_state.list.accept_loaded(result);
        if matches!(outcome, AcceptOutcome::Applied | AcceptOutcome::Empty) {
            self.issues_state.error = None;
            // A reload supersedes any in-flight detail load; discard it so a
            // stale detail never lands on the freshly-replaced list.
            self.issues_state.detail_pending = None;
            if self.issues_state.list.items().is_empty() {
                self.issues_state.issue_detail = None;
            }
        }
    }

    fn apply_issue_list_page_loaded(&mut self, page: IssueListPageLoadedData) {
        let identity = IssueListIdentity {
            scope_repo_id: page.scope_repo_id,
            filter: page.filter,
        };
        let result = PageResult {
            identity,
            request_id: ListRequestId::from_raw(page.request_id),
            requested_token: issue_page_token(page.request_cursor.clone()),
            items: page.issues,
            next_page: PageToken::from_cursor(page.cursor, page.has_more),
        };
        let outcome = self.issues_state.list.accept_page(result);
        if matches!(outcome, AcceptOutcome::Applied | AcceptOutcome::Empty) {
            self.issues_state.error = None;
        }
    }

    fn apply_issue_detail_loaded(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        issue_number: u64,
        request_id: u64,
        mut detail: crate::domain::IssueDetail,
    ) {
        let current_repo_id = self.selected_repository_id().cloned();
        if current_repo_id.as_ref() == Some(&scope_repo_id)
            && self.detail_pending_matches(&scope_repo_id, issue_number, request_id)
        {
            detail.comments.rebind_identity(CommentDetailIdentity {
                scope_repo_id,
                number: issue_number,
            });
            self.issues_state.error = None;
            self.issues_state.issue_detail = Some(detail);
            self.issues_state.loading.detail = false;
            self.issues_state.loading.comments = false;
            self.issues_state.detail_pending = None;
            self.issues_state.detail_subfocus = DetailSubfocus::Body;
            self.issues_state.detail_scroll_offset = 0;
        }
    }

    fn apply_issue_comments_page_loaded(&mut self, page: IssueCommentsPageLoadedData) {
        let result = PageResult {
            identity: CommentDetailIdentity {
                scope_repo_id: page.scope_repo_id,
                number: page.issue_number,
            },
            request_id: ListRequestId::from_raw(page.request_id),
            requested_token: issue_page_token(page.request_cursor),
            items: page.comments,
            next_page: PageToken::from_cursor(page.cursor, page.has_more),
        };
        let Some(detail) = &mut self.issues_state.issue_detail else {
            return;
        };
        let outcome = detail.comments.accept_page(result);
        if matches!(outcome, AcceptOutcome::Applied | AcceptOutcome::Empty) {
            self.issues_state.error = None;
            self.issues_state.loading.comments = false;
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

    /// Allocate and begin the next issue-comment page request.
    pub fn begin_issue_comment_page(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        issue_number: u64,
        cursor: Option<String>,
    ) -> Option<u64> {
        if !self.current_detail_matches(scope_repo_id, issue_number) {
            return None;
        }
        let detail = self.issues_state.issue_detail.as_mut()?;
        if detail.comments.has_pending_request() {
            return None;
        }
        detail.comments.rebind_identity(CommentDetailIdentity {
            scope_repo_id: scope_repo_id.clone(),
            number: issue_number,
        });
        let request_id = detail.comments.next_request_id().ok()?;
        let outcome = detail
            .comments
            .begin_page(issue_page_token(cursor), request_id);
        if matches!(outcome, crate::state::pagination::BeginOutcome::Started) {
            self.issues_state.loading.comments = true;
            Some(request_id.get())
        } else {
            None
        }
    }

    #[cfg(test)]
    pub(crate) fn mark_comments_page_loading(
        &mut self,
        scope_repo_id: RepositoryId,
        issue_number: u64,
        cursor: Option<String>,
    ) {
        let Some(detail) = self.issues_state.issue_detail.as_mut() else {
            return;
        };
        let token = issue_page_token(cursor);
        detail.comments = crate::domain::PaginatedList::from_loaded(
            CommentDetailIdentity {
                scope_repo_id,
                number: issue_number,
            },
            detail.comments.items().to_vec(),
            token.clone(),
        );
        if matches!(
            detail
                .comments
                .begin_page(token, ListRequestId::from_raw(0)),
            crate::state::pagination::BeginOutcome::Started
        ) {
            self.issues_state.loading.comments = true;
        }
    }

    pub fn mark_issue_list_page_loading(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        filter: IssueFilter,
        cursor: Option<String>,
    ) -> bool {
        self.mark_issue_list_page_loading_with_request_id(scope_repo_id, filter, cursor, 0)
    }

    pub fn mark_issue_list_page_loading_with_request_id(
        &mut self,
        _scope_repo_id: crate::domain::RepositoryId,
        _filter: IssueFilter,
        cursor: Option<String>,
        request_id: u64,
    ) -> bool {
        // Identity is reused from the prior reload's stored identity (a page
        // load only fires after a reload established scope+filter). The
        // requested-token + request-id correlation in `accept_page` rejects
        // stale pages, so the scope/filter args are not needed here.
        let token = issue_page_token(cursor);
        let started = self
            .issues_state
            .list
            .begin_page(token, ListRequestId::from_raw(request_id));
        matches!(started, crate::state::pagination::BeginOutcome::Started)
    }

    pub fn mark_issue_list_reload_loading(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        filter: IssueFilter,
        request_id: u64,
    ) {
        self.issues_state.list.begin_reload(
            IssueListIdentity {
                scope_repo_id,
                filter,
            },
            ListRequestId::from_raw(request_id),
        );
        // A reload supersedes any in-flight detail load; discard it so a stale
        // detail never lands on the freshly-replaced list.
        self.issues_state.detail_pending = None;
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
            AppEvent::IssueListLoaded { .. } | AppEvent::IssueListPageLoaded { .. } => {
                self.apply_issue_list_data(event);
            }
            AppEvent::IssueDetailLoaded {
                scope_repo_id,
                issue_number,
                request_id,
                detail,
            } => self.apply_issue_detail_loaded(scope_repo_id, issue_number, request_id, *detail),
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
            _ => {}
        }
    }

    /// Handle error events.
    pub(crate) fn apply_issues_error(&mut self, event: AppEvent) {
        match event {
            AppEvent::IssueListLoadFailed { .. }
            | AppEvent::IssueDetailLoadFailed { .. }
            | AppEvent::IssueCommentsPageFailed { .. } => self.apply_issue_load_error(event),
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
                scope_repo_id,
                *filter,
                request_id,
                request_cursor,
                error,
            ),
            AppEvent::IssueDetailLoadFailed {
                scope_repo_id,
                issue_number,
                request_id,
                error,
            } => {
                let current_repo_id = self.selected_repository_id().cloned();
                if current_repo_id.as_ref() == Some(&scope_repo_id)
                    && self.detail_pending_matches(&scope_repo_id, issue_number, request_id)
                {
                    self.issues_state.loading.detail = false;
                    self.issues_state.detail_pending = None;
                    self.issues_state.error = Some(error);
                }
            }
            AppEvent::IssueCommentsPageFailed {
                scope_repo_id,
                issue_number,
                request_id,
                request_cursor,
                error,
            } => self.apply_issue_comments_page_failed(
                scope_repo_id,
                issue_number,
                request_id,
                request_cursor,
                error,
            ),
            _ => {}
        }
    }

    /// Apply an issue-comment page failure via `PaginatedList::accept_failure`.
    fn apply_issue_comments_page_failed(
        &mut self,
        scope_repo_id: RepositoryId,
        issue_number: u64,
        request_id: u64,
        request_cursor: Option<String>,
        error: String,
    ) {
        if !self.current_detail_matches(&scope_repo_id, issue_number) {
            return;
        }
        let Some(detail) = &mut self.issues_state.issue_detail else {
            return;
        };
        let correlation = LoadCorrelation::Page {
            identity: CommentDetailIdentity {
                scope_repo_id,
                number: issue_number,
            },
            token: issue_page_token(request_cursor),
            request_id: ListRequestId::from_raw(request_id),
        };
        if matches!(
            detail.comments.accept_failure(&correlation),
            AcceptOutcome::Applied
        ) {
            self.issues_state.loading.comments = false;
            self.issues_state.error = Some(error);
        }
    }

    /// Apply an issue-list load failure via `PaginatedList::accept_failure`.
    ///
    /// A failure could correlate to either a reload or a page load; try the
    /// reload correlation first, then the page correlation. Whichever clears
    /// the pending marker derives `is_loading() == false`.
    fn apply_issue_list_load_failed(
        &mut self,
        scope_repo_id: RepositoryId,
        filter: IssueFilter,
        request_id: u64,
        request_cursor: Option<String>,
        error: String,
    ) {
        let identity = IssueListIdentity {
            scope_repo_id,
            filter,
        };
        let reload_correlation = LoadCorrelation::Reload {
            identity: identity.clone(),
            request_id: ListRequestId::from_raw(request_id),
        };
        let page_correlation = LoadCorrelation::Page {
            identity,
            token: issue_page_token(request_cursor),
            request_id: ListRequestId::from_raw(request_id),
        };
        let applied = matches!(
            self.issues_state.list.accept_failure(&reload_correlation),
            AcceptOutcome::Applied
        ) || matches!(
            self.issues_state.list.accept_failure(&page_correlation),
            AcceptOutcome::Applied
        );
        if applied {
            self.issues_state.error = Some(error);
        }
    }
}
