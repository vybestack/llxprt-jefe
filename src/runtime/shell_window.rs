//! Embedded shell-window multiplexer operations (issue #222).

use std::collections::{BTreeSet, HashMap};
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

/// Immutable inputs needed to drive a shell-window subprocess without holding
/// an `AppContext` guard (issue #374 S1).
///
/// Mirrors the existing `AttachInputs` snapshot/free-execute boundary: the
/// caller captures this under a short lock, releases the lock, runs the
/// (potentially blocking) multiplexer subprocess via `execute_*`, then
/// revalidates ownership with [`ShellWindowInputs::owner_still_matches`]
/// before applying the result. Command construction is unchanged from the
/// in-lock path.
#[derive(Clone, Debug)]
pub struct ShellWindowInputs {
    /// The owning agent id captured at snapshot time.
    pub owner: AgentId,
    /// The multiplexer session name backing the owner (e.g. `jefe-{agent_id}`).
    pub session_name: String,
    /// Runtime lifecycle captured with the session identity.
    pub lifecycle_generation: u64,
    /// Whether the owner's repository is remote. Open/select reject remote
    /// snapshots; close/hide remain available for cleanup and compensation.
    pub remote_enabled: bool,
    /// The owner's working directory, needed only by `execute_open`.
    pub work_dir: std::path::PathBuf,
}

impl ShellWindowInputs {
    /// Open (create-or-select) the shell window off-lock. Remote snapshots are
    /// rejected without a subprocess, matching the in-lock path's invariant.
    pub fn execute_open(&self) -> Result<(), RuntimeError> {
        if self.remote_enabled {
            return Err(RuntimeError::SpawnFailed(
                "embedded shell is local-only for remote repositories".to_owned(),
            ));
        }
        open_shell_window(&self.session_name, &self.work_dir)
    }

    /// Select the existing shell window off-lock. Remote snapshots are rejected
    /// without a subprocess; a missing window surfaces `SessionNotFound`.
    pub fn execute_select(&self) -> Result<(), RuntimeError> {
        if self.remote_enabled {
            return Err(RuntimeError::SpawnFailed(
                "embedded shell is local-only for remote repositories".to_owned(),
            ));
        }
        if !shell_window_exists(&self.session_name)? {
            return Err(RuntimeError::SessionNotFound(self.owner.0.clone()));
        }
        select_shell_window(&self.session_name)
    }

    /// Close (kill) the shell window off-lock after its tracked session was
    /// snapshotted. The operation itself does not require the owner to be live.
    pub fn execute_close(&self) -> Result<(), RuntimeError> {
        close_shell_window(&self.session_name)
    }

    /// Hide the shell window off-lock by selecting window 0.
    pub fn execute_hide(&self) -> Result<(), RuntimeError> {
        hide_shell_window(&self.session_name)
    }

    /// Revalidate that the owner is still tracked with the same session name
    /// after an off-lock subprocess (issue #374 stale-owner guard).
    ///
    /// `sessions` is the current `TmuxRuntimeManager::sessions` map; a mismatch
    /// means the attached owner changed while the subprocess was in flight and
    /// the result must be discarded.
    #[must_use]
    pub fn owner_still_matches(&self, sessions: &HashMap<AgentId, RuntimeSession>) -> bool {
        sessions.get(&self.owner).is_some_and(|session| {
            session.session_name == self.session_name
                && session.lifecycle_generation == self.lifecycle_generation
        })
    }
}

/// Build a [`ShellWindowInputs`] snapshot for `agent_id` from the current
/// sessions map (issue #374 S1). Returns `None` for an untracked agent so the
/// caller surfaces the typed `SessionNotFound` failure under the short lock
/// before releasing it.
#[must_use]
pub fn shell_window_inputs_for(
    sessions: &HashMap<AgentId, RuntimeSession>,
    agent_id: &AgentId,
) -> Option<ShellWindowInputs> {
    let session = sessions.get(agent_id)?;
    Some(ShellWindowInputs {
        owner: agent_id.clone(),
        session_name: session.session_name.clone(),
        lifecycle_generation: session.lifecycle_generation,
        remote_enabled: session.launch_signature.remote.enabled,
        work_dir: session.launch_signature.work_dir.clone(),
    })
}

