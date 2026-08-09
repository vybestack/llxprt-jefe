//! Multiplexer-backed execution of strict schema-1 scenarios (issue #383 S8).
//!
//! The Unix PTY runner in [`super::runner`] is the primary schema-1 executor.
//! Native Windows has no PTY, so it drives the same schema-1 scenarios through
//! the retained tmux/psmux driver instead. Both consume
//! [`parse_scenario_v1`](super::parse_scenario_v1) — there is exactly one
//! scenario parser and one key encoder, so this is a second *backend*, not a
//! second format.
//!
//! Only the operations a multiplexer session can honor are supported; the
//! process-boundary operations (`capture`, `assert-capture`) are rejected as
//! typed errors rather than silently skipped.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::contract::{EnvVar, FileContent, ScenarioV1, Step, WaitSource};
use super::error::HarnessError;
use super::keys;
use crate::harness::tmux_driver::{TmuxDriver, TmuxPaneSize, TmuxSessionGuard, TmuxStartRequest};

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const HISTORY_LIMIT: u32 = 2_000;

/// Inputs for one multiplexer-backed schema-1 run.
pub struct TmuxRunRequest {
    pub session: String,
    pub jefe_binary: PathBuf,
    pub config_dir: PathBuf,
    pub working_dir: PathBuf,
    pub artifact_dir: Option<PathBuf>,
    pub keep_session: bool,
}

/// Summary of a completed run.
pub struct TmuxRunSummary {
    pub steps_run: usize,
    pub multiplexer_details: String,
}

/// Execute a schema-1 scenario against a real multiplexer session.
///
/// # Errors
///
/// [`HarnessError`] for driver failures, unmet waits, failed frame assertions,
/// or operations this backend cannot honor.
pub fn run_tmux_v1(
    scenario: &ScenarioV1,
    request: &TmuxRunRequest,
) -> Result<TmuxRunSummary, HarnessError> {
    materialize_workspace(scenario, &request.working_dir)?;
    let driver = TmuxDriver::new();
    let details = driver.diagnostics();
    let _signal_guard = crate::harness::signal_cleanup::SignalCleanupGuard::new(driver.clone())
        .map_err(|err| HarnessError::process(format!("install signal cleanup: {err}")))?;
    if let Some(directory) = &request.artifact_dir {
        write_artifact(directory, "multiplexer.txt", &details)?;
    }
    let start = TmuxStartRequest::jefe(
        request.session.clone(),
        request.jefe_binary.clone(),
        request.config_dir.clone(),
        request.working_dir.clone(),
        TmuxPaneSize::new(
            scenario.terminal.cols,
            scenario.terminal.rows,
            HISTORY_LIMIT,
        ),
    )
    .map_err(|err| HarnessError::process(format!("build start request: {err}")))?
    .with_keep_session(request.keep_session)
    .with_env(contained_app_env(scenario, &request.working_dir)?);
    let session = driver
        .start_session(&start)
        .map_err(|err| HarnessError::process(format!("start session: {err}")))?;
    let guard = TmuxSessionGuard::new(driver.clone(), session);
    let Some(session) = guard.session().cloned() else {
        return Err(HarnessError::process(
            "session guard must hold a live session".to_string(),
        ));
    };

    let mut steps_run = 0usize;
    let mut outcome = Ok(());
    for step in &scenario.steps {
        match execute(&driver, &session, step) {
            Ok(true) => {
                steps_run += 1;
            }
            Ok(false) => {
                steps_run += 1;
                break;
            }
            Err(err) => {
                write_failure_artifacts(&driver, &session, request.artifact_dir.as_ref(), &err);
                outcome = Err(err);
                break;
            }
        }
    }
    drop(guard);
    outcome.map(|()| TmuxRunSummary {
        steps_run,
        multiplexer_details: details,
    })
}

