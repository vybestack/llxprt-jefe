//! Dashboard filtering derived from the current instance's declared Search overlay.

use super::AppState;

impl AppState {
    /// This instance's active Search query while the Search overlay is open.
    #[must_use]
    pub fn search_query(&self) -> Option<&str> {
        self.nav.current().overlays().search_query()
    }

    /// Toggle the active-only (`v`) filter and re-normalize selection.
    pub(super) fn toggle_hide_idle_repositories(&mut self) {
        self.hide_idle_repositories = !self.hide_idle_repositories;
        self.dashboard_grab = None;
        self.normalize_selection_indices();
    }

    /// Whether the declared Search overlay contains a non-blank query.
    #[must_use]
    pub fn dashboard_filter_active(&self) -> bool {
        self.search_query()
            .is_some_and(|query| !query.trim().is_empty())
    }

    /// Whether the current Search overlay query matches a name.
    #[must_use]
    pub fn dashboard_filter_matches(&self, name: &str) -> bool {
        self.search_query().is_none_or(|query| {
            let needle = query.trim().to_lowercase();
            needle.is_empty() || name.to_lowercase().contains(&needle)
        })
    }
}
