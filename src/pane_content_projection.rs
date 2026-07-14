//! Application-boundary inputs for mouse-selection content projection.

use crate::dashboard_git_info::resolve_dashboard_git_info;
use crate::runtime::TerminalSnapshot;
use crate::selection::{
    PaneContent, PaneContentContext, SelectablePane, pane_content_lines_with_context,
};
use crate::state::AppState;

/// Resolve boundary-owned display data, then run the pure pane projection.
#[must_use]
pub fn projected_pane_content(
    pane: SelectablePane,
    state: &AppState,
    snapshot: Option<&TerminalSnapshot>,
    history_lines: &[String],
    cols: u16,
    rows: u16,
) -> PaneContent {
    let resolved_git_info;
    let git_info = if matches!(pane, SelectablePane::AgentList | SelectablePane::Preview) {
        if let Some(bound) = state.selection_dashboard_git_info.as_ref() {
            Some(bound)
        } else {
            resolved_git_info = resolve_dashboard_git_info(state);
            resolved_git_info.as_ref()
        }
    } else {
        None
    };
    pane_content_lines_with_context(
        pane,
        state,
        &PaneContentContext {
            terminal_snapshot: snapshot,
            history_lines,
            term_cols: cols,
            term_rows: rows,
            dashboard_git_info: git_info,
        },
    )
}
