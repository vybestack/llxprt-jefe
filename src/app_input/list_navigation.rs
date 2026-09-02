//! Pure list-page geometry shared by workspace key resolvers.
//!
//! The terminal size is read only by input-boundary handlers. These helpers
//! combine that row count with the same pane-layout and list-geometry functions
//! used by rendering, producing the typed capacity carried through reducers.

use jefe::layout::{
    OUTER_BARS_HEIGHT, actions_pane_rows, effective_render_size, issues_pane_rows, prs_pane_rows,
    split_layout_for_render_size,
};
use jefe::list_viewport::{ListGeometry, PageItemCount, PaneRows, RowsPerItem};
use jefe::state::{AppState, PaneFocus, ScreenId};

/// Derive the visible compact-list capacity for Issues mode.
#[must_use]
pub(super) fn issues_page_item_count(
    state: &AppState,
    terminal_cols: u16,
    terminal_rows: u16,
) -> PageItemCount {
    let (_, render_rows) = effective_render_size(terminal_cols, terminal_rows);
    let (pane_rows, _) = issues_pane_rows(
        usize::from(render_rows),
        state.issues_state.error.is_some(),
        state.issues_state.filter_ui.controls_open,
    );
    compact_list_page_item_count(pane_rows)
}

/// Derive the visible compact-list capacity for Pull Requests mode.
#[must_use]
pub(super) fn prs_page_item_count(
    state: &AppState,
    terminal_cols: u16,
    terminal_rows: u16,
) -> PageItemCount {
    let (_, render_rows) = effective_render_size(terminal_cols, terminal_rows);
    let (pane_rows, _) = prs_pane_rows(
        usize::from(render_rows),
        state.prs_state.error.is_some(),
        state.prs_state.filter_ui.controls_open,
    );
    compact_list_page_item_count(pane_rows)
}

/// Derive the visible compact-list capacity for Actions mode.
#[must_use]
pub(super) fn actions_page_item_count(
    state: &AppState,
    terminal_cols: u16,
    terminal_rows: u16,
) -> PageItemCount {
    let (_, render_rows) = effective_render_size(terminal_cols, terminal_rows);
    let (pane_rows, _) = actions_pane_rows(
        usize::from(render_rows),
        state.actions_state.error.is_some(),
        state.actions_state.ui.filter_ui_open,
    );
    compact_list_page_item_count(pane_rows)
}

/// Derive the visible page capacity for the focused Dashboard or Split list.
#[must_use]
pub fn dashboard_page_item_count(
    state: &AppState,
    screen: Option<ScreenId>,
    terminal_cols: u16,
    terminal_rows: u16,
) -> PageItemCount {
    if let Some(count) = host_list_page_item_count(state) {
        return count;
    }
    // The split view is an open descriptor reached by identity, so its sidebar
    // capacity is keyed here rather than through a compiled variant (issue
    // #706).
    if state.screen() == jefe::workbench::REPOSITORIES_IDENTITY {
        return split_page_item_count(terminal_cols, terminal_rows);
    }
    match screen {
        Some(ScreenId::Issues) => issues_page_item_count(state, terminal_cols, terminal_rows),
        Some(ScreenId::PullRequests) => prs_page_item_count(state, terminal_cols, terminal_rows),
        Some(ScreenId::Actions) => actions_page_item_count(state, terminal_cols, terminal_rows),
        Some(_) => dashboard_pane_page_item_count(state, terminal_cols, terminal_rows),
        None => PageItemCount::default(),
    }
}

fn host_list_page_item_count(state: &AppState) -> Option<PageItemCount> {
    if !state.focused_host_reorder_panel() {
        return None;
    }
    let current = state.nav.current();
    let Some(layout) = state
        .resolved_layout
        .as_ref()
        .filter(|layout| layout.screen_instance == current.id)
    else {
        return Some(PageItemCount::default());
    };
    Some(
        layout
            .panel(&current.panel_focus)
            .map_or_else(PageItemCount::default, |resolved| {
                PageItemCount::new(usize::from(resolved.content.height))
            }),
    )
}

/// The split sidebar's page capacity.
///
/// The fixed left rail shares its height with the STATUS block, so its row
/// count comes from the split layout rather than the focused pane.
fn split_page_item_count(terminal_cols: u16, terminal_rows: u16) -> PageItemCount {
    let (render_cols, render_rows) = effective_render_size(terminal_cols, terminal_rows);
    let pane_rows = split_layout_for_render_size(render_cols, render_rows).sidebar_rows;
    ListGeometry::bordered_padded(RowsPerItem::new(1))
        .page_item_count(PaneRows::new(usize::from(pane_rows)))
}

