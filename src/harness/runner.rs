//! Scenario runner/orchestrator for the tmux harness.
//!
//! The runner composes typed scenarios, pure matchers, and a driver seam. It is
//! intentionally small: terminal and tmux side effects remain behind
//! [`HarnessDriver`], while scenario execution, polling, and artifact decisions
//! live here.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P04
//! @requirement REQ-TMUX-HARNESS-004

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::capture::{PaneStatus, ScreenCapture, ScrollbackSample};
use super::config::AssertMode;
use super::error::ScenarioError;
use super::expand_macros;
use super::matchers::{
    MatchPattern, history_delta, screen_contains, screen_count, screen_right_edge,
};
use super::scenario::Scenario;
use super::step::Step;
use super::tmux_driver::{
    TmuxDriver, TmuxDriverError, TmuxSession, TmuxSessionGuard, TmuxStartRequest,
};
use tracing::warn;

#[cfg(not(windows))]
const DEFAULT_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(windows)]
const DEFAULT_WAIT_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Result of a scenario run.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P04
/// @requirement REQ-TMUX-HARNESS-004
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummary {
    pub steps_run: usize,
    pub artifact_dir: Option<PathBuf>,
    pub soft_failures: Vec<RunnerFailure>,
    pub multiplexer_details: Option<String>,
}

/// Structured failure details for a step.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P04
/// @requirement REQ-TMUX-HARNESS-004
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerFailure {
    pub step_index: usize,
    pub step_kind: String,
    pub reason: String,
}

/// Errors produced by scenario execution.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P04
/// @requirement REQ-TMUX-HARNESS-004
#[derive(Debug)]
pub enum RunnerError {
    Scenario(ScenarioError),
    Driver(String),
    Assertion(RunnerFailure),
    Artifact { path: PathBuf, reason: String },
}

impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scenario(err) => write!(f, "scenario error: {err}"),
            Self::Driver(reason) => write!(f, "driver error: {reason}"),
            Self::Assertion(failure) => {
                write!(f, "step {} failed: {}", failure.step_index, failure.reason)
            }
            Self::Artifact { path, reason } => {
                write!(f, "artifact error at '{}': {reason}", path.display())
            }
        }
    }
}

impl std::error::Error for RunnerError {}

impl From<ScenarioError> for RunnerError {
    fn from(value: ScenarioError) -> Self {
        Self::Scenario(value)
    }
}

/// Driver seam used by the runner.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P04
/// @requirement REQ-TMUX-HARNESS-004
pub trait HarnessDriver {
    type Error: std::fmt::Display;

    fn send_line(&mut self, line: &str) -> Result<(), Self::Error>;
    /// Send literal text without Enter; scenarios use `line` when submission is intended.
    fn send_type(&mut self, text: &str) -> Result<(), Self::Error>;
    fn send_key(&mut self, key: &str) -> Result<(), Self::Error>;
    fn send_keys(&mut self, keys: &[String]) -> Result<(), Self::Error>;
    fn capture_screen(&mut self) -> Result<ScreenCapture, Self::Error>;
    fn capture_scrollback(&mut self, lines: u32) -> Result<ScrollbackSample, Self::Error>;
    fn pane_status(&mut self) -> Result<PaneStatus, Self::Error>;
    fn history_size(&mut self) -> Result<u64, Self::Error>;
    fn copy_mode(&mut self, enabled: bool) -> Result<(), Self::Error>;
}

/// Concrete adapter from [`TmuxDriver`] plus session handle to the runner seam.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P04
/// @requirement REQ-TMUX-HARNESS-004
pub struct TmuxHarnessDriver {
    driver: TmuxDriver,
    session: TmuxSession,
}

impl TmuxHarnessDriver {
    #[must_use]
    pub const fn new(driver: TmuxDriver, session: TmuxSession) -> Self {
        Self { driver, session }
    }
}

impl HarnessDriver for TmuxHarnessDriver {
    type Error = TmuxDriverError;

    fn send_line(&mut self, line: &str) -> Result<(), Self::Error> {
        self.driver.send_line(&self.session, line)
    }

    fn send_type(&mut self, text: &str) -> Result<(), Self::Error> {
        self.driver.send_type(&self.session, text)
    }

    fn send_key(&mut self, key: &str) -> Result<(), Self::Error> {
        self.driver.send_key(&self.session, key)
    }

