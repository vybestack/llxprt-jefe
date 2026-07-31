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

use super::contract::{FileContent, ScenarioV1, Step, WaitSource};
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
    .with_keep_session(request.keep_session);
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
    }
    Ok(())
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
