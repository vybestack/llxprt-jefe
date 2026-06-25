//! Pull Requests mode load/result state operations.
//!
//! @plan PLAN-20260624-PR-MODE.P05
//! @requirement REQ-PR-006
//! @requirement REQ-PR-007
//! @requirement REQ-PR-009
//! @requirement REQ-PR-010
//! @requirement REQ-PR-NFR-002
//! @pseudocode component-001 lines 209-247

use super::{
    AppEvent, AppState, PrDetailPending, PrDetailSubfocus, PrListPagePending, PrListReloadPending,
};
use crate::domain::{PrFilter, PullRequest, RepositoryId};

pub(super) struct PrListLoadedData {
    scope_repo_id: RepositoryId,
    filter: PrFilter,
    request_id: u64,
    pull_requests: Vec<PullRequest>,
    cursor: Option<String>,
    has_more: bool,
}

pub(super) struct PrListPageLoadedData {
    scope_repo_id: RepositoryId,
    request_id: u64,
    pull_requests: Vec<PullRequest>,
    cursor: Option<String>,
    has_more: bool,
}

pub(super) struct PrCommentsPageLoadedData {
    pub(super) scope_repo_id: RepositoryId,
    pub(super) pr_number: u64,
    pub(super) request_id: u64,
    pub(super) comments: Vec<crate::domain::IssueComment>,
    pub(super) cursor: Option<String>,
    pub(super) has_more: bool,
}

impl AppState {
    /// Apply a PR list loaded result with staleness guards.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-006
    /// @requirement REQ-PR-014
    /// @pseudocode component-001 lines 209-223
    pub(super) fn apply_pr_list_loaded(&mut self, list: PrListLoadedData) {
        if !self.pr_list_reload_pending_matches(&list.scope_repo_id, &list.filter, list.request_id)
        {
            return;
        }
        self.prs_state.error = None;
        self.prs_state.pull_requests = list.pull_requests;
        self.prs_state.list_cursor = list.cursor;
        self.prs_state.has_more_prs = list.has_more;
        self.prs_state.loading.list = false;
        self.prs_state.list_reload_pending = None;
        self.prs_state.list_page_pending = None;
        self.prs_state.detail_pending = None;
        if self.prs_state.pull_requests.is_empty() {
            self.prs_state.selected_pr_index = None;
            self.prs_state.pr_detail = None;
        } else {
            self.prs_state.selected_pr_index = Some(0);
        }
        self.prs_state.list_scroll_offset = 0;
    }

    /// Apply a PR list page (append) with staleness guards.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-007
    /// @pseudocode component-001 lines 224-229
    pub(super) fn apply_pr_list_page_loaded(&mut self, page: PrListPageLoadedData) {
        if !self.pr_list_page_pending_matches(&page.scope_repo_id, page.request_id) {
            return;
        }
        self.prs_state.pull_requests.extend(page.pull_requests);
        self.prs_state.list_cursor = page.cursor;
        self.prs_state.has_more_prs = page.has_more;
        self.prs_state.loading.list = false;
        self.prs_state.list_page_pending = None;
    }

    /// Apply a PR detail loaded result with staleness guards.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 230-235
    pub(super) fn apply_pr_detail_loaded(
        &mut self,
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        detail: crate::domain::PullRequestDetail,
    ) {
        if !self.pr_detail_pending_matches(&scope_repo_id, pr_number, request_id) {
            return;
        }
        self.prs_state.error = None;
        self.prs_state.pr_detail = Some(detail);
        self.prs_state.loading.detail = false;
        self.prs_state.detail_pending = None;
        self.prs_state.detail_subfocus = PrDetailSubfocus::Body;
        self.prs_state.detail_scroll_offset = 0;
    }

    /// Apply a PR comments page (append) with staleness guards.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @pseudocode component-001 lines 236-241
    pub(super) fn apply_pr_comments_page_loaded(&mut self, page: PrCommentsPageLoadedData) {
        if !self.pr_comments_page_pending_matches(
            &page.scope_repo_id,
            page.pr_number,
            page.request_id,
        ) {
            return;
        }
        if let Some(detail) = &mut self.prs_state.pr_detail
            && detail.number == page.pr_number
        {
            detail.comments.extend(page.comments);
            detail.comments_cursor = page.cursor;
            detail.has_more_comments = page.has_more;
            self.prs_state.error = None;
            self.prs_state.loading.comments = false;
            self.prs_state.comments_page_pending = None;
        }
    }

