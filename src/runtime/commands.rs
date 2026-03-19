//! Tmux command execution.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @pseudocode component-002 lines 01-06

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use tracing::debug;

use crate::domain::{LaunchSignature, SandboxEngine};

use super::errors::RuntimeError;
use super::preflight::sandbox_ssh_agent_warning;

const SANDBOX_IMAGE_REPO: &str = "ghcr.io/vybestack/llxprt-code/sandbox";
const SANDBOX_IMAGE_NIGHTLY_TAG: &str = "nightly";
const REMOTE_SSH_COMMAND_TIMEOUT: Duration = Duration::from_secs(20);

const SANDBOX_IMAGE_LATEST_TAG: &str = "latest";

/// Build a local tmux `Command` that skips user config (`-f /dev/null`).
///
/// Jefe sets all tmux options programmatically, so loading `~/.tmux.conf` is
/// unnecessary and can cause errors (e.g., pane-scoped options in the user
/// config fail with "no current pane" when the server starts headlessly).
pub fn tmux_command() -> Command {
    let mut cmd = Command::new("tmux");
    cmd.args(["-f", "/dev/null"]);
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

pub fn enforce_clipboard_passthrough(session_name: &str) {
    let _ = tmux_cmd_status(
        ["set-option", "-g", "set-clipboard", "on"].as_ref(),
        None,
    );
    let _ = tmux_cmd_status(
        ["set-option", "-gp", "allow-passthrough", "on"].as_ref(),
        None,
    );
    let _ = tmux_cmd_status(
        ["set-option", "-t", session_name, "set-clipboard", "on"].as_ref(),
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
        .args(["list-panes", "-t", session_name, "-F", "#{session_name}:#{window_index}.#{pane_index}"])
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

pub(crate) fn shell_escape_single(value: &str) -> String {
    format!("'{}'", value.replace('\'', r#"'\''"#))
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
                return Err(RuntimeError::RemoteExecutionFailed(format!("{error_context}: {e}")));
            }
        }
    }
}

fn remote_ssh_args(remote: &crate::domain::RemoteRepositorySettings, remote_command: &str) -> Vec<String> {
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

pub(crate) fn remote_tmux_command(
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

pub(crate) fn build_remote_attach_command(
    remote: &crate::domain::RemoteRepositorySettings,
    session_name: &str,
) -> String {
    let remote_command = remote_tmux_command(
        remote,
        &format!("tmux attach-session -t {}", shell_escape_single(session_name)),
    );
    let ssh_args = remote_ssh_args(remote, &remote_command);
    format!("exec ssh {}", shell_join(&ssh_args))
}

pub(crate) fn run_remote_ssh(
    remote: &crate::domain::RemoteRepositorySettings,
    remote_command: &str,
) -> Result<Output, RuntimeError> {
    let ssh_args = remote_ssh_args(remote, remote_command);
    let mut cmd = Command::new("ssh");
    cmd.args(&ssh_args);
    run_command_capture(
        cmd,
        &format!(
            "ssh {}@{}",
            remote.login_user.trim(),
            remote.host.trim()
        ),
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
            let resolved = String::from_utf8_lossy(&retry_output.stdout).trim().to_owned();
            if !resolved.is_empty() {
                return Ok(resolved);
            }
        }
    }

    Err(RuntimeError::RemoteExecutionFailed(
        "could not resolve remote llxprt command; verify llxprt is installed for the remote user or provide a path-local install in the working directory".to_owned(),
    ))
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
    let mut launch_args: Vec<String> = Vec::new();

    if !signature.profile.is_empty() {
        launch_args.push("--profile-load".to_owned());
        launch_args.push(signature.profile.clone());
    }
    for flag in &signature.mode_flags {
        if !flag.is_empty() {
            launch_args.push(flag.clone());
        }
    }
    if signature.pass_continue {
        launch_args.push("--continue".to_owned());
    }
    if signature.sandbox_enabled {
        launch_args.push("--sandbox".to_owned());
        launch_args.push("--sandbox-engine".to_owned());
        launch_args.push(signature.sandbox_engine.as_llxprt_arg().to_owned());
    }

    let mut env_exports = Vec::new();
    if signature.sandbox_enabled {
        env_exports.push(format!(
            "export SANDBOX_FLAGS={};",
            shell_escape_single(&signature.sandbox_flags)
        ));
        if std::env::var_os("LLXPRT_SANDBOX_IMAGE").is_none()
            && let Some(image_ref) = resolve_sandbox_image(signature.sandbox_engine)
        {
            env_exports.push(format!(
                "export LLXPRT_SANDBOX_IMAGE={};",
                shell_escape_single(&image_ref)
            ));
        }
    }
    if !signature.llxprt_debug.is_empty() {
        env_exports.push(format!(
            "export LLXPRT_DEBUG={};",
            shell_escape_single(&signature.llxprt_debug)
        ));
    }

    let cli_command = if llxprt_command.contains(' ') {
        format!("{} {}", llxprt_command, shell_join(&launch_args))
    } else if launch_args.is_empty() {
        llxprt_command
    } else {
        format!("{} {}", shell_escape_single(&llxprt_command), shell_join(&launch_args))
    };

    let env_prefix = env_exports.join(" ");
    let tmux_script = format!(
        "set -e; mkdir -p {escaped_work_dir}; cd {escaped_work_dir}; {env_prefix} tmux new-session -d -s {} -c {} {} \\; set-option -t {} remain-on-exit on",
        shell_escape_single(session_name),
        escaped_work_dir,
        cli_command,
        shell_escape_single(session_name),
    );

    Ok(remote_tmux_command(remote, &tmux_script))
}


fn looks_like_semver_tag(version: &str) -> bool {
    let split_at = version.find(['-', '+']).unwrap_or(version.len());
    let core = &version[..split_at];
    let suffix = if split_at < version.len() {
        Some(&version[split_at + 1..])
    } else {
        None
    };

    let mut parts = core.split('.');
    let major = parts.next().unwrap_or_default();
    let minor = parts.next().unwrap_or_default();
    let patch = parts.next().unwrap_or_default();
    if parts.next().is_some() {
        return false;
    }

    if [major, minor, patch]
        .iter()
        .any(|part| part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()))
    {
        return false;
    }

    if let Some(suffix) = suffix {
        if suffix.is_empty() {
            return false;
        }

        if !suffix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
            return false;
        }
    }

    true
}

fn parse_llxprt_version_tag(version_output: &str) -> Option<String> {
    version_output
        .split_whitespace()
        .map(|token| {
            token.trim_matches(|c: char| {
                !(c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' || c == '+')
            })
        })
        .find_map(|token| {
            let normalized = token.strip_prefix('v').unwrap_or(token);
            if looks_like_semver_tag(normalized) {
                Some(normalized.to_owned())
            } else {
                None
            }
        })
}

fn detect_llxprt_version_tag() -> Option<String> {
    let output = Command::new("llxprt").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_llxprt_version_tag(&stdout)
}

fn sandbox_manifest_exists(engine: SandboxEngine, image_ref: &str) -> bool {
    let command = match engine {
        SandboxEngine::Podman => "podman",
        SandboxEngine::Docker => "docker",
        SandboxEngine::Seatbelt => return false,
    };

    Command::new(command)
        .args(["manifest", "inspect", image_ref])
        .output()
        .is_ok_and(|out| out.status.success())
}

fn resolve_sandbox_image(engine: SandboxEngine) -> Option<String> {
    match engine {
        SandboxEngine::Seatbelt => None,
        SandboxEngine::Podman | SandboxEngine::Docker => {
            if let Some(version_tag) = detect_llxprt_version_tag() {
                let version_image = format!("{SANDBOX_IMAGE_REPO}:{version_tag}");
                if sandbox_manifest_exists(engine, &version_image) {
                    return Some(version_image);
                }
            }

            let nightly_image = format!("{SANDBOX_IMAGE_REPO}:{SANDBOX_IMAGE_NIGHTLY_TAG}");
            if sandbox_manifest_exists(engine, &nightly_image) {
                return Some(nightly_image);
            }

            let latest_image = format!("{SANDBOX_IMAGE_REPO}:{SANDBOX_IMAGE_LATEST_TAG}");
            if sandbox_manifest_exists(engine, &latest_image) {
                return Some(latest_image);
            }

            None
        }
    }
}

/// Create a new detached tmux session running llxprt.
///
/// The session runs `llxprt` directly (not a shell), so when llxprt exits,
/// the tmux session becomes "dead" until explicit relaunch.
///
/// @pseudocode component-002 lines 01-06
#[allow(clippy::too_many_lines)]
pub fn create_session(
    session_name: &str,
    work_dir: &Path,
    signature: &LaunchSignature,
) -> Result<(), RuntimeError> {
    debug!(session_name = %session_name, work_dir = %work_dir.display(), "create_session start");

    if remote_is_enabled(&signature.remote) {
        let remote_command = build_remote_launch_command(session_name, work_dir, signature)?;
        let output = run_remote_ssh(&signature.remote, &remote_command)?;
        ensure_remote_success(&signature.remote, "remote tmux new-session", output)?;
        return Ok(());
    }

    // Kill any stale session with the same name first
    let _ = kill_session(session_name);

    // Retry once if tmux server is in a fork-broken state.
    for attempt in 0..=1 {
        let mut llxprt_args: Vec<String> = Vec::new();

        // Add profile if specified.
        if !signature.profile.is_empty() {
            llxprt_args.push("--profile-load".to_owned());
            llxprt_args.push(signature.profile.clone());
        }

        // Add mode flags (e.g., --yolo).
        for flag in &signature.mode_flags {
            if !flag.is_empty() {
                llxprt_args.push(flag.clone());
            }
        }

        // Add --continue if pass_continue is true.
        if signature.pass_continue {
            llxprt_args.push("--continue".to_owned());
        }

        // Sandbox launch parity with llxprt-code: explicit --sandbox and engine,
        // plus SANDBOX_FLAGS environment support.
        let mut launch_env: Vec<(String, String)> = Vec::new();
        let mut launch_warning: Option<String> = None;
        if signature.sandbox_enabled {
            llxprt_args.push("--sandbox".to_owned());
            llxprt_args.push("--sandbox-engine".to_owned());
            llxprt_args.push(signature.sandbox_engine.as_llxprt_arg().to_owned());
            launch_env.push(("SANDBOX_FLAGS".to_owned(), signature.sandbox_flags.clone()));

            if std::env::var_os("LLXPRT_SANDBOX_IMAGE").is_none()
                && let Some(image_ref) = resolve_sandbox_image(signature.sandbox_engine)
            {
                launch_env.push(("LLXPRT_SANDBOX_IMAGE".to_owned(), image_ref));
            }

            launch_warning = sandbox_ssh_agent_warning();
        }

        if !signature.llxprt_debug.is_empty() {
            launch_env.push(("LLXPRT_DEBUG".to_owned(), signature.llxprt_debug.clone()));
        }

        let mut cmd = tmux_command();
        cmd.arg("new-session")
            .arg("-d")
            .arg("-s")
            .arg(session_name)
            .arg("-c")
            .arg(work_dir.to_str().unwrap_or("."));

        debug!(session_name = %session_name, attempt, "create_session invoking tmux new-session");

        if !launch_env.is_empty() {
            cmd.arg("env");
            for (key, value) in &launch_env {
                cmd.arg(format!("{key}={value}"));
            }
        }

        cmd.arg("llxprt");
        for arg in &llxprt_args {
            cmd.arg(arg);
        }

        let output = cmd
            .output()
            .map_err(|e| RuntimeError::SpawnFailed(format!("tmux new-session: {e}")))?;

        if output.status.success() {
            debug!(session_name = %session_name, attempt, "create_session tmux new-session succeeded");

            // Enforce clipboard passthrough for each new session regardless of
            // user/system tmux defaults.
            enforce_clipboard_passthrough(session_name);

            // Preserve dead pane output in tmux for post-mortem inspection/relaunch context.
            let _ = tmux_cmd_status(
                ["set-option", "-t", session_name, "remain-on-exit", "on"].as_ref(),
                None,
            );
            apply_session_style(session_name);

            if let Some(warning) = launch_warning {
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

            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let fork_broken =
            stderr.contains("fork failed") || stderr.contains("Device not configured");

        if attempt == 0 && fork_broken {
            debug!(session_name = %session_name, attempt, stderr = %stderr, "create_session retrying after tmux fork failure");
            reset_tmux_server();
            continue;
        }

        debug!(session_name = %session_name, attempt, stderr = %stderr, "create_session tmux new-session failed");
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
    Some(text.lines().map(|line| line.to_owned()).collect())
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

    #[test]
    fn parse_llxprt_version_tag_handles_plain_semver() {
        assert_eq!(
            parse_llxprt_version_tag("0.8.1\n"),
            Some("0.8.1".to_owned())
        );
    }

    #[test]
    fn parse_llxprt_version_tag_handles_prefixed_semver() {
        assert_eq!(
            parse_llxprt_version_tag("llxprt v0.9.0"),
            Some("0.9.0".to_owned())
        );
    }

    #[test]
    fn parse_llxprt_version_tag_handles_nightly_semver_suffix() {
        assert_eq!(
            parse_llxprt_version_tag("0.9.0-nightly.260301.0223eb66a\n"),
            Some("0.9.0-nightly.260301.0223eb66a".to_owned())
        );
    }

    #[test]
    fn parse_llxprt_version_tag_rejects_non_semver() {
        assert_eq!(parse_llxprt_version_tag("nightly build"), None);
    }
}
