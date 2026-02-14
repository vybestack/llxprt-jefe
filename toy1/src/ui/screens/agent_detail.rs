//! Agent detail screen — expanded view of a single agent.

use iocraft::prelude::*;

use crate::app::AppState;
use crate::data::models::{OutputKind, TodoStatus};
use crate::presenter::format::{format_elapsed, status_icon, status_label, todo_icon};
use crate::theme::{ResolvedColors, ThemeColors};
use crate::ui::components::keybind_bar::KeybindBar;
use crate::ui::components::status_bar::StatusBar;

/// Props for the agent detail screen.
#[derive(Default, Props)]
pub struct AgentDetailProps {
    /// Application state (cloned snapshot).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
    /// Active theme name.
    pub theme_name: String,
}

/// Build detail lines for an agent.
fn build_detail_lines(state: &AppState, rc: &ResolvedColors) -> Vec<(String, Color)> {
    let agent = match state.current_agent() {
        Some(a) => a,
        None => return vec![("  No agent selected".to_owned(), rc.dim)],
    };

    let mut lines: Vec<(String, Color)> = Vec::new();

    lines.push((
        format!(
            " {} {}  --  {} {}  {}",
            agent.display_id,
            agent.purpose,
            status_icon(&agent.status),
            status_label(&agent.status),
            format_elapsed(agent.elapsed_secs)
        ),
        rc.fg,
    ));

    lines.push((
        format!("  Agent: {} via {}  Mode: {}", agent.model, agent.profile, agent.mode),
        rc.fg,
    ));
    lines.push((format!("  Dir:   {}", agent.work_dir), rc.dim));
    lines.push((String::new(), rc.dim));
    lines.push(("  Todo List".to_owned(), rc.fg));

    for todo in &agent.todos {
        let icon = todo_icon(&todo.status);
        let tc = match todo.status {
            TodoStatus::Completed | TodoStatus::Pending => rc.dim,
            TodoStatus::InProgress => rc.fg,
        };
        lines.push((format!("    {} {}", icon, todo.content), tc));
    }

    lines.push((String::new(), rc.dim));
    lines.push(("  Recent Output".to_owned(), rc.fg));

    for out in &agent.recent_output {
        let (prefix, oc) = match out.kind {
            OutputKind::ToolCall => ("    > ", rc.fg),
            OutputKind::Text => ("    ", rc.dim),
        };
        let suffix = out.tool_status.map_or_else(String::new, |ts| format!("  [{ts:?}]"));
        lines.push((format!("{}{}{}", prefix, out.content, suffix), oc));
    }

    lines
}

/// Expanded agent detail view.
#[component]
pub fn AgentDetail(props: &AgentDetailProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());

    let state = props.state.as_ref();
    let screen = state.map(|s| s.screen);
    let repo_count = state.map_or(0, |s| s.repositories.len());
    let running_count = state.map_or(0, AppState::running_count);
    let agent_count = state.map_or(0, AppState::agent_count);

    let detail_lines: Vec<(String, Color)> = match state {
        Some(s) => build_detail_lines(s, &rc),
        None => vec![("  No agent selected".to_owned(), rc.dim)],
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
            ) {
                Box(
                    border_style: BorderStyle::Round,
                    border_color: rc.border,
                    background_color: rc.bg,
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    padding: 1i32,
                ) {
                    #(detail_lines.into_iter().map(|(line, color): (String, Color)| {
                        element! {
                            Box(height: 1u32) {
                                Text(content: line, color: color)
                            }
                        }
                    }))
                }
            }

            KeybindBar(screen: screen, colors: props.colors.clone())
        }
    }
}
