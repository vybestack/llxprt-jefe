//! Session liveness must not be collapsed into a bare `bool` (issue #597).
//!
//! `SessionLiveness` separates `Missing` -- the session is gone -- from
//! `Unavailable` -- the multiplexer could not be asked. A helper returning
//! `bool` erases that distinction, and the resulting `false` reads as "not
//! alive" at every call site.
//!
//! #541 removed nine such collapses from the paths that decide agent death.
//! The helpers covered here were left behind because only tests called them,
//! which is precisely what makes them dangerous: `is_alive` reads like a total
//! predicate, so
//!
//! ```ignore
//! if !runtime.is_alive(&agent_id) { /* mark dead */ }
//! ```
//!
//! reintroduces the bug while looking obviously correct. The
//! `xtask check observation-coercion` policy cannot see this shape, because it
//! matches an uncertain accessor spent through an `Option` combinator and
//! these helpers return a bare `bool` with no accessor to match.
//!
//! So the deletion is asserted here, in source, rather than trusted to stay
//! deleted.

use std::path::{Path, PathBuf};

/// Helpers deleted by #597, each of which answered a three-valued question
/// with two values.
const DELETED_COLLAPSING_HELPERS: &[(&str, &str)] = &[
    ("src/runtime/liveness.rs", "fn check_session_alive"),
    ("src/runtime/liveness.rs", "fn check_remote_session_alive"),
    ("src/runtime/manager.rs", "fn is_alive"),
    ("src/runtime/manager.rs", "fn session_exists"),
];

#[test]
fn no_helper_answers_session_liveness_with_a_bare_bool() {
    for (relative, signature) in DELETED_COLLAPSING_HELPERS {
        let text = read_repo_text(relative);
        assert!(
            !text.contains(signature),
            "{relative} still declares `{signature}`. A session-liveness answer of `false` \
             cannot distinguish a session that is gone from one that could not be asked \
             about, which is the collapse issue #541 exists to remove (issue #597)."
        );
    }
}

/// The runtime manager trait must not offer a boolean liveness predicate.
///
/// Checked separately from the inherent methods above: a trait method is the
/// more dangerous of the two, because every implementor inherits the shape and
/// every caller programs against it.
#[test]
fn the_runtime_manager_trait_exposes_no_boolean_liveness_predicate() {
    let text = read_repo_text("src/runtime/manager.rs");
    for forbidden in [
        "fn is_alive(&self, agent_id: &AgentId) -> bool",
        "fn session_exists(&self, agent_id: &AgentId) -> bool",
    ] {
        assert!(
            !text.contains(forbidden),
            "RuntimeManager still declares `{forbidden}`. Implementors and callers would \
             inherit a total predicate over a question that has three answers (issue #597)."
        );
    }
}

/// Nothing may re-export the deleted helpers.
///
/// A re-export outlives its definition in review: the name stays reachable and
/// the next caller finds it by autocomplete rather than by reading the module.
#[test]
fn the_runtime_module_re_exports_no_collapsing_liveness_helper() {
    let text = read_repo_text("src/runtime/mod.rs");
    for forbidden in ["check_session_alive", "check_remote_session_alive"] {
        assert!(
            !text.contains(forbidden),
            "src/runtime/mod.rs still re-exports `{forbidden}` (issue #597)."
        );
    }
}

fn read_repo_text(relative: &str) -> String {
    let path = repo_path(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
