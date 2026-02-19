//! Agent list component.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-002
//! @requirement REQ-FUNC-006

use iocraft::prelude::*;

use crate::domain::{Agent, AgentStatus};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the agent list component.
#[derive(Default, Props)]
pub struct AgentListProps {
    /// List of agents.
    pub agents: Vec<Agent>,
    /// Currently selected agent index.
    pub selected: usize,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Agent list showing agents for the current repository.
#[component]
pub fn AgentList(props: &AgentListProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let border_style = if props.focused {
        BorderStyle::Double
    } else {
        BorderStyle::Round
    };

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            height: 100pct,
            border_style: border_style,
            border_color: rc.border,
            background_color: rc.bg,
        ) {
            // Title
            Box(height: 1u32, padding_left: 1u32) {
                Text(content: "Agents", weight: Weight::Bold, color: rc.fg)
            }

            // Agent list
            Box(
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                padding: 1u32,
                background_color: rc.bg,
            ) {
                #(props.agents.iter().enumerate().map(|(i, agent)| {
                    let selected = i == props.selected;
                    let status_icon = match agent.status {
                        AgentStatus::Running => "o",
                        AgentStatus::Completed => "+",
                        AgentStatus::Dead => "!",
                        AgentStatus::Errored => "x",
                        AgentStatus::Waiting => "*",
                        AgentStatus::Paused => "#",
                        AgentStatus::Queued => "-",
                    };
                    let status_color = match agent.status {
                        AgentStatus::Running | AgentStatus::Completed => rc.bright,
                        AgentStatus::Dead | AgentStatus::Errored => Color::Red,
                        AgentStatus::Waiting => Color::Yellow,
                        AgentStatus::Paused => Color::Blue,
                        AgentStatus::Queued => rc.dim,
                    };
                    let prefix = if selected { "> " } else { "  " };
                    let label = format!("{}{} {}", prefix, status_icon, agent.name);
                    if selected {
                        element! {
                            Box(height: 1u32, background_color: rc.sel_bg) {
                                Text(content: label, color: rc.sel_fg, weight: Weight::Bold)
                            }
                        }
                    } else {
                        element! {
                            Box(flex_direction: FlexDirection::Row, height: 1u32) {
                                Text(content: prefix, color: rc.fg)
                                Text(content: status_icon, color: status_color)
                                Text(content: format!(" {}", agent.name), color: rc.fg)
                            }
                        }
                    }
                }))
            }
        }
    }
}
