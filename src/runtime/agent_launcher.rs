//! Narrow Windows pane launcher used to preserve argv and scrub multiplexer state.

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::agent_executable::{AgentWrapperKind, NpmDirectInvocation, ResolvedAgentExecutable};

/// Private CLI marker consumed before Jefe's public argument parser.
pub const INTERNAL_LAUNCH_ARGUMENT: &str = "--jefe-internal-agent-launch";

static LAUNCH_PLAN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Private launch-plan payload carrying the resolved executable path, wrapper
/// kind, optional npm direct invocation, args, and environment.
///
/// `pub` so that behavioral tests can deserialize a plan file and call
/// [`command_for_payload`] to assert the exact program and argv — the same
/// production function used by [`run_launch_plan`].
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentLaunchPayload {
    pub path: PathBuf,
    pub wrapper: AgentWrapperKindPayload,
    /// When the resolved executable is a Windows `.cmd`/`.bat` npm wrapper
    /// with a derivable direct `node.exe` + `npm-cli.js` invocation, this
    /// carries that plan so the launcher never routes through `cmd.exe`.
    #[serde(default)]
    pub npm_direct: Option<NpmDirectInvocationPayload>,
    pub args: Vec<OsString>,
    pub environment: Vec<(OsString, OsString)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NpmDirectInvocationPayload {
    pub node_executable: PathBuf,
    pub cli_script: PathBuf,
}

impl From<&NpmDirectInvocation> for NpmDirectInvocationPayload {
    fn from(value: &NpmDirectInvocation) -> Self {
        Self {
            node_executable: value.node_executable().to_path_buf(),
            cli_script: value.cli_script().to_path_buf(),
        }
    }
}

impl NpmDirectInvocationPayload {
    /// Build a `Command` that launches `node.exe <cli_script> <args>` directly,
    /// preserving every argument as a distinct structural argv element.
    fn command(&self, args: &[OsString]) -> Command {
        let mut command = Command::new(&self.node_executable);
        command.arg(&self.cli_script);
        command.args(args);
        command
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentWrapperKindPayload {
    Direct,
    CommandScript,
    PowerShellScript,
}

impl From<AgentWrapperKind> for AgentWrapperKindPayload {
    fn from(value: AgentWrapperKind) -> Self {
        match value {
            AgentWrapperKind::Direct => Self::Direct,
            AgentWrapperKind::CommandScript => Self::CommandScript,
            AgentWrapperKind::PowerShellScript => Self::PowerShellScript,
        }
    }
}

/// Write a private launch plan and return only its non-secret transport path.
pub fn write_launch_plan(
    executable: &ResolvedAgentExecutable,
    args: &[OsString],
    environment: &[(OsString, OsString)],
) -> Result<PathBuf, AgentLauncherError> {
    let payload = AgentLaunchPayload {
        path: executable.path().to_path_buf(),
        wrapper: executable.wrapper_kind().into(),
        npm_direct: executable.npm_direct().map(Into::into),
        args: args.to_vec(),
        environment: environment.to_vec(),
    };
    let bytes =
        serde_json::to_vec(&payload).map_err(|_| AgentLauncherError::PlanSerializationFailed)?;
    for _ in 0..16 {
        let sequence = LAUNCH_PLAN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let path = std::env::temp_dir().join(format!(
            "jefe-agent-launch-{}-{timestamp:x}-{sequence:x}.json",
            std::process::id()
        ));
        match secure_launch_plan_file(&path) {
            Ok(mut file) => {
                if file.write_all(&bytes).is_err() {
                    drop(file);
                    return match std::fs::remove_file(&path) {
                        Ok(()) => Err(AgentLauncherError::PlanWriteFailed),
                        Err(_) => Err(AgentLauncherError::CleanupFailed),
                    };
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(AgentLauncherError::PlanCreateFailed),
        }
    }
    Err(AgentLauncherError::PlanCreateFailed)
}

/// Consume and execute a private launch plan, returning the child status.
pub fn run_launch_plan(path: &Path) -> Result<ExitStatus, AgentLauncherError> {
    if !valid_launch_plan_path(path) {
        return Err(AgentLauncherError::InvalidPlan);
    }
    let bytes = std::fs::read(path).map_err(|_| AgentLauncherError::PlanReadFailed)?;
    std::fs::remove_file(path).map_err(|_| AgentLauncherError::CleanupFailed)?;
    let payload: AgentLaunchPayload =
        serde_json::from_slice(&bytes).map_err(|_| AgentLauncherError::InvalidPlanPayload)?;
    let mut command = command_for_payload(&payload);
    for variable in ["TMUX", "TMUX_PANE", "TMUX_TMPDIR"] {
        command.env_remove(variable);
    }
    command.envs(payload.environment);
    command
        .status()
        .map_err(|_| AgentLauncherError::LaunchFailed)
}
fn valid_launch_plan_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    let parent_is_temp = path.parent().is_some_and(|parent| {
        std::fs::canonicalize(parent).is_ok_and(|actual| {
            std::fs::canonicalize(std::env::temp_dir()).is_ok_and(|expected| actual == expected)
        })
    });
    parent_is_temp
        && name.starts_with("jefe-agent-launch-")
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

#[cfg(unix)]
fn secure_launch_plan_file(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn secure_launch_plan_file(path: &Path) -> std::io::Result<std::fs::File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

/// Build the production `Command` from a deserialized launch payload.
///
/// This is the production command-spec seam: it inspects the payload's
/// resolved executable and wrapper kind to decide whether to launch
/// `node.exe <cli_script> <args>` directly (npm direct invocation), the
/// binary directly, or through a platform wrapper. The same function is
/// used by `run_launch_plan` in production and by behavioral tests that
/// assert the exact program and argv — no test-only duplicate.
pub fn command_for_payload(payload: &AgentLaunchPayload) -> Command {
    // When an npm direct invocation is present, launch `node.exe <cli_script>
    // <args>` directly — never `cmd.exe`. This keeps the selector and all npm
    // arguments as distinct structural argv elements that cmd.exe cannot
    // reparse, eliminating metacharacter injection.
    if let Some(npm_direct) = &payload.npm_direct {
        return npm_direct.command(&payload.args);
    }
    match payload.wrapper {
        AgentWrapperKindPayload::Direct => {
            let mut command = Command::new(&payload.path);
            command.args(&payload.args);
            command
        }
        AgentWrapperKindPayload::CommandScript => {
            let mut command = Command::new(
                std::env::var_os("COMSPEC").unwrap_or_else(|| OsString::from("cmd.exe")),
            );
            command
                .args(["/D", "/S", "/C"])
                .arg(&payload.path)
                .args(&payload.args);
            command
        }
        AgentWrapperKindPayload::PowerShellScript => {
            let mut command = Command::new(
                std::env::var_os("JEFE_POWERSHELL_BIN")
                    .unwrap_or_else(|| OsString::from("powershell.exe")),
            );
            command
                .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
                .arg(&payload.path)
                .args(&payload.args);
            command
        }
    }
}

/// Safe private-launch failure that never renders payload contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentLauncherError {
    InvalidPlan,
    PlanSerializationFailed,
    PlanCreateFailed,
    PlanWriteFailed,
    PlanReadFailed,
    InvalidPlanPayload,
    CleanupFailed,
    LaunchFailed,
}

impl std::fmt::Display for AgentLauncherError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPlan => formatter.write_str("invalid internal agent launch plan path"),
            Self::PlanSerializationFailed => {
                formatter.write_str("internal agent launch plan could not be serialized")
            }
            Self::PlanCreateFailed => {
                formatter.write_str("internal agent launch plan file could not be created")
            }
            Self::PlanWriteFailed => {
                formatter.write_str("internal agent launch plan file could not be written")
            }
            Self::PlanReadFailed => {
                formatter.write_str("internal agent launch plan file could not be read")
            }
            Self::InvalidPlanPayload => {
                formatter.write_str("internal agent launch plan payload is malformed")
            }
            Self::CleanupFailed => formatter.write_str("internal agent launch plan cleanup failed"),
            Self::LaunchFailed => formatter.write_str("agent process could not be started"),
        }
    }
}

impl std::error::Error for AgentLauncherError {}
