//! Multiplexer server-health I/O for the Windows liveness observer.

use std::cell::RefCell;
use std::process::Stdio;

use super::liveness::run_tmux_with_timeout;
use super::server_health::{
    ServerIdentity, ServerLivenessEvidence, ServerLivenessObservation, classify_server_health,
    parse_server_identity_output,
};
use super::{MultiplexerIsolation, MultiplexerPlan, capture_process_identity};

/// Server-identity probe format (issue #540).
///
/// `#{server_instance}` is the stable `-L` namespace token added by
/// upstream psmux#509. It leads the format deliberately: a multiplexer that
/// predates it renders the variable as empty text and still answers
/// successfully, so one format string works against both and a blank leading
/// field is the capability signal. `#{pid}` names whichever of the namespace's
/// per-session servers replied and is retained only as weaker fallback
/// evidence.
const SERVER_IDENTITY_FORMAT: &str = "#{server_instance}|#{pid}|#{version}";

/// Probe the local multiplexer server and classify it against the pinned identity.
pub fn observe_server_liveness(
    plan: &MultiplexerPlan,
    prior: Option<&ServerIdentity>,
    applied_exit_empty: &RefCell<Option<ServerIdentity>>,
) -> ServerLivenessObservation {
    let evidence = capture_server_identity_evidence(plan);
    let observation = classify_observation(prior, &evidence);
    log_server_observation(plan, prior, &evidence, &observation);
    #[cfg(windows)]
    apply_exit_empty_if_new_identity(plan, &observation, applied_exit_empty);
    #[cfg(not(windows))]
    let _ = applied_exit_empty;
    observation
}

pub(super) fn classify_observation(
    prior: Option<&ServerIdentity>,
    evidence: &ServerLivenessEvidence,
) -> ServerLivenessObservation {
    match evidence {
        ServerLivenessEvidence::CommandCompleted { stdout, .. } => {
            let Some(parsed) = parse_server_identity_output(stdout) else {
                return ServerLivenessObservation::Unavailable;
            };
            // The parser only knows the PID the multiplexer printed; the
            // creation discriminator has to come from the operating system.
            let current = match capture_process_identity(parsed.process.pid()) {
                Ok(process) => ServerIdentity::new(
                    crate::domain::ServerProcessIdentity::from_identity(process),
                    parsed.multiplexer,
                ),
                Err(_) => return ServerLivenessObservation::Unavailable,
            };
            classify_resolved_identity(prior, &current)
        }
        _ => super::classify_server_liveness(prior, evidence),
    }
}

/// Classify a freshly resolved server identity against the pinned prior.
///
/// Split out of [`classify_observation`] so the verdict is decided without
/// touching the operating system: everything above this line is probe I/O,
/// everything below is a pure comparison of two identities (issue #664).
pub(super) fn classify_resolved_identity(
    prior: Option<&ServerIdentity>,
    current: &ServerIdentity,
) -> ServerLivenessObservation {
    match classify_server_health(prior, Some(current), true) {
        super::ServerHealth::Healthy => ServerLivenessObservation::Healthy(Some(current.clone())),
        super::ServerHealth::Replaced => match prior {
            Some(prior) if !replaces(prior, current) => {
                ServerLivenessObservation::ConflictingIdentity(current.clone())
            }
            _ => ServerLivenessObservation::Replaced(current.clone()),
        },
        super::ServerHealth::Gone => ServerLivenessObservation::Unavailable,
    }
}

/// Whether `current` can be the process that replaced `prior`.
///
/// A restart necessarily creates the new server after the old one, so the
/// creation discriminator must be strictly newer. Equality is excluded: two
/// servers created in the same tick are unordered, and neither can be shown to
/// have replaced the other.
///
/// When either side lacks a discriminator the ordering is unverifiable rather
/// than contradictory, so this fails open and preserves the pre-#664
/// `Replaced` verdict instead of manufacturing a conflict from weak evidence.
fn replaces(prior: &ServerIdentity, current: &ServerIdentity) -> bool {
    match (prior.process.started_at(), current.process.started_at()) {
        (Some(prior_started), Some(current_started)) => current_started > prior_started,
        _ => true,
    }
}

