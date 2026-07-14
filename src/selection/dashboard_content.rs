//! Dashboard Agent-list and Preview copy projections.

use crate::dashboard_git_info::DashboardGitInfoSnapshot;
use crate::list_viewport::bordered_padded_content_width;
use crate::selection::SelectablePane;
use crate::state::{AppState, DashboardGrabPane};
use crate::ui::components::selectable_list::projected_content_lines;
use crate::ui::components::{
    AgentListSelection, AgentListView, AgentListWindow, agent_list_props, preview_content_lines,
};

use super::content::PaneContent;

#[must_use]
pub fn agent_list_lines(
    state: &AppState,
    render_cols: u16,
    render_rows: u16,
    git_info: Option<&DashboardGitInfoSnapshot>,
) -> PaneContent {
    let Some(repo) = state.selected_repository() else {
        return PaneContent::empty(SelectablePane::AgentList);
    };
    let agents = state.visible_agents_for_repository(&repo.id);
    let configured = git_info.is_none().then(|| {
        let info = crate::git_info::GitRepoInfo::from_configured_origin(&repo.github_repo);
        vec![info; agents.len()]
    });
    let git_infos = git_info.map_or_else(
        || configured.as_deref().unwrap_or_default(),
        |info| info.agents.as_slice(),
    );
    let pane_rows = crate::layout::dashboard_middle_row_heights_inner(render_rows).0;
    let pane_cols =
        render_cols.saturating_sub(crate::layout::LEFT_COL_WIDTH + crate::layout::RIGHT_COL_WIDTH);
    let props = agent_list_props(
        &agents,
        git_infos,
        AgentListView {
            selection: AgentListSelection {
                selected: state.selected_agent_local_index().unwrap_or(0),
                grabbed: state.dashboard_grab.as_ref().and_then(|grab| match grab {
                    DashboardGrabPane::Agent { local_index, .. } => Some(*local_index),
                    DashboardGrabPane::Repository { .. } => None,
                }),
            },
            window: AgentListWindow {
                pane_rows,
                content_width: bordered_padded_content_width(pane_cols),
            },
        },
        false,
        crate::theme::ThemeColors::default(),
        None,
    );
    PaneContent::new(
        SelectablePane::AgentList,
        projected_content_lines(&props)
            .into_iter()
            .map(|line| line.text),
    )
}

#[must_use]
pub fn preview_lines(state: &AppState, git_info: Option<&DashboardGitInfoSnapshot>) -> PaneContent {
    let agent = state.selected_agent();
    let configured = state.selected_repository().map(|repository| {
        crate::git_info::GitRepoInfo::from_configured_origin(&repository.github_repo)
    });
    let resolved = git_info.and_then(|info| info.preview.as_ref());
    let content_width = usize::from(bordered_padded_content_width(
        crate::layout::RIGHT_COL_WIDTH,
    ));
    PaneContent::new(
        SelectablePane::Preview,
        preview_content_lines(agent, resolved.or(configured.as_ref()), content_width),
    )
}
