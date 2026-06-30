//! Side-effecting tmux driver boundary for the TUI harness.
//!
//! This is the only harness module that shells out to `tmux`. It exposes
//! single-shot operations; polling and orchestration are owned by later runner
//! phases.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P03
//! @requirement REQ-TMUX-HARNESS-003

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use super::capture::{PaneStatus, PaneStatusParseError, ScreenCapture, ScrollbackSample};

const TMUX_TIMEOUT: Duration = Duration::from_secs(5);
const PANE_DEAD_FORMAT: &str = "#{pane_dead}";
const HISTORY_SIZE_FORMAT: &str = "#{history_size}";

/// Start request for a harness-owned tmux session.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P03
/// @requirement REQ-TMUX-HARNESS-003
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxStartRequest {
    pub session_name: String,
    pub command: Vec<String>,
    pub working_dir: PathBuf,
    pub cols: u16,
    pub rows: u16,
    pub history_limit: u32,
    pub keep_session: bool,
}

impl TmuxStartRequest {
    /// Build a start request for an arbitrary command.
    ///
    /// # Errors
    ///
    /// Returns [`TmuxDriverError::InvalidRequest`] when the request would be
    /// unusable by tmux.
    ///
    /// @plan PLAN-20260629-TMUX-HARNESS.P03
    /// @requirement REQ-TMUX-HARNESS-003
    pub fn command(
        session_name: impl Into<String>,
        command: Vec<String>,
        working_dir: impl Into<PathBuf>,
        cols: u16,
        rows: u16,
        history_limit: u32,
    ) -> Result<Self, TmuxDriverError> {
        let request = Self {
            session_name: session_name.into(),
            command,
            working_dir: working_dir.into(),
            cols,
            rows,
            history_limit,
            keep_session: false,
        };
        request.validate()?;
        Ok(request)
    }

    /// Build a start request for the real jefe binary with an isolated config.
    ///
    /// # Errors
    ///
    /// Returns [`TmuxDriverError::InvalidRequest`] for empty command/session
    /// fields or zero dimensions.
    ///
    /// @plan PLAN-20260629-TMUX-HARNESS.P03
    /// @requirement REQ-TMUX-HARNESS-003
    pub fn jefe(
        session_name: impl Into<String>,
        jefe_binary: impl Into<PathBuf>,
        config_dir: impl Into<PathBuf>,
        working_dir: impl Into<PathBuf>,
        dims: TmuxPaneSize,
    ) -> Result<Self, TmuxDriverError> {
        let config_dir = config_dir.into();
        let command = vec![
            jefe_binary.into().to_string_lossy().into_owned(),
            "--config".to_string(),
            config_dir.to_string_lossy().into_owned(),
        ];
        Self::command(
            session_name,
            command,
            working_dir,
            dims.cols,
            dims.rows,
            dims.history_limit,
        )
    }

    /// Return a copy of this request with keep-session cleanup semantics.
    #[must_use]
    pub fn with_keep_session(mut self, keep_session: bool) -> Self {
        self.keep_session = keep_session;
        self
    }

    fn validate(&self) -> Result<(), TmuxDriverError> {
        if self.session_name.trim().is_empty() {
            return Err(invalid_request("session name must not be empty"));
        }
        if self.command.is_empty() || self.command.iter().any(String::is_empty) {
            return Err(invalid_request("command must contain non-empty argv"));
        }
        if self.cols == 0 || self.rows == 0 || self.history_limit == 0 {
            return Err(invalid_request(
                "cols, rows, and history limit must be non-zero",
            ));
        }
        Ok(())
    }
}

/// Requested pane geometry and history capacity.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P03
/// @requirement REQ-TMUX-HARNESS-003
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TmuxPaneSize {
    pub cols: u16,
    pub rows: u16,
    pub history_limit: u32,
}

