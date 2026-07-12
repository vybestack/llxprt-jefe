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
    AgentListSelection, KeybindBar, Preview, Sidebar, StatusBar, TerminalView, agent_list_props,
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
    /// Retained scrollback history lines for the attached terminal (issue #198).
    pub history_lines: Vec<String>,
    /// Actual embedded-terminal pane dimensions (PTY layout). Used as the
    /// viewport projection size when the live snapshot is absent/empty so
    /// follow-tail/scroll math reflects the physical pane, not the whole
    /// retained history (issue #198 follow-up).
    pub terminal_pane_rows: usize,
    pub terminal_pane_cols: usize,
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
    // Resolve the selected repository once — reused for agents, git info, and
    // the preview pane (avoids redundant `selected_repository()` calls).
    let selected_repo = state.and_then(|s| s.selected_repository());
    let agents = selected_repo.map_or_else(Vec::new, |repo| {
        state.map_or_else(Vec::new, |s| s.visible_agents_for_repository(&repo.id))
    });

    // Resolve git display info (origin shortform + branch) for each visible
    // agent, parallel to `agents` by index (issue #170).
    let agent_git_infos: Vec<crate::git_info::GitRepoInfo> =
        selected_repo.map_or_else(Vec::new, |repo| {
            agents
                .iter()
                .map(|agent| {
                    crate::git_info::GitRepoInfo::resolve(
                        &repo.github_repo,
                        repo.remote.enabled,
                        &agent.work_dir,
                    )
                })
                .collect()
        });

    let selected_agent_data = state.and_then(|s| s.selected_agent().cloned());

    // Git info for the preview pane: reuse the already-resolved entry from
    // `agent_git_infos` when the selected agent is visible (avoids a redundant
    // `GitRepoInfo::resolve` call on every render frame).
    let selected_agent_git_info = agent_git_infos.get(selected_agent_idx).cloned();

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
                kennel_mode: state.is_some_and(crate::state::AppState::is_kennel_mode),
                warning_message: state.and_then(|s| s.warning_message.clone()),
                colors: colors.clone(),
                selection: selection,
            )

            // Main content area
            Box(
                flex_direction: FlexDirection::Row,
                flex_grow: 1.0_f32,
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
                    flex_grow: 1.0_f32,
                    height: 100pct,
                ) {
                    Box(height: agent_rows, width: 100pct) {
                        #(vec![selectable_list_element(agent_list_props(
                            &agents,
                            &agent_git_infos,
                            AgentListSelection {
                                selected: selected_agent_idx,
                                grabbed: grabbed_agent_idx,
                            },
                            !terminal_focused && pane_focus == PaneFocus::Agents,
                            colors.clone(),
                            selection,
                        ))])
                    }
                    Box(height: terminal_rows, width: 100pct) {
                        TerminalView(
                            // When a Jefe-owned terminal selection is active,
                            // use the snapshot that was captured at gesture
                            // start (selection_snapshot) so the highlight and
                            // copy use the SAME grid data (Finding B, issue
                            // #197). Gate on the selection targeting the
                            // terminal pane: a sidebar/agent-list selection
                            // renders from app state, not the terminal grid,
                            // so it must not pin the terminal to a stale
                            // snapshot (issue #197 review). Otherwise use the
                            // live render snapshot.
                            snapshot: {
                                let pinned = state.and_then(|s| {
                                    let sel_is_terminal = s
                                        .selection
                                        .is_some_and(|sel| {
                                            sel.pane()
                                                == crate::selection::SelectablePane::TerminalView
                                        });
                                    if sel_is_terminal {
                                        s.selection_snapshot.clone()
                                    } else {
                                        None
                                    }
                                });
                                pinned.or_else(|| props.terminal_snapshot.clone())
                            },
                            focused: terminal_focused,
                            colors: colors.clone(),
                            selection: selection,
                            session_live: session_live,
                            history_lines: props.history_lines.clone(),
                            terminal_history_offset: state.and_then(|s| s.terminal_history_offset),
                            override_theme: state.is_some_and(|s| s.override_agent_theme),
                            pane_rows: props.terminal_pane_rows,
                            pane_cols: props.terminal_pane_cols,
                        )
                    }
                }

                // Preview pane (fixed width)
                Box(width: preview_width, height: 100pct) {
                    Preview(
                        agent: selected_agent_data,
                        git_info: selected_agent_git_info,
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
