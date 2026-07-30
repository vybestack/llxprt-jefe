//! Narrow Windows pane launcher used to preserve argv and scrub multiplexer state.

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::agent_candidate_path::AgentWrapperKind;

/// Private CLI marker consumed before Jefe's public argument parser.
pub const INTERNAL_LAUNCH_ARGUMENT: &str = "--jefe-internal-agent-launch";

static LAUNCH_PLAN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Serialize, Deserialize)]
struct AgentLaunchPayload {
    path: PathBuf,
    wrapper: AgentWrapperKindPayload,
    script_launch: Option<ScriptLaunchPayload>,
    args: Vec<OsString>,
    environment: Vec<(OsString, OsString)>,
    /// Canonical working directory the agent child must start in (issue #530).
    /// Serializes losslessly so paths with spaces and non-ASCII survive the
    /// private pane-host transport; a payload missing this field is malformed.
    cwd: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct ScriptLaunchPayload {
    runtime: PathBuf,
    entrypoint: PathBuf,
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
    executable: &Path,
    wrapper: AgentWrapperKind,
    args: &[OsString],
    environment: &[(OsString, OsString)],
    cwd: &Path,
) -> Result<PathBuf, AgentLauncherError> {
    let payload = AgentLaunchPayload {
        path: executable.to_path_buf(),
        wrapper: wrapper.into(),
        script_launch: None,
        args: args.to_vec(),
        environment: environment.to_vec(),
        cwd: cwd.to_path_buf(),
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
    // Issue #530: the requested cwd must exist and be a directory before the
    // worker is spawned. A missing or non-directory cwd is a typed failure with
    // no fallback so the agent never silently starts in the wrong directory.
    if !payload.cwd.is_dir() {
        return Err(AgentLauncherError::InvalidWorkingDirectory);
    }
    let mut command = command_for_payload(&payload);
    for variable in ["TMUX", "TMUX_PANE", "TMUX_TMPDIR"] {
        command.env_remove(variable);
    }
    command.envs(payload.environment);

    // Issue #467 Slice 3 (AC6): on Windows the private pane host owns a
    // kill-on-close Job Object and assigns itself before spawning the worker so
    // the whole descendant tree inherits containment. The guard is held until
    // `status()` returns; host death closes the handle and the kernel reaps the
    // tree, while normal worker completion still returns the exit status. A
    // failure to establish containment is typed and refuses spawn so a host can
    // never start a tree it cannot contain. Unix behaviour is unchanged.
    #[cfg(windows)]
    let _containment = establish_worker_containment()?;

    command
        .status()
        .map_err(|_| AgentLauncherError::LaunchFailed)
}

#[cfg(windows)]
fn establish_worker_containment() -> Result<super::job_object::JobContainment, AgentLauncherError> {
    super::job_object::JobContainment::enable_for_current_process().map_err(|error| {
        tracing::error!(
            error = %error,
            "windows job object containment unavailable; refusing to spawn agent worker"
        );
        AgentLauncherError::ContainmentUnavailable
    })
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
    let mut command = base_command_for_payload(payload);
    // Issue #530: the actual agent child must start in the requested working
    // directory, not the session-host process's inherited cwd.
    command.current_dir(&payload.cwd);
    command
}

fn base_command_for_payload(payload: &AgentLaunchPayload) -> Command {
    if let Some(script_launch) = &payload.script_launch {
        let mut command = Command::new(&script_launch.runtime);
        command.arg(&script_launch.entrypoint).args(&payload.args);
        return command;
    }
    match payload.wrapper {
        AgentWrapperKindPayload::Direct => {
            let mut command = Command::new(&payload.path);
            command.args(&payload.args);
            command
        }
        AgentWrapperKindPayload::CommandScript => {
            // Canonical fingerprints store verbatim `\\?\` paths on Windows,
            // which cmd.exe cannot launch (issue #525). Strip the prefix only
            // at this command-construction boundary; Direct paths and the
            // structured script-launch plan are unaffected.
            let launch_path = super::agent_executable::strip_verbatim_prefix(&payload.path);
            let mut command = Command::new(
                std::env::var_os("COMSPEC").unwrap_or_else(|| OsString::from("cmd.exe")),
            );
            command
                .args(["/D", "/S", "/C"])
                .arg(&launch_path)
                .args(&payload.args);
            command
        }
        AgentWrapperKindPayload::PowerShellScript => {
            let launch_path = super::agent_executable::strip_verbatim_prefix(&payload.path);
            let mut command = Command::new(
                std::env::var_os("JEFE_POWERSHELL_BIN")
                    .unwrap_or_else(|| OsString::from("powershell.exe")),
            );
            command
                .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
                .arg(&launch_path)
                .args(&payload.args);
            command
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use super::{
        AgentLaunchPayload, AgentLauncherError, AgentWrapperKindPayload, command_for_payload,
    };

    #[test]
    fn canonical_command_script_payload_uses_launch_safe_path() {
        let dir = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("could not create launcher fixture: {error}"));
        let wrapper = dir.path().join("agent.cmd");
        std::fs::write(&wrapper, b"@echo off\r\nexit /b 0\r\n")
            .unwrap_or_else(|error| panic!("could not write launcher fixture: {error}"));
        let canonical = std::fs::canonicalize(&wrapper)
            .unwrap_or_else(|error| panic!("could not canonicalize launcher fixture: {error}"));
        let payload = AgentLaunchPayload {
            path: canonical.clone(),
            wrapper: AgentWrapperKindPayload::CommandScript,
            script_launch: None,
            args: vec![OsString::from("--version")],
            environment: Vec::new(),
            cwd: dir.path().to_path_buf(),
        };

        let command = command_for_payload(&payload);
        let args = command.get_args().collect::<Vec<_>>();
        assert_eq!(
            args[3],
            super::super::agent_executable::strip_verbatim_prefix(&canonical),
            "the private pane launcher must not pass a verbatim wrapper path to cmd.exe"
        );
    }

    // ── Issue #530: payload cwd projection and actual child cwd ──────────

    #[test]
    fn payload_cwd_round_trips_losslessly_through_serialization() {
        let workdir = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("could not create launcher fixture: {error}"));
        let requested = workdir.path().join("repo with spaces Ω");
        std::fs::create_dir_all(&requested)
            .unwrap_or_else(|error| panic!("could not create requested cwd fixture: {error}"));
        let payload = AgentLaunchPayload {
            path: workdir.path().join("agent.exe"),
            wrapper: AgentWrapperKindPayload::Direct,
            script_launch: None,
            args: vec![OsString::from("--version")],
            environment: Vec::new(),
            cwd: requested.clone(),
        };
        let bytes = serde_json::to_vec(&payload)
            .unwrap_or_else(|error| panic!("payload should serialize: {error}"));
        let decoded: AgentLaunchPayload = serde_json::from_slice(&bytes)
            .unwrap_or_else(|error| panic!("payload should deserialize: {error}"));
        assert_eq!(
            decoded.cwd, requested,
            "payload cwd must round-trip losslessly for paths with spaces and non-ASCII"
        );
    }

    #[test]
    fn command_for_payload_applies_requested_cwd_as_current_dir() {
        let workdir = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("could not create launcher fixture: {error}"));
        let requested = workdir.path().join("requested cwd");
        std::fs::create_dir_all(&requested)
            .unwrap_or_else(|error| panic!("could not create requested cwd fixture: {error}"));
        for wrapper in [
            AgentWrapperKindPayload::Direct,
            AgentWrapperKindPayload::CommandScript,
            AgentWrapperKindPayload::PowerShellScript,
        ] {
            let payload = AgentLaunchPayload {
                path: workdir.path().join("agent.bin"),
                wrapper,
                script_launch: None,
                args: vec![OsString::from("--version")],
                environment: Vec::new(),
                cwd: requested.clone(),
            };
            let command = command_for_payload(&payload);
            assert_eq!(
                command.get_current_dir(),
                Some(requested.as_path()),
                "the staged session host must start the actual agent child in the requested cwd ({wrapper:?})"
            );
        }
    }

    #[test]
    fn missing_cwd_in_payload_is_malformed() {
        // A payload JSON without a `cwd` field must fail to deserialize so the
        // missing-cwd contract cannot be silently bypassed.
        let json = br#"{"path":"C:/agent.exe","wrapper":"Direct","script_launch":null,"args":[],"environment":[]}"#;
        let result: Result<AgentLaunchPayload, _> = serde_json::from_slice(json);
        assert!(
            result.is_err(),
            "a payload without cwd must be rejected as malformed"
        );
    }

    #[test]
    fn run_launch_plan_rejects_nonexistent_cwd_before_worker_spawn() {
        let workdir = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("could not create launcher fixture: {error}"));
        let missing_cwd = workdir.path().join("does-not-exist");
        let payload = AgentLaunchPayload {
            path: PathBuf::from("C:/nonexistent-agent.exe"),
            wrapper: AgentWrapperKindPayload::Direct,
            script_launch: None,
            args: Vec::new(),
            environment: Vec::new(),
            cwd: missing_cwd,
        };
        let bytes = serde_json::to_vec(&payload)
            .unwrap_or_else(|error| panic!("payload should serialize: {error}"));
        let plan_path = std::env::temp_dir().join(format!(
            "jefe-agent-launch-{}-530-missing-cwd.json",
            std::process::id()
        ));
        std::fs::write(&plan_path, &bytes)
            .unwrap_or_else(|error| panic!("could not write plan fixture: {error}"));
        let result = super::run_launch_plan(&plan_path);
        assert!(
            matches!(result, Err(AgentLauncherError::InvalidWorkingDirectory)),
            "a nonexistent cwd must fail typed before worker spawn, got {result:?}"
        );
        assert!(
            !plan_path.exists(),
            "a rejected launch must still consume its private plan"
        );
    }

