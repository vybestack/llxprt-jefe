//! Pure list-page geometry shared by workspace key resolvers.
//!
//! The terminal size is read only by input-boundary handlers. These helpers
//! combine that row count with the same pane-layout and list-geometry functions
//! used by rendering, producing the typed capacity carried through reducers.

use jefe::layout::{
    OUTER_BARS_HEIGHT, actions_pane_rows, dashboard_middle_row_heights_inner,
    effective_render_size, issues_pane_rows, prs_pane_rows, split_layout_for_render_size,
};
use jefe::list_viewport::{ListGeometry, PageItemCount, PaneRows, RowsPerItem};
use jefe::state::{AppState, PaneFocus, ScreenMode};

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
pub(super) fn dashboard_page_item_count(
    state: &AppState,
    screen_mode: ScreenMode,
    terminal_cols: u16,
    terminal_rows: u16,
) -> PageItemCount {
    let (render_cols, render_rows) = effective_render_size(terminal_cols, terminal_rows);
    let pane_rows = match (screen_mode, state.pane_focus) {
        (ScreenMode::Dashboard, PaneFocus::Agents) => {
            dashboard_middle_row_heights_inner(render_rows).0
        }
        (ScreenMode::Split, _) => {
            split_layout_for_render_size(render_cols, render_rows).sidebar_rows
        }
        (_, PaneFocus::Repositories) => render_rows.saturating_sub(OUTER_BARS_HEIGHT),
        (_, PaneFocus::Agents | PaneFocus::Terminal) => 0,
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
        let state = AppState::default();
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
    fn split_page_capacity_uses_the_actual_sidebar_pane() {
        let state = AppState {
            screen_mode: ScreenMode::Split,
            ..AppState::default()
        };
        let layout = jefe::layout::split_layout_for_render_size(100, 25);
        let expected = ListGeometry::bordered_padded(RowsPerItem::new(1))
            .page_item_count(PaneRows::new(usize::from(layout.sidebar_rows)));

        assert_eq!(layout.sidebar_rows, 18);
        assert_eq!(expected, PageItemCount::new(13));
        assert_eq!(
            dashboard_page_item_count(&state, ScreenMode::Split, 100, 25),
            expected
        );
    }

    #[test]
    fn split_page_capacity_saturates_with_tiny_terminal() {
        let state = AppState::default();
        let layout = jefe::layout::split_layout_for_render_size(2, 6);
        let expected = ListGeometry::bordered_padded(RowsPerItem::new(1))
            .page_item_count(PaneRows::new(usize::from(layout.sidebar_rows)));

        assert_eq!(layout.sidebar_rows, 0);
        assert_eq!(expected, PageItemCount::new(1));
        assert_eq!(
            dashboard_page_item_count(&state, ScreenMode::Split, 2, 6),
            expected
        );
    }

    #[test]
    fn all_page_capacities_use_effective_render_size() {
        let state = AppState::default();
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
        assert_eq!(
            dashboard_page_item_count(&state, ScreenMode::Split, raw.0, raw.1),
            ListGeometry::bordered_padded(RowsPerItem::new(1)).page_item_count(PaneRows::new(
                usize::from(split_layout_for_render_size(effective.0, effective.1).sidebar_rows),
            ))
        );
    }
}