/// The dashboard's focused list capacity.
fn dashboard_pane_page_item_count(
    state: &AppState,
    terminal_cols: u16,
    terminal_rows: u16,
) -> PageItemCount {
    let (_, render_rows) = effective_render_size(terminal_cols, terminal_rows);
    let pane_rows = match state.pane_focus {
        PaneFocus::Repositories => render_rows.saturating_sub(OUTER_BARS_HEIGHT),
        PaneFocus::Agents | PaneFocus::Terminal => 0,
    };
    ListGeometry::bordered_padded(RowsPerItem::new(1))
        .page_item_count(PaneRows::new(usize::from(pane_rows)))
}
fn compact_list_page_item_count(pane_rows: usize) -> PageItemCount {
    ListGeometry::bordered(RowsPerItem::new(1)).page_item_count(PaneRows::new(pane_rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_page_capacity_uses_layout_bands_and_shared_geometry() {
        let state = crate::test_app_state();
        assert_eq!(
            issues_page_item_count(&state, 120, 22),
            PageItemCount::new(3)
        );
        assert_eq!(prs_page_item_count(&state, 120, 36), PageItemCount::new(7));
        assert_eq!(
            actions_page_item_count(&state, 120, 36),
            PageItemCount::new(7)
        );
    }

    #[test]
    fn open_dashboard_page_capacity_uses_exact_current_resolved_panel() {
        let mut state = crate::test_app_state();
        let screen_instance = state.nav.current().id;
        let resolved = jefe::screen_layout::resolve_screen(&state, 100, 25);
        assert!(state.publish_resolved_layout(screen_instance, resolved));
        let layout = state
            .resolved_layout
            .as_ref()
            .unwrap_or_else(|| panic!("dashboard layout must resolve"));
        let panel = layout
            .panel(&state.nav.current().panel_focus)
            .unwrap_or_else(|| panic!("focused dashboard panel must resolve"));
        let expected = PageItemCount::new(usize::from(panel.content.height));

        assert_ne!(expected, PageItemCount::default());
        assert_eq!(
            dashboard_page_item_count(&state, state.compiled_screen(), 100, 25),
            expected
        );
    }

    #[test]
    fn split_page_capacity_uses_the_actual_sidebar_pane() {
        let mut state = crate::test_app_state();
        state.nav = crate::state::navigation::NavState::rooted_definition(
            jefe::workbench::REPOSITORIES_IDENTITY,
            jefe::workbench::RouteId::from_static("repositories"),
            jefe::workbench::PanelId::from_static("repositories"),
        );
        let screen_instance = state.nav.current().id;
        let resolved = jefe::screen_layout::resolve_screen(&state, 100, 25);
        let expected = resolved
            .as_ref()
            .and_then(|layout| layout.panel(&state.nav.current().panel_focus))
            .map_or_else(PageItemCount::default, |panel| {
                PageItemCount::new(usize::from(panel.content.height))
            });
        assert!(state.publish_resolved_layout(screen_instance, resolved));

        // The left rail shares its height with the STATUS block (header plus
        // four buckets), the way the legacy split screen drew it, so the
        // sidebar pane is five rows shorter than a full-height list.
        assert_eq!(expected, PageItemCount::new(11));
        assert_eq!(
            dashboard_page_item_count(&state, state.compiled_screen(), 100, 25),
            expected
        );
    }

    #[test]
    fn split_page_capacity_saturates_with_tiny_terminal() {
        let mut state = crate::test_app_state();
        state.nav = crate::state::navigation::NavState::rooted_definition(
            jefe::workbench::REPOSITORIES_IDENTITY,
            jefe::workbench::RouteId::from_static("repositories"),
            jefe::workbench::PanelId::from_static("repositories"),
        );
        let screen_instance = state.nav.current().id;
        let resolved = jefe::screen_layout::resolve_screen(&state, 2, 6);
        assert!(state.publish_resolved_layout(screen_instance, resolved));
        let layout = jefe::layout::split_layout_for_render_size(2, 6);
        let expected = ListGeometry::bordered_padded(RowsPerItem::new(1))
            .page_item_count(PaneRows::new(usize::from(layout.sidebar_rows)));

        assert_eq!(layout.sidebar_rows, 0);
        assert_eq!(expected, PageItemCount::new(0));
        assert_eq!(
            dashboard_page_item_count(&state, state.compiled_screen(), 2, 6),
            expected
        );
    }

    #[test]
    fn all_page_capacities_use_effective_render_size() {
        let state = crate::test_app_state();
        let raw = (102, 27);
        let effective = effective_render_size(raw.0, raw.1);

        assert_eq!(
            issues_page_item_count(&state, raw.0, raw.1),
            compact_list_page_item_count(
                issues_pane_rows(usize::from(effective.1), false, false).0
            )
        );
        assert_eq!(
            prs_page_item_count(&state, raw.0, raw.1),
            compact_list_page_item_count(prs_pane_rows(usize::from(effective.1), false, false).0)
        );
        assert_eq!(
            actions_page_item_count(&state, raw.0, raw.1),
            compact_list_page_item_count(
                actions_pane_rows(usize::from(effective.1), false, false).0
            )
        );
        let mut state = state;
        state.nav = crate::state::navigation::NavState::rooted_definition(
            jefe::workbench::REPOSITORIES_IDENTITY,
            jefe::workbench::RouteId::from_static("repositories"),
            jefe::workbench::PanelId::from_static("repositories"),
        );
        let screen_instance = state.nav.current().id;
        let resolved = jefe::screen_layout::resolve_screen(&state, raw.0, raw.1);
        let expected = resolved
            .as_ref()
            .and_then(|layout| layout.panel(&state.nav.current().panel_focus))
            .map_or_else(PageItemCount::default, |panel| {
                PageItemCount::new(usize::from(panel.content.height))
            });
        assert!(state.publish_resolved_layout(screen_instance, resolved));
        assert_eq!(
            dashboard_page_item_count(&state, state.compiled_screen(), raw.0, raw.1),
            expected
        );
    }
}