    fn send_keys(&mut self, keys: &[String]) -> Result<(), Self::Error> {
        self.driver.send_keys(&self.session, keys)
    }

    fn capture_screen(&mut self) -> Result<ScreenCapture, Self::Error> {
        self.driver.capture_screen(&self.session)
    }

    fn capture_scrollback(&mut self, lines: u32) -> Result<ScrollbackSample, Self::Error> {
        self.driver.capture_scrollback(&self.session, lines)
    }

    fn pane_status(&mut self) -> Result<PaneStatus, Self::Error> {
        self.driver.pane_status(&self.session)
    }

    fn history_size(&mut self) -> Result<u64, Self::Error> {
        self.driver.history_size(&self.session)
    }

    fn copy_mode(&mut self, enabled: bool) -> Result<(), Self::Error> {
        self.driver.copy_mode(&self.session, enabled)
    }
}

/// Run a scenario against an already-started driver seam.
///
/// # Errors
///
/// Returns [`RunnerError`] for invalid scenarios, driver failures, assertion
/// failures, or artifact write failures.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P04
/// @requirement REQ-TMUX-HARNESS-004
pub fn run_scenario<D: HarnessDriver>(
    scenario: &Scenario,
    driver: &mut D,
    artifact_dir: Option<&Path>,
) -> Result<RunSummary, RunnerError> {
    let expanded = expand_macros(scenario)?;
    run_expanded_scenario(&expanded, driver, artifact_dir)
}

fn run_expanded_scenario<D: HarnessDriver>(
    scenario: &Scenario,
    driver: &mut D,
    artifact_dir: Option<&Path>,
) -> Result<RunSummary, RunnerError> {
    let mut context = RunContext::new(
        artifact_dir
            .map(Path::to_path_buf)
            .or_else(|| scenario.config.out_dir.clone()),
        scenario.config.assert_mode,
        effective_wait_timeout(scenario.config.wait_timeout_ms),
    );
    sleep_step(scenario.config.initial_wait_ms);
    run_steps(&scenario.steps, driver, &mut context)
}

/// Start a tmux session, run a scenario, and clean up according to the request.
///
/// # Errors
///
/// Returns [`RunnerError`] if the driver cannot start/cleanup or the scenario
/// run fails.
///
/// # Panics
///
/// Panics if the RAII guard's session is unexpectedly `None` immediately
/// after construction (an invariant violation — the guard is created with
/// a live session).
///
/// @plan PLAN-20260629-TMUX-HARNESS.P04
/// @requirement REQ-TMUX-HARNESS-004
pub fn run_tmux_scenario(
    scenario: &Scenario,
    request: &TmuxStartRequest,
    artifact_dir: Option<&Path>,
) -> Result<RunSummary, RunnerError> {
    let expanded = expand_macros(scenario)?;
    let tmux = TmuxDriver::new();
    // Issue #375: Register signal handlers before starting the session so
    // that an abrupt signal (SIGINT/SIGTERM/SIGHUP/SIGQUIT) kills the
    // harness-owned private-socket tmux server even though Rust Drop is
    // bypassed. On Windows this is a no-op (psmux processes die with parent).
    // The guard drops AFTER `TmuxSessionGuard` (reverse declaration order),
    // so normal-path RAII still tears down the session first.
    let _signal_guard = super::signal_cleanup::SignalCleanupGuard::new(tmux.clone())
        .map_err(|err| RunnerError::Driver(format!("failed to install signal cleanup: {err}")))?;
    let effective_request = request
        .clone()
        .with_keep_session(request.keep_session || expanded.config.keep_session);
    let effective_artifact_dir = artifact_dir
        .map(Path::to_path_buf)
        .or_else(|| expanded.config.out_dir.clone());
    if let Some(directory) = &effective_artifact_dir {
        write_text(directory.join("multiplexer.txt"), &tmux.diagnostics())?;
    }
    let session = match tmux.start_session(&effective_request) {
        Ok(session) => session,
        Err(error) => {
            write_startup_failure_artifact(effective_artifact_dir.as_ref(), &error);
            return Err(RunnerError::Driver(error.to_string()));
        }
    };
    // Issue #301 Phase 6: RAII guard ensures the session is killed on every
    // exit path (success, assertion failure, timeout, panic). Previously
    // `cleanup_session` was called manually after the run, which leaked
    // the session on early-return / panic paths.
    let guard = TmuxSessionGuard::new(tmux.clone(), session);
    // guard.session() is guaranteed Some right after construction.
    let session_ref = guard
        .session()
        .unwrap_or_else(|| panic!("guard must be created with a live session"));
    let mut driver = TmuxHarnessDriver::new(tmux.clone(), session_ref.clone());
    let mut result = run_expanded_scenario(&expanded, &mut driver, artifact_dir);
    if let Ok(summary) = &mut result {
        summary.multiplexer_details = Some(tmux.diagnostics());
    }
    // The guard's Drop kills the session (unless keep_session is true).
    // Explicit drop is unnecessary but documents intent: the session must
    // be torn down before returning the result.
    drop(guard);
    result
}

