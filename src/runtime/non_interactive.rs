//! Non-interactive (single-prompt, capture-stdout) agent execution (issue #214).
//!
//! Used to ask the configured default agent to rewrite an issue draft. Unlike
//! the interactive tmux-pane launch in [`super::commands`], this runs the agent
//! with its `-p`/`--prompt` print mode: it answers one prompt, prints the
//! response to stdout, and exits. No tmux session is created.
//!
//! Two halves:
//! - [`non_interactive_argv`]: pure argv/target construction (unit-tested).
//! - [`run_non_interactive`]: the I/O boundary (resolves the binary, runs it
//!   with a bounded timeout, captures and trims stdout).

use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use crate::domain::{AgentKind, LaunchSignature, LaunchSource, llxprt_launch_source};

use super::agent_executable::{AgentExecutableResolver, AgentExecutableTarget};
use super::agent_launcher::command_for_executable;
use super::command_capture::run_command_capture_with_timeout;
use super::errors::RuntimeError;

/// Default wall-clock budget for a single rewrite run.
///
/// Long enough for a model to study the repo source and produce a structured
/// issue, short enough that a stuck agent does not freeze the composer
/// indefinitely. Two minutes (120 s).
pub const NON_INTERACTIVE_TIMEOUT: Duration = Duration::new(120, 0);

/// Build the non-interactive inner argv for the configured agent.
///
/// Mirrors the per-kind construction in `commands::launch_args` but replaces
/// the interactive instruction (`-i` / positional) with a single `--prompt`
/// argument so the agent runs in print mode and exits after answering.
fn non_interactive_inner_args(signature: &LaunchSignature, instruction: &str) -> Vec<String> {
    match signature.agent_kind {
        AgentKind::CodePuppy => code_puppy_non_interactive_args(signature, instruction),
        AgentKind::Llxprt => llxprt_non_interactive_args(signature, instruction),
    }
}

fn code_puppy_non_interactive_args(signature: &LaunchSignature, instruction: &str) -> Vec<String> {
    let mut args = vec!["--prompt".to_owned(), instruction.to_owned()];
    if !signature.code_puppy_model.trim().is_empty() {
        args.push("--model".to_owned());
        args.push(signature.code_puppy_model.trim().to_owned());
    }
    if let Some(yolo) = signature.code_puppy_yolo {
        args.push("--yolo".to_owned());
        args.push(yolo.to_string());
    }
    args
}

fn llxprt_non_interactive_args(signature: &LaunchSignature, instruction: &str) -> Vec<String> {
    let mut args = Vec::new();
    if !signature.profile.is_empty() {
        args.push("--profile-load".to_owned());
        args.push(signature.profile.clone());
    }
    // Non-interactive rewrite is always a fresh run: never pass --continue
    // even if it lingers in the configured mode flags.
    args.extend(signature.mode_flags.iter().filter_map(|flag| {
        let trimmed = flag.trim();
        if trimmed.is_empty() || trimmed == "--continue" {
            None
        } else {
            Some(flag.clone())
        }
    }));
    if signature.sandbox_enabled {
        args.push("--sandbox".to_owned());
        args.push("--sandbox-engine".to_owned());
        args.push(signature.sandbox_engine.as_llxprt_arg().to_owned());
    }
    args.push("--prompt".to_owned());
    args.push(instruction.to_owned());
    args
}

