//! Pull Requests mode mutation state operations (comment-create lifecycle).
//!
//! @plan PLAN-20260624-PR-MODE.P05
//! @requirement REQ-PR-010
//! @pseudocode component-001 lines 316-327

use super::{AppEvent, AppState, InlineState, PrDetailSubfocus};
use crate::domain::RepositoryId;

impl AppState {
    /// Handle PR mutation events (CommentCreated, CommentCreateFailed, MutationFailed).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @pseudocode component-001 lines 316-327
    pub(super) fn apply_pr_mutation_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::PrCommentCreated {
                scope_repo_id,
                pr_number,
                mutation_id,
                comment,
            } => self.apply_pr_comment_created(&scope_repo_id, pr_number, mutation_id, comment),
            AppEvent::PrCommentCreateFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            } => self.apply_pr_comment_create_failed(&scope_repo_id, pr_number, mutation_id, error),
            AppEvent::PrMutationFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            } => self.apply_pr_mutation_failed(&scope_repo_id, pr_number, mutation_id, error),
            _ => false,
        }
    }

    /// Apply a successful comment creation: append comment, clear composer, follow viewport.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @pseudocode component-001 lines 316-322
    fn apply_pr_comment_created(
        &mut self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        comment: crate::domain::IssueComment,
    ) -> bool {
        if !self.pr_mutation_pending_matches(mutation_id, scope_repo_id) {
            return false;
        }
        let Some(detail) = &mut self.prs_state.pr_detail else {
            return false;
        };
        if detail.number != pr_number {
            return false;
        }
        detail.comments.push(comment);
        let new_idx = detail.comments.len().saturating_sub(1);
        self.prs_state.detail_subfocus = PrDetailSubfocus::Comment(new_idx);
        self.prs_state.mutation_pending = None;
        self.prs_state.inline_state = InlineState::None;
        self.prs_state.error = None;
        self.scroll_pr_detail_to_bottom();
        true
    }

    /// Apply a comment-create failure: preserve draft, set scoped error.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-013
    /// @pseudocode component-001 lines 323-327
    fn apply_pr_comment_create_failed(
        &mut self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        error: String,
    ) -> bool {
        if !self.pr_mutation_pending_matches(mutation_id, scope_repo_id) {
            return false;
        }
        if let Some(detail) = &self.prs_state.pr_detail
            && detail.number != pr_number
        {
            return false;
        }
        self.prs_state.mutation_pending = None;
        self.prs_state.error = Some(error);
        true
    }

    /// Apply a generic mutation failure: scoped error, never silent.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-013
    /// @pseudocode component-001 lines 323-327
    fn apply_pr_mutation_failed(
        &mut self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        error: String,
    ) -> bool {
        self.apply_pr_comment_create_failed(scope_repo_id, pr_number, mutation_id, error)
    }

    /// Check if a pending mutation matches mutation_id + scope.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @pseudocode component-001 lines 316-327
    fn pr_mutation_pending_matches(&self, mutation_id: u64, scope_repo_id: &RepositoryId) -> bool {
        self.prs_state
            .mutation_pending
            .as_ref()
            .is_some_and(|pending| {
                pending.mutation_id == mutation_id && pending.scope_repo_id == *scope_repo_id
            })
    }

    /// Handle PR error events (SendToAgentFailed).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-013
    /// @pseudocode component-001 lines 242-247
    pub(crate) fn apply_pr_error_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::PrSendToAgentFailed { error } => {
                self.prs_state.error = Some(error);
                true
            }
            _ => false,
        }
    }
}
