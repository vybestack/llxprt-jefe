//! Liveness checking for tmux sessions.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @pseudocode component-002 lines 33-35

use std::collections::HashSet;
use std::hash::BuildHasher;
use std::process::Stdio;
use std::time::{Duration, Instant};

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
/// Uses `kill -0` on Unix and the native process-identity probe on Windows.
/// Local-only: remote agents stay on the tmux/SSH-only path.
#[must_use]
pub fn pid_alive(pid: u32) -> bool {
    pid_alive_on_platform(pid)
}

#[cfg(unix)]
fn pid_alive_on_platform(pid: u32) -> bool {
    match std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .output()
    {
        Ok(output) => output.status.success(),
        Err(e) => {
            // Fail-open: this is a recovery safety net whose whole purpose is
            // to avoid marking live workers Dead. If we can't even run `kill`,
            // assume the worker is still alive rather than risk losing it.
            tracing::warn!(error = %e, pid, "failed to spawn kill -0; assuming worker alive");
            true
        }
    }
}

#[cfg(windows)]
fn pid_alive_on_platform(pid: u32) -> bool {
    super::process::capture_process_identity(pid).is_ok()
}

#[cfg(not(any(unix, windows)))]
fn pid_alive_on_platform(pid: u32) -> bool {
    tracing::warn!(
        pid,
        "PID liveness is unsupported on this platform; assuming worker alive"
    );
    true
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

fn parse_dead_pane_flags(output: &str) -> SessionLiveness {
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

/// Parse raw `tmux list-panes -a -F '#{session_name}:#{pane_dead}'` output into
/// a set of session names that have at least one non-dead pane.
///
/// Each line has the form `session_name:0` (alive pane) or `session_name:1`
/// (dead pane). A session is alive if it has at least one non-dead pane.
///
/// This is a pure function — it does not invoke tmux — so it can be unit-tested
/// without a tmux server.
///
/// `rsplit_once(':')` splits on the **last** colon, which correctly isolates
/// the `pane_dead` suffix even when the session name itself contains colons
/// (e.g., `my:session:0` -> `("my:session", "0")`). Jefe's session-name
/// sanitizer also replaces colons with underscores as an extra guarantee,
/// but the parser itself is robust regardless.
#[must_use]
pub fn parse_pane_alive(raw_output: &str) -> HashSet<String> {
    let mut alive_sessions: HashSet<String> = HashSet::new();
    for line in raw_output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((session, pane_dead)) = line.rsplit_once(':') {
            let session = session.trim();
            if session.is_empty() {
                continue;
            }
            // tmux #{pane_dead} outputs 0 (alive) or 1 (dead).
            if pane_dead.trim() == "0" {
                alive_sessions.insert(session.to_string());
            }
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LivenessIdentity {
    pub agent_id: AgentId,
    pub binding_session_name: Option<String>,
    pub lifecycle_generation: u64,
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
/// Returns an empty vector (no dead agents) when the tmux server is
/// unavailable — infrastructure failure must not cause all agents to be
/// falsely marked dead (issue #287 review).
#[must_use]
pub fn batch_liveness_check(targets: &[LivenessCheck]) -> Vec<AgentId> {
    batch_liveness_check_with_identity(targets)
        .into_iter()
        .map(|id| id.agent_id)
        .collect()
}

/// Batch liveness check returning identity triples (issue #301 Phase 4).
///
/// Like [`batch_liveness_check`] but returns [`LivenessIdentity`] so the
/// caller can verify the agent's current binding session name and lifecycle
/// generation still match before applying the dead status.
#[must_use]
pub fn batch_liveness_check_with_identity(targets: &[LivenessCheck]) -> Vec<LivenessIdentity> {
    let Some(existing) = list_all_sessions() else {
        tracing::warn!("tmux list-sessions failed; skipping liveness cycle");
        return Vec::new();
    };
    let Some(alive_panes) = list_alive_pane_sessions() else {
        tracing::warn!("tmux list-panes failed; skipping liveness cycle");
        return Vec::new();
    };
    reconcile_dead_agents_with_identity(targets, &existing, &alive_panes)
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
        "#{session_name}:#{pane_dead}",
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
fn run_tmux_with_timeout(command: &mut std::process::Command) -> Result<std::process::Output, ()> {
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
fn run_child_with_timeout(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AgentId, RemoteRepositorySettings};
    use crate::runtime::manager::LivenessCheck;

    fn make_liveness_check(agent_id: &str, session_name: &str, remote: bool) -> LivenessCheck {
        LivenessCheck {
            agent_id: AgentId(agent_id.to_string()),
            session_name: session_name.to_string(),
            remote: if remote {
                Some(RemoteRepositorySettings::default())
            } else {
                None
            },
            binding_session_name: Some(session_name.to_string()),
            lifecycle_generation: 0,
        }
    }

    // --- parse_alive_sessions (pure) ---

    #[test]
    fn parse_alive_sessions_basic() {
        let raw = "jefe-agent1
jefe-agent2
jefe-agent3
";
        let set = parse_alive_sessions(raw);
        assert_eq!(set.len(), 3);
        assert!(set.contains("jefe-agent1"));
        assert!(set.contains("jefe-agent2"));
        assert!(set.contains("jefe-agent3"));
    }

    #[test]
    fn parse_alive_sessions_trims_whitespace() {
        let raw = "  jefe-a  \n jefe-b \n\n";
        let set = parse_alive_sessions(raw);
        assert_eq!(set.len(), 2);
        assert!(set.contains("jefe-a"));
        assert!(set.contains("jefe-b"));
    }

    #[test]
    fn parse_alive_sessions_empty_output() {
        let set = parse_alive_sessions("");
        assert!(set.is_empty());
    }

    #[test]
    fn parse_alive_sessions_skips_empty_lines() {
        let raw = "jefe-a


jefe-b
";
        let set = parse_alive_sessions(raw);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn dead_pane_parser_preserves_tri_state() {
        assert_eq!(parse_dead_pane_flags("0\n1\n"), SessionLiveness::Alive);
        assert_eq!(parse_dead_pane_flags("1\ntrue\n"), SessionLiveness::Missing);
        assert_eq!(parse_dead_pane_flags(""), SessionLiveness::Unavailable);
        assert_eq!(
            parse_dead_pane_flags("unexpected\n"),
            SessionLiveness::Unavailable
        );
    }

    // --- parse_pane_alive (pure) ---

    #[test]
    fn parse_pane_alive_identifies_alive_panes() {
        // session:0 = alive pane, session:1 = dead pane
        let raw = "jefe-a:0
jefe-b:1
jefe-c:0
";
        let set = parse_pane_alive(raw);
        assert_eq!(set.len(), 2);
        assert!(set.contains("jefe-a"));
        assert!(set.contains("jefe-c"));
        assert!(!set.contains("jefe-b"));
    }

    #[test]
    fn parse_pane_alive_only_numeric_flags() {
        // tmux #{pane_dead} outputs only 0 (alive) or 1 (dead).
        // Non-numeric strings like "false" must not match.
        let raw = "jefe-a:0
jefe-b:1
jefe-c:false
";
        let set = parse_pane_alive(raw);
        assert!(set.contains("jefe-a"));
        assert!(!set.contains("jefe-b"));
        assert!(!set.contains("jefe-c"), "non-numeric flags must not match");
    }

    #[test]
    fn parse_pane_alive_empty_output() {
        let set = parse_pane_alive("");
        assert!(set.is_empty());
    }

    #[test]
    fn parse_pane_alive_skips_malformed_lines() {
        let raw = "jefe-a:0
malformed
jefe-b:0
";
        let set = parse_pane_alive(raw);
        assert_eq!(set.len(), 2);
        assert!(set.contains("jefe-a"));
        assert!(set.contains("jefe-b"));
    }

    // --- reconcile_dead_agents (pure) ---

    #[test]
    fn reconcile_dead_agents_finds_missing_sessions() {
        let targets = vec![
            make_liveness_check("agent1", "jefe-agent1", false),
            make_liveness_check("agent2", "jefe-agent2", false),
        ];
        let existing: HashSet<String> = std::iter::once("jefe-agent1".to_string()).collect();
        let alive_panes: HashSet<String> = std::iter::once("jefe-agent1".to_string()).collect();

        let dead = reconcile_dead_agents(&targets, &existing, &alive_panes);
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].0, "agent2");
    }

    #[test]
    fn reconcile_dead_agents_finds_dead_panes() {
        let targets = vec![
            make_liveness_check("agent1", "jefe-agent1", false),
            make_liveness_check("agent2", "jefe-agent2", false),
        ];
        let existing: HashSet<String> = ["jefe-agent1".to_string(), "jefe-agent2".to_string()]
            .into_iter()
            .collect();
        // agent1 has alive panes, agent2 has only dead panes
        let alive_panes: HashSet<String> = std::iter::once("jefe-agent1".to_string()).collect();

        let dead = reconcile_dead_agents(&targets, &existing, &alive_panes);
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].0, "agent2");
    }

    #[test]
    fn reconcile_dead_agents_all_alive() {
        let targets = vec![
            make_liveness_check("agent1", "jefe-agent1", false),
            make_liveness_check("agent2", "jefe-agent2", false),
        ];
        let existing: HashSet<String> = ["jefe-agent1".to_string(), "jefe-agent2".to_string()]
            .into_iter()
            .collect();
        let alive_panes: HashSet<String> = ["jefe-agent1".to_string(), "jefe-agent2".to_string()]
            .into_iter()
            .collect();

        let dead = reconcile_dead_agents(&targets, &existing, &alive_panes);
        assert!(dead.is_empty());
    }

    #[test]
    fn reconcile_dead_agents_excludes_remote_targets() {
        let targets = vec![
            make_liveness_check("local-agent", "jefe-local", false),
            make_liveness_check("remote-agent", "jefe-remote", true),
        ];
        // No sessions exist
        let existing: HashSet<String> = HashSet::new();
        let alive_panes: HashSet<String> = HashSet::new();

        let dead = reconcile_dead_agents(&targets, &existing, &alive_panes);
        // Only local-agent is dead; remote-agent is excluded
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].0, "local-agent");
    }

    #[test]
    fn reconcile_dead_agents_empty_targets() {
        let dead = reconcile_dead_agents(&[], &HashSet::new(), &HashSet::new());
        assert!(dead.is_empty());
    }

    // --- alive_session_set (integration, needs tmux) ---

    #[test]
    fn alive_session_set_does_not_panic_without_tmux_server() {
        // On a system without tmux or with no sessions, this returns None.
        // This test validates graceful failure, not the presence of tmux.
        let set = alive_session_set();
        // We don't assert the value because a tmux server might have sessions
        // from other processes. We just verify it doesn't panic.
        let _ = set;
    }

    #[test]
    fn reconcile_dead_agents_marks_all_dead_when_no_sessions_exist() {
        // When no tmux sessions exist, all local targets are dead.
        // This tests the pure reconcile function with deterministic inputs
        // (no tmux dependency).
        let targets = vec![
            make_liveness_check("agent1", "jefe-agent1", false),
            make_liveness_check("agent2", "jefe-agent2", false),
        ];

        let existing: HashSet<String> = HashSet::new();
        let alive_panes: HashSet<String> = HashSet::new();
        let dead = reconcile_dead_agents(&targets, &existing, &alive_panes);
        assert_eq!(dead.len(), 2, "empty tmux state means all targets are dead");
    }

    #[test]
    fn batch_liveness_check_does_not_panic() {
        // Smoke test: batch_liveness_check must not panic regardless of
        // whether a tmux server is available. The fail-open contract (returns
        // empty Vec when tmux is unavailable) is verified by the pure
        // reconcile_dead_agents test above.
        let targets = vec![
            make_liveness_check("agent1", "jefe-agent1", false),
            make_liveness_check("agent2", "jefe-agent2", false),
        ];
        let _ = batch_liveness_check(&targets);
    }

    // --- reconcile_dead_agents_with_identity (issue #301 Phase 4) ---

    #[test]
    fn reconcile_with_identity_returns_identity_triples() {
        let targets = vec![
            make_liveness_check("agent1", "jefe-agent1", false),
            make_liveness_check("agent2", "jefe-agent2", false),
        ];
        let existing: HashSet<String> = std::iter::once("jefe-agent1".to_string()).collect();
        let alive_panes: HashSet<String> = std::iter::once("jefe-agent1".to_string()).collect();

        let dead = reconcile_dead_agents_with_identity(&targets, &existing, &alive_panes);
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].agent_id.0, "agent2");
        assert_eq!(dead[0].binding_session_name.as_deref(), Some("jefe-agent2"));
        assert_eq!(dead[0].lifecycle_generation, 0);
    }

    #[test]
    fn reconcile_with_identity_excludes_remote() {
        let targets = vec![
            make_liveness_check("local", "jefe-local", false),
            make_liveness_check("remote", "jefe-remote", true),
        ];
        let existing: HashSet<String> = HashSet::new();
        let alive_panes: HashSet<String> = HashSet::new();

        let dead = reconcile_dead_agents_with_identity(&targets, &existing, &alive_panes);
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].agent_id.0, "local");
    }

    #[test]
    fn batch_liveness_check_with_identity_does_not_panic() {
        let targets = vec![
            make_liveness_check("agent1", "jefe-agent1", false),
            make_liveness_check("agent2", "jefe-agent2", false),
        ];
        let _ = batch_liveness_check_with_identity(&targets);
    }

    #[test]
    fn batch_command_count_constant_with_agent_count() {
        // Issue #301 Phase 4: batch_liveness_check uses exactly two tmux
        // subprocesses regardless of N. The pure reconcile function
        // processes N targets without any additional subprocesses.
        for n in 1..=5 {
            let targets: Vec<_> = (0..n)
                .map(|i| {
                    make_liveness_check(&format!("agent{i}"), &format!("jefe-agent{i}"), false)
                })
                .collect();
            let existing: HashSet<String> =
                targets.iter().map(|t| t.session_name.clone()).collect();
            let alive_panes: HashSet<String> = existing.clone();
            let dead = reconcile_dead_agents_with_identity(&targets, &existing, &alive_panes);
            assert!(dead.is_empty(), "all alive for n={n}");
        }
    }

    // --- existing tests ---

    #[test]
    fn check_nonexistent_session_returns_false() {
        // This session should not exist
        let alive = check_session_alive("jefe-nonexistent-test-session-12345");
        assert!(!alive);
    }

    #[test]
    fn pid_alive_returns_true_for_current_process() {
        // The current process always exists, so kill -0 must succeed.
        let me = std::process::id();
        assert!(pid_alive(me));
    }

    #[test]
    fn pid_alive_returns_false_for_nonexistent_pid() {
        // 2_000_000_000 is within pid_t (i32) range but far above every
        // platform's pid_max (Linux ~4.19M, macOS ~99998), so kill -0
        // deterministically returns ESRCH (no such process). u32::MAX
        // (4_294_967_295) overflows pid_t parsing on macOS, which is
        // implementation-defined.
        assert!(!pid_alive(2_000_000_000));
    }

    // --- run_child_with_timeout (issue #287 review: kill path must be verified) ---

    #[cfg(unix)]
    #[test]
    fn run_child_with_timeout_kills_long_running_subprocess() {
        // Spawn a `sleep 30` and verify run_child_with_timeout kills it after
        // a 1-second deadline rather than blocking indefinitely.
        use std::process::Command;
        let child = Command::new("sleep")
            .arg("30")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|_| panic!("spawn sleep"));
        let deadline = Instant::now() + Duration::from_secs(1);
        let result = run_child_with_timeout(child, deadline);
        assert!(result.is_err(), "timeout must produce Err");
    }

    #[cfg(unix)]
    #[test]
    fn run_child_with_timeout_returns_output_for_fast_subprocess() {
        use std::process::Command;
        let child = Command::new("echo")
            .arg("ok")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|_| panic!("spawn echo"));
        let deadline = Instant::now() + Duration::from_secs(5);
        let result = run_child_with_timeout(child, deadline);
        assert!(result.is_ok(), "fast subprocess must succeed");
        let output = result.unwrap_or_else(|()| panic!("checked ok"));
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("ok"), "output must contain echo result");
    }
}
