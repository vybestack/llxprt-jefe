//! Split mode screen - one row per running agent.
//!
//! Visual states per agent row:
//! - Normal:   round border, theme fg
//! - Selected: double border, theme fg (cursor highlight)
//! - Grabbed:  double border, inverse colors (being reordered)

use iocraft::prelude::*;

use crate::app::{AppState, SplitFocus};
use crate::presenter::format::{format_elapsed, status_icon, truncate};
use crate::theme::{ResolvedColors, ThemeColors};
use crate::ui::components::keybind_bar::KeybindBar;
use crate::ui::components::sidebar::Sidebar;
use crate::ui::components::status_bar::StatusBar;

/// Visual state for a single agent row in split mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RowStyle {
    Normal,
    Selected,
    Grabbed,
}

#[derive(Default, Props)]
pub struct SplitScreenProps {
    pub state: Option<AppState>,
    pub colors: Option<ThemeColors>,
    pub theme_name: String,
}

#[component]
pub fn SplitScreen(props: &SplitScreenProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());
    let state = props.state.as_ref();

    let repo_count = state.map_or(0, |s| s.repositories.len());
    let running_count = state.map_or(0, AppState::running_count);
    let agent_count = state.map_or(0, AppState::agent_count);

    let repos = state.map_or_else(Vec::new, |s| s.repositories.clone());
    let repo_cursor = state.map_or(0, |s| s.split.repo_cursor);
    let split_focus = state.map_or(SplitFocus::Repos, |s| s.split.focus);
    let grabbed = state.is_some_and(|s| s.split.grabbed);

    let filtered_positions = state.map_or_else(Vec::new, AppState::filtered_running_positions);
    let selected_row = state.map_or(0, |s| s.split.selected_row);
    let agents_focused = split_focus == SplitFocus::Agents;

    let status_suffix = if grabbed {
        "[GRABBED - ↑↓ reorder, enter release]"
    } else if agents_focused {
        "[↑↓ select, enter grab, esc back]"
    } else {
        "[r repos, a agents]"
    };

    let rows: Vec<(String, String, String, RowStyle)> = if let Some(s) = state {
        if filtered_positions.is_empty() {
            vec![(
                "No running agents".to_owned(),
                "Press m or esc to return".to_owned(),
                String::new(),
                RowStyle::Normal,
            )]
        } else {
            filtered_positions
                .iter()
                .enumerate()
                .filter_map(|(idx, (repo_idx, agent_idx))| {
                    let repo = s.repositories.get(*repo_idx)?;
                    let agent = repo.agents.get(*agent_idx)?;

                    let row_style = if agents_focused && idx == selected_row {
                        if grabbed { RowStyle::Grabbed } else { RowStyle::Selected }
                    } else {
                        RowStyle::Normal
                    };

                    let marker = match row_style {
                        RowStyle::Grabbed => "≡",
                        RowStyle::Selected => "▸",
                        RowStyle::Normal => " ",
                    };

                    let title = format!(
                        "{} {} {} / {}   {} {}",
                        marker,
                        status_icon(&agent.status),
                        repo.name,
                        agent.display_id,
                        truncate(&agent.name, 40),
                        format_elapsed(agent.elapsed_secs),
                    );

                    let todo = agent
                        .todos
                        .iter()
                        .find(|t| matches!(t.status, crate::data::models::TodoStatus::InProgress))
                        .map_or_else(
                            || "Todo: (none in progress)".to_owned(),
                            |t| format!("Todo: ▸ {}", truncate(&t.content, 64)),
                        );

                    let last = agent.recent_output.last().map_or_else(
                        || "Last: (no output)".to_owned(),
                        |line| format!("Last: {}", truncate(&line.content, 72)),
                    );

                    Some((title, todo, last, row_style))
                })
                .collect()
        }
    } else {
        vec![]
    };

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
                        repositories: repos,
                        selected: repo_cursor,
                        focused: split_focus == SplitFocus::Repos,
                        colors: props.colors.clone(),
                        show_all: true,
                    )
                }

                Box(
                    border_style: if agents_focused { BorderStyle::Double } else { BorderStyle::Round },
                    border_color: rc.border,
                    background_color: rc.bg,
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    width: 100pct,
                    padding_left: 1i32,
                    padding_right: 1i32,
                ) {
                    Box(height: 1u32) {
                        Text(
                            content: format!(
                                " SPLIT - Agents ({}) {}",
                                filtered_positions.len(),
                                status_suffix,
                            ),
                            color: rc.fg,
                            weight: Weight::Bold,
                        )
                    }

                    #(rows.into_iter().map(|(title, todo, last, style): (String, String, String, RowStyle)| {
                        let border = match style {
                            RowStyle::Grabbed | RowStyle::Selected => BorderStyle::Double,
                            RowStyle::Normal => BorderStyle::Round,
                        };
                        let fg = match style {
                            RowStyle::Grabbed => rc.sel_fg,
                            _ => rc.fg,
                        };
                        let bg = match style {
                            RowStyle::Grabbed => rc.sel_bg,
                            _ => rc.bg,
                        };
                        let dim = match style {
                            RowStyle::Grabbed => rc.sel_fg,
                            _ => rc.dim,
                        };
                        element! {
                            Box(
                                border_style: border,
                                border_color: if style == RowStyle::Selected { rc.sel_bg } else { rc.border },
                                background_color: bg,
                                flex_grow: 1.0,
                                width: 100pct,
                                flex_direction: FlexDirection::Column,
                                padding_left: 1i32,
                                padding_right: 1i32,
                            ) {
                                Box(height: 1u32) {
                                    Text(content: title, color: fg, weight: Weight::Bold)
                                }
                                Box(height: 1u32) {
                                    Text(content: todo, color: dim)
                                }
                                Box(height: 1u32) {
                                    Text(content: last, color: dim)
                                }
                            }
                        }
                    }))
                }
            }

            KeybindBar(screen: state.map(|s| s.screen), colors: props.colors.clone())
        }
    }
}
