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

/// `TmuxStartRequest` rejects NUL bytes in command argv (path injection).
#[test]
fn start_request_rejects_nul_in_command() {
    let result = TmuxStartRequest::command(
        "demo",
        vec!["echo\0rm".to_string()],
        temp_path(),
        80,
        24,
        1000,
    );
    assert!(matches!(result, Err(TmuxDriverError::InvalidRequest(_))));
}

/// `TmuxStartRequest` rejects NUL bytes in session name (path injection).
#[test]
fn start_request_rejects_nul_in_session_name() {
    let result = TmuxStartRequest::command(
        "ses\0sion",
        vec!["echo".to_string()],
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
        "unset TMUX TMUX_PANE TMUX_TMPDIR; tmux -f /dev/null -L {socket} set-option -t 'demo' status off; tmux -f /dev/null -L {socket} set-option -pt \"$TMUX_PANE\" remain-on-exit on; tmux -f /dev/null -L {socket} set-option -wt \"$TMUX_PANE\" history-limit 1000; exec '/bin/echo' 'a b' 'quote'\\''it'"
    );
    assert_eq!(args.last().map(String::as_str), Some(expected.as_str()));
}

/// Inner pane commands must scrub the tmux client env (`TMUX`, `TMUX_PANE`,
/// `TMUX_TMPDIR`) so the inner `tmux -L {socket}` calls resolve the harness
/// socket in the SAME directory as the outer `tmux_command()`. An inherited
/// `$TMUX_TMPDIR` (the #171 scenario) would otherwise redirect the inner calls
/// at the outer server's socket directory, leaving
/// `remain-on-exit`/`history-limit` unconfigured on the harness session (#173).
///
/// This is a pure-string assertion over the builder output (no tmux spawn), so
/// the regression locks the wrapper string shape: the `unset` prefix MUST
/// precede the first `tmux -f /dev/null -L {socket}` segment.
#[test]
fn tmux_pane_wrapper_command_scrubs_tmux_env_before_inner_calls() {
    let request = TmuxStartRequest::command(
        "env-scrub",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid");

    let wrapper = tmux_pane_wrapper_command(&request);
    let unset_pos = wrapper
        .find("unset TMUX TMUX_PANE TMUX_TMPDIR;")
        .unwrap_or_else(|| panic!("unset prefix missing from wrapper: {wrapper}"));
    let tmux_pos = wrapper
        .find("tmux -f /dev/null -L")
        .unwrap_or_else(|| panic!("inner tmux prefix missing from wrapper: {wrapper}"));

    assert!(
        unset_pos < tmux_pos,
        "unset scrub must precede the inner tmux calls; got wrapper={wrapper}"
    );
    // The exec'd command must survive the scrub verbatim.
    assert!(
        wrapper.ends_with("exec '/bin/true'"),
        "exec'd command must survive verbatim after the unset/inner calls; got {wrapper}"
    );
}

/// `with_env_path` sets the `env_path` field so the pane wrapper exports a
/// controlled PATH before exec-ing the command (issue #241).
#[test]
fn with_env_path_sets_field_on_request() {
    let request = TmuxStartRequest::command(
        "env-path-test",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid");

    let with_path = request.clone().with_env_path("/tmp/shims:/usr/bin");
    assert_eq!(with_path.env_path.as_deref(), Some("/tmp/shims:/usr/bin"));
    // Original is unchanged (builder consumes self, so clone first).
    assert!(request.env_path.is_none());
}

/// When `env_path` is set, the pane wrapper includes `export PATH=...` before
/// the first inner tmux call (issue #241). The PATH value is shell-escaped
/// using single-quote escaping (same as command argv), preventing injection.
#[test]
fn pane_wrapper_includes_path_export_when_env_path_set() {
    let request = TmuxStartRequest::command(
        "env-export-test",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid")
    .with_env_path("/tmp/shims:/usr/bin");

    let wrapper = tmux_pane_wrapper_command(&request);
    let export_pos = wrapper
        .find("export PATH='/tmp/shims:/usr/bin';")
        .unwrap_or_else(|| panic!("PATH export missing from wrapper: {wrapper}"));
    let unset_pos = wrapper
        .find("unset TMUX TMUX_PANE TMUX_TMPDIR;")
        .unwrap_or_else(|| panic!("unset prefix missing from wrapper: {wrapper}"));
    let tmux_pos = wrapper
        .find("tmux -f /dev/null -L")
        .unwrap_or_else(|| panic!("inner tmux prefix missing from wrapper: {wrapper}"));

    assert!(
        unset_pos < export_pos,
        "unset scrub must precede the PATH export; got wrapper={wrapper}"
    );
    assert!(
        export_pos < tmux_pos,
        "PATH export must precede the inner tmux calls; got wrapper={wrapper}"
    );
}

/// When `env_path` contains single quotes, they must be shell-escaped to
/// prevent PATH injection (issue #241 review fix #2).
#[test]
fn pane_wrapper_shell_escapes_single_quotes_in_env_path() {
    let request = TmuxStartRequest::command(
        "env-escape-test",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid")
    .with_env_path("/tmp/it's a path");

    let wrapper = tmux_pane_wrapper_command(&request);
    // The single quote in the path must be escaped as '\'' by shell_escape_single.
    assert!(
        wrapper.contains(r"'/tmp/it'\''s a path'"),
        "single quote in PATH must be shell-escaped; got wrapper={wrapper}"
    );
}

/// `env_path` containing NUL bytes must be rejected by validation.
#[test]
fn env_path_with_nul_byte_is_rejected() {
    let result = TmuxStartRequest::command(
        "nul-path-test",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid")
    .with_env_path("safe\0evil");

    assert!(
        result.validate().is_err(),
        "env_path with NUL byte must be rejected"
    );
}

/// `env_path` with backticks or $() must not be executed — single-quote
/// escaping neutralizes them.
#[test]
fn env_path_with_backticks_is_neutralized() {
    let request = TmuxStartRequest::command(
        "backtick-path-test",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid")
    .with_env_path("/tmp/$(whoami)/bin");

    let wrapper = tmux_pane_wrapper_command(&request);
    // The value is inside single quotes, so $() is not expanded.
    assert!(
        wrapper.contains("'/tmp/$(whoami)/bin'"),
        "backtick/$(...) in PATH must be inside single quotes (not expanded); got wrapper={wrapper}"
    );
}

/// When `env_path` is `None`, the pane wrapper does not include a PATH export.
#[test]
fn pane_wrapper_omits_path_export_when_env_path_absent() {
    let request = TmuxStartRequest::command(
        "no-env-path-test",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid");

    let wrapper = tmux_pane_wrapper_command(&request);
    assert!(
        !wrapper.contains("export PATH="),
        "wrapper should not include PATH export when env_path is None; got {wrapper}"
    );
}

/// The pane wrapper disables the tmux status bar so captures never show the
/// hostname, session name, or live clock.
#[test]
fn pane_wrapper_disables_tmux_status_bar() {
    let request = TmuxStartRequest::command(
        "status-off-test",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid");

    let wrapper = tmux_pane_wrapper_command(&request);
    assert!(
        wrapper.contains("status off"),
        "wrapper must disable tmux status bar; got {wrapper}"
    );
    assert!(
        wrapper.contains("'status-off-test'"),
        "status off must target the session name (single-quote escaped); got {wrapper}"
    );
}

/// **issue #241 Finding #2**: The pane wrapper exports extra env vars (e.g.
/// `JEFE_TUTORIAL_CAPTURE=1`) so the launched Jefe process knows to disable
/// nested managed-agent tmux status bars.
#[test]
fn pane_wrapper_exports_extra_env_vars() {
    let request = TmuxStartRequest::command(
        "extra-env-test",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid")
    .with_extra_env("JEFE_TUTORIAL_CAPTURE", "1");

    let wrapper = tmux_pane_wrapper_command(&request);
    assert!(
        wrapper.contains("export 'JEFE_TUTORIAL_CAPTURE'='1'"),
        "wrapper must export extra env var; got {wrapper}"
    );
}

/// **issue #241 Finding #2**: Without extra env vars, the wrapper does not
/// include any extra export statements (normal production is unchanged).
#[test]
fn pane_wrapper_omits_extra_env_when_empty() {
    let request = TmuxStartRequest::command(
        "no-extra-env",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid");

    let wrapper = tmux_pane_wrapper_command(&request);
    assert!(
        !wrapper.contains("JEFE_TUTORIAL_CAPTURE"),
        "wrapper must not include tutorial-capture env when not set: {wrapper}"
    );
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

// --- #241 Finding #1: session name shell-injection adversarial tests ---------

/// The session name is interpolated into a shell command string inside
/// double quotes (`set-option -t "{session}"`). A session name containing
/// a double quote must be escaped so it cannot break out of the quoting
/// context and inject shell commands.
///
/// We verify by asserting the wrapper string contains the session name
/// inside single quotes (via `shell_escape_single`), NOT inside double
/// quotes. If the session name were still in a double-quoted context, a
/// `"` character would prematurely close the quote.
#[test]
fn session_name_with_double_quote_is_escaped_in_wrapper() {
    let request = TmuxStartRequest::command(
        "evil\";touch /tmp/pwned;#",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid");

    let wrapper = tmux_pane_wrapper_command(&request);
    // The session name must NOT appear unescaped in a double-quoted context.
    // Instead it must be single-quote escaped.
    assert!(
        !wrapper.contains("\"evil\""),
        "session name with double quote must not appear in a double-quoted context: {wrapper}"
    );
    // The session name (including the injected text) must be inside single
    // quotes so the shell treats it as a literal string, not commands.
    assert!(
        wrapper.contains("'evil\";touch /tmp/pwned;#'"),
        "session name with double quote must be fully enclosed in single quotes: {wrapper}"
    );
}

/// A session name containing `$()` command substitution must not execute.
/// In a double-quoted context, `$()` would be expanded by the shell. With
/// single-quote escaping, it is inert.
#[test]
fn session_name_with_dollar_paren_is_neutralized() {
    let request = TmuxStartRequest::command(
        "evil$(whoami)",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid");

    let wrapper = tmux_pane_wrapper_command(&request);
    // The $() must be inside single quotes so it is not expanded.
    assert!(
        wrapper.contains("'evil$(whoami)'"),
        "session name with $() must be single-quote escaped: {wrapper}"
    );
}

/// A session name containing backticks must not execute.
#[test]
fn session_name_with_backticks_is_neutralized() {
    let request = TmuxStartRequest::command(
        "evil`whoami`",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid");

    let wrapper = tmux_pane_wrapper_command(&request);
    assert!(
        wrapper.contains("'evil`whoami`'"),
        "session name with backticks must be single-quote escaped: {wrapper}"
    );
}

/// A session name containing a newline must not break the shell command.
/// Single-quote escaping wraps the entire name so newlines are literal.
#[test]
fn session_name_with_newline_is_escaped() {
    let request = TmuxStartRequest::command(
        "evil\nwhoami",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid");

    let wrapper = tmux_pane_wrapper_command(&request);
    // The wrapper must still contain the exec command at the end.
    assert!(
        wrapper.contains("exec '/bin/true'"),
        "wrapper must still end with exec command despite newline in session name: {wrapper}"
    );
}

/// A session name containing a backslash must not escape the quoting context.
#[test]
fn session_name_with_backslash_is_escaped() {
    let request = TmuxStartRequest::command(
        "evil\\\"",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid");

    let wrapper = tmux_pane_wrapper_command(&request);
    // In single-quote escaping, backslash is literal (not an escape char).
    // The wrapper must not have the backslash escape a closing single quote.
    assert!(
        wrapper.contains("exec '/bin/true'"),
        "wrapper must still end with exec command despite backslash in session name: {wrapper}"
    );
}

/// A normal (safe) session name still works correctly after the escaping fix.
#[test]
fn normal_session_name_unaffected_by_escaping() {
    let request = TmuxStartRequest::command(
        "my-harness-session",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid");

    let wrapper = tmux_pane_wrapper_command(&request);
    assert!(
        wrapper.contains("'my-harness-session'"),
        "normal session name must appear in single-quoted form: {wrapper}"
    );
}

/// A session name containing a single quote is properly escaped using the
/// `'\''` idiom.
#[test]
fn session_name_with_single_quote_uses_escape_idiom() {
    let request = TmuxStartRequest::command(
        "it's",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid");

    let wrapper = tmux_pane_wrapper_command(&request);
    assert!(
        wrapper.contains("'it'\\''s'"),
        "session name with single quote must use the '\\'' escape idiom: {wrapper}"
    );
}
