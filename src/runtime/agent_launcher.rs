//! Narrow Windows pane launcher used to preserve argv and scrub multiplexer state.

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

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

/// Owns a private launch plan until the multiplexer successfully accepts it.
#[derive(Debug)]
pub struct PendingLaunchPlan {
    path: Option<PathBuf>,
}

impl PendingLaunchPlan {
    #[must_use]
    pub fn path(&self) -> &Path {
        self.path.as_deref().unwrap_or_else(|| Path::new(""))
    }

    pub fn disarm(&mut self) {
        self.path = None;
    }
}

impl Drop for PendingLaunchPlan {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Write a private launch plan and return its cleanup owner.
pub fn write_launch_plan(
    executable: &ResolvedAgentExecutable,
    args: &[OsString],
    environment: &[(OsString, OsString)],
) -> Result<PendingLaunchPlan, AgentLauncherError> {
    let payload = AgentLaunchPayload {
        path: executable.path().to_path_buf(),
        wrapper: executable.wrapper_kind().into(),
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
                return Ok(PendingLaunchPlan { path: Some(path) });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_launch_plan_removes_file_unless_disarmed() {
        let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
        let removable = temp.path().join("removable.json");
        std::fs::write(&removable, "payload")
            .unwrap_or_else(|error| panic!("write removable plan: {error}"));
        {
            let _plan = PendingLaunchPlan {
                path: Some(removable.clone()),
            };
        }
        assert!(!removable.exists());

        let retained = temp.path().join("retained.json");
        std::fs::write(&retained, "payload")
            .unwrap_or_else(|error| panic!("write retained plan: {error}"));
        {
            let mut plan = PendingLaunchPlan {
                path: Some(retained.clone()),
            };
            plan.disarm();
        }
        assert!(retained.exists());
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
