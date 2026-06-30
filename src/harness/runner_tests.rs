//! Behavioral tests for the harness runner/orchestrator.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P04
//! @requirement REQ-TMUX-HARNESS-004

use std::collections::VecDeque;
use std::path::PathBuf;

use super::*;
use crate::harness::capture::{PaneStatus, ScreenCapture, ScrollbackSample};
use crate::harness::{TmuxDriver, TmuxPaneSize, TmuxStartRequest, parse_scenario, run_scenario};

trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

fn error_or_panic<T: std::fmt::Debug, E>(result: Result<T, E>, context: &str) -> E {
    match result {
        Err(error) => error,
        Ok(value) => panic!("{context}: unexpectedly succeeded with {value:?}"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FakeError(String);

impl std::fmt::Display for FakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Default)]
struct FakeDriver {
    screens: VecDeque<ScreenCapture>,
    scrollbacks: VecDeque<ScrollbackSample>,
    pane_statuses: VecDeque<PaneStatus>,
    history_sizes: VecDeque<u64>,
    lines_sent: Vec<String>,
    keys_sent: Vec<String>,
    copy_modes: Vec<bool>,
    screen_capture_count: usize,
    fail_screen_after: Option<usize>,
}

impl FakeDriver {
    fn with_screens(mut self, lines: &[&str]) -> Self {
        self.screens = lines
            .iter()
            .map(|line| ScreenCapture::new(1, 80, vec![(*line).to_string()]))
            .collect();
        self
    }

    fn with_scrollback(mut self, lines: &[&str]) -> Self {
        self.scrollbacks = lines
            .iter()
            .enumerate()
            .map(|(idx, line)| ScrollbackSample::new(idx as u64, vec![(*line).to_string()]))
            .collect();
        self
    }
}

impl HarnessDriver for FakeDriver {
    type Error = FakeError;

    fn send_line(&mut self, line: &str) -> Result<(), Self::Error> {
        self.lines_sent.push(line.to_string());
        Ok(())
    }

    fn send_key(&mut self, key: &str) -> Result<(), Self::Error> {
        self.keys_sent.push(key.to_string());
        Ok(())
    }

    fn send_keys(&mut self, keys: &[String]) -> Result<(), Self::Error> {
        self.keys_sent.extend(keys.iter().cloned());
        Ok(())
    }

    fn capture_screen(&mut self) -> Result<ScreenCapture, Self::Error> {
        self.screen_capture_count += 1;
        if self
            .fail_screen_after
            .is_some_and(|threshold| self.screen_capture_count > threshold)
        {
            return Err(FakeError("screen capture failed".to_string()));
        }
        Ok(self
            .screens
            .pop_front()
            .unwrap_or_else(|| ScreenCapture::new(1, 80, vec![String::new()])))
    }

    fn capture_scrollback(&mut self, _lines: u32) -> Result<ScrollbackSample, Self::Error> {
        Ok(self
            .scrollbacks
            .pop_front()
            .unwrap_or_else(|| ScrollbackSample::new(0, Vec::new())))
    }

    fn pane_status(&mut self) -> Result<PaneStatus, Self::Error> {
        Ok(self
            .pane_statuses
            .pop_front()
            .unwrap_or(PaneStatus { dead: false }))
    }

    fn history_size(&mut self) -> Result<u64, Self::Error> {
        Ok(self.history_sizes.pop_front().unwrap_or(0))
    }

    fn copy_mode(&mut self, enabled: bool) -> Result<(), Self::Error> {
        self.copy_modes.push(enabled);
        Ok(())
    }
}

fn scenario(json_steps: &str) -> crate::harness::Scenario {
    parse_scenario(&format!(
        r#"{{ "config": {{ "cols": 80, "rows": 24 }}, "steps": {json_steps} }}"#
    ))
    .value_or_panic("scenario should parse")
}

#[test]
fn expect_passes_when_screen_contains_literal() {
    let scenario = scenario(r#"[ { "expect": "ready" } ]"#);
    let mut driver = FakeDriver::default().with_screens(&["system ready"]);

    let summary = run_scenario(&scenario, &mut driver, None).value_or_panic("runner should pass");

    assert_eq!(summary.steps_run, 1);
    assert!(summary.soft_failures.is_empty());
}

#[test]
fn expect_failure_writes_artifacts() {
    let scenario = scenario(r#"[ { "expect": "ready" } ]"#);
    let artifact_dir = tempfile::tempdir().value_or_panic("tempdir");
    let mut driver = FakeDriver::default()
        .with_screens(&["not yet", "final screen"])
        .with_scrollback(&["history tail"]);

    let err = error_or_panic(
        run_scenario(&scenario, &mut driver, Some(artifact_dir.path())),
        "expect should fail",
    );

    assert!(matches!(err, RunnerError::Assertion(_)));
    assert!(artifact_dir.path().join("final-screen.txt").exists());
    assert!(artifact_dir.path().join("final-scrollback.txt").exists());
    assert!(artifact_dir.path().join("error.txt").exists());
}

#[test]
fn wait_for_succeeds_when_later_capture_matches() {
    let scenario = scenario(r#"[ { "waitFor": "ready" } ]"#);
    let mut driver = FakeDriver::default().with_screens(&["loading", "still loading", "ready"]);

    let summary = run_scenario(&scenario, &mut driver, None).value_or_panic("waitFor should pass");

    assert_eq!(summary.steps_run, 1);
}

#[test]
fn line_key_keys_and_copy_mode_forward_to_driver() {
    let scenario = scenario(
        r#"[
            { "line": "hello" },
            { "key": "Enter" },
            { "keys": ["C-b", "["] },
            { "copyMode": true },
            { "copyMode": false }
        ]"#,
    );
    let mut driver = FakeDriver::default();

    let _summary = run_scenario(&scenario, &mut driver, None).value_or_panic("runner should pass");

    assert_eq!(driver.lines_sent, ["hello"]);
    assert_eq!(driver.keys_sent, ["Enter", "C-b", "["]);
    assert_eq!(driver.copy_modes, [true, false]);
}

#[test]
fn capture_step_writes_named_screen_artifact() {
    let scenario = scenario(r#"[ { "capture": "first" } ]"#);
    let artifact_dir = tempfile::tempdir().value_or_panic("tempdir");
    let mut driver = FakeDriver::default().with_screens(&["first screen"]);

    let summary = run_scenario(&scenario, &mut driver, Some(artifact_dir.path()))
        .value_or_panic("capture should pass");

    assert_eq!(
        summary.artifact_dir,
        Some(PathBuf::from(artifact_dir.path()))
    );
    assert!(artifact_dir.path().join("first.screen.txt").exists());
}

#[test]
fn capture_name_is_sanitized_inside_artifact_dir() {
    let scenario = scenario(r#"[ { "capture": "../first" } ]"#);
    let artifact_dir = tempfile::tempdir().value_or_panic("tempdir");
    let mut driver = FakeDriver::default().with_screens(&["first screen"]);

    let _summary = run_scenario(&scenario, &mut driver, Some(artifact_dir.path()))
        .value_or_panic("capture should pass");

    assert!(artifact_dir.path().join("---first.screen.txt").exists());
    assert!(
        !artifact_dir
            .path()
            .join("..")
            .join("first.screen.txt")
            .exists()
    );
}

#[test]
fn scenario_config_out_dir_is_used_when_no_explicit_artifact_dir_is_passed() {
    let artifact_dir = tempfile::tempdir().value_or_panic("tempdir");
    let json = format!(
        r#"{{
            "config": {{ "cols": 80, "rows": 24, "out_dir": "{}" }},
            "steps": [ {{ "capture": "screen" }} ]
        }}"#,
        artifact_dir.path().display()
    );
    let scenario = parse_scenario(&json).value_or_panic("scenario should parse");
    let mut driver = FakeDriver::default().with_screens(&["from config"]);

    let summary = run_scenario(&scenario, &mut driver, None).value_or_panic("run should pass");

    assert_eq!(
        summary.artifact_dir,
        Some(PathBuf::from(artifact_dir.path()))
    );
    assert!(artifact_dir.path().join("screen.screen.txt").exists());
}

#[test]
fn history_delta_uses_named_sample() {
    let scenario = scenario(r#"[ { "historySample": "a" }, { "expectHistoryDelta": "a" } ]"#);
    let mut driver = FakeDriver {
        history_sizes: VecDeque::from([1, 3]),
        scrollbacks: VecDeque::from([
            ScrollbackSample::new(1, vec!["one".to_string()]),
            ScrollbackSample::new(3, vec!["two".to_string()]),
        ]),
        ..FakeDriver::default()
    };

    let summary = run_scenario(&scenario, &mut driver, None).value_or_panic("delta should pass");

    assert_eq!(summary.steps_run, 2);
}

#[test]
fn wait_for_exit_polls_until_pane_dead() {
    let scenario = scenario(r#"[ { "waitForExit": 500 } ]"#);
    let mut driver = FakeDriver {
        pane_statuses: VecDeque::from([PaneStatus { dead: false }, PaneStatus { dead: true }]),
        ..FakeDriver::default()
    };

    let summary =
        run_scenario(&scenario, &mut driver, None).value_or_panic("exit wait should pass");

    assert_eq!(summary.steps_run, 1);
}

#[test]
fn wait_for_not_and_expect_count_execute_matchers() {
    let scenario = scenario(r#"[ { "waitForNot": "gone" }, { "expectCount": "ok", "count": 2 } ]"#);
    let mut driver = FakeDriver::default().with_screens(&["ready", "ok ok"]);

    let summary = run_scenario(&scenario, &mut driver, None).value_or_panic("matchers should pass");

    assert_eq!(summary.steps_run, 2);
}

#[test]
fn timeout_failure_uses_failing_step_context() {
    let scenario = scenario(r#"[ { "line": "before" }, { "waitForExit": 1 } ]"#);
    let mut driver = FakeDriver {
        pane_statuses: VecDeque::from([PaneStatus { dead: false }]),
        ..FakeDriver::default()
    };

    let err = error_or_panic(
        run_scenario(&scenario, &mut driver, None),
        "waitForExit should fail",
    );

    match err {
        RunnerError::Assertion(failure) => {
            assert_eq!(failure.step_index, 1);
            assert_eq!(failure.step_kind, "waitForExit");
            assert!(failure.reason.contains("timeout"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn soft_assertion_mode_records_failure_and_continues() {
    let scenario = parse_scenario(
        r#"{
            "config": { "cols": 80, "rows": 24, "assert_mode": "soft" },
            "steps": [ { "expect": "missing" }, { "line": "after" } ]
        }"#,
    )
    .value_or_panic("scenario should parse");
    let mut driver = FakeDriver::default().with_screens(&["different"]);

    let summary =
        run_scenario(&scenario, &mut driver, None).value_or_panic("soft run should continue");

    assert_eq!(summary.steps_run, 2);
    assert_eq!(summary.soft_failures.len(), 1);
    assert_eq!(driver.lines_sent, ["after"]);
}

#[test]
fn history_sample_writes_labeled_artifact() {
    let scenario = scenario(r#"[ { "historySample": "before/typing" } ]"#);
    let artifact_dir = tempfile::tempdir().value_or_panic("tempdir");
    let mut driver = FakeDriver {
        history_sizes: VecDeque::from([3]),
        scrollbacks: VecDeque::from([ScrollbackSample::new(3, vec!["history".to_string()])]),
        ..FakeDriver::default()
    };

    let _summary = run_scenario(&scenario, &mut driver, Some(artifact_dir.path()))
        .value_or_panic("history sample should pass");

    assert!(
        artifact_dir
            .path()
            .join("before-typing.history.txt")
            .exists()
    );
}

#[test]
fn driver_error_during_assertion_is_propagated() {
    let scenario = scenario(r#"[ { "expect": "ready" } ]"#);
    let mut driver = FakeDriver {
        fail_screen_after: Some(0),
        ..FakeDriver::default()
    };

    let err = error_or_panic(
        run_scenario(&scenario, &mut driver, None),
        "driver should fail",
    );

    assert!(matches!(err, RunnerError::Driver(_)));
}

#[test]
fn artifact_capture_failure_preserves_original_assertion() {
    let scenario = scenario(r#"[ { "expect": "ready" } ]"#);
    let artifact_dir = tempfile::tempdir().value_or_panic("tempdir");
    let mut driver = FakeDriver {
        screens: VecDeque::from([ScreenCapture::new(1, 80, vec!["nope".to_string()])]),
        fail_screen_after: Some(1),
        ..FakeDriver::default()
    };

    let err = error_or_panic(
        run_scenario(&scenario, &mut driver, Some(artifact_dir.path())),
        "expect should fail",
    );

    assert!(matches!(err, RunnerError::Assertion(_)));
}

#[test]
fn guarded_real_jefe_runner_scenario_starts_and_quits() {
    let tmux = TmuxDriver::new();
    if !tmux.is_available() {
        let _ = std::io::Write::write_all(
            &mut std::io::stderr(),
            b"skipping real runner test: tmux unavailable\n",
        );
        return;
    }
    let Some(jefe_binary) = jefe_binary_path() else {
        let _ = std::io::Write::write_all(
            &mut std::io::stderr(),
            b"skipping real runner test: jefe binary unavailable\n",
        );
        return;
    };
    let config_dir = tempfile::tempdir().value_or_panic("isolated config tempdir");
    let scenario = scenario(
        r#"[
            { "waitFor": "LLxprt Jefe" },
            { "key": "q" },
            { "waitForExit": 3000 }
        ]"#,
    );
    let session_name = unique_session("runner-jefe");
    let request = TmuxStartRequest::jefe(
        session_name,
        jefe_binary,
        config_dir.path(),
        std::env::current_dir().value_or_panic("current dir"),
        TmuxPaneSize::new(100, 30, 2_000),
    )
    .value_or_panic("jefe request");

    let summary =
        run_tmux_scenario(&scenario, &request, None).value_or_panic("real runner scenario");

    assert_eq!(summary.steps_run, 3);
}

fn jefe_binary_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_jefe") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    let current = std::env::current_exe().ok()?;
    let deps_dir = current.parent()?;
    let debug_dir = deps_dir.parent()?;
    let candidate = debug_dir.join("jefe");
    candidate.exists().then_some(candidate)
}

fn unique_session(label: &str) -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("jefe-runner-{label}-{pid}-{nanos}")
}
