//! Native-host OpenSSH planning, execution, and failure classification.
//!
//! Local executable and argv construction is kept separate from controlled
//! Unix command strings executed by the remote Linux shell. Every non-PTY SSH
//! operation uses the same bounded executor; interactive attachment consumes
//! the same validated plan through `portable-pty`.

use std::ffi::OsString;
use std::fmt;
use std::io::{Error as IoError, ErrorKind, Read};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::domain::RemoteRepositorySettings;
use crate::local_command::{self, LocalTool, LocalToolError};

/// Maximum duration for a non-interactive SSH operation.
pub const SSH_OPERATION_TIMEOUT: Duration = Duration::from_secs(20);

/// Whether the remote operation requires a terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SshMode {
    /// Disable PTY allocation for probes and file operations.
    NonInteractive,
    /// Force PTY allocation for tmux attachment and lifecycle commands.
    Terminal,
}

/// Cooperative cancellation signal for a running non-interactive SSH process.
#[derive(Clone, Debug, Default)]
pub struct SshCancellation {
    cancelled: Arc<AtomicBool>,
}

impl SshCancellation {
    /// Request cancellation. The executor kills and reaps the SSH child.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// A validated, shell-free local OpenSSH invocation.
#[derive(Clone, PartialEq, Eq)]
pub struct SshPlan {
    executable: PathBuf,
    args: Vec<OsString>,
}

impl fmt::Debug for SshPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SshPlan")
            .field("executable", &"<redacted>")
            .field("argument_count", &self.args.len())
            .finish()
    }
}

impl SshPlan {
    /// Plan an OpenSSH invocation using the configured local executable.
    pub fn new(
        remote: &RemoteRepositorySettings,
        remote_command: &str,
        mode: SshMode,
    ) -> Result<Self, SshError> {
        let executable = local_command::resolve(LocalTool::Ssh).map_err(SshError::LocalTool)?;
        Self::with_executable(executable, remote, remote_command, mode)
    }

    /// Plan an invocation with an explicitly supplied executable.
    pub fn with_executable(
        executable: PathBuf,
        remote: &RemoteRepositorySettings,
        remote_command: &str,
        mode: SshMode,
    ) -> Result<Self, SshError> {
        Ok(Self {
            executable,
            args: Self::arguments(remote, remote_command, mode)?,
        })
    }

    /// Build validated OpenSSH arguments without resolving an executable.
    pub fn arguments(
        remote: &RemoteRepositorySettings,
        remote_command: &str,
        mode: SshMode,
    ) -> Result<Vec<OsString>, SshError> {
        crate::domain::target::validate_remote(remote).map_err(SshError::InvalidSettings)?;
        if !remote.enabled {
            return Err(SshError::InvalidSettings(
                "remote SSH transport is disabled".to_owned(),
            ));
        }
        if remote_command.is_empty() {
            return Err(SshError::InvalidSettings(
                "remote command must not be empty".to_owned(),
            ));
        }
        let mut args = common_args(remote);
        args.push(OsString::from(match mode {
            SshMode::NonInteractive => "-T",
            SshMode::Terminal => "-tt",
        }));
        args.push(OsString::from("--"));
        args.push(OsString::from(format!(
            "{}@{}",
            remote.login_user.trim(),
            remote.host.trim()
        )));
        args.push(OsString::from(remote_command));
        Ok(args)
    }

    /// Return the resolved local OpenSSH executable.
    #[must_use]
    pub fn executable(&self) -> &std::path::Path {
        &self.executable
    }

    /// Return the exact OpenSSH argv entries.
    #[must_use]
    pub fn args(&self) -> &[OsString] {
        &self.args
    }

    /// Construct a process command without a local shell.
    #[must_use]
    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        command.args(&self.args);
        command
    }

    /// Execute a bounded non-interactive plan with optional stdin and cancellation.
    pub fn execute(
        &self,
        stdin: Option<&[u8]>,
        timeout: Duration,
        cancellation: Option<&SshCancellation>,
    ) -> Result<Output, SshError> {
        if cancellation.is_some_and(SshCancellation::is_cancelled) {
            return Err(SshError::Cancelled);
        }
        let mut command = self.command();
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        if stdin.is_some() {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        execute_command(command, stdin, timeout, cancellation)
    }
}

