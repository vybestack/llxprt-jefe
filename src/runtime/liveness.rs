//! Liveness checking for tmux sessions.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @pseudocode component-002 lines 33-35

use std::collections::HashSet;
use std::hash::BuildHasher;
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::domain::liveness_observation::{Observed, ProbeBoundary};
use crate::domain::{AgentId, RemoteRepositorySettings};
use crate::runtime::commands::{
    remote_tmux_command, run_remote_ssh, shell_escape_single, tmux_command,
};
use crate::runtime::manager::LivenessCheck;

/// Timeout for local tmux subprocess invocations in the batch liveness path.
/// Matches the `TMUX_TIMEOUT` used by the harness driver so a hung tmux server
/// cannot stall the background liveness thread indefinitely (issue #287).
const LOCAL_TMUX_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

/// Check if a process with the given PID is alive.
///
/// This **complements**, not replaces, [`check_session_alive`]. When the jefe
/// multiplexer server has died but the worker is still running,
/// `check_session_alive` reports false while `pid_alive` reports true — letting
/// jefe recognize the worker is recoverable rather than marking the agent Dead.
///
/// Uses the typed process observation service on every platform. Only a
/// confirmed exit returns false; inaccessible and failed probes fail open.
/// Local-only: remote agents stay on the tmux/SSH-only path.
#[must_use]
pub fn pid_alive(pid: u32) -> bool {
    let liveness = super::process::pid_liveness(pid);
    if liveness == super::process::ProcessLiveness::ProbeFailure {
        tracing::warn!(pid, "PID liveness probe failed; assuming worker alive");
    }
    super::process::process_liveness_indicates_alive(liveness)
}

/// Result of probing one persistent multiplexer session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLiveness {
    /// The session exists and contains a non-dead pane.
    Alive,
    /// The session is absent or all of its panes have exited.
    Missing,
    /// The multiplexer command could not be started or queried.
    Unavailable,
}

/// Probe whether a local session exists and contains a non-dead pane.
#[must_use]
pub fn session_liveness(session_name: &str) -> SessionLiveness {
    let Ok(mut command) = tmux_command() else {
        return SessionLiveness::Unavailable;
    };
    let Ok(output) = command.args(["has-session", "-t", session_name]).output() else {
        return SessionLiveness::Unavailable;
    };
    if !output.status.success() {
        return SessionLiveness::Missing;
    }

    let Ok(mut command) = tmux_command() else {
        return SessionLiveness::Unavailable;
    };
    let Ok(output) = command
        .args(["list-panes", "-t", session_name, "-F", "#{pane_dead}"])
        .output()
    else {
        return SessionLiveness::Unavailable;
    };
    if !output.status.success() {
        return SessionLiveness::Missing;
    }
    parse_dead_pane_flags(&String::from_utf8_lossy(&output.stdout))
}

pub(super) fn parse_dead_pane_flags(output: &str) -> SessionLiveness {
    let mut saw_dead = false;
    for flag in output
        .lines()
        .map(str::trim)
        .filter(|flag| !flag.is_empty())
    {
        if flag == "0" || flag.eq_ignore_ascii_case("false") {
            return SessionLiveness::Alive;
        }
        if flag == "1" || flag.eq_ignore_ascii_case("true") {
            saw_dead = true;
        } else {
            return SessionLiveness::Unavailable;
        }
    }
    if saw_dead {
        SessionLiveness::Missing
    } else {
        SessionLiveness::Unavailable
    }
}

/// Check if a tmux session exists and has at least one non-dead pane.
///
/// @pseudocode component-002 lines 33-35
#[must_use]
pub fn check_session_alive(session_name: &str) -> bool {
    session_liveness(session_name) == SessionLiveness::Alive
}

