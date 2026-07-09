//! Tmux command execution.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @pseudocode component-002 lines 01-06

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use tracing::debug;

use crate::domain::LaunchSignature;

use super::errors::RuntimeError;
use super::preflight::sandbox_ssh_agent_warning;
use super::socket::jefe_tmux_socket_path;

const REMOTE_SSH_COMMAND_TIMEOUT: Duration = Duration::from_secs(20);

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

/// The fixed base arguments every jefe local tmux command starts with:
/// `-f /dev/null` (skip user config) and `-S <jefe-socket>` (dedicated socket).
///
/// Factored out so tests can inspect the base arg composition deterministically
/// without spawning tmux.
#[must_use]
pub fn tmux_base_args() -> Vec<String> {
    let socket = jefe_tmux_socket_path();
    vec![
        "-f".to_owned(),
        "/dev/null".to_owned(),
        "-S".to_owned(),
        socket.to_string_lossy().into_owned(),
    ]
}

/// Build a local tmux `Command` that skips user config (`-f /dev/null`) and
/// targets jefe's *private* socket (`-S <jefe-socket>`).
///
/// Jefe sets all tmux options programmatically, so loading `~/.tmux.conf` is
/// unnecessary and can cause errors (e.g., pane-scoped options in the user
/// config fail with "no current pane" when the server starts headlessly).
///
/// The dedicated socket (`-S`) isolates jefe's sessions from any unrelated user
/// tmux sessions that share the default socket. This prevents jefe from
/// destroying unrelated sessions and means jefe is unaffected when the shared
/// default server dies (e.g. an OS reboot of the default tmux server).
pub fn tmux_command() -> Command {
    let mut cmd = Command::new("tmux");
    let base = tmux_base_args();
    cmd.args(&base);
    cmd
}

fn tmux_cmd_status(args: &[&str], cwd: Option<&str>) -> Result<(), String> {
    let mut cmd = tmux_command();
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

    if let Ok(output) = tmux_command()
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

fn shell_join(parts: &[String]) -> String {
    parts
        .iter()
        .map(|part| shell_escape_single(part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn remote_is_enabled(remote: &crate::domain::RemoteRepositorySettings) -> bool {
    remote.enabled && !remote.host.trim().is_empty() && !remote.login_user.trim().is_empty()
}

fn remote_effective_user(remote: &crate::domain::RemoteRepositorySettings) -> String {
    if remote.run_as_user.trim().is_empty() {
        remote.login_user.trim().to_owned()
    } else {
        remote.run_as_user.trim().to_owned()
    }
}

fn run_command_capture(mut cmd: Command, error_context: &str) -> Result<Output, RuntimeError> {
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| RuntimeError::RemoteExecutionFailed(format!("{error_context}: {e}")))?;

    let deadline = Instant::now() + REMOTE_SSH_COMMAND_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child.wait_with_output().map_err(|e| {
                    RuntimeError::RemoteExecutionFailed(format!("{error_context}: {e}"))
                });
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(RuntimeError::RemoteExecutionFailed(format!(
                        "{error_context}: timed out after {}s",
                        REMOTE_SSH_COMMAND_TIMEOUT.as_secs()
                    )));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RuntimeError::RemoteExecutionFailed(format!(
                    "{error_context}: {e}"
                )));
            }
        }
    }
}

