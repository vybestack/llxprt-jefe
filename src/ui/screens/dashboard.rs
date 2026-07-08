//! Main dashboard screen - sidebar, agent list, terminal, and preview.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-002
//! @requirement REQ-FUNC-003
//! @pseudocode component-001 lines 13-20

use iocraft::prelude::*;

use crate::layout::{LEFT_COL_WIDTH, RIGHT_COL_WIDTH, dashboard_middle_row_heights};
use crate::runtime::TerminalSnapshot;
use crate::state::{AppState, DashboardGrabPane, PaneFocus, ScreenMode};
use crate::theme::{ResolvedColors, ThemeColors};

use super::super::components::{
    KeybindBar, Preview, Sidebar, StatusBar, TerminalView, agent_list_props,
    selectable_list_element,
};

/// Props for the dashboard screen.
#[derive(Default, Props)]
pub struct DashboardProps {
    /// Application state (cloned snapshot).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
    /// Active theme name.
    pub theme_name: String,
    /// Terminal snapshot for the active agent's PTY.
    pub terminal_snapshot: Option<TerminalSnapshot>,
}

/// The main dashboard screen: sidebar + middle (agents + terminal) + preview.
///
/// Layout (toy1 pattern):
/// ```text
/// +----------------------------------------------------------+
/// | StatusBar                                                |
/// +--------+---------------------------+---------------------+
/// |        |  AgentList (25%)          |                     |
/// |Sidebar |---------------------------| Preview             |
/// | (22)   |  TerminalView (75%)       |                     |
/// |        |                           |                     |
/// +--------+---------------------------+---------------------+
/// | KeybindBar                                               |
/// +----------------------------------------------------------+
/// ```
#[component]
pub fn Dashboard(props: &DashboardProps) -> impl Into<AnyElement<'static>> {
    let state = props.state.as_ref();
    let selection = state.and_then(|s| s.selection);

    // Extract state values with defaults
    let visible_repo_indices = state.map_or_else(Vec::new, AppState::visible_repository_indices);
    let repo_count = visible_repo_indices.len();
    let running_count = state.map_or(0, |s| s.agents.iter().filter(|a| a.is_running()).count());
    let agent_count = state.map_or(0, AppState::visible_agent_count);
    let selected_repo_idx = state
        .and_then(AppState::selected_repository_visible_index)
        .unwrap_or(0);
    let selected_agent_idx = state
        .and_then(crate::state::AppState::selected_agent_local_index)
        .unwrap_or(0);
    let pane_focus = state.map_or(PaneFocus::Repositories, |s| s.pane_focus);
    let terminal_focused = state.is_some_and(|s| s.terminal_focused);

    // Dashboard reorder grab indicator indices (only for the relevant pane).
    let grabbed_repo_idx = state.and_then(|s| match s.dashboard_grab.as_ref()? {
        DashboardGrabPane::Repository { visible_index } => Some(*visible_index),
        DashboardGrabPane::Agent { .. } => None,
    });
    let grabbed_agent_idx = state.and_then(|s| match s.dashboard_grab.as_ref()? {
        DashboardGrabPane::Agent { local_index, .. } => Some(*local_index),
        DashboardGrabPane::Repository { .. } => None,
    });

    let repositories: Vec<_> = state.map_or_else(Vec::new, |s| {
        visible_repo_indices
            .iter()
            .filter_map(|idx| s.repositories.get(*idx).cloned())
            .collect()
    });
    let agent_counts: Vec<usize> = state.map_or_else(Vec::new, |s| {
        visible_repo_indices
            .iter()
            .filter_map(|idx| {
                s.repositories
                    .get(*idx)
                    .map(|repo| s.visible_agent_count_for_repository(&repo.id))
            })
            .collect()
    });
    let agents = state.map_or_else(Vec::new, |s| {
        s.selected_repository()
            .map_or_else(Vec::new, |repo| s.visible_agents_for_repository(&repo.id))
    });
    let selected_agent_data = state.and_then(|s| s.selected_agent().cloned());

    // Whether the selected agent is Running with a live session. Threading this
    // to TerminalView lets the empty-state copy distinguish a healthy live
    // session (viewer not yet attached) from a genuinely unattached terminal
    // (issue #160).
    let session_live = selected_agent_data
        .as_ref()
        .is_some_and(crate::domain::Agent::is_running);

    // Resolve colors with green screen fallback
    let colors = props.colors.clone().unwrap_or_default();
    let rc = ResolvedColors::from_theme(Some(&colors));

    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let (agent_rows, terminal_rows) = dashboard_middle_row_heights(term_cols, term_rows);

    // Single source of truth for fixed column widths: the iocraft width field
    // expects a u32, so convert the u16 layout constants once here.
    let sidebar_width = u32::from(LEFT_COL_WIDTH);
    let preview_width = u32::from(RIGHT_COL_WIDTH);

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            background_color: rc.bg,
            width: 100pct,
            height: 100pct,
        ) {
            // Top status bar
            StatusBar(
                repo_count: repo_count,
                running_count: running_count,
                agent_count: agent_count,
                theme_name: props.theme_name.clone(),
                version: crate::VERSION.to_owned(),
                warning_message: state.and_then(|s| s.warning_message.clone()),
                colors: colors.clone(),
                selection: selection,
            )

            // Main content area
            Box(
                flex_direction: FlexDirection::Row,
                flex_grow: 1.0,
                width: 100pct,
            ) {
                // Sidebar (fixed width)
                Box(width: sidebar_width, height: 100pct) {
                    Sidebar(
                        repositories: repositories,
                        agent_counts: agent_counts,
                        selected: selected_repo_idx,
                        focused: !terminal_focused && pane_focus == PaneFocus::Repositories,
                        grabbed: grabbed_repo_idx,
                        colors: colors.clone(),
                        selection: selection,
                    )
                }

                // Middle column (agent list + terminal)
                Box(
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    height: 100pct,
                ) {
                    Box(height: agent_rows, width: 100pct) {
                        #(vec![selectable_list_element(agent_list_props(
                            &agents,
                            selected_agent_idx,
                            grabbed_agent_idx,
                            !terminal_focused && pane_focus == PaneFocus::Agents,
                            colors.clone(),
                            selection,
                        ))])
                    }
                    Box(height: terminal_rows, width: 100pct) {
                        TerminalView(
                            snapshot: props.terminal_snapshot.clone(),
                            focused: terminal_focused,
                            colors: colors.clone(),
                            selection: selection,
                            session_live: session_live,
                        )
                    }
                }

                // Preview pane (fixed width)
                Box(width: preview_width, height: 100pct) {
                    Preview(
                        agent: selected_agent_data,
                        focused: false,
                        colors: colors.clone(),
                    )
                }
            }

            // Bottom keybind bar
            KeybindBar(
                screen_mode: state.map_or(ScreenMode::Dashboard, |s| s.screen_mode),
                terminal_focused: terminal_focused,
                colors: colors,
            )
        }
    }
    .into_any()
}
