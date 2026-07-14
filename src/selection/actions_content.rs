//! Actions pane content projections used by mouse selection and copy.

use crate::selection::SelectablePane;
use crate::state::AppState;
use crate::ui::components::selectable_list::{ProjectedContentLine, projected_content_lines};
use crate::ui::components::{
    ActionsListLayout, ActionsListWindow, actions_header_rows, actions_list_props,
    actions_list_status_message,
};

use super::content::PaneContent;

#[must_use]
pub fn actions_list_lines(state: &AppState, render_cols: u16, render_rows: u16) -> PaneContent {
    let (pane_rows, _) = crate::layout::actions_pane_rows(
        usize::from(render_rows),
        state.actions_state.error.is_some(),
        state.actions_state.ui.filter_ui_open,
    );
    let runs = state.actions_state.runs();
    let filter = &state.actions_state.committed_filter;
    let has_filters =
        !filter.workflow.is_empty() || !filter.status.is_empty() || !filter.search.is_empty();
    let props = actions_list_props(
        runs,
        ActionsListWindow {
            selected_index: state.actions_state.selected_run_index(),
            list_pane_rows: u16::try_from(pane_rows).unwrap_or(u16::MAX),
            available_width: Some(crate::layout::pr_list_content_width(render_cols)),
            layout: ActionsListLayout::Compact,
        },
        false,
        actions_list_status_message(
            state.actions_state.list_loading(),
            runs.is_empty(),
            has_filters,
        ),
        crate::theme::ThemeColors::default(),
        None,
    );
    projected_content(SelectablePane::ActionsList, projected_content_lines(&props))
}

#[must_use]
pub fn actions_detail_lines(state: &AppState, render_cols: u16, _render_rows: u16) -> PaneContent {
    let Some(detail) = state.actions_state.run_detail.as_ref() else {
        return PaneContent::new(
            SelectablePane::ActionsDetail,
            [
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                "Select a workflow run to view details.".to_string(),
            ],
        );
    };
    let content_width = usize::from(crate::layout::prs_detail_content_width(render_cols));
    let mut lines = actions_header_rows(detail)
        .into_iter()
        .map(|row| crate::list_viewport::fit_text_to_width(&row.content, content_width))
        .collect::<Vec<_>>();
    lines.extend(crate::actions_detail_projection::actions_detail_body_lines(
        detail,
        &state.actions_state.expanded_jobs,
        content_width,
    ));
    PaneContent::new(SelectablePane::ActionsDetail, lines)
}

fn projected_content(pane: SelectablePane, lines: Vec<ProjectedContentLine>) -> PaneContent {
    PaneContent::new(pane, lines.into_iter().map(|line| line.text))
}
