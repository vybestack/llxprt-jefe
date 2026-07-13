//! Platform-aware local multiplexer resolution, isolation, and dependency probing.
//!
//! Unix uses upstream tmux on Jefe's private socket. Native Windows uses psmux
//! on a private `-L` namespace. Remote SSH command construction intentionally
//! remains in `runtime::commands` and does not use this local policy.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use super::agent_executable::ResolvedAgentExecutable;
use super::agent_launcher::{AgentLauncherError, INTERNAL_LAUNCH_ARGUMENT, write_launch_plan};
const MINIMUM_PSMUX_VERSION: MultiplexerVersion = MultiplexerVersion::new(3, 3, 6);
const WINDOWS_INSTALL_GUIDANCE: &str =
    "install psmux 3.3.6 or newer with `winget install marlocarlo.psmux`, then restart Jefe";
const UNIX_INSTALL_GUIDANCE: &str =
    "install upstream tmux with your operating system package manager";

/// Local operating-system policy used to select a multiplexer implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalPlatform {
    /// Upstream tmux with Unix-domain-socket isolation.
    Unix,
    /// Native psmux with named-namespace isolation.
    Windows,
}

impl LocalPlatform {
    /// Return the policy for the current compilation target.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unix
        }
    }
}

/// Isolation handle owned by Jefe's local multiplexer runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultiplexerIsolation {
    /// Private upstream-tmux Unix socket.
    Socket(PathBuf),
    /// Private native-psmux namespace.
    Namespace(String),
}

/// Multiplexer behavior that callers may require before launching a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiplexerCapability {
    /// Isolation via an explicitly named psmux namespace.
    NamespaceIsolation,
    /// Isolation via an explicit upstream-tmux socket.
    SocketIsolation,
    /// Interactive client attachment.
    AttachSession,
    /// Pane capture and introspection.
    PaneCapture,
}

/// Parsed tmux-compatible semantic version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MultiplexerVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

impl MultiplexerVersion {
    /// Construct a parsed version.
    #[must_use]
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Parse output such as `tmux 3.3.6`.
    pub fn parse(output: &str) -> Result<Self, MultiplexerError> {
        let token = output
            .split_whitespace()
            .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
            .ok_or_else(|| MultiplexerError::MalformedVersion {
                path: None,
                output: output.to_owned(),
            })?;
        let mut parts = token.split('.');
        let major = parse_version_part(parts.next(), output)?;
        let Some(minor_part) = parts.next() else {
            return Err(malformed_version(output));
        };
        let patch_part = parts.next();
        if parts.next().is_some() {
            return Err(malformed_version(output));
        }
        let (minor, patch) = match patch_part {
            Some(part) => (
                parse_version_part(Some(minor_part), output)?,
                parse_final_version_part(part, output)?,
            ),
            None => (parse_final_version_part(minor_part, output)?, 0),
        };
        Ok(Self::new(major, minor, patch))
    }
}

impl std::fmt::Display for MultiplexerVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Pure, fully resolved local multiplexer command policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiplexerPlan {
    platform: LocalPlatform,
    executable: PathBuf,
    isolation: MultiplexerIsolation,
    base_args: Vec<OsString>,
}

impl MultiplexerPlan {
    /// Validate and construct a plan for an explicit platform and isolation.
    pub fn for_platform(
        platform: LocalPlatform,
        executable: PathBuf,
        isolation: MultiplexerIsolation,
    ) -> Result<Self, MultiplexerError> {
        validate_executable(platform, &executable)?;
        let base_args = base_args(platform, &isolation)?;
        Ok(Self {
            platform,
            executable,
            isolation,
            base_args,
        })
    }

    /// Resolve the current platform's executable and stable production isolation handle.
    pub fn current() -> Result<Self, MultiplexerError> {
        Self::resolved(false)
    }

    #[cfg(test)]
    pub(crate) fn current_for_test() -> Result<Self, MultiplexerError> {
        Self::resolved(true)
    }

    fn resolved(unique: bool) -> Result<Self, MultiplexerError> {
        let platform = LocalPlatform::current();
        let executable = resolve_executable(platform)?;
        let isolation = match platform {
            LocalPlatform::Unix => {
                MultiplexerIsolation::Socket(super::socket::jefe_tmux_socket_path().to_path_buf())
            }
            LocalPlatform::Windows if unique => {
                MultiplexerIsolation::Namespace(unique_test_namespace())
            }
            LocalPlatform::Windows => MultiplexerIsolation::Namespace(stable_jefe_namespace()),
        };
        Self::for_platform(platform, executable, isolation)
    }