impl super::manager::TmuxRuntimeManager {
    /// Snapshot the actual session_name/work_dir/remote for `agent_id` while
    /// the manager lock is held, so the shell-window subprocess can run
    /// off-lock (issue #374 S1).
    ///
    /// This is the concrete snapshot boundary: the caller holds the
    /// `AppContext` mutex, calls this to capture typed inputs, then releases
    /// the lock and dispatches `ShellWindowInputs::execute_*`. Returns `None`
    /// for an untracked agent so the caller surfaces `SessionNotFound` under
    /// the short lock.
    #[must_use]
    pub fn shell_window_inputs(&self, agent_id: &AgentId) -> Option<ShellWindowInputs> {
        shell_window_inputs_for(&self.sessions, agent_id)
    }

    /// Revalidate that the owner captured in `inputs` is still tracked with
    /// the same session name after an off-lock subprocess (issue #374 S2).
    ///
    /// Used by open/select paths to reject stale results when the attached
    /// owner changed while the subprocess was in flight.
    #[must_use]
    pub fn shell_window_owner_matches(&self, inputs: &ShellWindowInputs) -> bool {
        inputs.owner_still_matches(&self.sessions)
    }
}

pub(super) fn open_manager_shell_window(
    sessions: &HashMap<AgentId, RuntimeSession>,
    agent_id: &AgentId,
) -> Result<(), RuntimeError> {
    let Some(inputs) = shell_window_inputs_for(sessions, agent_id) else {
        return Err(RuntimeError::SessionNotFound(agent_id.0.clone()));
    };
    inputs.execute_open()
}

pub(super) fn select_manager_shell_window(
    sessions: &HashMap<AgentId, RuntimeSession>,
    agent_id: &AgentId,
) -> Result<(), RuntimeError> {
    let Some(inputs) = shell_window_inputs_for(sessions, agent_id) else {
        return Err(RuntimeError::SessionNotFound(agent_id.0.clone()));
    };
    inputs.execute_select()
}

pub(super) fn close_manager_shell_window(
    sessions: &HashMap<AgentId, RuntimeSession>,
    agent_id: &AgentId,
) -> Result<(), RuntimeError> {
    let Some(inputs) = shell_window_inputs_for(sessions, agent_id) else {
        return Err(RuntimeError::SessionNotFound(agent_id.0.clone()));
    };
    inputs.execute_close()
}

/// Hide the embedded shell window for an agent by selecting window 0
/// (issue #361 PR A). Manager-scoped helper so the trait impl stays thin.
pub(super) fn hide_manager_shell_window(
    sessions: &HashMap<AgentId, RuntimeSession>,
    agent_id: &AgentId,
) -> Result<(), RuntimeError> {
    let Some(inputs) = shell_window_inputs_for(sessions, agent_id) else {
        return Err(RuntimeError::SessionNotFound(agent_id.0.clone()));
    };
    inputs.execute_hide()
}

