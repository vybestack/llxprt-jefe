//! Guarded integration tests for the tmux driver boundary.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P03
//! @requirement REQ-TMUX-HARNESS-003

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::*;
use crate::harness::{MatchPattern, screen_contains, scrollback_contains};

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

/// Test helper that cleans up a harness session unless the test already did.
struct SessionGuard<'a> {
    driver: &'a TmuxDriver,
    session: TmuxSession,
}

impl Drop for SessionGuard<'_> {
    fn drop(&mut self) {
        let _ = self.driver.cleanup_session(&self.session);
    }
}

/// `TmuxStartRequest` rejects unusable command/geometry values before shelling out.
/// Absent-session classification is narrow and does not swallow arbitrary failures.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P03
/// @requirement REQ-TMUX-HARNESS-003
#[test]
fn absent_session_error_classifier_is_narrow() {
    let absent = TmuxDriverError::Failed {
        command: "tmux -f /dev/null has-session -t missing".to_string(),
        stderr: "can't find session: missing".to_string(),
    };
    let server_absent = TmuxDriverError::Failed {
        command: "tmux -f /dev/null has-session -t missing".to_string(),
        stderr: "no server running on /tmp/tmux-501/default".to_string(),
    };
    let linux_server_absent = TmuxDriverError::Failed {
        command: "tmux -f /dev/null has-session -t missing".to_string(),
        stderr: "error connecting to /tmp/tmux-1001/default (No such file or directory)"
            .to_string(),
    };
    let real_failure = TmuxDriverError::Failed {
        command: "tmux -f /dev/null has-session -t missing".to_string(),
        stderr: "permission denied".to_string(),
    };

    assert!(is_absent_session_error(&absent));
    assert!(is_absent_session_error(&server_absent));
    assert!(is_absent_session_error(&linux_server_absent));
    assert!(!is_absent_session_error(&real_failure));
}

///
/// @plan PLAN-20260629-TMUX-HARNESS.P03
/// @requirement REQ-TMUX-HARNESS-003
#[test]
fn start_request_rejects_empty_command() {
    let result = TmuxStartRequest::command("demo", Vec::new(), temp_path(), 80, 24, 1000);
    assert!(matches!(result, Err(TmuxDriverError::InvalidRequest(_))));
}

/// `TmuxStartRequest` rejects blank session names.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P03
/// @requirement REQ-TMUX-HARNESS-003
#[test]
fn start_request_rejects_empty_session_name() {
    let result =
        TmuxStartRequest::command("  ", vec!["echo".to_string()], temp_path(), 80, 24, 1000);
    assert!(matches!(result, Err(TmuxDriverError::InvalidRequest(_))));
}

/// `TmuxStartRequest` rejects empty argv entries.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P03
/// @requirement REQ-TMUX-HARNESS-003
#[test]
fn start_request_rejects_empty_argv_element() {
    let result = TmuxStartRequest::command(
        "demo",
        vec!["echo".to_string(), String::new()],
        temp_path(),
        80,
        24,
        1000,
    );
    assert!(matches!(result, Err(TmuxDriverError::InvalidRequest(_))));
}

