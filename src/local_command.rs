//! Explicit local executable resolution and typed subprocess construction.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const WINDOWS_DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";

/// Supported local command-line tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalTool {
    /// Git command-line client.
    Git,
    /// GitHub command-line client.
    Gh,
    /// OpenSSH command-line client.
    Ssh,
    /// Unix `kill` utility used by the local process-identity probe.
    Kill,
    /// Unix `ps` utility used by the macOS process-identity probe.
    Ps,
}

impl LocalTool {
    fn name(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Gh => "gh",
            Self::Ssh => "ssh",
            Self::Kill => "kill",
            Self::Ps => "ps",
        }
    }

    fn override_name(self) -> &'static str {
        match self {
            Self::Git => "JEFE_GIT_BIN",
            Self::Gh => "JEFE_GH_BIN",
            Self::Ssh => "JEFE_SSH_BIN",
            Self::Kill => "JEFE_KILL_BIN",
            Self::Ps => "JEFE_PS_BIN",
        }
    }

    /// Whether this tool must resolve from trusted system directories rather
    /// than the full `PATH`.
    ///
    /// Security-sensitive probe tools (`kill`, `ps`) participate in process
    /// liveness decisions, so a manipulated `PATH` must not silently
    /// substitute an untrusted executable under the selected deployment
    /// policy.
    const fn requires_trusted_path(self) -> bool {
        matches!(self, Self::Kill | Self::Ps)
    }
}

/// Host executable-resolution policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolPlatform {
    Windows,
    Unix,
}

impl ToolPlatform {
    const fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unix
        }
    }
}

/// Failure to resolve a required local tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalToolError {
    /// The executable was not found on `PATH`.
    NotFound { tool: LocalTool },
    /// An explicit executable override does not identify an executable file.
    InvalidOverride { tool: LocalTool, path: PathBuf },
}

impl fmt::Display for LocalToolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { tool } => write!(
                formatter,
                "{} executable not found; install it or set {}",
                tool.name(),
                tool.override_name()
            ),
            Self::InvalidOverride { tool, path } => write!(
                formatter,
                "{} does not identify an executable file: {}",
                tool.override_name(),
                path.display()
            ),
        }
    }
}

impl std::error::Error for LocalToolError {}

/// Resolve a local tool to an explicit executable path.
pub fn resolve(tool: LocalTool) -> Result<PathBuf, LocalToolError> {
    let override_path = std::env::var_os(tool.override_name()).map(PathBuf::from);
    let paths = if tool.requires_trusted_path() {
        trusted_unix_directories()
    } else {
        std::env::var_os("PATH")
            .filter(|value| !value.is_empty())
            .map(|value| {
                std::env::split_paths(&value)
                    .filter(|path| !path.as_os_str().is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    resolve_in(
        tool,
        ToolPlatform::current(),
        &paths,
        std::env::var_os("PATHEXT"),
        override_path,
    )
}

/// Return the portable set of trusted system directories used to resolve
/// security-sensitive probe executables.
///
/// Only canonical absolute system directories are searched, never the full
/// `PATH`, so a manipulated `PATH` cannot substitute an untrusted `kill` or
/// `ps` under the selected deployment policy.
fn trusted_unix_directories() -> Vec<PathBuf> {
    [
        PathBuf::from("/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/sbin"),
        PathBuf::from("/usr/sbin"),
    ]
    .into()
}

/// Construct a command using an explicitly resolved executable.
pub fn command(tool: LocalTool) -> Result<Command, LocalToolError> {
    resolve(tool).map(Command::new)
}

/// Failure from a bounded subprocess invocation.
#[derive(Debug)]
pub enum BoundedRunError {
    /// The subprocess could not be spawned.
    Spawn(std::io::Error),
    /// The deadline elapsed before the subprocess exited; the child was killed.
    Timeout,
    /// The subprocess exited but its output could not be captured.
    Io(std::io::Error),
    /// A piped standard stream was unexpectedly missing.
    Pipe(&'static str),
}

impl fmt::Display for BoundedRunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(error) => write!(formatter, "spawn failed: {error}"),
            Self::Timeout => write!(formatter, "subprocess exceeded its deadline"),
            Self::Io(error) => write!(formatter, "subprocess I/O failed: {error}"),
            Self::Pipe(name) => write!(formatter, "{name} pipe was unavailable"),
        }
    }
}

impl std::error::Error for BoundedRunError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Spawn(error) | Self::Io(error) => Some(error),
            Self::Timeout | Self::Pipe(_) => None,
        }
    }
}