/// Check if a remote tmux session exists and has at least one non-dead pane.
#[must_use]
pub fn check_remote_session_alive(remote: &RemoteRepositorySettings, session_name: &str) -> bool {
    let command = remote_tmux_command(
        remote,
        &format!(
            "tmux has-session -t {} && tmux list-panes -t {} -F '#{{pane_dead}}'",
            shell_escape_single(session_name),
            shell_escape_single(session_name)
        ),
    );

    let output = run_remote_ssh(remote, &command);
    let Ok(out) = output else {
        return false;
    };
    if !out.status.success() {
        return false;
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        let dead_flag = line.trim();
        if dead_flag.is_empty() {
            continue;
        }
        if dead_flag == "0" || dead_flag.eq_ignore_ascii_case("false") {
            return true;
        }
    }

    false
}

/// Parse raw `tmux list-sessions -F '#{session_name}'` output into a set of
/// session names.
///
/// Each non-empty line is a session name. Lines that are empty or consist
/// only of whitespace are skipped (tmux emits trailing newlines).
///
/// This is a pure function — it does not invoke tmux — so it can be unit-tested
/// without a tmux server.
#[must_use]
pub fn parse_alive_sessions(raw_output: &str) -> HashSet<String> {
    raw_output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect()
}

/// Parse raw `tmux list-panes -a -F '#{session_name}:#{window_index}:#{pane_dead}'` output into
/// a set of session names that have at least one non-dead pane.
///
/// Each line has the form `session_name:0` (alive pane) or `session_name:1`
/// (dead pane). A session is alive if it has at least one non-dead pane.
///
/// This is a pure function — it does not invoke tmux — so it can be unit-tested
/// without a tmux server.
///
/// The original agent lives in window index zero. Other windows, including
/// the temporary issue-222 shell, must not keep a dead agent classified alive.
#[must_use]
pub fn parse_pane_alive(raw_output: &str) -> HashSet<String> {
    let mut alive_sessions: HashSet<String> = HashSet::new();
    for line in raw_output.lines() {
        let line = line.trim();
        let Some((target, pane_dead)) = line.rsplit_once(':') else {
            continue;
        };
        let Some((session, window_index)) = target.rsplit_once(':') else {
            continue;
        };
        if !session.trim().is_empty() && window_index.trim() == "0" && pane_dead.trim() == "0" {
            alive_sessions.insert(session.trim().to_owned());
        }
    }
    alive_sessions
}

/// Reconcile which target agents are dead given a set of existing session names
/// and a set of sessions that have at least one non-dead pane.
///
/// A session is alive if it exists in `existing_sessions` AND appears in
/// `alive_pane_sessions`. A target is dead if its session_name is not alive.
/// Remote targets are excluded (the caller should filter them before calling).
///
/// This is a pure function — it does not invoke tmux — so it can be unit-tested
/// without a tmux server.
#[must_use]
pub fn reconcile_dead_agents<S: BuildHasher>(
    targets: &[LivenessCheck],
    existing_sessions: &HashSet<String, S>,
    alive_pane_sessions: &HashSet<String, S>,
) -> Vec<AgentId> {
    targets
        .iter()
        .filter(|t| {
            t.remote.is_none()
                && (!existing_sessions.contains(&t.session_name)
                    || !alive_pane_sessions.contains(&t.session_name))
        })
        .map(|t| t.agent_id.clone())
        .collect()
}

/// Liveness identity triple returned by [`reconcile_dead_agents_with_identity`].
///
/// Carries enough information for the caller to verify the result is not stale
/// (issue #301 Phase 4): the agent id, the session name that was checked, and
/// the lifecycle generation at snapshot time.
///
/// **PII note:** `binding_session_name` may encode user or project
/// identifiers (tmux session names often include usernames or project
/// names). Redact before including in logs or persisted diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LivenessIdentity {
    pub agent_id: AgentId,
    pub binding_session_name: Option<String>,
    pub lifecycle_generation: u64,
    /// What became of the agent worker when this pane died (issue #543).
    pub worker: WorkerDisposition,
}