/// The new-session argv passes a single shell command with shell-escaped parts.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P03
/// @requirement REQ-TMUX-HARNESS-003
#[test]
fn new_session_command_shell_escapes_argv_parts() {
    let request = TmuxStartRequest::command(
        "demo",
        vec![
            "/bin/echo".to_string(),
            "a b".to_string(),
            "quote'it".to_string(),
        ],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid");

    let args = new_session_args(&request);
    let socket = harness_socket_name();
    let expected = format!(
        "tmux -f /dev/null -L {socket} set-option -pt \"$TMUX_PANE\" remain-on-exit on; tmux -f /dev/null -L {socket} set-option -wt \"$TMUX_PANE\" history-limit 1000; exec '/bin/echo' 'a b' 'quote'\\''it'"
    );
    assert_eq!(args.last().map(String::as_str), Some(expected.as_str()));
}

/// Real jefe requests always include an isolated `--config` directory.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P03
/// @requirement REQ-TMUX-HARNESS-003
#[test]
fn jefe_request_includes_isolated_config_arg() {
    let request = TmuxStartRequest::jefe(
        "jefe-demo",
        "/tmp/jefe-bin",
        "/tmp/jefe-config",
        temp_path(),
        TmuxPaneSize::new(100, 30, 2000),
    )
    .value_or_panic("jefe request should be valid");

    assert_eq!(
        request.command,
        vec![
            "/tmp/jefe-bin".to_string(),
            "--config".to_string(),
            "/tmp/jefe-config".to_string(),
        ]
    );
}

/// A guarded real jefe session starts with an isolated config directory.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P03
/// @requirement REQ-TMUX-HARNESS-003
#[test]
fn real_jefe_session_uses_isolated_config_when_binary_available() {
    let driver = TmuxDriver::new();
    if !driver.is_available() {
        return;
    }
    let Some(binary) = jefe_binary_path() else {
        return;
    };
    let config_dir = tempfile::tempdir().value_or_panic("config tempdir");
    let request = TmuxStartRequest::jefe(
        unique_session("jefe"),
        binary,
        config_dir.path(),
        temp_path(),
        TmuxPaneSize::new(100, 30, 2000),
    )
    .value_or_panic("jefe request should be valid");
    let session = driver
        .start_session(&request)
        .value_or_panic("jefe tmux session should start");
    let guard = SessionGuard {
        driver: &driver,
        session,
    };

    let capture = wait_for_screen_literal(&driver, &guard.session, "LLxprt Jefe")
        .value_or_panic("jefe screen should render stable title");
    let outcome = screen_contains(&capture, MatchPattern::literal("LLxprt Jefe"));

    assert!(outcome.matched, "jefe capture was {capture:?}");
}

/// A guarded real tmux session can start, render output, and be captured.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P03
/// @requirement REQ-TMUX-HARNESS-003
#[test]
fn tmux_session_captures_visible_screen_when_available() {
    let driver = TmuxDriver::new();
    if !driver.is_available() {
        return;
    }
    let request = shell_request("capture", "printf 'harness-ready\\n'; sleep 2");
    let session = driver
        .start_session(&request)
        .value_or_panic("tmux session should start");
    let guard = SessionGuard {
        driver: &driver,
        session,
    };

    let capture = wait_for_screen_literal(&driver, &guard.session, "harness-ready")
        .value_or_panic("screen should contain readiness marker");
    let outcome = screen_contains(&capture, MatchPattern::literal("harness-ready"));

    assert!(outcome.matched, "capture was {capture:?}");
}

/// Pane liveness can be read after the process exits because remain-on-exit is set.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P03
/// @requirement REQ-TMUX-HARNESS-003
#[test]
fn pane_status_reads_dead_after_process_exit_when_tmux_available() {
    let driver = TmuxDriver::new();
    if !driver.is_available() {
        return;
    }
    let request = shell_request("dead", "exit 0");
    let session = driver
        .start_session(&request)
        .value_or_panic("tmux session should start");
    let guard = SessionGuard {
        driver: &driver,
        session,
    };

    let status = wait_for_dead_pane(&driver, &guard.session)
        .value_or_panic("pane status should become dead");

    assert!(status.dead);
}

/// History size is readable and monotonic as output accrues.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P03
/// @requirement REQ-TMUX-HARNESS-003
#[test]
fn history_size_is_readable_and_monotonic_when_tmux_available() {
    let driver = TmuxDriver::new();
    if !driver.is_available() {
        return;
    }
    let request = shell_request(
        "history",
        "printf 'reader-ready\\n'; while read line; do echo \"$line\"; done",
    );
    let session = driver
        .start_session(&request)
        .value_or_panic("tmux session should start");
    let guard = SessionGuard {
        driver: &driver,
        session,
    };

    let _ready = wait_for_screen_literal(&driver, &guard.session, "reader-ready")
        .value_or_panic("reader loop should announce readiness");
    let before = driver
        .history_size(&guard.session)
        .value_or_panic("history size before");
    driver
        .send_line(&guard.session, "unique-payload-marker")
        .value_or_panic("send line should work");
    let (after, sample) =
        wait_for_literal_scrollback(&driver, &guard.session, "unique-payload-marker")
            .value_or_panic("literal line should appear in scrollback");
    let literal = scrollback_contains(&sample, MatchPattern::literal("unique-payload-marker"));

    assert!(after >= before, "before={before} after={after}");
    assert!(sample.history_size >= after);
    assert!(
        literal.matched,
        "send_line must send literal text followed by newline"
    );
}

fn shell_request(label: &str, script: &str) -> TmuxStartRequest {
    TmuxStartRequest::command(
        unique_session(label),
        vec!["/bin/sh".to_string(), "-c".to_string(), script.to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("shell request should be valid")
}

fn wait_for_literal_scrollback(
    driver: &TmuxDriver,
    session: &TmuxSession,
    literal: &str,
) -> Result<(u64, crate::harness::ScrollbackSample), TmuxDriverError> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let after = driver.history_size(session)?;
        let sample = driver.capture_scrollback(session, 20)?;
        let outcome = scrollback_contains(&sample, MatchPattern::literal(literal));
        if outcome.matched || Instant::now() >= deadline {
            return Ok((after, sample));
        }
        sleep_briefly();
    }
}

fn wait_for_screen_literal(
    driver: &TmuxDriver,
    session: &TmuxSession,
    literal: &str,
) -> Result<crate::harness::ScreenCapture, TmuxDriverError> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let capture = driver.capture_screen(session)?;
        let outcome = screen_contains(&capture, MatchPattern::literal(literal));
        if outcome.matched || Instant::now() >= deadline {
            return Ok(capture);
        }
        sleep_briefly();
    }
}

