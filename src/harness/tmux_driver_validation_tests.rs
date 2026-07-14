//! Tests for `TmuxStartRequest` validation and pane wrapper command generation.
//!
//! Extracted from the original `tmux_driver_tests.rs` to keep file sizes under
//! the project limit.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P03
//! @requirement REQ-TMUX-HARNESS-003

use std::path::PathBuf;

use super::*;

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
pub(super) struct SessionGuard<'a> {
    pub(super) driver: &'a TmuxDriver,
    pub(super) session: TmuxSession,
}

impl Drop for SessionGuard<'_> {
    fn drop(&mut self) {
        let _ = self.driver.cleanup_session(&self.session);
    }
}

pub(super) fn temp_path() -> PathBuf {
    std::env::temp_dir()
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
        "unset TMUX TMUX_PANE TMUX_TMPDIR; tmux -f /dev/null -L {socket} set-option -pt \"$TMUX_PANE\" remain-on-exit on; tmux -f /dev/null -L {socket} set-option -wt \"$TMUX_PANE\" history-limit 1000; exec '/bin/echo' 'a b' 'quote'\\''it'"
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

/// By default (origin/main behavior), the pane wrapper does NOT disable the
/// tmux status bar. The status bar is left at its server default.
#[test]
fn pane_wrapper_default_does_not_disable_status_bar() {
    let request = TmuxStartRequest::command(
        "status-default",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid");

    let wrapper = tmux_pane_wrapper_command(&request);
    assert!(
        !wrapper.contains("status off"),
        "default wrapper must NOT disable tmux status bar; got {wrapper}"
    );
}

/// When explicitly opted-in via `with_suppress_status_bar(true)`, the pane
/// wrapper disables the tmux status bar, targeting the session by name.
#[test]
fn pane_wrapper_opt_in_disables_status_bar() {
    let request = TmuxStartRequest::command(
        "status-off-test",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("request should be valid")
    .with_suppress_status_bar(true);

    let wrapper = tmux_pane_wrapper_command(&request);
    assert!(
        wrapper.contains("status off"),
        "opt-in wrapper must disable tmux status bar; got {wrapper}"
    );
    assert!(
        wrapper.contains("'status-off-test'"),
        "status off must target the session name (single-quote escaped); got {wrapper}"
    );
}

/// The pane wrapper exports extra env vars so the launched process receives
/// caller-controlled environment configuration.
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
    .value_or_panic("request should be valid");
    let request = request
        .with_extra_env("JEFE_HARNESS_TEST_MODE", "unit")
        .value_or_panic("with_extra_env should accept valid key");

    let wrapper = tmux_pane_wrapper_command(&request);
    assert!(
        wrapper.contains("export 'JEFE_HARNESS_TEST_MODE'='unit'"),
        "wrapper must export extra env var; got {wrapper}"
    );
}

/// Without extra env vars, the wrapper does not include any extra export
/// statements (normal production is unchanged).
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
        !wrapper.contains("JEFE_HARNESS_TEST_MODE"),
        "wrapper must not include harness test env when not set: {wrapper}"
    );
}

/// extra_env keys must be valid portable shell identifiers. An empty key is
/// rejected at validation time, before tmux launch.
#[test]
fn extra_env_empty_key_rejected_at_validation() {
    let request = TmuxStartRequest::command(
        "bad-env",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("base request should be valid");
    let result = request.with_extra_env("", "value");
    assert!(result.is_err(), "empty extra_env key must be rejected");
}

/// extra_env keys starting with a digit are rejected (not portable shell ids).
#[test]
fn extra_env_leading_digit_key_rejected_at_validation() {
    let request = TmuxStartRequest::command(
        "bad-env",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("base request should be valid");
    let result = request.with_extra_env("1KEY", "value");
    assert!(
        result.is_err(),
        "extra_env key starting with digit must be rejected"
    );
}

/// extra_env keys containing punctuation are rejected (injection risk).
#[test]
fn extra_env_punctuation_key_rejected_at_validation() {
    let request = TmuxStartRequest::command(
        "bad-env",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("base request should be valid");
    let result = request.with_extra_env("KEY;rm -rf", "value");
    assert!(
        result.is_err(),
        "extra_env key with punctuation must be rejected"
    );
}

/// extra_env keys with valid underscore/alphanumeric are accepted.
#[test]
fn extra_env_valid_underscore_alphanumeric_accepted() {
    let request = TmuxStartRequest::command(
        "good-env",
        vec!["/bin/true".to_string()],
        temp_path(),
        80,
        24,
        1000,
    )
    .value_or_panic("base request should be valid");
    let request = request
        .with_extra_env("_VALID_KEY_123", "value")
        .value_or_panic("valid key should be accepted");
    let wrapper = tmux_pane_wrapper_command(&request);
    assert!(wrapper.contains("export '_VALID_KEY_123'"));
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
