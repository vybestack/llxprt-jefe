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

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

// ─── Artifact preservation (dedicated socket) ───────────────────────────

/// `kill-server` (the only thing `perform_cleanup` does) never touches the
/// filesystem — it only kills tmux processes. Artifact directories must
/// survive (A3).
///
/// We prove this on a **dedicated socket** to avoid killing the shared
/// per-process harness socket that concurrent runner tests depend on.
#[test]
fn kill_server_preserves_artifact_files_on_dedicated_socket() {
    let driver = TmuxDriver::new();
    if !driver.is_available() {
        let _ = std::io::Write::write_all(
            &mut std::io::stderr(),
            b"skipping artifact preservation test: tmux unavailable\n",
        );
        return;
    }

    let socket = format!("jefe-artifact-{}", unique_suffix());
    let session = format!("artifact-{}", unique_suffix());
    let _guard = SocketGuard::new(&socket);
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
    poll_until_dead(&socket, &session);

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

    // RAII guards ensure both sockets are killed even on panic.
    let _guard_a = SocketGuard::new(&socket_a);
    let _guard_b = SocketGuard::new(&socket_b);

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
    poll_until_dead(&socket_a, &session_a);

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
    let _guard = SocketGuard::new(&socket);

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
    poll_until_dead(&socket, &session);
    assert!(
        kill_server_on_socket_is_ok(&socket),
        "second kill on dead server must be idempotent"
    );
}

// ─── Signal delivery integration test ───────────────────────────────────