const BOUNDED_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Run a command to completion, killing and reaping it once `timeout` elapses.
///
/// Mirrors the bounded executor in `ssh.rs`: spawn with piped stdout/stderr,
/// poll `try_wait` against an `Instant` deadline, then kill and reap the child
/// on timeout. No stdin is written. Used by the local process-identity probe so
/// a hung or manipulated `kill`/`ps` cannot block startup or the render-path
/// liveness poll indefinitely.
///
/// # Errors
/// Returns [`BoundedRunError::Spawn`] if the child cannot be spawned,
/// [`BoundedRunError::Timeout`] if the deadline elapses, or
/// [`BoundedRunError::Io`] / [`BoundedRunError::Pipe`] if the captured output
/// cannot be collected.
pub fn run_bounded(mut command: Command, timeout: Duration) -> Result<Output, BoundedRunError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(BoundedRunError::Spawn)?;
    let stdout = take_pipe(child.stdout.take(), &mut child, "stdout")?;
    let stderr = take_pipe(child.stderr.take(), &mut child, "stderr")?;
    let stdout_reader = read_pipe(stdout);
    let stderr_reader = read_pipe(stderr);
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                return Err(stop_execution(
                    &mut child,
                    stdout_reader,
                    stderr_reader,
                    BoundedRunError::Timeout,
                ));
            }
            Ok(None) => std::thread::sleep(BOUNDED_POLL_INTERVAL),
            Err(error) => {
                return Err(stop_execution(
                    &mut child,
                    stdout_reader,
                    stderr_reader,
                    BoundedRunError::Io(error),
                ));
            }
        }
    };
    let stdout = join_reader(stdout_reader)?;
    let stderr = join_reader(stderr_reader)?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn take_pipe<T: Read + Send + 'static>(
    pipe: Option<T>,
    child: &mut std::process::Child,
    name: &'static str,
) -> Result<T, BoundedRunError> {
    pipe.ok_or_else(|| {
        terminate_child(child);
        BoundedRunError::Pipe(name)
    })
}

fn read_pipe<T: Read + Send + 'static>(
    mut pipe: T,
) -> std::thread::JoinHandle<std::io::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes)?;
        Ok(bytes)
    })
}

fn join_reader(
    reader: std::thread::JoinHandle<std::io::Result<Vec<u8>>>,
) -> Result<Vec<u8>, BoundedRunError> {
    reader
        .join()
        .map_err(|_| {
            BoundedRunError::Io(std::io::Error::other(
                "output reader terminated unexpectedly",
            ))
        })?
        .map_err(BoundedRunError::Io)
}

fn stop_execution(
    child: &mut std::process::Child,
    stdout: std::thread::JoinHandle<std::io::Result<Vec<u8>>>,
    stderr: std::thread::JoinHandle<std::io::Result<Vec<u8>>>,
    error: BoundedRunError,
) -> BoundedRunError {
    terminate_child(child);
    let _ = join_reader(stdout);
    let _ = join_reader(stderr);
    error
}

fn terminate_child(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn resolve_in(
    tool: LocalTool,
    platform: ToolPlatform,
    paths: &[PathBuf],
    pathext: Option<OsString>,
    override_path: Option<PathBuf>,
) -> Result<PathBuf, LocalToolError> {
    if let Some(path) = override_path {
        if executable_file(&path, platform) {
            return Ok(path);
        }
        return Err(LocalToolError::InvalidOverride { tool, path });
    }
    let candidates = executable_names(tool.name(), platform, pathext.as_deref());
    for directory in paths {
        for candidate in &candidates {
            let path = directory.join(candidate);
            if executable_file(&path, platform) {
                return Ok(path);
            }
        }
    }
    Err(LocalToolError::NotFound { tool })
}

fn executable_names(name: &str, platform: ToolPlatform, pathext: Option<&OsStr>) -> Vec<OsString> {
    if platform == ToolPlatform::Unix {
        return vec![OsString::from(name)];
    }
    let extensions = pathext
        .map(OsStr::to_string_lossy)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| WINDOWS_DEFAULT_PATHEXT.into());
    extensions
        .split(';')
        .filter(|extension| !extension.is_empty())
        .map(|extension| format!("{name}{extension}"))
        .map(OsString::from)
        .collect()
}

