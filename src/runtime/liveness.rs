//! Liveness checking for tmux sessions.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @pseudocode component-002 lines 33-35

use std::collections::HashSet;
use std::hash::BuildHasher;

use crate::domain::{AgentId, RemoteRepositorySettings};
use crate::runtime::commands::{
    remote_tmux_command, run_remote_ssh, shell_escape_single, tmux_command,
};
use crate::runtime::manager::LivenessCheck;

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

/// Check if a tmux session exists and has at least one non-dead pane.
///
/// @pseudocode component-002 lines 33-35
#[must_use]
pub fn check_session_alive(session_name: &str) -> bool {
    let Ok(mut command) = tmux_command() else {
        return false;
    };
    let has_session = command.args(["has-session", "-t", session_name]).output();

    let Ok(out) = has_session else {
        return false;
    };
    if !out.status.success() {
        return false;
    }

    let Ok(mut command) = tmux_command() else {
        return false;
    };
    let panes = command
        .args(["list-panes", "-t", session_name, "-F", "#{pane_dead}"])
        .output();

    let Ok(out) = panes else {
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
            let pane_dead = pane_dead.trim();
            if pane_dead == "0" || pane_dead.eq_ignore_ascii_case("false") {
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

/// Query the tmux server once for all alive sessions, returning the set of
/// session names that exist AND have at least one non-dead pane.
///
/// This uses exactly **two** tmux subprocess invocations regardless of the
/// number of agents, replacing the previous approach of 2 subprocesses per
/// running agent (issue #287).
///
/// Returns the set of session names that are alive. An empty set means either
/// no sessions exist or the tmux server is unavailable.
#[must_use]
pub fn alive_session_set() -> HashSet<String> {
    let existing = list_all_sessions();
    if existing.is_empty() {
        return HashSet::new();
    }
    let alive_panes = list_alive_pane_sessions();
    existing.intersection(&alive_panes).cloned().collect()
}

/// Batch liveness check: query the tmux server once (two subprocesses total)
/// and reconcile against the given local targets, returning the agent IDs
/// whose sessions are dead or missing.
///
/// Remote targets are excluded automatically. This is the single-call API
/// for callers that want dead agent IDs without managing the intermediate sets.
#[must_use]
pub fn batch_liveness_check(targets: &[LivenessCheck]) -> Vec<AgentId> {
    let existing = list_all_sessions();
    let alive_panes = list_alive_pane_sessions();
    reconcile_dead_agents(targets, &existing, &alive_panes)
}

/// Query the tmux server for all session names (one subprocess).
#[must_use]
fn list_all_sessions() -> HashSet<String> {
    let Ok(mut command) = tmux_command() else {
        return HashSet::new();
    };
    let output = command
        .args(["list-sessions", "-F", "#{session_name}"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            parse_alive_sessions(&String::from_utf8_lossy(&out.stdout))
        }
        _ => HashSet::new(),
    }
}

/// Query the tmux server for all sessions that have at least one non-dead pane
/// (one subprocess).
///
/// Uses `tmux list-panes -a` (all sessions) with a format that includes the
/// session name and pane-dead flag, so a single subprocess covers every
/// session.
#[must_use]
fn list_alive_pane_sessions() -> HashSet<String> {
    let Ok(mut command) = tmux_command() else {
        return HashSet::new();
    };
    let output = command
        .args(["list-panes", "-a", "-F", "#{session_name}:#{pane_dead}"])
        .output();

    match output {
        Ok(out) if out.status.success() => parse_pane_alive(&String::from_utf8_lossy(&out.stdout)),
        _ => HashSet::new(),
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
        let raw = "  jefe-a  
 jefe-b 

";
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
    fn parse_pane_alive_handles_false_flag() {
        let raw = "jefe-a:false
jefe-b:true
";
        let set = parse_pane_alive(raw);
        assert!(set.contains("jefe-a"));
        assert!(!set.contains("jefe-b"));
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
    fn alive_session_set_empty_when_no_tmux_server() {
        // On a system without tmux or with no sessions, this returns empty.
        // This test validates graceful failure, not the presence of tmux.
        let set = alive_session_set();
        // We don't assert is_empty because a tmux server might have sessions
        // from other processes. We just verify it doesn't panic.
        let _ = set;
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
}
