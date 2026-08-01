//! Tmux command execution.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @pseudocode component-002 lines 01-06

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Output};

use tracing::debug;

use crate::domain::RemoteRepositorySettings;
use crate::domain::agent_definition::{AgentLaunchPlan, Target};

use super::agent_remote_plan::{posix_single_quote, serialize_process_command};
use super::errors::RuntimeError;
use super::multiplexer::{MultiplexerCapability, MultiplexerPlan};

/// tmux client environment variables that must NEVER propagate into an agent
/// pane. tmux sets `TMUX=<socket>,<pid>,<n>` and `TMUX_PANE=%<n>` inside every
/// pane, handing the llxprt child (and any tool it spawns) a live handle to
/// jefe's private tmux server. A bare `tmux` inside such an agent then talks to
/// jefe's server and can kill it — disconnecting every agent at once (#171).
///
/// `TMUX_TMPDIR` is also stripped so agent-side tmux activity cannot locate
/// jefe's socket directory by convention. Stripping happens via `env -u` inside
/// the pane command (the tmux server populates the pane env, so removing the
/// vars from jefe's own process env would have no effect).
const TMUX_ENV_VARS_TO_SCRUB: &[&str] = &["TMUX", "TMUX_PANE", "TMUX_TMPDIR"];

/// Build the `env -u <VAR> ...` argv prefix that scrubs jefe's tmux client vars
/// from the process running inside an agent pane. Returned as owned `String`s
/// so callers can splice them into either a local `Command` argv list or a
/// remote shell command string.
///
/// See [`TMUX_ENV_VARS_TO_SCRUB`] for why this is mandatory (#171).
#[must_use]
fn tmux_scrub_env_args() -> Vec<String> {
    let mut args = vec!["env".to_owned()];
    for var in TMUX_ENV_VARS_TO_SCRUB {
        args.push("-u".to_owned());
        args.push((*var).to_owned());
    }
    args
}

/// Resolve the local platform multiplexer and construct its isolated command.
///
/// Unix preserves upstream tmux's `/dev/null` configuration and private socket.
/// Native Windows selects qualified psmux with `NUL` and a private namespace.
pub fn tmux_command() -> Result<Command, RuntimeError> {
    MultiplexerPlan::current()
        .map(|plan| plan.command())
        .map_err(RuntimeError::Multiplexer)
}

// Re-export the pane capture / introspection helpers that production callers
// (`commands::capture_pane_lines` / `commands::capture_pane_history` /
// `commands::pane_pid` in `manager.rs`) still resolve, after the functions
// moved to `pane_capture.rs` for file-size reasons.
pub use super::pane_capture::{capture_pane_history, capture_pane_lines, pane_pid};

