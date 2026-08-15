//! Exact selected-package outcomes for the workbench candidate (CWR1-00).
//!
//! Each test proves one row of strict selection: which single installed
//! package an active owner owns, and which unresolvable selections are fatal
//! before anything is published. The candidate is composed end to end so the
//! outcome is observed exactly as a consumer will see it.

use super::support::{
    PackageSpec, config_root, host_binaries, plugins_root, provider_relative, publish_settings,
    resolve_paths, scan_roots, selected_owner, selection_toml, stage, stage_config,
};
use jefe::persistence::plugin_inventory::{MANIFEST_FILE_NAME, PluginInventory, UnavailableReason};
use jefe::startup_selection::SelectionRefused;

/// An active pin resolves to exactly that installed version, and the provider
/// descriptor composed for it names that version's directory (CWR1-00).
#[test]
fn an_enabled_pin_selects_exactly_that_installed_version() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let v1 = PackageSpec::persistent_actions("vendor.pinned");
    let v2 = PackageSpec {
        version: "2.0.0",
        ..PackageSpec::persistent_actions("vendor.pinned")
    };
    let inventory = stage_config(
        temp.path(),
        &[(&v1, &host_binaries()), (&v2, &host_binaries())],
    );
    let paths = resolve_paths(&config_root(temp.path()));
    let settings = publish_settings(&inventory, &selection_toml("vendor.pinned", Some("1.0.0")));

    let candidate = super::support::build(&paths, &inventory, &settings, temp.path())
        .unwrap_or_else(|error| panic!("an exact pin must build: {error}"));

    let owner = selected_owner(&candidate, "vendor.pinned");
    assert_eq!(
        owner.package().coordinate().version().as_str(),
        "1.0.0",
        "the pin must select exactly version 1.0.0"
    );
    let action = jefe::domain::action_registry::ActionId::parse("vendor.pinned.run")
        .unwrap_or_else(|error| panic!("action id must parse: {error:?}"));
    let descriptor = candidate
        .providers()
        .catalog()
        .get(&action)
        .unwrap_or_else(|| panic!("the selected package's provider must be composed"));
    assert!(
        descriptor
            .binary
            .ends_with(format!("vendor.pinned/1.0.0/{}", provider_relative())),
        "composition must use only the exact selected package, got {}",
        descriptor.binary.display()
    );
}

/// Without a pin, one owner resolves to the highest installed valid version,
/// deterministically from the single retained inventory.
#[test]
fn an_unpinned_enabled_owner_selects_the_highest_installed_version() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let v1 = PackageSpec::persistent_actions("vendor.unpinned");
    let v2 = PackageSpec {
        version: "2.0.0",
        ..PackageSpec::persistent_actions("vendor.unpinned")
    };
    let inventory = stage_config(
        temp.path(),
        &[(&v1, &host_binaries()), (&v2, &host_binaries())],
    );
    let paths = resolve_paths(&config_root(temp.path()));
    let settings = publish_settings(&inventory, &selection_toml("vendor.unpinned", None));

    let candidate = super::support::build(&paths, &inventory, &settings, temp.path())
        .unwrap_or_else(|error| panic!("an unpinned owner must build: {error}"));

    let owner = selected_owner(&candidate, "vendor.unpinned");
    assert_eq!(
        owner.package().coordinate().version().as_str(),
        "2.0.0",
        "an unpinned owner must resolve to the highest installed version"
    );
}

