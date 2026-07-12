//! Per-repository user-preference snapshot/restore operations (issue #163).
//!
//! Owns the read/write bridge between the live issues/PRs filter+search state
//! and the persisted [`UserPreferences`] map keyed by [`RepositoryId`]. All
//! methods are pure state transitions — disk persistence happens in the
//! app-shell layer after the reducer runs.
//!
//! `sync_preferences_for_repo_change` is the single chokepoint that prevents
//! filters from leaking across repos when the selected repository changes via
//! any path (dashboard select, jump-to-agent, etc.).

use crate::domain::{MergeMethod, RepositoryId};
use crate::state::AppState;

impl AppState {
    /// Clone the currently-selected repository id (issue #163). Thin wrapper
    /// over `selected_repository_id` for the `remember_*` helpers, which need
    /// an owned id to resolve the borrow conflict between reading
    /// `self.repositories` and mutating `self.user_preferences`.
    pub(super) fn current_repo_id(&self) -> Option<RepositoryId> {
        self.selected_repository_id().cloned()
    }

    /// Persist the OLD repo's live filter/search and restore the NEW repo's
    /// preferences when the selected repository changes while a list mode
    /// (issues or PRs) is active (issue #163 per-repo isolation).
    ///
    /// `prev_repo_id` is the repo that was selected BEFORE the index changed;
    /// it receives a final snapshot of the live filter/search/field-index so
    /// uncommitted selections are not silently discarded. The new repo's
    /// stored preferences are then restored into the live state. This is the
    /// single chokepoint that prevents filters from leaking across repos via
    /// any repo-selection path (dashboard select, jump-to-agent, etc.).
    pub(super) fn sync_preferences_for_repo_change(&mut self, prev_repo_id: Option<RepositoryId>) {
        let new_repo_id = self.current_repo_id();
        if new_repo_id == prev_repo_id {
            return;
        }
        // Snapshot the OLD repo's live selections into its stored preferences
        // before the new repo's preferences overwrite the live state.
        if let Some(old_id) = prev_repo_id {
            if self.issues_state.active {
                self.remember_issue_preferences_for(&old_id);
            }
            if self.prs_state.active {
                self.remember_pr_preferences_for(&old_id);
            }
        }
        // Restore the NEW repo's stored preferences into the live state.
        if self.issues_state.active {
            self.reset_issues_for_repo_change();
        }
        if self.prs_state.active {
            self.reset_prs_for_repo_change();
        }
    }

    /// Snapshot the current issue filter/search/field-index into per-repo
    /// preferences (issue #163). No-op when no repo is selected.
    pub(super) fn remember_issue_preferences(&mut self) {
        let Some(repo_id) = self.current_repo_id() else {
            return;
        };
        self.remember_issue_preferences_for(&repo_id);
    }

    /// Snapshot the current issue filter/search/field-index into the
    /// preferences for an EXPLICIT repo id (issue #163). Used by
    /// `sync_preferences_for_repo_change` to persist the OLD repo's live
    /// selections before the new repo's preferences overwrite them.
    fn remember_issue_preferences_for(&mut self, repo_id: &RepositoryId) {
        let filter = self.issues_state.committed_filter.clone();
        let search_query = self.issues_state.search_query.clone();
        let field_index = self.issues_state.filter_ui.field_index;
        self.user_preferences
            .update_field_for_repo(repo_id, |prefs| {
                prefs.issue_filter = filter;
                prefs.issue_search_query = search_query;
                prefs.issue_filter_field_index = field_index;
            });
    }

    /// Snapshot the current PR filter/search/field-index into per-repo
    /// preferences (issue #163). No-op when no repo is selected.
    pub(super) fn remember_pr_preferences(&mut self) {
        let Some(repo_id) = self.current_repo_id() else {
            return;
        };
        self.remember_pr_preferences_for(&repo_id);
    }

    /// Snapshot the current PR filter/search/field-index into the preferences
    /// for an EXPLICIT repo id (issue #163). See `remember_issue_preferences_for`.
    fn remember_pr_preferences_for(&mut self, repo_id: &RepositoryId) {
        let filter = self.prs_state.committed_filter.clone();
        let search_query = self.prs_state.search_query.clone();
        let field_index = self.prs_state.filter_ui.field_index;
        self.user_preferences
            .update_field_for_repo(repo_id, |prefs| {
                prefs.pr_filter = filter;
                prefs.pr_search_query = search_query;
                prefs.pr_filter_field_index = field_index;
            });
    }

    /// Record the confirmed merge method for the current repo (issue #163).
    pub(super) fn remember_merge_method(&mut self, method: MergeMethod) {
        if let Some(repo_id) = self.current_repo_id() {
            self.user_preferences
                .update_field_for_repo(&repo_id, |prefs| {
                    prefs.last_merge_method = Some(method);
                });
        }
    }

    /// Snapshot the current PR filter field-index into per-repo preferences
    /// (issue #163). Uses in-place mutation since this fires on every cursor
    /// keystroke. No-op when no repo is selected.
    pub(super) fn remember_pr_filter_field_index(&mut self) {
        if let Some(repo_id) = self.current_repo_id() {
            let idx = self.prs_state.filter_ui.field_index;
            self.user_preferences
                .update_field_for_repo(&repo_id, |prefs| {
                    prefs.pr_filter_field_index = idx;
                });
        }
    }

    /// Snapshot the current issue filter field-index into per-repo preferences
    /// (issue #163). Uses in-place mutation since this fires on every cursor
    /// keystroke. No-op when no repo is selected.
    pub(super) fn remember_issue_filter_field_index(&mut self) {
        if let Some(repo_id) = self.current_repo_id() {
            let idx = self.issues_state.filter_ui.field_index;
            self.user_preferences
                .update_field_for_repo(&repo_id, |prefs| {
                    prefs.issue_filter_field_index = idx;
                });
        }
    }
}
