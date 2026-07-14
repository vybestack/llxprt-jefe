//! Issue close + delete lifecycle state operations (issue #182).
//!
//! Mirrors `prs_merge_ops.rs`. Owns the delete-confirm overlay transitions
//! (open/arm/confirm/cancel) and the close/delete result lifecycle
//! (IssueClosed/IssueDeleted/MutationFailed). All transitions are
//! deterministic and side-effect-free.

use super::{
    AppEvent, AppState, InlineState, IssueDeleteConfirmState, IssueLifecycleMutationPending,
    ReadOnlyHintKind,
};
use crate::domain::{IssueState, RepositoryId};

impl AppState {
    /// Apply an issue close/delete lifecycle event (returns handled).
    pub(super) fn apply_issue_close_delete_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::CloseIssue => {
                self.begin_issue_close();
                true
            }
            AppEvent::OpenDeleteIssueConfirm => {
                self.open_issue_delete_confirm();
                true
            }
            AppEvent::IssueDeleteConfirm => {
                self.confirm_issue_delete();
                true
            }
            AppEvent::IssueDeleteCancel => {
                self.issues_state.delete_confirm = None;
                true
            }
            AppEvent::IssueClosed {
                scope_repo_id,
                issue_number,
                mutation_id,
                close_reason: _,
                duplicate_of: _,
            } => self.apply_issue_closed(scope_repo_id, *issue_number, *mutation_id),
            AppEvent::IssueDeleted {
                scope_repo_id,
                issue_number,
                mutation_id,
            } => self.apply_issue_deleted(scope_repo_id, *issue_number, *mutation_id),
            AppEvent::MutationFailed {
                scope_repo_id,
                issue_number,
                mutation_id,
                error,
            } => self.apply_lifecycle_mutation_failed(
                scope_repo_id,
                *issue_number,
                *mutation_id,
                error,
            ),
            _ => false,
        }
    }

    /// Resolve the focused issue number (list selection or detail).
    pub(super) fn focused_issue_number(&self) -> Option<u64> {
        if let Some(detail) = &self.issues_state.issue_detail {
            return Some(detail.number);
        }
        self.issues_state
            .selected_issue_index()
            .and_then(|idx| self.issues_state.issues().get(idx))
            .map(|issue| issue.number)
    }

    /// Resolve the focused issue's state (list row or detail).
    pub(super) fn focused_issue_state(&self) -> Option<IssueState> {
        if let Some(detail) = &self.issues_state.issue_detail {
            return Some(detail.state);
        }
        self.issues_state
            .selected_issue_index()
            .and_then(|idx| self.issues_state.issues().get(idx))
            .map(|issue| issue.state)
    }

    /// Whether any overlay or in-flight lifecycle mutation would block starting
    /// a new close/delete. Extracted so the guard cannot drift between the two
    /// entry points.
    pub(super) fn lifecycle_overlay_active(&self) -> bool {
        self.issues_state.inline_state != InlineState::None
            || self.issues_state.agent_chooser.is_some()
            || self.issues_state.delete_confirm.is_some()
            || self.issues_state.close_reason_chooser.is_some()
            || self.issues_state.close_mutation_pending.is_some()
            || self.issues_state.delete_mutation_pending.is_some()
    }

    /// Begin a close mutation on the focused issue.
    fn begin_issue_close(&mut self) {
        if self.lifecycle_overlay_active() {
            return;
        }
        let Some(state) = self.focused_issue_state() else {
            self.show_issue_notice(ReadOnlyHintKind::NoIssueFocused);
            return;
        };
        if state == IssueState::Closed {
            self.show_issue_notice(ReadOnlyHintKind::IssueAlreadyClosed);
            return;
        }
        let Some(issue_number) = self.focused_issue_number() else {
            self.show_issue_notice(ReadOnlyHintKind::NoIssueFocused);
            return;
        };
        let Some(scope) = self.selected_issue_scope_repo_id() else {
            // No repository selected: the async result would carry a real scope
            // and never match this pending, leaving it stuck. Bail with a notice.
            self.show_issue_notice(ReadOnlyHintKind::NoIssueFocused);
            return;
        };
        let mutation_id = self.next_issue_mutation_id();
        self.issues_state.close_mutation_pending = Some(IssueLifecycleMutationPending {
            scope_repo_id: scope,
            mutation_id,
            issue_number,
            node_id: None,
            close_reason: None,
            duplicate_of: None,
        });
    }

    /// Open the delete confirm overlay (precondition: issue focused, no overlays).
    fn open_issue_delete_confirm(&mut self) {
        if self.lifecycle_overlay_active() {
            return;
        }
        let Some(issue_number) = self.focused_issue_number() else {
            self.show_issue_notice(ReadOnlyHintKind::NoIssueFocused);
            return;
        };
        self.issues_state.delete_confirm = Some(IssueDeleteConfirmState {
            issue_number,
            awaiting_confirmation: false,
        });
    }

    /// Confirm the delete: first Enter arms confirmation; second Enter dispatches.
    fn confirm_issue_delete(&mut self) {
        let Some(confirm) = &mut self.issues_state.delete_confirm else {
            return;
        };
        if !confirm.awaiting_confirmation {
            confirm.awaiting_confirmation = true;
            return;
        }
        let issue_number = confirm.issue_number;
        let Some(scope) = self.selected_issue_scope_repo_id() else {
            // No repository selected: bail so the pending cannot be created
            // with an empty scope that would never match the async result.
            self.issues_state.delete_confirm = None;
            self.show_issue_notice(ReadOnlyHintKind::NoIssueFocused);
            return;
        };
        let Some(node_id) = self.focused_issue_node_id(issue_number) else {
            self.issues_state.delete_confirm = None;
            self.issues_state.error = Some("Cannot delete: issue node id unavailable".to_string());
            return;
        };
        let mutation_id = self.next_issue_mutation_id();
        self.issues_state.delete_confirm = None;
        self.issues_state.delete_mutation_pending = Some(IssueLifecycleMutationPending {
            scope_repo_id: scope,
            mutation_id,
            issue_number,
            node_id: Some(node_id),
            close_reason: None,
            duplicate_of: None,
        });
    }

    /// Resolve the node id for a given issue number (from list or detail).
    ///
    /// Returns `None` when the issue is not found OR its node id is empty, so
    /// the caller can produce a single clear "node id unavailable" diagnostic
    /// instead of distinguishing not-found from incomplete-data.
    pub(super) fn focused_issue_node_id(&self, issue_number: u64) -> Option<String> {
        let raw = if let Some(detail) = &self.issues_state.issue_detail
            && detail.number == issue_number
        {
            Some(detail.node_id.clone())
        } else {
            self.issues_state
                .issues()
                .iter()
                .find(|issue| issue.number == issue_number)
                .map(|issue| issue.node_id.clone())
        };
        raw.filter(|id| !id.is_empty())
    }

    /// Apply a successful close: update list + detail state, clear pending.
    /// Returns `true` (handled) even on mutation-id mismatch — a stale result is
    /// gracefully ignored (the pending is kept) rather than treated as unhandled.
    fn apply_issue_closed(
        &mut self,
        scope_repo_id: &RepositoryId,
        issue_number: u64,
        mutation_id: u64,
    ) -> bool {
        let pending_matches = self
            .issues_state
            .close_mutation_pending
            .as_ref()
            .is_some_and(|p| {
                p.mutation_id == mutation_id
                    && p.scope_repo_id == *scope_repo_id
                    && p.issue_number == issue_number
            });
        if !pending_matches {
            return true;
        }
        self.issues_state.close_mutation_pending = None;
        if let Some(issue) = self
            .issues_state
            .list
            .items_mut()
            .iter_mut()
            .find(|issue| issue.number == issue_number)
        {
            issue.state = IssueState::Closed;
        }
        if let Some(detail) = &mut self.issues_state.issue_detail
            && detail.number == issue_number
        {
            detail.state = IssueState::Closed;
        }
        self.issues_state.draft_notice = Some(format!("Closed issue #{issue_number}"));
        true
    }

    /// Apply a successful delete: remove from list, clear detail, clear pending.
    /// Returns `true` (handled) even on mutation-id mismatch — a stale result is
    /// gracefully ignored (the pending is kept) rather than treated as unhandled.
    fn apply_issue_deleted(
        &mut self,
        scope_repo_id: &RepositoryId,
        issue_number: u64,
        mutation_id: u64,
    ) -> bool {
        let pending_matches = self
            .issues_state
            .delete_mutation_pending
            .as_ref()
            .is_some_and(|p| {
                p.mutation_id == mutation_id
                    && p.scope_repo_id == *scope_repo_id
                    && p.issue_number == issue_number
            });
        if !pending_matches {
            return true;
        }
        self.issues_state.delete_mutation_pending = None;
        // Capture the deleted issue's index BEFORE removal so the selection can
        // be adjusted precisely (shifting down when an earlier row is removed,
        // rather than silently landing on whichever issue now occupies the slot).
        let deleted_index = self
            .issues_state
            .issues()
            .iter()
            .position(|issue| issue.number == issue_number);
        self.issues_state
            .list
            .items_mut()
            .retain(|issue| issue.number != issue_number);
        if self
            .issues_state
            .issue_detail
            .as_ref()
            .is_some_and(|detail| detail.number == issue_number)
        {
            self.issues_state.issue_detail = None;
            self.issues_state.issue_focus = super::IssueFocus::IssueList;
        }
        self.fix_issue_selection_after_delete(deleted_index);
        self.issues_state.draft_notice = Some(format!("Deleted issue #{issue_number}"));
        true
    }

    /// Fix the selected issue index after a delete.
    ///
    /// - An earlier row removed (`deleted < sel`): decrement the selection
    ///   index so it still points at the same issue (which shifted up one slot).
    /// - The selected row itself removed (`deleted == sel`): keep the index,
    ///   which now points at the next issue (standard list-delete semantics);
    ///   if it was the final row, the clamp below moves it to the new last row.
    /// - List empty: clear the selection.
    fn fix_issue_selection_after_delete(&mut self, deleted_index: Option<usize>) {
        if self.issues_state.issues().is_empty() {
            self.issues_state.list.set_selected_index(None);
            return;
        }
        let max_idx = self.issues_state.issues().len() - 1;
        let current = self.issues_state.selected_issue_index();
        match (deleted_index, current) {
            (Some(deleted), Some(sel)) if deleted < sel => {
                // An earlier row was removed: shift the selection down to track
                // the same issue.
                self.issues_state.list.set_selected_index(Some(sel - 1));
            }
            _ => {
                if let Some(idx) = current
                    && idx > max_idx
                {
                    self.issues_state.list.set_selected_index(Some(max_idx));
                }
            }
        }
    }

    /// Apply a lifecycle mutation failure: clear the matching pending + set error.
    ///
    /// Matches on the FULL operation identity (mutation id + scope + issue
    /// number) so an asynchronous failure for a different scope/issue cannot
    /// clear a lifecycle pending or display a wrong scoped error. When the
    /// failure event carries no issue number (`None`), matching falls back to
    /// mutation id + scope only so the shared `MutationFailed` variant cannot
    /// leave a lifecycle pending permanently stuck.
    fn apply_lifecycle_mutation_failed(
        &mut self,
        scope_repo_id: &RepositoryId,
        issue_number: Option<u64>,
        mutation_id: Option<u64>,
        error: &str,
    ) -> bool {
        let Some(mid) = mutation_id else {
            return false;
        };
        let close_matches = self
            .issues_state
            .close_mutation_pending
            .as_ref()
            .is_some_and(|p| pending_matches(p, mid, scope_repo_id, issue_number));
        let delete_matches = self
            .issues_state
            .delete_mutation_pending
            .as_ref()
            .is_some_and(|p| pending_matches(p, mid, scope_repo_id, issue_number));
        if !close_matches && !delete_matches {
            return false;
        }
        if close_matches {
            self.issues_state.close_mutation_pending = None;
        }
        if delete_matches {
            self.issues_state.delete_mutation_pending = None;
        }
        let issue_ref =
            issue_number.map_or_else(|| "an issue".to_string(), |n| format!("issue #{n}"));
        self.issues_state.error = Some(format!(
            "Failed to mutate {issue_ref} for {}: {error}",
            scope_repo_id.0
        ));
        true
    }

    pub(super) fn next_issue_mutation_id(&mut self) -> u64 {
        self.issues_state.next_mutation_id = self.issues_state.next_mutation_id.saturating_add(1);
        self.issues_state.next_mutation_id
    }

    /// Resolve the scope repository ID for the currently-selected repository.
    ///
    /// Returns `None` when no repository is selected (or the index is stale),
    /// so callers can bail before creating a mutation pending with an empty
    /// scope that would never match an async result carrying the real scope.
    pub(super) fn selected_issue_scope_repo_id(&self) -> Option<RepositoryId> {
        self.selected_repository_index
            .and_then(|idx| self.repositories.get(idx))
            .map(|r| r.id.clone())
    }

    pub(super) fn show_issue_notice(&mut self, kind: ReadOnlyHintKind) {
        let text = match kind {
            ReadOnlyHintKind::IssueAlreadyClosed => "Issue is already closed".to_string(),
            ReadOnlyHintKind::NoIssueFocused => "No issue selected".to_string(),
            ReadOnlyHintKind::NoDuplicateTarget => {
                "Select an issue to mark as duplicate".to_string()
            }
            // The remaining variants are PR-domain hints that this issues path
            // never emits; they are enumerated so adding a new variant forces an
            // explicit decision here rather than being silently swallowed.
            ReadOnlyHintKind::ReadOnlyReplyOnComment
            | ReadOnlyHintKind::ReadOnlyNoComment
            | ReadOnlyHintKind::ReadOnlyNotEditable
            | ReadOnlyHintKind::NoSelectionToOpen
            | ReadOnlyHintKind::NoPrToMerge
            | ReadOnlyHintKind::PrNotMergeable
            | ReadOnlyHintKind::ReadOnlyResolveOnThread => "Action not available".to_string(),
        };
        self.issues_state.draft_notice = Some(text);
    }
}

/// Match a lifecycle pending against a failure event's operation identity.
///
/// Matches mutation id + scope, and issue number when present. An absent issue
/// number (`None`) matches any pending (fallback so the shared `MutationFailed`
/// variant can never leave a lifecycle pending stuck). Uses an explicit match
/// to stay MSRV-clean (no `Option::is_none_or`, stable only since 1.82).
fn pending_matches(
    pending: &IssueLifecycleMutationPending,
    mutation_id: u64,
    scope_repo_id: &RepositoryId,
    issue_number: Option<u64>,
) -> bool {
    pending.mutation_id == mutation_id
        && pending.scope_repo_id == *scope_repo_id
        && match issue_number {
            Some(n) => n == pending.issue_number,
            None => true,
        }
}
