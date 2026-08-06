//! Behavioral contracts for installation identity (issue #547).

use super::namespace::{InstallationId, InstallationIdentity, NamespaceError, NamespaceOrigin};
use std::path::{Path, PathBuf};

fn state_path(raw: &str) -> PathBuf {
    PathBuf::from(raw)
}

/// V3: the identity must survive path casing differences.
///
/// Windows paths are case-insensitive, so `%LOCALAPPDATA%` arriving with
/// different casing between launches must not move the namespace and orphan
/// every running session.
#[test]
fn identity_ignores_state_path_casing() {
    let lower =
        InstallationId::for_state_path(Path::new(r"c:\users\dev\appdata\local\jefe\state.json"));
    let upper =
        InstallationId::for_state_path(Path::new(r"C:\Users\Dev\AppData\Local\Jefe\State.json"));

    assert_eq!(lower, upper);
}

/// The same location spelled with either separator is the same installation.
#[test]
fn identity_ignores_state_path_separator_style() {
    let backslash = InstallationId::for_state_path(Path::new(r"C:\Users\dev\jefe\state.json"));
    let forward = InstallationId::for_state_path(Path::new("C:/Users/dev/jefe/state.json"));

    assert_eq!(backslash, forward);
}

/// A trailing separator is spelling, not identity.
#[test]
fn identity_ignores_trailing_state_path_separator() {
    let bare = InstallationId::for_state_path(Path::new(r"C:\Users\dev\jefe"));
    let trailing = InstallationId::for_state_path(Path::new(r"C:\Users\dev\jefe\"));

    assert_eq!(bare, trailing);
}

/// V4: genuinely separate users keep separate session pools.
///
/// Distinct accounts resolve distinct home roots, so state-path keying
/// preserves user isolation structurally rather than by special case.
#[test]
fn identity_separates_distinct_user_state_paths() {
    let alice =
        InstallationId::for_state_path(Path::new(r"C:\Users\alice\AppData\Local\jefe\state.json"));
    let bob =
        InstallationId::for_state_path(Path::new(r"C:\Users\bob\AppData\Local\jefe\state.json"));

    assert_ne!(alice, bob);
}

/// Separate installations for one user stay separate.
///
/// This is the worktree case: two checkouts launched with different `--config`
/// roots must never share a multiplexer server, on any platform.
#[test]
fn identity_separates_distinct_installations_for_one_user() {
    let first = InstallationId::for_state_path(Path::new(r"C:\work\tree-one\.jefe\state.json"));
    let second = InstallationId::for_state_path(Path::new(r"C:\work\tree-two\.jefe\state.json"));

    assert_ne!(first, second);
}

/// V1/V5: nothing outside the path participates in the derivation.
///
/// Because the state path is the sole input, renaming the machine and running
/// elevated versus unelevated cannot move the namespace.
#[test]
fn identity_is_a_pure_function_of_the_state_path() {
    let path = Path::new(r"C:\Users\dev\AppData\Local\jefe\state.json");

    assert_eq!(
        InstallationId::for_state_path(path),
        InstallationId::for_state_path(path)
    );
}

/// The identity stays private and safe as both a server name and a file name.
#[test]
fn identity_is_private_and_wire_safe() {
    let identity =
        InstallationId::for_state_path(Path::new(r"C:\Users\alice\AppData\Local\jefe\state.json"));

    assert!(identity.as_str().starts_with("jefe-"));
    assert!(
        identity
            .as_str()
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    );
    assert!(!identity.as_str().contains("alice"));
}

/// Isolated runs stay inside the installation's identity prefix.
#[test]
fn isolated_runs_extend_the_stable_identity() {
    let path = Path::new(r"C:\Users\dev\AppData\Local\jefe\state.json");
    let stable = InstallationId::for_state_path(path);

    let first = InstallationId::unique_for_state_path(path);
    let second = InstallationId::unique_for_state_path(path);

    assert_ne!(first, second);
    assert!(first.as_str().starts_with(stable.as_str()));
    assert!(second.as_str().starts_with(stable.as_str()));
}

/// An isolated run is still safe as a file name.
#[test]
fn isolated_run_identity_is_wire_safe() {
    let identity =
        InstallationId::unique_for_state_path(Path::new(r"C:\Users\dev\jefe\state.json"));

    assert!(
        identity
            .as_str()
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    );
}

/// V11: an explicit override is honored verbatim.
///
/// This is the deliberate isolation mechanism, so the operator gets exactly the
/// namespace they asked for rather than a hash of it.
#[test]
fn an_explicit_override_is_honored_verbatim() {
    assert_eq!(
        InstallationId::from_override("psmux-ab-test").map(|id| id.as_str().to_owned()),
        Ok("psmux-ab-test".to_owned())
    );
}

/// Surrounding whitespace is shell noise, not part of the name.
#[test]
fn an_explicit_override_is_trimmed() {
    assert_eq!(
        InstallationId::from_override("  spaced  ").map(|id| id.as_str().to_owned()),
        Ok("spaced".to_owned())
    );
}

/// An unusable override is refused rather than silently ignored.
///
/// Falling back to the derived namespace would attach the operator to the very
/// sessions they asked to be separated from.
#[test]
fn an_empty_override_is_refused() {
    assert_eq!(
        InstallationId::from_override("   ").err(),
        Some(NamespaceError::Empty)
    );
}

#[test]
fn an_overlong_override_is_refused() {
    let raw = "a".repeat(65);

    assert_eq!(
        InstallationId::from_override(&raw).err(),
        Some(NamespaceError::TooLong { length: 65 })
    );
}

/// Path separators in an override would escape the socket directory on Unix.
#[test]
fn an_override_with_path_separators_is_refused() {
    assert_eq!(
        InstallationId::from_override("../escape").err(),
        Some(NamespaceError::IllegalCharacter { character: '.' })
    );
}

#[test]
fn an_override_with_whitespace_inside_is_refused() {
    assert_eq!(
        InstallationId::from_override("two words").err(),
        Some(NamespaceError::IllegalCharacter { character: ' ' })
    );
}

/// The identity carries where it came from, so doctor can explain it.
#[test]
fn a_derived_identity_reports_the_state_path_it_came_from() {
    let path = state_path(r"C:\Users\dev\jefe\state.json");
    let identity = InstallationIdentity::for_state_path(&path);

    assert_eq!(identity.origin().state_path(), Some(path.as_path()));
    assert!(!identity.origin().is_override());
    assert_eq!(
        identity.id(),
        &InstallationId::for_state_path(path.as_path())
    );
}

/// An override reports itself as deliberate, so it can be surfaced loudly.
#[test]
fn an_override_identity_reports_itself_as_deliberate() {
    let identity = InstallationIdentity::from_override("ab-test")
        .unwrap_or_else(|error| panic!("a plain override should be accepted: {error}"));

    assert!(identity.origin().is_override());
    assert_eq!(identity.origin().state_path(), None);
    assert_eq!(
        identity.origin(),
        &NamespaceOrigin::Override("ab-test".to_owned())
    );
}

/// An isolated run is distinguishable from the installation's real namespace.
#[test]
fn an_isolated_run_identity_is_distinguishable_from_the_stable_one() {
    let path = state_path(r"C:\Users\dev\jefe\state.json");
    let isolated = InstallationIdentity::isolated_run(&path);

    assert_eq!(
        isolated.origin(),
        &NamespaceOrigin::IsolatedRun(path.clone())
    );
    assert_ne!(
        isolated.id(),
        &InstallationId::for_state_path(path.as_path())
    );
}

/// Provenance is human-readable, because it exists to be shown to an operator
/// asking why their sessions are not where they expected.
#[test]
fn provenance_renders_for_display() {
    let derived = InstallationIdentity::for_state_path(Path::new(r"C:\Users\dev\jefe\state.json"));
    let rendered = derived.to_string();

    assert!(rendered.contains(derived.id().as_str()));
    assert!(rendered.contains("state.json"));
}
