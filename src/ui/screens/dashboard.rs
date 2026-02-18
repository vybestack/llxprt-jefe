//! Main dashboard screen - sidebar, agent list, terminal, and preview.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-002
//! @requirement REQ-FUNC-003
//! @pseudocode component-001 lines 13-20

use iocraft::prelude::*;

use crate::runtime::TerminalSnapshot;
use crate::state::{AppState, PaneFocus, ScreenMode};
use crate::theme::{ResolvedColors, ThemeColors};

use super::super::components::{AgentList, KeybindBar, Preview, Sidebar, StatusBar, TerminalView};

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

    // Extract state values with defaults
    let repo_count = state.map_or(0, |s| s.repositories.len());
    let running_count = state.map_or(0, |s| s.agents.iter().filter(|a| a.is_running()).count());
    let agent_count = state.map_or(0, |s| s.agents.len());
    let selected_repo_idx = state.and_then(|s| s.selected_repository_index).unwrap_or(0);
    let selected_agent_idx = state
        .and_then(|s| s.selected_agent_local_index())
        .unwrap_or(0);
    let pane_focus = state.map_or(PaneFocus::Repositories, |s| s.pane_focus);
    let terminal_focused = state.is_some_and(|s| s.terminal_focused);

    let repositories = state.map_or_else(Vec::new, |s| s.repositories.clone());
    let agents = state.map_or_else(Vec::new, |s| {
        s.selected_repository().map_or_else(Vec::new, |repo| {
            s.agents
                .iter()
                .filter(|agent| agent.repository_id == repo.id)
                .cloned()
                .collect()
        })
    });
    let selected_agent_data = state.and_then(|s| s.selected_agent().cloned());

    // Resolve colors with green screen fallback
    let colors = props.colors.clone().unwrap_or_default();
    let rc = ResolvedColors::from_theme(Some(&colors));

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
                colors: colors.clone(),
            )

            // Main content area
            Box(
                flex_direction: FlexDirection::Row,
                flex_grow: 1.0,
                width: 100pct,
            ) {
                // Sidebar (fixed width)
                Box(width: 22u32, height: 100pct) {
                    Sidebar(
                        repositories: repositories,
                        selected: selected_repo_idx,
                        focused: !terminal_focused && pane_focus == PaneFocus::Repositories,
                        colors: colors.clone(),
                    )
                }

                // Middle column (agent list + terminal)
                Box(
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    height: 100pct,
                ) {
                    Box(height: 25pct, width: 100pct) {
                        AgentList(
                            agents: agents,
                            selected: selected_agent_idx,
                            focused: !terminal_focused && pane_focus == PaneFocus::Agents,
                            colors: colors.clone(),
                        )
                    }
                    Box(height: 75pct, width: 100pct) {
                        TerminalView(
                            snapshot: props.terminal_snapshot.clone(),
                            focused: terminal_focused,
                            colors: colors.clone(),
                        )
                    }
                }

                // Preview pane (fixed width)
                Box(width: 36u32, height: 100pct) {
                    Preview(
                        agent: selected_agent_data,
                        focused: !terminal_focused && pane_focus == PaneFocus::Terminal,
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
}
