//! Tmux command execution.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @pseudocode component-002 lines 01-06

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use tracing::debug;

use crate::domain::{AgentKind, LaunchSignature};

use super::agent_executable::AgentExecutableResolver;
use super::errors::RuntimeError;
use super::multiplexer::{MultiplexerCapability, MultiplexerPlan};
use super::preflight::sandbox_ssh_agent_warning;

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

fn tmux_cmd_status(args: &[&str], cwd: Option<&str>) -> Result<(), String> {
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

fn configure_prefix_with(
    session_name: &str,
    mut apply: impl FnMut(&[&str]) -> Result<(), String>,
) -> Result<(), String> {
    for option in prefix_options_for_passthrough() {
        let value = if *option == "prefix" {
            local_prefix_value()
        } else {
            "None"
        };
        if cfg!(windows) {
            apply(["set-option", "-g", option, value].as_ref())?;
        } else {
            apply(["set-option", "-t", session_name, option, value].as_ref())?;
        }
    }
    Ok(())
}

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

fn shell_join(parts: &[String]) -> String {
    parts
        .iter()
        .map(|part| shell_escape_single(part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn remote_is_enabled(remote: &crate::domain::RemoteRepositorySettings) -> bool {
    // Delegate to the shared validated contract in domain::target so the
    // runtime layer's definition of "remote" can never drift from the
    // availability/prep layers. The shared predicate requires enabled AND
    // nonempty login_user AND nonempty host.
    crate::domain::target::is_valid_remote(remote)
}

fn remote_effective_user(remote: &crate::domain::RemoteRepositorySettings) -> String {
    if remote.run_as_user.trim().is_empty() {
        remote.login_user.trim().to_owned()
    } else {
        remote.run_as_user.trim().to_owned()
    }
}

pub(super) fn run_command_capture(
    cmd: Command,
    error_context: &str,
) -> Result<Output, RuntimeError> {
    run_command_capture_with_timeout(cmd, REMOTE_SSH_COMMAND_TIMEOUT, error_context)
}

/// [`run_command_capture`] with an injectable deadline so tests can drive the
/// timeout branch with a sub-second value instead of the 20s production
/// default (#173).
fn run_command_capture_with_timeout(
    mut cmd: Command,
    timeout: Duration,
    error_context: &str,
) -> Result<Output, RuntimeError> {
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| RuntimeError::RemoteExecutionFailed(format!("{error_context}: {e}")))?;

    let deadline = Instant::now() + timeout;
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
                        timeout.as_secs_f64()
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
    // Runtime defense-in-depth: validate SSH identity fields before
    // constructing the destination. The authoritative validation happens at
    // form/persistence boundaries via domain::target::validate_remote, but
    // every SSH command site re-checks at runtime (not just debug builds) so
    // a stale or unvalidated RemoteRepositorySettings can never reach the
    // shell. The `--` separator below is the final structural guard: it ends
    // option parsing so a destination starting with '-' cannot be parsed as
    // an ssh option even if validation were bypassed.
    let user = remote.login_user.trim();
    let host = remote.host.trim();
    assert!(
        crate::domain::target::is_valid_ssh_identity(user)
            && crate::domain::target::is_valid_ssh_identity(host),
        "SSH identity fields must be validated before reaching remote_ssh_args"
    );
    vec![
        "-o".to_owned(),
        "BatchMode=yes".to_owned(),
        "-o".to_owned(),
        "ConnectTimeout=10".to_owned(),
        // Auto-accept the host key on first connect (TOFU) and verify it on
        // subsequent connections so SSH never hangs waiting for interactive
        // acceptance in the non-PTY runtime path.
        "-o".to_owned(),
        "StrictHostKeyChecking=accept-new".to_owned(),
        // Post-connect keepalive so a hung remote command is detected within
        // ~15s instead of blocking indefinitely.
        "-o".to_owned(),
        "ServerAliveInterval=5".to_owned(),
        "-o".to_owned(),
        "ServerAliveCountMax=3".to_owned(),
        "-tt".to_owned(),
        // `--` ends option parsing so a destination starting with '-' cannot
        // be misinterpreted as an ssh option (defense in depth; validation
        // is the primary guard).
        "--".to_owned(),
        format!("{user}@{host}"),
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

fn resolve_remote_agent_command(
    remote: &crate::domain::RemoteRepositorySettings,
    work_dir: &Path,
    setup_env: bool,
    agent_kind: AgentKind,
) -> Result<String, RuntimeError> {
    match agent_kind {
        AgentKind::CodePuppy => resolve_remote_code_puppy_command(remote, work_dir),
        AgentKind::Llxprt => resolve_remote_llxprt_command(remote, work_dir, setup_env),
    }
}

fn resolve_remote_code_puppy_command(
    remote: &crate::domain::RemoteRepositorySettings,
    work_dir: &Path,
) -> Result<String, RuntimeError> {
    let work_dir = shell_escape_single(&work_dir.to_string_lossy());
    let script = format!(
        "set -e; cd {work_dir}; command -v code-puppy >/dev/null 2>&1; printf '%s\\n' code-puppy"
    );
    let output = run_remote_ssh(remote, &remote_tmux_command(remote, &script))?;
    let resolved = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if output.status.success() && !resolved.is_empty() {
        Ok(resolved)
    } else {
        Err(RuntimeError::RemoteExecutionFailed(
            "could not resolve remote code-puppy command; verify code-puppy is installed for the remote user".to_owned(),
        ))
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
    match signature.agent_kind {
        AgentKind::CodePuppy => code_puppy_launch_args(signature),
        AgentKind::Llxprt => llxprt_launch_args(signature),
    }
}

fn code_puppy_launch_args(signature: &LaunchSignature) -> Vec<String> {
    // Code Puppy interactive mode: output `-i`, an optional explicit model,
    // and, for fresh (issue/PR-driven) sends, one positional instruction.
    //
    // Fresh sends replace mode_flags with one positional instruction and
    // force pass_continue off. That structural contract avoids coupling the
    // runtime layer to natural-language prompt text while still rejecting all
    // arbitrary persisted LLxprt flags.
    let mut args = vec!["-i".to_owned()];
    if signature.code_puppy_quick_resume {
        args.push("--quick-resume".to_owned());
        args.push(signature.work_dir.to_string_lossy().into_owned());
    }
    if !signature.code_puppy_model.trim().is_empty() {
        args.push("--model".to_owned());
        args.push(signature.code_puppy_model.trim().to_owned());
    }
    if let Some(yolo) = signature.code_puppy_yolo {
        args.push("--yolo".to_owned());
        args.push(yolo.to_string());
    }
    if !signature.pass_continue
        && let [instruction] = signature.mode_flags.as_slice()
        && !instruction.starts_with('-')
    {
        args.push(instruction.clone());
    }
    args
}

fn llxprt_launch_args(signature: &LaunchSignature) -> Vec<String> {
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
    if signature.agent_kind == AgentKind::CodePuppy {
        return env_exports;
    }
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
    let agent_command = resolve_remote_agent_command(
        remote,
        work_dir,
        remote.setup_env_default,
        signature.agent_kind,
    )?;
    let args = launch_args(signature);
    let cli_command = remote_cli_command(&agent_command, &args);
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
    // Disable the tmux prefix on the remote session using the shared
    // [`prefix_disable_tmux_subcommands`] builder so this inline creation
    // script and the reattach fragment ([`remote_disable_prefix_fragment`])
    // format the option sequence identically and cannot drift (#200). The
    // remote tmux server also defaults to `C-b`, which the remote attach
    // client would consume before it reaches the agent; jefe never needs a
    // user-facing prefix on its managed sessions. The sub-commands continue
    // the `tmux new-session` invocation via the `\;` separator.
    let prefix_options = format!(" \\; {}", prefix_disable_tmux_subcommands(escaped_session));
    format!(
        "set -e; mkdir -p {escaped_work_dir}; cd {escaped_work_dir}; {env_prefix} tmux new-session -d -s {escaped_session} -c {escaped_work_dir} {pane_command} \\; set-option -t {escaped_session} remain-on-exit on{prefix_options}"
    )
}

struct LocalLaunchPlan {
    agent_kind: AgentKind,
    args: Vec<String>,
    env: Vec<(String, String)>,
    warning: Option<String>,
}

fn local_launch_plan(signature: &LaunchSignature) -> LocalLaunchPlan {
    let mut env = Vec::new();
    let warning = match signature.agent_kind {
        AgentKind::Llxprt => {
            if signature.sandbox_enabled {
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
            }
        }
        AgentKind::CodePuppy => None,
    };
    if matches!(signature.agent_kind, AgentKind::Llxprt) && !signature.llxprt_debug.is_empty() {
        env.push(("LLXPRT_DEBUG".to_owned(), signature.llxprt_debug.clone()));
    }
    LocalLaunchPlan {
        agent_kind: signature.agent_kind,
        args: launch_args(signature),
        env,
        warning,
    }
}

fn local_launch_command(
    session_name: &str,
    work_dir: &Path,
    launch: &LocalLaunchPlan,
) -> Result<Command, RuntimeError> {
    let multiplexer = MultiplexerPlan::current().map_err(RuntimeError::Multiplexer)?;
    let mut cmd = multiplexer.command();
    cmd.arg("new-session")
        .arg("-d")
        .arg("-s")
        .arg(session_name)
        .arg("-c")
        .arg(work_dir);

    let executable = AgentExecutableResolver::current()
        .resolve(launch.agent_kind)
        .map_err(RuntimeError::AgentExecutable)?;
    let pane_args = launch.args.iter().map(OsString::from).collect::<Vec<_>>();
    let environment = launch
        .env
        .iter()
        .map(|(key, value)| (OsString::from(key), OsString::from(value)))
        .collect::<Vec<_>>();
    for arg in multiplexer
        .agent_pane_command_args(&executable, &pane_args, &environment)
        .map_err(RuntimeError::Multiplexer)?
    {
        cmd.arg(arg);
    }
    Ok(cmd)
}

/// Build the Unix pane-command argv for remote shell construction and
/// regression tests. Local runtime launch uses `MultiplexerPlan::pane_command_args`
/// so native Windows never receives this Unix `env -u` prefix.
#[cfg(test)]
fn local_pane_command_args(plan: &LocalLaunchPlan) -> Vec<String> {
    let mut args = tmux_scrub_env_args();
    for (key, value) in &plan.env {
        args.push(format!("{key}={value}"));
    }
    args.push(plan.agent_kind.binary_name().to_owned());
    args.extend(plan.args.iter().cloned());
    args
}

fn finalize_local_session(session_name: &str, warning: Option<String>) {
    enforce_clipboard_passthrough(session_name);
    if let Err(error) = configure_prefix_for_passthrough(session_name) {
        debug!(session_name = %session_name, error = %error, "prefix passthrough option failed on create; will retry on attach");
    }
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

enum LocalCreateFailure {
    Runtime(RuntimeError),
    Command(String),
}

fn try_local_create_session(
    session_name: &str,
    work_dir: &Path,
    signature: &LaunchSignature,
    attempt: u8,
) -> Result<(), LocalCreateFailure> {
    let plan = local_launch_plan(signature);
    let mut cmd =
        local_launch_command(session_name, work_dir, &plan).map_err(LocalCreateFailure::Runtime)?;
    debug!(session_name = %session_name, attempt, "create_session invoking local multiplexer new-session");

    let output = cmd
        .output()
        .map_err(|error| LocalCreateFailure::Command(error.to_string()))?;
    if output.status.success() {
        debug!(session_name = %session_name, attempt, "create_session local multiplexer new-session succeeded");
        finalize_local_session(session_name, plan.warning);
        Ok(())
    } else {
        Err(LocalCreateFailure::Command(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
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

    MultiplexerPlan::current()
        .and_then(|plan| {
            plan.preflight(&[
                MultiplexerCapability::AttachSession,
                MultiplexerCapability::PaneCapture,
            ])
        })
        .map_err(RuntimeError::Multiplexer)?;

    let _ = kill_session(session_name);
    match try_local_create_session(session_name, work_dir, signature, 0) {
        Ok(()) => return Ok(()),
        Err(LocalCreateFailure::Runtime(error)) => return Err(error),
        Err(LocalCreateFailure::Command(stderr)) if is_tmux_fork_broken(&stderr) => {
            debug!(session_name = %session_name, attempt = 0, stderr = %stderr, "create_session retrying after multiplexer fork failure");
            // Scoped recovery: kill only this one target session in Jefe's
            // private isolation handle. Never terminate the whole server.
            let _ = kill_session(session_name);
        }
        Err(LocalCreateFailure::Command(stderr)) => {
            return Err(local_spawn_error(session_name, 0, stderr));
        }
    }

    match try_local_create_session(session_name, work_dir, signature, 1) {
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
    Ok(output.status.success())
}

/// Kill a tmux session.
///
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

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
#[path = "prefix_passthrough_tests.rs"]
mod prefix_passthrough_tests;
