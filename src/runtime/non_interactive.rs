//! Non-interactive (single-prompt) agent execution (issue #214 / #359).
//!
//! Used to ask the configured default agent to rewrite an issue draft. Unlike
//! the interactive tmux-pane launch in [`super::commands`], this runs the agent
//! with its `-p`/`--prompt` print mode. The rewritten issue is read from a
//! known temp file so thinking/tool/session noise on stdout cannot pollute the
//! draft (issue #359). Stdout/stderr are used only for failure diagnostics.
//!
//! Two halves:
//! - [`non_interactive_argv`]: pure argv/target construction (unit-tested).
//! - [`run_non_interactive`]: the I/O boundary (resolves the binary, runs it
//!   with a bounded timeout, reads and trims the output file).

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
    // even if it lingers in the configured mode flags. Match by prefix so a
    // parameterized form like --continue=true is also stripped.
    args.extend(signature.mode_flags.iter().filter_map(|flag| {
        let trimmed = flag.trim();
        if trimmed.is_empty() || trimmed.starts_with("--continue") {
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

/// Extract a concise, trimmed stderr excerpt for error diagnostics.
///
/// Returns `None` when stderr is empty or not valid UTF-8 (non-UTF-8 stderr is
/// rare and not worth lossy conversion — the status code already conveys the
/// failure). Bounded so a verbose agent does not flood the user-facing notice.
fn stderr_excerpt(stderr: &[u8]) -> Option<String> {
    const MAX_LEN: usize = 500;
    let text = std::str::from_utf8(stderr).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let taken: String = trimmed.chars().take(MAX_LEN).collect();
    if trimmed.chars().count() > MAX_LEN {
        Some(format!("{taken}…"))
    } else {
        Some(taken)
    }
}

/// Read and validate the rewrite output file written by the agent (issue #359).
///
/// Pure enough for unit tests: no process spawn. Returns trimmed UTF-8 text or
/// a diagnostic that may include `stderr_hint` when the file is missing/empty.
fn read_rewrite_output_file(
    output_path: &Path,
    stderr_hint: Option<&str>,
) -> Result<String, RuntimeError> {
    let with_hint = |base: &str| match stderr_hint.filter(|s| !s.is_empty()) {
        Some(stderr) => format!("{base}; stderr: {stderr}"),
        None => base.to_owned(),
    };
    let text = std::fs::read_to_string(output_path).map_err(|error| {
        RuntimeError::RemoteExecutionFailed(with_hint(&format!(
            "agent did not write rewrite output to {}: {error}",
            output_path.display()
        )))
    })?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(RuntimeError::RemoteExecutionFailed(with_hint(
            "agent wrote an empty rewrite output file",
        )));
    }
    Ok(trimmed.to_owned())
}

/// Run the configured default agent non-interactively in `work_dir`, feeding
/// it `instruction` via `--prompt`, then return the rewritten issue text from
/// `output_path` (issue #359 temp-file protocol).
///
/// Local execution only: non-interactive remote capture requires dedicated
/// SSH plumbing and is out of scope for issue #214.
///
/// # Errors
///
/// Returns a [`RuntimeError`] when the binary cannot be resolved, the process
/// cannot be spawned, it times out, exits non-zero, or the output file is
/// missing/empty.
pub fn run_non_interactive(
    signature: &LaunchSignature,
    work_dir: &Path,
    instruction: &str,
    output_path: &Path,
) -> Result<String, RuntimeError> {
    let (target, args) = non_interactive_argv(signature, instruction);
    let executable = AgentExecutableResolver::current()
        .resolve_target(target)
        .map_err(RuntimeError::AgentExecutable)?;
    let owned_args: Vec<OsString> = args.into_iter().map(OsString::from).collect();
    let mut command = command_for_executable(&executable, &owned_args);
    if work_dir.as_os_str().is_empty() {
        // Defensive fallback: the dispatch layer always resolves a real
        // work_dir (the repository base_dir, or the process cwd). An empty
        // path here is unexpected; fall back to the process working directory
        // rather than leaving the child to inherit an unspecified location.
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
    let stderr_hint = stderr_excerpt(&output.stderr);
    if !output.status.success() {
        let status = output
            .status
            .code()
            .map_or_else(|| "signal".to_owned(), |c| c.to_string());
        let detail = match &stderr_hint {
            Some(stderr) => format!("agent exited with status {status}: {stderr}"),
            None => format!("agent exited with status {status}"),
        };
        return Err(RuntimeError::RemoteExecutionFailed(detail));
    }
    read_rewrite_output_file(output_path, stderr_hint.as_deref())
}

#[cfg(test)]
#[path = "non_interactive_tests.rs"]
mod tests;
