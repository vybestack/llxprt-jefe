//! Behavioral contracts for namespace recording and drift reporting (#547 V2).

use super::namespace::{InstallationId, InstallationIdentity, NamespaceDrift, NamespaceOrigin};
use super::namespace_record::{describe, reconcile};
use std::path::{Path, PathBuf};

fn temp_state_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "jefe-namespace-record-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("temp state dir must be creatable: {error}"));
    dir
}

fn derived(state_path: &Path) -> InstallationIdentity {
    InstallationIdentity::for_state_path(state_path)
}

/// A fresh installation records its namespace and says nothing alarming.
#[test]
fn a_first_launch_records_the_namespace_without_raising_an_alarm() {
    let dir = temp_state_dir("first");
    let state_path = dir.join("state.json");
    let identity = derived(&state_path);

    let drift = reconcile(&state_path, identity.origin(), identity.id());

    assert_eq!(drift, NamespaceDrift::FirstRun);
    assert!(
        describe(&drift, identity.id()).is_none(),
        "a brand new installation has nothing to warn about"
    );
    assert!(
        dir.join("runtime-namespace.json").exists(),
        "the namespace must be recorded so the next launch can compare"
    );
}

/// The recorded namespace is what makes the second launch quiet.
#[test]
fn a_second_launch_of_the_same_installation_is_stable() {
    let dir = temp_state_dir("stable");
    let state_path = dir.join("state.json");
    let identity = derived(&state_path);

    let _ = reconcile(&state_path, identity.origin(), identity.id());
    let drift = reconcile(&state_path, identity.origin(), identity.id());

    assert_eq!(drift, NamespaceDrift::Stable);
    assert!(describe(&drift, identity.id()).is_none());
}

/// State that predates the record means a previous build owned this
/// installation, and its sessions are somewhere this build cannot name.
#[test]
fn existing_state_without_a_record_is_reported_as_a_lost_previous_namespace() {
    let dir = temp_state_dir("upgraded");
    let state_path = dir.join("state.json");
    std::fs::write(&state_path, "{}")
        .unwrap_or_else(|error| panic!("state file must be writable: {error}"));
    let identity = derived(&state_path);

    let drift = reconcile(&state_path, identity.origin(), identity.id());

    assert_eq!(drift, NamespaceDrift::PreviousNamespaceUnknown);
    let report = describe(&drift, identity.id())
        .unwrap_or_else(|| panic!("a possibly stranded installation must be reported"));
    assert!(
        report.contains(identity.id().as_str()),
        "the operator must be told which namespace is now in force, got: {report}"
    );
}

/// A changed namespace must name the namespace that was left behind, because
/// that name is the only way back to the sessions still running on it.
#[test]
fn a_changed_namespace_is_reported_with_the_namespace_left_behind() {
    let dir = temp_state_dir("changed");
    let state_path = dir.join("state.json");
    std::fs::write(&state_path, "{}")
        .unwrap_or_else(|error| panic!("state file must be writable: {error}"));
    std::fs::write(
        dir.join("runtime-namespace.json"),
        r#"{"namespace":"jefe-76134a0ba22f56e9","state_path":"whatever"}"#,
    )
    .unwrap_or_else(|error| panic!("record must be writable: {error}"));
    let identity = derived(&state_path);

    let drift = reconcile(&state_path, identity.origin(), identity.id());

    assert_eq!(
        drift,
        NamespaceDrift::Changed {
            previous: "jefe-76134a0ba22f56e9".to_owned()
        }
    );
    let report = describe(&drift, identity.id())
        .unwrap_or_else(|| panic!("a namespace change must be reported"));
    assert!(
        report.contains("jefe-76134a0ba22f56e9"),
        "the abandoned namespace must be named, got: {report}"
    );
}

