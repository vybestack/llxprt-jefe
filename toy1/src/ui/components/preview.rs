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

    lines.push((format!("  Profile: {}", agent.profile), rc.fg));
    lines.push((format!("  Mode:    {}", agent.mode), rc.dim));

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

/// Static placeholder content for the preview pane when no agent is selected.
/// Keeps the visual layout populated so the three-column look is preserved.
fn build_placeholder_lines(rc: &ResolvedColors) -> Vec<(String, Color)> {
    vec![
        (" #1872 Fix ACP socket timeout".to_owned(), rc.dim),
        ("  Status:  ● Running  00:42:17".to_owned(), rc.dim),
        ("  Profile: default".to_owned(), rc.dim),
        ("  Mode:    --yolo".to_owned(), rc.dim),
        (String::new(), rc.dim),
        ("  -- Todo --".to_owned(), rc.dim),
        ("  [OK] Read issue description".to_owned(), rc.dim),
        ("  [OK] Find relevant source files".to_owned(), rc.dim),
        ("  ▸ Implement socket timeout".to_owned(), rc.dim),
        ("  ○ Write tests".to_owned(), rc.dim),
        ("  ○ Run CI checks".to_owned(), rc.dim),
        (String::new(), rc.dim),
        ("  -- Output --".to_owned(), rc.dim),
        ("  Editing src/acp/socket.rs".to_owned(), rc.dim),
        ("  Added timeout parameter to".to_owned(), rc.dim),
        ("  connect() with default of".to_owned(), rc.dim),
        ("  30 seconds...".to_owned(), rc.dim),
        (String::new(), rc.dim),
        ("  N new repo  n new agent".to_owned(), rc.dim),
    ]
}

/// Right-side agent preview/detail pane.
#[component]
pub fn Preview(props: &PreviewProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());
    let border_style = if props.focused { BorderStyle::Double } else { BorderStyle::Round };

    let lines: Vec<(String, Color)> = if let Some(agent) = &props.agent {
        build_preview_lines(agent, &rc)
    } else {
        build_placeholder_lines(&rc)
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