fn remote_ssh_args(
    remote: &crate::domain::RemoteRepositorySettings,
    remote_command: &str,
) -> Vec<String> {
    vec![
        "-o".to_owned(),
        "BatchMode=yes".to_owned(),
        "-o".to_owned(),
        "ConnectTimeout=10".to_owned(),
        "-tt".to_owned(),
        format!("{}@{}", remote.login_user.trim(), remote.host.trim()),
        remote_command.to_owned(),
    ]
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

pub fn build_remote_attach_command(
    remote: &crate::domain::RemoteRepositorySettings,
    session_name: &str,
) -> String {
    let remote_command = remote_tmux_command(
        remote,
        &format!(
            "tmux attach-session -t {}",
            shell_escape_single(session_name)
        ),
    );
    let ssh_args = remote_ssh_args(remote, &remote_command);
    format!("exec ssh {}", shell_join(&ssh_args))
}

pub fn run_remote_ssh(
    remote: &crate::domain::RemoteRepositorySettings,
    remote_command: &str,
) -> Result<Output, RuntimeError> {
    let ssh_args = remote_ssh_args(remote, remote_command);
    let mut cmd = Command::new("ssh");
    cmd.args(&ssh_args);
    run_command_capture(
        cmd,
        &format!("ssh {}@{}", remote.login_user.trim(), remote.host.trim()),
    )
}

fn ensure_remote_success(
    remote: &crate::domain::RemoteRepositorySettings,
    action: &str,
    output: Output,
) -> Result<Output, RuntimeError> {
    if output.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!(
                "remote command failed on {}@{} with status {}",
                remote.login_user.trim(),
                remote.host.trim(),
                output.status
            )
        };
        Err(RuntimeError::RemoteExecutionFailed(format!(
            "{action}: {detail}"
        )))
    }
}

fn resolve_remote_llxprt_command(
    remote: &crate::domain::RemoteRepositorySettings,
    work_dir: &Path,
    setup_env: bool,
) -> Result<String, RuntimeError> {
    let work_dir_string = work_dir.to_string_lossy().into_owned();
    let escaped_work_dir = shell_escape_single(&work_dir_string);
    let path_local = format!("{work_dir_string}/node_modules/.bin/llxprt");
    let escaped_path_local = shell_escape_single(&path_local);

    let resolver_script = format!(
        "set -e; cd {escaped_work_dir}; if command -v llxprt >/dev/null 2>&1; then printf '%s\\n' llxprt; elif [ -x {escaped_path_local} ]; then printf '%s\\n' {escaped_path_local}; else exit 127; fi"
    );
    let resolver_command = remote_tmux_command(remote, &resolver_script);
    let output = run_remote_ssh(remote, &resolver_command)?;
    if output.status.success() {
        let resolved = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !resolved.is_empty() {
            return Ok(resolved);
        }
    }

    if !setup_env {
        return Err(RuntimeError::RemoteExecutionFailed(
            "could not resolve remote llxprt command; llxprt was not installed globally or path-locally, and Setup Env Default is disabled".to_owned(),
        ));
    }

    if setup_env {
        let setup_script = format!(
            "set -e; mkdir -p {escaped_work_dir}; cd {escaped_work_dir}; if ! command -v node >/dev/null 2>&1; then echo 'node is required on the remote host for setup-env' >&2; exit 127; fi; if ! command -v npm >/dev/null 2>&1; then echo 'npm is required on the remote host for setup-env' >&2; exit 127; fi; npm install @vybestack/llxprt-code"
        );
        let setup_command = remote_tmux_command(remote, &setup_script);
        let setup_output = run_remote_ssh(remote, &setup_command)?;
        ensure_remote_success(remote, "remote setup-env", setup_output)?;

        let retry_output = run_remote_ssh(remote, &resolver_command)?;
        if retry_output.status.success() {
            let resolved = String::from_utf8_lossy(&retry_output.stdout)
                .trim()
                .to_owned();
            if !resolved.is_empty() {
                return Ok(resolved);
            }
        }
    }

    Err(RuntimeError::RemoteExecutionFailed(
        "could not resolve remote llxprt command; verify llxprt is installed for the remote user or provide a path-local install in the working directory".to_owned(),
    ))
}

fn launch_args(signature: &LaunchSignature) -> Vec<String> {
    let mut args = Vec::new();
    if !signature.profile.is_empty() {
        args.push("--profile-load".to_owned());
        args.push(signature.profile.clone());
    }
    args.extend(
        signature
            .mode_flags
            .iter()
            .filter(|flag| !flag.is_empty())
            .cloned(),
    );
    if signature.pass_continue {
        args.push("--continue".to_owned());
    }
    if signature.sandbox_enabled {
        args.push("--sandbox".to_owned());
        args.push("--sandbox-engine".to_owned());
        args.push(signature.sandbox_engine.as_llxprt_arg().to_owned());
    }
    args
}

