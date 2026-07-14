//! Actions mode list-load/result state operations.
//!
//! All list-load correlation is delegated to [`PaginatedList`]; these helpers
//! only construct identity/result values, delegate, and apply Actions-specific
//! side effects (clearing the error, resetting run detail after a reload)
//! based on the returned [`AcceptOutcome`].
//!
//! [`PaginatedList`]: crate::state::pagination::PaginatedList
//! [`AcceptOutcome`]: crate::state::pagination::AcceptOutcome

use super::{ActionsListIdentity, AppState};
use crate::domain::{ActionsFilter, ListRequestId, PageToken, RepositoryId, WorkflowRun};
use crate::state::pagination::{AcceptOutcome, LoadCorrelation, PageResult, ReloadResult};

/// Bundled fields extracted from `ActionsMessage::RunsLoaded` /
/// `RunsPageLoaded` so that reload/page-apply functions stay within the clippy
/// argument limit.
pub(super) struct RunsLoadData {
    pub(super) scope_repo_id: RepositoryId,
    pub(super) filter: ActionsFilter,
    pub(super) page: u32,
    pub(super) request_id: u64,
    pub(super) runs: Vec<WorkflowRun>,
    pub(super) has_more: bool,
}

impl AppState {
    /// Apply a completed reload (page-1 replace) via `PaginatedList::accept_loaded`.
    pub(super) fn reload_runs(&mut self, data: RunsLoadData) -> bool {
        let identity = ActionsListIdentity {
            scope_repo_id: data.scope_repo_id,
            filter: data.filter,
        };
        let result = ReloadResult {
            identity,
            request_id: ListRequestId::from_raw(data.request_id),
            items: data.runs,
            next_page: PageToken::after_page(data.page, data.has_more),
        };
        let outcome = self.actions_state.list.accept_loaded(result);
        if matches!(outcome, AcceptOutcome::Applied | AcceptOutcome::Empty) {
            self.actions_state.error = None;
            self.actions_state.run_detail = None;
            self.reset_actions_inspection();
            self.actions_state.loading.detail = false;
            self.actions_state.detail_pending = None;
        }
        true
    }

    /// Apply a completed page append (load-more) via `PaginatedList::accept_page`.
    pub(super) fn apply_runs_page_loaded(&mut self, data: RunsLoadData) -> bool {
        let identity = ActionsListIdentity {
            scope_repo_id: data.scope_repo_id,
            filter: data.filter,
        };
        let result = PageResult {
            identity,
            request_id: ListRequestId::from_raw(data.request_id),
            requested_token: PageToken::PageNumber(data.page),
            items: data.runs,
            next_page: PageToken::after_page(data.page, data.has_more),
        };
        let outcome = self.actions_state.list.accept_page(result);
        if matches!(outcome, AcceptOutcome::Applied | AcceptOutcome::Empty) {
            self.actions_state.error = None;
        }
        true
    }

    /// Apply a reload failure via `PaginatedList::accept_failure`.
    pub(super) fn fail_runs_load(
        &mut self,
        scope_repo_id: RepositoryId,
        filter: ActionsFilter,
        request_id: u64,
        error: String,
    ) -> bool {
        let identity = ActionsListIdentity {
            scope_repo_id,
            filter,
        };
        let correlation = LoadCorrelation::Reload {
            identity,
            request_id: ListRequestId::from_raw(request_id),
        };
        let outcome = self.actions_state.list.accept_failure(&correlation);
        if matches!(outcome, AcceptOutcome::Applied) {
            self.actions_state.error = Some(error);
        }
        true
    }

    /// Apply a page-load failure via `PaginatedList::accept_failure`.
    pub(super) fn fail_runs_page_load(
        &mut self,
        scope_repo_id: RepositoryId,
        filter: ActionsFilter,
        page: u32,
        request_id: u64,
        error: String,
    ) -> bool {
        let identity = ActionsListIdentity {
            scope_repo_id,
            filter,
        };
        let correlation = LoadCorrelation::Page {
            identity,
            token: PageToken::PageNumber(page),
            request_id: ListRequestId::from_raw(request_id),
        };
        let outcome = self.actions_state.list.accept_failure(&correlation);
        if matches!(outcome, AcceptOutcome::Applied) {
            self.actions_state.error = Some(error);
        }
        true
    }

    /// Reset run-list state and start a fresh visible reload via
    /// `PaginatedList::begin_reload`. The request id is allocated centrally.
    pub(super) fn trigger_list_reload(&mut self) -> bool {
        self.actions_state.list.clear_items();
        self.actions_state.run_detail = None;
        self.reset_actions_inspection();
        self.actions_state.loading.detail = false;
        self.actions_state.detail_pending = None;
        if let Some(repo_id) = self.selected_repository().map(|r| r.id.clone()) {
            self.begin_actions_reload(repo_id);
        }
        true
    }

    /// Allocate a request id and begin a visible reload on the list.
    pub(super) fn begin_actions_reload(&mut self, repo_id: RepositoryId) {
        self.reset_actions_inspection();
        let Ok(request_id) = self.actions_state.list.next_request_id() else {
            return;
        };
        let identity = ActionsListIdentity {
            scope_repo_id: repo_id,
            filter: self.actions_state.committed_filter.clone(),
        };
        self.actions_state.list.begin_reload(identity, request_id);
    }
}