    /// Return the resolved executable without converting it to UTF-8.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Return the platform-correct arguments prepended to every local command.
    #[must_use]
    pub fn base_args(&self) -> &[OsString] {
        &self.base_args
    }

    /// Build the platform-correct pane command passed to a new session.
    pub fn pane_command_args(
        &self,
        program: &OsStr,
        args: &[OsString],
        environment: &[(OsString, OsString)],
    ) -> Result<Vec<OsString>, MultiplexerError> {
        match self.platform {
            LocalPlatform::Unix => unix_pane_command_args(program, args, environment),
            LocalPlatform::Windows => {
                windows_pane_command_args(program, args, environment).map(|line| vec![line])
            }
        }
    }

    /// Build a pane command from a resolved agent's explicit wrapper strategy.
    pub fn agent_pane_command_args(
        &self,
        executable: &ResolvedAgentExecutable,
        args: &[OsString],
        environment: &[(OsString, OsString)],
    ) -> Result<Vec<OsString>, MultiplexerError> {
        if self.platform == LocalPlatform::Unix {
            return self.pane_command_args(executable.path().as_os_str(), args, environment);
        }

        let launcher =
            std::env::current_exe().map_err(|_| MultiplexerError::CurrentExecutableUnavailable)?;
        self.agent_pane_command_args_with_launcher(executable, args, environment, &launcher)
    }

    /// Build the Windows pane command with an explicit Jefe launcher path.
    #[doc(hidden)]
    pub fn agent_pane_command_args_with_launcher(
        &self,
        executable: &ResolvedAgentExecutable,
        args: &[OsString],
        environment: &[(OsString, OsString)],
        launcher: &Path,
    ) -> Result<Vec<OsString>, MultiplexerError> {
        let plan_path = write_launch_plan(executable, args, environment)
            .map_err(MultiplexerError::AgentLaunchPlan)?;
        self.pane_command_args(
            launcher.as_os_str(),
            &[
                OsString::from(INTERNAL_LAUNCH_ARGUMENT),
                plan_path.into_os_string(),
            ],
            &[],
        )
    }
    #[must_use]
    pub const fn isolation(&self) -> &MultiplexerIsolation {
        &self.isolation
    }

    /// Return whether this plan supports a required operation.
    #[must_use]
    pub const fn supports(&self, capability: MultiplexerCapability) -> bool {
        match (self.platform, capability) {
            (LocalPlatform::Unix, MultiplexerCapability::NamespaceIsolation)
            | (LocalPlatform::Windows, MultiplexerCapability::SocketIsolation) => false,
            (_, MultiplexerCapability::AttachSession | MultiplexerCapability::PaneCapture)
            | (LocalPlatform::Unix, MultiplexerCapability::SocketIsolation)
            | (LocalPlatform::Windows, MultiplexerCapability::NamespaceIsolation) => true,
        }
    }

    /// Build a process command carrying this plan's executable and base args.
    #[must_use]
    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        command.args(&self.base_args);
        command
    }

    /// Probe the executable and enforce version and capability policy.
    pub fn preflight(
        &self,
        required: &[MultiplexerCapability],
    ) -> Result<MultiplexerVersion, MultiplexerError> {
        let output =
            self.command()
                .arg("-V")
                .output()
                .map_err(|error| MultiplexerError::LaunchFailed {
                    path: self.executable.clone(),
                    reason: error.to_string(),
                    guidance: guidance(self.platform),
                })?;
        let version = classify_probe(output_observation(self.platform, &self.executable, output))?;
        for capability in required {
            if !self.supports(*capability) {
                return Err(MultiplexerError::RequiredCapabilityUnavailable {
                    path: self.executable.clone(),
                    version,
                    capability: *capability,
                });
            }
        }
        Ok(version)
    }
}

/// Captured input to the pure dependency-probe classifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeObservation {
    /// No acceptable executable was found.
    Missing {
        platform: LocalPlatform,
        path: PathBuf,
    },
    /// The executable could not be started.
    LaunchFailed {
        platform: LocalPlatform,
        path: PathBuf,
        reason: String,
    },
    /// The executable completed and produced output.
    Output {
        platform: LocalPlatform,
        path: PathBuf,
        status_success: bool,
        stdout: String,
        stderr: String,
    },
    /// A parsed executable lacks a caller-required capability.
    CapabilityMissing {
        platform: LocalPlatform,
        path: PathBuf,
        version: MultiplexerVersion,
        capability: MultiplexerCapability,
    },
}

