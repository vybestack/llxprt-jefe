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

fn classify_observation(
    prior: Option<&ServerIdentity>,
    evidence: &ServerLivenessEvidence,
) -> ServerLivenessObservation {
    match evidence {
        ServerLivenessEvidence::CommandCompleted { stdout, .. } => {
            let Some(parsed) = parse_server_identity_output(stdout) else {
                return ServerLivenessObservation::Unavailable;
            };
            let current = match capture_process_identity(parsed.process.pid()) {
                Ok(process) => ServerIdentity::new(
                    crate::domain::ServerProcessIdentity::from_identity(process),
                    parsed.multiplexer,
                ),
                Err(_) => return ServerLivenessObservation::Unavailable,
            };
            match classify_server_health(prior, Some(&current), true) {
                super::ServerHealth::Healthy => ServerLivenessObservation::Healthy(Some(current)),
                super::ServerHealth::Replaced => ServerLivenessObservation::Replaced(current),
                super::ServerHealth::Gone => ServerLivenessObservation::Unavailable,
            }
        }
        _ => super::classify_server_liveness(prior, evidence),
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
}

#[cfg(windows)]
fn apply_exit_empty_if_new_identity(
    plan: &MultiplexerPlan,
    observation: &ServerLivenessObservation,
    applied: &RefCell<Option<ServerIdentity>>,
) {
    let current = match observation {
        ServerLivenessObservation::Healthy(Some(id)) | ServerLivenessObservation::Replaced(id) => {
            Some(id.clone())
        }
        _ => None,
    };
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