/// Resolve the executable target and full argv (including the multiplexer
/// wrapper, e.g. `uvx`/`npm`) for a non-interactive run. Pure: no I/O.
///
/// Mirrors `commands::launch_target_and_args` so target resolution is
/// consistent with the interactive launch path.
#[must_use]
pub fn non_interactive_argv(
    signature: &LaunchSignature,
    instruction: &str,
) -> (AgentExecutableTarget, Vec<String>) {
    let inner_args = non_interactive_inner_args(signature, instruction);
    if let Some(from_spec) = crate::domain::code_puppy_uvx_from_spec(&signature.code_puppy_version)
    {
        let mut args = vec![
            "--from".to_owned(),
            from_spec,
            AgentKind::CodePuppy.binary_name().to_owned(),
        ];
        args.extend(inner_args);
        return (AgentExecutableTarget::Uvx, args);
    }
    match llxprt_launch_source(signature.agent_kind, signature.llxprt_version.as_ref()) {
        LaunchSource::Direct => (
            AgentExecutableTarget::Agent(signature.agent_kind),
            inner_args,
        ),
        LaunchSource::NpmBacked(selector) => {
            let mut args = vec![
                "exec".to_owned(),
                "--yes".to_owned(),
                format!("--package={}", selector.package_spec()),
                "--".to_owned(),
                AgentKind::Llxprt.binary_name().to_owned(),
            ];
            args.extend(inner_args);
            (AgentExecutableTarget::Npm, args)
        }
    }
}

/// Run the configured default agent non-interactively in `work_dir`, feeding
/// it `instruction` via `--prompt`, and return the captured, trimmed stdout.
///
/// Local execution only: non-interactive remote capture requires dedicated
/// SSH plumbing and is out of scope for issue #214.
///
/// # Why a direct subprocess, not the secure launch-plan flow
///
/// The interactive agent launch (`commands.rs`) routes through
/// `write_launch_plan`/`run_launch_plan` because it must spawn the agent into
/// a tmux/psmux pane with argv scrubbing and `TMUX` env cleanup. That boundary
/// returns only an `ExitStatus` — it cannot capture the agent's stdout, which
/// is the whole point of a non-interactive rewrite. This path therefore builds
/// a foreground capture subprocess via `command_for_executable` (the same
/// resolver/wrapper logic, minus the pane-launch serialization) and pipes
/// stdout/stderr through `run_command_capture_with_timeout`. The `TMUX` env
/// scrub in the launch-plan exists so a child pane does not inherit the parent
/// multiplexer; a non-interactive `--prompt` run ignores multiplexers, so the
/// scrub does not apply here.
///
/// # Errors
///
/// Returns a [`RuntimeError`] when the binary cannot be resolved, the process
/// cannot be spawned, it times out, or it exits non-zero / produces empty
/// output.
pub fn run_non_interactive(
    signature: &LaunchSignature,
    work_dir: &Path,
    instruction: &str,
) -> Result<String, RuntimeError> {
    let (target, args) = non_interactive_argv(signature, instruction);
    let executable = AgentExecutableResolver::current()
        .resolve_target(target)
        .map_err(RuntimeError::AgentExecutable)?;
    let owned_args: Vec<OsString> = args.into_iter().map(OsString::from).collect();
    let mut command = command_for_executable(&executable, &owned_args);
    if work_dir.as_os_str().is_empty() {
        // An unset work_dir resolves to the current directory; propagate a
        // failure to resolve it rather than running the agent in an
        // arbitrary/unintended location.
        let current = std::env::current_dir().map_err(|_| {
            RuntimeError::SpawnFailed(
                "could not resolve the current directory for the non-interactive run".to_owned(),
            )
        })?;
        command.current_dir(current);
    } else {
        command.current_dir(work_dir);
    }
    command.stdin(std::process::Stdio::null());
    let output = run_command_capture_with_timeout(
        command,
        NON_INTERACTIVE_TIMEOUT,
        "agent rewrite (non-interactive)",
    )?;
    if !output.status.success() {
        return Err(RuntimeError::RemoteExecutionFailed(format!(
            "agent exited with status {}",
            output
                .status
                .code()
                .map_or_else(|| "signal".to_owned(), |c| c.to_string())
        )));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|_| {
        RuntimeError::RemoteExecutionFailed(
            "agent produced non-UTF-8 output that could not be used as an issue draft".to_owned(),
        )
    })?;
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Err(RuntimeError::RemoteExecutionFailed(
            "agent produced no output".to_owned(),
        ));
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
#[path = "non_interactive_tests.rs"]
mod tests;
