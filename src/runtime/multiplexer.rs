//! Platform-aware local multiplexer resolution, isolation, and dependency probing.
//!
//! Unix uses upstream tmux on Jefe's private socket. Native Windows uses psmux
//! on a private `-L` namespace. Remote SSH command construction intentionally
//! remains in `runtime::commands` and does not use this local policy.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::agent_candidate_path::AgentWrapperKind;

use super::agent_launcher::{AgentLauncherError, INTERNAL_LAUNCH_ARGUMENT, write_launch_plan};
use super::launch_gates::LaunchGate;
use super::multiplexer_contract::{PaneCommandBudget, pane_command_budget};
const MINIMUM_PSMUX_VERSION: MultiplexerVersion = MultiplexerVersion::new(3, 3, 7);
const WINDOWS_INSTALL_GUIDANCE: &str =
    "install psmux 3.3.7 or newer with `winget upgrade marlocarlo.psmux`, then restart Jefe";
const UNIX_INSTALL_GUIDANCE: &str =
    "install upstream tmux with your operating system package manager";

/// Inherited psmux session-routing variables that must be scrubbed from any
/// native Windows local command so Jefe never appears nested inside a parent
/// psmux session. `PSMUX_CLAUDE_TEAMMATE_MODE` and `PSMUX_CONFIG_FILE` are
/// intentionally retained: team mode is not session routing, and the plan's
/// base args already carry `-f NUL`. `pub(super)` so the local attach command
/// builder can share the exact same list instead of duplicating it.
pub(super) const PSMUX_INHERITED_SESSION_VARS: [&str; 2] =
    super::multiplexer_contract::PSMUX_SESSION_ROUTING_VARS;

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
        let mut components = token.split('.');
        let major_raw = components.next().ok_or_else(|| malformed_version(output))?;
        let major = parse_strict_version_part(major_raw, output)?;
        let minor_raw = components.next();
        let patch_raw = components.next();
        // After consuming up to three components, no trailing component may remain.
        if components.next().is_some() {
            return Err(malformed_version(output));
        }
        // The major component is always strict. Only the final present component
        // may carry a single alphabetic release letter (e.g. Homebrew `tmux 3.7b`).
        let (minor, patch) = match (minor_raw, patch_raw) {
            (Some(minor_raw), None) => {
                let minor = parse_final_version_part(minor_raw, output)?;
                (minor, 0)
            }
            (Some(minor_raw), Some(patch_raw)) => {
                let minor = parse_strict_version_part(minor_raw, output)?;
                let patch = parse_final_version_part(patch_raw, output)?;
                (minor, patch)
            }
            (None, _) => return Err(malformed_version(output)),
        };
        Ok(Self::new(major, minor, patch))
    }
}