/// An unpinned owner whose highest discovered coordinate is unusable refuses
/// the candidate: selection never falls back past a broken higher version to
/// a lower usable one (CWR1-00).
#[test]
fn an_unpinned_owner_with_a_higher_unavailable_version_refuses_the_candidate() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = plugins_root(&config_root(temp.path()));
    let good = PackageSpec::persistent_actions("vendor.highest");
    stage(&root, &good, &host_binaries());
    let bad = PackageSpec {
        version: "2.0.0",
        ..PackageSpec::persistent_actions("vendor.highest")
    };
    let bad_dir = root.join(bad.id).join(bad.version);
    std::fs::create_dir_all(&bad_dir)
        .unwrap_or_else(|error| panic!("staging must succeed: {error}"));
    std::fs::write(bad_dir.join(MANIFEST_FILE_NAME), b"{ not a manifest")
        .unwrap_or_else(|error| panic!("bad manifest must write: {error}"));
    let inventory = scan_roots(&[root]);
    let paths = resolve_paths(&config_root(temp.path()));
    let settings = publish_settings(&inventory, &selection_toml("vendor.highest", None));

    let refusal = super::support::expect_selection_refusal(super::support::build(
        &paths,
        &inventory,
        &settings,
        temp.path(),
    ));

    match &refusal {
        SelectionRefused::Unavailable { owner, reason } => {
            assert_eq!(owner.as_str(), "vendor.highest");
            assert!(
                matches!(reason, UnavailableReason::InvalidManifest { .. }),
                "the refusal must be for the broken 2.0.0 package, not the valid 1.0.0"
            );
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

/// An unpinned owner whose highest discovered coordinate is contested by two
/// physical packages refuses the candidate instead of silently selecting the
/// lower valid version.
#[test]
fn an_unpinned_owner_with_a_higher_ambiguous_coordinate_refuses_the_candidate() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root_a = temp.path().join("packages-a");
    let root_b = temp.path().join("packages-b");
    let good = PackageSpec::persistent_actions("vendor.contested");
    stage(&root_a, &good, &host_binaries());
    let contested = PackageSpec {
        version: "2.0.0",
        ..PackageSpec::persistent_actions("vendor.contested")
    };
    stage(&root_a, &contested, &host_binaries());
    stage(&root_b, &contested, &host_binaries());
    let inventory = scan_roots(&[root_a, root_b]);
    let paths = resolve_paths(&config_root(temp.path()));
    let settings = publish_settings(&inventory, &selection_toml("vendor.contested", None));

    let refusal = super::support::expect_selection_refusal(super::support::build(
        &paths,
        &inventory,
        &settings,
        temp.path(),
    ));

    match &refusal {
        SelectionRefused::Ambiguous {
            owner,
            paths: claimants,
        } => {
            assert_eq!(owner.as_str(), "vendor.contested");
            assert_eq!(claimants.len(), 2, "both physical claimants must be named");
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
}

/// A lower unusable coordinate never hides a higher valid one: the winner is
/// the highest discovered coordinate, and it selects when uniquely valid.
#[test]
fn an_unpinned_owner_selects_a_higher_valid_version_above_a_lower_unavailable_one() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = plugins_root(&config_root(temp.path()));
    let bad = PackageSpec::persistent_actions("vendor.rising");
    let bad_dir = root.join(bad.id).join(bad.version);
    std::fs::create_dir_all(&bad_dir)
        .unwrap_or_else(|error| panic!("staging must succeed: {error}"));
    std::fs::write(bad_dir.join(MANIFEST_FILE_NAME), b"{ not a manifest")
        .unwrap_or_else(|error| panic!("bad manifest must write: {error}"));
    let good = PackageSpec {
        version: "2.0.0",
        ..PackageSpec::persistent_actions("vendor.rising")
    };
    stage(&root, &good, &host_binaries());
    let inventory = scan_roots(&[root]);
    let paths = resolve_paths(&config_root(temp.path()));
    let settings = publish_settings(&inventory, &selection_toml("vendor.rising", None));

    let candidate = super::support::build(&paths, &inventory, &settings, temp.path())
        .unwrap_or_else(|error| panic!("a uniquely valid highest version must build: {error}"));

    let owner = selected_owner(&candidate, "vendor.rising");
    assert_eq!(
        owner.package().coordinate().version().as_str(),
        "2.0.0",
        "the higher valid version must win over the lower unusable one"
    );
}

/// A pinned version that no installed package provides exactly is fatal
/// before publication: the operator named a specific program, and starting a
/// different one would be a different workbench (CWR1-00).
#[test]
fn a_pinned_version_that_is_not_installed_refuses_the_candidate() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let spec = PackageSpec::persistent_actions("vendor.missing");
    let inventory = stage_config(temp.path(), &[(&spec, &host_binaries())]);
    let paths = resolve_paths(&config_root(temp.path()));
    let settings = publish_settings(&inventory, &selection_toml("vendor.missing", Some("9.9.9")));

    let refusal = super::support::expect_selection_refusal(super::support::build(
        &paths,
        &inventory,
        &settings,
        temp.path(),
    ));

    match &refusal {
        SelectionRefused::Missing { owner, version } => {
            assert_eq!(owner.as_str(), "vendor.missing");
            assert_eq!(
                version
                    .as_deref()
                    .map(jefe::domain::CanonicalSemver::as_str),
                Some("9.9.9")
            );
        }
        other => panic!("expected Missing, got {other:?}"),
    }
}

/// A disabled owner is not an active selection: its pin never has to resolve,
/// and nothing it declares is required (decision 4).
#[test]
fn a_disabled_owner_whose_pin_is_not_installed_is_not_active() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let spec = PackageSpec::persistent_actions("vendor.off");
    let inventory = stage_config(temp.path(), &[(&spec, &host_binaries())]);
    let paths = resolve_paths(&config_root(temp.path()));
    let settings = publish_settings(
        &inventory,
        "settings_schema = 2\n\n[plugins.\"vendor.off\"]\nenabled = false\nversion = \"9.9.9\"\n",
    );

    let candidate = super::support::build(&paths, &inventory, &settings, temp.path())
        .unwrap_or_else(|error| panic!("a disabled owner must not block: {error}"));

    assert!(
        candidate
            .selected_owners()
            .iter()
            .all(|owner| owner.owner().as_str() != "vendor.off"),
        "a disabled owner must select nothing"
    );
}