impl TmuxPaneSize {
    /// Construct pane dimensions for a harness session.
    #[must_use]
    pub const fn new(cols: u16, rows: u16, history_limit: u32) -> Self {
        Self {
            cols,
            rows,
            history_limit,
        }
    }
}

/// Handle for a tmux session started by the harness.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P03
/// @requirement REQ-TMUX-HARNESS-003
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxSession {
    pub name: String,
    pub cols: u16,
    pub rows: u16,
    pub keep_session: bool,
}

/// Tmux-backed harness driver.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P03
/// @requirement REQ-TMUX-HARNESS-003
#[derive(Debug, Clone, Default)]
pub struct TmuxDriver;

impl TmuxDriver {
    /// Construct a driver using the local `tmux` binary.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Return whether a usable `tmux` binary is on PATH.
    #[must_use]
    pub fn is_available(&self) -> bool {
        tmux_command()
            .arg("-V")
            .output()
            .is_ok_and(|out| out.status.success())
    }

    /// Start a detached tmux session.
    ///
    /// # Errors
    ///
    /// Returns [`TmuxDriverError`] when validation fails or tmux exits with a
    /// non-zero status.
    pub fn start_session(
        &self,
        request: &TmuxStartRequest,
    ) -> Result<TmuxSession, TmuxDriverError> {
        request.validate()?;
        Self::kill_session_if_exists(&request.session_name)?;
        run_tmux_owned(&new_session_args(request), Some(&request.working_dir))?;
        Ok(TmuxSession {
            name: request.session_name.clone(),
            cols: request.cols,
            rows: request.rows,
            keep_session: request.keep_session,
        })
    }

    /// Kill a tmux session unless it was marked keep-session.
    ///
    /// # Errors
    ///
    /// Returns an error when tmux fails to kill a session that should be
    /// removed.
    pub fn cleanup_session(&self, session: &TmuxSession) -> Result<(), TmuxDriverError> {
        if session.keep_session {
            return Ok(());
        }
        Self::kill_session_if_exists(&session.name)
    }

    /// Send a literal line followed by Enter.
    ///
    /// # Errors
    ///
    /// Returns [`TmuxDriverError`] if tmux rejects the key send.
    pub fn send_line(&self, session: &TmuxSession, line: &str) -> Result<(), TmuxDriverError> {
        run_tmux(&["send-keys", "-l", "-t", &session.name, "--", line], None)?;
        self.send_key(session, "Enter")
    }

    /// Send a single named key.
    ///
    /// # Errors
    ///
    /// Returns [`TmuxDriverError`] if tmux rejects the key send.
    pub fn send_key(&self, session: &TmuxSession, key: &str) -> Result<(), TmuxDriverError> {
        if key.is_empty() {
            return Err(invalid_request("key must not be empty"));
        }
        run_tmux(&["send-keys", "-t", &session.name, key], None)
    }

    /// Send multiple named keys in order.
    ///
    /// # Errors
    ///
    /// Returns [`TmuxDriverError`] if any key is empty or tmux rejects the send.
    pub fn send_keys(&self, session: &TmuxSession, keys: &[String]) -> Result<(), TmuxDriverError> {
        if keys.iter().any(String::is_empty) {
            return Err(invalid_request("keys must not contain empty entries"));
        }
        let mut args = vec![
            "send-keys".to_string(),
            "-t".to_string(),
            session.name.clone(),
            "--".to_string(),
        ];
        args.extend(keys.iter().cloned());
        run_tmux_owned(&args, None).map(|_| ())
    }

    /// Capture the current visible pane screen as plain text lines.
    ///
    /// # Errors
    ///
    /// Returns [`TmuxDriverError`] if tmux capture fails.
    pub fn capture_screen(&self, session: &TmuxSession) -> Result<ScreenCapture, TmuxDriverError> {
        let output = run_tmux_capture(&["capture-pane", "-p", "-t", &session.name])?;
        Ok(ScreenCapture::new(
            session.rows,
            session.cols,
            output_lines(&output.stdout),
        ))
    }