fn common_args(remote: &RemoteRepositorySettings) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("-o"),
        OsString::from("BatchMode=yes"),
        OsString::from("-o"),
        OsString::from("ConnectTimeout=10"),
        OsString::from("-o"),
        OsString::from("StrictHostKeyChecking=accept-new"),
        OsString::from("-o"),
        OsString::from("ServerAliveInterval=5"),
        OsString::from("-o"),
        OsString::from("ServerAliveCountMax=3"),
    ];
    if let Some(port) = remote.port {
        args.extend([OsString::from("-p"), OsString::from(port.to_string())]);
    }
    if !remote.identity_file.as_os_str().is_empty() {
        args.extend([
            OsString::from("-i"),
            remote.identity_file.as_os_str().to_owned(),
        ]);
    }
    for option in &remote.options {
        args.extend([OsString::from("-o"), OsString::from(option)]);
    }
    args
}

fn execute_command(
    mut command: Command,
    stdin: Option<&[u8]>,
    timeout: Duration,
    cancellation: Option<&SshCancellation>,
) -> Result<Output, SshError> {
    let mut child = command
        .spawn()
        .map_err(|error| SshError::Spawn(SshIoError::new(error)))?;
    let stdout = take_pipe(child.stdout.take(), &mut child, "stdout")?;
    let stderr = take_pipe(child.stderr.take(), &mut child, "stderr")?;
    let stdout_reader = read_pipe(stdout);
    let stderr_reader = read_pipe(stderr);
    let stdin_writer = start_stdin_writer(stdin, &mut child)?;
    let deadline = Instant::now() + timeout;

    let status = loop {
        if cancellation.is_some_and(SshCancellation::is_cancelled) {
            return Err(stop_execution(
                &mut child,
                stdout_reader,
                stderr_reader,
                stdin_writer,
                SshError::Cancelled,
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                return Err(stop_execution(
                    &mut child,
                    stdout_reader,
                    stderr_reader,
                    stdin_writer,
                    SshError::Timeout,
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                return Err(stop_execution(
                    &mut child,
                    stdout_reader,
                    stderr_reader,
                    stdin_writer,
                    SshError::Io(SshIoError::new(error)),
                ));
            }
        }
    };

    let stdout = join_reader(stdout_reader);
    let stderr = join_reader(stderr_reader);
    let stdin = join_writer(stdin_writer);
    stdin?;
    Ok(Output {
        status,
        stdout: stdout?,
        stderr: stderr?,
    })
}

fn take_pipe<T>(
    pipe: Option<T>,
    child: &mut std::process::Child,
    name: &str,
) -> Result<T, SshError> {
    pipe.ok_or_else(|| {
        terminate_child(child);
        SshError::Io(SshIoError::message(format!(
            "OpenSSH {name} pipe was unavailable"
        )))
    })
}

fn start_stdin_writer(
    stdin: Option<&[u8]>,
    child: &mut std::process::Child,
) -> Result<Option<std::thread::JoinHandle<std::io::Result<()>>>, SshError> {
    use std::io::Write;

    let Some(bytes) = stdin else {
        return Ok(None);
    };
    let mut pipe = take_pipe(child.stdin.take(), child, "stdin")?;
    let bytes = bytes.to_vec();
    Ok(Some(std::thread::spawn(move || pipe.write_all(&bytes))))
}

fn read_pipe(
    mut pipe: impl Read + Send + 'static,
) -> std::thread::JoinHandle<std::io::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes)?;
        Ok(bytes)
    })
}

fn join_reader(
    reader: std::thread::JoinHandle<std::io::Result<Vec<u8>>>,
) -> Result<Vec<u8>, SshError> {
    reader
        .join()
        .map_err(|_| {
            SshError::Io(SshIoError::message(
                "OpenSSH output reader terminated unexpectedly",
            ))
        })?
        .map_err(|error| SshError::Io(SshIoError::new(error)))
}

fn join_writer(
    writer: Option<std::thread::JoinHandle<std::io::Result<()>>>,
) -> Result<(), SshError> {
    let Some(writer) = writer else {
        return Ok(());
    };
    writer
        .join()
        .map_err(|_| {
            SshError::Io(SshIoError::message(
                "OpenSSH input writer terminated unexpectedly",
            ))
        })?
        .map_err(|error| SshError::Io(SshIoError::new(error)))
}

fn stop_execution(
    child: &mut std::process::Child,
    stdout: std::thread::JoinHandle<std::io::Result<Vec<u8>>>,
    stderr: std::thread::JoinHandle<std::io::Result<Vec<u8>>>,
    stdin: Option<std::thread::JoinHandle<std::io::Result<()>>>,
    error: SshError,
) -> SshError {
    terminate_child(child);
    let _ = join_writer(stdin);
    let _ = join_reader(stdout);
    let _ = join_reader(stderr);
    error
}

