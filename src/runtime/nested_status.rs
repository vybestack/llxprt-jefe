//! Tutorial-capture nested tmux status bar control.
//!
//! **issue #241 Finding #2**: The harness already disables the tmux status
//! bar on its own sessions, but Jefe creates nested managed-agent tmux
//! sessions internally (via `commands::create_session`). Without this module,
//! those nested sessions can show a status bar with the hostname or live
//! clock, leaking into tutorial-capture artifacts.
//!
//! This module provides a capture-only runtime-boundary control: when
//! `JEFE_TUTORIAL_CAPTURE` is set to a truthy value, the runtime disables the
//! status bar on every managed agent session. Normal production behavior
//! (styled status bar) is unchanged when the env var is absent or falsy.
//!
//! ## Boundary
//!
//! This module reads an environment variable and calls back into
//! `commands::tmux_cmd_status` (the runtime's private tmux helper). It does
//! not own the tmux server lifecycle.

use tracing::debug;

/// The environment variable name that signals tutorial-capture mode.
///
/// When set to a truthy value (`1`, `true`, `yes` — case-insensitive), the
/// runtime disables the status bar on managed agent sessions so captures
/// never include the hostname, session name, or live clock.
pub const TUTORIAL_CAPTURE_ENV: &str = "JEFE_TUTORIAL_CAPTURE";

/// Whether the runtime should disable the tmux status bar on managed agent
/// sessions.
///
/// Returns `true` only when [`TUTORIAL_CAPTURE_ENV`] is set to a truthy value.
/// This is a capture-only control: normal production behavior (styled status
/// bar) is unchanged when the env var is absent or falsy.
#[must_use]
pub fn should_disable_nested_tmux_status() -> bool {
    is_tutorial_capture_env(std::env::var(TUTORIAL_CAPTURE_ENV).ok().as_deref())
}

/// Pure predicate: whether the given env-var value (or `None` if unset)
/// signals tutorial-capture mode. Separated from
/// [`should_disable_nested_tmux_status`] so it is directly unit-testable
/// without mutating process environment variables (which requires `unsafe`
/// under Rust 2024).
#[must_use]
pub fn is_tutorial_capture_env(value: Option<&str>) -> bool {
    match value {
        Some(v) => matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"),
        None => false,
    }
}

/// Build the tmux argv to disable the status bar for a session. Pure
/// (does not spawn tmux) so it is directly unit-testable.
#[must_use]
pub fn nested_status_disable_argv(session_name: &str) -> Vec<String> {
    vec![
        "set-option".to_string(),
        "-t".to_string(),
        session_name.to_string(),
        "status".to_string(),
        "off".to_string(),
    ]
}

/// Disable the tmux status bar on a managed agent session when running under
/// tutorial-capture. No-op in normal production.
///
/// Calls back into `commands::tmux_cmd_status` via the function reference
/// passed by the caller, keeping this module free of a direct dependency on
/// the tmux command builder.
pub fn disable_nested_tmux_status_if_capture(
    session_name: &str,
    tmux_run: impl Fn(&[&str]) -> Result<(), String>,
) {
    if !should_disable_nested_tmux_status() {
        return;
    }
    let disable_argv = nested_status_disable_argv(session_name);
    let refs: Vec<&str> = disable_argv.iter().map(String::as_str).collect();
    if let Err(error) = tmux_run(&refs) {
        debug!(
            session_name = %session_name,
            error = %error,
            "could not disable nested tmux status for tutorial capture"
        );
    }
}