/// The fate of an agent worker relative to the pane that launched it.
///
/// Pane death and worker death are separate events. They coincide only where
/// the agent runs as the pane's direct command; where the pane leader is a
/// session host, the worker can outlive the pane and become an orphan. Naming
/// the two cases apart is what stops a dead pane from being reported as a dead
/// agent (issue #543).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkerDisposition {
    /// No recorded worker anchor survived the pane: the agent really is gone.
    GoneWithPane,
    /// A recorded worker anchor is still alive and still matches its creation
    /// token. The pane died; the agent did not. Deciding what to *do* about an
    /// unowned live worker belongs to the ownership model (issue #542); this
    /// pass only refuses to call it death.
    SurvivedPane,
    /// No worker anchors were recorded, so the pane's death says nothing about
    /// the worker either way.
    Unknown,
}

/// Classify a worker's fate from freshly probed anchor evidence.
///
/// Only a confirmed-alive anchor that still matches its recorded creation token
/// counts as survival, so a recycled PID can never make a dead agent look
/// alive.
#[must_use]
pub fn classify_worker_disposition(
    anchors: &[super::orphan::ObservedDescendant],
) -> WorkerDisposition {
    if anchors.is_empty() {
        return WorkerDisposition::Unknown;
    }
    if anchors
        .iter()
        .any(|anchor| matches!(anchor.liveness, super::process::ProcessLiveness::Alive))
    {
        return WorkerDisposition::SurvivedPane;
    }
    WorkerDisposition::GoneWithPane
}

/// Reconcile dead agents and return identity triples (issue #301 Phase 4).
///
/// Like [`reconcile_dead_agents`] but returns [`LivenessIdentity`] so the
/// caller can verify the agent's current binding session name and lifecycle
/// generation still match before marking the agent dead.
///
/// A session is dead if it does not appear in `existing_sessions` (the
/// session is completely gone) OR if it exists but has no alive panes (not
/// in `alive_pane_sessions`). Both checks are necessary because
/// `existing_sessions` and `alive_pane_sessions` come from independent tmux
/// queries (`list-sessions` and `list-panes -a` respectively) and are not
/// guaranteed to have a subset relationship — a session could be listed by
/// `list-panes -a` but not yet visible to `list-sessions` (or vice versa)
/// during a concurrent session create/destroy window.
#[must_use]
pub fn reconcile_dead_agents_with_identity<S: BuildHasher>(
    targets: &[LivenessCheck],
    existing_sessions: &HashSet<String, S>,
    alive_pane_sessions: &HashSet<String, S>,
) -> Vec<LivenessIdentity> {
    targets
        .iter()
        .filter(|t| {
            t.remote.is_none()
                && (!existing_sessions.contains(&t.session_name)
                    || !alive_pane_sessions.contains(&t.session_name))
        })
        .map(|t| LivenessIdentity {
            agent_id: t.agent_id.clone(),
            binding_session_name: t.binding_session_name.clone(),
            lifecycle_generation: t.lifecycle_generation,
            // Probe the recorded anchors so a pane death is not reported as a
            // worker death without evidence (issue #543).
            worker: classify_worker_disposition(&probe_worker_anchors(&t.worker_identities)),
        })
        .collect()
}

/// Freshly probe each recorded worker anchor, rejecting PID reuse.
#[must_use]
fn probe_worker_anchors(
    anchors: &[crate::domain::WorkerProcessIdentity],
) -> Vec<super::orphan::ObservedDescendant> {
    anchors
        .iter()
        .map(|anchor| {
            if super::orphan::descendant_still_matches_anchor(*anchor) {
                super::orphan::ObservedDescendant::alive(*anchor)
            } else {
                super::orphan::ObservedDescendant::dead(*anchor)
            }
        })
        .collect()
}

/// Query the tmux server once for all alive sessions, returning the set of
/// session names that exist AND have at least one non-dead pane.
///
/// This uses exactly **two** tmux subprocess invocations regardless of the
/// number of agents, replacing the previous approach of 2 subprocesses per
/// running agent (issue #287).
///
/// Returns `None` if the tmux server is unavailable or the command fails, so
/// callers can skip reconciliation instead of falsely marking all agents dead
/// (issue #287 review: infrastructure failure must not masquerade as dead
/// sessions).
#[must_use]
pub fn alive_session_set() -> Option<HashSet<String>> {
    let existing = list_all_sessions()?;
    let alive_panes = list_alive_pane_sessions()?;
    Some(existing.intersection(&alive_panes).cloned().collect())
}