impl std::fmt::Display for MultiplexerVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Everything a pane needs in order to launch one agent.
///
/// These values are always supplied together and are individually ambiguous
/// (several are paths), so they travel as one value rather than as a long
/// positional parameter list.
#[derive(Debug, Clone, Copy)]
pub struct AgentPaneLaunch<'a> {
    /// The agent executable and the wrapper strategy required to run it.
    pub executable: (&'a Path, AgentWrapperKind),
    /// Arguments passed to the agent itself.
    pub args: &'a [OsString],
    /// Environment overrides applied to the agent.
    pub environment: &'a [(OsString, OsString)],
    /// Working directory the agent must start in (issue #530).
    pub cwd: &'a Path,
    /// Where the session host records the identity of the worker it spawns, so
    /// jefe can tell the agent apart from the pane leader (issue #543). `None`
    /// where the pane leader is itself the agent.
    pub worker_report: Option<&'a Path>,
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

    /// Derive a plan for the same binary against different isolation.
    ///
    /// Contract conformance probing needs a namespace it owns outright, so that
    /// verbs which would be destructive against live agents are exercised only
    /// on sessions the prober created (issue #540).
    pub fn with_isolation(
        &self,
        isolation: MultiplexerIsolation,
    ) -> Result<Self, MultiplexerError> {
        Self::for_platform(self.platform, self.executable.clone(), isolation)
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
        let command = match self.platform {
            LocalPlatform::Unix => unix_pane_command_args(program, args, environment)?,
            LocalPlatform::Windows => {
                vec![windows_pane_command_args(program, args, environment)?]
            }
        };
        enforce_pane_command_budget(&command)?;
        Ok(command)
    }

    /// Build a pane command from a resolved agent's explicit wrapper strategy.
    ///
    /// Grouped so the agent's identity, its arguments and where its worker
    /// identity is reported travel together rather than as parallel positional
    /// parameters that are easy to transpose.
    pub fn agent_pane_command_args(
        &self,
        launch: &AgentPaneLaunch<'_>,
    ) -> Result<Vec<OsString>, MultiplexerError> {
        if self.platform == LocalPlatform::Unix {
            return self.pane_command_args(
                launch.executable.0.as_os_str(),
                launch.args,
                launch.environment,
            );
        }

        let launcher =
            std::env::current_exe().map_err(|_| MultiplexerError::CurrentExecutableUnavailable)?;
        self.agent_pane_command_args_with_launcher(launch, &launcher)
    }

    /// Build the Windows pane command with an explicit Jefe launcher path.
    #[doc(hidden)]
    pub fn agent_pane_command_args_with_launcher(
        &self,
        launch: &AgentPaneLaunch<'_>,
        launcher: &Path,
    ) -> Result<Vec<OsString>, MultiplexerError> {
        let plan_path = write_launch_plan(
            launch.executable.0,
            launch.executable.1,
            launch.args,
            launch.environment,
            launch.cwd,
            launch.worker_report,
        )
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

    /// Build the native Windows pane command launching an already-staged
    /// session-host image (issue #467).
    ///
    /// On Windows the staged copy replaces the live build target as the psmux
    /// pane launcher while argv/env are preserved unchanged. Unix/remote
    /// command paths never stage a host and must reject this call so a staged
    /// Windows path can never leak into the structurally unchanged tmux/SSH
    /// command construction.
    pub fn agent_pane_command_args_with_staged_host(
        &self,
        launch: &AgentPaneLaunch<'_>,
        staged_host: &Path,
    ) -> Result<Vec<OsString>, MultiplexerError> {
        if self.platform != LocalPlatform::Windows {
            return Err(MultiplexerError::InvalidIsolation {
                platform: self.platform,
            });
        }
        self.agent_pane_command_args_with_launcher(launch, staged_host)
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
    ///
    /// On native Windows, inherited psmux session-routing variables
    /// (`PSMUX_SESSION`/`PSMUX_TARGET_SESSION`) are scrubbed so Jefe never
    /// appears nested inside a parent psmux session even when its own process
    /// was launched from inside one. Unix is unaffected.
    #[must_use]
    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        command.args(&self.base_args);
        if self.platform == LocalPlatform::Windows {
            for variable in PSMUX_INHERITED_SESSION_VARS {
                command.env_remove(variable);
            }
        }
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
    /// The assembled pane command exceeds the measured budget.
    ///
    /// psmux exits 0 and creates the session whether or not the command
    /// survives the shell's ceiling, so an overrun is invisible unless it is
    /// refused here (issue #544 V7).
    PaneCommandOverBudget {
        /// Bytes the assembled command occupies.
        bytes: usize,
        /// The measured ceiling it exceeded.
        budget: PaneCommandBudget,
    },
}

impl std::fmt::Display for MultiplexerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingExecutable { .. }
            | Self::RejectedExecutable { .. }
            | Self::LaunchFailed { .. }
            | Self::MalformedVersion { .. }
            | Self::UnsupportedVersion { .. }
            | Self::RequiredCapabilityUnavailable { .. } => {
                format_executable_error(formatter, self)
            }
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
            Self::PaneCommandOverBudget { bytes, budget } => {
                format_pane_command_over_budget(formatter, *bytes, *budget)
            }
        }
    }
}

