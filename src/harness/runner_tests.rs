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
    let out_dir = artifact_dir.path().to_string_lossy().replace('\\', "\\\\");
    let json = format!(
        r#"{{
            "config": {{ "cols": 80, "rows": 24, "out_dir": "{out_dir}" }},
            "steps": [ {{ "capture": "screen" }} ]
        }}"#,
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

/// Resolve the jefe binary for a guarded integration test.
///
/// Prints a labeled skip reason and returns `None` when tmux or the jefe
/// binary is unavailable. `context` labels the message (e.g. "real runner
/// test") so it's clear which scenario was skipped.
fn guarded_jefe_binary(context: &str) -> Option<PathBuf> {
    let tmux = TmuxDriver::new();
    if !tmux.is_available() {
        let _ = std::io::Write::write_all(
            &mut std::io::stderr(),
            format!("skipping {context}: tmux unavailable\n").as_bytes(),
        );
        return None;
    }
    let binary = jefe_binary_path();
    if binary.is_none() {
        let _ = std::io::Write::write_all(
            &mut std::io::stderr(),
            format!("skipping {context}: jefe binary unavailable\n").as_bytes(),
        );
    }
    binary
}

#[test]
fn guarded_real_jefe_runner_scenario_starts_and_quits() {
    let Some(jefe_binary) = guarded_jefe_binary("real runner test") else {
        return;
    };
    let config_dir = tempfile::tempdir().value_or_panic("isolated config tempdir");
    let scenario = scenario(
        r#"[
            { "waitFor": "LLxprt Jefe" },
            { "key": "C-q" },
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

/// Rapid triple-`q` (`qqq`) quits the app — behavioral proof of the quit
/// sequence fallback (issue #129). Three bare `q`s sent back-to-back land
/// within the 1s window, so the app exits.
#[test]
fn guarded_real_jefe_qqq_quits() {
    let Some(jefe_binary) = guarded_jefe_binary("qqq quit test") else {
        return;
    };
    let config_dir = tempfile::tempdir().value_or_panic("isolated config tempdir");
    let scenario = scenario(
        r#"[
            { "waitFor": "LLxprt Jefe" },
            { "key": "q" },
            { "key": "q" },
            { "key": "q" },
            { "waitForExit": 3000 }
        ]"#,
    );
    let session_name = unique_session("qqq-jefe");
    let request = TmuxStartRequest::jefe(
        session_name,
        jefe_binary,
        config_dir.path(),
        std::env::current_dir().value_or_panic("current dir"),
        TmuxPaneSize::new(100, 30, 2_000),
    )
    .value_or_panic("jefe request");

    let summary = run_tmux_scenario(&scenario, &request, None).value_or_panic("qqq quit scenario");

    assert_eq!(summary.steps_run, 5);
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
    let candidate = debug_dir.join(format!("jefe{}", std::env::consts::EXE_SUFFIX));
    candidate.exists().then_some(candidate)
}

fn unique_session(label: &str) -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("jefe-runner-{label}-{pid}-{nanos}")
}

/// Issue #116: When active-only mode is ON and the user kills an agent, the
/// dead agent should remain visible (sticky) until the user navigates away.
///
/// This scenario pre-creates a tmux session running `sleep 300` so jefe's
/// runtime kill can actually succeed, seeds a state.json with a Running agent
/// bound to that session, then drives the real jefe binary through the
/// kill → still-visible → navigate → filtered → quit flow.
#[cfg(unix)]
#[test]
fn guarded_real_jefe_sticky_kill_scenario() {
    let Some(jefe_binary) = guarded_jefe_binary("sticky kill test") else {
        return;
    };

    let unique = unique_session("stickyagent");
    let agent_session = format!("jefe-stickyagent-{unique}");

    // Pre-create a tmux session running sleep on jefe's dedicated socket so
    // jefe's session-exists check (which now targets the private socket) finds
    // it. The session runs `sleep 300` so jefe's kill can target it.
    let session_ok = create_sleep_session_on_jefe_socket(&agent_session, 300);

    if !session_ok {
        let _ = std::io::Write::write_all(
            &mut std::io::stderr(),
            b"skipping sticky kill test: could not pre-create agent tmux session\n",
        );
        return;
    }

    // Guard ensures the pre-created session is cleaned up even on panic.
    let _cleanup = TmuxSessionCleanup {
        session_name: agent_session.clone(),
    };

    let config_dir = tempfile::tempdir().value_or_panic("isolated config tempdir");
    seed_sticky_agent_state(config_dir.path(), &agent_session);

    let summary = run_sticky_scenario(&jefe_binary, config_dir.path());
    assert_eq!(summary.steps_run, 13);
}