/// Batch liveness check: query the tmux server once (two subprocesses total)
/// and reconcile against the given local targets, returning the agent IDs
/// whose sessions are dead or missing.
///
/// Remote targets are excluded automatically. This is the single-call API
/// for callers that want dead agent IDs without managing the intermediate sets.
///
/// Yields [`Observed::Unknown`] when the multiplexer could not answer, so a
/// caller can tell "no agents are dead" apart from "we could not find out".
/// Collapsing those two is the fail-open defect issue #541 exists to remove.
#[must_use]
pub fn batch_liveness_check(targets: &[LivenessCheck]) -> Observed<Vec<AgentId>> {
    batch_liveness_check_with_identity(targets)
        .map(|ids| ids.into_iter().map(|id| id.agent_id).collect())
}

/// Batch liveness check returning identity triples (issue #301 Phase 4).
///
/// Like [`batch_liveness_check`] but returns [`LivenessIdentity`] so the
/// caller can verify the agent's current binding session name and lifecycle
/// generation still match before applying the dead status.
#[must_use]
pub fn batch_liveness_check_with_identity(
    targets: &[LivenessCheck],
) -> Observed<Vec<LivenessIdentity>> {
    let existing = list_all_sessions().map_or_else(
        || {
            Observed::unknown(
                ProbeBoundary::SessionList,
                "the multiplexer did not answer list-sessions",
            )
        },
        Observed::Known,
    );
    let alive_panes = list_alive_pane_sessions().map_or_else(
        || {
            Observed::unknown(
                ProbeBoundary::PaneList,
                "the multiplexer did not answer list-panes",
            )
        },
        Observed::Known,
    );
    reconcile_observed(targets, existing, alive_panes)
}

/// Reconcile targets against probe results that may not have answered.
///
/// Pure, so each boundary can be driven to failure in a test without touching
/// the environment. If either probe is unknown the whole reconciliation is
/// unknown: a session set missing because `list-sessions` failed is
/// indistinguishable from one missing because the sessions ended, and guessing
/// between those is exactly how #527 marked twenty live panes stopped.
#[must_use]
pub fn reconcile_observed(
    targets: &[LivenessCheck],
    existing: Observed<HashSet<String>>,
    alive_panes: Observed<HashSet<String>>,
) -> Observed<Vec<LivenessIdentity>> {
    let existing = match existing {
        Observed::Known(sessions) => sessions,
        Observed::Unknown(reason) => {
            tracing::warn!(%reason, "liveness held: the session list is unknown");
            return Observed::Unknown(reason);
        }
    };
    let alive_panes = match alive_panes {
        Observed::Known(panes) => panes,
        Observed::Unknown(reason) => {
            tracing::warn!(%reason, "liveness held: the pane list is unknown");
            return Observed::Unknown(reason);
        }
    };
    Observed::Known(reconcile_dead_agents_with_identity(
        targets,
        &existing,
        &alive_panes,
    ))
}

/// Query the tmux server for all session names (one subprocess).
///
/// Returns `None` when the tmux server is unavailable or the command fails,
/// so the caller can distinguish infrastructure failure from an empty session
/// set (issue #287 review: silent empty-set returns caused all agents to be
/// falsely reported dead when tmux was unavailable).
#[must_use]
fn list_all_sessions() -> Option<HashSet<String>> {
    let mut command = match tmux_command() {
        Ok(cmd) => cmd,
        Err(e) => {
            tracing::warn!(error = %e, "list_all_sessions: tmux_command failed");
            return None;
        }
    };
    let output = run_tmux_with_timeout(command.args(["list-sessions", "-F", "#{session_name}"]));
    match output {
        Ok(out) if out.status.success() => {
            Some(parse_alive_sessions(&String::from_utf8_lossy(&out.stdout)))
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            tracing::warn!(
                status = %out.status,
                stderr = %stderr.trim(),
                "list_all_sessions: tmux list-sessions failed"
            );
            None
        }
        Err(()) => {
            tracing::warn!("list_all_sessions: tmux list-sessions timed out or spawn failed");
            None
        }
    }
}

