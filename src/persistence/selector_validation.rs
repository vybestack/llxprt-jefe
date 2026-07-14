//! Persistence-boundary selector validation.
//!
//! Extracted from `persistence/mod.rs` so that module stays under the
//! recommended line limit while keeping this a pure persistence-boundary
//! validator (no runtime or UI dependencies).
//!
//! @requirement issue #269

use super::{PersistenceError, State};

/// Validate all persisted LLxprt selector locations in a deserialized state.
///
/// A hand-edited `state.json` could carry an embedded NUL byte (`\u0000`) in
/// any version selector field. JSON deserialization succeeds (it is a valid
/// JSON string), but the resulting `String` is structurally unrepresentable
/// as a process argument or shell-escaped token. Loading such state silently
/// would let the invalid selector persist until a launch attempt that can
/// never succeed.
///
/// This helper runs the reusable domain validation
/// ([`crate::domain::validate_version_selector`]) over every persisted
/// selector location:
/// - Repository defaults (`default_llxprt_version`)
/// - Agents (`llxprt_version`)
/// - Runtime bindings (`launch_signature.llxprt_version`)
///
/// It does **not** blank-out invalid strings — it returns a typed
/// [`PersistenceError::ParseError`] naming the first offending location so
/// the user can fix the source file. The runtime pre-kill validation
/// ([`crate::runtime::RuntimeError::InvalidVersionSelector`]) remains as a
/// defense-in-depth backstop.
///
/// # Errors
///
/// Returns [`PersistenceError::ParseError`] when any selector contains an
/// embedded NUL byte, with a message identifying the location.
pub fn validate_state_selectors(state: &State) -> Result<(), PersistenceError> {
    for (idx, repo) in state.repositories.iter().enumerate() {
        if let Err(err) = crate::domain::validate_version_selector(&repo.default_llxprt_version) {
            return Err(PersistenceError::ParseError(format!(
                "repository[{idx}] ('{}') default_llxprt_version: {err}",
                repo.name
            )));
        }
    }
    for (idx, agent) in state.agents.iter().enumerate() {
        if let Err(err) = crate::domain::validate_version_selector(&agent.llxprt_version) {
            return Err(PersistenceError::ParseError(format!(
                "agent[{idx}] ('{}') llxprt_version: {err}",
                agent.name
            )));
        }
        if let Some(binding) = &agent.runtime_binding
            && let Err(err) =
                crate::domain::validate_version_selector(&binding.launch_signature.llxprt_version)
        {
            return Err(PersistenceError::ParseError(format!(
                "agent[{idx}] ('{}') runtime_binding launch_signature llxprt_version: {err}",
                agent.name
            )));
        }
    }
    Ok(())
}
