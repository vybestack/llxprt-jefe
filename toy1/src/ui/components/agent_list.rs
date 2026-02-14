//! Agent list component showing agents for the selected repository.

use iocraft::prelude::*;

use crate::data::models::Agent;
use crate::presenter::format::{format_elapsed, status_icon, truncate};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the agent list.
#[derive(Default, Props)]
pub struct AgentListProps {
    /// The repository name for the header.
    pub repo_name: String,
    /// Agents to display.
    pub agents: Vec<Agent>,
    /// Index of the selected agent.
    pub selected: usize,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
}

/// Main agent list panel.
#[component]
pub fn AgentList(props: &AgentListProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());
    let border_style = if props.focused { BorderStyle::Double } else { BorderStyle::Round };

    let header = format!(" Agents: {}", props.repo_name);

    let selected = props.selected;
    let agent_rows: Vec<(String, bool)> = if props.agents.is_empty() {
        vec![("  No agents".to_owned(), false)]
    } else {
        props
            .agents
            .iter()
            .enumerate()
            .map(|(i, agent)| {
                let is_selected = i == selected;
                let icon = status_icon(&agent.status);
                let elapsed = format_elapsed(agent.elapsed_secs);
                let purpose = truncate(&format!("{} {}", agent.display_id, agent.purpose), 36);
                let sel = if is_selected { "\u{25b8}" } else { " " };
                let line = format!(" {} {} {}  {}", sel, icon, purpose, elapsed);
                (line, is_selected)
            })
            .collect()
    };

    element! {
        Box(
            border_style: border_style,
            border_color: rc.border,
            background_color: rc.bg,
            width: 100pct,
            height: 100pct,
            flex_direction: FlexDirection::Column,
            padding_left: 1i32,
            padding_right: 1i32,
        ) {
            Box(height: 1u32) {
                Text(content: header, color: rc.fg, weight: Weight::Bold)
            }
            #(agent_rows.into_iter().map(|(line, is_sel): (String, bool)| {
                if is_sel {
                    element! {
                        Box(height: 1u32, background_color: rc.sel_bg) {
                            Text(content: line, color: rc.sel_fg, weight: Weight::Bold)
                        }
                    }
                } else {
                    element! {
                        Box(height: 1u32) {
                            Text(content: line, color: rc.fg)
                        }
                    }
                }
            }))
        }
    }
}
