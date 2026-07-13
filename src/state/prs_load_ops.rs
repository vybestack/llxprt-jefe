//! Pull Requests mode load/result state operations.
//!
//! @plan PLAN-20260624-PR-MODE.P05
//! @requirement REQ-PR-006
//! @requirement REQ-PR-007
//! @requirement REQ-PR-009
//! @requirement REQ-PR-010
//! @requirement REQ-PR-NFR-002
//! @pseudocode component-001 lines 209-247

use super::{AppEvent, AppState, PrDetailPending, PrDetailSubfocus, PrListIdentity};
use crate::domain::{
    CommentDetailIdentity, ListRequestId, PageToken, PrFilter, PullRequest, RepositoryId,
};
use crate::state::pagination::{AcceptOutcome, LoadCorrelation, PageResult, ReloadResult};

pub(super) struct PrListLoadedData {
    scope_repo_id: RepositoryId,
    filter: PrFilter,
    request_id: u64,
    pull_requests: Vec<PullRequest>,
    cursor: Option<String>,
    has_more: bool,
}

/// Payload for a silent background refresh (issue #128). Mirrors
/// `PrListLoadedData` but the reducer preserves selection/scroll/detail.
pub(super) struct PrListSilentRefreshedData {
    pub(super) scope_repo_id: RepositoryId,
    pub(super) filter: PrFilter,
    pub(super) request_id: u64,
    pub(super) pull_requests: Vec<PullRequest>,
    pub(super) cursor: Option<String>,
    pub(super) has_more: bool,
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

/// Build the cursor-based [`PageToken`] used to request a page from the cursor
/// the dispatch layer intends to fetch. A `None` cursor collapses to `Done`
/// (matching `PageToken::from_cursor`).
fn pr_page_token(cursor: Option<String>) -> PageToken {
    cursor.map_or(PageToken::Done, PageToken::Cursor)
}

impl AppState {
    /// Apply a PR list loaded result via `PaginatedList::accept_loaded`.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-006
    /// @requirement REQ-PR-014
    /// @pseudocode component-001 lines 209-223
    pub(super) fn apply_pr_list_loaded(&mut self, list: PrListLoadedData) {
        let identity = PrListIdentity {
            scope_repo_id: list.scope_repo_id,
            filter: list.filter,
        };
        let result = ReloadResult {
            identity,
            request_id: ListRequestId::from_raw(list.request_id),
            items: list.pull_requests,
            next_page: PageToken::from_cursor(list.cursor, list.has_more),
        };
        let outcome = self.prs_state.list.accept_loaded(result);
        if matches!(outcome, AcceptOutcome::Applied | AcceptOutcome::Empty) {
            self.prs_state.error = None;
            // Do NOT clear detail_pending here — clearing it cancels any
            // in-flight detail load (e.g. the post-merge detail reload). The
            // detail staleness guard (pr_detail_pending_matches) already
            // discards stale results when the scope or selected PR number
            // changes (issue #128).
            if self.prs_state.list.items().is_empty() {
                self.prs_state.pr_detail = None;
            } else {
                // The previous pr_detail is STALE (it is for a PR from the
                // prior list). Clear it so the detail pane does not show old
                // content until the fresh detail/preview load repopulates it.
                self.prs_state.pr_detail = None;
                self.prs_state.detail_subfocus = PrDetailSubfocus::Body;
                self.prs_state.detail_scroll_offset = 0;
            }
            self.prs_state.list_scroll_offset = 0;
        }
    }

    /// Apply a silent background refresh (issue #128). Mirrors
    /// `apply_pr_list_loaded` but preserves selection, scroll offset, and
    /// `pr_detail`, and does NOT set `loading.list` (no spinner flash).
    ///
    /// @requirement issue #128
    pub(super) fn apply_pr_list_silent_refreshed(&mut self, data: PrListSilentRefreshedData) {
        let identity = PrListIdentity {
            scope_repo_id: data.scope_repo_id,
            filter: data.filter,
        };
        // Remember the selected PR by number so we can follow it across a
        // reorder or list replacement.
        let selected_pr_number = self
            .prs_state
            .selected_pr_index()
            .and_then(|idx| self.prs_state.pull_requests().get(idx))
            .map(|pr| pr.number);
        let result = ReloadResult {
            identity,
            request_id: ListRequestId::from_raw(data.request_id),
            items: data.pull_requests,
            next_page: PageToken::from_cursor(data.cursor, data.has_more),
        };
        let outcome = self.prs_state.list.accept_loaded(result);
        if matches!(outcome, AcceptOutcome::Applied | AcceptOutcome::Empty) {
            self.prs_state.error = None;
            // Do NOT clear detail_pending or pr_detail — the detail reload is
            // a separate operation; preserve the current detail until it
            // arrives.
            self.preserve_silent_refresh_selection(selected_pr_number);
        }
    }