/// Seed a config directory with a state.json containing a single Running agent
/// bound to the given tmux session name (issue #116 scenario fixture).
#[cfg(unix)]
fn seed_sticky_agent_state(config_dir: &std::path::Path, agent_session: &str) {
    use crate::domain::{
        Agent, AgentId, AgentStatus, DEFAULT_SANDBOX_FLAGS, LaunchSignature,
        RemoteRepositorySettings, Repository, RepositoryId, RuntimeBinding, SandboxEngine,
    };
    use crate::persistence::{FilePersistenceManager, PersistenceManager, PersistencePaths, State};

    let mut agent = Agent::new(
        AgentId("stickyagent".into()),
        RepositoryId("testrepo".into()),
        "StickyAgent".into(),
        std::path::PathBuf::from("/tmp"),
    );
    agent.status = AgentStatus::Running;
    agent.shortcut_slot = Some(1);
    agent.runtime_binding = Some(RuntimeBinding {
        session_name: agent_session.to_string(),
        launch_signature: LaunchSignature {
            work_dir: std::path::PathBuf::from("/tmp"),
            profile: String::new(),
            code_puppy_model: String::new(),
            code_puppy_yolo: Some(false),
            code_puppy_quick_resume: false,
            mode_flags: vec![],
            llxprt_debug: String::new(),
            pass_continue: true,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: DEFAULT_SANDBOX_FLAGS.to_owned(),
            remote: RemoteRepositorySettings::default(),
            agent_kind: crate::domain::AgentKind::Llxprt,
        },
        attached: false,
        last_seen: None,
        process_identity: None,
        pid: None,
    });

    let persisted_state = State {
        schema_version: crate::persistence::STATE_SCHEMA_VERSION,
        repositories: vec![Repository::new(
            RepositoryId("testrepo".into()),
            "TestRepo".into(),
            "testrepo".into(),
            std::path::PathBuf::from("/tmp"),
        )],
        agents: vec![agent],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        hide_idle_repositories: false,
        last_selected_agent_by_repo: vec![],
        pane_focus: String::new(),
        terminal_focused: false,
        user_preferences: crate::domain::UserPreferences::default(),
    };
    let paths = PersistencePaths {
        settings_path: config_dir.join("settings.toml"),
        state_path: config_dir.join("state.json"),
    };
    let persistence = FilePersistenceManager::with_paths(paths);
    persistence
        .save_state(&persisted_state)
        .unwrap_or_else(|e| panic!("save state: {e:?}"));
}

/// Run the issue #116 sticky-kill TUI scenario against the real jefe binary.
#[cfg(unix)]
fn run_sticky_scenario(jefe_binary: &std::path::Path, config_dir: &std::path::Path) -> RunSummary {
    let scenario = scenario(
        r#"[
            { "waitFor": "LLxprt Jefe" },
            { "key": "v" },
            { "wait": 300 },
            { "key": "Tab" },
            { "wait": 200 },
            { "key": "C-k" },
            { "wait": 1000 },
            { "expect": "StickyAgent" },
            { "key": "Down" },
            { "wait": 500 },
            { "waitForNot": "StickyAgent" },
            { "key": "C-q" },
            { "waitForExit": 3000 }
        ]"#,
    );
    let session_name = unique_session("sticky-jefe");
    let request = TmuxStartRequest::jefe(
        session_name,
        jefe_binary.to_path_buf(),
        config_dir,
        std::env::current_dir().value_or_panic("current dir"),
        TmuxPaneSize::new(100, 30, 2_000),
    )
    .value_or_panic("jefe request");

    run_tmux_scenario(&scenario, &request, None).value_or_panic("run sticky scenario")
}

