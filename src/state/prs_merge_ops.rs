//! Pull Requests mode merge-chooser + merge-lifecycle state operations (issue #92).
//!
//! Mirrors the agent-chooser reducer (`apply_pr_agent_chooser_event`) and the
//! comment-mutation reducer (`apply_pr_mutation_event`). Owns the
//! merge-method chooser overlay transitions (open/navigate/confirm/cancel)
//! and the merge-result lifecycle (Merged/MergeFailed/MergeMethodsLoaded).
//!
//! @plan PLAN-20260624-PR-MODE.P05
//! @requirement REQ-PR-009

use super::{
    AppEvent, AppState, InlineState, PrFocus, PrMergeChooserState, PrMergeMutationPending,
    ReadOnlyHintKind,
};
use crate::domain::{MERGE_METHODS, MergeMethod, PrState, RepositoryId};

impl AppState {
    /// Apply a PR merge-chooser / merge-lifecycle event (returns handled).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    pub(super) fn apply_pr_merge_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::PrOpenMergeChooser => {
                self.open_pr_merge_chooser();
                true
            }
            AppEvent::PrMergeNavigateUp => {
                self.navigate_pr_merge_chooser(false);
                true
            }
            AppEvent::PrMergeNavigateDown => {
                self.navigate_pr_merge_chooser(true);
                true
            }
            AppEvent::PrMergeConfirm => {
                self.confirm_pr_merge();
                true
            }
            AppEvent::PrMergeCancel => {
                self.prs_state.merge_chooser = None;
                true
            }
            AppEvent::PrMerged {
                scope_repo_id,
                pr_number,
                method,
            } => self.apply_pr_merged(scope_repo_id, *pr_number, *method),
            AppEvent::PrMergeFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            } => self.apply_pr_merge_failed(scope_repo_id, *pr_number, *mutation_id, error),
            AppEvent::PrMergeMethodsLoaded {
                scope_repo_id,
                pr_number,
                allowed_methods,
            } => self.apply_pr_merge_methods_loaded(scope_repo_id, *pr_number, allowed_methods),
            _ => false,
        }
    }

    /// Open the merge chooser (precondition: detail, no overlays, Open+mergeable).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    fn open_pr_merge_chooser(&mut self) {
        if self.prs_state.pr_focus != PrFocus::PrDetail
            || self.prs_state.inline_state != InlineState::None
            || self.prs_state.agent_chooser.is_some()
            || self.prs_state.merge_chooser.is_some()
            || self.prs_state.merge_mutation_pending.is_some()
        {
            return;
        }
        let Some(detail) = &self.prs_state.pr_detail else {
            self.apply_pr_show_notice(ReadOnlyHintKind::NoPrToMerge);
            return;
        };
        if detail.state != PrState::Open {
            self.apply_pr_show_notice(ReadOnlyHintKind::PrNotMergeable);
            return;
        }
        if detail.mergeable == Some(false) {
            self.apply_pr_show_notice(ReadOnlyHintKind::PrNotMergeable);
            return;
        }
        self.prs_state.merge_chooser = Some(PrMergeChooserState {
            selected_index: 0,
            allowed_methods: None,
            awaiting_confirmation: false,
        });
    }

    /// Navigate the merge chooser selection among enabled methods (wraps).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    fn navigate_pr_merge_chooser(&mut self, forward: bool) {
        let Some(chooser) = &mut self.prs_state.merge_chooser else {
            return;
        };
        let enabled = enabled_method_indices(chooser.allowed_methods.as_deref());
        if enabled.len() <= 1 {
            return;
        }
        let current = chooser.selected_index;
        let pos = enabled.iter().position(|&i| i == current).unwrap_or(0);
        let len = enabled.len();
        let next_pos = if forward {
            (pos + 1) % len
        } else {
            (pos + len - 1) % len
        };
        chooser.selected_index = enabled[next_pos];
    }

    /// Confirm the merge: first Enter arms confirmation; second Enter dispatches.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    fn confirm_pr_merge(&mut self) {
        let Some(chooser) = &mut self.prs_state.merge_chooser else {
            return;
        };
        if !chooser.awaiting_confirmation {
            chooser.awaiting_confirmation = true;
            return;
        }
        let selected = chooser.selected_index;
        let method = MERGE_METHODS
            .get(selected)
            .copied()
            .unwrap_or(MergeMethod::Merge);
        let scope = self.current_pr_scope_repo_id();
        let pr_number = self.prs_state.pr_detail.as_ref().map_or(0, |d| d.number);
        let mutation_id = self.next_merge_mutation_id();
        self.prs_state.merge_chooser = None;
        self.prs_state.merge_mutation_pending = Some(PrMergeMutationPending {
            scope_repo_id: scope,
            mutation_id,
            pr_number,
            method,
        });
    }

    /// Apply a successful merge: update detail state, clear pending, set notice.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    fn apply_pr_merged(
        &mut self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        _method: MergeMethod,
    ) -> bool {
        self.prs_state.merge_mutation_pending = None;
        if !self.scope_repo_id_matches_pr_merge(scope_repo_id) {
            return true;
        }
        if let Some(detail) = &mut self.prs_state.pr_detail
            && detail.number == pr_number
        {
            detail.state = PrState::Merged;
        }
        if let Some(pr) = self
            .prs_state
            .pull_requests
            .iter_mut()
            .find(|p| p.number == pr_number)
        {
            pr.state = PrState::Merged;
        }
        self.prs_state.draft_notice = Some(format!("Merged PR #{pr_number}"));
        true
    }

    /// Apply a merge failure: clear pending, set scoped error.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    fn apply_pr_merge_failed(
        &mut self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        error: &str,
    ) -> bool {
        let pending_matches = self
            .prs_state
            .merge_mutation_pending
            .as_ref()
            .is_some_and(|p| {
                p.mutation_id == mutation_id
                    && p.scope_repo_id == *scope_repo_id
                    && p.pr_number == pr_number
            });
        if pending_matches {
            self.prs_state.merge_mutation_pending = None;
            self.prs_state.error = Some(format!("Failed to merge PR #{pr_number}: {error}"));
        }
        true
    }

    /// Apply loaded merge methods: update chooser.allowed_methods.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    fn apply_pr_merge_methods_loaded(
        &mut self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        allowed_methods: &[MergeMethod],
    ) -> bool {
        if !self.scope_repo_id_matches_pr_merge(scope_repo_id) {
            return true;
        }
        if self.prs_state.pr_detail.as_ref().map(|d| d.number) != Some(pr_number) {
            return true;
        }
        if let Some(chooser) = &mut self.prs_state.merge_chooser {
            chooser.allowed_methods = Some(allowed_methods.to_vec());
        }
        true
    }

    /// Allocate the next monotonic merge mutation id.
    fn next_merge_mutation_id(&mut self) -> u64 {
        self.prs_state.next_mutation_id += 1;
        self.prs_state.next_mutation_id
    }

    /// Resolve the scope repository ID for the currently-selected repository.
    fn current_pr_scope_repo_id(&self) -> RepositoryId {
        self.selected_repository_index
            .and_then(|idx| self.repositories.get(idx))
            .map_or_else(|| RepositoryId(String::new()), |r| r.id.clone())
    }

    /// Check if a scope_repo_id matches the currently-selected repository.
    fn scope_repo_id_matches_pr_merge(&self, scope_repo_id: &RepositoryId) -> bool {
        self.selected_repository_index
            .and_then(|idx| self.repositories.get(idx))
            .is_some_and(|repo| &repo.id == scope_repo_id)
    }
}

/// Compute the indices (into `MERGE_METHODS`) that are navigable (enabled).
///
/// When `allowed_methods` is `None` (not yet loaded), ALL methods are enabled.
/// When loaded, only methods present in the list are enabled.
fn enabled_method_indices(allowed: Option<&[MergeMethod]>) -> Vec<usize> {
    match allowed {
        None => (0..MERGE_METHODS.len()).collect(),
        Some(methods) => MERGE_METHODS
            .iter()
            .enumerate()
            .filter(|(_, m)| methods.contains(m))
            .map(|(i, _)| i)
            .collect(),
    }
}