/// Query the tmux server for all sessions that have at least one non-dead pane
/// (one subprocess).
///
/// Returns `None` on infrastructure failure, so the caller can skip
/// reconciliation rather than falsely marking all agents dead (issue #287
/// review).
///
/// Uses `tmux list-panes -a` (all sessions) with a format that includes the
/// session name and pane-dead flag, so a single subprocess covers every
/// session.
#[must_use]
fn list_alive_pane_sessions() -> Option<HashSet<String>> {
    let mut command = match tmux_command() {
        Ok(cmd) => cmd,
        Err(e) => {
            tracing::warn!(error = %e, "list_alive_pane_sessions: tmux_command failed");
            return None;
        }
    };
    let output = run_tmux_with_timeout(command.args([
        "list-panes",
        "-a",
        "-F",
        "#{session_name}:#{window_index}:#{pane_dead}",
    ]));
    match output {
        Ok(out) if out.status.success() => {
            Some(parse_pane_alive(&String::from_utf8_lossy(&out.stdout)))
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            tracing::warn!(
                status = %out.status,
                stderr = %stderr.trim(),
                "list_alive_pane_sessions: tmux list-panes failed"
            );
            None
        }
        Err(()) => {
            tracing::warn!("list_alive_pane_sessions: tmux list-panes timed out or spawn failed");
            None
        }
    }
}

/// Run a tmux subprocess with a bounded timeout, killing it if it exceeds the
/// deadline. This prevents a hung tmux server from stalling the background
/// liveness thread indefinitely (issue #287 review).
pub fn run_tmux_with_timeout(
    command: &mut std::process::Command,
) -> Result<std::process::Output, ()> {
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    let child = command.spawn().map_err(|_| ())?;
    let deadline = Instant::now() + LOCAL_TMUX_COMMAND_TIMEOUT;
    run_child_with_timeout(child, deadline)
}

/// Testable inner: run a child to completion with a bounded deadline, killing
/// it on timeout. Separated from [`run_tmux_with_timeout`] so the timeout
/// behavior can be unit-tested with a plain `sleep` subprocess instead of a
/// real tmux invocation (issue #287 review: kill path must be verified).
pub(super) fn run_child_with_timeout(
    mut child: std::process::Child,
    deadline: Instant,
) -> Result<std::process::Output, ()> {
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().map_err(|_| ()),
            Ok(None) => {
                if Instant::now() >= deadline {
                    if let Err(e) = child.kill() {
                        tracing::warn!(error = %e, "failed to kill child on timeout");
                    }
                    if let Err(e) = child.wait() {
                        tracing::warn!(error = %e, "failed to reap child after kill");
                    }
                    return Err(());
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => {
                if let Err(kill_err) = child.kill() {
                    tracing::warn!(error = %kill_err, wait_error = %e, "failed to kill child after try_wait error");
                }
                if let Err(wait_err) = child.wait() {
                    tracing::warn!(error = %wait_err, wait_error = %e, "failed to reap child after try_wait error");
                }
                return Err(());
            }
        }
    }
}

/// List all jefe-managed tmux sessions.
#[allow(dead_code)]
pub fn list_jefe_sessions() -> Vec<String> {
    let Ok(mut command) = tmux_command() else {
        return Vec::new();
    };
    let output = command
        .args(["list-sessions", "-F", "#{session_name}"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .filter(|line| line.starts_with("jefe-"))
                .map(String::from)
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Kill a tmux session.
#[allow(dead_code)]
pub fn kill_session(session_name: &str) -> bool {
    let Ok(mut command) = tmux_command() else {
        return false;
    };
    let output = command.args(["kill-session", "-t", session_name]).output();

    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}