/// Close every tracked `jefe-shell` window best-effort (issue #361 PR A).
/// Manager-scoped helper.
///
/// Returns the per-session failures so callers (graceful shutdown) can report
/// them without blocking the quit path. Sessions whose close succeeds are not
/// represented; failures follow the deterministic discovery order.
pub(super) fn close_all_manager_shell_windows() -> Vec<RuntimeError> {
    match observe_shell_window_sessions() {
        Ok(session_names) => close_all_shell_windows(&session_names),
        Err(error) => vec![error],
    }
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
    let mut command = select_window_command(&multiplexer, target);
    let output = command
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

/// Build a `select-window -t <target>` command carrying the plan's base args.
///
/// Exposed so structural tests can verify the command shape (including the
/// psmux `-L` namespace prefix invariant) without live tmux.
pub(super) fn select_window_command(multiplexer: &MultiplexerPlan, target: &str) -> Command {
    let mut command = multiplexer.command();
    command.args(["select-window", "-t", target]);
    command
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

pub(super) fn new_window_command(
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

/// Hide the embedded shell window for `session_name` by selecting the agent
/// window 0 (issue #361 PR A).
///
/// This leaves the `jefe-shell` window alive but makes window 0 the
/// multiplexer's current window, satisfying the invariant that whenever a
/// shell is not visible, the owning session's current window is window 0.
/// The viewer/capture path follows the multiplexer current window, so this
/// restores the agent pane without disturbing the shell.
pub fn hide_shell_window(session_name: &str) -> Result<(), RuntimeError> {
    select_window(&format!("{session_name}:0"))
}

/// Observe every session hosting a `jefe-shell` window via one batched
/// `list-windows -a` query, with a bounded fallback (issue #361).
pub fn observe_shell_window_sessions() -> Result<Vec<String>, RuntimeError> {
    let multiplexer = MultiplexerPlan::current().map_err(RuntimeError::Multiplexer)?;
    let mut batched = list_all_shell_windows_plan(&multiplexer);
    match batched.output() {
        Ok(output) if output.status.success() => Ok(parse_sessions_with_shell_windows(
            &String::from_utf8_lossy(&output.stdout),
        )),
        Ok(_failed) => {
            // Bounded fallback enumerates all sessions so orphans are still
            // discovered.
            let mut sessions_command = multiplexer.command();
            sessions_command.args(["list-sessions", "-F", "#{session_name}"]);
            let sessions_output = sessions_command.output().map_err(|error| {
                RuntimeError::CapabilityProbeFailed(format!(
                    "tmux list-sessions spawn failed: {error}"
                ))
            })?;
            if !sessions_output.status.success() {
                return Err(RuntimeError::CapabilityProbeFailed(format!(
                    "tmux list-sessions failed: {}",
                    String::from_utf8_lossy(&sessions_output.stderr)
                )));
            }
            let known: Vec<String> = String::from_utf8_lossy(&sessions_output.stdout)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(String::from)
                .collect();
            let mut owners = Vec::new();
            for session in &known {
                let mut command = list_shell_windows_plan(&multiplexer, session);
                match command.output() {
                    Ok(output) if output.status.success() => {
                        if parse_shell_window_names(&String::from_utf8_lossy(&output.stdout)) {
                            owners.push(session.clone());
                        }
                    }
                    Ok(output) => {
                        return Err(RuntimeError::CapabilityProbeFailed(format!(
                            "tmux list-windows failed for {session}: {}",
                            String::from_utf8_lossy(&output.stderr)
                        )));
                    }
                    Err(error) => {
                        return Err(RuntimeError::CapabilityProbeFailed(format!(
                            "tmux list-windows spawn failed for {session}: {error}"
                        )));
                    }
                }
            }
            Ok(owners)
        }
        Err(error) => Err(RuntimeError::CapabilityProbeFailed(format!(
            "tmux list-windows -a spawn failed: {error}"
        ))),
    }
}

/// Build a `list-windows -a -F #{session_name}:#{window_name}` command for
/// observing every `jefe-shell` window across all sessions in a single batched
/// query (issue #361 PR A).
///
/// `-a` enumerates windows in every session; the format string joins session
/// and window name so one query discovers owners even for sessions the caller
/// does not know about yet (needed for startup orphan detection). Exposed so
/// structural tests can verify the command shape across Unix and Windows/psmux
/// without live tmux.
pub(super) fn list_all_shell_windows_plan(multiplexer: &MultiplexerPlan) -> Command {
    let mut command = multiplexer.command();
    command.args(["list-windows", "-a", "-F", "#{session_name}:#{window_name}"]);
    command
}

/// Parse raw `list-windows -a -F #{session_name}:#{window_name}` stdout into
/// the set of session names that host a `jefe-shell` window (issue #361).
///
/// Each non-empty line is `<session_name>:<window_name>`. A session owns a
/// shell window when at least one of its lines names `jefe-shell`. Window
/// names may themselves contain colons, so only the *last* colon is treated
/// as the delimiter (window names are the trailing segment). Pure function:
/// no I/O, deterministic, unit-testable.
#[must_use]
pub fn parse_sessions_with_shell_windows(raw: &str) -> Vec<String> {
    raw.lines()
        .filter_map(|line| line.rsplit_once(':'))
        .filter(|(_, window)| window.trim() == SHELL_WINDOW_NAME)
        .map(|(session, _)| session.trim())
        .filter(|session| !session.is_empty())
        .map(String::from)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Parse raw `list-windows -t <session> -F #{window_name}` stdout and report
/// whether a `jefe-shell` window is present (issue #361). Used by the bounded
/// fallback path only.
#[must_use]
pub fn parse_shell_window_names(stdout: &str) -> bool {
    stdout.lines().any(|name| name.trim() == SHELL_WINDOW_NAME)
}

/// Build one `kill-window -t <session>:jefe-shell` command per session name
/// for graceful shutdown (issue #361 PR A).
///
/// Returns the commands without executing them so the caller can drive them
/// best-effort (log failures, do not block quit). Exposed for structural
/// tests.
pub(super) fn close_all_shell_windows_plan(
    multiplexer: &MultiplexerPlan,
    session_names: &[String],
) -> Vec<Command> {
    session_names
        .iter()
        .map(|session| {
            let target = format!("{session}:{SHELL_WINDOW_NAME}");
            let mut command = multiplexer.command();
            command.args(["kill-window", "-t", &target]);
            command
        })
        .collect()
}

/// Close every known `jefe-shell` window best-effort (issue #361 PR A).
///
/// Used by graceful shutdown. Failures are collected and logged via `tracing`
/// and returned so the caller can surface them; they do not block the quit
/// path. Does not kill agent sessions — only the temporary `jefe-shell`
/// windows. Each session is closed exactly once (the visible shell is closed
/// here too, so the caller must not double-close it separately).
pub fn close_all_shell_windows(session_names: &[String]) -> Vec<RuntimeError> {
    let multiplexer = match MultiplexerPlan::current() {
        Ok(plan) => plan,
        Err(error) => {
            tracing::warn!(error = %error, "shutdown: multiplexer unavailable, skipping shell cleanup");
            return vec![RuntimeError::Multiplexer(error)];
        }
    };
    let mut failures = Vec::new();
    for mut command in close_all_shell_windows_plan(&multiplexer, session_names) {
        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        match command.output() {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                tracing::warn!(target = ?args, stderr = %stderr, "shutdown: best-effort shell close failed");
                failures.push(RuntimeError::KillFailed(format!(
                    "tmux kill-window failed: {stderr}"
                )));
            }
            Err(error) => {
                tracing::warn!(target = ?args, error = %error, "shutdown: best-effort shell close spawn failed");
                failures.push(RuntimeError::KillFailed(format!(
                    "tmux kill-window spawn failed: {error}"
                )));
            }
        }
    }
    failures
}

/// Build a `list-windows -t <session> -F #{window_name}` command for the
/// bounded fallback observation path (issue #361 PR A). Exposed so structural
/// tests can verify the command shape across Unix and Windows/psmux.
pub(super) fn list_shell_windows_plan(
    multiplexer: &MultiplexerPlan,
    session_name: &str,
) -> Command {
    let mut command = multiplexer.command();
    command.args(["list-windows", "-t", session_name, "-F", "#{window_name}"]);
    command
}

/// Build a `capture-pane -p -t <session>:jefe-shell` argv for the Terminal
/// Manager preview (issue #361 PR B).
///
/// Targeted capture of the shell window only — never the agent pane and never
/// a second live viewer. Exposed so structural tests can verify the command
/// shape across Unix tmux and Windows/psmux without live tmux.
#[must_use]
pub(super) fn capture_shell_preview_command(
    multiplexer: &MultiplexerPlan,
    session_name: &str,
) -> Command {
    let mut command = multiplexer.command();
    let target = format!("{session_name}:{SHELL_WINDOW_NAME}");
    command.args(["capture-pane", "-p", "-t", &target]);
    command
}

/// Capture a throttled, read-only preview of the `<session>:jefe-shell` pane
/// as plain text lines (issue #361 PR B).
///
/// Targets the shell window only — never the agent pane, never a second live
/// viewer. Bounded by the visible pane (no `-S` history); the manager preview
/// is intentionally reduced. Session-name free so dead owners still produce a
/// (failed) result rather than blocking the manager.
pub fn capture_shell_preview(session_name: &str) -> Result<Vec<String>, RuntimeError> {
    let multiplexer = MultiplexerPlan::current().map_err(RuntimeError::Multiplexer)?;
    let mut command = capture_shell_preview_command(&multiplexer, session_name);
    let output = command.output().map_err(|error| {
        RuntimeError::CapabilityProbeFailed(format!("tmux capture-pane spawn failed: {error}"))
    })?;
    if !output.status.success() {
        return Err(RuntimeError::CapabilityProbeFailed(format!(
            "tmux capture-pane failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().map(std::borrow::ToOwned::to_owned).collect())
}

#[cfg(test)]
#[path = "shell_window_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "shell_lifecycle_tests.rs"]
mod lifecycle_tests;

#[cfg(test)]
#[path = "shell_window_snapshot_tests.rs"]
mod snapshot_tests;
