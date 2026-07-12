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

use std::process::{Command, Stdio};

use crate::github::{
    AUTH_SCOPES, DeviceCode, GhError, build_auth_login_args, build_auth_login_env,
    parse_device_code,
};

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
    let args = build_auth_login_args(AUTH_SCOPES);
    let env = build_auth_login_env();

    let mut command = Command::new("gh");
    command.args(&args);
    for (key, value) in &env {
        command.env(key, value);
    }
    // stdin closed so gh takes its non-interactive path (no "Press Enter").
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let child = command.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            GhError::NotInstalled
        } else {
            GhError::NetworkError(e.to_string())
        }
    })?;

    // `wait_with_output` drains stdout and stderr concurrently while waiting,
    // avoiding the pipe-buffer deadlock that reading one pipe to EOF before
    // the other can cause on large child output.
    let output = child
        .wait_with_output()
        .map_err(|e| GhError::NetworkError(e.to_string()))?;
    let exit_success = output.status.success();

    let stderr_buf = String::from_utf8_lossy(&output.stderr).to_string();
    let stdout_buf = String::from_utf8_lossy(&output.stdout).to_string();

    // gh writes the device code to stderr; fall back to scanning stdout too
    // (defensive — gh's stream choice has changed across versions).
    let combined = if stderr_buf.is_empty() {
        stdout_buf
    } else {
        stderr_buf.clone()
    };
    let code = parse_device_code(&combined);

    Ok(AuthRunResult {
        code,
        exit_success,
        stderr: stderr_buf,
    })
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