struct RunContext {
    artifact_dir: Option<PathBuf>,
    assert_mode: AssertMode,
    wait_timeout: Duration,
    history_samples: BTreeMap<String, ScrollbackSample>,
    soft_failures: Vec<RunnerFailure>,
}

impl RunContext {
    fn new(artifact_dir: Option<PathBuf>, assert_mode: AssertMode, wait_timeout: Duration) -> Self {
        Self {
            artifact_dir,
            assert_mode,
            wait_timeout,
            history_samples: BTreeMap::new(),
            soft_failures: Vec::new(),
        }
    }
}

/// Resolve a scenario's wait budget. A zero/absent value keeps the platform
/// default so existing Linux scenarios are unaffected. A non-zero value is an
/// explicit per-scenario override in milliseconds.
///
/// This is a pure function so it can be unit-tested without wall-clock timing.
#[must_use]
fn effective_wait_timeout(wait_timeout_ms: u32) -> Duration {
    if wait_timeout_ms == 0 {
        DEFAULT_WAIT_TIMEOUT
    } else {
        Duration::from_millis(u64::from(wait_timeout_ms))
    }
}

fn run_steps<D: HarnessDriver>(
    steps: &[Step],
    driver: &mut D,
    context: &mut RunContext,
) -> Result<RunSummary, RunnerError> {
    for (index, step) in steps.iter().enumerate() {
        if let Err(err) = execute_step(index, step, driver, context) {
            if let Err(artifact_err) = write_failure_artifacts(driver, context, index, step, &err) {
                warn!(%artifact_err, "failed to write harness failure artifacts");
            }
            return Err(err);
        }
    }
    Ok(RunSummary {
        steps_run: steps.len(),
        artifact_dir: context.artifact_dir.clone(),
        soft_failures: context.soft_failures.clone(),
        multiplexer_details: None,
    })
}

fn execute_step<D: HarnessDriver>(
    index: usize,
    step: &Step,
    driver: &mut D,
    context: &mut RunContext,
) -> Result<(), RunnerError> {
    match step {
        Step::Wait { milliseconds } => {
            sleep_step(*milliseconds);
            Ok(())
        }
        Step::Line { text } => driver_call(driver.send_line(text)),
        Step::Type { text } => driver_call(driver.send_type(text)),
        Step::Key { key } => driver_call(driver.send_key(key)),
        Step::Keys { keys } => driver_call(driver.send_keys(keys)),
        Step::WaitFor { pattern } => wait_for_pattern(index, step, driver, context, pattern, true),
        Step::WaitForNot { pattern } => {
            wait_for_pattern(index, step, driver, context, pattern, false)
        }
        Step::Expect { pattern } => expect_screen(index, step, driver, context, pattern),
        Step::ExpectRightEdge { pattern } => {
            expect_right_edge(index, step, driver, context, pattern)
        }
        Step::ExpectCount { pattern, count } => {
            expect_count(index, step, driver, context, pattern, *count)
        }
        Step::Capture { name } => capture_artifact(driver, context, name),
        Step::HistorySample { name } => history_sample(driver, context, name),
        Step::ExpectHistoryDelta { name } => {
            expect_history_delta(index, step, driver, name, context)
        }
        Step::CopyMode { enabled } => driver_call(driver.copy_mode(*enabled)),
        Step::WaitForExit { timeout_ms } => wait_for_exit(index, step, driver, *timeout_ms),
        Step::Macro { .. } => Err(RunnerError::Scenario(ScenarioError::InvalidStep {
            reason: "macro step remained after expansion".to_string(),
        })),
    }
}