pub(super) fn tmux_cmd_status(args: &[&str], cwd: Option<&str>) -> Result<(), String> {
    let mut cmd = tmux_command().map_err(|error| error.to_string())?;
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

pub(super) fn apply_session_style(session_name: &str) {
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

/// Configure multiplexer prefix keys for transparent child input (#200, #260).
///
/// Unix applies this to `session_name`. Windows psmux ignores session-scoped
/// prefix values, so its private server is configured globally. Windows assigns
/// `prefix` to Jefe-owned F12 because psmux 3.3.6 still reserves `C-b` when the
/// option is `None`; `prefix2` stays disabled.
pub fn configure_prefix_for_passthrough(session_name: &str) -> Result<(), String> {
    configure_prefix_with(session_name, |args| tmux_cmd_status(args, None))
}

#[cfg(feature = "psmux-smoke")]
pub fn configure_prefix_for_passthrough_with_plan(
    session_name: &str,
    plan: &MultiplexerPlan,
) -> Result<(), String> {
    configure_prefix_with(session_name, |args| multiplexer_cmd_status(plan, args))
}

#[path = "commands_root_keys.rs"]
mod commands_root_keys;

use commands_root_keys::configure_prefix_with;

#[cfg(feature = "psmux-smoke")]
fn multiplexer_cmd_status(plan: &MultiplexerPlan, args: &[&str]) -> Result<(), String> {
    let output = plan
        .command()
        .args(args)
        .output()
        .map_err(|error| format!("failed to run multiplexer {args:?}: {error}"))?;
    output.status.success().then_some(()).ok_or_else(|| {
        format!(
            "multiplexer {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

/// Prefix value that preserves `C-b`; Jefe intercepts F12 before forwarding.
#[must_use]
const fn local_prefix_value() -> &'static str {
    if cfg!(windows) { "F12" } else { "None" }
}

#[cfg(test)]
#[test]
fn local_prefix_value_matches_platform_policy() {
    let expected = if cfg!(windows) { "F12" } else { "None" };
    assert_eq!(local_prefix_value(), expected);
}
/// The tmux prefix options managed for transparent agent input (#200, #260).
#[must_use]
pub fn prefix_options_for_passthrough() -> &'static [&'static str] {
    &["prefix", "prefix2"]
}

/// Build the `\;`-joined sequence of `set-option -t <session> <option> None`
/// sub-commands for every option in [`prefix_options_for_passthrough`].
///
/// This is the single builder for the remote prefix-disable sub-command
/// sequence, shared by the remote reattach fragment
/// ([`remote_disable_prefix_fragment`]) and the remote creation script
/// ([`build_remote_tmux_script`]) so the option list and separator formatting
/// live in one place and cannot drift (#200 review).
///
/// The returned sequence has no leading `tmux`: callers embed it either as a
/// standalone `tmux <sequence>` shell command (reattach fragment) or as
/// continuation sub-commands of an existing `tmux new-session ... \; <sequence>`
/// invocation (creation script).
fn prefix_disable_tmux_subcommands(escaped_session: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut first = true;
    for option in prefix_options_for_passthrough() {
        if !first {
            parts.push("\\;".to_owned());
        }
        parts.push("set-option".to_owned());
        parts.push("-t".to_owned());
        parts.push(escaped_session.to_owned());
        parts.push((*option).to_owned());
        parts.push("None".to_owned());
        first = false;
    }
    parts.join(" ")
}

/// Build the remote Unix tmux fragment that sets both prefix keys to `None`.
/// Used to remediate remote sessions created before the inline fix (#200);
/// Windows remotes are outside the SSH/tmux runtime contract.
fn remote_disable_prefix_fragment(escaped_session: &str) -> String {
    format!("tmux {}", prefix_disable_tmux_subcommands(escaped_session))
}

/// SSH command that disables both tmux prefix keys on an existing remote
/// session, wrapped through the remote user-escalation path. Best-effort: a
/// failure (e.g. the session already exited) is non-fatal for reattach.
pub fn remote_disable_prefix_command(
    remote: &crate::domain::RemoteRepositorySettings,
    session_name: &str,
) -> String {
    let escaped_session = shell_escape_single(session_name);
    remote_tmux_command(remote, &remote_disable_prefix_fragment(&escaped_session))
}

pub fn enforce_clipboard_passthrough(session_name: &str) {
    const PANE_FORMAT: &str = "#{session_name}:#{window_index}.#{pane_index}";

    let _ = tmux_cmd_status(["set-option", "-g", "set-clipboard", "on"].as_ref(), None);
    let _ = tmux_cmd_status(
        ["set-option", "-gp", "allow-passthrough", "on"].as_ref(),
        None,
    );
    let _ = tmux_cmd_status(
        [
            "set-option",
            "-p",
            "-t",
            session_name,
            "allow-passthrough",
            "on",
        ]
        .as_ref(),
        None,
    );

    if let Ok(mut command) = tmux_command()
        && let Ok(output) = command
            .args(["list-panes", "-t", session_name, "-F", PANE_FORMAT])
            .output()
        && output.status.success()
    {
        let panes = String::from_utf8_lossy(&output.stdout);
        for pane in panes.lines().map(str::trim).filter(|line| !line.is_empty()) {
            let _ = tmux_cmd_status(
                ["set-option", "-pt", pane, "allow-passthrough", "on"].as_ref(),
                None,
            );
        }
    }
}

pub fn shell_escape_single(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

fn remote_effective_user(remote: &crate::domain::RemoteRepositorySettings) -> String {
    if remote.run_as_user.trim().is_empty() {
        remote.login_user.trim().to_owned()
    } else {
        remote.run_as_user.trim().to_owned()
    }
}
pub fn remote_tmux_command(
    remote: &crate::domain::RemoteRepositorySettings,
    inner_command: &str,
) -> String {
    let effective_user = remote_effective_user(remote);
    if effective_user == remote.login_user.trim() {
        inner_command.to_owned()
    } else {
        format!(
            "sudo -n su - {} -c {}",
            shell_escape_single(&effective_user),
            shell_escape_single(inner_command),
        )
    }
}

fn remote_has_session_command(
    remote: &crate::domain::RemoteRepositorySettings,
    session_name: &str,
) -> String {
    remote_tmux_command(
        remote,
        &format!("tmux has-session -t {}", shell_escape_single(session_name)),
    )
}

fn remote_kill_session_command(
    remote: &crate::domain::RemoteRepositorySettings,
    session_name: &str,
) -> String {
    remote_tmux_command(
        remote,
        &format!("tmux kill-session -t {}", shell_escape_single(session_name)),
    )
}

pub fn build_remote_attach_plan(
    remote: &crate::domain::RemoteRepositorySettings,
    session_name: &str,
) -> Result<crate::ssh::SshPlan, RuntimeError> {
    let remote_command = remote_tmux_command(
        remote,
        &format!(
            "tmux attach-session -t {}",
            shell_escape_single(session_name)
        ),
    );
    crate::ssh::SshPlan::new(remote, &remote_command, crate::ssh::SshMode::Terminal)
        .map_err(|error| RuntimeError::RemoteExecutionFailed(error.to_string()))
}

pub fn run_remote_ssh(
    remote: &crate::domain::RemoteRepositorySettings,
    remote_command: &str,
) -> Result<Output, RuntimeError> {
    let plan = crate::ssh::SshPlan::new(remote, remote_command, crate::ssh::SshMode::Terminal)
        .map_err(|error| RuntimeError::RemoteExecutionFailed(error.to_string()))?;
    plan.execute(None, crate::ssh::SSH_OPERATION_TIMEOUT, None)
        .map_err(|error| RuntimeError::RemoteExecutionFailed(error.to_string()))
}

fn ensure_remote_success(
    remote: &crate::domain::RemoteRepositorySettings,
    action: &str,
    output: Output,
) -> Result<Output, RuntimeError> {
    if output.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let failure = crate::ssh::classify_failure(output.status.code(), &stderr);
        Err(RuntimeError::RemoteExecutionFailed(format!(
            "{action} on {}@{}: {failure}",
            remote.login_user.trim(),
            remote.host.trim()
        )))
    }
}

fn remote_target_matches_settings(
    target: &crate::domain::agent_definition::RemoteTarget,
    settings: &RemoteRepositorySettings,
) -> bool {
    let identity = (target.host.as_str(), target.user.as_str());
    let configured_identity = (settings.host.as_str(), settings.login_user.as_str());
    let ports_match = target.port.unwrap_or(22) == settings.port.unwrap_or(22);
    identity == configured_identity && ports_match && target.run_as_user == settings.run_as_user
}

fn remote_for_plan<'a>(
    plan: &AgentLaunchPlan,
    remote: Option<&'a RemoteRepositorySettings>,
) -> Result<Option<&'a RemoteRepositorySettings>, RuntimeError> {
    match (&plan.target, remote) {
        (Target::Local { .. }, None) => Ok(None),
        (Target::Remote(target), Some(settings))
            if remote_target_matches_settings(target, settings) =>
        {
            Ok(Some(settings))
        }
        (Target::Local { .. }, Some(_)) | (Target::Remote(_), None | Some(_)) => {
            Err(RuntimeError::SpawnFailed(
                "launch plan target does not match authorized runtime transport".to_owned(),
            ))
        }
    }
}

fn build_remote_tmux_script(
    plan: &AgentLaunchPlan,
    session_name: &str,
) -> Result<String, RuntimeError> {
    let cwd = plan.cwd.to_str().ok_or_else(|| {
        RuntimeError::RemoteExecutionFailed("remote working directory is not UTF-8".to_owned())
    })?;
    let escaped_cwd = posix_single_quote(cwd)
        .map_err(|error| RuntimeError::RemoteExecutionFailed(error.to_string()))?;
    let escaped_session = posix_single_quote(session_name)
        .map_err(|error| RuntimeError::RemoteExecutionFailed(error.to_string()))?;
    let process = serialize_process_command(plan)
        .map_err(|error| RuntimeError::RemoteExecutionFailed(error.to_string()))?;
    let scrub = tmux_scrub_env_args().join(" ");
    let prefix_options = format!(" \\; {}", prefix_disable_tmux_subcommands(&escaped_session));
    Ok(format!(
        "set -e; mkdir -p {escaped_cwd}; cd {escaped_cwd}; tmux new-session -d -s {escaped_session} -c {escaped_cwd} {scrub} {process} \\; set-option -t {escaped_session} remain-on-exit on{prefix_options}"
    ))
}

fn local_launch_command(
    session_name: &str,
    plan: &AgentLaunchPlan,
    session_host_root: Option<&Path>,
) -> Result<Command, RuntimeError> {
    let multiplexer = MultiplexerPlan::current().map_err(RuntimeError::Multiplexer)?;
    let mut command = multiplexer.command();
    command
        .arg("new-session")
        .arg("-d")
        .arg("-s")
        .arg(session_name)
        .arg("-c")
        .arg(&plan.cwd);
    let pane_args: Vec<OsString> = plan.argv.iter().map(OsString::from).collect();
    let environment: Vec<(OsString, OsString)> = plan
        .env
        .iter()
        .map(|(key, value)| (OsString::from(key), OsString::from(value)))
        .collect();
    // Issue #543: the host records the worker it spawns here. Stale reports from
    // a previous launch of the same session must not be mistaken for this one.
    let worker_report = super::worker_report::report_path_for_session(session_name);
    super::worker_report::remove_report(&worker_report);
    let pane_command = super::session_host::resolve_local_pane_command(
        &multiplexer,
        &super::multiplexer::AgentPaneLaunch {
            executable: (&plan.executable, plan.executable_wrapper),
            args: &pane_args,
            environment: &environment,
            cwd: &plan.cwd,
            worker_report: Some(worker_report.as_path()),
        },
        session_host_root.map(|root| (root, session_name)),
    )?;
    for argument in pane_command {
        command.arg(argument);
    }
    Ok(command)
}

/// Local-session finalization (clipboard/prefix passthrough, remain-on-exit,
/// style, warning), split out so this file stays under the source-size limit.
enum LocalCreateFailure {
    Runtime(RuntimeError),
    Command(String),
}

fn try_local_create_session(
    session_name: &str,
    plan: &AgentLaunchPlan,
    attempt: u8,
    session_host_root: Option<&Path>,
) -> Result<(), LocalCreateFailure> {
    let mut command = local_launch_command(session_name, plan, session_host_root)
        .map_err(LocalCreateFailure::Runtime)?;
    debug!(
        session_name,
        attempt, "create_session invoking local multiplexer new-session"
    );
    let output = command
        .output()
        .map_err(|error| LocalCreateFailure::Command(error.to_string()))?;
    if output.status.success() {
        debug!(
            session_name,
            attempt, "create_session local multiplexer new-session succeeded"
        );
        super::commands_finalize::finalize_local_session(session_name, None);
        Ok(())
    } else {
        Err(LocalCreateFailure::Command(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
    }
}

fn create_remote_session(
    session_name: &str,
    plan: &AgentLaunchPlan,
    remote: &RemoteRepositorySettings,
) -> Result<(), RuntimeError> {
    let script = build_remote_tmux_script(plan, session_name)?;
    let remote_command = remote_tmux_command(remote, &script);
    let output = run_remote_ssh(remote, &remote_command)?;
    ensure_remote_success(remote, "remote tmux new-session", output).map(|_| ())
}

fn is_tmux_fork_broken(stderr: &str) -> bool {
    stderr.contains("fork failed") || stderr.contains("Device not configured")
}

fn local_spawn_error(session_name: &str, attempt: u8, stderr: String) -> RuntimeError {
    debug!(session_name = %session_name, attempt, stderr = %stderr, "create_session tmux new-session failed");
    RuntimeError::SpawnFailed(format!("tmux new-session failed: {stderr}"))
}

/// Create a detached tmux session from one finalized immutable launch plan.
///
/// The runtime never resolves an executable or reconstructs argv here. Remote
/// transport settings are supplied separately because they include connection
/// material excluded from the canonical launch signature.
pub fn create_session(
    session_name: &str,
    plan: &AgentLaunchPlan,
    remote: Option<&RemoteRepositorySettings>,
    session_host_root: Option<&Path>,
) -> Result<(), RuntimeError> {
    debug!(session_name, work_dir = %plan.cwd.display(), "create_session start");
    if let Some(remote) = remote_for_plan(plan, remote)? {
        return create_remote_session(session_name, plan, remote);
    }

    MultiplexerPlan::current()
        .and_then(|multiplexer| {
            multiplexer.preflight(&[
                MultiplexerCapability::AttachSession,
                MultiplexerCapability::PaneCapture,
            ])
        })
        .map_err(RuntimeError::Multiplexer)?;

    let _ = kill_session(session_name);
    match try_local_create_session(session_name, plan, 0, session_host_root) {
        Ok(()) => return Ok(()),
        Err(LocalCreateFailure::Runtime(error)) => return Err(error),
        Err(LocalCreateFailure::Command(stderr)) if is_tmux_fork_broken(&stderr) => {
            debug!(
                session_name,
                attempt = 0,
                stderr,
                "create_session retrying after multiplexer fork failure"
            );
            let _ = kill_session(session_name);
        }
        Err(LocalCreateFailure::Command(stderr)) => {
            return Err(local_spawn_error(session_name, 0, stderr));
        }
    }

    match try_local_create_session(session_name, plan, 1, session_host_root) {
        Ok(()) => Ok(()),
        Err(LocalCreateFailure::Runtime(error)) => Err(error),
        Err(LocalCreateFailure::Command(stderr)) => Err(local_spawn_error(session_name, 1, stderr)),
    }
}

/// Check if a tmux session exists.
#[allow(dead_code)]
pub fn session_exists(session_name: &str) -> Result<bool, RuntimeError> {
    let output = tmux_command()?
        .args(["has-session", "-t", session_name])
        .output()
        .map_err(|error| RuntimeError::CapabilityProbeFailed(error.to_string()))?;
    Ok(output.status.success())
}

pub fn remote_session_exists(
    remote: &crate::domain::RemoteRepositorySettings,
    session_name: &str,
) -> Result<bool, RuntimeError> {
    let command = remote_has_session_command(remote, session_name);
    let output = run_remote_ssh(remote, &command)?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(RuntimeError::CapabilityProbeFailed(format!(
            "remote tmux session probe failed: {}",
            output.status
        ))),
    }
}
/// Kill a tmux session.
/// @pseudocode component-002 lines 24-25
pub fn kill_session(session_name: &str) -> Result<(), RuntimeError> {
    let output = tmux_command()?
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

pub fn kill_remote_session(
    remote: &crate::domain::RemoteRepositorySettings,
    session_name: &str,
) -> Result<(), RuntimeError> {
    let command = remote_kill_session_command(remote, session_name);
    let output = run_remote_ssh(remote, &command)?;
    ensure_remote_success(remote, "remote tmux kill-session", output)?;
    Ok(())
}

/// Send keys to a tmux session (for testing/automation).
#[allow(dead_code)]
pub fn send_keys(session_name: &str, keys: &str) -> Result<(), RuntimeError> {
    let output = tmux_command()?
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

#[cfg(all(test, unix))]
#[path = "prefix_passthrough_tests.rs"]
mod prefix_passthrough_tests;
