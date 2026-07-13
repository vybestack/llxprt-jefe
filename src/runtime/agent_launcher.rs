//! Narrow Windows pane launcher used to preserve argv and scrub multiplexer state.

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use super::agent_executable::{AgentWrapperKind, ResolvedAgentExecutable};

/// Private CLI marker consumed before Jefe's public argument parser.
pub const INTERNAL_LAUNCH_ARGUMENT: &str = "--jefe-internal-agent-launch";

static LAUNCH_PLAN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Serialize, Deserialize)]
struct AgentLaunchPayload {
    path: PathBuf,
    wrapper: AgentWrapperKindPayload,
    args: Vec<OsString>,
    environment: Vec<(OsString, OsString)>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum AgentWrapperKindPayload {
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
        args: args.to_vec(),
        environment: environment.to_vec(),
    };
    let bytes = serde_json::to_vec(&payload).map_err(|_| AgentLauncherError::InvalidPlan)?;
    for _ in 0..16 {
        let sequence = LAUNCH_PLAN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "jefe-agent-launch-{}-{sequence:x}.json",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(&bytes)
                    .map_err(|_| AgentLauncherError::InvalidPlan)?;
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(AgentLauncherError::InvalidPlan),
        }
    }
    Err(AgentLauncherError::InvalidPlan)
}

/// Consume and execute a private launch plan, returning the child status.
pub fn run_launch_plan(path: &Path) -> Result<ExitStatus, AgentLauncherError> {
    let bytes = std::fs::read(path).map_err(|_| AgentLauncherError::InvalidPlan)?;
    let _ = std::fs::remove_file(path);
    let payload: AgentLaunchPayload =
        serde_json::from_slice(&bytes).map_err(|_| AgentLauncherError::InvalidPlan)?;
    let mut command = command_for_payload(&payload);
    for variable in ["TMUX", "TMUX_PANE", "TMUX_TMPDIR"] {
        command.env_remove(variable);
    }
    command.envs(payload.environment);
    command
        .status()
        .map_err(|_| AgentLauncherError::LaunchFailed)
}

fn command_for_payload(payload: &AgentLaunchPayload) -> Command {
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
    LaunchFailed,
}

impl std::fmt::Display for AgentLauncherError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPlan => formatter.write_str("invalid internal agent launch plan"),
            Self::LaunchFailed => formatter.write_str("agent process could not be started"),
        }
    }
}

impl std::error::Error for AgentLauncherError {}
