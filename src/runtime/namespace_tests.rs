//! Behavioral contracts for installation identity (issue #547).

use super::namespace::{
    InstallationHistory, InstallationId, InstallationIdentity, NamespaceDrift, NamespaceError,
    NamespaceOrigin,
};
use std::path::{Path, PathBuf};

fn state_path(raw: &str) -> PathBuf {
    PathBuf::from(raw)
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
///
/// The traversal case alone does not prove this: it is refused on the leading
/// `.` before any separator is examined. Both separator styles are asserted
/// directly so the rule this test is named for is the rule it enforces.
#[test]
fn an_override_with_path_separators_is_refused() {
    assert_eq!(
        InstallationId::from_override("../escape").err(),
        Some(NamespaceError::IllegalCharacter { character: '.' })
    );
    assert_eq!(
        InstallationId::from_override("foo/bar").err(),
        Some(NamespaceError::IllegalCharacter { character: '/' })
    );
    let backslash = r"\".chars().next().unwrap_or_default();
    assert_eq!(
        InstallationId::from_override(r"foo\bar").err(),
        Some(NamespaceError::IllegalCharacter {
            character: backslash
        })
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

/// V2: a genuinely new installation has nothing to strand, so it stays quiet.
#[test]
fn a_brand_new_installation_reports_no_drift() {
    let active =
        InstallationId::for_state_path(Path::new("/home/dev/.local/state/jefe/state.json"));

    assert_eq!(
        NamespaceDrift::assess(None, &active, InstallationHistory::New),
        NamespaceDrift::FirstRun
    );
}

/// V2: an installation that already has state but no recorded namespace was
/// created by an older build, whose namespace this build cannot reproduce.
///
/// This is the case that stranded live agents: the sessions exist, but the
/// namespace they were started under is gone with the build that computed it.
/// Reporting it is the entire point of recording the namespace at all.
#[test]
fn a_preexisting_installation_without_a_record_is_reported_as_possibly_stranded() {
    let active =
        InstallationId::for_state_path(Path::new("/home/dev/.local/state/jefe/state.json"));

    assert_eq!(
        NamespaceDrift::assess(None, &active, InstallationHistory::Preexisting),
        NamespaceDrift::PreviousNamespaceUnknown
    );
}

/// V2: the steady state is silent.
#[test]
fn a_matching_record_reports_no_drift() {
    let active =
        InstallationId::for_state_path(Path::new("/home/dev/.local/state/jefe/state.json"));

    assert_eq!(
        NamespaceDrift::assess(
            Some(active.as_str()),
            &active,
            InstallationHistory::Preexisting
        ),
        NamespaceDrift::Stable
    );
}

/// V2: a changed namespace must name the one that was left behind.
///
/// "Namespace changed" without the previous value is unactionable: recovering
/// the stranded sessions requires knowing which server to look on.
#[test]
fn a_changed_namespace_reports_the_one_that_was_left_behind() {
    let active =
        InstallationId::for_state_path(Path::new("/home/dev/.local/state/jefe/state.json"));

    let drift = NamespaceDrift::assess(
        Some("jefe-76134a0ba22f56e9"),
        &active,
        InstallationHistory::Preexisting,
    );

    assert_eq!(
        drift,
        NamespaceDrift::Changed {
            previous: "jefe-76134a0ba22f56e9".to_owned()
        }
    );
}

/// V2: "changed" and "never recorded" are different problems and must not be
/// collapsed, because only one of them can name a recovery target.
#[test]
fn an_unknown_previous_namespace_is_distinguishable_from_a_changed_one() {
    let active =
        InstallationId::for_state_path(Path::new("/home/dev/.local/state/jefe/state.json"));

    let unknown = NamespaceDrift::assess(None, &active, InstallationHistory::Preexisting);
    let changed =
        NamespaceDrift::assess(Some("jefe-old"), &active, InstallationHistory::Preexisting);

    assert_ne!(unknown, changed);
    assert!(unknown.is_actionable());
    assert!(changed.is_actionable());
    assert!(!NamespaceDrift::assess(None, &active, InstallationHistory::New).is_actionable());
    assert!(
        !NamespaceDrift::assess(
            Some(active.as_str()),
            &active,
            InstallationHistory::Preexisting
        )
        .is_actionable()
    );
}

/// The same directory spelled with redundant `.` and `..` components is the
/// same installation. Callers that reach the derivation without going through
/// `persistence::paths::resolve` — notably the environment fallback — would
/// otherwise hash two spellings of one location to two namespaces, which is the
/// exact way sessions get stranded that this identity exists to prevent.
#[test]
fn redundant_path_components_do_not_change_the_installation() {
    let plain = InstallationId::for_state_path(Path::new("/home/dev/state/jefe/state.json"));
    let dotted = InstallationId::for_state_path(Path::new("/home/dev/state/./jefe/state.json"));
    let backtracked =
        InstallationId::for_state_path(Path::new("/home/dev/cache/../state/jefe/state.json"));

    assert_eq!(
        plain, dotted,
        "a `.` component is spelling, not a different installation"
    );
    assert_eq!(
        plain, backtracked,
        "a `..` component that resolves to the same directory is the same installation"
    );
}

/// A leading `..` has nothing to pop, so it must survive rather than being
/// silently discarded — dropping it would fold genuinely different relative
/// locations onto one namespace.
#[test]
fn a_leading_parent_component_is_not_swallowed() {
    let parent = InstallationId::for_state_path(Path::new("../state/jefe/state.json"));
    let here = InstallationId::for_state_path(Path::new("state/jefe/state.json"));

    assert_ne!(
        parent, here,
        "`../state` and `state` are different locations and must stay distinct"
    );
}

/// Case folding is a Windows filesystem property, not a universal one. On a
/// case-sensitive filesystem two paths differing only in case are two different
/// installations, and folding them together would put both of them on one
/// multiplexer server — the cross-installation collision this issue removed.
#[cfg(unix)]
#[test]
fn case_distinct_paths_are_distinct_installations_on_case_sensitive_systems() {
    let lower = InstallationId::for_state_path(Path::new("/home/dev/jefe/state.json"));
    let upper = InstallationId::for_state_path(Path::new("/home/dev/Jefe/state.json"));

    assert_ne!(
        lower, upper,
        "case-sensitive filesystems must not share one namespace across two directories"
    );
}

/// On Windows the same directory is routinely spelled with different casing —
/// `%LOCALAPPDATA%` alone varies between processes — so casing there really is
/// spelling and must not move the namespace.
#[cfg(windows)]
#[test]
fn case_differences_do_not_change_the_installation_on_windows() {
    let lower = InstallationId::for_state_path(Path::new(r"c:\users\dev\jefe\state.json"));
    let upper = InstallationId::for_state_path(Path::new(r"C:\Users\Dev\Jefe\state.json"));

    assert_eq!(
        lower, upper,
        "casing drift on a case-insensitive filesystem must not restart the namespace"
    );
}