fn executable_file(path: &Path, platform: ToolPlatform) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    if platform == ToolPlatform::Unix {
        use std::os::unix::fs::PermissionsExt;
        return path
            .metadata()
            .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0);
    }
    let _ = platform;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn windows_resolves_pathext_in_directory_order_with_spaces_and_unicode() {
        let root = tempfile::Builder::new()
            .prefix("jefe tools Ω ")
            .tempdir()
            .unwrap_or_else(|error| panic!("create tool directory: {error}"));
        let second = root.path().join("second tools");
        std::fs::create_dir_all(&second)
            .unwrap_or_else(|error| panic!("create second tool directory: {error}"));
        let executable = second.join("git.EXE");
        std::fs::write(&executable, b"fixture")
            .unwrap_or_else(|error| panic!("write tool fixture: {error}"));

        let resolved = resolve_in(
            LocalTool::Git,
            ToolPlatform::Windows,
            &[root.path().to_path_buf(), second],
            Some(OsString::from(".CMD;.EXE")),
            None,
        );

        assert_eq!(resolved, Ok(executable));
    }

    #[test]
    fn invalid_explicit_override_is_a_typed_error() {
        let missing = std::env::temp_dir().join("jefe-missing-tool-override.exe");
        let resolved = resolve_in(
            LocalTool::Git,
            ToolPlatform::Windows,
            &[],
            None,
            Some(missing.clone()),
        );
        assert_eq!(
            resolved,
            Err(LocalToolError::InvalidOverride {
                tool: LocalTool::Git,
                path: missing,
            })
        );
    }

    #[test]
    fn explicit_override_is_preserved_as_a_path() {
        let root = tempfile::Builder::new()
            .prefix("jefe override Ω ")
            .tempdir()
            .unwrap_or_else(|error| panic!("create override directory: {error}"));
        let override_path = root.path().join("git.exe");
        std::fs::write(&override_path, b"fixture")
            .unwrap_or_else(|error| panic!("write override fixture: {error}"));
        let resolved = resolve_in(
            LocalTool::Git,
            ToolPlatform::Windows,
            &[],
            None,
            Some(override_path.clone()),
        );
        assert_eq!(resolved, Ok(override_path));
    }

    #[test]
    fn missing_tool_is_a_typed_error() {
        let result = resolve_in(LocalTool::Gh, ToolPlatform::Unix, &[], None, None);
        assert!(matches!(
            result,
            Err(LocalToolError::NotFound {
                tool: LocalTool::Gh
            })
        ));
    }

    #[test]
    fn windows_resolves_openssh_from_unicode_path() {
        let root = tempfile::Builder::new()
            .prefix("jefe OpenSSH Ω ")
            .tempdir()
            .unwrap_or_else(|error| panic!("create OpenSSH fixture: {error}"));
        let executable = root.path().join("ssh.EXE");
        std::fs::write(&executable, b"fixture")
            .unwrap_or_else(|error| panic!("write OpenSSH fixture: {error}"));
        let resolved = resolve_in(
            LocalTool::Ssh,
            ToolPlatform::Windows,
            &[root.path().to_path_buf()],
            Some(OsString::from(".EXE")),
            None,
        );
        assert_eq!(resolved, Ok(executable));
    }

    #[cfg(unix)]
    fn write_unix_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, b"#!/bin/sh\nexit 0\n")
            .unwrap_or_else(|error| panic!("write tool fixture: {error}"));
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|error| panic!("chmod tool fixture: {error}"));
    }

    #[cfg(unix)]
    #[test]
    fn unix_resolves_kill_from_trusted_directory() {
        let root = tempfile::Builder::new()
            .prefix("jefe kill probe ")
            .tempdir()
            .unwrap_or_else(|error| panic!("create kill directory: {error}"));
        let executable = root.path().join("kill");
        write_unix_executable(&executable);
        let resolved = resolve_in(
            LocalTool::Kill,
            ToolPlatform::Unix,
            &[root.path().to_path_buf()],
            None,
            None,
        );
        assert_eq!(resolved, Ok(executable));
    }

    #[cfg(unix)]
    #[test]
    fn unix_resolves_ps_from_trusted_directory() {
        let root = tempfile::Builder::new()
            .prefix("jefe ps probe ")
            .tempdir()
            .unwrap_or_else(|error| panic!("create ps directory: {error}"));
        let executable = root.path().join("ps");
        write_unix_executable(&executable);
        let resolved = resolve_in(
            LocalTool::Ps,
            ToolPlatform::Unix,
            &[root.path().to_path_buf()],
            None,
            None,
        );
        assert_eq!(resolved, Ok(executable));
    }

    #[cfg(unix)]
    #[test]
    fn unix_kill_override_is_preserved_and_invalid_is_rejected() {
        let root = tempfile::Builder::new()
            .prefix("jefe kill override ")
            .tempdir()
            .unwrap_or_else(|error| panic!("create override directory: {error}"));
        let valid = root.path().join("custom-kill");
        write_unix_executable(&valid);
        let resolved = resolve_in(
            LocalTool::Kill,
            ToolPlatform::Unix,
            &[],
            None,
            Some(valid.clone()),
        );
        assert_eq!(resolved, Ok(valid));

        let missing = root.path().join("nope");
        let invalid = resolve_in(
            LocalTool::Kill,
            ToolPlatform::Unix,
            &[],
            None,
            Some(missing.clone()),
        );
        assert_eq!(
            invalid,
            Err(LocalToolError::InvalidOverride {
                tool: LocalTool::Kill,
                path: missing,
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn run_bounded_returns_output_for_a_fast_exiting_command() {
        let mut command = std::process::Command::new("sh");
        command.args(["-c", "printf hello; exit 0"]);
        let output = run_bounded(command, std::time::Duration::from_secs(2))
            .unwrap_or_else(|error| panic!("run_bounded fast-path failed: {error}"));
        assert!(output.status.success());
        assert_eq!(output.stdout, b"hello");
    }

    #[cfg(unix)]
    #[test]
    fn run_bounded_times_out_and_reaps_a_hanging_subprocess() {
        let mut command = std::process::Command::new("sh");
        command.args(["-c", "trap 'exit 0' TERM; sleep 30"]);
        let timeout = std::time::Duration::from_millis(200);
        let result = run_bounded(command, timeout);
        assert!(matches!(result, Err(BoundedRunError::Timeout)));
    }

    #[cfg(unix)]
    #[test]
    fn run_bounded_reports_spawn_failure() {
        let missing = std::env::temp_dir().join("jefe-no-such-probe-binary");
        let command = std::process::Command::new(&missing);
        let result = run_bounded(command, std::time::Duration::from_secs(1));
        assert!(matches!(result, Err(BoundedRunError::Spawn(_))));
    }

    #[cfg(unix)]
    #[test]
    fn unix_probe_tools_resolve_only_from_trusted_system_directories() {
        // Kill and Ps are security-sensitive probe tools: they must resolve
        // only from canonical system directories, never from arbitrary PATH
        // entries that an attacker could influence. This is the trust policy
        // that prevents a manipulated PATH from silently substituting an
        // untrusted probe executable under the selected deployment policy.
        assert!(LocalTool::Kill.requires_trusted_path());
        assert!(LocalTool::Ps.requires_trusted_path());

        // Non-security tools continue to use the full PATH.
        assert!(!LocalTool::Git.requires_trusted_path());
        assert!(!LocalTool::Gh.requires_trusted_path());
        assert!(!LocalTool::Ssh.requires_trusted_path());

        // The trusted directory list contains only canonical absolute system
        // directories — never user-writable or PATH-injected locations.
        let trusted = trusted_unix_directories();
        assert!(trusted.iter().all(|dir| dir.is_absolute()));
        assert!(trusted.iter().any(|dir| dir == &PathBuf::from("/bin")));
        assert!(trusted.iter().any(|dir| dir == &PathBuf::from("/usr/bin")));

        // An untrusted directory is never in the trusted list, so a malicious
        // executable placed there cannot be resolved through resolve().
        let untrusted = tempfile::Builder::new()
            .prefix("jefe untrusted probe ")
            .tempdir()
            .unwrap_or_else(|error| panic!("create untrusted probe directory: {error}"));
        assert!(!trusted.contains(&untrusted.path().to_path_buf()));

        // The real system kill binary resolves from the trusted list when it
        // exists there (CI and dev hosts have /bin/kill or /usr/bin/kill).
        let resolved = resolve_in(LocalTool::Kill, ToolPlatform::Unix, &trusted, None, None);
        if let Ok(path) = resolved {
            let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
            assert!(
                trusted.contains(&parent),
                "kill must resolve from a trusted directory, got {}",
                path.display()
            );
        }
    }
}
