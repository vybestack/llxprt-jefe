//! Agent list component.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-002
//! @requirement REQ-FUNC-006

use iocraft::prelude::*;

use crate::domain::{Agent, AgentStatus};
use crate::theme::ThemeColors;

use super::{ListPanel, ListPanelRow, ListPanelSegment};

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

fn agent_row(agent: &Agent, selected: bool) -> ListPanelRow {
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
        AgentStatus::Running | AgentStatus::Completed => Some(Color::Green),
        AgentStatus::Dead | AgentStatus::Errored => Some(Color::Red),
        AgentStatus::Waiting => Some(Color::Yellow),
        AgentStatus::Paused => Some(Color::Blue),
        AgentStatus::Queued => None,
    };
    let prefix = if selected { "> " } else { "  " };
    let shortcut_label = agent
        .shortcut_slot
        .map_or_else(String::new, |slot| format!("[{slot}] "));

    ListPanelRow {
        primary: vec![
            ListPanelSegment {
                text: prefix.to_string(),
                color: None,
            },
            ListPanelSegment {
                text: status_icon.to_string(),
                color: status_color,
            },
            ListPanelSegment {
                text: format!(" {}{}", shortcut_label, agent.name),
                color: None,
            },
        ],
        secondary: Vec::new(),
    }
}

/// Agent list showing agents for the current repository.
#[component]
pub fn AgentList(props: &AgentListProps) -> impl Into<AnyElement<'static>> {
    let rows: Vec<ListPanelRow> = props
        .agents
        .iter()
        .enumerate()
        .map(|(i, agent)| agent_row(agent, i == props.selected))
        .collect();

    element! {
        ListPanel(
            title: "Agents".to_string(),
            rows: rows,
            selected_index: Some(props.selected),
            focused: props.focused,
            loading: false,
            loading_message: String::new(),
            empty_message: "No agents yet".to_string(),
            compact: true,
            scroll_offset: props.selected.saturating_sub(1),
            colors: props.colors.clone(),
        )
    }
}
