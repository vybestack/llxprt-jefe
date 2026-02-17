//! Split screen - repository management with grab/reorder functionality.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-003
//! @pseudocode component-001 lines 21-28

use iocraft::prelude::*;

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

    let repo_count = state.map_or(0, |s| s.repositories.len());
    let running_count = state.map_or(0, |s| s.agents.iter().filter(|a| a.is_running()).count());
    let agent_count = state.map_or(0, |s| s.agents.len());
    let repositories = state.map_or_else(Vec::new, |s| s.repositories.clone());
    let selected_repo_idx = state.and_then(|s| s.selected_repository_index).unwrap_or(0);
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
                colors: colors.clone(),
            )

            // Main content - repository list
            Box(
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
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
                    flex_grow: 1.0,
                    width: 100pct,
                    border_style: BorderStyle::Round,
                    border_color: rc.border,
                    background_color: rc.bg,
                ) {
                    #(repositories.iter().enumerate().map(|(i, repo)| {
                        let selected = i == selected_repo_idx;
                        let prefix = if selected { "> " } else { "  " };
                        let color = if selected { rc.bright } else { rc.dim };
                        element! {
                            Text(
                                content: format!("{}{} ({} agents)", prefix, repo.name, repo.agent_ids.len()),
                                color: color,
                            )
                        }
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