fn wait_for_dead_pane(
    driver: &TmuxDriver,
    session: &TmuxSession,
) -> Result<crate::harness::PaneStatus, TmuxDriverError> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let status = driver.pane_status(session)?;
        if status.dead || Instant::now() >= deadline {
            return Ok(status);
        }
        sleep_briefly();
    }
}

fn unique_session(label: &str) -> String {
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("jefe-harness-{label}-{pid}-{nanos}")
}

fn temp_path() -> PathBuf {
    std::env::temp_dir()
}

fn sleep_briefly() {
    std::thread::sleep(Duration::from_millis(150));
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

// --- #171 regression tests: harness tmux socket isolation + env scrub ---------

/// The harness socket name is stable within a process and carries a per-process
/// suffix so parallel test runs never share a harness server (#171).
#[test]
fn harness_socket_name_is_per_process_and_stable() {
    let name = harness_socket_name();
    assert!(
        name.starts_with("jefe-harness-"),
        "socket name should be prefixed jefe-harness-, got {name}"
    );
    let suffix = name.strip_prefix("jefe-harness-").unwrap_or(name);
    assert!(
        !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()),
        "socket suffix should be the numeric PID, got {suffix}"
    );
    // Stable across calls (cached via OnceLock).
    assert_eq!(harness_socket_name().as_ptr(), name.as_ptr());
}

/// Every formatted harness tmux command must carry the dedicated `-L <socket>`
/// flag so harness calls can never land on an inherited/outer server (#171).
#[test]
fn format_command_carries_dedicated_socket_flag() {
    let socket = harness_socket_name();
    let empty = format_command(&[]);
    assert_eq!(empty, format!("tmux -f /dev/null -L {socket}"));

    let with_args = format_command(&["has-session".to_owned(), "-t".to_owned(), "x".to_owned()]);
    assert_eq!(
        with_args,
        format!("tmux -f /dev/null -L {socket} has-session -t x")
    );
}

/// A harness session must run on the harness's dedicated `-L` socket, not on
/// any inherited/outer/default server. This is the behavioral guarantee of
/// #171: every harness tmux call is pinned to `tmux -L jefe-harness-<pid>`, so
/// the harness never lands on (and its kill/respawn churn never disrupts) an
/// outer server — even when the test process itself is running inside a jefe
/// pane that sets `$TMUX`.
///
/// We prove isolation without mutating process env (`set_var` is `unsafe` under
/// edition 2024 and forbidden here): after starting a session through the
/// driver, that session must NOT be listed on the shared default tmux server
/// (queried with no `-L`), proving the harness did not leak onto it.
#[test]
fn harness_session_runs_on_dedicated_socket() {
    let driver = TmuxDriver::new();
    if !driver.is_available() {
        return;
    }

    let request = shell_request("socket", "printf 'socket-ready\\n'; sleep 2");
    let session = driver
        .start_session(&request)
        .value_or_panic("harness session should start on its dedicated socket");
    let guard = SessionGuard {
        driver: &driver,
        session,
    };

    // The session must render on the harness's own server.
    let capture = wait_for_screen_literal(&driver, &guard.session, "socket-ready")
        .value_or_panic("harness session should render on the isolated server");
    assert!(
        screen_contains(&capture, MatchPattern::literal("socket-ready")).matched,
        "capture was {capture:?}"
    );

    // CRITICAL isolation check (#171): query the *default* shared tmux server
    // (no `-L`) and confirm the harness session is NOT listed there. If it
    // were, the harness would have leaked onto the outer server and its
    // kill-session lifecycle could disrupt it. A non-existent default server
    // (no sessions at all) is the strongest possible pass.
    let default_listing = std::process::Command::new("tmux")
        .args(["-f", "/dev/null", "list-sessions"])
        .output();
    if let Ok(out) = default_listing {
        let listing = String::from_utf8_lossy(&out.stdout).to_string();
        assert!(
            !listing
                .lines()
                .any(|line| line.starts_with(&format!("{}:", guard.session.name))),
            "harness session leaked onto the default/shared tmux server (#171); listing:\n{listing}"
        );
    }
}