/// Execute one step. `Ok(false)` means the scenario finished.
fn execute(
    driver: &TmuxDriver,
    session: &crate::harness::tmux_driver::TmuxSession,
    step: &Step,
) -> Result<bool, HarnessError> {
    match step {
        Step::Launch { .. } => Ok(true),
        Step::Key { key, modifiers } => {
            let bytes = keys::encode("step.key", key, modifiers)?;
            let text = String::from_utf8(bytes).map_err(|err| {
                HarnessError::process(format!("key '{key}' is not encodable as text: {err}"))
            })?;
            driver
                .send_type(session, &text)
                .map_err(|err| HarnessError::process(format!("send key '{key}': {err}")))?;
            Ok(true)
        }
        Step::Text { text } => {
            driver
                .send_type(session, text)
                .map_err(|err| HarnessError::process(format!("send text: {err}")))?;
            Ok(true)
        }
        Step::Wait {
            source,
            literal,
            timeout_ms,
        } => {
            if !matches!(source, WaitSource::Frame) {
                return Err(HarnessError::process(
                    "the multiplexer backend observes rendered frames only".to_string(),
                ));
            }
            wait_for_frame(driver, session, literal, *timeout_ms)?;
            Ok(true)
        }
        Step::AssertFrame { contains, absent } => {
            assert_frame(driver, session, contains, absent)?;
            Ok(true)
        }
        Step::Finish => Ok(false),
        Step::Write { .. }
        | Step::Mkdir { .. }
        | Step::Remove { .. }
        | Step::Resize { .. }
        | Step::AssertFile { .. }
        | Step::Restart => Err(HarnessError::process(format!(
            "the multiplexer backend does not support the '{}' operation",
            step.op_name()
        ))),
        Step::Capture { .. } | Step::AssertCapture { .. } => Err(HarnessError::process(format!(
            "process-boundary operation '{}' requires the PTY runner",
            step.op_name()
        ))),
    }
}

fn wait_for_frame(
    driver: &TmuxDriver,
    session: &crate::harness::tmux_driver::TmuxSession,
    literal: &str,
    timeout_ms: u64,
) -> Result<(), HarnessError> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let capture = driver
            .capture_screen(session)
            .map_err(|err| HarnessError::process(format!("capture screen: {err}")))?;
        if capture.lines.iter().any(|line| line.contains(literal)) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(HarnessError::assertion(format!(
                "frame did not contain '{literal}' within {timeout_ms}ms"
            )));
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn assert_frame(
    driver: &TmuxDriver,
    session: &crate::harness::tmux_driver::TmuxSession,
    contains: &[String],
    absent: &[String],
) -> Result<(), HarnessError> {
    let capture = driver
        .capture_screen(session)
        .map_err(|err| HarnessError::process(format!("capture screen: {err}")))?;
    for literal in contains {
        if !capture.lines.iter().any(|line| line.contains(literal)) {
            return Err(HarnessError::assertion(format!(
                "frame must contain '{literal}'"
            )));
        }
    }
    for literal in absent {
        if capture.lines.iter().any(|line| line.contains(literal)) {
            return Err(HarnessError::assertion(format!(
                "frame must not contain '{literal}'"
            )));
        }
    }
    Ok(())
}

/// Materialize the declared workspace fixtures under the run's working
/// directory. `${workspace}` resolves to that directory.
/// Deterministic values the PTY backend imposes that this backend must not.
///
/// This backend runs against a real multiplexer and a real jefe: stripping
/// `PATH` would stop it finding `tmux` at all. They are dropped unless the
/// scenario asks for them by name.
const PTY_ONLY_DEFAULTS: [&str; 8] = [
    "HOME",
    "PATH",
    "TMPDIR",
    "JEFE_CONFIG_DIR",
    "JEFE_STATE_DIR",
    "JEFE_PLUGIN_DIR",
    "LANG",
    "TERM",
];

