//! Split screen - repository management with grab/reorder functionality.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-003
//! @pseudocode component-001 lines 21-28

use iocraft::prelude::*;

use crate::selection::{SelectablePane, row_highlight_range};
use crate::state::{AppState, ScreenMode};
use crate::theme::{ResolvedColors, ThemeColors};

use super::super::components::{KeybindBar, StatusBar};

/// Props for the split screen.
#[derive(Default, Props)]
pub struct SplitScreenProps {
    /// Application state (cloned snapshot).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
    /// Active theme name.
    pub theme_name: String,
}

/// Split screen for repository management.
///
/// Layout:
/// ```text
/// +----------------------------------------------------------+
/// | StatusBar                                                |
/// +----------------------------------------------------------+
/// | Repository list with filter/search                       |
/// |                                                          |
/// | [Grab mode: select and reorder with arrows]             |
/// |                                                          |
/// +----------------------------------------------------------+
/// | KeybindBar (split mode keys)                            |
/// +----------------------------------------------------------+
/// ```
#[component]
pub fn SplitScreen(props: &SplitScreenProps) -> impl Into<AnyElement<'static>> {
    let state = props.state.as_ref();
    let selection = state.and_then(|s| s.selection);

    let visible_repo_indices = state.map_or_else(Vec::new, AppState::visible_repository_indices);
    let repo_count = visible_repo_indices.len();
    let running_count = state.map_or(0, |s| s.agents.iter().filter(|a| a.is_running()).count());
    let agent_count = state.map_or(0, AppState::visible_agent_count);
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
    let selected_repo_idx = state
        .and_then(AppState::selected_repository_visible_index)
        .unwrap_or(0);
    // @plan PLAN-20260216-FIRSTVERSION-V1.P11
    let search_query = state
        .and_then(|s| {
            if let crate::state::ModalState::Search { query } = &s.modal {
                Some(query.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    let colors = props.colors.clone().unwrap_or_default();
    let rc = ResolvedColors::from_theme(Some(&colors));

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            background_color: rc.bg,
            width: 100pct,
            height: 100pct,
        ) {
            // Status bar
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

            // Main content - repository list
            Box(
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0_f32,
                width: 100pct,
                padding: 1u32,
                background_color: rc.bg,
            ) {
                // Search/filter bar
                Box(height: 3u32, width: 100pct, background_color: rc.bg) {
                    Text(content: format!("Filter: {}_", search_query), color: rc.fg)
                }

                // Repository list
                Box(
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0_f32,
                    width: 100pct,
                    border_style: BorderStyle::Round,
                    border_color: rc.border,
                    background_color: rc.bg,
                ) {
                    #(repositories.iter().enumerate().map(|(i, repo)| {
                        let selected = i == selected_repo_idx;
                        let prefix = if selected { "> " } else { "  " };
                        let visible_count = agent_counts.get(i).copied()
                            .unwrap_or(repo.agent_ids.len());
                        let line = format!("{}{} ({} agents)", prefix, repo.name, visible_count);
                        let highlighted = selection
                            .filter(|s| s.pane() == SelectablePane::Sidebar)
                            .and_then(|s| row_highlight_range(&s, i))
                            .is_some();
                        let row_bg = if highlighted { rc.sel_bg } else { rc.bg };
                        let fg = if highlighted { rc.sel_fg } else { rc.fg };
                        let weight = if selected { Weight::Bold } else { Weight::Normal };
                        element! {
                            Box(height: 1u32, background_color: row_bg) {
                                Text(content: line, color: fg, weight: weight)
                            }
                        }
                        .into_any()
                    }))
                }
            }

            // Keybind bar
            KeybindBar(
                screen_mode: ScreenMode::Split,
                terminal_focused: false,
                colors: colors,
            )
        }
    }
}
