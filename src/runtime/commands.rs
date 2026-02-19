//! Tmux command execution.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @pseudocode component-002 lines 01-06

use std::path::Path;
use std::process::Command;

use crate::domain::LaunchSignature;

use super::errors::RuntimeError;

fn tmux_cmd_status(args: &[&str], cwd: Option<&str>) -> Result<(), String> {
    let mut cmd = Command::new("tmux");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("failed to run tmux {args:?}: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "tmux {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn reset_tmux_server() {
    let _ = tmux_cmd_status(["kill-server"].as_ref(), None);
}

fn apply_session_style(session_name: &str) {
    // Match app reverse-style bars: green-ish status background with black text.
    let _ = tmux_cmd_status(
        [
            "set-option",
            "-t",
            session_name,
            "status-style",
            "fg=colour0,bg=#6a9955",
        ]
        .as_ref(),
        None,
    );
}

/// Create a new detached tmux session running llxprt.
///
/// The session runs `llxprt` directly (not a shell), so when llxprt exits,
/// the tmux session becomes "dead" until explicit relaunch.
///
/// @pseudocode component-002 lines 01-06
pub fn create_session(
    session_name: &str,
    work_dir: &Path,
    signature: &LaunchSignature,
) -> Result<(), RuntimeError> {
    // Kill any stale session with the same name first
    let _ = kill_session(session_name);

    // Retry once if tmux server is in a fork-broken state.
    for attempt in 0..=1 {
        let mut cmd = Command::new("tmux");
        cmd.arg("new-session")
            .arg("-d")
            .arg("-s")
            .arg(session_name)
            .arg("-c")
            .arg(work_dir.to_str().unwrap_or("."))
            .arg("llxprt"); // Run llxprt directly

        // Add profile if specified
        if !signature.profile.is_empty() {
            cmd.arg("--profile-load").arg(&signature.profile);
        }

        // Add mode flags (e.g., --yolo)
        for flag in &signature.mode_flags {
            if !flag.is_empty() {
                cmd.arg(flag);
            }
        }

        // Add --continue if pass_continue is true
        if signature.pass_continue {
            cmd.arg("--continue");
        }

        let output = cmd
            .output()
            .map_err(|e| RuntimeError::SpawnFailed(format!("tmux new-session: {e}")))?;

        if output.status.success() {
            apply_session_style(session_name);
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let fork_broken =
            stderr.contains("fork failed") || stderr.contains("Device not configured");

        if attempt == 0 && fork_broken {
            reset_tmux_server();
            continue;
        }

        return Err(RuntimeError::SpawnFailed(format!(
            "tmux new-session failed: {stderr}"
        )));
    }

    Err(RuntimeError::SpawnFailed(
        "tmux new-session failed after retry".to_owned(),
    ))
}

/// Check if a tmux session exists.
#[allow(dead_code)]
pub fn session_exists(session_name: &str) -> bool {
    let output = Command::new("tmux")
        .args(["has-session", "-t", session_name])
        .output();

    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

/// Kill a tmux session.
///
/// @pseudocode component-002 lines 24-25
pub fn kill_session(session_name: &str) -> Result<(), RuntimeError> {
    let output = Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .output()
        .map_err(|e| RuntimeError::KillFailed(format!("tmux kill-session: {e}")))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(RuntimeError::KillFailed(format!(
            "tmux kill-session failed: {stderr}"
        )))
    }
}

/// Send keys to a tmux session (for testing/automation).
#[allow(dead_code)]
pub fn send_keys(session_name: &str, keys: &str) -> Result<(), RuntimeError> {
    let output = Command::new("tmux")
        .args(["send-keys", "-t", session_name, keys, "Enter"])
        .output()
        .map_err(|e| RuntimeError::WriteFailed(format!("tmux send-keys: {e}")))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(RuntimeError::WriteFailed(format!(
            "tmux send-keys failed: {stderr}"
        )))
    }
}
