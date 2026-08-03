//! Pull-request delete state operations (issue #183).
//!
//! Mirrors `issues_close_delete_ops`. Owns the destructive-confirm overlay
//! (open, arm, confirm, cancel) and the delete result lifecycle. Every
//! transition is deterministic and free of side effects: the decision to close
//! the pull request before removing its branch, and the refusal to remove a
//! base branch, are both made here so the boundary layer only executes.

use super::{
    AppState, InlineState, PrDeleteConfirmState, PrDeleteMutationPending, PrFocus,
    PrLifecycleEvent, ReadOnlyHintKind,
};
use crate::domain::{PrState, RepositoryId};

impl AppState {
    /// Apply a pull-request delete event (returns handled).
    pub(super) fn apply_pr_delete_event(&mut self, event: &PrLifecycleEvent) -> bool {
        match event {
            PrLifecycleEvent::OpenDeleteConfirm => {
                self.open_pr_delete_confirm();
                true
            }
            PrLifecycleEvent::DeleteConfirm => {
                self.confirm_pr_delete();
                true
            }
            PrLifecycleEvent::DeleteCancel => {
                self.prs_state.delete_confirm = None;
                true
            }
            PrLifecycleEvent::Deleted {
                scope_repo_id,
                pr_number,
                mutation_id,
                branch,
                closed,
            } => {
                self.apply_pr_deleted(scope_repo_id, *pr_number, *mutation_id, branch, *closed);
                true
            }
            PrLifecycleEvent::DeleteFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            } => {
                self.apply_pr_delete_failed(scope_repo_id, *pr_number, *mutation_id, error);
                true
            }
            _ => false,
        }
    }

    /// Open the destructive overlay for the focused pull request.
    fn open_pr_delete_confirm(&mut self) {
        if self.pr_delete_blocked() {
            return;
        }
        if !matches!(self.prs_state.pr_focus, PrFocus::PrList | PrFocus::PrDetail) {
            self.apply_pr_show_notice(ReadOnlyHintKind::NoPrToDelete);
            return;
        }
        let Some(detail) = self.prs_state.pr_detail.as_ref() else {
            self.apply_pr_show_notice(ReadOnlyHintKind::NoPrToDelete);
            return;
        };
        self.prs_state.delete_confirm = Some(PrDeleteConfirmState {
            pr_number: detail.number,
            head_ref: detail.head_ref.clone(),
            base_ref: detail.base_ref.clone(),
            is_open: detail.state == PrState::Open,
            awaiting_confirmation: false,
        });
    }

    /// Whether an overlay or an in-flight mutation owns the screen already.
    fn pr_delete_blocked(&self) -> bool {
        self.prs_state.inline_state != InlineState::None
            || self.prs_state.agent_chooser.is_some()
            || self.prs_state.merge_chooser.is_some()
            || self.prs_state.property_editor.is_some()
            || self.prs_state.delete_confirm.is_some()
            || self.prs_state.mutation_pending.is_some()
            || self.prs_state.merge_mutation_pending.is_some()
            || self.prs_state.property_mutation_pending.is_some()
            || self.prs_state.delete_mutation_pending.is_some()
    }

    /// Arm the overlay, or record the pending delete once armed.
    fn confirm_pr_delete(&mut self) {
        let Some(confirm) = self.prs_state.delete_confirm.as_mut() else {
            return;
        };
        if !confirm.awaiting_confirmation {
            confirm.awaiting_confirmation = true;
            return;
        }
        let confirm = confirm.clone();
        self.prs_state.delete_confirm = None;
        if let Err(refusal) = pr_delete_refusal(&confirm) {
            self.prs_state.error = Some(refusal);
            return;
        }
        let Some(scope_repo_id) = self.selected_pr_scope_repo_id() else {
            self.apply_pr_show_notice(ReadOnlyHintKind::NoPrToDelete);
            return;
        };
        self.prs_state.next_mutation_id = self.prs_state.next_mutation_id.saturating_add(1);
        self.prs_state.delete_mutation_pending = Some(PrDeleteMutationPending {
            scope_repo_id,
            mutation_id: self.prs_state.next_mutation_id,
            pr_number: confirm.pr_number,
            head_ref: confirm.head_ref,
            close_first: confirm.is_open,
        });
    }

    /// The selected repository's id, or `None` when nothing is selected.
    ///
    /// A pending recorded with an empty scope could never match the result the
    /// boundary layer delivers, so the delete is refused instead.
    fn selected_pr_scope_repo_id(&self) -> Option<RepositoryId> {
        self.selected_repository_index
            .and_then(|index| self.repositories.get(index))
            .map(|repo| repo.id.clone())
    }

    /// Apply a completed delete: retire the pending and reflect the close.
    fn apply_pr_deleted(
        &mut self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        branch: &str,
        closed: bool,
    ) {
        if !self.pr_delete_pending_matches(scope_repo_id, pr_number, mutation_id) {
            return;
        }
        self.prs_state.delete_mutation_pending = None;
        self.prs_state.error = None;
        if closed {
            self.mark_pull_request_closed(pr_number);
        }
        self.prs_state.post_mutation_refresh.request();
        self.prs_state.draft_notice = Some(if closed {
            format!("Closed PR #{pr_number} and deleted branch {branch}")
        } else {
            format!("Deleted branch {branch} for PR #{pr_number}")
        });
    }

    /// Apply a failed delete: retire the pending and surface the reason.
    fn apply_pr_delete_failed(
        &mut self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        error: &str,
    ) {
        if !self.pr_delete_pending_matches(scope_repo_id, pr_number, mutation_id) {
            return;
        }
        self.prs_state.delete_mutation_pending = None;
        self.prs_state.error = Some(format!("Failed to delete PR #{pr_number}: {error}"));
    }

    /// Whether a result belongs to the delete currently in flight.
    fn pr_delete_pending_matches(
        &self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        mutation_id: u64,
    ) -> bool {
        self.prs_state
            .delete_mutation_pending
            .as_ref()
            .is_some_and(|pending| {
                pending.mutation_id == mutation_id
                    && pending.pr_number == pr_number
                    && pending.scope_repo_id == *scope_repo_id
            })
    }

    /// Reflect a close optimistically in both the list row and the detail.
    fn mark_pull_request_closed(&mut self, pr_number: u64) {
        let mut rows = self.prs_state.list.items().to_vec();
        for row in &mut rows {
            if row.number == pr_number {
                row.state = PrState::Closed;
            }
        }
        self.prs_state.list.replace_items(rows);
        if let Some(detail) = self.prs_state.pr_detail.as_mut()
            && detail.number == pr_number
        {
            detail.state = PrState::Closed;
        }
    }
}

/// Why this pull request's branch must not be removed, if it must not be.
///
/// Both refusals are decided before any request is sent: an unknown head branch
/// would delete nothing identifiable, and removing the branch the pull request
/// targets would take the base out from under it.
fn pr_delete_refusal(confirm: &PrDeleteConfirmState) -> Result<(), String> {
    let head = confirm.head_ref.trim();
    if head.is_empty() {
        return Err(format!(
            "Cannot delete PR #{}: its head branch is unknown. Reload the pull request list and \
             try again.",
            confirm.pr_number
        ));
    }
    if head == confirm.base_ref.trim() {
        return Err(format!(
            "Refusing to delete {head}: it is the base branch of PR #{}.",
            confirm.pr_number
        ));
    }
    Ok(())
}