fn terminate_child(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Preserved local process error with programmatic OS classification.
#[derive(Clone)]
pub struct SshIoError {
    source: Arc<IoError>,
}

impl SshIoError {
    fn new(source: IoError) -> Self {
        Self {
            source: Arc::new(source),
        }
    }

    fn message(message: impl Into<String>) -> Self {
        Self::new(IoError::other(message.into()))
    }

    /// Return the standard I/O error category.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        self.source.kind()
    }

    /// Return the platform error code when one is available.
    #[must_use]
    pub fn raw_os_error(&self) -> Option<i32> {
        self.source.raw_os_error()
    }
}

impl fmt::Debug for SshIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SshIoError")
            .field("kind", &self.kind())
            .field("raw_os_error", &self.raw_os_error())
            .finish()
    }
}

impl fmt::Display for SshIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.source.fmt(formatter)
    }
}

impl PartialEq for SshIoError {
    fn eq(&self, other: &Self) -> bool {
        self.kind() == other.kind()
            && self.raw_os_error() == other.raw_os_error()
            && self.source.to_string() == other.source.to_string()
    }
}

impl Eq for SshIoError {}

impl std::error::Error for SshIoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

/// Typed SSH planning and execution failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshError {
    /// Local OpenSSH could not be resolved.
    LocalTool(LocalToolError),
    /// Connection settings are incomplete or unsafe.
    InvalidSettings(String),
    /// OpenSSH could not be started.
    Spawn(SshIoError),
    /// Local process I/O failed.
    Io(SshIoError),
    /// The remote host key could not be verified.
    HostKey,
    /// Authentication failed.
    Authentication,
    /// The operation exceeded its deadline.
    Timeout,
    /// The process was cancelled.
    Cancelled,
    /// Remote tmux is unavailable.
    MissingRemoteTmux,
    /// The selected agent runtime is unavailable remotely.
    MissingRemoteRuntime,
    /// A remote operation failed without exposing its payload.
    RemoteCommand { status: Option<i32> },
}

impl fmt::Display for SshError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocalTool(error) => write!(formatter, "OpenSSH unavailable: {error}"),
            Self::InvalidSettings(error) => write!(formatter, "invalid SSH settings: {error}"),
            Self::Spawn(error) => write!(formatter, "could not start OpenSSH: {error}"),
            Self::Io(error) => write!(formatter, "OpenSSH process I/O failed: {error}"),
            Self::HostKey => write!(
                formatter,
                "SSH host-key verification failed; verify known_hosts"
            ),
            Self::Authentication => write!(
                formatter,
                "SSH authentication failed; verify the configured identity"
            ),
            Self::Timeout => write!(formatter, "SSH operation timed out"),
            Self::Cancelled => write!(formatter, "SSH operation was cancelled"),
            Self::MissingRemoteTmux => {
                write!(formatter, "remote tmux is not installed or unavailable")
            }
            Self::MissingRemoteRuntime => write!(
                formatter,
                "remote agent runtime is not installed or unavailable"
            ),
            Self::RemoteCommand { status } => {
                write!(
                    formatter,
                    "remote SSH operation failed with status {status:?}"
                )
            }
        }
    }
}

impl std::error::Error for SshError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::LocalTool(error) => Some(error),
            Self::Spawn(error) | Self::Io(error) => Some(error),
            Self::InvalidSettings(_)
            | Self::HostKey
            | Self::Authentication
            | Self::Timeout
            | Self::Cancelled
            | Self::MissingRemoteTmux
            | Self::MissingRemoteRuntime
            | Self::RemoteCommand { .. } => None,
        }
    }
}