    /// Re-derive selection + scroll after a silent refresh (issue #128 helper).
    fn preserve_silent_refresh_selection(&mut self, selected_pr_number: Option<u64>) {
        if self.prs_state.pull_requests().is_empty() {
            self.prs_state.list.set_selected_index(None);
            // Do NOT clear pr_detail on an empty silent refresh (issue #128):
            // the detail pane keeps showing the last-loaded detail until the
            // next manual reload, avoiding an empty flash.
            return;
        }
        // Follow the previously-selected PR by number; fall back to first.
        let new_index = selected_pr_number
            .and_then(|num| {
                self.prs_state
                    .pull_requests()
                    .iter()
                    .position(|pr| pr.number == num)
            })
            .unwrap_or(0);
        self.prs_state.list.set_selected_index(Some(new_index));
        // Clamp the scroll offset so it never exceeds the new list bounds.
        let max_scroll = self.prs_state.pull_requests().len().saturating_sub(1);
        if self.prs_state.list_scroll_offset > max_scroll {
            self.prs_state.list_scroll_offset = max_scroll;
        }
    }

    /// Apply a silent background refresh failure (issue #128). Clears the
    /// pending marker WITHOUT surfacing an error (silent, non-disruptive).
    ///
    /// @requirement issue #128
    pub(super) fn apply_pr_list_silent_refresh_failed(
        &mut self,
        scope_repo_id: &RepositoryId,
        request_id: u64,
    ) {
        // The event scope must match the list's stored identity scope so a
        // silent failure from a different repository cannot cancel the current
        // refresh.
        let Some(identity) = self.prs_state.list.identity().cloned() else {
            return;
        };
        if identity.scope_repo_id != *scope_repo_id {
            return;
        }
        let correlation = LoadCorrelation::Reload {
            identity,
            request_id: ListRequestId::from_raw(request_id),
        };
        self.prs_state.list.accept_failure(&correlation);
        // Do NOT set error — background failures are silent.
    }

    /// Apply a PR list page (append) via `PaginatedList::accept_page`.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-007
    /// @pseudocode component-001 lines 224-229
    pub(super) fn apply_pr_list_page_loaded(&mut self, page: PrListPageLoadedData) {
        let Some(identity) = self.prs_state.list.identity().cloned() else {
            return;
        };
        // The event scope must match the list's stored identity scope; a page
        // response from a different repository must never be appended.
        if identity.scope_repo_id != page.scope_repo_id {
            return;
        }
        // The event does not carry the request cursor; derive the
        // requested_token from the stored next_page (the token used to begin
        // the page load — it is not updated until accept_page succeeds).
        let requested_token = self.prs_state.list.next_page().clone();
        let result = PageResult {
            identity,
            request_id: ListRequestId::from_raw(page.request_id),
            requested_token,
            items: page.pull_requests,
            next_page: PageToken::from_cursor(page.cursor, page.has_more),
        };
        let outcome = self.prs_state.list.accept_page(result);
        if matches!(outcome, AcceptOutcome::Applied | AcceptOutcome::Empty) {
            self.prs_state.error = None;
        }
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
        mut detail: crate::domain::PullRequestDetail,
    ) {
        if !self.pr_detail_pending_matches(&scope_repo_id, pr_number, request_id) {
            return;
        }
        detail.comments.rebind_identity(CommentDetailIdentity {
            scope_repo_id,
            number: pr_number,
        });
        self.prs_state.error = None;
        self.prs_state.pr_detail = Some(detail);
        self.prs_state.loading.detail = false;
        self.prs_state.loading.comments = false;
        self.prs_state.detail_pending = None;
        self.prs_state.detail_subfocus = PrDetailSubfocus::Body;
        self.prs_state.detail_scroll_offset = 0;
    }