/// Having reported a change once, the new namespace becomes the baseline.
#[test]
fn a_reported_change_is_not_reported_again_on_the_next_launch() {
    let dir = temp_state_dir("rerecord");
    let state_path = dir.join("state.json");
    std::fs::write(&state_path, "{}")
        .unwrap_or_else(|error| panic!("state file must be writable: {error}"));
    std::fs::write(
        dir.join("runtime-namespace.json"),
        r#"{"namespace":"jefe-old","state_path":"whatever"}"#,
    )
    .unwrap_or_else(|error| panic!("record must be writable: {error}"));
    let identity = derived(&state_path);

    let first = reconcile(&state_path, identity.origin(), identity.id());
    let second = reconcile(&state_path, identity.origin(), identity.id());

    assert!(matches!(first, NamespaceDrift::Changed { .. }));
    assert_eq!(
        second,
        NamespaceDrift::Stable,
        "re-reporting a change the operator has already been told about is noise"
    );
}

/// A deliberate override is temporary isolation. Recording it would make the
/// operator's next ordinary launch look like drift.
#[test]
fn an_override_is_never_recorded_against_the_installation() {
    let dir = temp_state_dir("override");
    let state_path = dir.join("state.json");
    std::fs::write(&state_path, "{}")
        .unwrap_or_else(|error| panic!("state file must be writable: {error}"));
    let overridden = InstallationIdentity::from_override("ab-testing")
        .unwrap_or_else(|error| panic!("a plain override should be accepted: {error}"));

    let drift = reconcile(&state_path, overridden.origin(), overridden.id());

    assert_eq!(drift, NamespaceDrift::Stable);
    assert!(
        !dir.join("runtime-namespace.json").exists(),
        "an override must not become this installation's remembered namespace"
    );
}

/// A corrupt record cannot name the previous namespace, which is precisely the
/// "unknown previous namespace" case rather than a crash or a silent pass.
#[test]
fn a_corrupt_record_degrades_to_an_unknown_previous_namespace() {
    let dir = temp_state_dir("corrupt");
    let state_path = dir.join("state.json");
    std::fs::write(&state_path, "{}")
        .unwrap_or_else(|error| panic!("state file must be writable: {error}"));
    std::fs::write(dir.join("runtime-namespace.json"), "{ truncated")
        .unwrap_or_else(|error| panic!("record must be writable: {error}"));
    let identity = derived(&state_path);

    let drift = reconcile(&state_path, identity.origin(), identity.id());

    assert_eq!(drift, NamespaceDrift::PreviousNamespaceUnknown);
}

/// The record is scoped to its installation: a second config directory keeps
/// its own, which is what makes two worktrees independent rather than rivals.
#[test]
fn each_installation_records_its_own_namespace() {
    let first_dir = temp_state_dir("scoped-a");
    let second_dir = temp_state_dir("scoped-b");
    let first_state = first_dir.join("state.json");
    let second_state = second_dir.join("state.json");
    let first = derived(&first_state);
    let second = derived(&second_state);

    let _ = reconcile(&first_state, first.origin(), first.id());
    let _ = reconcile(&second_state, second.origin(), second.id());

    assert_ne!(
        first.id(),
        second.id(),
        "distinct installations must not share a namespace"
    );
    assert_eq!(
        reconcile(&first_state, first.origin(), first.id()),
        NamespaceDrift::Stable,
        "recording a sibling installation must not disturb this one"
    );
}

/// Guard the reducer's own inputs: reconciliation is keyed on the identity it
/// is handed, not on ambient process state.
#[test]
fn reconciliation_compares_against_the_identity_it_is_given() {
    let dir = temp_state_dir("explicit");
    let state_path = dir.join("state.json");
    std::fs::write(&state_path, "{}")
        .unwrap_or_else(|error| panic!("state file must be writable: {error}"));
    let identity = derived(&state_path);
    let _ = reconcile(&state_path, identity.origin(), identity.id());

    let unrelated = InstallationId::for_state_path(Path::new("/somewhere/else/state.json"));
    let drift = reconcile(
        &state_path,
        &NamespaceOrigin::StatePath(state_path.clone()),
        &unrelated,
    );

    assert_eq!(
        drift,
        NamespaceDrift::Changed {
            previous: identity.id().as_str().to_owned()
        }
    );
}