/// Delivering SIGTERM to a process that holds a `SignalCleanupGuard` causes
/// the guard's background thread to run `perform_cleanup` and exit with
/// `128 + SIGTERM` (A4). This verifies the full signal → cleanup → exit
/// pipeline rather than just the raw tmux kill-server behavior.
///
/// The test spawns a child `cargo test` process that:
///   1. Starts a tmux session on the harness socket.
///   2. Constructs a `SignalCleanupGuard`.
///   3. Sends SIGTERM to itself via `signal_hook::low_level::raise`.
///
/// The parent asserts:
///   - The child exits with code 128 + SIGTERM (143).
///   - The child's harness tmux server is killed after exit.
#[test]
fn signal_delivery_triggers_cleanup_and_exit() {
    let driver = TmuxDriver::new();
    if !driver.is_available() {
        let _ = std::io::Write::write_all(
            &mut std::io::stderr(),
            b"skipping signal delivery test: tmux unavailable
",
        );
        return;
    }

    // Spawn a `cargo test` child that runs the signal-self-test helper.
    // The child constructs a SignalCleanupGuard, starts a tmux session on
    // the harness socket, then raises SIGTERM to itself. The guard's
    // background thread should call perform_cleanup + process::exit(143).
    //
    // The helper is #[ignore]'d so it only runs when explicitly requested
    // via --ignored, preventing it from running in normal test suites and
    // calling process::exit(143) on the whole binary.
    let child = std::process::Command::new("cargo")
        .args([
            "test",
            "--features",
            "psmux-smoke",
            "--",
            "--ignored",
            "--exact",
            "harness::signal_cleanup::tests::signal_self_test_helper",
            "--nocapture",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .value_or_panic("should spawn cargo test child");

    let child_pid = child.id();

    let output = child
        .wait_with_output()
        .value_or_panic("should wait for child");

    // The child should exit with 128 + SIGTERM = 143.
    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 128 + signal_hook::consts::SIGTERM,
        "child should exit with 128+SIGTERM (143), got {code}
stdout: {}
stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // Verify the child's harness socket was cleaned up. The harness socket
    // name is `jefe-harness-<child_pid>`. After the signal cleanup, no
    // server should be running on that socket.
    let child_socket = format!("jefe-harness-{child_pid}");
    poll_until_server_dead(&child_socket);
    assert!(
        !server_exists_on_socket(&child_socket),
        "child's harness tmux server must be killed by signal cleanup"
    );
}

/// Helper test that runs in a child `cargo test` process. Registers a
/// `SignalCleanupGuard`, starts a tmux session on the harness socket, then
/// sends SIGTERM to itself via `signal_hook::low_level::raise`. The guard's
/// background thread should call `perform_cleanup` (killing the harness
/// tmux server) and then `std::process::exit(143)`.
///
/// Marked `#[ignore]` because it calls `process::exit(143)`, which would
/// terminate the entire test binary if run alongside other tests. The
/// parent test spawns a child `cargo test -- --ignored` process that runs
/// only this test.
#[test]
#[ignore = "calls process::exit(143); only run via child cargo test from signal_delivery_triggers_cleanup_and_exit"]
fn signal_self_test_helper() {
    let driver = TmuxDriver::new();
    if !driver.is_available() {
        // No-op if tmux is unavailable. The parent will fail the exit-code
        // check, but that's expected — the parent test should be guarded
        // by its own tmux availability check.
        let _ = signal_hook::low_level::raise(signal_hook::consts::SIGTERM);
        return;
    }

    let _guard = SignalCleanupGuard::new(driver.clone()).value_or_panic("guard should construct");

    // Start a tmux session on the harness socket so we can verify it gets
    // killed by the signal cleanup.
    let session_name = format!("signal-test-{}", std::process::id());
    let request = crate::harness::TmuxStartRequest::command(
        session_name,
        vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "sleep 300".to_string(),
        ],
        std::env::temp_dir(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid");
    let _session = driver
        .start_session(&request)
        .value_or_panic("session should start");

    // Send SIGTERM to ourselves via signal-hook (async-signal-safe, no
    // unsafe block needed). The SignalCleanupGuard's background thread
    // should catch it, call perform_cleanup (kill_harness_server), and exit
    // with 128 + 15 = 143.
    let _ = signal_hook::low_level::raise(signal_hook::consts::SIGTERM);

    // If the guard didn't work, the test would continue past here. We add
    // a short wait as a safety net so the test doesn't hang indefinitely.
    std::thread::sleep(Duration::from_secs(5));
    // If we reach here, the signal cleanup failed. Force-exit so the
    // parent test gets a non-143 exit code and fails.
    std::process::exit(1);
}

// ─── Helpers ────────────────────────────────────────────────────────────

/// Process-wide counter for unique socket/session names. Combined with
/// PID and nanosecond timestamp, guarantees uniqueness even under parallel
/// test execution within the same nanosecond.
static SUFFIX_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique suffix combining PID, nanosecond timestamp, and a
/// monotonic counter to guarantee uniqueness across parallel tests.
fn unique_suffix() -> String {
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let counter = SUFFIX_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{pid}-{nanos}-{counter}")
}

/// RAII guard that kills a dedicated-socket tmux server on drop, ensuring
/// cleanup even if the test panics mid-scenario.
struct SocketGuard {
    socket: String,
}

impl SocketGuard {
    fn new(socket: &str) -> Self {
        Self {
            socket: socket.to_string(),
        }
    }
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        kill_server_on_socket(&self.socket);
    }
}

/// Poll until a session is dead, with a 3-second timeout and 50ms interval.
/// Replaces fragile fixed sleeps with deterministic polling.
fn poll_until_dead(socket: &str, session_name: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if !session_exists_on_socket(socket, session_name) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
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
        .env("LC_ALL", "C")
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
        .env("LC_ALL", "C")
        .output();
    matches!(result, Ok(out) if out.status.success())
}

fn kill_server_on_socket(socket: &str) {
    let _ = kill_server_on_socket_is_ok(socket);
}

/// Returns true if kill-server either succeeds (exit 0) or fails with a
/// "no server running" message (the idempotent case). This mirrors the
/// `is_no_server_error` classification in `tmux_driver.rs`.
///
/// `LC_ALL=C` is set to ensure tmux emits English-language error messages
/// regardless of the system locale, so the stderr classification matches.
fn kill_server_on_socket_is_ok(socket: &str) -> bool {
    let result = std::process::Command::new("tmux")
        .args(["-L", socket, "-f", "/dev/null", "kill-server"])
        .env("LC_ALL", "C")
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

/// Check whether a tmux server is running on the given socket by listing
/// sessions. Returns true if a server responds (even with zero sessions),
/// false if no server exists.
fn server_exists_on_socket(socket: &str) -> bool {
    let result = std::process::Command::new("tmux")
        .args(["-L", socket, "-f", "/dev/null", "list-sessions"])
        .env("LC_ALL", "C")
        .output();
    match result {
        Ok(out) => {
            // A running server with sessions exits 0.
            // A non-existent server exits non-zero with "no server running"
            // or "error connecting" in stderr.
            // Any other non-zero exit is ambiguous — treat as server exists
            // (safer for the assertion: we don't want false "killed" claims).
            if out.status.success() {
                return true;
            }
            let stderr = String::from_utf8_lossy(&out.stderr);
            !(stderr.contains("no server running") || stderr.contains("error connecting"))
        }
        Err(_) => false,
    }
}

/// Poll until no tmux server responds on the given socket (3s deadline,
/// 50ms interval).
fn poll_until_server_dead(socket: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if !server_exists_on_socket(socket) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