fn sleep_step(milliseconds: u64) {
    std::thread::sleep(Duration::from_millis(milliseconds));
}

fn wait_for_pattern<D: HarnessDriver>(
    index: usize,
    step: &Step,
    driver: &mut D,
    context: &RunContext,
    pattern: &str,
    should_match: bool,
) -> Result<(), RunnerError> {
    poll_until(index, step, context.wait_timeout, || {
        let capture = driver.capture_screen().map_err(driver_error)?;
        let matched = screen_contains(&capture, MatchPattern::literal(pattern)).matched;
        Ok(matched == should_match)
    })
}

fn wait_for_exit<D: HarnessDriver>(
    index: usize,
    step: &Step,
    driver: &mut D,
    timeout_ms: u64,
) -> Result<(), RunnerError> {
    poll_until(index, step, Duration::from_millis(timeout_ms), || {
        let status = driver.pane_status().map_err(driver_error)?;
        Ok(status.dead)
    })
}

fn poll_until<F>(
    index: usize,
    step: &Step,
    timeout: Duration,
    mut predicate: F,
) -> Result<(), RunnerError>
where
    F: FnMut() -> Result<bool, RunnerError>,
{
    let deadline = Instant::now() + timeout;
    loop {
        if predicate()? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(RunnerError::Assertion(failure(
                index,
                step,
                "condition did not become true before timeout".to_string(),
            )));
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn expect_screen<D: HarnessDriver>(
    index: usize,
    step: &Step,
    driver: &mut D,
    context: &mut RunContext,
    pattern: &str,
) -> Result<(), RunnerError> {
    let capture = driver.capture_screen().map_err(driver_error)?;
    let outcome = screen_contains(&capture, MatchPattern::literal(pattern));
    handle_assertion(
        context,
        index,
        step,
        outcome.matched,
        format!("expected screen to contain '{pattern}'"),
    )
}

fn expect_right_edge<D: HarnessDriver>(
    index: usize,
    step: &Step,
    driver: &mut D,
    context: &mut RunContext,
    pattern: &str,
) -> Result<(), RunnerError> {
    let capture = driver.capture_screen().map_err(driver_error)?;
    let outcome = screen_right_edge(&capture, MatchPattern::literal(pattern));
    handle_assertion(
        context,
        index,
        step,
        outcome.matched,
        format!("expected a full-width screen line to end with '{pattern}'"),
    )
}

fn expect_count<D: HarnessDriver>(
    index: usize,
    step: &Step,
    driver: &mut D,
    context: &mut RunContext,
    pattern: &str,
    count: u32,
) -> Result<(), RunnerError> {
    let capture = driver.capture_screen().map_err(driver_error)?;
    let outcome = screen_count(&capture, MatchPattern::literal(pattern), count as usize);
    handle_assertion(
        context,
        index,
        step,
        outcome.matched,
        format!(
            "expected '{pattern}' count {}, got {}",
            count, outcome.actual
        ),
    )
}

fn history_sample<D: HarnessDriver>(
    driver: &mut D,
    context: &mut RunContext,
    name: &str,
) -> Result<(), RunnerError> {
    let history_size = driver.history_size().map_err(driver_error)?;
    let sample = driver.capture_scrollback(200).map_err(driver_error)?;
    let snapshot = ScrollbackSample::new(history_size, sample.lines);
    write_history_sample(context, name, &snapshot)?;
    context.history_samples.insert(name.to_string(), snapshot);
    Ok(())
}

fn expect_history_delta<D: HarnessDriver>(
    index: usize,
    step: &Step,
    driver: &mut D,
    name: &str,
    context: &mut RunContext,
) -> Result<(), RunnerError> {
    let before = context.history_samples.get(name).ok_or_else(|| {
        RunnerError::Assertion(failure(
            index,
            step,
            format!("missing history sample '{name}'"),
        ))
    })?;
    let after = ScrollbackSample::new(
        driver.history_size().map_err(driver_error)?,
        driver.capture_scrollback(200).map_err(driver_error)?.lines,
    );
    let outcome = history_delta(before, &after, 1);
    handle_assertion(
        context,
        index,
        step,
        outcome.matched,
        format!("expected history delta for sample '{name}'"),
    )
}

fn capture_artifact<D: HarnessDriver>(
    driver: &mut D,
    context: &RunContext,
    name: &str,
) -> Result<(), RunnerError> {
    let Some(dir) = &context.artifact_dir else {
        return Ok(());
    };
    let capture = driver.capture_screen().map_err(driver_error)?;
    let label = artifact_label(name);
    write_text(
        dir.join(format!("{label}.screen.txt")),
        &capture.lines.join("\n"),
    )
}

fn write_history_sample(
    context: &RunContext,
    name: &str,
    sample: &ScrollbackSample,
) -> Result<(), RunnerError> {
    let Some(dir) = &context.artifact_dir else {
        return Ok(());
    };
    let label = artifact_label(name);
    write_text(
        dir.join(format!("{label}.history.txt")),
        &sample.lines.join("\n"),
    )
}

fn artifact_label(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "sample".to_string()
    } else {
        sanitized
    }
}
fn handle_assertion(
    context: &mut RunContext,
    index: usize,
    step: &Step,
    matched: bool,
    reason: String,
) -> Result<(), RunnerError> {
    if matched {
        return Ok(());
    }
    let failure = failure(index, step, reason);
    match context.assert_mode {
        AssertMode::Soft => {
            context.soft_failures.push(failure);
            Ok(())
        }
        AssertMode::Strict => Err(RunnerError::Assertion(failure)),
    }
}

fn failure(index: usize, step: &Step, reason: String) -> RunnerFailure {
    RunnerFailure {
        step_index: index,
        step_kind: step_kind(step),
        reason,
    }
}

fn write_failure_artifacts<D: HarnessDriver>(
    driver: &mut D,
    context: &RunContext,
    index: usize,
    step: &Step,
    err: &RunnerError,
) -> Result<(), RunnerError> {
    let Some(dir) = &context.artifact_dir else {
        return Ok(());
    };
    let screen = driver.capture_screen().map_err(driver_error)?;
    let scrollback = driver.capture_scrollback(200).map_err(driver_error)?;
    write_text(dir.join("final-screen.txt"), &screen.lines.join("\n"))?;
    write_text(
        dir.join("final-scrollback.txt"),
        &scrollback.lines.join("\n"),
    )?;
    write_text(
        dir.join("error.txt"),
        &format!(
            "step={index}\nkind={}\nreason={}\nerror={err}\n",
            step_kind(step),
            failure_reason(err)
        ),
    )
}

fn failure_reason(err: &RunnerError) -> String {
    match err {
        RunnerError::Assertion(failure) => failure.reason.clone(),
        other => other.to_string(),
    }
}

fn write_text(path: PathBuf, text: &str) -> Result<(), RunnerError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| RunnerError::Artifact {
            path: parent.to_path_buf(),
            reason: err.to_string(),
        })?;
    }
    fs::write(&path, text).map_err(|err| RunnerError::Artifact {
        path,
        reason: err.to_string(),
    })
}