fn remote_env_exports(signature: &LaunchSignature) -> Vec<String> {
    let mut env_exports = Vec::new();
    if signature.sandbox_enabled {
        env_exports.push(format!(
            "export SANDBOX_FLAGS={};",
            shell_escape_single(&signature.sandbox_flags)
        ));
        if let Some(image_ref) = std::env::var_os("LLXPRT_SANDBOX_IMAGE") {
            env_exports.push(format!(
                "export LLXPRT_SANDBOX_IMAGE={};",
                shell_escape_single(&image_ref.to_string_lossy())
            ));
        }
    }
    if !signature.llxprt_debug.is_empty() {
        env_exports.push(format!(
            "export LLXPRT_DEBUG={};",
            shell_escape_single(&signature.llxprt_debug)
        ));
    }
    env_exports
}

fn remote_cli_command(llxprt_command: &str, launch_args: &[String]) -> String {
    let executable = if llxprt_command == "llxprt" {
        llxprt_command.to_owned()
    } else {
        shell_escape_single(llxprt_command)
    };

    if launch_args.is_empty() {
        executable
    } else {
        format!("{} {}", executable, shell_join(launch_args))
    }
}

fn build_remote_launch_command(
    session_name: &str,
    work_dir: &Path,
    signature: &LaunchSignature,
) -> Result<String, RuntimeError> {
    let remote = &signature.remote;
    let work_dir_string = work_dir.to_string_lossy().into_owned();
    let escaped_work_dir = shell_escape_single(&work_dir_string);
    let llxprt_command = resolve_remote_llxprt_command(remote, work_dir, remote.setup_env_default)?;
    let args = launch_args(signature);
    let cli_command = remote_cli_command(&llxprt_command, &args);
    // Scrub jefe's tmux client vars from the remote agent pane for the same
    // reason as the local path (#171): a bare `tmux` inside the agent must not
    // reach the (remote) tmux server hosting the agent session.
    let env_scrub = tmux_scrub_env_args().join(" ");
    let pane_command = format!("{env_scrub} {cli_command}");
    let env_prefix = remote_env_exports(signature).join(" ");
    let escaped_session = shell_escape_single(session_name);
    let tmux_script = build_remote_tmux_script(
        &escaped_work_dir,
        &env_prefix,
        &escaped_session,
        &pane_command,
    );

    Ok(remote_tmux_command(remote, &tmux_script))
}

/// Assemble the remote tmux startup script from its already-escaped parts.
///
/// Factored out of [`build_remote_launch_command`] so the script template —
/// including the `env -u` scrub inside `pane_command` — is unit-testable
/// without the SSH resolver side effect (#171).
fn build_remote_tmux_script(
    escaped_work_dir: &str,
    env_prefix: &str,
    escaped_session: &str,
    pane_command: &str,
) -> String {
    format!(
        "set -e; mkdir -p {escaped_work_dir}; cd {escaped_work_dir}; {env_prefix} tmux new-session -d -s {escaped_session} -c {escaped_work_dir} {pane_command} \\; set-option -t {escaped_session} remain-on-exit on"
    )
}

struct LocalLaunchPlan {
    args: Vec<String>,
    env: Vec<(String, String)>,
    warning: Option<String>,
}

fn local_launch_plan(signature: &LaunchSignature) -> LocalLaunchPlan {
    let mut env = Vec::new();
    let warning = if signature.sandbox_enabled {
        env.push(("SANDBOX_FLAGS".to_owned(), signature.sandbox_flags.clone()));
        if let Some(image_ref) = std::env::var_os("LLXPRT_SANDBOX_IMAGE") {
            env.push((
                "LLXPRT_SANDBOX_IMAGE".to_owned(),
                image_ref.to_string_lossy().into_owned(),
            ));
        }
        sandbox_ssh_agent_warning()
    } else {
        None
    };
    if !signature.llxprt_debug.is_empty() {
        env.push(("LLXPRT_DEBUG".to_owned(), signature.llxprt_debug.clone()));
    }
    LocalLaunchPlan {
        args: launch_args(signature),
        env,
        warning,
    }
}

