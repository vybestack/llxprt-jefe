//! One-shot `gh auth login --web` subprocess driver for the in-app device-code
//! auth remediation dialog (issue #244).
//!
//! This is NOT a tmux/PTY session — it is a single short-lived subprocess owned
//! by the runtime layer (the runtime layer owns process orchestration per
//! `dev-docs/standards/persistence-and-runtime.md`). It runs `gh auth login`
//! non-interactively (stdin closed so `gh`'s `CanPrompt()` is false → no
//! "Press Enter" prompt; `GH_BROWSER=/bin/true` so it never spawns a browser),
//! captures stderr, parses the one-time code + URL, and reports exit status.
//!
//! The caller (dispatch layer) runs this off the UI thread via
//! `spawn_gh_task_with_panic` and delivers `AuthCodeReceived` / `AuthSucceeded`
//! / `AuthFailed` events back to the state layer.

use std::io::Read;
use std::process::Stdio;
use std::time::Duration;

use crate::github::{
    AUTH_SCOPES, DeviceCode, GhError, build_auth_login_args, build_auth_login_env,
    parse_device_code,
};

/// Upper bound on how long `run_device_auth` will wait for `gh auth login --web`
/// to exit before killing the child and surfacing a retryable error. Generous
/// by design: it only bounds a hung subprocess (interactive-prompt leak,
/// network stall, CLI bug) — `gh`'s own device-code flow expires server-side
/// well before this and the user is authorizing in parallel (issue #244).
const DEVICE_AUTH_WAIT_SECONDS: u64 = 5 * 60;
const DEVICE_AUTH_WAIT_DEADLINE: Duration = Duration::from_secs(DEVICE_AUTH_WAIT_SECONDS);

/// The outcome of running the device-code auth flow.
///
/// `code` is `Some` when `gh` emitted a parseable one-time code before
/// exiting. `exit_success` is true when `gh` exited 0 (token stored). Both
/// can be true (success after the user authorized); `code` can also be `Some`
/// with `exit_success == false` (the code was shown but then expired or the
/// user denied — `gh` exits non-zero).
#[derive(Debug, Clone)]
pub struct AuthRunResult {
    /// The parsed one-time code + verification URL, if `gh` emitted them.
    pub code: Option<DeviceCode>,
    /// Whether `gh auth login` exited 0 (success / token stored).
    pub exit_success: bool,
    /// Raw captured stderr (for diagnostics on failure).
    pub stderr: String,
}

/// Run the non-interactive device-code auth flow.
///
/// Blocks until `gh auth login --web` exits. Intended to be called inside
/// `smol::unblock` / `spawn_gh_task_with_panic` so it never blocks the UI.
///
/// Errors: returns `GhError::NotInstalled` if `gh` is not on PATH, else
/// `GhError::NetworkError` for spawn failures. Subprocess non-zero exit is NOT
/// an `Err` — it is reported via `AuthRunResult::exit_success` so the caller
/// can surface a retryable failure in-dialog.
pub fn run_device_auth() -> Result<AuthRunResult, GhError> {
    let mut command =
        crate::local_command::command(crate::local_command::LocalTool::Gh).map_err(|error| {
            match error {
                crate::local_command::LocalToolError::NotFound { .. } => GhError::NotInstalled,
                crate::local_command::LocalToolError::InvalidOverride { .. } => {
                    GhError::ToolResolution(error.to_string())
                }
            }
        })?;
    command.args(build_auth_login_args(AUTH_SCOPES));
    for (key, value) in &build_auth_login_env() {
        command.env(key, value);
    }
    // stdin closed so gh takes its non-interactive path (no "Press Enter").
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            GhError::NotInstalled
        } else {
            GhError::NetworkError(e.to_string())
        }
    })?;

    // Drain stdout/stderr on reader threads so neither pipe can fill its kernel
    // buffer and deadlock the child, while the main thread polls `try_wait`
    // against a deadline (see `wait_with_deadline`).
    let stderr_thread = spawn_pipe_reader(child.stderr.take());
    let stdout_thread = spawn_pipe_reader(child.stdout.take());

    // On timeout `wait_with_deadline` kills the child, so the reader threads
    // return promptly once their pipes close. Join them on BOTH paths so we
    // never leak detached reader threads on an error (issue #244 OCR review).
    let exit_status = wait_with_deadline(&mut child);
    let stderr_buf = stderr_thread.join().unwrap_or_default();
    let stdout_buf = stdout_thread.join().unwrap_or_default();
    let exit_success = exit_status?.success();

    // gh writes the device code to stderr; fall back to scanning stdout too
    // (defensive — gh's stream choice has changed across versions).
    let combined: &str = if stderr_buf.is_empty() {
        &stdout_buf
    } else {
        &stderr_buf
    };
    let code = parse_device_code(combined);

    Ok(AuthRunResult {
        code,
        exit_success,
        stderr: stderr_buf,
    })
}

/// Spawn a thread that drains a piped child stream to a `String`. Returns an
/// empty-string thread if the pipe was not captured. Reading on a dedicated
/// thread (one per pipe) prevents a pipe-buffer deadlock when the child writes
/// more than the kernel buffer to one stream while we wait on the other.
fn spawn_pipe_reader<R: Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        pipe.map(|mut s| {
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            buf
        })
        .unwrap_or_default()
    })
}

/// Poll `child.try_wait()` against `DEVICE_AUTH_WAIT_DEADLINE`. On expiry, kill
/// the child and return a retryable `GhError::NetworkError` so the dispatch
/// layer surfaces the failure in-dialog instead of blocking the worker thread
/// forever (issue #244 OCR review: hang-safety).
fn wait_with_deadline(
    child: &mut std::process::Child,
) -> Result<std::process::ExitStatus, GhError> {
    let deadline = std::time::Instant::now() + DEVICE_AUTH_WAIT_DEADLINE;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| GhError::NetworkError(e.to_string()))?
        {
            return Ok(status);
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            return Err(GhError::NetworkError(format!(
                "gh auth login did not finish within {} seconds",
                DEVICE_AUTH_WAIT_DEADLINE.as_secs()
            )));
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_run_result_holds_code_and_status() {
        let result = AuthRunResult {
            code: Some(DeviceCode {
                code: "1234-5678".to_string(),
                verification_url: "https://github.com/login/device".to_string(),
            }),
            exit_success: true,
            stderr: "! First copy your one-time code: 1234-5678".to_string(),
        };
        assert!(result.exit_success);
        let code = result
            .code
            .as_ref()
            .unwrap_or_else(|| panic!("code must be present on success"));
        assert_eq!(code.code, "1234-5678");
    }

    #[test]
    fn auth_run_result_allows_failure_without_code() {
        let result = AuthRunResult {
            code: None,
            exit_success: false,
            stderr: "error: could not connect".to_string(),
        };
        assert!(!result.exit_success);
        assert!(result.code.is_none());
    }
}