/// Pre-create a tmux session running `sleep <seconds>` on jefe's dedicated
/// socket so jefe's session-exists check (which targets the private socket)
/// finds it. Returns `true` on success.
#[cfg(unix)]
fn create_sleep_session_on_jefe_socket(session_name: &str, seconds: u64) -> bool {
    let jefe_socket = crate::runtime::jefe_tmux_socket_path();
    match std::process::Command::new("tmux")
        .args([
            "-S",
            &jefe_socket.to_string_lossy(),
            "new-session",
            "-d",
            "-s",
            session_name,
            "--",
            "sleep",
            &seconds.to_string(),
        ])
        .output()
    {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            tracing::warn!(
                stderr = %String::from_utf8_lossy(&output.stderr),
                "create_sleep_session_on_jefe_socket failed"
            );
            false
        }
        Err(error) => {
            tracing::warn!(
                %error,
                "create_sleep_session_on_jefe_socket failed to spawn tmux"
            );
            false
        }
    }
}

/// Capture pane text for a session on jefe's dedicated socket.
#[cfg(unix)]
fn capture_jefe_pane(session_name: &str) -> Option<String> {
    let jefe_socket = crate::runtime::jefe_tmux_socket_path();
    let output = std::process::Command::new("tmux")
        .args([
            "-S",
            &jefe_socket.to_string_lossy(),
            "capture-pane",
            "-t",
            session_name,
            "-p",
            "-S",
            "-",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// RAII guard that kills a pre-created tmux session on drop, ensuring cleanup
/// even if the test panics mid-scenario.
#[cfg(unix)]
struct TmuxSessionCleanup {
    session_name: String,
}

#[cfg(unix)]
impl Drop for TmuxSessionCleanup {
    fn drop(&mut self) {
        // Best-effort kill on both jefe's dedicated socket and the default
        // socket, so a pre-created session is cleaned up regardless of which
        // socket it lives on.
        let jefe_socket = crate::runtime::jefe_tmux_socket_path();
        let _ = std::process::Command::new("tmux")
            .args([
                "-S",
                &jefe_socket.to_string_lossy(),
                "kill-session",
                "-t",
                &self.session_name,
            ])
            .output();
        let _ = std::process::Command::new("tmux")
            .args(["kill-session", "-t", &self.session_name])
            .output();
    }
}

/// Issue #117: Ctrl-r should restart (kill + relaunch) a running agent in one
/// action. This scenario pre-creates a tmux session running `sleep 300`, seeds
/// a state.json with a Running agent bound to that session, then drives the
/// real jefe binary through the restart flow: active-only → Tab to Agents →
/// Ctrl-r → expect agent still visible and running → quit.
#[cfg(unix)]
#[test]
fn guarded_real_jefe_restart_scenario() {
    let Some(jefe_binary) = guarded_jefe_binary("restart test") else {
        return;
    };

    let unique = unique_session("restartagent");
    let agent_session = format!("jefe-restartagent-{unique}");

    // Pre-create a tmux session running sleep on jefe's dedicated socket so
    // jefe's session-exists check (which now targets the private socket) finds
    // it. The session runs `sleep 300` so jefe's restart kill can target it.
    let session_ok = create_sleep_session_on_jefe_socket(&agent_session, 300);

    if !session_ok {
        let _ = std::io::Write::write_all(
            &mut std::io::stderr(),
            b"skipping restart test: could not pre-create agent tmux session
",
        );
        return;
    }

    // Guard ensures the pre-created session is cleaned up even on panic.
    let _cleanup = TmuxSessionCleanup {
        session_name: agent_session.clone(),
    };

    let config_dir = tempfile::tempdir().value_or_panic("isolated config tempdir");
    seed_restart_agent_state(config_dir.path(), &agent_session);

    let summary = run_restart_scenario(&jefe_binary, config_dir.path());
    assert_eq!(summary.steps_run, 8);

    // Verify the restart actually killed the original `sleep 300` process.
    // The seeded session name now matches jefe's session name, so restart
    // targeted THIS session. Two valid success outcomes:
    //  - The session was killed and recreated with the agent command (capture
    //    succeeds; content must NOT contain "sleep 300"), or
    //  - The session was killed and not (yet) recreated in this environment
    //    (capture returns None — tmux kill-session killed the sleep process
    //    along with the pane, so the sleep is dead).
    // The only FAILURE is the session existing AND still running "sleep 300",
    // i.e. restart did not kill the sleep process — which is the regression
    // this test guards against.
    let sleep_survived_restart =
        capture_jefe_pane(&agent_session).is_some_and(|pane| pane.contains("sleep 300"));
    assert!(
        !sleep_survived_restart,
        "restart should have killed the original sleep process"
    );
}

/// Seed a config directory with a state.json containing a single Running agent
/// bound to the given tmux session name (issue #117 scenario fixture).
#[cfg(unix)]
fn seed_restart_agent_state(config_dir: &std::path::Path, agent_session: &str) {
    use crate::domain::{
        Agent, AgentId, AgentStatus, DEFAULT_SANDBOX_FLAGS, LaunchSignature,
        RemoteRepositorySettings, Repository, RepositoryId, RuntimeBinding, SandboxEngine,
    };
    use crate::persistence::{FilePersistenceManager, PersistenceManager, PersistencePaths, State};
    // `RuntimeSession::session_name_for(agent_id)` reproduces `agent_session`
    // exactly. This keeps the pre-created (sleep) session name coherent with
    // the name jefe computes for the agent, so restart targets the SAME session
    // the scenario seeded.
    let agent_id_value = agent_session.strip_prefix("jefe-").unwrap_or(agent_session);
    let mut agent = Agent::new(
        AgentId(agent_id_value.to_owned()),
        RepositoryId("testrepo".into()),
        "RestartAgent".into(),
        std::path::PathBuf::from("/tmp"),
    );
    agent.status = AgentStatus::Running;
    agent.shortcut_slot = Some(1);
    agent.runtime_binding = Some(RuntimeBinding {
        session_name: agent_session.to_string(),
        launch_signature: LaunchSignature {
            work_dir: std::path::PathBuf::from("/tmp"),
            profile: String::new(),
            code_puppy_model: String::new(),
            code_puppy_yolo: Some(false),
            code_puppy_quick_resume: false,
            mode_flags: vec![],
            llxprt_debug: String::new(),
            pass_continue: true,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: DEFAULT_SANDBOX_FLAGS.to_owned(),
            remote: RemoteRepositorySettings::default(),
            agent_kind: crate::domain::AgentKind::Llxprt,
        },
        attached: false,
        last_seen: None,
        process_identity: None,
        pid: None,
    });

    let persisted_state = State {
        schema_version: crate::persistence::STATE_SCHEMA_VERSION,
        repositories: vec![Repository::new(
            RepositoryId("testrepo".into()),
            "TestRepo".into(),
            "testrepo".into(),
            std::path::PathBuf::from("/tmp"),
        )],
        agents: vec![agent],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        hide_idle_repositories: false,
        last_selected_agent_by_repo: vec![],
        pane_focus: String::new(),
        terminal_focused: false,
        user_preferences: crate::domain::UserPreferences::default(),
    };
    let paths = PersistencePaths {
        settings_path: config_dir.join("settings.toml"),
        state_path: config_dir.join("state.json"),
    };
    let persistence = FilePersistenceManager::with_paths(paths);
    persistence
        .save_state(&persisted_state)
        .unwrap_or_else(|e| panic!("save state: {e:?}"));
}

/// Run the issue #117 restart TUI scenario against the real jefe binary.
#[cfg(unix)]
fn run_restart_scenario(jefe_binary: &std::path::Path, config_dir: &std::path::Path) -> RunSummary {
    let scenario = scenario(
        r#"[
            { "waitFor": "LLxprt Jefe" },
            { "key": "Tab" },
            { "wait": 200 },
            { "key": "C-r" },
            { "wait": 3000 },
            { "expect": "RestartAgent" },
            { "key": "C-q" },
            { "waitForExit": 5000 }
        ]"#,
    );
    let session_name = unique_session("restart-jefe");
    let request = TmuxStartRequest::jefe(
        session_name,
        jefe_binary.to_path_buf(),
        config_dir,
        std::env::current_dir().value_or_panic("current dir"),
        TmuxPaneSize::new(100, 30, 2_000),
    )
    .value_or_panic("jefe request");

    run_tmux_scenario(&scenario, &request, None).value_or_panic("run restart scenario")
}
