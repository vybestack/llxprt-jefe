//! Embedded shell-window multiplexer operations (issue #222).

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use crate::domain::AgentId;

use super::errors::RuntimeError;
use super::multiplexer::MultiplexerPlan;
use super::session::RuntimeSession;

/// Fixed multiplexer window name for the embedded agent shell.
pub const SHELL_WINDOW_NAME: &str = "jefe-shell";

fn manager_session<'a>(
    sessions: &'a HashMap<AgentId, RuntimeSession>,
    agent_id: &AgentId,
) -> Result<&'a RuntimeSession, RuntimeError> {
    sessions
        .get(agent_id)
        .ok_or_else(|| RuntimeError::SessionNotFound(agent_id.0.clone()))
}

pub(super) fn open_manager_shell_window(
    sessions: &HashMap<AgentId, RuntimeSession>,
    agent_id: &AgentId,
) -> Result<(), RuntimeError> {
    let session = manager_session(sessions, agent_id)?;
    if session.launch_signature.remote.enabled {
        return Err(RuntimeError::SpawnFailed(
            "embedded shell is local-only for remote repositories".to_owned(),
        ));
    }
    open_shell_window(&session.session_name, &session.launch_signature.work_dir)
}

pub(super) fn close_manager_shell_window(
    sessions: &HashMap<AgentId, RuntimeSession>,
    agent_id: &AgentId,
) -> Result<(), RuntimeError> {
    close_shell_window(&manager_session(sessions, agent_id)?.session_name)
}

pub(super) fn manager_shell_window_exists(
    sessions: &HashMap<AgentId, RuntimeSession>,
    agent_id: &AgentId,
) -> Result<bool, RuntimeError> {
    shell_window_exists(&manager_session(sessions, agent_id)?.session_name)
}

/// Open or select the temporary shell window without disturbing the agent pane.
pub fn open_shell_window(session_name: &str, work_dir: &Path) -> Result<(), RuntimeError> {
    if shell_window_exists(session_name)? {
        return select_shell_window(session_name);
    }
    if !agent_window_alive(session_name)? {
        return Err(RuntimeError::SpawnFailed(
            "agent window is no longer running".to_owned(),
        ));
    }

    let multiplexer = MultiplexerPlan::current().map_err(RuntimeError::Multiplexer)?;
    let output = new_window_command(&multiplexer, session_name, work_dir, &default_shell())?
        .output()
        .map_err(|error| RuntimeError::SpawnFailed(format!("tmux new-window: {error}")))?;
    if output.status.success() {
        return Ok(());
    }
    if shell_window_exists(session_name)? {
        return select_shell_window(session_name);
    }
    Err(RuntimeError::SpawnFailed(format!(
        "tmux new-window failed: {}",
        String::from_utf8_lossy(&output.stderr)
    )))
}

/// Check for the named window while preserving command failures as errors.
pub fn shell_window_exists(session_name: &str) -> Result<bool, RuntimeError> {
    let multiplexer = MultiplexerPlan::current().map_err(RuntimeError::Multiplexer)?;
    let output = multiplexer
        .command()
        .args(["list-windows", "-t", session_name, "-F", "#{window_name}"])
        .output()
        .map_err(|error| {
            RuntimeError::CapabilityProbeFailed(format!("tmux list-windows: {error}"))
        })?;
    if !output.status.success() {
        return Err(RuntimeError::CapabilityProbeFailed(format!(
            "tmux list-windows failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|name| name.trim() == SHELL_WINDOW_NAME))
}

/// Kill only the temporary shell window and select the original agent window.
pub fn close_shell_window(session_name: &str) -> Result<(), RuntimeError> {
    if !shell_window_exists(session_name)? {
        return Ok(());
    }

    let multiplexer = MultiplexerPlan::current().map_err(RuntimeError::Multiplexer)?;
    let target = format!("{session_name}:{SHELL_WINDOW_NAME}");
    let output = multiplexer
        .command()
        .args(["kill-window", "-t", &target])
        .output()
        .map_err(|error| RuntimeError::KillFailed(format!("tmux kill-window: {error}")))?;
    if output.status.success() {
        return select_agent_window(session_name);
    }
    if !shell_window_exists(session_name)? {
        return select_agent_window(session_name);
    }
    Err(RuntimeError::KillFailed(format!(
        "tmux kill-window failed: {}",
        String::from_utf8_lossy(&output.stderr)
    )))
}

fn select_agent_window(session_name: &str) -> Result<(), RuntimeError> {
    select_window(&format!("{session_name}:0"))
}

fn select_shell_window(session_name: &str) -> Result<(), RuntimeError> {
    select_window(&format!("{session_name}:{SHELL_WINDOW_NAME}"))
}

fn select_window(target: &str) -> Result<(), RuntimeError> {
    let multiplexer = MultiplexerPlan::current().map_err(RuntimeError::Multiplexer)?;
    let output = multiplexer
        .command()
        .args(["select-window", "-t", target])
        .output()
        .map_err(|error| RuntimeError::SpawnFailed(format!("tmux select-window: {error}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(RuntimeError::SpawnFailed(format!(
            "tmux select-window failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )))
    }
}

fn agent_window_alive(session_name: &str) -> Result<bool, RuntimeError> {
    let multiplexer = MultiplexerPlan::current().map_err(RuntimeError::Multiplexer)?;
    let target = format!("{session_name}:0");
    let output = multiplexer
        .command()
        .args(["list-panes", "-t", &target, "-F", "#{pane_dead}"])
        .output()
        .map_err(|error| {
            RuntimeError::CapabilityProbeFailed(format!("tmux list-panes: {error}"))
        })?;
    if !output.status.success() {
        return Err(RuntimeError::CapabilityProbeFailed(format!(
            "tmux list-panes failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|value| value.trim() == "0"))
}

fn new_window_command(
    multiplexer: &MultiplexerPlan,
    session_name: &str,
    work_dir: &Path,
    shell: &OsString,
) -> Result<Command, RuntimeError> {
    let mut command = multiplexer.command();
    command
        .arg("new-window")
        .arg("-n")
        .arg(SHELL_WINDOW_NAME)
        .arg("-t")
        .arg(session_name)
        .arg("-c")
        .arg(work_dir);
    for argument in multiplexer
        .pane_command_args(shell.as_os_str(), &[], &[])
        .map_err(RuntimeError::Multiplexer)?
    {
        command.arg(argument);
    }
    Ok(command)
}

#[cfg(windows)]
fn default_shell() -> OsString {
    std::env::var_os("COMSPEC").unwrap_or_else(|| OsString::from("cmd.exe"))
}

#[cfg(not(windows))]
fn default_shell() -> OsString {
    std::env::var_os("SHELL").unwrap_or_else(|| OsString::from("/bin/sh"))
}

#[cfg(test)]
#[path = "shell_window_tests.rs"]
mod tests;