    #[test]
    fn run_launch_plan_rejects_non_directory_cwd_before_worker_spawn() {
        let workdir = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("could not create launcher fixture: {error}"));
        let file_cwd = workdir.path().join("not-a-directory.txt");
        std::fs::write(&file_cwd, b"file")
            .unwrap_or_else(|error| panic!("could not write cwd fixture: {error}"));
        let payload = AgentLaunchPayload {
            path: PathBuf::from("C:/nonexistent-agent.exe"),
            wrapper: AgentWrapperKindPayload::Direct,
            script_launch: None,
            args: Vec::new(),
            environment: Vec::new(),
            cwd: file_cwd,
        };
        let bytes = serde_json::to_vec(&payload)
            .unwrap_or_else(|error| panic!("payload should serialize: {error}"));
        let plan_path = std::env::temp_dir().join(format!(
            "jefe-agent-launch-{}-530-file-cwd.json",
            std::process::id()
        ));
        std::fs::write(&plan_path, &bytes)
            .unwrap_or_else(|error| panic!("could not write plan fixture: {error}"));
        let result = super::run_launch_plan(&plan_path);
        assert!(
            matches!(result, Err(AgentLauncherError::InvalidWorkingDirectory)),
            "a non-directory cwd must fail typed before worker spawn, got {result:?}"
        );
        assert!(
            !plan_path.exists(),
            "a rejected launch must still consume its private plan"
        );
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
    /// The requested working directory does not exist or is not a directory
    /// (issue #530). The worker is refused spawn so the agent never silently
    /// starts in the wrong directory.
    InvalidWorkingDirectory,
    /// Windows Job Object containment could not be established before spawning
    /// the worker (issue #467 Slice 3). Worker spawn is refused so a host can
    /// never start a descendant tree it cannot reliably contain.
    #[cfg(windows)]
    ContainmentUnavailable,
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
            Self::InvalidWorkingDirectory => {
                formatter.write_str("agent working directory does not exist or is not a directory")
            }
            #[cfg(windows)]
            Self::ContainmentUnavailable => formatter.write_str(
                "windows job object containment could not be established for agent worker",
            ),
        }
    }
}

impl std::error::Error for AgentLauncherError {}
