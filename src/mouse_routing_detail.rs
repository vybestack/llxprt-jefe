//! Detail-pane viewport refresh for mouse routing.

use jefe::selection::SelectablePane;
use jefe::state::AppState;

pub(super) fn refresh_detail_viewport_rows(
    state: &mut AppState,
    pane: SelectablePane,
    term_cols: u16,
    term_rows: u16,
) {
    let (render_cols, render_rows) = jefe::layout::effective_render_size(term_cols, term_rows);
    let term_rows_usize = usize::from(render_rows);
    match pane {
        SelectablePane::IssueDetail => {
            state.issues_state.detail_viewport_rows = jefe::layout::issues_detail_viewport_rows(
                term_rows_usize,
                jefe::layout::issues_banner_visible(
                    state.issues_state.error.as_deref(),
                    state.issues_state.draft_notice.as_deref(),
                ),
                state.issues_state.filter_ui.controls_open,
            );
        }
        SelectablePane::PrDetail => {
            state.prs_state.detail_viewport_rows = jefe::layout::prs_detail_viewport_rows(
                term_rows_usize,
                state.prs_state.error.is_some(),
                state.prs_state.filter_ui.controls_open,
            );
        }
        SelectablePane::ActionsDetail => {
            let geometry = jefe::layout::actions_detail_geometry(
                render_cols,
                render_rows,
                state.actions_state.error.is_some(),
                state.actions_state.ui.filter_ui_open,
            );
            state.actions_state.detail_viewport_rows = geometry.viewport_rows;
            state.actions_state.detail_content_width = geometry.content_width;
        }
        _ => {}
    }
}
