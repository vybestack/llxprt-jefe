//! Behavioral tests for the private-socket-aware signal cleanup (issue #375).
//!
//! These tests prove that signal-triggered cleanup:
//! - targets only the harness-owned tmux server/session,
//! - never touches unrelated tmux servers or sessions,
//! - preserves artifact directories,
//! - is idempotent when the server is already dead.
//!
//! No test in this file calls `TmuxDriver::kill_harness_server()` directly,
//! because that would kill every session on the per-process harness socket
//! (shared by all concurrent tests in this binary). Instead, isolation and
//! idempotency are proved with raw tmux on dedicated sockets.

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

// ─── Artifact preservation (no tmux kill required) ──────────────────────

/// `kill-server` (the only thing `perform_cleanup` does) never touches the
/// filesystem — it only kills tmux processes. Artifact directories must
/// survive (A3).
///
/// We prove this on a **dedicated socket** to avoid killing the shared
/// per-process harness socket that concurrent runner tests depend on.
/// `perform_cleanup` is a one-line wrapper around `kill_harness_server`,
/// which is itself a one-line `tmux kill-server` on the harness socket.
/// The filesystem-preserving property of `kill-server` is identical
/// regardless of which socket it targets.
#[test]
fn kill_server_preserves_artifact_files_on_dedicated_socket() {
    let driver = TmuxDriver::new();
    if !driver.is_available() {
        let _ = std::io::Write::write_all(
            &mut std::io::stderr(),
            b"skipping artifact preservation test: tmux unavailable
",
        );
        return;
    }

    let socket = format!("jefe-artifact-{}", unique_suffix());
    let session = format!("artifact-{}", unique_suffix());
    assert!(
        start_session_on_socket(&socket, &session),
        "should start session"
    );

    let artifact_dir = tempfile::tempdir().value_or_panic("artifact tempdir");
    let marker = artifact_dir.path().join("marker.txt");
    std::fs::write(&marker, "diagnostic data").value_or_panic("write marker");

    // kill-server on the dedicated socket — this is the exact operation
    // perform_cleanup performs (just on the harness socket instead).
    kill_server_on_socket(&socket);
    std::thread::sleep(Duration::from_millis(200));

    assert!(
        marker.exists(),
        "artifact file must survive tmux kill-server (#375)"
    );
    let content = std::fs::read_to_string(&marker).value_or_panic("read marker");
    assert_eq!(content, "diagnostic data");
    assert!(
        !session_exists_on_socket(&socket, &session),
        "session must be dead after kill-server"
    );
}

// ─── Isolation and idempotency tests (raw tmux on dedicated sockets) ─────

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

/// `kill-server` is idempotent: calling it when no server exists does not
/// produce a hard error (A6). On a non-existent socket, `kill-server` exits
/// non-zero with a "no server running" message — which
/// `is_no_server_error` classifies as success (idempotent). On a live
/// server, it kills everything and exits 0. A second call on the now-dead
/// socket must also be classified as success.
#[test]
fn kill_server_is_idempotent_on_dedicated_socket() {
    let driver = TmuxDriver::new();
    if !driver.is_available() {
        let _ = std::io::Write::write_all(
            &mut std::io::stderr(),
            b"skipping idempotency test: tmux unavailable\n",
        );
        return;
    }

    let socket = format!("jefe-idem-{}", unique_suffix());

    // Kill on a socket that has no server — must be classified as success
    // (either exit 0 or "no server running" stderr).
    assert!(
        kill_server_on_socket_is_ok(&socket),
        "kill-server on non-existent server must be idempotent"
    );

    // Start a session, kill it, then kill again — second call must also be ok.
    let session = format!("idem-{}", unique_suffix());
    assert!(
        start_session_on_socket(&socket, &session),
        "should start session"
    );
    assert!(kill_server_on_socket_is_ok(&socket), "first kill must work");
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        kill_server_on_socket_is_ok(&socket),
        "second kill on dead server must be idempotent"
    );
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
    let _ = kill_server_on_socket_is_ok(socket);
}

/// Returns true if kill-server either succeeds (exit 0) or fails with a
/// "no server running" message (the idempotent case). This mirrors the
/// `is_no_server_error` classification in `tmux_driver.rs`.
fn kill_server_on_socket_is_ok(socket: &str) -> bool {
    let result = std::process::Command::new("tmux")
        .args(["-L", socket, "-f", "/dev/null", "kill-server"])
        .output();
    match result {
        Ok(out) if out.status.success() => true,
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            stderr.contains("no server running")
                || (stderr.contains("error connecting")
                    && stderr.contains("No such file or directory"))
        }
        Err(_) => false,
    }
}
