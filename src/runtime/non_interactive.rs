//! Generic definition-driven non-interactive agent execution.

use std::path::Path;
use std::time::Duration;

use crate::domain::{AgentLaunchRequest, Id, TypedValue};

use super::agent_probe::command_for_path;
use super::command_capture::run_command_capture_with_timeout;
use super::errors::RuntimeError;

/// Default wall-clock budget for a single rewrite run.
pub const NON_INTERACTIVE_TIMEOUT: Duration = Duration::new(120, 0);

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

fn read_rewrite_output_file(
    output_path: &Path,
    stderr_hint: Option<&str>,
) -> Result<String, RuntimeError> {
    let with_hint = |base: &str| match stderr_hint.filter(|value| !value.is_empty()) {
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

/// Run the selected definition non-interactively from one immutable launch plan.
pub fn run_non_interactive(
    request: &AgentLaunchRequest,
    work_dir: &Path,
    instruction: &str,
    output_path: &Path,
) -> Result<String, RuntimeError> {
    if request.remote.enabled {
        return Err(RuntimeError::RemoteExecutionFailed(
            "non-interactive remote execution is not supported".to_owned(),
        ));
    }
    let mut request = request.clone();
    request.work_dir = work_dir.to_path_buf();
    request.operation = crate::domain::agent_definition::Operation::Normal;
    let prompt =
        Id::parse("prompt").map_err(|error| RuntimeError::SpawnFailed(error.to_string()))?;
    request
        .values
        .insert(prompt, TypedValue::String(instruction.to_owned()));
    let launch_state = super::launch_compose::observe_launch_state(&request)?;
    let prepared = super::launch_compose::prepare_launch(&request, &launch_state)?;
    let cleared = prepared
        .authorized()
        .prepare_current(&super::ProcessSandboxInspector::new())
        .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))?;
    let plan = cleared.plan();
    let mut command = command_for_path(&plan.executable, plan.executable_wrapper, &plan.argv);
    command
        .envs(plan.env.iter().cloned())
        .current_dir(&plan.cwd);
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
            .map_or_else(|| "signal".to_owned(), |code| code.to_string());
        let detail = stderr_hint.as_ref().map_or_else(
            || format!("agent exited with status {status}"),
            |stderr| format!("agent exited with status {status}: {stderr}"),
        );
        return Err(RuntimeError::RemoteExecutionFailed(detail));
    }
    read_rewrite_output_file(output_path, stderr_hint.as_deref())
}