/// Classify dependency observations into a qualified version or typed error.
pub fn classify_probe(
    observation: ProbeObservation,
) -> Result<MultiplexerVersion, MultiplexerError> {
    match observation {
        ProbeObservation::Missing { platform, path } => Err(MultiplexerError::MissingExecutable {
            path,
            guidance: guidance(platform),
        }),
        ProbeObservation::LaunchFailed {
            platform,
            path,
            reason,
        } => Err(MultiplexerError::LaunchFailed {
            path,
            reason,
            guidance: guidance(platform),
        }),
        ProbeObservation::CapabilityMissing {
            platform: _,
            path,
            version,
            capability,
        } => Err(MultiplexerError::RequiredCapabilityUnavailable {
            path,
            version,
            capability,
        }),
        ProbeObservation::Output {
            platform,
            path,
            status_success,
            stdout,
            stderr,
        } => classify_output(platform, path, status_success, stdout, stderr),
    }
}

/// Typed failures from local multiplexer resolution and dependency preflight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultiplexerError {
    /// No supported executable was found.
    MissingExecutable {
        path: PathBuf,
        guidance: &'static str,
    },
    /// A compatibility-environment executable was rejected.
    RejectedExecutable { path: PathBuf, reason: &'static str },
    /// The executable could not be launched.
    LaunchFailed {
        path: PathBuf,
        reason: String,
        guidance: &'static str,
    },
    /// Version output was not tmux-compatible.
    MalformedVersion {
        path: Option<PathBuf>,
        output: String,
    },
    /// The executable version is below the supported minimum.
    UnsupportedVersion {
        path: PathBuf,
        detected: MultiplexerVersion,
        minimum: MultiplexerVersion,
        guidance: &'static str,
    },
    /// A required command capability is unavailable.
    RequiredCapabilityUnavailable {
        path: PathBuf,
        version: MultiplexerVersion,
        capability: MultiplexerCapability,
    },
    /// The selected isolation handle does not match the platform policy.
    InvalidIsolation { platform: LocalPlatform },
    /// A psmux namespace contains unsupported characters or length.
    InvalidNamespace { namespace: String },
    /// A Windows shell command argument cannot be represented as Unicode.
    NonUnicodeArgument { value: OsString },
    /// An environment variable name cannot be represented safely in PowerShell.
    InvalidEnvironmentVariable { name: OsString },
    /// Jefe's own executable path could not be determined for the private launcher.
    CurrentExecutableUnavailable,
    /// The narrow Windows agent launch plan could not be prepared.
    AgentLaunchPlan(AgentLauncherError),
}

impl std::fmt::Display for MultiplexerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingExecutable { path, guidance } => write!(
                formatter,
                "multiplexer executable '{}' was not found; {guidance}",
                path.display()
            ),
            Self::RejectedExecutable { path, reason } => write!(
                formatter,
                "rejected multiplexer executable '{}': {reason}",
                path.display()
            ),
            Self::LaunchFailed {
                path,
                reason,
                guidance,
            } => write!(
                formatter,
                "failed to launch multiplexer '{}': {reason}; {guidance}",
                path.display()
            ),
            Self::MalformedVersion { path, output } => {
                format_malformed_version(formatter, path.as_deref(), output)
            }
            Self::UnsupportedVersion {
                path,
                detected,
                minimum,
                guidance,
            } => write!(
                formatter,
                "unsupported multiplexer version {detected} at '{}'; minimum is {minimum}; {guidance}",
                path.display()
            ),
            Self::RequiredCapabilityUnavailable {
                path,
                version,
                capability,
            } => write!(
                formatter,
                "multiplexer '{}' version {version} lacks required capability {capability:?}",
                path.display()
            ),
            Self::InvalidIsolation { platform } => {
                write!(formatter, "invalid multiplexer isolation for {platform:?}")
            }
            Self::InvalidNamespace { namespace } => {
                write!(formatter, "invalid private psmux namespace {namespace:?}")
            }
            Self::NonUnicodeArgument { value } => write!(
                formatter,
                "Windows psmux shell argument is not valid Unicode: {}",
                Path::new(value).display()
            ),
            Self::InvalidEnvironmentVariable { name } => {
                format_invalid_environment_variable(formatter, name)
            }
            Self::CurrentExecutableUnavailable | Self::AgentLaunchPlan(_) => {
                format_agent_launch_error(formatter, self)
            }
        }
    }
}
fn format_agent_launch_error(
    formatter: &mut std::fmt::Formatter<'_>,
    error: &MultiplexerError,
) -> std::fmt::Result {
    match error {
        MultiplexerError::CurrentExecutableUnavailable => {
            formatter.write_str("Jefe executable path is unavailable for Windows agent launch")
        }
        MultiplexerError::AgentLaunchPlan(source) => write!(
            formatter,
            "Windows agent launch plan preparation failed: {source}"
        ),
        _ => formatter.write_str("unrelated multiplexer error"),
    }
}

