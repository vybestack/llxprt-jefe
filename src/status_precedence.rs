//! Shared nine-level agent status precedence resolution.
//!
//! Extracted from `preview_view` (issue #626) so the Preview pane and the
//! Workbench grid share exactly one implementation of "what status is this
//! agent in". Pure: no I/O, no clock, no mutation of inputs.
//!
//! The precedence is evaluated top-down across nine levels. A confirmed
//! process exit (level 1) is terminal and wins over everything. A
//! queued/spawning process (level 2) reports "Starting" only while nothing
//! has been observed from it, because levels 3-9 all describe an alive
//! process and a published observation is proof that it is alive.

use crate::domain::AgentStatus;
use crate::domain::observation::{
    AgentObservation, Availability, FieldState, NativeActivityState, ObservationHealth, ToolPhase,
    WaitReason,
};

/// The resolved precedence level for one agent, independent of any display
/// representation.
///
/// Both the Preview pane ([`crate::preview_view`]) and the Workbench grid
/// ([`crate::workbench_view`]) consume this single resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedStatus {
    /// Level 1: confirmed process death.
    Dead,
    /// Level 1: process completed cleanly.
    Completed,
    /// Level 1: process errored.
    Errored,
    /// Level 1: server/binding lost (process `ServerLost` or observation
    /// health `Disconnected`). Both surface the same way to users.
    Disconnected,
    /// Level 2: queued/spawning, never yet observed.
    Starting,
    /// No observation and the process status is `Waiting` (jefe's own belief,
    /// not authoritative telemetry).
    ProcessWaiting,
    /// No observation and the process status is `Paused`.
    ProcessPaused,
    /// Running with no observation, or observation health `Unsupported`.
    TelemetryUnsupported,
    /// Level 3-4: observation health `Connecting`.
    Connecting,
    /// Level 3-4: observation health `Stale`.
    Stale,
    /// Level 3-4: observation health `ProtocolError`.
    ProtocolError,
    /// Level 5: live observation with an explicit unresolved wait. Carries the
    /// reason so callers can surface it without re-reading the observation.
    Waiting(WaitReason),
    /// Level 6: live observation with a known terminal failure.
    Failed,
    /// Level 7: live observation reporting the session has ended.
    Ended,
    /// Level 8: live observation with an active turn or active work.
    Working,
    /// Level 9: live observation that is idle with nothing pending.
    Ready,
    /// Live observation that does not match any of levels 5-9.
    Unknown,
}

impl ResolvedStatus {
    /// The human-readable label, matching the strings the Preview pane has
    /// always rendered so extracting the logic changes no displayed text.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Dead => "Dead",
            Self::Completed => "Completed",
            Self::Errored => "Errored",
            Self::Disconnected => "Disconnected",
            Self::Starting => "Starting",
            Self::ProcessWaiting => "Waiting",
            Self::ProcessPaused => "Paused",
            Self::TelemetryUnsupported => "Running — telemetry unsupported",
            Self::Connecting => "Connecting",
            Self::Stale => "Stale",
            Self::ProtocolError => "Protocol error",
            Self::Waiting(reason) => return format!("Waiting — {}", wait_reason_label(reason)),
            Self::Failed => "Failed",
            Self::Ended => "Ended",
            Self::Working => "Working",
            Self::Ready => "Ready",
            Self::Unknown => "Unknown",
        }
        .to_string()
    }
}

/// Resolve the accepted status precedence without mutating `AgentStatus`.
///
/// This is the single source of truth shared by the Preview and Workbench
/// projections. The nine-level precedence from issue #522 is evaluated
/// top-down.
#[must_use]
pub fn resolve_status(
    status: AgentStatus,
    observation: Option<&AgentObservation>,
) -> ResolvedStatus {
    // Level 1: confirmed process exit is terminal.
    match status {
        AgentStatus::Dead => return ResolvedStatus::Dead,
        AgentStatus::Completed => return ResolvedStatus::Completed,
        AgentStatus::Errored => return ResolvedStatus::Errored,
        AgentStatus::ServerLost => return ResolvedStatus::Disconnected,
        AgentStatus::Queued | AgentStatus::Running | AgentStatus::Waiting | AgentStatus::Paused => {
        }
    }
    // Level 2: a queued/spawning process is Starting until it proves otherwise.
    //
    // Levels 3-9 describe an alive process, and an observation is that proof.
    if status == AgentStatus::Queued && observation.is_none() {
        return ResolvedStatus::Starting;
    }
    let Some(observation) = observation else {
        // Without observation the process status is all we know.
        return match status {
            AgentStatus::Waiting => ResolvedStatus::ProcessWaiting,
            AgentStatus::Paused => ResolvedStatus::ProcessPaused,
            _ => ResolvedStatus::TelemetryUnsupported,
        };
    };
    // Levels 3-4: observation health for alive processes.
    match observation.health {
        ObservationHealth::Unsupported => return ResolvedStatus::TelemetryUnsupported,
        ObservationHealth::Connecting => return ResolvedStatus::Connecting,
        ObservationHealth::Stale => return ResolvedStatus::Stale,
        ObservationHealth::Disconnected => return ResolvedStatus::Disconnected,
        ObservationHealth::ProtocolError => return ResolvedStatus::ProtocolError,
        ObservationHealth::Live => {}
    }
    // Levels 5-9: live observation status.
    resolve_live(observation)
}

/// Resolve the live-observation precedence (levels 5-9).
fn resolve_live(observation: &AgentObservation) -> ResolvedStatus {
    if let Some(reason) = known_wait(observation) {
        return ResolvedStatus::Waiting(reason);
    }
    if known_terminal_failure(observation) {
        return ResolvedStatus::Failed;
    }
    if observation.session_ended {
        return ResolvedStatus::Ended;
    }
    if active_turn(observation) || active_work(observation) {
        return ResolvedStatus::Working;
    }
    if known_ready(observation) {
        return ResolvedStatus::Ready;
    }
    ResolvedStatus::Unknown
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

/// The display label for a wait reason (differs from `WaitReason::as_str`
/// for `UserInput` and `Other`, so it is centralized here).
#[must_use]
pub const fn wait_reason_label(reason: WaitReason) -> &'static str {
    match reason {
        WaitReason::Permission => "permission",
        WaitReason::Question => "question",
        WaitReason::Elicitation => "elicitation",
        WaitReason::Choice => "choice",
        WaitReason::UserInput => "user input",
        WaitReason::Other => "input",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_reproduces_preview_strings() {
        assert_eq!(ResolvedStatus::Dead.label(), "Dead");
        assert_eq!(ResolvedStatus::Starting.label(), "Starting");
        assert_eq!(
            ResolvedStatus::TelemetryUnsupported.label(),
            "Running — telemetry unsupported"
        );
        assert_eq!(
            ResolvedStatus::Waiting(WaitReason::Permission).label(),
            "Waiting — permission"
        );
        assert_eq!(
            ResolvedStatus::Waiting(WaitReason::UserInput).label(),
            "Waiting — user input"
        );
        assert_eq!(ResolvedStatus::Unknown.label(), "Unknown");
    }
}
