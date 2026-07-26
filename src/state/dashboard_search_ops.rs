//! Dashboard "search lite" state transitions for repositories and agents
//! (issue #405).
//!
//! Implements the focused-input search mode: typing a query live-filters the
//! repository sidebar (by repo `name`) and the agent pane (by agent `name`),
//! case-insensitively, AND-composed with `hide_idle_repositories`. The query
//! is runtime-only — never persisted. Selection indices are re-normalized
//! after a query change so a filtered-out selection clamps to a visible item.

use super::{AppState, UiNavigationMessage};

impl AppState {
    /// Focus the dashboard search input.
    pub(super) fn focus_dashboard_search(&mut self) {
        self.dashboard_search.input_focused = true;
    }

    /// Blur the dashboard search input, retaining the query so the filter
    /// persists until explicitly cleared (mirrors Issues/PRs Enter-to-apply).
    pub(super) fn blur_dashboard_search(&mut self) {
        self.dashboard_search.input_focused = false;
    }

    /// Replace the dashboard search query and re-normalize selection so a
    /// filtered-out selection clamps to a visible item.
    pub(super) fn set_dashboard_search_query(&mut self, query: String) {
        self.dashboard_search.lowered_query = query.trim().to_lowercase();
        self.dashboard_search.query = query;
        self.normalize_selection_indices();
    }

    /// Clear the dashboard search query and blur the input.
    pub(super) fn clear_dashboard_search(&mut self) {
        self.dashboard_search.query.clear();
        self.dashboard_search.lowered_query.clear();
        self.dashboard_search.input_focused = false;
        self.normalize_selection_indices();
    }

    /// Toggle the active-only (`v`) filter and re-normalize selection.
    /// Extracted from `apply_ui_navigation` so the reducer stays within the
    /// function line budget.
    pub(super) fn toggle_hide_idle_repositories(&mut self) {
        self.hide_idle_repositories = !self.hide_idle_repositories;
        self.dashboard_grab = None;
        self.normalize_selection_indices();
    }

    /// Whether the dashboard search query is active (non-empty after trim).
    #[must_use]
    pub fn dashboard_search_active(&self) -> bool {
        !self.dashboard_search.query.trim().is_empty()
    }

    /// Whether the trimmed query matches a name (case-insensitive substring).
    /// An empty/blank query matches everything.
    #[must_use]
    pub fn dashboard_search_matches(&self, name: &str) -> bool {
        let needle = self.dashboard_search.lowered_query.as_str();
        if needle.is_empty() {
            return true;
        }
        name.to_lowercase().contains(needle)
    }
}

/// Dispatch a dashboard-search [`UiNavigationMessage`] to its state mutator.
///
/// Kept separate from `apply_ui_navigation` so the main reducer stays within
/// the function line budget. Only the four dashboard-search variants are
/// routed here; any other message is a programming error (the caller matches
/// the variants before delegating).
pub(super) fn apply_dashboard_search_message(state: &mut AppState, message: UiNavigationMessage) {
    match message {
        UiNavigationMessage::FocusDashboardSearch => state.focus_dashboard_search(),
        UiNavigationMessage::BlurDashboardSearch => state.blur_dashboard_search(),
        UiNavigationMessage::SetDashboardSearchQuery { query } => {
            state.set_dashboard_search_query(query);
        }
        UiNavigationMessage::ClearDashboardSearch => state.clear_dashboard_search(),
        other => debug_assert!(
            false,
            "apply_dashboard_search_message received non-dashboard-search message: {other:?}"
        ),
    }
}
