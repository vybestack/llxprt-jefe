//! Liveness checking for tmux sessions.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @pseudocode component-002 lines 33-35


use crate::domain::RemoteRepositorySettings;
use crate::runtime::commands::{remote_tmux_command, run_remote_ssh, shell_escape_single, tmux_command};

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
pub fn check_remote_session_alive(
    remote: &RemoteRepositorySettings,
    session_name: &str,
) -> bool {
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
}