    /// Apply a silent background detail refresh (issue #128). Mirrors
    /// `apply_pr_detail_loaded` but does NOT set `loading.detail`, does NOT
    /// reset `detail_subfocus` or `detail_scroll_offset`, and does NOT set an
    /// error. Preserves the user's scroll/focus position.
    ///
    /// @requirement issue #128
    pub(super) fn apply_pr_detail_silent_refreshed(
        &mut self,
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        mut detail: crate::domain::PullRequestDetail,
    ) {
        if !self.pr_detail_pending_matches(&scope_repo_id, pr_number, request_id) {
            return;
        }
        detail.comments.rebind_identity(CommentDetailIdentity {
            scope_repo_id,
            number: pr_number,
        });
        // Do NOT set loading.detail (silent), do NOT reset detail_subfocus or
        // detail_scroll_offset, do NOT set error.
        self.prs_state.pr_detail = Some(detail);
        self.prs_state.loading.comments = false;
        self.prs_state.detail_pending = None;
    }

    /// Apply a silent background detail refresh failure (issue #128). Clears
    /// `detail_pending` silently WITHOUT setting `loading.detail` or an error.
    ///
    /// @requirement issue #128
    pub(super) fn apply_pr_detail_silent_refresh_failed(
        &mut self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        request_id: u64,
    ) {
        if self.pr_detail_pending_matches(scope_repo_id, pr_number, request_id) {
            self.prs_state.detail_pending = None;
        }
    }

    /// Apply a PR comments page (append) with staleness guards.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @pseudocode component-001 lines 236-241
    pub(super) fn apply_pr_comments_page_loaded(&mut self, page: PrCommentsPageLoadedData) {
        let Some(detail) = &mut self.prs_state.pr_detail else {
            return;
        };
        let result = PageResult {
            identity: CommentDetailIdentity {
                scope_repo_id: page.scope_repo_id,
                number: page.pr_number,
            },
            request_id: ListRequestId::from_raw(page.request_id),
            requested_token: detail.comments.next_page().clone(),
            items: page.comments,
            next_page: PageToken::from_cursor(page.cursor, page.has_more),
        };
        let outcome = detail.comments.accept_page(result);
        if matches!(outcome, AcceptOutcome::Applied | AcceptOutcome::Empty) {
            self.prs_state.error = None;
            self.prs_state.loading.comments = false;
        }
    }