fn local_launch_command(session_name: &str, work_dir: &Path, plan: &LocalLaunchPlan) -> Command {
    let mut cmd = tmux_command();
    cmd.arg("new-session")
        .arg("-d")
        .arg("-s")
        .arg(session_name)
        .arg("-c")
        .arg(work_dir);

    // Wrap the pane command in `env -u TMUX -u TMUX_PANE -u TMUX_TMPDIR …` so
    // the llxprt child (and any tool it spawns) cannot reach jefe's private
    // tmux server via a bare `tmux` (#171). tmux's server populates the pane
    // env, so the scrub MUST live in the pane command rather than jefe's own
    // process env. The argv is built by the pure [`local_pane_command_args`]
    // helper so it is directly unit-testable.
    for arg in local_pane_command_args(plan) {
        cmd.arg(arg);
    }
    cmd
}

/// Build the pane-command argv for a local agent session: the `env -u` scrub
/// prefix, any `KEY=VALUE` env assignments, then `llxprt` and its launch args.
///
/// Factored out of [`local_launch_command`] so the scrub is unit-testable
/// without spawning tmux or introspecting a `Command` (#171).
fn local_pane_command_args(plan: &LocalLaunchPlan) -> Vec<String> {
    let mut args = tmux_scrub_env_args();
    for (key, value) in &plan.env {
        args.push(format!("{key}={value}"));
    }
    args.push("llxprt".to_owned());
    args.extend(plan.args.iter().cloned());
    args
}

fn finalize_local_session(session_name: &str, warning: Option<String>) {
    enforce_clipboard_passthrough(session_name);
    let _ = tmux_cmd_status(
        ["set-option", "-t", session_name, "remain-on-exit", "on"].as_ref(),
        None,
    );
    apply_session_style(session_name);

    if let Some(warning) = warning {
        debug!(session_name = %session_name, warning = %warning, "runtime launch preflight warning");
        let _ = tmux_cmd_status(
            [
                "display-message",
                "-t",
                session_name,
                &format!("[jefe] warning: {warning}"),
            ]
            .as_ref(),
            None,
        );
    }
}

