//! Synchronous schema-1 operation state machine (issue #380).
//!
//! Executes a validated scenario end to end: workspace creation, fixture
//! materialization, capture registration, real-PTY launch, sequential step
//! execution with bounded literal waits, exact-size resize acknowledgement,
//! restart with durable-file preservation, and finish with escalating
//! process-group teardown. The first failure stops later steps, performs the
//! same cleanup, retains the workspace and a bounded report, and permits a
//! fresh run.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::capture;
use super::contract::{
    CaptureExpectation, FileExpectation, Modifier, ScenarioV1, Size, Step, WaitSource,
};
use super::env;
use super::error::HarnessError;
use super::interp;
use super::keys;
use super::pty::{POLL_INTERVAL, ProcessExit, PtySession};
use super::report::{AppExit, CaptureReport, Frame, Report, StepResult};
use super::workspace::Workspace;

/// Resize acknowledgement shares the wait contract's upper bound.
const RESIZE_ACK_TIMEOUT: Duration = Duration::from_secs(10);

/// The outcome of a run: the report plus the overall error, if any.
pub struct RunOutcome {
    pub report: Report,
    pub error: Option<HarnessError>,
}

/// Configuration for a run.
pub struct RunnerConfig {
    /// The capture shim fixture binary.
    pub shim_binary: PathBuf,
    /// Host binaries to materialize into `bin/<name>` inside the workspace
    /// before any step runs (the app-under-test enters the hermetic world
    /// this way; launch still resolves only against the explicit PATH).
    pub installs: Vec<(String, PathBuf)>,
}

/// Execute a validated scenario. Always returns a report; `error` carries
/// the first failure and its exit mapping.
#[must_use]
pub fn run(scenario: &ScenarioV1, config: &RunnerConfig) -> RunOutcome {
    let workspace = match Workspace::create(&scenario.workspace) {
        Ok(workspace) => workspace,
        Err(err) => {
            let mut report = Report::new(&scenario.name, "");
            report.status = "failed".to_string();
            return RunOutcome {
                report,
                error: Some(err),
            };
        }
    };
    let root = workspace.root().to_string_lossy().into_owned();
    let mut state = RunState {
        scenario,
        config,
        workspace,
        session: None,
        capture_names: Vec::new(),
        report: Report::new(&scenario.name, &root),
    };
    let run_error = state.install_binaries().err().or_else(|| state.execute());
    let capture_error = state.load_capture_reports();
    let error = run_error.or(capture_error);
    if error.is_some() {
        state.report.status = "failed".to_string();
    }
    RunOutcome {
        report: state.report,
        error,
    }
}

struct RunState<'a> {
    scenario: &'a ScenarioV1,
    config: &'a RunnerConfig,
    workspace: Workspace,
    session: Option<PtySession>,
    capture_names: Vec<String>,
    report: Report,
}

