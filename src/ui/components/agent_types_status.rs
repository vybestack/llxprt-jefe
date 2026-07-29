//! Thin renderer for the pure agent-type status projection.

use iocraft::prelude::*;

use crate::agent_status_view::{AgentAvailabilityObservation, project_agent_type_statuses};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the startup Agent Types availability pane.
#[derive(Default, Props)]
pub struct AgentTypesStatusProps {
    pub observations: Vec<AgentAvailabilityObservation>,
    pub selected_index: usize,
    pub colors: ThemeColors,
}

/// Render all observed definitions with enablement and create gating visible.
#[component]
pub fn AgentTypesStatus(props: &AgentTypesStatusProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let rows = project_agent_type_statuses(&props.observations)
        .into_iter()
        .enumerate()
        .flat_map(|(index, row)| status_lines(row, index == props.selected_index))
        .map(|line| {
            element! {
                Text(content: line, color: rc.fg)
            }
        })
        .collect::<Vec<_>>();

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0_f32,
            height: 100pct,
            border_style: BorderStyle::Round,
            border_color: rc.border,
            padding_left: 1u32,
            padding_right: 1u32,
        ) {
            Text(content: "Agent Types", color: rc.bright)
            #(rows)
            Text(content: " Space Toggle  Enter Details  q Back", color: rc.dim)
        }
    }
}

fn status_lines(row: crate::agent_status_view::AgentTypeStatusView, selected: bool) -> Vec<String> {
    let marker = if selected { ">" } else { " " };
    let enablement = if row.enabled { "enabled" } else { "disabled" };
    let create = if row.create_enabled {
        "[Create enabled]"
    } else {
        "[Create disabled]"
    };
    let mut lines = vec![format!(
        "{marker} {}  {}, {}  {create}",
        row.display_name, row.status_text, enablement
    )];
    if let Some(reason) = row.reason {
        let code = row
            .error_code
            .map_or(String::new(), |value| format!("{value}  "));
        lines.push(format!("  {code}{reason}"));
    }
    lines
}