    /// Apply a PR list load failure (scoped error, never silent).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-002
    /// @pseudocode component-001 lines 242-247
    pub(super) fn apply_pr_list_load_failed(
        &mut self,
        scope_repo_id: &RepositoryId,
        request_id: u64,
        error: String,
    ) {
        if self.pr_list_reload_pending_matches_id(scope_repo_id, request_id) {
            self.prs_state.loading.list = false;
            self.prs_state.list_reload_pending = None;
            self.prs_state.error = Some(error);
        }
    }

    /// Apply a PR detail load failure (scoped error, never silent).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-002
    /// @pseudocode component-001 lines 242-247
    pub(super) fn apply_pr_detail_load_failed(
        &mut self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        request_id: u64,
        error: String,
    ) {
        if self.pr_detail_pending_matches(scope_repo_id, pr_number, request_id) {
            self.prs_state.loading.detail = false;
            self.prs_state.detail_pending = None;
            self.prs_state.error = Some(error);
        }
    }

    /// Apply a PR comments page failure (scoped error, never silent).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-002
    /// @pseudocode component-001 lines 242-247
    pub(super) fn apply_pr_comments_page_failed(
        &mut self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        request_id: u64,
        error: String,
    ) {
        if self.pr_comments_page_pending_matches(scope_repo_id, pr_number, request_id) {
            self.prs_state.loading.comments = false;
            self.prs_state.comments_page_pending = None;
            self.prs_state.error = Some(error);
        }
    }

    /// Check if a pending list-reload matches scope + filter + request_id.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-002
    /// @pseudocode component-001 lines 209-223
    fn pr_list_reload_pending_matches(
        &self,
        scope_repo_id: &RepositoryId,
        filter: &PrFilter,
        request_id: u64,
    ) -> bool {
        if request_id == 0 {
            return self.selected_repository_id() == Some(scope_repo_id)
                && self.prs_state.committed_filter == *filter;
        }
        self.selected_repository_id() == Some(scope_repo_id)
            && self.prs_state.committed_filter == *filter
            && self
                .prs_state
                .list_reload_pending
                .as_ref()
                .is_some_and(|pending| {
                    pending.scope_repo_id == *scope_repo_id
                        && pending.filter == *filter
                        && pending.request_id == request_id
                })
    }

    /// Check if a pending list-reload matches scope + request_id (filter-less variant).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-002
    /// @pseudocode component-001 lines 209-223
    fn pr_list_reload_pending_matches_id(
        &self,
        scope_repo_id: &RepositoryId,
        request_id: u64,
    ) -> bool {
        if request_id == 0 {
            return self.selected_repository_id() == Some(scope_repo_id);
        }
        self.selected_repository_id() == Some(scope_repo_id)
            && self
                .prs_state
                .list_reload_pending
                .as_ref()
                .is_some_and(|pending| {
                    pending.scope_repo_id == *scope_repo_id && pending.request_id == request_id
                })
    }

    /// Check if a pending list-page matches scope + request_id.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-002
    /// @pseudocode component-001 lines 224-225
    fn pr_list_page_pending_matches(&self, scope_repo_id: &RepositoryId, request_id: u64) -> bool {
        self.selected_repository_id() == Some(scope_repo_id)
            && self
                .prs_state
                .list_page_pending
                .as_ref()
                .is_some_and(|pending| {
                    pending.scope_repo_id == *scope_repo_id && pending.request_id == request_id
                })
    }

    /// Check if a pending detail request matches scope + pr_number + request_id.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 230-235
    fn pr_detail_pending_matches(
        &self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        request_id: u64,
    ) -> bool {
        let scope_ok = self.selected_repository_id() == Some(scope_repo_id);
        if !scope_ok {
            return false;
        }
        let selected_matches = self
            .prs_state
            .selected_pr_index
            .and_then(|idx| self.prs_state.pull_requests.get(idx))
            .is_some_and(|pr| pr.number == pr_number);
        if !selected_matches {
            return false;
        }
        if request_id == 0 {
            return true;
        }
        self.prs_state
            .detail_pending
            .as_ref()
            .is_some_and(|pending| {
                pending.scope_repo_id == *scope_repo_id
                    && pending.pr_number == pr_number
                    && pending.request_id == request_id
            })
    }