/// Classify redacted OpenSSH diagnostics into actionable failure categories.
#[must_use]
pub fn classify_failure(status: Option<i32>, stderr: &str) -> SshError {
    let diagnostic = stderr.to_ascii_lowercase();
    if status.is_none() {
        SshError::Cancelled
    } else if diagnostic.contains("host key verification failed")
        || diagnostic.contains("remote host identification has changed")
    {
        SshError::HostKey
    } else if diagnostic.contains("permission denied")
        || diagnostic.contains("authentication failed")
    {
        SshError::Authentication
    } else if diagnostic.contains("timed out") || diagnostic.contains("connection timeout") {
        SshError::Timeout
    } else if diagnostic.contains("tmux: command not found")
        || diagnostic.contains("tmux: not found")
    {
        SshError::MissingRemoteTmux
    } else if diagnostic.contains("code-puppy: not found")
        || diagnostic.contains("llxprt: not found")
    {
        SshError::MissingRemoteRuntime
    } else {
        SshError::RemoteCommand { status }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote() -> RemoteRepositorySettings {
        RemoteRepositorySettings {
            enabled: true,
            login_user: "ubuntu".to_owned(),
            host: "linux.example".to_owned(),
            port: Some(2222),
            identity_file: PathBuf::from(r"C:\Keys Ω\agent key"),
            options: vec!["Compression=yes".to_owned()],
            ..RemoteRepositorySettings::default()
        }
    }

    #[test]
    fn plan_preserves_windows_paths_and_remote_command_as_distinct_arguments() {
        let plan = SshPlan::with_executable(
            PathBuf::from(r"C:\Program Files\OpenSSH\ssh.exe"),
            &remote(),
            "tmux attach-session -t 'agent Ω'",
            SshMode::Terminal,
        )
        .unwrap_or_else(|error| panic!("plan SSH: {error}"));
        assert_eq!(
            plan.executable(),
            std::path::Path::new(r"C:\Program Files\OpenSSH\ssh.exe")
        );
        assert!(
            plan.args()
                .contains(&OsString::from(r"C:\Keys Ω\agent key"))
        );
        assert!(plan.args().contains(&OsString::from("2222")));
        assert_eq!(
            plan.args().last(),
            Some(&OsString::from("tmux attach-session -t 'agent Ω'"))
        );
    }

    #[test]
    fn unsafe_local_command_options_are_rejected() {
        let mut settings = remote();
        settings.options = vec!["ProxyCommand=steal-secret".to_owned()];
        let result = SshPlan::with_executable(
            PathBuf::from("ssh"),
            &settings,
            "true",
            SshMode::NonInteractive,
        );
        assert!(matches!(result, Err(SshError::InvalidSettings(_))));
    }

    #[test]
    fn plans_reject_disabled_transport_and_empty_remote_commands() {
        let mut settings = remote();
        settings.enabled = false;
        let disabled = SshPlan::arguments(&settings, "true", SshMode::NonInteractive);
        assert!(matches!(disabled, Err(SshError::InvalidSettings(_))));

        settings.enabled = true;
        let empty = SshPlan::arguments(&settings, "", SshMode::NonInteractive);
        assert!(matches!(empty, Err(SshError::InvalidSettings(_))));
    }

    #[test]
    fn plan_debug_output_redacts_executable_and_command_payload() {
        let plan = SshPlan::with_executable(
            PathBuf::from(r"C:\Program Files\OpenSSH\ssh.exe"),
            &remote(),
            "token=secret",
            SshMode::NonInteractive,
        )
        .unwrap_or_else(|error| panic!("plan SSH: {error}"));
        let diagnostic = format!("{plan:?}");
        assert!(!diagnostic.contains("OpenSSH"));
        assert!(!diagnostic.contains("secret"));
    }

    #[test]
    fn failures_are_typed_and_do_not_retain_sensitive_payloads() {
        assert_eq!(
            classify_failure(Some(255), "Host key verification failed."),
            SshError::HostKey
        );
        assert_eq!(
            classify_failure(Some(255), "Permission denied (publickey)."),
            SshError::Authentication
        );
        assert_eq!(
            classify_failure(None, "secret command"),
            SshError::Cancelled
        );
        assert!(
            !classify_failure(Some(17), "token=secret")
                .to_string()
                .contains("secret")
        );
    }

    #[test]
    fn spawn_errors_preserve_io_kind_and_source() {
        let plan = SshPlan {
            executable: std::env::temp_dir().join("jefe-definitely-missing-ssh-executable"),
            args: Vec::new(),
        };
        let result = plan.execute(None, Duration::from_secs(1), None);
        let Err(SshError::Spawn(error)) = result else {
            panic!("expected typed OpenSSH spawn failure");
        };
        assert_eq!(error.kind(), ErrorKind::NotFound);
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn cancellation_is_typed_before_process_execution() {
        let cancellation = SshCancellation::default();
        cancellation.cancel();
        let plan = SshPlan {
            executable: std::env::current_exe()
                .unwrap_or_else(|error| panic!("resolve test executable: {error}")),
            args: Vec::new(),
        };
        let result = plan.execute(None, Duration::from_secs(1), Some(&cancellation));
        assert_eq!(result, Err(SshError::Cancelled));
    }
}
