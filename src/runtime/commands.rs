//! Tmux command execution.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @pseudocode component-002 lines 01-06

use std::path::Path;
use std::process::Command;

use crate::domain::LaunchSignature;

use super::errors::RuntimeError;

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
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(RuntimeError::SpawnFailed(format!(
            "tmux new-session failed: {stderr}"
        )))
    }
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