    /// Apply a PR list load failure via `PaginatedList::accept_failure`.
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
        // A failure could be for a reload or a page load. Try the reload
        // correlation first, then the page correlation (the pending operation
        // is exactly one of the two). Either clearing the pending marker
        // derives `is_loading() == false`.
        let Some(identity) = self.prs_state.list.identity().cloned() else {
            // No stored identity: nothing pending can match.
            return;
        };
        // The event scope must match the list's stored identity scope so a
        // failure from a different repository cannot clear the current request.
        if identity.scope_repo_id != *scope_repo_id {
            return;
        }
        let request_id = ListRequestId::from_raw(request_id);
        let reload_correlation = LoadCorrelation::Reload {
            identity: identity.clone(),
            request_id,
        };
        let outcome = self.prs_state.list.accept_failure(&reload_correlation);
        let applied = matches!(outcome, AcceptOutcome::Applied)
            || matches!(
                self.prs_state.list.accept_failure(&LoadCorrelation::Page {
                    identity,
                    token: self.prs_state.list.next_page().clone(),
                    request_id,
                }),
                AcceptOutcome::Applied
            );
        if applied {
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
    /// Always surfaces the error when the scope matches the selected repo, so
    /// a failure is never silently dropped even when no pending comment-page
    /// request can correlate it (e.g. the detail was cleared while the request
    /// was in flight). Clears `loading.comments` if a matching pending was
    /// canceled; an unmatched failure leaves loading alone since some other
    /// request may legitimately be in flight.
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
        if self.selected_repository_id() != Some(scope_repo_id) {
            return;
        }
        if let Some(detail) = &mut self.prs_state.pr_detail
            && detail.number == pr_number
        {
            let correlation = LoadCorrelation::Page {
                identity: CommentDetailIdentity {
                    scope_repo_id: scope_repo_id.clone(),
                    number: pr_number,
                },
                token: detail.comments.next_page().clone(),
                request_id: ListRequestId::from_raw(request_id),
            };
            if matches!(
                detail.comments.accept_failure(&correlation),
                AcceptOutcome::Applied
            ) {
                self.prs_state.loading.comments = false;
            }
        }
        self.prs_state.error = Some(error);
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
            .selected_pr_index()
            .and_then(|idx| self.prs_state.pull_requests().get(idx))
            .is_some_and(|pr| pr.number == pr_number);
        if !selected_matches {
            return false;
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
        self.prs_state.list.begin_reload(
            PrListIdentity {
                scope_repo_id,
                filter,
            },
            ListRequestId::from_raw(request_id),
        );
        // A reload supersedes any in-flight detail load; discard it so a stale
        // detail never lands on the freshly-replaced list.
        self.prs_state.detail_pending = None;
    }

    /// Mark a silent background refresh as pending (issue #128). Does NOT set
    /// `loading.list` (no spinner flash) and does NOT clear `detail_pending`.
    pub fn mark_pr_list_silent_refresh_loading(
        &mut self,
        scope_repo_id: RepositoryId,
        filter: PrFilter,
        request_id: u64,
    ) {
        self.prs_state.list.begin_silent_reload(
            PrListIdentity {
                scope_repo_id,
                filter,
            },
            ListRequestId::from_raw(request_id),
        );
    }

    /// Mark a PR list page as loading (staleness tracking).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-007
    /// @pseudocode component-001 lines 224-229
    pub fn mark_pr_list_page_loading(
        &mut self,
        _scope_repo_id: RepositoryId,
        _filter: PrFilter,
        cursor: Option<String>,
        request_id: u64,
    ) -> bool {
        // Identity is reused from the prior reload's stored identity (a page
        // load only fires after a reload established scope+filter).
        let token = pr_page_token(cursor);
        let started = self
            .prs_state
            .list
            .begin_page(token, ListRequestId::from_raw(request_id));
        matches!(started, crate::state::pagination::BeginOutcome::Started)
    }

    /// Allocate a request id and begin a visible reload on the PR list,
    /// mirroring the Actions `begin_actions_reload` pattern. Used by the
    /// reducer's scope-change / filter-change reset paths so `list_pending()`
    /// is observable before the dispatch layer spawns the fetch.
    pub(super) fn begin_prs_reload(&mut self, repo_id: RepositoryId) {
        let Ok(request_id) = self.prs_state.list.next_request_id() else {
            return;
        };
        let identity = PrListIdentity {
            scope_repo_id: repo_id,
            filter: self.prs_state.committed_filter.clone(),
        };
        self.prs_state.list.begin_reload(identity, request_id);
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
    /// Allocate and begin the next PR-comment page request.
    pub fn begin_pr_comment_page(
        &mut self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        cursor: Option<String>,
    ) -> Option<u64> {
        if self.selected_repository_id() != Some(scope_repo_id) {
            return None;
        }
        let detail = self.prs_state.pr_detail.as_mut()?;
        if detail.number != pr_number || detail.comments.has_pending_request() {
            return None;
        }
        detail.comments.rebind_identity(CommentDetailIdentity {
            scope_repo_id: scope_repo_id.clone(),
            number: pr_number,
        });
        let request_id = detail.comments.next_request_id().ok()?;
        let outcome = detail
            .comments
            .begin_page(pr_page_token(cursor), request_id);
        if matches!(outcome, crate::state::pagination::BeginOutcome::Started) {
            self.prs_state.loading.comments = true;
            // Clear any stale error from a prior failed page so the UI does
            // not show both an error and the loading spinner at once.
            self.prs_state.error = None;
            Some(request_id.get())
        } else {
            None
        }
    }

    /// Mark a PR detail silent refresh as pending (issue #128). Sets
    /// `detail_pending` for staleness tracking but does NOT set
    /// `loading.detail` (no spinner flash).
    ///
    /// @requirement issue #128
    pub fn mark_pr_detail_silent_loading(
        &mut self,
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
    ) {
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

    /// Next PR list request ID via `PaginatedList::next_request_id`.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-002
    /// @pseudocode component-001 lines 88-98
    pub fn next_pr_list_request_id(&mut self) -> u64 {
        self.prs_state
            .list
            .next_request_id()
            .map_or(0, ListRequestId::get)
    }

    /// Handle PR data-loaded events (dispatched from apply_prs_event).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-007
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 21-27,209-241
    pub(crate) fn apply_prs_data(&mut self, event: AppEvent) {
        if let AppEvent::PrListSilentRefreshed { .. } = event {
            self.apply_prs_silent_list_data(event);
            return;
        }
        match event {
            AppEvent::PrListLoaded { .. } => self.apply_prs_list_loaded_data(event),
            AppEvent::PrListPageLoaded { .. } => self.apply_prs_list_page_data(event),
            detail_event @ (AppEvent::PrDetailLoaded { .. }
            | AppEvent::PrDetailSilentRefreshed { .. }) => {
                self.apply_prs_detail_data(detail_event);
            }
            AppEvent::PrCommentsPageLoaded { .. } => self.apply_prs_comments_data(event),
            _ => {}
        }
    }

    /// Apply a silent list refresh event (issue #128). Extracted from
    /// `apply_prs_data` to keep it under the per-function line limit.
    fn apply_prs_silent_list_data(&mut self, event: AppEvent) {
        if let AppEvent::PrListSilentRefreshed {
            scope_repo_id,
            filter,
            request_id,
            pull_requests,
            cursor,
            has_more,
        } = event
        {
            self.apply_pr_list_silent_refreshed(PrListSilentRefreshedData {
                scope_repo_id,
                filter: *filter,
                request_id,
                pull_requests,
                cursor,
                has_more,
            });
        }
    }

    /// Apply a `PrListLoaded` event. Extracted from `apply_prs_data`.
    fn apply_prs_list_loaded_data(&mut self, event: AppEvent) {
        if let AppEvent::PrListLoaded {
            scope_repo_id,
            filter,
            request_id,
            pull_requests,
            cursor,
            has_more,
        } = event
        {
            self.apply_pr_list_loaded(PrListLoadedData {
                scope_repo_id,
                filter: *filter,
                request_id,
                pull_requests,
                cursor,
                has_more,
            });
        }
    }

    /// Apply a `PrListPageLoaded` event. Extracted from `apply_prs_data`.
    fn apply_prs_list_page_data(&mut self, event: AppEvent) {
        if let AppEvent::PrListPageLoaded {
            scope_repo_id,
            request_id,
            pull_requests,
            cursor,
            has_more,
        } = event
        {
            self.apply_pr_list_page_loaded(PrListPageLoadedData {
                scope_repo_id,
                request_id,
                pull_requests,
                cursor,
                has_more,
            });
        }
    }

    /// Apply a `PrCommentsPageLoaded` event. Extracted from `apply_prs_data`.
    fn apply_prs_comments_data(&mut self, event: AppEvent) {
        if let AppEvent::PrCommentsPageLoaded {
            scope_repo_id,
            pr_number,
            request_id,
            comments,
            cursor,
            has_more,
        } = event
        {
            self.apply_pr_comments_page_loaded(PrCommentsPageLoadedData {
                scope_repo_id,
                pr_number,
                request_id,
                comments,
                cursor,
                has_more,
            });
        }
    }

    /// Apply a detail data event (loud or silent). Extracted from
    /// `apply_prs_data` to keep it under the per-function line limit.
    ///
    /// @requirement issue #128
    fn apply_prs_detail_data(&mut self, event: AppEvent) {
        match event {
            AppEvent::PrDetailLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                detail,
            } => self.apply_pr_detail_loaded(scope_repo_id, pr_number, request_id, *detail),
            AppEvent::PrDetailSilentRefreshed {
                scope_repo_id,
                pr_number,
                request_id,
                detail,
            } => {
                self.apply_pr_detail_silent_refreshed(
                    scope_repo_id,
                    pr_number,
                    request_id,
                    *detail,
                );
            }
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
            AppEvent::PrListSilentRefreshFailed {
                scope_repo_id,
                request_id,
            } => self.apply_pr_list_silent_refresh_failed(&scope_repo_id, request_id),
            AppEvent::PrDetailLoadFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            } => self.apply_pr_detail_load_failed(&scope_repo_id, pr_number, request_id, error),
            AppEvent::PrDetailSilentRefreshFailed {
                scope_repo_id,
                pr_number,
                request_id,
            } => self.apply_pr_detail_silent_refresh_failed(&scope_repo_id, pr_number, request_id),
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
