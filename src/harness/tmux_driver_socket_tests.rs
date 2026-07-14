//! #171 regression tests: harness tmux socket isolation and env scrub.
//!
//! Extracted from the original `tmux_driver_tests.rs` to keep file sizes under
//! the project limit.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P03
//! @requirement REQ-TMUX-HARNESS-003

use super::*;

use super::session_tests::{unique_session, wait_for_screen_literal};
use super::validation_tests::{SessionGuard, temp_path};
use crate::harness::{MatchPattern, screen_contains};

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
        !suffix.is_empty(),
        "socket suffix should be non-empty, got {suffix}"
    );
    // Stable across calls (cached via OnceLock).
    assert_eq!(harness_socket_name().as_ptr(), name.as_ptr());
}

/// Every formatted harness tmux command must carry the dedicated `-L <socket>`
/// flag so harness calls can never land on an inherited/outer server (#171).
/// The shared prefix comes from [`harness_tmux_prefix_str`] so the `Command`
/// builder and shell-string builders cannot drift (#173).
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

/// The shared `Command`-builder prefix and shell-string prefix must agree on
/// the harness socket name so inline `tmux -L {socket}` calls and the spawned
/// `Command` resolve the same server (#173 DRY guard).
#[test]
fn harness_tmux_prefix_args_and_str_resolve_same_socket() {
    let args = harness_tmux_prefix_args();
    assert_eq!(args, ["-f", "/dev/null", "-L", harness_socket_name()]);
    let s = harness_tmux_prefix_str();
    let expected = format!("tmux -f /dev/null -L {}", harness_socket_name());
    assert_eq!(s, expected);
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
    //
    // The probe MUST scrub `TMUX`/`TMUX_PANE`/`TMUX_TMPDIR` from its own env:
    // when this test runs inside a jefe pane (the exact #171 scenario), an
    // inherited `$TMUX` would redirect the bare `tmux list-sessions` at the
    // outer/jefe server instead of the default server, invalidating the proof.
    let default_listing = std::process::Command::new("tmux")
        .args(["-f", "/dev/null", "list-sessions"])
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .env_remove("TMUX_TMPDIR")
        .output();
    let out = match default_listing {
        Ok(out) => out,
        Err(err) => {
            // tmux was confirmed available above (`driver.is_available()`), so a
            // spawn failure here is a genuine environment problem — NOT a pass.
            // Surfacing it prevents the #171 isolation assertion from being
            // skipped vacuously (#173).
            panic!(
                "tmux list-sessions probe failed to spawn even though tmux is available (#171 isolation assertion skipped): {err}"
            );
        }
    };
    let listing = String::from_utf8_lossy(&out.stdout).to_string();
    // Two pass cases:
    //  (a) no default server running → empty stdout (strongest pass).
    //  (b) a default server is running but our harness session is not listed.
    // A non-zero exit with empty stdout is the legitimate "no default server"
    // case; only a non-empty listing containing our session name is a fail.
    assert!(
        !listing
            .lines()
            .any(|line| line.starts_with(&format!("{}:", guard.session.name))),
        "harness session leaked onto the default/shared tmux server (#171); listing:\n{listing}"
    );
}
