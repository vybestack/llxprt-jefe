//! Behavioral tests for the private-socket-aware signal cleanup (issue #375).
//!
//! These tests prove that signal-triggered cleanup:
//! - targets only the harness-owned tmux server/session,
//! - never touches unrelated tmux servers or sessions,
//! - preserves artifact directories,
//! - is idempotent when the server is already dead.
//!
//! Tests that call `kill_harness_server` on the real harness socket would
//! destroy sessions from concurrent tests sharing the same per-process socket,
//! so isolation behavior is proved with raw tmux on dedicated sockets instead.

#![cfg(unix)]

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;
use crate::harness::tmux_driver::TmuxDriver;

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

// ─── Unit tests (no tmux required) ──────────────────────────────────────

/// The guard registers exactly four Unix signals (A1).
#[test]
fn guard_registers_four_unix_signals() {
    let driver = TmuxDriver::new();
    let guard = SignalCleanupGuard::new(driver).value_or_panic("guard construction should succeed");
    assert_eq!(
        guard.handler_count(),
        4,
        "expected SIGINT/SIGTERM/SIGHUP/SIGQUIT"
    );
    drop(guard);
}

/// The guard reports 4 handlers after construction, and a second guard also
/// reports 4 after the first is dropped (A7 partial — re-registration works).
#[test]
fn handler_count_reflects_registration() {
    let guard = SignalCleanupGuard::new(TmuxDriver::new()).value_or_panic("guard should construct");
    assert_eq!(guard.handler_count(), 4);
    drop(guard);

    let guard2 =
        SignalCleanupGuard::new(TmuxDriver::new()).value_or_panic("second guard should construct");
    assert_eq!(guard2.handler_count(), 4);
}

/// `kill_harness_server` is idempotent: calling when no harness server is
/// running returns `Ok` (A6).
#[test]
fn kill_harness_server_is_idempotent_when_no_server() {
    let driver = TmuxDriver::new();
    // If a harness server happens to be running from another test, kill it
    // first so the second call proves idempotency on an already-dead server.
    let _ = driver.kill_harness_server();
    let result = driver.kill_harness_server();
    assert!(
        result.is_ok(),
        "kill_harness_server should be idempotent, got: {result:?}"
    );
}

// ─── Artifact preservation (no tmux session required) ───────────────────

/// `perform_cleanup` never touches the filesystem — it only shells out to
/// `tmux kill-server`. Artifact directories must survive (A3).
///
/// We verify this by creating a marker file and calling `perform_cleanup`
/// (which calls `kill_harness_server` — a no-op when no server exists, but
/// the point is it never touches files regardless).
#[test]
fn perform_cleanup_preserves_artifact_files() {
    let driver = TmuxDriver::new();
    let artifact_dir = tempfile::tempdir().value_or_panic("artifact tempdir");
    let marker = artifact_dir.path().join("marker.txt");
    std::fs::write(&marker, "diagnostic data").value_or_panic("write marker");

    // perform_cleanup calls kill_harness_server — a tmux shell-out that
    // never touches the filesystem.
    perform_cleanup(&driver);

    assert!(
        marker.exists(),
        "artifact file must survive signal cleanup (#375)"
    );
    let content = std::fs::read_to_string(&marker).value_or_panic("read marker");
    assert_eq!(content, "diagnostic data");
}

// ─── Isolation test (raw tmux on dedicated sockets) ─────────────────────

/// `kill-server` on one socket never affects sessions on a different socket
/// (A2). This proves the isolation guarantee that `kill_harness_server` relies
/// on: because the harness uses `-L <unique-socket>`, killing the harness
/// server cannot touch sessions on any other socket.
///
/// We use raw tmux on two dedicated sockets (not the shared harness socket)
/// to avoid disrupting concurrent tests that share the per-process harness
/// socket.
#[test]
fn kill_server_on_one_socket_does_not_affect_another() {
    let driver = TmuxDriver::new();
    if !driver.is_available() {
        let _ = std::io::Write::write_all(
            &mut std::io::stderr(),
            b"skipping isolation test: tmux unavailable\n",
        );
        return;
    }

    let socket_a = format!("jefe-iso-a-{}", unique_suffix());
    let socket_b = format!("jefe-iso-b-{}", unique_suffix());
    let session_a = format!("iso-a-{}", unique_suffix());
    let session_b = format!("iso-b-{}", unique_suffix());

    // Start sessions on both sockets.
    assert!(
        start_session_on_socket(&socket_a, &session_a),
        "should start session on socket A"
    );
    assert!(
        start_session_on_socket(&socket_b, &session_b),
        "should start session on socket B"
    );

    // Verify both are alive.
    assert!(
        session_exists_on_socket(&socket_a, &session_a),
        "session A should exist before kill"
    );
    assert!(
        session_exists_on_socket(&socket_b, &session_b),
        "session B should exist before kill"
    );

    // Kill the server on socket A only.
    kill_server_on_socket(&socket_a);

    // Give tmux a moment to process.
    std::thread::sleep(Duration::from_millis(200));

    // Session A must be dead (server killed).
    assert!(
        !session_exists_on_socket(&socket_a, &session_a),
        "session A must be dead after kill-server on its socket"
    );

    // Session B must still be alive — kill-server on socket A cannot
    // affect socket B. This is the core isolation guarantee (#375).
    assert!(
        session_exists_on_socket(&socket_b, &session_b),
        "session B on a different socket must survive (#375 isolation)"
    );

    // Clean up socket B.
    kill_server_on_socket(&socket_b);
}

// ─── Helpers ────────────────────────────────────────────────────────────

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

fn start_session_on_socket(socket: &str, session_name: &str) -> bool {
    let result = std::process::Command::new("tmux")
        .args([
            "-L",
            socket,
            "-f",
            "/dev/null",
            "new-session",
            "-d",
            "-s",
            session_name,
            "--",
            "sleep",
            "60",
        ])
        .output();
    matches!(result, Ok(out) if out.status.success())
}

fn session_exists_on_socket(socket: &str, session_name: &str) -> bool {
    let result = std::process::Command::new("tmux")
        .args([
            "-L",
            socket,
            "-f",
            "/dev/null",
            "has-session",
            "-t",
            session_name,
        ])
        .output();
    matches!(result, Ok(out) if out.status.success())
}

fn kill_server_on_socket(socket: &str) {
    let _ = std::process::Command::new("tmux")
        .args(["-L", socket, "-f", "/dev/null", "kill-server"])
        .output();
}
