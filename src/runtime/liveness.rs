//! Liveness checking for tmux sessions.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @pseudocode component-002 lines 33-35

use crate::domain::RemoteRepositorySettings;
use crate::runtime::commands::{
    remote_tmux_command, run_remote_ssh, shell_escape_single, tmux_command,
};

/// Check if a process with the given PID is alive via `kill -0`.
///
/// This **complements**, not replaces, [`check_session_alive`]. When the jefe
/// tmux server has died but the `llxprt` worker was reparented to launchd
/// (ppid=1) and is still running, `check_session_alive` reports false (no
/// tmux session) while `pid_alive` reports true — letting jefe recognize the
/// worker is recoverable rather than marking the agent Dead.
///
/// Uses a shell-out to `kill -0` (resolved via PATH) because the project
/// forbids `unsafe` code and the `libc`/`nix`/`sysinfo` crates. Local-only:
/// remote agents must stay on the tmux/SSH-only path.
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
    let filter = format!("PID eq {pid}");
    match std::process::Command::new("tasklist")
        .args(["/FI", &filter, "/FO", "CSV", "/NH"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let expected = format!("\",\"{pid}\",");
            String::from_utf8_lossy(&output.stdout).contains(&expected)
        }
        Ok(_) => false,
        Err(e) => {
            tracing::warn!(error = %e, pid, "failed to spawn tasklist; assuming worker alive");
            true
        }
    }
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
    let has_session = tmux_command()
        .args(["has-session", "-t", session_name])
        .output();

    let Ok(out) = has_session else {
        return false;
    };
    if !out.status.success() {
        return false;
    }

    let panes = tmux_command()
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

/// List all jefe-managed tmux sessions.
#[allow(dead_code)]
pub fn list_jefe_sessions() -> Vec<String> {
    let output = tmux_command()
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
    let output = tmux_command()
        .args(["kill-session", "-t", session_name])
        .output();

    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