    /// Check if a pending comments-page matches scope + pr_number + request_id.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @pseudocode component-001 lines 236-241
    fn pr_comments_page_pending_matches(
        &self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        request_id: u64,
    ) -> bool {
        self.selected_repository_id() == Some(scope_repo_id)
            && self
                .prs_state
                .comments_page_pending
                .as_ref()
                .is_some_and(|pending| {
                    pending.scope_repo_id == *scope_repo_id
                        && pending.pr_number == pr_number
                        && pending.request_id == request_id
                })
    }

    /// Mark a PR list reload as loading (staleness tracking).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-002
    /// @pseudocode component-001 lines 209-223
    pub fn mark_pr_list_reload_loading(
        &mut self,
        scope_repo_id: RepositoryId,
        filter: PrFilter,
        request_id: u64,
    ) {
        self.prs_state.loading.list = true;
        self.prs_state.list_page_pending = None;
        self.prs_state.detail_pending = None;
        self.prs_state.list_reload_pending = Some(PrListReloadPending {
            scope_repo_id,
            filter,
            request_id,
        });
    }

    /// Mark a PR list page as loading (staleness tracking).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-007
    /// @pseudocode component-001 lines 224-229
    pub fn mark_pr_list_page_loading(
        &mut self,
        scope_repo_id: RepositoryId,
        filter: PrFilter,
        cursor: Option<String>,
        request_id: u64,
    ) {
        self.prs_state.loading.list = true;
        self.prs_state.list_reload_pending = None;
        self.prs_state.list_page_pending = Some(PrListPagePending {
            scope_repo_id,
            filter,
            cursor,
            request_id,
        });
    }

    /// Mark a PR detail as loading (staleness tracking).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 230-235
    pub fn mark_pr_detail_loading(
        &mut self,
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
    ) {
        self.prs_state.loading.detail = true;
        self.prs_state.detail_pending = Some(PrDetailPending {
            scope_repo_id,
            pr_number,
            request_id,
        });
    }

    /// Next PR detail request ID (staleness counter).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-002
    /// @pseudocode component-001 lines 88-98
    pub fn next_pr_detail_request_id(&mut self) -> u64 {
        let request_id = self.prs_state.next_pr_detail_request_id.saturating_add(1);
        self.prs_state.next_pr_detail_request_id = request_id;
        request_id
    }

    /// Next PR list request ID (staleness counter).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-002
    /// @pseudocode component-001 lines 88-98
    pub fn next_pr_list_request_id(&mut self) -> u64 {
        let request_id = self.prs_state.next_pr_list_request_id.saturating_add(1);
        self.prs_state.next_pr_list_request_id = request_id;
        request_id
    }

    /// Handle PR data-loaded events (dispatched from apply_prs_event).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-007
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 21-27,209-241
    pub(crate) fn apply_prs_data(&mut self, event: AppEvent) {
        match event {
            AppEvent::PrListLoaded {
                scope_repo_id,
                filter,
                request_id,
                pull_requests,
                cursor,
                has_more,
            } => self.apply_pr_list_loaded(PrListLoadedData {
                scope_repo_id,
                filter: *filter,
                request_id,
                pull_requests,
                cursor,
                has_more,
            }),
            AppEvent::PrListPageLoaded {
                scope_repo_id,
                request_id,
                pull_requests,
                cursor,
                has_more,
            } => self.apply_pr_list_page_loaded(PrListPageLoadedData {
                scope_repo_id,
                request_id,
                pull_requests,
                cursor,
                has_more,
            }),
            AppEvent::PrDetailLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                detail,
            } => self.apply_pr_detail_loaded(scope_repo_id, pr_number, request_id, *detail),
            AppEvent::PrCommentsPageLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                comments,
                cursor,
                has_more,
            } => self.apply_pr_comments_page_loaded(PrCommentsPageLoadedData {
                scope_repo_id,
                pr_number,
                request_id,
                comments,
                cursor,
                has_more,
            }),
            _ => {}
        }
    }

    /// Handle PR load-error events (scoped errors, never silent).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-002
    /// @pseudocode component-001 lines 242-247
    pub(crate) fn apply_prs_load_error(&mut self, event: AppEvent) {
        match event {
            AppEvent::PrListLoadFailed {
                scope_repo_id,
                request_id,
                error,
            } => self.apply_pr_list_load_failed(&scope_repo_id, request_id, error),
            AppEvent::PrDetailLoadFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            } => self.apply_pr_detail_load_failed(&scope_repo_id, pr_number, request_id, error),
            AppEvent::PrCommentsPageFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            } => self.apply_pr_comments_page_failed(&scope_repo_id, pr_number, request_id, error),
            _ => {}
        }
    }
}
