//! Agent preview pane showing details of the selected agent.

use iocraft::prelude::*;

use crate::data::models::{Agent, OutputKind, TodoStatus};
use crate::presenter::format::{format_elapsed, status_icon, status_label, todo_icon};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the preview pane.
#[derive(Default, Props)]
pub struct PreviewProps {
    /// The agent to preview (cloned).
    pub agent: Option<Agent>,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
}

/// Build preview lines from an agent.
fn build_preview_lines(agent: &Agent, rc: &ResolvedColors) -> Vec<(String, Color)> {
    let mut lines: Vec<(String, Color)> = Vec::new();

    lines.push((
        format!(" {} {}", agent.display_id, agent.purpose),
        rc.fg,
    ));
    lines.push((
        format!(
            "  Status:  {} {}  {}",
            status_icon(&agent.status),
            status_label(&agent.status),
            format_elapsed(agent.elapsed_secs)
        ),
        rc.fg,
    ));

    lines.push((format!("  Model:   {}", agent.model), rc.fg));
    lines.push((format!("  Profile: {}", agent.profile), rc.dim));

    lines.push(("  -- Todo --".to_owned(), rc.fg));
    for todo in &agent.todos {
        let icon = todo_icon(&todo.status);
        let tc = match todo.status {
            TodoStatus::Completed | TodoStatus::Pending => rc.dim,
            TodoStatus::InProgress => rc.fg,
        };
        lines.push((format!("  {} {}", icon, todo.content), tc));
    }

    lines.push(("  -- Output --".to_owned(), rc.fg));
    for out in agent.recent_output.iter().rev().take(4).rev() {
        let prefix = if out.kind == OutputKind::ToolCall { "  > " } else { "  " };
        let oc = if out.kind == OutputKind::ToolCall { rc.fg } else { rc.dim };
        lines.push((format!("{}{}", prefix, out.content), oc));
    }

    lines
}

/// Right-side agent preview/detail pane.
#[component]
pub fn Preview(props: &PreviewProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());
    let border_style = if props.focused { BorderStyle::Double } else { BorderStyle::Round };

    let lines: Vec<(String, Color)> = if let Some(agent) = &props.agent {
        build_preview_lines(agent, &rc)
    } else {
        vec![("  No agent selected".to_owned(), rc.dim)]
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
            #(lines.into_iter().map(|(line, color): (String, Color)| {
                element! {
                    Box(height: 1u32) {
                        Text(content: line, color: color)
                    }
                }
            }))
        }
    }
}
