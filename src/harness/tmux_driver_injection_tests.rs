//! #241 Finding #1: session name shell-injection adversarial tests.
//!
//! Extracted from the original `tmux_driver_tests.rs` to keep file sizes under
//! the project limit.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P03
//! @requirement REQ-TMUX-HARNESS-003

use super::*;

use super::validation_tests::temp_path;

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
    .value_or_panic("request should be valid")
    .with_suppress_status_bar(true);

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
    .value_or_panic("request should be valid")
    .with_suppress_status_bar(true);

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
    .value_or_panic("request should be valid")
    .with_suppress_status_bar(true);

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
/// Uses `with_suppress_status_bar(true)` so the session name appears in the
/// `set-option -t <session>` segment where escaping is exercised.
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
    .value_or_panic("request should be valid")
    .with_suppress_status_bar(true);

    let wrapper = tmux_pane_wrapper_command(&request);
    assert!(
        wrapper.contains("'my-harness-session'"),
        "normal session name must appear in single-quoted form: {wrapper}"
    );
}

/// A session name containing a single quote is properly escaped using the
/// `'\''` idiom. Uses `with_suppress_status_bar(true)` so the session name
/// appears in the `set-option -t <session>` segment where escaping matters.
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
    .value_or_panic("request should be valid")
    .with_suppress_status_bar(true);

    let wrapper = tmux_pane_wrapper_command(&request);
    assert!(
        wrapper.contains("'it'\\''s'"),
        "session name with single quote must use the '\\'' escape idiom: {wrapper}"
    );
}