/// The environment the contained jefe is launched with (issue #390).
///
/// The scenario's own `workspace.env` and launch `env` are applied, with
/// `${workspace}` interpolated, so a scenario can actually configure the app it
/// is testing. This backend previously discarded the launch step outright.
///
/// `JEFE_SOCKET_PATH` is forced into the workspace unless the scenario names
/// its own. Jefe's tmux socket is derived from the *uid*, not from `--config`,
/// so without this a scenario joins whatever jefe server the operator already
/// has running: it sees their live agent sessions, reports them as unmatched,
/// and is one code path away from acting on them. Isolation here is not a
/// convenience, it is the difference between a test and an accident.
fn contained_app_env(
    scenario: &ScenarioV1,
    working_dir: &Path,
) -> Result<Vec<(String, String)>, HarnessError> {
    let root = working_dir.to_string_lossy().into_owned();
    let launch_env = scenario
        .steps
        .iter()
        .find_map(|step| match step {
            Step::Launch { env, .. } => Some(env.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let mut env = super::env::build(&root, &scenario.workspace.env, &launch_env)?;
    for name in PTY_ONLY_DEFAULTS {
        if !declares(scenario, &launch_env, name) {
            env.remove(name);
        }
    }
    env.entry("JEFE_SOCKET_PATH".to_string())
        .or_insert_with(|| format!("{root}/jefe-harness.sock"));
    Ok(env.into_iter().collect())
}

/// Whether the scenario itself asked for `name`, at either scope.
fn declares(scenario: &ScenarioV1, launch_env: &[EnvVar], name: &str) -> bool {
    scenario
        .workspace
        .env
        .iter()
        .chain(launch_env)
        .any(|entry| entry.name == name)
}

fn materialize_workspace(scenario: &ScenarioV1, root: &Path) -> Result<(), HarnessError> {
    for dir in &scenario.workspace.dirs {
        std::fs::create_dir_all(root.join(dir.path.as_str())).map_err(|err| {
            HarnessError::process(format!("create '{}': {err}", dir.path.as_str()))
        })?;
    }
    for file in &scenario.workspace.files {
        let path = root.join(file.path.as_str());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                HarnessError::process(format!("create parent of '{}': {err}", file.path.as_str()))
            })?;
        }
        let bytes = match &file.content {
            FileContent::Utf8(text) => text.as_bytes().to_vec(),
            FileContent::Base64(raw) => raw.clone(),
        };
        std::fs::write(&path, bytes).map_err(|err| {
            HarnessError::process(format!("write '{}': {err}", file.path.as_str()))
        })?;
        apply_file_mode(&path, file.mode, file.path.as_str())?;
    }
    Ok(())
}

#[cfg(unix)]
fn apply_file_mode(path: &Path, mode: u32, display_path: &str) -> Result<(), HarnessError> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|err| HarnessError::process(format!("chmod '{display_path}': {err}")))
}

#[cfg(not(unix))]
fn apply_file_mode(path: &Path, _mode: u32, display_path: &str) -> Result<(), HarnessError> {
    std::fs::metadata(path)
        .map(|_| ())
        .map_err(|err| HarnessError::process(format!("verify '{display_path}' after write: {err}")))
}

fn write_failure_artifacts(
    driver: &TmuxDriver,
    session: &crate::harness::tmux_driver::TmuxSession,
    directory: Option<&PathBuf>,
    error: &HarnessError,
) {
    let Some(directory) = directory else {
        return;
    };
    if let Ok(capture) = driver.capture_screen(session) {
        let _ = write_artifact(directory, "final-screen.txt", &capture.lines.join("\n"));
    }
    let _ = write_artifact(directory, "error.txt", &format!("{error}\n"));
}

fn write_artifact(directory: &Path, name: &str, body: &str) -> Result<(), HarnessError> {
    std::fs::create_dir_all(directory).map_err(|err| {
        HarnessError::process(format!(
            "create artifact dir '{}': {err}",
            directory.display()
        ))
    })?;
    std::fs::write(directory.join(name), body)
        .map_err(|err| HarnessError::process(format!("write artifact '{name}': {err}")))
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::harness::v1::contract::{FileSpec, Platform, Size, WorkspaceSpec};

    struct Cleanup(std::path::PathBuf);

    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn tmux_workspace_materialization_preserves_executable_mode() {
        let root =
            std::env::temp_dir().join(format!("jefe-tmux-materialize-mode-{}", std::process::id()));
        std::fs::create_dir_all(&root)
            .unwrap_or_else(|error| panic!("temporary root must create: {error}"));
        let _cleanup = Cleanup(root.clone());
        let path = crate::harness::v1::validate::validate_rel_path("test path", "bin/provider")
            .unwrap_or_else(|error| panic!("fixture path must validate: {error}"));
        let scenario = ScenarioV1 {
            name: "mode preservation".to_owned(),
            platform: Platform::current().unwrap_or(Platform::Linux),
            terminal: Size { cols: 80, rows: 24 },
            workspace: WorkspaceSpec {
                dirs: Vec::new(),
                files: vec![FileSpec {
                    path,
                    content: FileContent::Utf8("#!/bin/sh\n".to_owned()),
                    mode: 0o755,
                }],
                env: Vec::new(),
            },
            steps: Vec::new(),
            secrets: Vec::new(),
        };

        materialize_workspace(&scenario, &root)
            .unwrap_or_else(|error| panic!("workspace must materialize: {error}"));
        let mode = std::fs::metadata(root.join("bin/provider"))
            .unwrap_or_else(|error| panic!("provider fixture must stat: {error}"))
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755);
    }
}