impl RunState<'_> {
    /// Copy configured host binaries into `bin/<name>` (mode 0755) so the
    /// hermetic PATH can resolve them.
    fn install_binaries(&mut self) -> Result<(), HarnessError> {
        use super::contract::RelPath;
        for (name, source) in &self.config.installs {
            let target_rel = RelPath::derived(format!("bin/{name}"));
            let target = self.workspace.resolve(&target_rel)?;
            std::fs::copy(source, &target).map_err(|err| {
                HarnessError::process(format!("install '{name}' into workspace: {err}"))
            })?;
            std::fs::set_permissions(&target, std::os::unix::fs::PermissionsExt::from_mode(0o755))
                .map_err(|err| HarnessError::process(format!("chmod installed '{name}': {err}")))?;
        }
        Ok(())
    }

    fn execute(&mut self) -> Option<HarnessError> {
        for (index, step) in self.scenario.steps.iter().enumerate() {
            let result = self.execute_step(step);
            self.report.steps.push(StepResult {
                index,
                op: step.op_name().to_string(),
                status: if result.is_ok() { "passed" } else { "failed" }.to_string(),
                error: result.as_ref().err().map(ToString::to_string),
            });
            if let Err(err) = result {
                self.cleanup_after_failure();
                return Some(err);
            }
        }
        // A scenario without an explicit finish still tears down the app.
        if self.session.is_some()
            && let Err(err) = self.finish()
        {
            self.report.steps.push(StepResult {
                index: self.scenario.steps.len(),
                op: "finish".to_string(),
                status: "failed".to_string(),
                error: Some(err.to_string()),
            });
            return Some(err);
        }
        None
    }

    fn execute_step(&mut self, step: &Step) -> Result<(), HarnessError> {
        match step {
            Step::Write { file } => self.workspace.write_file(file),
            Step::Mkdir { dir } => self.workspace.mkdir(dir),
            Step::Remove { path } => self.workspace.remove(path),
            Step::Capture {
                name,
                path,
                behavior,
            } => {
                capture::register(
                    &mut self.workspace,
                    &self.config.shim_binary,
                    name,
                    path,
                    behavior,
                )?;
                self.capture_names.push(name.clone());
                Ok(())
            }
            Step::Launch { argv, env, cwd } => self.launch(argv, env, cwd),
            Step::Key { key, modifiers } => self.send_key(key, modifiers),
            Step::Text { text } => self.session_mut()?.write_bytes(text.as_bytes()),
            Step::Resize { size } => self.resize(*size),
            Step::Wait {
                source,
                literal,
                timeout_ms,
            } => self.wait_for(*source, literal, *timeout_ms),
            Step::AssertFrame { contains, absent } => self.assert_frame(contains, absent),
            Step::AssertCapture { capture } => self.assert_capture(capture),
            Step::AssertFile { file } => self.assert_file(file),
            Step::Restart => self.restart(),
            Step::Finish => self.finish(),
        }
    }

    fn session_mut(&mut self) -> Result<&mut PtySession, HarnessError> {
        self.session
            .as_mut()
            .ok_or_else(|| HarnessError::process("no application is running".to_string()))
    }

    fn launch(
        &mut self,
        argv: &[String],
        launch_env: &[super::contract::EnvVar],
        cwd: &super::contract::RelPath,
    ) -> Result<(), HarnessError> {
        let root = self.workspace.root().to_string_lossy().into_owned();
        let environment = env::build(&root, &self.scenario.workspace.env, launch_env)?;
        let cwd_abs = self.workspace.resolve(cwd)?;
        if !cwd_abs.is_dir() {
            return Err(HarnessError::process(format!(
                "launch cwd '{}' is not a directory",
                cwd.as_str()
            )));
        }
        let argv = interpolate_argv(argv, &root)?;
        let resolved = resolve_program(&argv, &environment)?;
        let session =
            PtySession::launch(&resolved, &environment, &cwd_abs, self.scenario.terminal)?;
        self.session = Some(session);
        Ok(())
    }

    fn send_key(&mut self, key: &str, modifiers: &[Modifier]) -> Result<(), HarnessError> {
        let bytes = keys::encode("key", key, modifiers)?;
        self.session_mut()?.write_bytes(&bytes)
    }

    fn resize(&mut self, size: Size) -> Result<(), HarnessError> {
        let session = self.session_mut()?;
        let generation = session.generation();
        session.resize(size)?;
        // Acknowledge only after the app emits fresh output following resize.
        // The terminal model itself has exact typed dimensions; rendered rows
        // are intentionally right-trimmed and therefore need not be `cols`
        // characters wide.
        let deadline = Instant::now() + RESIZE_ACK_TIMEOUT;
        loop {
            let session = self.session_mut()?;
            let lines = session.frame_lines()?;
            if session.generation() > generation
                && session.size() == size
                && lines.len() == size.rows as usize
            {
                let frame = Frame {
                    cols: size.cols,
                    rows: size.rows,
                    lines,
                };
                self.report.push_frame(frame);
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(HarnessError::wait_timeout(format!(
                    "resize to {}x{} was not acknowledged",
                    size.cols, size.rows
                )));
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    fn wait_for(
        &mut self,
        source: WaitSource,
        literal: &str,
        timeout_ms: u64,
    ) -> Result<(), HarnessError> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let session = self.session_mut()?;
            let found = match source {
                WaitSource::Frame => session
                    .frame_lines()?
                    .iter()
                    .any(|line| line.contains(literal)),
                // One real PTY merges the app's stdout and stderr; both
                // sources scan the merged byte stream by contract.
                WaitSource::Stdout | WaitSource::Stderr => session.stream_text()?.contains(literal),
            };
            if found {
                self.record_frame()?;
                return Ok(());
            }
            if Instant::now() >= deadline {
                self.record_frame()?;
                return Err(HarnessError::wait_timeout(format!(
                    "literal '{literal}' not observed within {timeout_ms} ms"
                )));
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    fn assert_frame(&mut self, contains: &[String], absent: &[String]) -> Result<(), HarnessError> {
        let lines = self.session_mut()?.frame_lines()?;
        self.record_frame()?;
        for needle in contains {
            if !lines.iter().any(|line| line.contains(needle.as_str())) {
                return Err(HarnessError::assertion(format!(
                    "frame does not contain '{needle}'"
                )));
            }
        }
        for needle in absent {
            if lines.iter().any(|line| line.contains(needle.as_str())) {
                return Err(HarnessError::assertion(format!(
                    "frame contains forbidden '{needle}'"
                )));
            }
        }
        Ok(())
    }

    fn assert_capture(&self, expectation: &CaptureExpectation) -> Result<(), HarnessError> {
        let records = capture::load_records(self.workspace.root(), &expectation.name)?;
        let root = self.workspace.root().to_string_lossy().into_owned();
        capture::check_expectation(&records, expectation, &root)
    }

    fn assert_file(&mut self, expectation: &FileExpectation) -> Result<(), HarnessError> {
        let exists = self.workspace.exists(&expectation.path)?;
        if exists != expectation.exists {
            return Err(HarnessError::assertion(format!(
                "file '{}' exists={exists}, expected exists={}",
                expectation.path.as_str(),
                expectation.exists
            )));
        }
        if let Some(content) = &expectation.content {
            let actual = self.workspace.read_file(&expectation.path)?;
            if actual != content.bytes() {
                return Err(HarnessError::assertion(format!(
                    "file '{}' content mismatch ({} bytes recorded, {} expected)",
                    expectation.path.as_str(),
                    actual.len(),
                    content.bytes().len()
                )));
            }
        }
        Ok(())
    }

    /// Restart: terminate/reap the old group, then relaunch the same argv in
    /// the same workspace. Durable files survive; processes, PTY buffers,
    /// and frames do not.
    fn restart(&mut self) -> Result<(), HarnessError> {
        let mut session = self
            .session
            .take()
            .ok_or_else(|| HarnessError::process("no application to restart".to_string()))?;
        session.stop()?;
        drop(session);
        let launch = self
            .scenario
            .steps
            .iter()
            .find_map(|step| match step {
                Step::Launch { argv, env, cwd } => Some((argv.clone(), env.clone(), cwd.clone())),
                _ => None,
            })
            .ok_or_else(|| HarnessError::process("scenario has no launch step".to_string()))?;
        self.launch(&launch.0, &launch.1, &launch.2)
    }

    /// Finish: graceful stop then escalation, always reaping the group.
    fn finish(&mut self) -> Result<(), HarnessError> {
        let Some(mut session) = self.session.take() else {
            return Ok(());
        };
        let exit = session.stop()?;
        self.record_exit(exit);
        Ok(())
    }

    fn record_exit(&mut self, exit: ProcessExit) {
        self.report.app_exit = Some(AppExit {
            exit_code: exit.exit_code,
        });
    }

    fn record_frame(&mut self) -> Result<(), HarnessError> {
        if let Some(session) = &self.session {
            let size = session.size();
            self.report.push_frame(Frame {
                cols: size.cols,
                rows: size.rows,
                lines: session.frame_lines()?,
            });
        }
        Ok(())
    }

    /// On failure: stop the app (best effort, preserving the original
    /// error), keep the workspace, and keep the bounded report.
    fn cleanup_after_failure(&mut self) {
        if let Some(mut session) = self.session.take() {
            match session.stop() {
                Ok(exit) => self.record_exit(exit),
                Err(err) => {
                    self.report.steps.push(StepResult {
                        index: self.scenario.steps.len(),
                        op: "cleanup".to_string(),
                        status: "failed".to_string(),
                        error: Some(err.to_string()),
                    });
                }
            }
        }
    }

    fn load_capture_reports(&mut self) -> Option<HarnessError> {
        for name in &self.capture_names {
            match capture::load_records(self.workspace.root(), name) {
                Ok(invocations) => self.report.captures.push(CaptureReport {
                    name: name.clone(),
                    invocations,
                }),
                Err(err) => return Some(err),
            }
        }
        None
    }
}

fn interpolate_argv(argv: &[String], root: &str) -> Result<Vec<String>, HarnessError> {
    argv.iter()
        .enumerate()
        .map(|(index, arg)| interp::apply(&format!("launch.argv[{index}]"), arg, root))
        .collect()
}

/// Resolve argv[0]: absolute paths are used as-is; bare names resolve only
/// against the explicit environment PATH (no host lookup).
fn resolve_program(
    argv: &[String],
    environment: &BTreeMap<String, String>,
) -> Result<Vec<String>, HarnessError> {
    let Some(program) = argv.first() else {
        return Err(HarnessError::process("launch argv is empty".to_string()));
    };
    if program.starts_with('/') {
        return Ok(argv.to_vec());
    }
    let path_value = environment.get("PATH").cloned().unwrap_or_default();
    for dir in path_value.split(':').filter(|dir| !dir.is_empty()) {
        let candidate = std::path::Path::new(dir).join(program);
        if candidate.is_file() {
            let mut resolved = argv.to_vec();
            resolved[0] = candidate.to_string_lossy().into_owned();
            return Ok(resolved);
        }
    }
    Err(HarnessError::process(format!(
        "program '{program}' not found on the explicit PATH"
    )))
}
