//! Native-Windows psmux driver boundary for the shared TUI harness.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::capture::{PaneStatus, PaneStatusParseError, ScreenCapture, ScrollbackSample};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const MINIMUM_PSMUX_VERSION: PsmuxVersion = PsmuxVersion::new(3, 3, 6);
const PANE_DEAD_FORMAT: &str = "#{pane_dead}";
const HISTORY_SIZE_FORMAT: &str = "#{history_size}";
const TMUX_ENV_VARS_TO_SCRUB: &[&str] = &["TMUX", "TMUX_PANE", "TMUX_TMPDIR"];

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

    pub fn jefe(
        session_name: impl Into<String>,
        jefe_binary: impl Into<PathBuf>,
        config_dir: impl Into<PathBuf>,
        working_dir: impl Into<PathBuf>,
        dims: TmuxPaneSize,
    ) -> Result<Self, TmuxDriverError> {
        let command = vec![
            jefe_binary.into().to_string_lossy().into_owned(),
            "--config".to_string(),
            config_dir.into().to_string_lossy().into_owned(),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TmuxPaneSize {
    pub cols: u16,
    pub rows: u16,
    pub history_limit: u32,
}

impl TmuxPaneSize {
    #[must_use]
    pub const fn new(cols: u16, rows: u16, history_limit: u32) -> Self {
        Self {
            cols,
            rows,
            history_limit,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxSession {
    pub name: String,
    pub cols: u16,
    pub rows: u16,
    pub keep_session: bool,
}

#[derive(Debug, Clone)]
pub struct TmuxDriver {
    executable: PathBuf,
    namespace: String,
}

impl Default for TmuxDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl TmuxDriver {
    #[must_use]
    pub fn new() -> Self {
        Self {
            executable: std::env::var_os("JEFE_PSMUX_BIN")
                .map_or_else(|| PathBuf::from("psmux"), PathBuf::from),
            namespace: unique_namespace(),
        }
    }

    #[must_use]
    pub fn is_available(&self) -> bool {
        self.qualified_version().is_ok()
    }

    #[must_use]
    pub fn diagnostics(&self) -> String {
        let version = self
            .version_output()
            .unwrap_or_else(|error| format!("unavailable ({error})"));
        format!(
            "multiplexer: psmux\nexecutable: {}\npsmux version: {version}\nnamespace: {}\n",
            self.executable.display(),
            self.namespace
        )
    }

    pub fn start_session(
        &self,
        request: &TmuxStartRequest,
    ) -> Result<TmuxSession, TmuxDriverError> {
        request.validate()?;
        self.qualified_version()?;
        let args = new_session_args(request);
        self.run_owned(&args, Some(&request.working_dir))?;
        if let Err(error) = self.configure_session(request) {
            return match self.kill_owned_namespace() {
                Ok(()) => Err(error),
                Err(cleanup) => Err(TmuxDriverError::Cleanup {
                    session: error.to_string(),
                    namespace: cleanup.to_string(),
                }),
            };
        }
        Ok(TmuxSession {
            name: request.session_name.clone(),
            cols: request.cols,
            rows: request.rows,
            keep_session: request.keep_session,
        })
    }

    pub fn cleanup_session(&self, session: &TmuxSession) -> Result<(), TmuxDriverError> {
        if session.keep_session {
            return Ok(());
        }
        let session_cleanup =
            ignore_absent_cleanup(self.run(&["kill-session", "-t", &session.name]));
        let namespace_cleanup = ignore_absent_cleanup(self.kill_owned_namespace());
        match (session_cleanup, namespace_cleanup) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(session), Ok(())) => Err(session),
            (Ok(()), Err(namespace)) => Err(namespace),
            (Err(session), Err(namespace)) => Err(TmuxDriverError::Cleanup {
                session: session.to_string(),
                namespace: namespace.to_string(),
            }),
        }
    }

    pub fn send_line(&self, session: &TmuxSession, line: &str) -> Result<(), TmuxDriverError> {
        self.run(&["send-keys", "-l", "-t", &session.name, "--", line])?;
        self.send_key(session, "Enter")
    }

    pub fn send_key(&self, session: &TmuxSession, key: &str) -> Result<(), TmuxDriverError> {
        if key.is_empty() {
            return Err(invalid_request("key must not be empty"));
        }
        self.run(&["send-keys", "-t", &session.name, "--", key])
    }

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
        self.run_owned(&args, None).map(|_| ())
    }

    pub fn capture_screen(&self, session: &TmuxSession) -> Result<ScreenCapture, TmuxDriverError> {
        let output = self.capture(&["capture-pane", "-p", "-t", &session.name])?;
        Ok(ScreenCapture::new(
            session.rows,
            session.cols,
            output_lines(&output.stdout),
        ))
    }

    pub fn capture_scrollback(
        &self,
        session: &TmuxSession,
        lines: u32,
    ) -> Result<ScrollbackSample, TmuxDriverError> {
        let history_size = self.history_size(session)?;
        let start = format!("-{}", lines.max(1));
        let output = self.capture(&["capture-pane", "-p", "-S", &start, "-t", &session.name])?;
        Ok(ScrollbackSample::new(
            history_size,
            output_lines(&output.stdout),
        ))
    }

    pub fn pane_status(&self, session: &TmuxSession) -> Result<PaneStatus, TmuxDriverError> {
        let output = self.display_message(session, PANE_DEAD_FORMAT)?;
        PaneStatus::parse_tmux_pane_dead(&output).map_err(TmuxDriverError::PaneStatus)
    }

    pub fn history_size(&self, session: &TmuxSession) -> Result<u64, TmuxDriverError> {
        let output = self.display_message(session, HISTORY_SIZE_FORMAT)?;
        output
            .trim()
            .parse::<u64>()
            .map_err(|_| TmuxDriverError::Parse {
                command: "display-message history_size".to_string(),
                value: output.trim().to_string(),
            })
    }

    pub fn copy_mode(&self, session: &TmuxSession, enabled: bool) -> Result<(), TmuxDriverError> {
        if enabled {
            self.run(&["copy-mode", "-t", &session.name])
        } else {
            self.run(&["send-keys", "-t", &session.name, "q"])
        }
    }

    fn configure_session(&self, request: &TmuxStartRequest) -> Result<(), TmuxDriverError> {
        self.run(&[
            "set-option",
            "-t",
            &request.session_name,
            "remain-on-exit",
            "on",
        ])?;
        self.run(&[
            "set-option",
            "-wt",
            &request.session_name,
            "history-limit",
            &request.history_limit.to_string(),
        ])
    }

    fn display_message(
        &self,
        session: &TmuxSession,
        format: &str,
    ) -> Result<String, TmuxDriverError> {
        let output = self.capture(&["display-message", "-p", "-t", &session.name, format])?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn qualified_version(&self) -> Result<PsmuxVersion, TmuxDriverError> {
        let text = self.version_output()?;
        let version =
            PsmuxVersion::parse(&text).map_err(|reason| TmuxDriverError::Incompatible {
                executable: self.executable.clone(),
                reason,
            })?;
        if version < MINIMUM_PSMUX_VERSION {
            return Err(TmuxDriverError::Incompatible {
                executable: self.executable.clone(),
                reason: format!(
                    "found {version}; minimum supported version is {MINIMUM_PSMUX_VERSION}"
                ),
            });
        }
        Ok(version)
    }

    fn version_output(&self) -> Result<String, TmuxDriverError> {
        let output = Command::new(&self.executable)
            .arg("-V")
            .output()
            .map_err(|error| TmuxDriverError::Unavailable {
                executable: self.executable.clone(),
                reason: error.to_string(),
            })?;
        if !output.status.success() {
            return Err(TmuxDriverError::Unavailable {
                executable: self.executable.clone(),
                reason: super::psmux_process::format_output(&output),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn kill_owned_namespace(&self) -> Result<(), TmuxDriverError> {
        self.run(&["kill-server"])
    }

    fn run(&self, args: &[&str]) -> Result<(), TmuxDriverError> {
        let owned = args
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>();
        self.run_owned(&owned, None).map(|_| ())
    }

    fn capture(&self, args: &[&str]) -> Result<Output, TmuxDriverError> {
        let owned = args
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>();
        self.run_owned(&owned, None)
    }

    fn run_owned(&self, args: &[String], cwd: Option<&Path>) -> Result<Output, TmuxDriverError> {
        let command_name = format_command(&self.executable, &self.namespace, args);
        let mut command = Command::new(&self.executable);
        command
            .arg("-f")
            .arg("NUL")
            .arg("-L")
            .arg(&self.namespace)
            .args(args);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for variable in TMUX_ENV_VARS_TO_SCRUB {
            command.env_remove(variable);
        }
        if let Some(directory) = cwd {
            command.current_dir(directory);
        }
        let child = command.spawn().map_err(|error| TmuxDriverError::Spawn {
            command: command_name.clone(),
            reason: error.to_string(),
        })?;
        super::psmux_process::wait_for_command(child, &command_name, COMMAND_TIMEOUT)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TmuxDriverError {
    InvalidRequest(String),
    Unavailable { executable: PathBuf, reason: String },
    Incompatible { executable: PathBuf, reason: String },
    Spawn { command: String, reason: String },
    Timeout { command: String },
    Failed { command: String, stderr: String },
    Parse { command: String, value: String },
    Cleanup { session: String, namespace: String },
    PaneStatus(PaneStatusParseError),
}

impl std::fmt::Display for TmuxDriverError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(reason) => {
                write!(formatter, "invalid psmux driver request: {reason}")
            }
            Self::Unavailable { executable, reason } => write!(
                formatter,
                "psmux is unavailable at '{}': {reason}; install marlocarlo.psmux >= {MINIMUM_PSMUX_VERSION} or set JEFE_PSMUX_BIN",
                executable.display()
            ),
            Self::Incompatible { executable, reason } => write!(
                formatter,
                "incompatible psmux at '{}': {reason}",
                executable.display()
            ),
            Self::Spawn { command, reason } => {
                write!(formatter, "failed to spawn {command}: {reason}")
            }
            Self::Timeout { command } => write!(formatter, "psmux command timed out: {command}"),
            Self::Failed { command, stderr } => {
                write!(formatter, "psmux command failed ({command}): {stderr}")
            }
            Self::Parse { command, value } => {
                write!(formatter, "failed to parse {command} output: '{value}'")
            }
            Self::Cleanup { session, namespace } => write!(
                formatter,
                "psmux cleanup failed: session cleanup: {session}; namespace cleanup: {namespace}"
            ),
            Self::PaneStatus(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for TmuxDriverError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct PsmuxVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

impl PsmuxVersion {
    const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        let token = value
            .split_whitespace()
            .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
            .ok_or_else(|| format!("version output contains no numeric token: {value:?}"))?;
        let mut parts = token.split('.');
        let major = parse_version_part(parts.next(), "major", value)?;
        let minor = parse_version_part(parts.next(), "minor", value)?;
        let patch = parse_version_part(parts.next(), "patch", value)?;
        Ok(Self::new(major, minor, patch))
    }
}

impl std::fmt::Display for PsmuxVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

fn invalid_request(reason: &str) -> TmuxDriverError {
    TmuxDriverError::InvalidRequest(reason.to_string())
}

fn ignore_absent_cleanup(result: Result<(), TmuxDriverError>) -> Result<(), TmuxDriverError> {
    match result {
        Err(TmuxDriverError::Failed { stderr, .. })
            if stderr.to_ascii_lowercase().contains("no server running")
                || stderr.to_ascii_lowercase().contains("no sessions")
                || stderr.to_ascii_lowercase().contains("can't find session") =>
        {
            Ok(())
        }
        other => other,
    }
}

fn parse_version_part(part: Option<&str>, name: &str, source: &str) -> Result<u32, String> {
    let value =
        part.ok_or_else(|| format!("version output has no {name} component: {source:?}"))?;
    value
        .trim_matches(|character: char| !character.is_ascii_digit())
        .parse::<u32>()
        .map_err(|error| format!("invalid {name} version component in {source:?}: {error}"))
}

fn new_session_args(request: &TmuxStartRequest) -> Vec<String> {
    let mut args = vec![
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
    ];
    args.push(windows_command_line(&request.command));
    args
}

fn windows_command_line(arguments: &[String]) -> String {
    let quoted = arguments
        .iter()
        .map(|argument| powershell_quote(argument))
        .collect::<Vec<_>>()
        .join(" ");
    format!("& {quoted}")
}

fn powershell_quote(argument: &str) -> String {
    format!("'{}'", argument.replace('\'', "''"))
}

fn unique_namespace() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("jefe-harness-{}-{nanos:x}-{sequence:x}", std::process::id())
}

fn format_command(executable: &Path, namespace: &str, args: &[String]) -> String {
    format!("{} -L {namespace} {}", executable.display(), args.join(" "))
}

fn output_lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(std::borrow::ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
#[path = "psmux_driver_tests.rs"]
mod tests;
