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
///
/// Defined by the declared divergence rather than spelled again here, so the
/// remediation and the expectation it satisfies cannot drift apart (#540).
pub(super) const PAGE_UP_ROOT_UNBIND: [&str; 4] =
    crate::runtime::multiplexer_contract::PAGE_UP_ROOT_UNBIND_COMMAND;

/// Enable extended-key reporting on the Jefe-owned multiplexer server (#627).
///
/// The multiplexer, not Jefe, decides what a pane child receives for a
/// modified key. With `extended-keys off` — the default — it collapses every
/// modified Enter to a bare `CR` before the child ever sees it, so chords such
/// as `Ctrl+Enter` are unreachable no matter how Jefe encodes them. With the
/// option on, a child that asks for extended keys receives the modified form
/// and a child that does not still receives the plain `CR`, so this is
/// transparent for children that never negotiate.
///
/// This is a server option: it is scoped to Jefe's private multiplexer server,
/// not the user's own.
pub(super) const EXTENDED_KEYS_ENABLE: [&str; 4] = ["set-option", "-s", "extended-keys", "on"];

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
///
/// Both platforms enable extended-key reporting on Jefe's private multiplexer
/// server so modified Enter chords survive the multiplexer hop (#627).
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
        apply(&PAGE_UP_ROOT_UNBIND)?;
    }
    apply(&EXTENDED_KEYS_ENABLE)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::configure_prefix_with;

    use std::cell::RefCell;

    fn recorded_calls(session: &str) -> Vec<Vec<String>> {
        let captured: RefCell<Vec<Vec<String>>> = RefCell::new(Vec::new());
        let result = configure_prefix_with(session, |args| {
            captured
                .borrow_mut()
                .push(args.iter().map(|arg| (*arg).to_owned()).collect());
            Ok::<(), String>(())
        });
        assert!(result.is_ok(), "configuration should succeed: {result:?}");
        captured.into_inner()
    }

    /// The multiplexer, not jefe, decides what a pane child receives for a
    /// modified key. With extended-key reporting off — its default — it
    /// collapses every modified Enter to a bare CR before the child sees it,
    /// so a chord such as `Ctrl+Enter` is unreachable however jefe encodes it
    /// (issue #627).
    #[test]
    fn extended_keys_are_enabled_on_the_jefe_owned_server() {
        let calls = recorded_calls("jefe-agent-extkeys");

        let enabled = calls.iter().any(|call| {
            matches!(
                call.as_slice(),
                [set, scope, option, value]
                    if set == "set-option"
                        && scope == "-s"
                        && option == "extended-keys"
                        && value == "on"
            )
        });
        assert!(
            enabled,
            "extended keys must be enabled on jefe's own multiplexer server; calls: {calls:?}"
        );
    }

    /// The option is server-scoped, so it never reaches a multiplexer server
    /// the user owns.
    #[test]
    fn extended_keys_are_not_applied_to_a_session_or_globally() {
        let calls = recorded_calls("jefe-agent-extkeys");

        for call in &calls {
            if call.iter().any(|arg| arg == "extended-keys") {
                assert!(
                    call.contains(&"-s".to_owned()),
                    "extended-keys must be set with server scope; got {call:?}"
                );
            }
        }
    }
}
