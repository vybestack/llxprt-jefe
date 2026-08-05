//! Behavioral contracts for privacy-conscious runtime namespace identity.

use super::identity::{
    namespace_for_identity, namespace_for_state_path, unique_namespace_for_identity,
    unique_namespace_for_state_path,
};
use std::path::Path;

#[test]
fn namespace_is_deterministic_private_and_valid() {
    let raw_identity = b"S-1-5-21-private-user-material";
    let first = namespace_for_identity(raw_identity);
    let second = namespace_for_identity(raw_identity);

    assert_eq!(first, second);
    assert!(first.starts_with("jefe-"));
    assert!(
        first
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    );
    assert!(!first.contains("private"));
    assert!(!first.contains("S-1-5"));
}

#[test]
fn namespaces_separate_users_and_parallel_runs() {
    let first_user = namespace_for_identity(b"user-one");
    let second_user = namespace_for_identity(b"user-two");
    assert_ne!(first_user, second_user);

    let first_run = unique_namespace_for_identity(b"user-one");
    let second_run = unique_namespace_for_identity(b"user-one");
    assert_ne!(first_run, second_run);
    assert!(first_run.starts_with(&first_user));
    assert!(second_run.starts_with(&first_user));
}

/// Issue #547 V3: the namespace must survive path casing differences.
///
/// Windows paths are case-insensitive, so `%LOCALAPPDATA%` arriving with
/// different casing between launches must not move the namespace and orphan
/// every running session.
#[test]
fn namespace_ignores_state_path_casing() {
    let lower = namespace_for_state_path(Path::new(r"c:\users\dev\appdata\local\jefe\state.json"));
    let upper = namespace_for_state_path(Path::new(r"C:\Users\Dev\AppData\Local\Jefe\State.json"));

    assert_eq!(lower, upper);
}

/// The same location spelled with either separator is the same installation.
#[test]
fn namespace_ignores_state_path_separator_style() {
    let backslash = namespace_for_state_path(Path::new(r"C:\Users\dev\jefe\state.json"));
    let forward = namespace_for_state_path(Path::new("C:/Users/dev/jefe/state.json"));

    assert_eq!(backslash, forward);
}

/// A trailing separator is spelling, not identity.
#[test]
fn namespace_ignores_trailing_state_path_separator() {
    let bare = namespace_for_state_path(Path::new(r"C:\Users\dev\jefe"));
    let trailing = namespace_for_state_path(Path::new(r"C:\Users\dev\jefe\"));

    assert_eq!(bare, trailing);
}

/// Issue #547 V4: genuinely separate users keep separate session pools.
///
/// Distinct accounts resolve distinct `%LOCALAPPDATA%` roots, so state-path
/// keying preserves user isolation structurally rather than by special case.
#[test]
fn namespace_separates_distinct_user_state_paths() {
    let alice =
        namespace_for_state_path(Path::new(r"C:\Users\alice\AppData\Local\jefe\state.json"));
    let bob = namespace_for_state_path(Path::new(r"C:\Users\bob\AppData\Local\jefe\state.json"));

    assert_ne!(alice, bob);
}

/// Separate installations for one user (for example `--config` roots) stay separate.
#[test]
fn namespace_separates_distinct_installations_for_one_user() {
    let first = namespace_for_state_path(Path::new(r"C:\work\tree-one\.jefe\state.json"));
    let second = namespace_for_state_path(Path::new(r"C:\work\tree-two\.jefe\state.json"));

    assert_ne!(first, second);
}

/// Issue #547 V1/V5: nothing outside the path participates in the derivation.
///
/// Because the state path is the sole input, renaming the machine and running
/// elevated versus unelevated cannot move the namespace.
#[test]
fn namespace_is_a_pure_function_of_the_state_path() {
    let path = Path::new(r"C:\Users\dev\AppData\Local\jefe\state.json");

    assert_eq!(
        namespace_for_state_path(path),
        namespace_for_state_path(path)
    );
}

/// The derived namespace stays private and wire-safe for `psmux -L`.
#[test]
fn state_path_namespace_is_private_and_wire_safe() {
    let namespace =
        namespace_for_state_path(Path::new(r"C:\Users\alice\AppData\Local\jefe\state.json"));

    assert!(namespace.starts_with("jefe-"));
    assert!(
        namespace
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    );
    assert!(!namespace.contains("alice"));
}

/// Isolated runs stay inside the installation's namespace prefix.
#[test]
fn unique_state_path_namespace_extends_the_stable_one() {
    let path = Path::new(r"C:\Users\dev\AppData\Local\jefe\state.json");
    let stable = namespace_for_state_path(path);

    let first = unique_namespace_for_state_path(path);
    let second = unique_namespace_for_state_path(path);

    assert_ne!(first, second);
    assert!(first.starts_with(&stable));
    assert!(second.starts_with(&stable));
}