impl std::error::Error for MultiplexerError {}

fn format_malformed_version(
    formatter: &mut std::fmt::Formatter<'_>,
    path: Option<&Path>,
    output: &str,
) -> std::fmt::Result {
    match path {
        Some(path) => write!(
            formatter,
            "malformed multiplexer version output from '{}': {output:?}",
            path.display()
        ),
        None => write!(
            formatter,
            "malformed multiplexer version output: {output:?}"
        ),
    }
}

fn format_invalid_environment_variable(
    formatter: &mut std::fmt::Formatter<'_>,
    name: &OsStr,
) -> std::fmt::Result {
    write!(
        formatter,
        "invalid Windows environment variable name: {}",
        Path::new(name).display()
    )
}

/// Return deterministic executable names considered for a platform.
#[must_use]
pub fn executable_candidates(platform: LocalPlatform) -> Vec<OsString> {
    match platform {
        LocalPlatform::Unix => vec![OsString::from("tmux")],
        LocalPlatform::Windows => vec![OsString::from("psmux.exe"), OsString::from("psmux")],
    }
}

/// Validate a psmux namespace accepted by Jefe's private-isolation policy.
pub fn validate_namespace(namespace: &str) -> Result<(), MultiplexerError> {
    let valid = (8..=80).contains(&namespace.len())
        && namespace
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    if valid {
        Ok(())
    } else {
        Err(MultiplexerError::InvalidNamespace {
            namespace: namespace.to_owned(),
        })
    }
}

fn unix_pane_command_args(
    program: &OsStr,
    args: &[OsString],
    environment: &[(OsString, OsString)],
) -> Result<Vec<OsString>, MultiplexerError> {
    let mut command = vec![OsString::from("env")];
    for variable in ["TMUX", "TMUX_PANE", "TMUX_TMPDIR"] {
        command.push(OsString::from("-u"));
        command.push(OsString::from(variable));
    }
    for (key, value) in environment {
        environment_variable_name(key)?;
        let mut assignment = key.clone();
        assignment.push("=");
        assignment.push(value);
        command.push(assignment);
    }
    command.push(program.to_owned());
    command.extend(args.iter().cloned());
    Ok(command)
}

fn windows_pane_command_args(
    program: &OsStr,
    args: &[OsString],
    environment: &[(OsString, OsString)],
) -> Result<OsString, MultiplexerError> {
    let mut commands = ["TMUX", "TMUX_PANE", "TMUX_TMPDIR"]
        .map(|variable| format!("$env:{variable}=$null"))
        .to_vec();
    for (key, value) in environment {
        commands.push(format!(
            "$env:{}={}",
            environment_variable_name(key)?,
            powershell_quote(unicode_argument(value)?)
        ));
    }
    let mut launch = format!("& {}", powershell_quote(unicode_argument(program)?));
    for argument in args {
        launch.push(' ');
        launch.push_str(&powershell_quote(unicode_argument(argument)?));
    }
    commands.push(launch);
    Ok(OsString::from(commands.join("; ")))
}

fn unicode_argument(value: &OsStr) -> Result<&str, MultiplexerError> {
    value
        .to_str()
        .ok_or_else(|| MultiplexerError::NonUnicodeArgument {
            value: value.to_owned(),
        })
}

fn environment_variable_name(value: &OsStr) -> Result<&str, MultiplexerError> {
    let name = unicode_argument(value)?;
    let mut bytes = name.bytes();
    let valid = bytes
        .next()
        .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric());
    if valid {
        Ok(name)
    } else {
        Err(MultiplexerError::InvalidEnvironmentVariable {
            name: value.to_owned(),
        })
    }
}
fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn base_args(
    platform: LocalPlatform,
    isolation: &MultiplexerIsolation,
) -> Result<Vec<OsString>, MultiplexerError> {
    match (platform, isolation) {
        (LocalPlatform::Unix, MultiplexerIsolation::Socket(socket)) => Ok(vec![
            OsString::from("-f"),
            OsString::from("/dev/null"),
            OsString::from("-S"),
            socket.as_os_str().to_owned(),
        ]),
        (LocalPlatform::Windows, MultiplexerIsolation::Namespace(namespace)) => {
            validate_namespace(namespace)?;
            Ok(vec![
                OsString::from("-f"),
                OsString::from("NUL"),
                OsString::from("-L"),
                OsString::from(namespace),
            ])
        }
        _ => Err(MultiplexerError::InvalidIsolation { platform }),
    }
}

