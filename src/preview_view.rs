//! Pure, iocraft-free selected-agent Preview projection.

use std::time::Instant;

use crate::domain::observation::{
    AgentObservation, Availability, FieldState, NativeActivityState, ObservationHealth, ToolPhase,
    WaitReason,
};
use crate::domain::{Agent, AgentStatus};
use crate::git_info::GitRepoInfo;
use crate::list_viewport::fit_text_to_width;

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
                lines.extend(todos.items.iter().map(|todo| {
                    let marker = if todo.completed { "x" } else { " " };
                    format!("  [{marker}] {}", todo.text.as_str())
                }));
            }
        }
        FieldState::Supported {
            availability: Availability::Degraded { last_value, .. },
            ..
        } => lines.extend(
            last_value
                .items
                .iter()
                .map(|todo| format!("  [stale] {}", todo.text.as_str())),
        ),
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
#[must_use]
pub fn project_status(status: AgentStatus, observation: Option<&AgentObservation>) -> String {
    match status {
        // A confirmed exit is terminal, but the label must not conflate a
        // successful completion with a failure. These match the status labels
        // the rest of the application already renders.
        AgentStatus::Dead => return "Dead".to_string(),
        AgentStatus::Completed => return "Completed".to_string(),
        AgentStatus::Errored => return "Errored".to_string(),
        AgentStatus::ServerLost => return "Disconnected".to_string(),
        AgentStatus::Queued | AgentStatus::Running | AgentStatus::Waiting | AgentStatus::Paused => {
        }
    }
    let Some(observation) = observation else {
        // Without observation the process status is all we know, so report it
        // rather than flattening distinct states into "Running".
        return match status {
            AgentStatus::Queued => "Starting".to_string(),
            AgentStatus::Waiting => "Waiting".to_string(),
            AgentStatus::Paused => "Paused".to_string(),
            _ => "Running — telemetry unsupported".to_string(),
        };
    };
    match observation.health {
        ObservationHealth::Unsupported => return "Running — telemetry unsupported".to_string(),
        ObservationHealth::Connecting => return "Connecting".to_string(),
        ObservationHealth::Stale => return "Stale".to_string(),
        ObservationHealth::Disconnected => return "Disconnected".to_string(),
        ObservationHealth::ProtocolError => return "Protocol error".to_string(),
        ObservationHealth::Live => {}
    }
    live_status(observation)
}

fn live_status(observation: &AgentObservation) -> String {
    if let Some(reason) = known_wait(observation) {
        return format!("Waiting — {}", wait_label(reason));
    }
    if known_terminal_failure(observation) {
        return "Failed".to_string();
    }
    if observation.session_ended {
        return "Ended".to_string();
    }
    if active_turn(observation) || active_work(observation) {
        return "Working".to_string();
    }
    if known_ready(observation) {
        return "Ready".to_string();
    }
    "Unknown".to_string()
}

fn known_wait(observation: &AgentObservation) -> Option<WaitReason> {
    match &observation.wait {
        FieldState::Supported {
            availability: Availability::Known(Some(wait)),
            ..
        } => Some(wait.reason),
        _ => None,
    }
}

const fn wait_label(reason: WaitReason) -> &'static str {
    match reason {
        WaitReason::Permission => "permission",
        WaitReason::Question => "question",
        WaitReason::Elicitation => "elicitation",
        WaitReason::Choice => "choice",
        WaitReason::UserInput => "user input",
        WaitReason::Other => "input",
    }
}

fn known_terminal_failure(observation: &AgentObservation) -> bool {
    matches!(
        observation.terminal,
        FieldState::Supported {
            availability: Availability::Known(Some(_)),
            ..
        }
    )
}

fn active_turn(observation: &AgentObservation) -> bool {
    matches!(
        observation.turn,
        FieldState::Supported {
            availability: Availability::Known(Some(_)),
            ..
        }
    )
}

fn active_work(observation: &AgentObservation) -> bool {
    let activity = matches!(
        observation.activity,
        FieldState::Supported {
            availability: Availability::Known(ref value),
            ..
        } if matches!(value.state, NativeActivityState::Thinking | NativeActivityState::Acting)
    );
    let tool = matches!(
        observation.tool,
        FieldState::Supported {
            availability: Availability::Known(ref value),
            ..
        } if !matches!(value.phase, ToolPhase::Succeeded | ToolPhase::Failed | ToolPhase::Cancelled)
    );
    activity || tool
}

fn known_ready(observation: &AgentObservation) -> bool {
    matches!(
        observation.activity,
        FieldState::Supported {
            availability: Availability::Known(ref value),
            ..
        } if value.state == NativeActivityState::Idle
    ) && matches!(
        observation.wait,
        FieldState::Supported {
            availability: Availability::Known(None),
            ..
        }
    ) && matches!(
        observation.turn,
        FieldState::Supported {
            availability: Availability::Known(None),
            ..
        }
    ) && matches!(
        observation.terminal,
        FieldState::Supported {
            availability: Availability::Known(None),
            ..
        }
    )
}