/// Renders the arms that describe a multiplexer executable jefe could not use.
///
/// Split out of `Display` only to keep that match inside the function-length
/// gate; the wording of every arm is unchanged.
fn format_executable_error(
    formatter: &mut std::fmt::Formatter<'_>,
    error: &MultiplexerError,
) -> std::fmt::Result {
    match error {
        MultiplexerError::MissingExecutable { path, guidance } => write!(
            formatter,
            "multiplexer executable '{}' was not found; {guidance}",
            path.display()
        ),
        MultiplexerError::RejectedExecutable { path, reason } => write!(
            formatter,
            "rejected multiplexer executable '{}': {reason}",
            path.display()
        ),
        MultiplexerError::LaunchFailed {
            path,
            reason,
            guidance,
        } => write!(
            formatter,
            "failed to launch multiplexer '{}': {reason}; {guidance}",
            path.display()
        ),
        MultiplexerError::MalformedVersion { path, output } => {
            format_malformed_version(formatter, path.as_deref(), output)
        }
        MultiplexerError::UnsupportedVersion {
            path,
            detected,
            minimum,
            guidance,
        } => write!(
            formatter,
            "unsupported multiplexer version {detected} at '{}'; minimum is {minimum}; {guidance}",
            path.display()
        ),
        MultiplexerError::RequiredCapabilityUnavailable {
            path,
            version,
            capability,
        } => write!(
            formatter,
            "multiplexer '{}' version {version} lacks required capability {capability:?}",
            path.display()
        ),
        // Listed rather than caught by `_` so adding a variant fails to compile
        // here instead of silently degrading to the text below (issue #544).
        MultiplexerError::InvalidIsolation { .. }
        | MultiplexerError::InvalidNamespace { .. }
        | MultiplexerError::NonUnicodeArgument { .. }
        | MultiplexerError::InvalidEnvironmentVariable { .. }
        | MultiplexerError::CurrentExecutableUnavailable
        | MultiplexerError::AgentLaunchPlan(_)
        | MultiplexerError::PaneCommandOverBudget { .. } => {
            formatter.write_str("unrelated multiplexer error")
        }
    }
}

fn format_pane_command_over_budget(
    formatter: &mut std::fmt::Formatter<'_>,
    bytes: usize,
    budget: PaneCommandBudget,
) -> std::fmt::Result {
    write!(
        formatter,
        "{}",
        LaunchGate::PaneCommand.refused(format!(
            "the pane command is {bytes} bytes, over the {} usable bytes measured on {}; \
             the multiplexer reports success and creates the session even when a command \
             this long never runs, so it is refused here instead of being truncated",
            budget.bytes, budget.measured_on
        ))
    )
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
        // Listed rather than caught by `_` for the same reason as above.
        MultiplexerError::MissingExecutable { .. }
        | MultiplexerError::RejectedExecutable { .. }
        | MultiplexerError::LaunchFailed { .. }
        | MultiplexerError::MalformedVersion { .. }
        | MultiplexerError::UnsupportedVersion { .. }
        | MultiplexerError::RequiredCapabilityUnavailable { .. }
        | MultiplexerError::InvalidIsolation { .. }
        | MultiplexerError::InvalidNamespace { .. }
        | MultiplexerError::NonUnicodeArgument { .. }
        | MultiplexerError::InvalidEnvironmentVariable { .. }
        | MultiplexerError::PaneCommandOverBudget { .. } => {
            formatter.write_str("unrelated multiplexer error")
        }
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

/// Bytes the assembled pane command occupies, counting the separator each
/// element needs — a `; ` join inside a PowerShell command line, or the NUL
/// terminator an `exec` argv pays per entry.
fn pane_command_bytes(command: &[OsString]) -> usize {
    command.iter().map(|part| part.len() + 1).sum()
}

/// Refuse a pane command the measured platform ceiling cannot carry.
///
/// The overrun is otherwise invisible: psmux exits 0, creates the session, and
/// the command simply never runs, which is why this is a refusal rather than a
/// truncation (issue #544 V7).
fn enforce_pane_command_budget(command: &[OsString]) -> Result<(), MultiplexerError> {
    let budget = pane_command_budget();
    let bytes = pane_command_bytes(command);
    if bytes > budget.bytes {
        return Err(MultiplexerError::PaneCommandOverBudget { bytes, budget });
    }
    Ok(())
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
    super::identity::unique_current_user_namespace()
}

fn stable_jefe_namespace() -> String {
    super::identity::stable_current_user_namespace()
}

fn parse_strict_version_part(part: &str, source: &str) -> Result<u32, MultiplexerError> {
    if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(malformed_version(source));
    }
    part.parse::<u32>().map_err(|_| malformed_version(source))
}

/// Parse the final present version component, permitting an optional single
/// trailing ASCII alphabetic release letter (e.g. Homebrew `tmux 3.7b`).
///
/// The letter carries no semantic weight beyond release identification; it is
/// discarded so that `3.7b` resolves to `3.7.0` and `3.3.6a` to `3.3.6`.
fn parse_final_version_part(part: &str, source: &str) -> Result<u32, MultiplexerError> {
    let digits_end = part
        .bytes()
        .position(|byte| !byte.is_ascii_digit())
        .unwrap_or(part.len());
    let (digits, suffix) = part.split_at(digits_end);
    let valid_suffix = suffix.is_empty()
        || (suffix.len() == 1
            && suffix
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_lowercase()));
    if digits.is_empty() || !valid_suffix {
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
