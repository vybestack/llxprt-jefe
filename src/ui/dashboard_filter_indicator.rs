//! Pure projection for the dashboard filtered-view indicator (issue #405).
//!
//! iocraft-free and side-effect-free so it can be unit-tested without a
//! terminal. Returns a short human-readable label whenever the dashboard's
//! visible list is reduced by the "search lite" query and/or the active-only
//! (`v`) filter, or `None` when nothing is filtered. This directly addresses
//! the issue's "make it obvious you're looking at a filtered view."

use crate::state::AppState;

/// Build the dashboard filtered-view indicator string.
///
/// - Search active → names the query (e.g. `filter [search]: "al"`).
/// - Active-only on → `filter [active-only]`.
/// - Both active → names both (e.g. `filter [active-only, search]: "al"`).
/// - Neither → `None` (nothing rendered).
#[must_use]
pub fn dashboard_filter_indicator(state: &AppState) -> Option<String> {
    let search_active = state.dashboard_filter_active();
    let active_only = state.hide_idle_repositories;
    if !search_active && !active_only {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    if active_only {
        parts.push("active-only");
    }
    if search_active {
        parts.push("search");
    }
    let label = parts.join(", ");
    if search_active {
        Some(format!(
            "filter [{label}]: {:?}",
            state.search_query().unwrap_or_default()
        ))
    } else {
        Some(format!("filter [{label}]"))
    }
}