fn try_local_create_session(
    session_name: &str,
    work_dir: &Path,
    signature: &LaunchSignature,
    attempt: u8,
) -> Result<(), String> {
    let plan = local_launch_plan(signature);
    let mut cmd = local_launch_command(session_name, work_dir, &plan);
    debug!(session_name = %session_name, attempt, "create_session invoking tmux new-session");

    let output = cmd.output().map_err(|e| format!("tmux new-session: {e}"))?;
    if output.status.success() {
        debug!(session_name = %session_name, attempt, "create_session tmux new-session succeeded");
        finalize_local_session(session_name, plan.warning);
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

fn create_remote_session(
    session_name: &str,
    work_dir: &Path,
    signature: &LaunchSignature,
) -> Result<(), RuntimeError> {
    let remote_command = build_remote_launch_command(session_name, work_dir, signature)?;
    let output = run_remote_ssh(&signature.remote, &remote_command)?;
    ensure_remote_success(&signature.remote, "remote tmux new-session", output)?;
    Ok(())
}

fn is_tmux_fork_broken(stderr: &str) -> bool {
    stderr.contains("fork failed") || stderr.contains("Device not configured")
}

fn local_spawn_error(session_name: &str, attempt: u8, stderr: String) -> RuntimeError {
    debug!(session_name = %session_name, attempt, stderr = %stderr, "create_session tmux new-session failed");
    RuntimeError::SpawnFailed(format!("tmux new-session failed: {stderr}"))
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
    debug!(session_name = %session_name, work_dir = %work_dir.display(), "create_session start");
    if remote_is_enabled(&signature.remote) {
        return create_remote_session(session_name, work_dir, signature);
    }

    let _ = kill_session(session_name);
    match try_local_create_session(session_name, work_dir, signature, 0) {
        Ok(()) => return Ok(()),
        Err(stderr) if is_tmux_fork_broken(&stderr) => {
            debug!(session_name = %session_name, attempt = 0, stderr = %stderr, "create_session retrying after tmux fork failure");
            // Scoped recovery: kill only this one target session on the
            // jefe-private socket, then retry. We must NOT call `tmux
            // kill-server` here — that would nuke every jefe session (and,
            // before the dedicated socket, every unrelated user session too)
            // over a transient per-session fork error.
            let _ = kill_session(session_name);
        }
        Err(stderr) => return Err(local_spawn_error(session_name, 0, stderr)),
    }

    match try_local_create_session(session_name, work_dir, signature, 1) {
        Ok(()) => Ok(()),
        Err(stderr) => Err(local_spawn_error(session_name, 1, stderr)),
    }
}

/// Check if a tmux session exists.
#[allow(dead_code)]
pub fn session_exists(session_name: &str) -> bool {
    let output = tmux_command()
        .args(["has-session", "-t", session_name])
        .output();

    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

pub fn remote_session_exists(
    remote: &crate::domain::RemoteRepositorySettings,
    session_name: &str,
) -> Result<bool, RuntimeError> {
    let command = remote_has_session_command(remote, session_name);
    let output = run_remote_ssh(remote, &command)?;
    Ok(output.status.success())
}

/// Capture pane output for a session as plain text lines.
pub fn capture_pane_lines(session_name: &str) -> Option<Vec<String>> {
    let output = tmux_command()
        .args(["capture-pane", "-p", "-t", session_name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    Some(text.lines().map(std::borrow::ToOwned::to_owned).collect())
}

/// Parse the stdout of `tmux list-panes -t <session> -F '#{pane_pid}'` into a
/// single PID.
///
/// Returns the first non-empty trimmed line parsed as a `u32`, or `None` if the
/// output is empty/garbage. Factored out of [`pane_pid`] so the parsing logic is
/// unit-testable without spawning tmux.
#[must_use]
pub fn parse_pane_pid(stdout: &str) -> Option<u32> {
    stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .and_then(|line| line.parse::<u32>().ok())
}

/// Query the PID of the (first) pane in a local tmux session.
///
/// Runs `tmux list-panes -t <session> -F '#{pane_pid}'` against the jefe-private
/// socket. Because `llxprt` runs as the pane's direct command (not a shell
/// wrapper), the returned PID **is** the worker process itself. Local sessions
/// only.
///
/// Returns `None` if tmux is unavailable, the session does not exist, or the
/// output cannot be parsed. This is the PID-fallback input used to detect
/// workers that are still alive after their tmux session is gone.
#[must_use]
pub fn pane_pid(session_name: &str) -> Option<u32> {
    let output = tmux_command()
        .args(["list-panes", "-t", session_name, "-F", "#{pane_pid}"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_pane_pid(&String::from_utf8_lossy(&output.stdout))
}

/// Kill a tmux session.
///
/// @pseudocode component-002 lines 24-25
pub fn kill_session(session_name: &str) -> Result<(), RuntimeError> {
    let output = tmux_command()
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
    let output = tmux_command()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SandboxEngine;

    fn base_signature() -> LaunchSignature {
        LaunchSignature {
            work_dir: std::path::PathBuf::from("/tmp"),
            profile: String::new(),
            mode_flags: Vec::new(),
            llxprt_debug: String::new(),
            pass_continue: true,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: crate::domain::DEFAULT_SANDBOX_FLAGS.to_owned(),
            remote: crate::domain::RemoteRepositorySettings::default(),
        }
    }

    #[test]
    fn llxprt_debug_env_is_omitted_when_empty() {
        let signature = base_signature();
        let mut launch_env: Vec<(String, String)> = Vec::new();

        if signature.sandbox_enabled {
            launch_env.push(("SANDBOX_FLAGS".to_owned(), signature.sandbox_flags.clone()));
        }
        if !signature.llxprt_debug.is_empty() {
            launch_env.push(("LLXPRT_DEBUG".to_owned(), signature.llxprt_debug.clone()));
        }

        assert!(!launch_env.iter().any(|(key, _)| key == "LLXPRT_DEBUG"));
    }

    #[test]
    fn remote_tmux_command_wraps_run_as_user_once() {
        let remote = crate::domain::RemoteRepositorySettings {
            enabled: true,
            login_user: "ubuntu".to_owned(),
            host: "example.com".to_owned(),
            run_as_user: "acoliver".to_owned(),
            setup_env_default: false,
        };

        let command = remote_tmux_command(&remote, "tmux has-session -t 'demo'");
        assert_eq!(
            command,
            "sudo -n su - 'acoliver' -c 'tmux has-session -t '\\''demo'\\'''"
        );
    }

    #[test]
    fn remote_execution_timeout_returns_clear_error() {
        let mut cmd = Command::new("python3");
        cmd.args(["-c", "import time; time.sleep(30)"]);

        let Err(RuntimeError::RemoteExecutionFailed(message)) =
            run_command_capture(cmd, "timeout probe")
        else {
            panic!("expected remote timeout failure");
        };

        assert!(message.contains("timed out"));
        assert!(message.contains("timeout probe"));
    }

    #[test]
    fn llxprt_debug_env_is_included_when_non_empty() {
        let mut signature = base_signature();
        signature.llxprt_debug = "trace=1".to_owned();

        let mut launch_env: Vec<(String, String)> = Vec::new();
        if signature.sandbox_enabled {
            launch_env.push(("SANDBOX_FLAGS".to_owned(), signature.sandbox_flags.clone()));
        }
        if !signature.llxprt_debug.is_empty() {
            launch_env.push(("LLXPRT_DEBUG".to_owned(), signature.llxprt_debug.clone()));
        }

        assert_eq!(
            launch_env
                .into_iter()
                .find(|(key, _)| key == "LLXPRT_DEBUG")
                .map(|(_, value)| value),
            Some("trace=1".to_owned())
        );
    }

    #[test]
    fn remote_launch_command_enables_remain_on_exit() {
        let session_name = shell_escape_single("jefe-agent-test");
        let work_dir = shell_escape_single("/tmp/work");
        let cli_command = shell_escape_single("/tmp/work/node_modules/.bin/llxprt");
        let env_prefix = String::new();

        let tmux_script = format!(
            "set -e; mkdir -p {work_dir}; cd {work_dir}; {env_prefix} tmux new-session -d -s {session_name} -c {work_dir} {cli_command} \\; set-option -t {session_name} remain-on-exit on"
        );

        assert!(tmux_script.contains("tmux new-session -d -s 'jefe-agent-test'"));
        assert!(tmux_script.contains("set-option -t 'jefe-agent-test' remain-on-exit on"));
    }

    /// The `env -u` scrub prefix must strip every tmux client var so an agent's
    /// bare `tmux` can never reach jefe's private server (#171).
    #[test]
    fn tmux_scrub_env_args_strips_all_tmux_client_vars() {
        let args = tmux_scrub_env_args();
        assert_eq!(
            args,
            vec![
                "env".to_owned(),
                "-u".to_owned(),
                "TMUX".to_owned(),
                "-u".to_owned(),
                "TMUX_PANE".to_owned(),
                "-u".to_owned(),
                "TMUX_TMPDIR".to_owned(),
            ]
        );
    }

    /// The local pane command must begin with the `env -u` scrub and place it
    /// before `llxprt`, so the agent child never inherits jefe's tmux handle
    /// (#171). Covers both the no-extra-env and with-env cases.
    #[test]
    fn local_pane_command_scrubs_tmux_env_before_llxprt() {
        let plan_no_env = LocalLaunchPlan {
            args: vec!["--continue".to_owned()],
            env: Vec::new(),
            warning: None,
        };
        let args = local_pane_command_args(&plan_no_env);
        assert_eq!(args[0], "env");
        assert_eq!(args[1], "-u");
        assert_eq!(args[2], "TMUX");
        assert_eq!(args[3], "-u");
        assert_eq!(args[4], "TMUX_PANE");
        assert_eq!(args[5], "-u");
        assert_eq!(args[6], "TMUX_TMPDIR");
        // scrub (7 entries) is immediately followed by llxprt + its args.
        assert_eq!(args[7], "llxprt", "scrub must immediately precede llxprt");
        assert_eq!(args[8], "--continue");

        // With an env assignment, the K=V must sit between the scrub and llxprt.
        let plan_with_env = LocalLaunchPlan {
            args: Vec::new(),
            env: vec![("LLXPRT_DEBUG".to_owned(), "trace=1".to_owned())],
            warning: None,
        };
        let args = local_pane_command_args(&plan_with_env);
        let scrub_end = args
            .windows(2)
            .rposition(|w| w[0] == "-u" && w[1] == "TMUX_TMPDIR");
        assert!(scrub_end.is_some(), "scrub must be present");
        let scrub_end = scrub_end.unwrap_or(0);
        assert_eq!(
            args[scrub_end + 2],
            "LLXPRT_DEBUG=trace=1",
            "env assignment must follow the scrub"
        );
        assert_eq!(
            args[scrub_end + 3],
            "llxprt",
            "llxprt must follow the env assignment"
        );
    }

    /// The remote launch script must prepend the `env -u` scrub to the pane
    /// command so a remote agent cannot reach the (remote) tmux server hosting
    /// it (#171). Exercises the real [`build_remote_tmux_script`] template plus
    /// the real scrub/escape helpers rather than re-deriving the string.
    #[test]
    fn remote_launch_command_scrubs_tmux_env_from_pane() {
        let escaped_work_dir = shell_escape_single("/tmp/work");
        let escaped_session = shell_escape_single("jefe-agent-scrub");
        // Build the pane command from the same production helpers the launcher
        // uses, so a regression in any of them (removing the scrub, reordering
        // the template) is caught here.
        let cli_command = remote_cli_command(
            "/tmp/work/node_modules/.bin/llxprt",
            &launch_args(&base_signature()),
        );
        let env_scrub = tmux_scrub_env_args().join(" ");
        let pane_command = format!("{env_scrub} {cli_command}");

        let tmux_script = build_remote_tmux_script(
            &escaped_work_dir,
            "", // no sandbox/debug env exports in the base signature
            &escaped_session,
            &pane_command,
        );

        // The scrub prefix must appear before the llxprt command in the pane.
        let llxprt_pos = tmux_script.find("/tmp/work/node_modules/.bin/llxprt");
        let scrub_pos = tmux_script.find("env -u TMUX -u TMUX_PANE -u TMUX_TMPDIR");
        assert!(llxprt_pos.is_some(), "cli command should be present");
        assert!(scrub_pos.is_some(), "env scrub prefix should be present");
        assert!(
            scrub_pos < llxprt_pos,
            "env scrub must precede the llxprt command"
        );
    }

    #[test]
    fn sandbox_flags_env_value_is_raw_for_tmux_argv() {
        let key = "SANDBOX_FLAGS";
        let value = "--cpus=2 --memory=12288m --pids-limit=256";
        let arg = format!("{key}={value}");
        // Rust's Command::arg() passes this as a single argv entry to tmux.
        // tmux escapes each argument when constructing its sh -c command, so
        // spaces survive without extra quoting.  Adding shell_escape_single()
        // would embed literal quote characters in the env var value.
        assert_eq!(
            arg,
            "SANDBOX_FLAGS=--cpus=2 --memory=12288m --pids-limit=256"
        );
    }

    #[test]
    fn tmux_base_args_include_config_skip_and_dedicated_socket() {
        let args = tmux_base_args();
        let socket = crate::runtime::jefe_tmux_socket_path();
        assert_eq!(
            args,
            vec![
                "-f".to_owned(),
                "/dev/null".to_owned(),
                "-S".to_owned(),
                socket.to_string_lossy().into_owned(),
            ]
        );
    }

    #[test]
    fn parse_pane_pid_extracts_first_numeric_line() {
        assert_eq!(parse_pane_pid("12345\n"), Some(12_345));
        assert_eq!(parse_pane_pid("  98765  \n"), Some(98_765));
    }

    #[test]
    fn parse_pane_pid_returns_none_for_empty_output() {
        assert_eq!(parse_pane_pid(""), None);
        assert_eq!(parse_pane_pid("   \n  \n"), None);
    }

    #[test]
    fn parse_pane_pid_returns_none_for_garbage() {
        assert_eq!(parse_pane_pid("not-a-pid\n"), None);
        assert_eq!(parse_pane_pid("abc\ndef\n"), None);
    }

    #[test]
    fn is_tmux_fork_broken_classifies_known_messages() {
        assert!(is_tmux_fork_broken("tmux: fork failed"));
        assert!(is_tmux_fork_broken(
            "open terminal failed: Device not configured"
        ));
        assert!(!is_tmux_fork_broken("session already exists"));
        assert!(!is_tmux_fork_broken(""));
    }
}
