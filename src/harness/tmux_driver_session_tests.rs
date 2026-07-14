//! Guarded real-tmux integration tests for the driver boundary.
//!
//! These tests shell out to a real tmux server when available and are skipped
//! (via early return) otherwise. Extracted from the original
//! `tmux_driver_tests.rs` to keep file sizes under the project limit.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P03
//! @requirement REQ-TMUX-HARNESS-003

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::*;
use crate::harness::{MatchPattern, screen_contains, scrollback_contains};

use super::validation_tests::{SessionGuard, temp_path};

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

pub(super) fn wait_for_screen_literal(
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

pub(super) fn unique_session(label: &str) -> String {
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("jefe-harness-{label}-{pid}-{nanos}")
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