fn capture_server_identity_evidence(plan: &MultiplexerPlan) -> ServerLivenessEvidence {
    let mut command = plan.command();
    command
        .args(["display-message", "-p", SERVER_IDENTITY_FORMAT])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match run_tmux_with_timeout(&mut command) {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            if output.status.success() {
                ServerLivenessEvidence::command_succeeded(&stdout, &stderr)
            } else {
                ServerLivenessEvidence::command_failed(&stderr)
            }
        }
        Err(()) => ServerLivenessEvidence::spawn_failed(),
    }
}

fn log_server_observation(
    plan: &MultiplexerPlan,
    prior: Option<&ServerIdentity>,
    evidence: &ServerLivenessEvidence,
    observation: &ServerLivenessObservation,
) {
    let namespace = match plan.isolation() {
        MultiplexerIsolation::Namespace(name) => name.clone(),
        MultiplexerIsolation::Socket(path) => path.to_string_lossy().into_owned(),
    };
    let (status, has_stderr) = match evidence {
        ServerLivenessEvidence::CommandCompleted { stderr, .. } => ("ok", !stderr.is_empty()),
        ServerLivenessEvidence::CommandFailed { stderr } => ("nonzero", !stderr.is_empty()),
        ServerLivenessEvidence::SpawnFailed => ("spawn-failed", false),
    };
    tracing::debug!(
        executable = %plan.executable().display(),
        namespace = %namespace,
        command = "display-message",
        status,
        has_stderr,
        prior_pid = prior.map(|id| id.process.pid()),
        observation = ?observation,
        "server liveness probe",
    );
    if let ServerLivenessObservation::ConflictingIdentity(observed) = observation {
        tracing::warn!(
            namespace = %namespace,
            prior_pid = prior.map(|id| id.process.pid()),
            prior_started_at = prior.and_then(|id| id.process.started_at()),
            observed_pid = observed.process.pid(),
            observed_started_at = observed.process.started_at(),
            "server identity probe answered with a process that cannot have replaced the pinned server; making no state change",
        );
    }
}

/// The identity, if any, that exit-empty remediation should be applied to.
///
/// Only identities jefe has accepted as the current server qualify. A
/// conflicting identity has not been accepted, so remediation must not
/// reconfigure whichever server happened to answer (issue #664).
#[cfg(windows)]
pub(super) fn exit_empty_target(observation: &ServerLivenessObservation) -> Option<ServerIdentity> {
    match observation {
        ServerLivenessObservation::Healthy(Some(id)) | ServerLivenessObservation::Replaced(id) => {
            Some(id.clone())
        }
        _ => None,
    }
}

#[cfg(windows)]
fn apply_exit_empty_if_new_identity(
    plan: &MultiplexerPlan,
    observation: &ServerLivenessObservation,
    applied: &RefCell<Option<ServerIdentity>>,
) {
    let current = exit_empty_target(observation);
    if current.is_none() || *applied.borrow() == current {
        return;
    }
    let mut command = plan.command();
    command.args(super::multiplexer_contract::EXIT_EMPTY_REMEDIATION);
    match run_tmux_with_timeout(&mut command) {
        Ok(output) if output.status.success() => *applied.borrow_mut() = current,
        Ok(output) => tracing::warn!(
            namespace = ?plan.isolation(),
            stderr = %String::from_utf8_lossy(&output.stderr).trim(),
            "exit-empty set-option failed; failing open",
        ),
        Err(()) => tracing::warn!(
            namespace = ?plan.isolation(),
            "exit-empty set-option spawn/timeout failed; failing open",
        ),
    }
}
