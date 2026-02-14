//! Split mode screen - one row per running agent.

use iocraft::prelude::*;

use crate::app::AppState;
use crate::presenter::format::{format_elapsed, status_icon, truncate};
use crate::theme::{ResolvedColors, ThemeColors};
use crate::ui::components::keybind_bar::KeybindBar;
use crate::ui::components::sidebar::Sidebar;
use crate::ui::components::status_bar::StatusBar;

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
    let selected_repo = state.map_or(0, |s| s.selected_repo);

    let running_positions = state.map_or_else(Vec::new, AppState::running_agent_positions);
    let selected_row = state.map_or(0, |s| s.split.selected_row);
    let reorder_armed = state.is_some_and(|s| s.split.reorder_armed);

    let rows: Vec<(String, String, String, bool)> = if let Some(s) = state {
        if running_positions.is_empty() {
            vec![(
                "No running agents".to_owned(),
                "Press m or esc to return".to_owned(),
                String::new(),
                false,
            )]
        } else {
            running_positions
                .iter()
                .enumerate()
                .filter_map(|(idx, (repo_idx, agent_idx))| {
                    let repo = s.repositories.get(*repo_idx)?;
                    let agent = repo.agents.get(*agent_idx)?;
                    let marker = if reorder_armed && idx == selected_row {
                        "▸"
                    } else {
                        " "
                    };

                    let title = format!(
                        "{} {} {} / {}   {} {}",
                        marker,
                        status_icon(&agent.status),
                        repo.name,
                        agent.display_id,
                        truncate(&agent.purpose, 40),
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

                    Some((title, todo, last, reorder_armed && idx == selected_row))
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
                        selected: selected_repo,
                        focused: !reorder_armed,
                        colors: props.colors.clone(),
                    )
                }

                Box(
                    border_style: if reorder_armed { BorderStyle::Double } else { BorderStyle::Round },
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
                                " SPLIT MODE - Active Agents ({}) {}",
                                running_positions.len(),
                                if reorder_armed { "[reorder armed]" } else { "" }
                            ),
                            color: rc.fg,
                            weight: Weight::Bold,
                        )
                    }

                    #(rows.into_iter().map(|(title, todo, last, selected): (String, String, String, bool)| {
                        let border = if selected { BorderStyle::Double } else { BorderStyle::Round };
                        let fg = if selected { rc.sel_fg } else { rc.fg };
                        let bg = if selected { rc.sel_bg } else { rc.bg };
                        element! {
                            Box(
                                border_style: border,
                                border_color: rc.border,
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
                                    Text(content: todo, color: if selected { rc.sel_fg } else { rc.dim })
                                }
                                Box(height: 1u32) {
                                    Text(content: last, color: if selected { rc.sel_fg } else { rc.dim })
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