    /// Capture a bounded sample of pane history.
    ///
    /// # Errors
    ///
    /// Returns [`TmuxDriverError`] if tmux capture or history-size parsing
    /// fails.
    pub fn capture_scrollback(
        &self,
        session: &TmuxSession,
        lines: u32,
    ) -> Result<ScrollbackSample, TmuxDriverError> {
        let history_size = self.history_size(session)?;
        let start = format!("-{}", lines.max(1));
        let output = run_tmux_capture(&["capture-pane", "-p", "-S", &start, "-t", &session.name])?;
        Ok(ScrollbackSample::new(
            history_size,
            output_lines(&output.stdout),
        ))
    }

    /// Read pane dead/live status.
    ///
    /// # Errors
    ///
    /// Returns [`TmuxDriverError`] if tmux or status parsing fails.
    pub fn pane_status(&self, session: &TmuxSession) -> Result<PaneStatus, TmuxDriverError> {
        let output = Self::display_message(session, PANE_DEAD_FORMAT)?;
        PaneStatus::parse_tmux_pane_dead(&output).map_err(TmuxDriverError::PaneStatus)
    }

    /// Read tmux history size for the session's active pane.
    ///
    /// # Errors
    ///
    /// Returns [`TmuxDriverError`] if tmux fails or emits a non-integer value.
    pub fn history_size(&self, session: &TmuxSession) -> Result<u64, TmuxDriverError> {
        let output = Self::display_message(session, HISTORY_SIZE_FORMAT)?;
        output
            .trim()
            .parse::<u64>()
            .map_err(|_| TmuxDriverError::Parse {
                command: "display-message history_size".to_string(),
                value: output.trim().to_string(),
            })
    }

    /// Enter or exit tmux copy mode.
    ///
    /// # Errors
    ///
    /// Returns [`TmuxDriverError`] if tmux rejects the operation.
    pub fn copy_mode(&self, session: &TmuxSession, enabled: bool) -> Result<(), TmuxDriverError> {
        if enabled {
            run_tmux(&["copy-mode", "-t", &session.name], None)
        } else {
            run_tmux(&["send-keys", "-t", &session.name, "q"], None)
        }
    }

