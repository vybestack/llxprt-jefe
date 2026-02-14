//! Main dashboard screen — sidebar, middle split (agents + terminal), and preview.

use iocraft::prelude::*;

use crate::app::{ActivePane, AppState};
use crate::pty::TerminalSnapshot;
use crate::theme::ThemeColors;
use crate::ui::components::agent_list::AgentList;
use crate::ui::components::keybind_bar::KeybindBar;
use crate::ui::components::preview::Preview;
use crate::ui::components::sidebar::Sidebar;
use crate::ui::components::status_bar::StatusBar;
use crate::ui::components::terminal_view::TerminalView;

/// Props for the dashboard screen.
#[derive(Default, Props)]
pub struct DashboardProps {
    /// Application state (cloned snapshot).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
    /// Active theme name.
    pub theme_name: String,
    /// Terminal plain-text lines for the active agent's PTY.
    pub terminal_lines: Vec<String>,
    /// Full styled terminal snapshot for the active agent's PTY.
    pub terminal_snapshot: Option<TerminalSnapshot>,
}

/// The main dashboard screen: sidebar + middle split + preview.
#[component]
pub fn Dashboard(props: &DashboardProps) -> impl Into<AnyElement<'static>> {
    let state = props.state.as_ref();

    let repo_count = state.map_or(0, |s| s.repositories.len());
    let running_count = state.map_or(0, AppState::running_count);
    let agent_count = state.map_or(0, AppState::agent_count);
    let selected_repo = state.map_or(0, |s| s.selected_repo);
    let selected_agent = state.map_or(0, |s| s.selected_agent);
    let active_pane = state.map_or(ActivePane::Sidebar, |s| s.active_pane);
    let screen = state.map(|s| s.screen);
    let terminal_focused = state.map_or(false, |s| s.terminal_focused);

    let repositories = state.map_or_else(Vec::new, |s| s.repositories.clone());
    let repo_name = state
        .and_then(AppState::current_repo)
        .map_or_else(String::new, |r| r.name.clone());
    let agents = state
        .and_then(AppState::current_repo)
        .map_or_else(Vec::new, |r| r.agents.clone());
    let selected_agent_data = state.and_then(AppState::current_agent).cloned();

    let rc = crate::theme::ResolvedColors::from_theme(props.colors.as_ref());

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            background_color: rc.bg,
            width: 100pct,
            height: 100pct,
        ) {
            StatusBar(
                repo_count: repo_count,
                running_count: running_count,
                agent_count: agent_count,
                theme_name: props.theme_name.clone(),
                colors: props.colors.clone(),
            )

            Box(
                flex_direction: FlexDirection::Row,
                flex_grow: 1.0,
                width: 100pct,
                align_items: AlignItems::Stretch,
            ) {
                Box(width: 22u32, height: 100pct) {
                    Sidebar(
                        repositories: repositories,
                        selected: selected_repo,
                        focused: !terminal_focused && active_pane == ActivePane::Sidebar,
                        colors: props.colors.clone(),
                    )
                }

                // Middle column (fills remaining width): top 25% list + bottom 75% terminal.
                Box(
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    height: 100pct,
                ) {
                    Box(height: 25pct, width: 100pct) {
                        AgentList(
                            repo_name: repo_name,
                            agents: agents,
                            selected: selected_agent,
                            focused: !terminal_focused && active_pane == ActivePane::AgentList,
                            colors: props.colors.clone(),
                        )
                    }
                    Box(height: 75pct, width: 100pct) {
                        TerminalView(
                            lines: props.terminal_lines.clone(),
                            snapshot: props.terminal_snapshot.clone(),
                            focused: terminal_focused,
                            colors: props.colors.clone(),
                        )
                    }
                }

                Box(width: 36u32, height: 100pct) {
                    Preview(
                        agent: selected_agent_data,
                        focused: !terminal_focused && active_pane == ActivePane::Preview,
                        colors: props.colors.clone(),
                    )
                }
            }

            KeybindBar(
                screen: screen,
                colors: props.colors.clone(),
            )
        }
    }
}