/// A pinned package that exists but cannot be classified (unusable manifest)
/// is fatal when actively selected, not silently skipped.
#[test]
fn an_unavailable_pinned_package_refuses_the_candidate() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = plugins_root(&config_root(temp.path()));
    let good = PackageSpec::persistent_actions("vendor.broken");
    stage(&root, &good, &host_binaries());
    let bad = PackageSpec {
        version: "2.0.0",
        ..PackageSpec::persistent_actions("vendor.broken")
    };
    let bad_dir = root.join(bad.id).join(bad.version);
    std::fs::create_dir_all(&bad_dir)
        .unwrap_or_else(|error| panic!("staging must succeed: {error}"));
    std::fs::write(bad_dir.join(MANIFEST_FILE_NAME), b"{ not a manifest")
        .unwrap_or_else(|error| panic!("bad manifest must write: {error}"));
    let inventory = scan_roots(&[root]);
    let paths = resolve_paths(&config_root(temp.path()));
    let settings = publish_settings(&inventory, &selection_toml("vendor.broken", Some("2.0.0")));

    let refusal = super::support::expect_selection_refusal(super::support::build(
        &paths,
        &inventory,
        &settings,
        temp.path(),
    ));

    match &refusal {
        SelectionRefused::Unavailable { owner, .. } => {
            assert_eq!(owner.as_str(), "vendor.broken");
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

/// Two physically distinct packages claiming one pinned coordinate are
/// ambiguous, and an active selection on that coordinate is fatal rather than
/// a coin flip between two programs.
#[test]
fn an_ambiguous_pinned_coordinate_refuses_the_candidate() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root_a = temp.path().join("packages-a");
    let root_b = temp.path().join("packages-b");
    let good = PackageSpec::persistent_actions("vendor.two");
    stage(&root_a, &good, &host_binaries());
    let contested = PackageSpec {
        version: "2.0.0",
        ..PackageSpec::persistent_actions("vendor.two")
    };
    stage(&root_a, &contested, &host_binaries());
    stage(&root_b, &contested, &host_binaries());
    let inventory = scan_roots(&[root_a, root_b]);
    let paths = resolve_paths(&config_root(temp.path()));
    let settings = publish_settings(&inventory, &selection_toml("vendor.two", Some("2.0.0")));

    let refusal = super::support::expect_selection_refusal(super::support::build(
        &paths,
        &inventory,
        &settings,
        temp.path(),
    ));

    match &refusal {
        SelectionRefused::Ambiguous {
            owner,
            paths: claimants,
        } => {
            assert_eq!(owner.as_str(), "vendor.two");
            assert_eq!(claimants.len(), 2, "both physical claimants must be named");
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
}

/// A pure classification check on the exact-selection seam itself: dormant
/// owners (installed but unselected) never appear.
#[test]
fn selection_distinguishes_active_from_dormant_owners() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let on = PackageSpec::one_shot("vendor.on");
    let off = PackageSpec::one_shot("vendor.off");
    let inventory = stage_config(
        temp.path(),
        &[(&on, &host_binaries()), (&off, &host_binaries())],
    );
    let settings = publish_settings(
        &inventory,
        "settings_schema = 2\n\n[plugins.\"vendor.on\"]\nenabled = true\n",
    );

    let selected = jefe::startup_selection::select_exactly(&inventory, &settings)
        .unwrap_or_else(|error| panic!("selection must resolve: {error}"));

    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].owner().as_str(), "vendor.on");
}

/// The staged fixture helper must agree with itself: host staging produces a
/// selectable binary and the installed package exposes its version text.
#[test]
fn staged_packages_expose_their_version_text() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let spec = PackageSpec::one_shot("vendor.helper");
    let inventory = stage_config(temp.path(), &[(&spec, &host_binaries())]);
    let packages = inventory.packages();
    let package: &jefe::persistence::plugin_inventory::InstalledPackage = &packages[0];
    assert_eq!(package.coordinate().version().as_str(), "1.0.0");
    assert!(PluginInventory::default().packages().is_empty());
}