fn validate_executable(platform: LocalPlatform, executable: &Path) -> Result<(), MultiplexerError> {
    if platform != LocalPlatform::Windows {
        return Ok(());
    }
    let filename = executable
        .file_name()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase);
    let compatibility_path = executable.components().any(|component| {
        component.as_os_str().to_str().is_some_and(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "wsl" | "cygwin" | "cygwin64" | "msys" | "msys2" | "msys64" | "git"
            )
        })
    });
    if compatibility_path
        || !filename
            .as_deref()
            .is_some_and(|name| matches!(name, "psmux" | "psmux.exe"))
    {
        return Err(MultiplexerError::RejectedExecutable {
            path: executable.to_path_buf(),
            reason: "native Windows requires official psmux; WSL, Cygwin, MSYS2, and Git Bash tmux are unsupported",
        });
    }
    Ok(())
}

fn resolve_executable(platform: LocalPlatform) -> Result<PathBuf, MultiplexerError> {
    let override_name = match platform {
        LocalPlatform::Unix => "JEFE_TMUX_BIN",
        LocalPlatform::Windows => "JEFE_PSMUX_BIN",
    };
    if let Some(explicit) = std::env::var_os(override_name).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(explicit);
        validate_executable(platform, &path)?;
        return Ok(path);
    }
    for candidate in executable_candidates(platform) {
        if let Some(path) = find_on_path(&candidate) {
            validate_executable(platform, &path)?;
            return Ok(path);
        }
    }
    let path = PathBuf::from(&executable_candidates(platform)[0]);
    Err(MultiplexerError::MissingExecutable {
        path,
        guidance: guidance(platform),
    })
}

fn find_on_path(candidate: &OsStr) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        let candidate_path = directory.join(candidate);
        if candidate_path.is_file() {
            return Some(candidate_path);
        }
    }
    None
}

fn unique_test_namespace() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("jefe-test-{}-{sequence:x}", std::process::id())
}

fn stable_jefe_namespace() -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for value in [
        std::env::var_os("USERNAME"),
        std::env::current_exe().ok().map(PathBuf::into_os_string),
    ]
    .into_iter()
    .flatten()
    {
        for byte in value.as_encoded_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("jefe-{hash:016x}")
}

fn parse_version_part(part: Option<&str>, source: &str) -> Result<u32, MultiplexerError> {
    let Some(part) = part else {
        return Err(malformed_version(source));
    };
    if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(malformed_version(source));
    }
    part.parse::<u32>().map_err(|_| malformed_version(source))
}

fn parse_final_version_part(part: &str, source: &str) -> Result<u32, MultiplexerError> {
    let digit_count = part.bytes().take_while(u8::is_ascii_digit).count();
    let (digits, suffix) = part.split_at(digit_count);
    if digits.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return Err(malformed_version(source));
    }
    digits.parse::<u32>().map_err(|_| malformed_version(source))
}

fn malformed_version(source: &str) -> MultiplexerError {
    MultiplexerError::MalformedVersion {
        path: None,
        output: source.to_owned(),
    }
}

fn output_observation(platform: LocalPlatform, path: &Path, output: Output) -> ProbeObservation {
    ProbeObservation::Output {
        platform,
        path: path.to_path_buf(),
        status_success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn classify_output(
    platform: LocalPlatform,
    path: PathBuf,
    status_success: bool,
    stdout: String,
    stderr: String,
) -> Result<MultiplexerVersion, MultiplexerError> {
    if !status_success {
        return Err(MultiplexerError::LaunchFailed {
            path,
            reason: stderr,
            guidance: guidance(platform),
        });
    }
    let version = MultiplexerVersion::parse(&stdout).map_err(|error| match error {
        MultiplexerError::MalformedVersion { output, .. } => MultiplexerError::MalformedVersion {
            path: Some(path.clone()),
            output,
        },
        other => other,
    })?;
    if platform == LocalPlatform::Windows && version < MINIMUM_PSMUX_VERSION {
        return Err(MultiplexerError::UnsupportedVersion {
            path,
            detected: version,
            minimum: MINIMUM_PSMUX_VERSION,
            guidance: WINDOWS_INSTALL_GUIDANCE,
        });
    }
    Ok(version)
}

const fn guidance(platform: LocalPlatform) -> &'static str {
    match platform {
        LocalPlatform::Unix => UNIX_INSTALL_GUIDANCE,
        LocalPlatform::Windows => WINDOWS_INSTALL_GUIDANCE,
    }
}
