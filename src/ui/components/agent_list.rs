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
    let border_color = if props.focused { rc.border_focused } else { rc.border };

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            height: 100pct,
            border_style: BorderStyle::Round,
            border_color: border_color,
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
                        AgentStatus::Running => "*",
                        AgentStatus::Completed => "+",
                        AgentStatus::Dead => "x",
                        AgentStatus::Errored => "!",
                        AgentStatus::Waiting => "?",
                        AgentStatus::Paused => "-",
                        AgentStatus::Queued => "o",
                    };
                    let status_color = match agent.status {
                        AgentStatus::Running | AgentStatus::Completed => rc.bright,
                        AgentStatus::Dead | AgentStatus::Errored => Color::Red,
                        AgentStatus::Waiting => Color::Yellow,
                        AgentStatus::Paused => Color::Blue,
                        AgentStatus::Queued => rc.dim,
                    };
                    let prefix = if selected { "> " } else { "  " };
                    let name_color = if selected { rc.bright } else { rc.fg };
                    element! {
                        Box(flex_direction: FlexDirection::Row) {
                            Text(content: prefix, color: name_color)
                            Text(content: status_icon, color: status_color)
                            Text(content: format!(" {}", agent.name), color: name_color)
                        }
                    }
                }))
            }
        }
    }
}
