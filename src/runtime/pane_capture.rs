//! tmux pane capture / introspection helpers.
//!
//! Extracted from `commands.rs` so that file stays under the source-file size
//! hard limit. Each function shells out to the jefe-private tmux socket via
//! [`super::commands::tmux_command`].

use super::commands::tmux_command;

/// Capture pane output for a session as plain text lines.
pub fn capture_pane_lines(session_name: &str) -> Option<Vec<String>> {
    let output = tmux_command()
        .args(["capture-pane", "-p", "-t", session_name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    Some(text.lines().map(std::borrow::ToOwned::to_owned).collect())
}

/// Build the `capture-pane -p -S -<N> -E -` argv for a bounded history capture.
///
/// `-S -<history_lines>` starts `history_lines` lines before the top of the
/// visible pane; `-E -` ends at the bottom of the visible pane (current line).
/// This returns plain-text scrollback history **including the visible pane**.
/// The caller (`TmuxRuntimeManager::capture_history`) strips the last
/// `live_snapshot.rows` lines so the cached result is history ABOVE the
/// visible pane only — the live Alacritty snapshot already represents the
/// visible pane (issue #198 review fix #1).
///
/// Factored as a pure `#[must_use]` function so the argv composition is
/// unit-testable without spawning tmux (issue #198).
#[must_use]
pub fn capture_pane_history_args(session_name: &str, history_lines: usize) -> Vec<String> {
    let start = if history_lines == 0 {
        "0".to_owned()
    } else {
        format!("-{history_lines}")
    };
    vec![
        "capture-pane".to_owned(),
        "-p".to_owned(),
        "-t".to_owned(),
        session_name.to_owned(),
        "-S".to_owned(),
        start,
        "-E".to_owned(),
        "-".to_owned(),
    ]
}

/// Capture bounded scrollback history for a session as plain text lines.
///
/// Uses `capture-pane -p -S -<history_lines> -E -` to retrieve the last
/// `history_lines` lines of tmux scrollback **including the visible pane**.
/// The caller must strip the visible-pane rows before composing with the live
/// snapshot to avoid duplication (issue #198 review fix #1).
/// Returns `None` if tmux is unavailable or the command fails (issue #198).
pub fn capture_pane_history(session_name: &str, history_lines: usize) -> Option<Vec<String>> {
    let argv = capture_pane_history_args(session_name, history_lines);
    let argv_refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    let output = tmux_command().args(&argv_refs).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Some(text.lines().map(std::borrow::ToOwned::to_owned).collect())
}

/// Parse the stdout of `tmux list-panes -t <session> -F '#{pane_pid}'` into a
/// single PID.
///
/// Returns the first non-empty trimmed line parsed as a `u32`, or `None` if the
/// output is empty/garbage. Factored out of [`pane_pid`] so the parsing logic is
/// unit-testable without spawning tmux.
#[must_use]
pub fn parse_pane_pid(stdout: &str) -> Option<u32> {
    stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .and_then(|line| line.parse::<u32>().ok())
}

/// Query the PID of the (first) pane in a local tmux session.
///
/// Runs `tmux list-panes -t <session> -F '#{pane_pid}'` against the jefe-private
/// socket. Because `llxprt` runs as the pane's direct command (not a shell
/// wrapper), the returned PID **is** the worker process itself. Local sessions
/// only.
///
/// Returns `None` if tmux is unavailable, the session does not exist, or the
/// output cannot be parsed. This is the PID-fallback input used to detect
/// workers that are still alive after their tmux session is gone.
#[must_use]
pub fn pane_pid(session_name: &str) -> Option<u32> {
    let output = tmux_command()
        .args(["list-panes", "-t", session_name, "-F", "#{pane_pid}"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_pane_pid(&String::from_utf8_lossy(&output.stdout))
}
