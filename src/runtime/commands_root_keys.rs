//! Windows psmux root-table transparent-key configuration (#465).
//!
//! psmux 3.3.7 ships a default root-table binding `PageUp -> copy-mode -u`
//! that consumes bare PageUp events before they reach the pane child. The
//! unbind command below removes that binding so Jefe-owned psmux sessions
//! deliver transparent Page-key semantics. The upstream psmux fix is commit
//! 913f5e9 (July 23, 2026), after the 3.3.7 release; this configures around
//! the defect until a fixed release is available.

use super::{local_prefix_value, prefix_options_for_passthrough};

/// The root-table `PageUp` unbind command for transparent Page-key delivery.
pub(super) const PAGE_UP_ROOT_UNBIND: &[&str] = &["unbind-key", "-T", "root", "PageUp"];

/// Configure multiplexer prefix keys and root-table bindings for transparent
/// child input (#200, #260, #465).
///
/// Unix applies prefix options to `session_name`. Windows psmux ignores
/// session-scoped prefix values, so its private server is configured globally.
/// Windows assigns `prefix` to Jefe-owned F12 because psmux 3.3.6 still
/// reserves `C-b` when the option is `None`; `prefix2` stays disabled.
///
/// On Windows, psmux 3.3.7's default root-table `PageUp -> copy-mode -u`
/// binding is removed so bare PageUp events reach the pane child (#465).
/// Unix tmux does not ship this default binding, so the unbind is Windows-only.
pub(super) fn configure_prefix_with(
    session_name: &str,
    mut apply: impl FnMut(&[&str]) -> Result<(), String>,
) -> Result<(), String> {
    for option in prefix_options_for_passthrough() {
        let value = if *option == "prefix" {
            local_prefix_value()
        } else {
            "None"
        };
        if cfg!(windows) {
            apply(["set-option", "-g", option, value].as_ref())?;
        } else {
            apply(["set-option", "-t", session_name, option, value].as_ref())?;
        }
    }
    if cfg!(windows) {
        apply(PAGE_UP_ROOT_UNBIND)?;
    }
    Ok(())
}
