//! Behavioral contracts for the installation-identity boundary (issue #547).
//!
//! The process-global cell itself is deliberately untested here: writing to it
//! would fix one answer for every other test in the binary. The decisions it
//! makes are tested through the pure helpers it delegates to.

use super::installation::{
    IdentityUnavailable, InstallationError, current_from, reconcile, resolve_with,
};
use super::namespace::{InstallationIdentity, NamespaceError, NamespaceOrigin};
use std::path::Path;
use std::sync::OnceLock;

fn state_path() -> &'static Path {
    Path::new(r"C:\Users\dev\AppData\Local\jefe\state.json")
}

/// Reading the active identity before startup initializes it must fail without
/// mutating the write-once cell. A second read proves the accessor did not
/// quietly install an ambient fallback on the first attempt.
#[test]
fn current_requires_authoritative_startup_initialization() {
    let active = OnceLock::new();

    assert_eq!(current_from(&active), Err(IdentityUnavailable));
    assert_eq!(current_from(&active), Err(IdentityUnavailable));
    assert!(active.get().is_none());
}

/// With no override, identity comes from the location jefe was launched from.
#[test]
fn without_an_override_the_identity_comes_from_the_state_path() {
    let resolved = resolve_with(None, state_path())
        .unwrap_or_else(|error| panic!("a plain state path should resolve: {error}"));

    assert_eq!(resolved, InstallationIdentity::for_state_path(state_path()));
    assert!(!resolved.origin().is_override());
}

/// Repeated resolution of one effective state path must re-adopt the same
/// installation across process restarts, rebuilds, and upgrades.
#[test]
fn identity_is_stable_across_repeated_resolution_for_the_same_state_path() {
    let first = resolve_with(None, state_path())
        .unwrap_or_else(|error| panic!("first identity should resolve: {error}"));
    let second = resolve_with(None, state_path())
        .unwrap_or_else(|error| panic!("second identity should resolve: {error}"));

    assert_eq!(first.id(), second.id());
    assert_eq!(first, InstallationIdentity::for_state_path(state_path()));
}

/// Different effective state paths must select different multiplexer servers.
#[test]
fn distinct_state_paths_derive_distinct_identities() {
    let first_path = Path::new(r"C:\work\one\.jefe\state.json");
    let second_path = Path::new(r"C:\work\two\.jefe\state.json");
    let first = resolve_with(None, first_path)
        .unwrap_or_else(|error| panic!("first identity should resolve: {error}"));
    let second = resolve_with(None, second_path)
        .unwrap_or_else(|error| panic!("second identity should resolve: {error}"));

    assert_ne!(first.id(), second.id());
}

/// A present but blank override fails closed instead of silently selecting the
/// derived installation namespace.
#[test]
fn a_blank_override_is_rejected() {
    let error = resolve_with(Some("   "), state_path())
        .err()
        .unwrap_or_else(|| panic!("a present blank override must be rejected"));

    assert_eq!(error, InstallationError::Override(NamespaceError::Empty));
}

/// A deliberate override wins over the state path and says where it came from.
#[test]
fn an_override_wins_over_the_state_path() {
    let resolved = resolve_with(Some("ab-test"), state_path())
        .unwrap_or_else(|error| panic!("a plain override should resolve: {error}"));

    assert_eq!(
        resolved.origin(),
        &NamespaceOrigin::Override("ab-test".to_owned())
    );
    assert_ne!(resolved, InstallationIdentity::for_state_path(state_path()));
}

/// An unusable override is an error, never a silent fall-back.
///
/// Falling back would attach the operator to the exact session pool they asked
/// to be separated from, which is the failure mode issue #547 is about.
#[test]
fn an_unusable_override_is_an_error_rather_than_a_fallback() {
    assert_eq!(
        resolve_with(Some("two words"), state_path()),
        Err(InstallationError::Override(
            NamespaceError::IllegalCharacter { character: ' ' }
        ))
    );
}

/// Re-initializing with the same answer is a no-op, so double-initialization
/// during startup refactors is harmless.
#[test]
fn re_initializing_with_the_same_identity_is_accepted() {
    let first = InstallationIdentity::for_state_path(state_path());
    let second = InstallationIdentity::for_state_path(state_path());

    assert_eq!(reconcile(&first, &second), Ok(()));
}

/// Switching identity mid-process is refused, and the refusal names both sides.
///
/// A server may already be running under the active identity; silently moving
/// would orphan every session on it.
#[test]
fn switching_identity_mid_process_is_refused() {
    let active = InstallationIdentity::for_state_path(Path::new(r"C:\work\one\.jefe\state.json"));
    let requested =
        InstallationIdentity::for_state_path(Path::new(r"C:\work\two\.jefe\state.json"));

    assert_eq!(
        reconcile(&active, &requested),
        Err(InstallationError::AlreadyResolved {
            active: active.id().as_str().to_owned(),
            requested: requested.id().as_str().to_owned(),
        })
    );
}

/// The refusal has to be readable, since it surfaces as a startup error.
#[test]
fn the_refusal_explains_itself() {
    let error = InstallationError::AlreadyResolved {
        active: "jefe-1111111111111111".to_owned(),
        requested: "jefe-2222222222222222".to_owned(),
    };

    let rendered = error.to_string();
    assert!(rendered.contains("jefe-1111111111111111"));
    assert!(rendered.contains("jefe-2222222222222222"));
}

/// A rejected override must name the variable an operator has to fix.
#[test]
fn a_rejected_override_names_the_variable() {
    let error = InstallationError::Override(NamespaceError::Empty);

    assert!(error.to_string().contains("JEFE_NAMESPACE"));
}