    fn display_message(session: &TmuxSession, format: &str) -> Result<String, TmuxDriverError> {
        let output = run_tmux_capture(&["display-message", "-p", "-t", &session.name, format])?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn kill_session_if_exists(session_name: &str) -> Result<(), TmuxDriverError> {
        match run_tmux_capture(&["has-session", "-t", session_name]) {
            Ok(_) => run_tmux(&["kill-session", "-t", session_name], None),
            Err(err) if is_absent_session_error(&err) => Ok(()),
            Err(err) => Err(err),
        }
    }
}

fn is_absent_session_error(err: &TmuxDriverError) -> bool {
    let TmuxDriverError::Failed { command, stderr } = err else {
        return false;
    };
    command.contains("has-session")
        && (stderr.contains("can't find session")
            || stderr.contains("no server running")
            || (stderr.contains("error connecting")
                && stderr.contains("No such file or directory")))
}

/// Error type for tmux-driver operations.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P03
/// @requirement REQ-TMUX-HARNESS-003
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TmuxDriverError {
    InvalidRequest(String),
    Spawn { command: String, reason: String },
    Timeout { command: String },
    Failed { command: String, stderr: String },
    Parse { command: String, value: String },
    PaneStatus(PaneStatusParseError),
}

impl std::fmt::Display for TmuxDriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(reason) => write!(f, "invalid tmux driver request: {reason}"),
            Self::Spawn { command, reason } => write!(f, "failed to spawn {command}: {reason}"),
            Self::Timeout { command } => write!(f, "tmux command timed out: {command}"),
            Self::Failed { command, stderr } => {
                write!(f, "tmux command failed ({command}): {stderr}")
            }
            Self::Parse { command, value } => {
                write!(f, "failed to parse {command} output: '{value}'")
            }
            Self::PaneStatus(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for TmuxDriverError {}

fn invalid_request(reason: &str) -> TmuxDriverError {
    TmuxDriverError::InvalidRequest(reason.to_string())
}

fn tmux_command() -> Command {
    let mut cmd = Command::new("tmux");
    cmd.args(["-f", "/dev/null"]);
    cmd
}

fn new_session_args(request: &TmuxStartRequest) -> Vec<String> {
    vec![
        "new-session".to_string(),
        "-d".to_string(),
        "-s".to_string(),
        request.session_name.clone(),
        "-x".to_string(),
        request.cols.to_string(),
        "-y".to_string(),
        request.rows.to_string(),
        "-c".to_string(),
        request.working_dir.to_string_lossy().into_owned(),
        tmux_pane_wrapper_command(request),
    ]
}

fn tmux_pane_wrapper_command(request: &TmuxStartRequest) -> String {
    format!(
        "tmux -f /dev/null set-option -pt \"$TMUX_PANE\" remain-on-exit on; tmux -f /dev/null set-option -wt \"$TMUX_PANE\" history-limit {}; exec {}",
        request.history_limit,
        shell_join(&request.command)
    )
}

fn shell_join(parts: &[String]) -> String {
    parts
        .iter()
        .map(|part| shell_escape_single(part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_escape_single(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

fn run_tmux(args: &[&str], cwd: Option<&Path>) -> Result<(), TmuxDriverError> {
    let owned = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    run_tmux_owned(&owned, cwd).map(|_| ())
}

fn run_tmux_capture(args: &[&str]) -> Result<Output, TmuxDriverError> {
    let owned = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    run_tmux_owned(&owned, None)
}

fn run_tmux_owned(args: &[String], cwd: Option<&Path>) -> Result<Output, TmuxDriverError> {
    let command_name = format_command(args);
    let mut cmd = tmux_command();
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let child = cmd.spawn().map_err(|err| TmuxDriverError::Spawn {
        command: command_name.clone(),
        reason: err.to_string(),
    })?;
    wait_for_tmux(child, &command_name)
}

fn wait_for_tmux(
    mut child: std::process::Child,
    command_name: &str,
) -> Result<Output, TmuxDriverError> {
    let deadline = Instant::now() + TMUX_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return tmux_output(child, command_name),
            Ok(None) if Instant::now() >= deadline => {
                return tmux_timeout(&mut child, command_name);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(err) => return tmux_wait_error(&mut child, command_name, err),
        }
    }
}

fn tmux_output(child: std::process::Child, command_name: &str) -> Result<Output, TmuxDriverError> {
    let output = child
        .wait_with_output()
        .map_err(|err| TmuxDriverError::Spawn {
            command: command_name.to_string(),
            reason: err.to_string(),
        })?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(TmuxDriverError::Failed {
            command: command_name.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

fn tmux_timeout(
    child: &mut std::process::Child,
    command_name: &str,
) -> Result<Output, TmuxDriverError> {
    let _ = child.kill();
    let _ = child.wait();
    Err(TmuxDriverError::Timeout {
        command: command_name.to_string(),
    })
}

fn tmux_wait_error(
    child: &mut std::process::Child,
    command_name: &str,
    err: std::io::Error,
) -> Result<Output, TmuxDriverError> {
    let _ = child.kill();
    let _ = child.wait();
    Err(TmuxDriverError::Spawn {
        command: command_name.to_string(),
        reason: err.to_string(),
    })
}

fn output_lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(std::borrow::ToOwned::to_owned)
        .collect()
}

fn format_command(args: &[String]) -> String {
    if args.is_empty() {
        return "tmux -f /dev/null".to_string();
    }
    format!("tmux -f /dev/null {}", args.join(" "))
}

#[cfg(test)]
#[path = "tmux_driver_tests.rs"]
mod tests;