fn write_startup_failure_artifact(directory: Option<&PathBuf>, error: &TmuxDriverError) {
    if let Some(directory) = directory {
        let artifact = write_text(
            directory.join("error.txt"),
            &format!("startup error: {error}"),
        );
        if let Err(artifact_error) = artifact {
            warn!(%artifact_error, "failed to write harness startup failure artifact");
        }
    }
}

fn driver_call<E: std::fmt::Display>(result: Result<(), E>) -> Result<(), RunnerError> {
    result.map_err(driver_error)
}

fn driver_error<E: std::fmt::Display>(err: E) -> RunnerError {
    RunnerError::Driver(err.to_string())
}

fn step_kind(step: &Step) -> String {
    match step {
        Step::Wait { .. } => "wait",
        Step::Line { .. } => "line",
        Step::Type { .. } => "type",
        Step::Key { .. } => "key",
        Step::Keys { .. } => "keys",
        Step::WaitFor { .. } => "waitFor",
        Step::WaitForNot { .. } => "waitForNot",
        Step::Expect { .. } => "expect",
        Step::ExpectRightEdge { .. } => "expectRightEdge",
        Step::ExpectCount { .. } => "expectCount",
        Step::Capture { .. } => "capture",
        Step::HistorySample { .. } => "historySample",
        Step::ExpectHistoryDelta { .. } => "expectHistoryDelta",
        Step::CopyMode { .. } => "copyMode",
        Step::WaitForExit { .. } => "waitForExit",
        Step::Macro { .. } => "macro",
    }
    .to_string()
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
