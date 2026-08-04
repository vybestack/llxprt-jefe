//! Pure, iocraft-free selected-agent Preview projection.

use std::time::Instant;

use crate::domain::observation::{AgentObservation, Availability, FieldState, TodoState};
use crate::domain::{Agent, AgentStatus};
use crate::git_info::GitRepoInfo;
use crate::list_viewport::fit_text_to_width;
use crate::status_precedence::{ResolvedStatus, resolve_status};

/// Plain finite-width rows consumed by the Preview component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewView {
    pub lines: Vec<String>,
    pub todo_header_row: Option<usize>,
}

/// Build the selected-agent Preview from process and runtime observation axes.
#[must_use]
pub fn build_preview_view(
    agent: Option<&Agent>,
    git_info: Option<&GitRepoInfo>,
    observation: Option<&AgentObservation>,
    content_width: usize,
) -> PreviewView {
    build_preview_view_at(agent, git_info, observation, content_width, Instant::now())
}

/// Clock-injected Preview projection used to prove monotonic turn elapsed time.
#[must_use]
pub fn build_preview_view_at(
    agent: Option<&Agent>,
    git_info: Option<&GitRepoInfo>,
    observation: Option<&AgentObservation>,
    content_width: usize,
    now: Instant,
) -> PreviewView {
    let Some(agent) = agent else {
        return PreviewView {
            lines: vec![fit_text_to_width("No agent selected", content_width)],
            todo_header_row: None,
        };
    };
    let repository = git_info
        .and_then(|info| info.origin_shortform.as_deref())
        .unwrap_or("(unknown)");
    let branch = git_info
        .and_then(|info| info.branch.as_deref())
        .unwrap_or("(unknown)");
    let mut lines = vec![
        format!("Name: {}", agent.name),
        format!("Status: {}", project_status(agent.status, observation)),
        format!("Repo: {repository}"),
        format!("Branch: {branch}"),
        format!("Dir: {}", agent.work_dir.display()),
    ];
    append_turn_elapsed(&mut lines, observation, now);
    lines.push(String::new());
    let todo_header_row = lines.len();
    lines.push("Todo:".to_string());
    append_todos(&mut lines, observation);
    append_last_message(&mut lines, observation);
    PreviewView {
        lines: lines
            .into_iter()
            .map(|line| fit_text_to_width(&line, content_width))
            .collect(),
        todo_header_row: Some(todo_header_row),
    }
}

fn append_turn_elapsed(
    lines: &mut Vec<String>,
    observation: Option<&AgentObservation>,
    now: Instant,
) {
    let Some(observation) = observation else {
        return;
    };
    let elapsed_anchor = match &observation.turn {
        FieldState::Supported {
            availability: Availability::Known(Some(turn)),
            ..
        } => Some(turn.elapsed_ms),
        _ => None,
    };
    if let Some(anchor) = elapsed_anchor {
        let local_elapsed = observation.turn_observed_at.map_or(0, |observed| {
            u64::try_from(now.saturating_duration_since(observed).as_millis()).unwrap_or(u64::MAX)
        });
        lines.push(format!(
            "Turn elapsed: {}",
            format_elapsed(anchor.saturating_add(local_elapsed))
        ));
    }
}

fn format_elapsed(elapsed_ms: u64) -> String {
    let seconds = elapsed_ms / 1_000;
    let minutes = seconds / 60;
    let remainder = seconds % 60;
    if minutes == 0 {
        format!("{seconds}s")
    } else {
        format!("{minutes}m {remainder}s")
    }
}

/// The checklist marker for a published task state.
///
/// The in-progress marker is the producer's own state, so the reader is being
/// told what the agent is working on rather than shown a guess. A state JSP/1
/// does not recognize gets its own marker: it is certainly not completed, and
/// calling it pending would be the same guess in a different costume.
const fn todo_marker(state: TodoState) -> &'static str {
    match state {
        TodoState::Completed => "x",
        TodoState::InProgress => ">",
        TodoState::Pending => " ",
        TodoState::Unrecognized => "?",
    }
}

fn append_todos(lines: &mut Vec<String>, observation: Option<&AgentObservation>) {
    let Some(observation) = observation else {
        lines.push("  (telemetry unavailable)".to_string());
        return;
    };
    match &observation.todos {
        FieldState::Unsupported => lines.push("  (unsupported)".to_string()),
        FieldState::Supported {
            availability: Availability::Unknown,
            ..
        } => lines.push("  (unknown)".to_string()),
        FieldState::Supported {
            availability: Availability::Known(todos),
            ..
        } => {
            if todos.items.is_empty() {
                lines.push("  (no tasks)".to_string());
            } else {
                lines.extend(
                    todos.items.iter().map(|todo| {
                        format!("  [{}] {}", todo_marker(todo.state), todo.text.as_str())
                    }),
                );
            }
        }
        FieldState::Supported {
            availability: Availability::Degraded { last_value, .. },
            ..
        } => {
            // An empty stale list must still say so; otherwise the section
            // renders blank and reads as though nothing is known.
            if last_value.items.is_empty() {
                lines.push("  [stale] (no tasks)".to_string());
            } else {
                lines.extend(
                    last_value
                        .items
                        .iter()
                        .map(|todo| format!("  [stale] {}", todo.text.as_str())),
                );
            }
        }
    }
}

fn append_last_message(lines: &mut Vec<String>, observation: Option<&AgentObservation>) {
    let Some(observation) = observation else {
        return;
    };
    let message = match &observation.last_message {
        FieldState::Supported {
            availability: Availability::Known(message),
            ..
        }
        | FieldState::Supported {
            availability:
                Availability::Degraded {
                    last_value: message,
                    ..
                },
            ..
        } => Some(message),
        FieldState::Unsupported
        | FieldState::Supported {
            availability: Availability::Unknown,
            ..
        } => None,
    };
    if let Some(message) = message {
        lines.push(String::new());
        lines.push(format!("Last reply: {}", message.content.as_str()));
    }
}

/// Resolve the accepted status precedence without mutating `AgentStatus`.
///
/// Thin wrapper over [`crate::status_precedence::resolve_status`] so the
/// Preview pane and the Workbench grid share exactly one implementation of
/// "what status is this agent in". The nine-level precedence from issue #522
/// is evaluated top-down; see the shared module for the full rationale.
#[must_use]
pub fn project_status(status: AgentStatus, observation: Option<&AgentObservation>) -> String {
    resolve_status(status, observation).label()
}

/// Resolve the typed precedence level for an agent.
///
/// Exposed so the Workbench projection can read the structured
/// [`ResolvedStatus`] (for bucketing) without re-parsing the label string.
#[must_use]
pub fn project_resolved_status(
    status: AgentStatus,
    observation: Option<&AgentObservation>,
) -> ResolvedStatus {
    resolve_status(status, observation)
}
