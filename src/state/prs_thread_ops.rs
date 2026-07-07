//! Pull Requests mode review-thread state operations (issue #119).
//!
//! Handles opening the thread-reply composer, toggling resolve/unresolve
//! pending state, and applying resolve succeeded/failed results. Thread
//! navigation uses a flat index across all reviews' `review_threads` (matching
//! `PrDetailSubfocus::ReviewThread(usize)` and the renderer's flattening).
//!
//! @plan PLAN-20260624-PR-MODE.P05
//! @requirement REQ-PR-009

use super::{
    AppEvent, AppState, ComposerTarget, InlineState, PrDetailSubfocus, PrFocus,
    PrThreadResolvePending,
};
use crate::domain::{PrReviewThread, RepositoryId};

impl AppState {
    /// Apply review-thread events (open reply, toggle resolve, succeeded/failed).
    ///
    /// Returns `true` when the event was handled. Added to the dispatch chain
    /// in `apply_prs_event`.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    pub(super) fn apply_pr_thread_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::PrOpenThreadReplyComposer { thread_index } => {
                self.pr_open_thread_reply_composer(*thread_index);
                true
            }
            AppEvent::PrToggleThreadResolve { thread_index } => {
                self.pr_toggle_thread_resolve(*thread_index);
                true
            }
            AppEvent::PrThreadResolveSucceeded {
                scope_repo_id,
                thread_index,
                is_resolved,
                request_id,
            } => {
                self.pr_thread_resolve_succeeded(
                    scope_repo_id,
                    *thread_index,
                    *is_resolved,
                    *request_id,
                );
                true
            }
            AppEvent::PrThreadResolveFailed {
                scope_repo_id,
                thread_index,
                request_id,
                error,
            } => {
                self.pr_thread_resolve_failed(scope_repo_id, *thread_index, *request_id, error);
                true
            }
            _ => false,
        }
    }

    /// Open the thread-reply composer for the given flat thread index.
    ///
    /// Prefills with `@author ` from the thread's last comment author. Sets
    /// subfocus to `ReviewThread(thread_index)`. No-op when an inline composer
    /// is already active or when the thread index is out of range.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    fn pr_open_thread_reply_composer(&mut self, thread_index: usize) {
        if self.prs_state.pr_focus != PrFocus::PrDetail {
            return;
        }
        if self.prs_state.inline_state != InlineState::None {
            return;
        }
        let thread = self.pr_find_thread(thread_index);
        let Some(thread) = thread else {
            return;
        };
        let author = thread
            .comments
            .last()
            .map(|c| format!("@{} ", c.author_login))
            .unwrap_or_default();
        let cursor = author.len();
        self.prs_state.inline_state = InlineState::Composer {
            target: ComposerTarget::ReplyToReviewThread {
                thread_index,
                author: author.clone(),
            },
            text: author,
            cursor,
        };
        self.prs_state.detail_subfocus = PrDetailSubfocus::ReviewThread(thread_index);
    }

    /// Toggle resolve/unresolve on a review thread.
    ///
    /// Sets `thread_resolve_pending` with the target state (resolve=true for an
    /// unresolved thread, resolve=false for a resolved one). The dispatch layer
    /// spawns the actual GraphQL mutation and emits succeeded/failed.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    fn pr_toggle_thread_resolve(&mut self, thread_index: usize) {
        let Some(thread) = self.pr_find_thread(thread_index) else {
            return;
        };
        let resolve = !thread.is_resolved;
        let Some(scope) = self.selected_repository_id().cloned() else {
            self.prs_state.error = Some("No repository selected".to_string());
            return;
        };
        let request_id = self
            .prs_state
            .next_thread_resolve_request_id
            .saturating_add(1);
        self.prs_state.next_thread_resolve_request_id = request_id;
        self.prs_state.thread_resolve_pending = Some(PrThreadResolvePending {
            scope_repo_id: scope,
            thread_index,
            resolve,
            request_id,
        });
    }

    /// Apply a successful resolve/unresolve mutation.
    ///
    /// Flips the thread's `is_resolved` to the returned value and clears
    /// pending — but only when the request_id matches (staleness guard).
    /// Out-of-range thread indices clear pending without panic.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    fn pr_thread_resolve_succeeded(
        &mut self,
        scope_repo_id: &RepositoryId,
        thread_index: usize,
        is_resolved: bool,
        request_id: u64,
    ) {
        if !self.scope_repo_id_matches_pr(scope_repo_id) {
            return;
        }
        if !self.pr_thread_resolve_request_matches(request_id) {
            return;
        }
        self.prs_state.thread_resolve_pending = None;
        if let Some(thread) =
            Self::pr_find_thread_mut(self.prs_state.pr_detail.as_mut(), thread_index)
        {
            thread.is_resolved = is_resolved;
        }
    }

    /// Apply a failed resolve/unresolve mutation.
    ///
    /// Clears pending and sets a visible error — but only when the request_id
    /// matches (staleness guard).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    fn pr_thread_resolve_failed(
        &mut self,
        scope_repo_id: &RepositoryId,
        _thread_index: usize,
        request_id: u64,
        error: &str,
    ) {
        if !self.scope_repo_id_matches_pr(scope_repo_id) {
            return;
        }
        if !self.pr_thread_resolve_request_matches(request_id) {
            return;
        }
        self.prs_state.thread_resolve_pending = None;
        self.prs_state.error = Some(error.to_string());
    }

    /// Check whether a request_id matches the current pending resolve.
    fn pr_thread_resolve_request_matches(&self, request_id: u64) -> bool {
        self.prs_state
            .thread_resolve_pending
            .as_ref()
            .is_some_and(|p| p.request_id == request_id)
    }

    /// Borrow a review thread by flat index (immutable).
    fn pr_find_thread(&self, thread_index: usize) -> Option<&PrReviewThread> {
        let detail = self.prs_state.pr_detail.as_ref()?;
        detail
            .reviews
            .iter()
            .flat_map(|r| &r.review_threads)
            .nth(thread_index)
    }

    /// Borrow a review thread by flat index (mutable).
    ///
    /// Walks `reviews → review_threads` in order, counting the flat index.
    /// This is the single source of truth for the flat-index → thread mapping,
    /// shared by the navigation cycle and the rendering projection.
    fn pr_find_thread_mut(
        detail: Option<&mut crate::domain::PullRequestDetail>,
        thread_index: usize,
    ) -> Option<&mut PrReviewThread> {
        let detail = detail?;
        let mut idx = 0usize;
        for review in &mut detail.reviews {
            for thread in &mut review.review_threads {
                if idx == thread_index {
                    return Some(thread);
                }
                idx += 1;
            }
        }
        None
    }

    /// Check if a scope_repo_id matches the currently-selected repository.
    ///
    /// Mirrors `scope_repo_id_matches` in `prs_ops.rs` but is scoped to this
    /// module to keep thread operations self-contained.
    fn scope_repo_id_matches_pr(&self, scope_repo_id: &RepositoryId) -> bool {
        self.selected_repository_index
            .and_then(|idx| self.repositories.get(idx))
            .is_some_and(|repo| &repo.id == scope_repo_id)
    }
}
