//! Dashboard Agent-list and Preview copy projections.

use crate::dashboard_git_info::DashboardGitInfoSnapshot;
use crate::list_viewport::bordered_padded_content_width;
use crate::provider_panel_view::{
    ModelProjectionInput, project_model_rows, visible_projected_window,
};
use crate::selection::SelectablePane;
use crate::state::AppState;
use crate::ui::components::preview_content_lines;
use crate::workbench::HostPanelModelSource;

use super::content::PaneContent;

#[must_use]
pub fn agent_list_lines(
    state: &AppState,
    render_cols: u16,
    render_rows: u16,
    git_info: Option<&DashboardGitInfoSnapshot>,
) -> PaneContent {
    let pane_rows = crate::layout::dashboard_middle_row_heights_inner(render_rows).0;
    let pane_cols =
        render_cols.saturating_sub(crate::layout::LEFT_COL_WIDTH + crate::layout::RIGHT_COL_WIDTH);
    let content_height = pane_rows.saturating_sub(crate::layout::AGENT_LIST_CHROME_ROWS);
    let content_width = pane_cols.saturating_sub(crate::layout::AGENT_LIST_CHROME_COLS);
    let model = crate::host_panel_models::project_host_panel(
        state,
        HostPanelModelSource::AgentList,
        git_info,
    );
    let rows = project_model_rows(ModelProjectionInput {
        body: &model.body,
        affordances: &model.action_affordances,
        description: None,
        loading: false,
        stale: false,
        selected_id: model.selected_id.as_ref(),
        marked_id: model.grabbed_id.as_ref(),
        form_draft: None,
        body_width: usize::from(content_width.max(1)),
    });
    PaneContent::new(
        SelectablePane::AgentList,
        visible_projected_window(
            rows,
            model.scroll_offset,
            content_height,
            model
                .selected_id
                .as_ref()
                .filter(|_| model.reveal_selection),
        )
        .rows
        .into_iter()
        .map(|row| row.text),
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
    let observation = agent.and_then(|agent| state.observations.get(&agent.id));
    PaneContent::new(
        SelectablePane::Preview,
        preview_content_lines(
            agent,
            resolved.or(configured.as_ref()),
            observation,
            content_width,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::AgentStatus;
    use crate::test_support::{host_panel_agent, host_panel_repository};

    #[test]
    fn copied_agent_rows_equal_the_screen_projection_at_fixed_width_and_window() {
        const RENDER_COLS: u16 = 90;
        const RENDER_ROWS: u16 = 20;

        let mut state = AppState::new(crate::test_support::published_workbench());
        state.repositories = vec![host_panel_repository("alpha")];
        state.agents = (0..24)
            .map(|index| {
                host_panel_agent(
                    &format!("agent-{index}-with-a-long-name"),
                    "repo-alpha",
                    AgentStatus::Dead,
                )
            })
            .collect();
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(5);
        state.agent_scroll_offset = 2;
        let descriptor = state
            .published_workbench()
            .screen_registry()
            .get_identity(crate::workbench::DASHBOARD_IDENTITY)
            .unwrap_or_else(|| panic!("dashboard descriptor must be published"));
        let layout = crate::screen_layout::resolve_screen(&state, RENDER_COLS, RENDER_ROWS)
            .unwrap_or_else(|| panic!("dashboard layout must resolve"));
        let git = crate::dashboard_git_info::resolve_dashboard_git_info(&state);

        let view = crate::provider_panel_view::project_current_screen(&state, descriptor, &layout)
            .unwrap_or_else(|error| panic!("dashboard projection: {error}"));
        let rendered = view
            .panels
            .iter()
            .find(|panel| panel.id.as_str() == "agents")
            .unwrap_or_else(|| panic!("AgentList projection must exist"));
        let copied = agent_list_lines(&state, RENDER_COLS, RENDER_ROWS, git.as_ref());

        assert_eq!(rendered.visible_window_origin, 4);
        assert!(rendered.max_scroll_offset > 0);
        assert_eq!(copied.lines, rendered.lines);
    }

    fn manual_copy_fixture() -> (AppState, crate::workbench::HostPanelCapability) {
        let mut state = AppState::new(crate::test_support::published_workbench());
        state.repositories = vec![host_panel_repository("alpha")];
        state.agents = (0..24)
            .map(|index| host_panel_agent(&format!("a{index}"), "repo-alpha", AgentStatus::Dead))
            .collect();
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(20);
        state.resolved_layout = crate::screen_layout::resolve_screen(&state, 90, 20);
        let capability = state
            .published_workbench()
            .screen_registry()
            .get_identity(state.screen())
            .and_then(|descriptor| {
                descriptor.panel(&crate::workbench::PanelId::from_static("agents"))
            })
            .and_then(crate::workbench::PanelDescriptor::host_capability)
            .unwrap_or_else(|| panic!("agent capability"));
        (state, capability)
    }

    fn assert_copy_matches_manual_window(state: &AppState, expected_origin: usize) {
        let descriptor = state
            .published_workbench()
            .screen_registry()
            .get_identity(state.screen())
            .unwrap_or_else(|| panic!("dashboard descriptor"));
        let layout = state
            .resolved_layout
            .as_ref()
            .unwrap_or_else(|| panic!("layout"));
        let rendered =
            crate::provider_panel_view::project_current_screen(state, descriptor, layout)
                .unwrap_or_else(|error| panic!("projection: {error}"))
                .panels
                .into_iter()
                .find(|panel| panel.id.as_str() == "agents")
                .unwrap_or_else(|| panic!("agent panel"));
        let git = crate::dashboard_git_info::resolve_dashboard_git_info(state);
        assert_eq!(rendered.visible_window_origin, expected_origin);
        assert_eq!(
            agent_list_lines(state, 90, 20, git.as_ref()).lines,
            rendered.lines
        );
    }

    #[test]
    fn copied_rows_follow_manual_scroll_including_zero_then_selection_reveal() {
        let (mut state, capability) = manual_copy_fixture();
        assert_copy_matches_manual_window(&state, 19);
        assert!(state.scroll_host_panel(capability, -1, 2));
        assert_copy_matches_manual_window(&state, 18);
        assert_copy_matches_manual_window(&state, 18);
        for _ in 0..18 {
            assert!(state.scroll_host_panel(capability, -1, 2));
        }
        assert_copy_matches_manual_window(&state, 0);
        assert!(!state.scroll_host_panel(capability, -1, 2));
        assert_copy_matches_manual_window(&state, 0);
        assert!(state.apply_host_panel_action(
            capability,
            crate::host_controls::ControlAction::Previous,
            90,
            20
        ));
        assert_copy_matches_manual_window(&state, 18);
    }
}
