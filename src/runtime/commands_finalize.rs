//! Local-session finalization (clipboard/prefix passthrough, remain-on-exit,
//! style, warning), split out of `commands.rs` so that file stays under the
//! source-size hard limit.

use super::{
    apply_session_style, configure_prefix_for_passthrough, enforce_clipboard_passthrough,
    tmux_cmd_status,
};
use tracing::debug;

/// Apply the post-`new-session` tmux options for a local agent session.
///
/// - Clipboard passthrough so the agent's copy key reaches jefe's clipboard
///   handler.
/// - Prefix passthrough so the agent's prefix key is forwarded to the pane.
/// - `remain-on-exit on` so a crashed agent does not close the pane and lose
///   the capture/output buffer.
/// - Apply jefe's session style.
/// - Display any preflight warning as a tmux display-message.
pub(super) fn finalize_local_session(session_name: &str, warning: Option<String>) {
    enforce_clipboard_passthrough(session_name);
    if let Err(error) = configure_prefix_for_passthrough(session_name) {
        debug!(session_name = %session_name, error = %error, "prefix passthrough option failed on create; will retry on attach");
    }
    let _ = tmux_cmd_status(
        ["set-option", "-t", session_name, "remain-on-exit", "on"].as_ref(),
        None,
    );
    apply_session_style(session_name);

    if let Some(warning) = warning {
        debug!(session_name = %session_name, warning = %warning, "runtime launch preflight warning");
        let _ = tmux_cmd_status(
            [
                "display-message",
                "-t",
                session_name,
                &format!("[jefe] warning: {warning}"),
            ]
            .as_ref(),
            None,
        );
    }
}
