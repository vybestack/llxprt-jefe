//! Physical-inventory table (issue #389 CW-09, acceptance rows R4–R8).

use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::domain::plugin::HostTriple;
use crate::domain::plugin::limits::MANIFEST_BYTE_LIMIT;
use crate::persistence::plugin_roots::{PluginRoot, PluginRootKind};

/// A minimal valid manifest for `id` at `version`.
fn manifest_json(id: &str, version: &str) -> String {
    format!(
        r#"{{
          "manifest_schema": 1,
          "id": "{id}",
          "version": "{version}",
          "display_name": "Package {id}",
          "host_api": {{ "minimum": "1.0.0", "maximum": "1.0.0" }},
          "protocol": 1,
          "provider": {{ "mode": "none", "binaries": {{}} }},
          "actions": [],
          "panels": [],
          "routes": [],
          "screens": []
        }}"#
    )
}

/// Build one package directory `<root>/<id>/<version>/plugin.json`.
fn write_package(root: &Path, id: &str, version: &str) -> PathBuf {
    write_package_with(root, id, version, &manifest_json(id, version))
}

/// Build one package directory carrying exactly `manifest`.
fn write_package_with(root: &Path, id: &str, version: &str, manifest: &str) -> PathBuf {
    let directory = root.join(id).join(version);
    fs::create_dir_all(&directory)
        .unwrap_or_else(|error| panic!("staging {} must succeed: {error}", directory.display()));
    fs::write(directory.join(MANIFEST_FILE_NAME), manifest.as_bytes())
        .unwrap_or_else(|error| panic!("manifest must write: {error}"));
    directory
}

fn read_only_root(path: &Path) -> PluginRoot {
    PluginRoot::new(path.to_path_buf(), PluginRootKind::System)
}

fn user_root(path: &Path) -> PluginRoot {
    PluginRoot::new(path.to_path_buf(), PluginRootKind::User)
}

fn coordinates(inventory: &PluginInventory) -> Vec<String> {
    inventory
        .packages()
        .iter()
        .map(|entry| entry.coordinate().to_string())
        .collect()
}

fn published_settings(
    inventory: &PluginInventory,
    source: &[u8],
) -> crate::persistence::settings_document::PublishedSettings {
    let catalog = crate::config_owners::owner_catalog_with_packages(inventory.packages())
        .unwrap_or_else(|error| panic!("owner catalog must compose: {error}"));
    crate::persistence::settings_document::SettingsDocument::parse(source)
        .unwrap_or_else(|error| panic!("settings must parse: {error:?}"))
        .publish(&catalog)
        .unwrap_or_else(|diagnostics| panic!("settings must publish: {diagnostics:?}"))
}

#[test]
fn selected_packages_use_only_the_enabled_exact_settings_version() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("root");
    write_package(&root, "vendor.pkg", "1.0.0");
    write_package(&root, "vendor.pkg", "2.0.0");
    write_package(&root, "other.pkg", "3.0.0");
    let inventory = scan(&[user_root(&root)]);
    let settings = published_settings(
        &inventory,
        br#"settings_schema = 2

[plugins."vendor.pkg"]
enabled = true
version = "1.0.0"

[plugins."other.pkg"]
enabled = false
version = "3.0.0"
"#,
    );

    let selected: Vec<String> = selected_packages(inventory.packages(), &settings)
        .into_iter()
        .map(|package| package.coordinate().to_string())
        .collect();

    assert_eq!(selected, vec!["vendor.pkg@1.0.0"]);
}

#[test]
fn configured_packages_retain_the_exact_disabled_source_version() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("root");
    write_package(&root, "vendor.pkg", "1.0.0");
    write_package(&root, "vendor.pkg", "2.0.0");
    let inventory = scan(&[user_root(&root)]);
    let settings = published_settings(
        &inventory,
        br#"settings_schema = 2

[plugins."vendor.pkg"]
enabled = false
version = "1.0.0"
"#,
    );

    let configured: Vec<String> = configured_packages(inventory.packages(), &settings)
        .into_iter()
        .map(|package| package.coordinate().to_string())
        .collect();

    assert_eq!(configured, vec!["vendor.pkg@1.0.0"]);
    assert!(selected_packages(inventory.packages(), &settings).is_empty());
}

#[test]
fn selected_packages_default_to_highest_precedence_installed_version() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("root");
    write_package(&root, "vendor.pkg", "1.0.0");
    write_package(&root, "vendor.pkg", "2.0.0-rc.1");
    write_package(&root, "vendor.pkg", "2.0.0");
    let inventory = scan(&[user_root(&root)]);
    let settings = published_settings(
        &inventory,
        br#"settings_schema = 2

[plugins."vendor.pkg"]
enabled = true
"#,
    );

    let selected = selected_packages(inventory.packages(), &settings);

    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].coordinate().to_string(), "vendor.pkg@2.0.0");
}

#[test]
fn lists_every_physical_version_in_exact_root_order() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let low = temp.path().join("low");
    let high = temp.path().join("high");
    write_package(&low, "vendor.pkg", "0.9.0");
    write_package(&high, "vendor.pkg", "1.0.0");
    write_package(&high, "other.pkg", "2.0.0");

    let inventory = scan(&[read_only_root(&low), user_root(&high)]);

    assert_eq!(
        coordinates(&inventory),
        vec!["other.pkg@2.0.0", "vendor.pkg@1.0.0", "vendor.pkg@0.9.0"],
        "listing order is id ascending then precedence descending"
    );
    assert!(inventory.ambiguities().is_empty());
}

#[test]
fn a_missing_root_is_skipped_without_failing_the_scan() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let present = temp.path().join("present");
    write_package(&present, "vendor.pkg", "1.0.0");

    let inventory = scan(&[
        read_only_root(&temp.path().join("absent")),
        user_root(&present),
    ]);

    assert_eq!(coordinates(&inventory), vec!["vendor.pkg@1.0.0"]);
}

#[cfg(unix)]
#[test]
fn the_first_physical_occurrence_wins_and_later_aliases_are_recorded() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let cellar = temp.path().join("Cellar");
    write_package(&cellar, "vendor.pkg", "1.0.0");
    let prefix = temp.path().join("prefix");
    if std::os::unix::fs::symlink(&cellar, &prefix).is_err() {
        return;
    }

    let inventory = scan(&[read_only_root(&cellar), user_root(&prefix)]);

    assert_eq!(
        coordinates(&inventory),
        vec!["vendor.pkg@1.0.0"],
        "one physical package yields exactly one row"
    );
    let entry = inventory
        .packages()
        .first()
        .unwrap_or_else(|| panic!("the package must be listed"));
    assert_eq!(entry.root(), cellar, "the first occurrence wins");
    assert_eq!(
        entry.aliases().len(),
        1,
        "the later alias must be recorded as provenance"
    );
    assert!(inventory.ambiguities().is_empty());
}

#[test]
fn two_physically_distinct_packages_at_one_coordinate_are_ambiguous() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let low = temp.path().join("low");
    let high = temp.path().join("high");
    write_package(&low, "vendor.pkg", "1.0.0");
    write_package(&high, "vendor.pkg", "1.0.0");

    let inventory = scan(&[read_only_root(&low), user_root(&high)]);

    assert!(
        coordinates(&inventory).is_empty(),
        "precedence never resolves the collision, so neither package is selected"
    );
    let ambiguity = inventory
        .ambiguities()
        .first()
        .unwrap_or_else(|| panic!("the collision must be reported"));
    assert_eq!(ambiguity.code(), PluginCode::Ambiguous);
    assert_eq!(ambiguity.coordinate().to_string(), "vendor.pkg@1.0.0");
    assert_eq!(
        ambiguity.paths().len(),
        2,
        "both physical paths must be named"
    );
}

#[test]
fn byte_identical_but_physically_distinct_packages_are_still_ambiguous() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let low = temp.path().join("low");
    let high = temp.path().join("high");
    // Both carry exactly the same manifest bytes, so only their physical
    // location distinguishes them.
    write_package(&low, "vendor.pkg", "1.0.0");
    write_package(&high, "vendor.pkg", "1.0.0");

    let inventory = scan(&[read_only_root(&low), user_root(&high)]);

    assert!(coordinates(&inventory).is_empty());
    assert_eq!(inventory.ambiguities().len(), 1);
}

#[test]
fn an_ambiguity_does_not_block_unrelated_packages() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let low = temp.path().join("low");
    let high = temp.path().join("high");
    write_package(&low, "vendor.pkg", "1.0.0");
    write_package(&high, "vendor.pkg", "1.0.0");
    write_package(&high, "other.pkg", "3.0.0");

    let inventory = scan(&[read_only_root(&low), user_root(&high)]);

    assert_eq!(coordinates(&inventory), vec!["other.pkg@3.0.0"]);
    assert_eq!(inventory.ambiguities().len(), 1);
}

#[test]
fn directories_outside_the_package_layout_are_not_packages() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("root");
    write_package(&root, "vendor.pkg", "1.0.0");
    // Reserved id, single-label id, and non-canonical versions are not
    // packages at all; they are silently not part of the inventory.
    write_package(&root, "core.dashboard", "1.0.0");
    write_package(&root, "vendor", "1.0.0");
    write_package(&root, "vendor.other", "v1.0.0");
    write_package(&root, "vendor.other", "1.0");
    if fs::write(root.join("loose-file"), b"x").is_err() {
        return;
    }

    let inventory = scan(&[user_root(&root)]);

    assert_eq!(coordinates(&inventory), vec!["vendor.pkg@1.0.0"]);
}

#[test]
fn a_well_named_package_without_a_manifest_is_listed_unavailable() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("root");
    write_package(&root, "vendor.pkg", "1.0.0");
    let bare = root.join("vendor.bare").join("1.0.0");
    if fs::create_dir_all(&bare).is_err() {
        return;
    }

    let inventory = scan(&[user_root(&root)]);

    assert_eq!(coordinates(&inventory), vec!["vendor.pkg@1.0.0"]);
    let unavailable = inventory
        .unavailable()
        .first()
        .unwrap_or_else(|| panic!("the manifest-less package must be listed"));
    assert_eq!(unavailable.coordinate().to_string(), "vendor.bare@1.0.0");
    assert_eq!(unavailable.reason(), &UnavailableReason::MissingManifest);
}

#[cfg(unix)]
#[test]
fn a_package_whose_directory_escapes_its_root_is_rejected() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let outside = temp.path().join("outside");
    write_package(&outside, "vendor.escape", "1.0.0");
    let root = temp.path().join("root");
    let owner = root.join("vendor.escape");
    if fs::create_dir_all(&owner).is_err() {
        return;
    }
    if std::os::unix::fs::symlink(
        outside.join("vendor.escape").join("1.0.0"),
        owner.join("1.0.0"),
    )
    .is_err()
    {
        return;
    }

    let inventory = scan(&[user_root(&root)]);

    assert!(
        coordinates(&inventory).is_empty(),
        "a symlink escape must never be selected"
    );
    let unavailable = inventory
        .unavailable()
        .first()
        .unwrap_or_else(|| panic!("the escape must be reported"));
    assert_eq!(unavailable.reason(), &UnavailableReason::EscapesRoot);
}

#[test]
fn a_selected_package_carries_its_parsed_manifest() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("root");
    write_package(&root, "vendor.pkg", "1.0.0");

    let inventory = scan(&[user_root(&root)]);
    let entry = inventory
        .packages()
        .first()
        .unwrap_or_else(|| panic!("the package must be listed"));
    assert_eq!(entry.display_name(), "Package vendor.pkg");
    assert_eq!(entry.manifest().id().as_str(), "vendor.pkg");
}

#[test]
fn a_manifest_that_does_not_parse_is_listed_unavailable() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("root");
    write_package(&root, "vendor.good", "1.0.0");
    // Schema 9 is not a schema this executable reads.
    let broken = manifest_json("vendor.broken", "1.0.0")
        .replace(r#""manifest_schema": 1"#, r#""manifest_schema": 9"#);
    write_package_with(&root, "vendor.broken", "1.0.0", &broken);

    let inventory = scan(&[user_root(&root)]);

    assert_eq!(
        coordinates(&inventory),
        vec!["vendor.good@1.0.0"],
        "a broken package must not block a valid neighbour"
    );
    let unavailable = inventory
        .unavailable()
        .first()
        .unwrap_or_else(|| panic!("the broken package must be listed"));
    assert_eq!(unavailable.coordinate().to_string(), "vendor.broken@1.0.0");
    assert!(
        matches!(
            unavailable.reason(),
            UnavailableReason::InvalidManifest { .. }
        ),
        "expected an invalid-manifest reason, got {:?}",
        unavailable.reason()
    );
}

#[test]
fn a_manifest_whose_identity_contradicts_its_directory_is_rejected() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("root");
    // The directory says 1.0.0; the manifest claims 2.0.0.
    let mismatched = manifest_json("vendor.pkg", "2.0.0");
    write_package_with(&root, "vendor.pkg", "1.0.0", &mismatched);

    let inventory = scan(&[user_root(&root)]);

    assert!(coordinates(&inventory).is_empty());
    let unavailable = inventory
        .unavailable()
        .first()
        .unwrap_or_else(|| panic!("the mismatch must be reported"));
    assert_eq!(
        unavailable.reason(),
        &UnavailableReason::IdentityMismatch {
            declared: "vendor.pkg@2.0.0".to_owned()
        }
    );
}

#[test]
fn a_package_with_no_binary_for_this_host_is_listed_with_its_reason() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("root");
    let unsupported = manifest_json("vendor.pkg", "1.0.0").replace(
        r#""provider": { "mode": "none", "binaries": {} }"#,
        r#""provider": { "mode": "one-shot", "binaries": { "mips-unknown-linux-gnu": "bin/p" } }"#,
    );
    write_package_with(&root, "vendor.pkg", "1.0.0", &unsupported);

    let inventory = scan(&[user_root(&root)]);

    // The package is still listed — it is installed and valid, just not
    // runnable here — so the UI can show its name alongside the reason.
    assert_eq!(coordinates(&inventory), vec!["vendor.pkg@1.0.0"]);
    let entry = inventory
        .packages()
        .first()
        .unwrap_or_else(|| panic!("the package must be listed"));
    let host = HostTriple::current();
    let reason = entry
        .unsupported_reason(&host)
        .unwrap_or_else(|| panic!("this host has no binary"));
    assert_eq!(reason, format!("no binary for {host}"));
}

#[test]
fn a_provider_free_package_is_not_reported_as_unsupported() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("root");
    write_package(&root, "vendor.pkg", "1.0.0");

    let inventory = scan(&[user_root(&root)]);
    let entry = inventory
        .packages()
        .first()
        .unwrap_or_else(|| panic!("the package must be listed"));
    assert_eq!(entry.unsupported_reason(&HostTriple::current()), None);
}

#[test]
fn a_manifest_over_the_byte_bound_is_not_read_into_memory() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("root");
    let oversize = format!(
        "{}{}",
        manifest_json("vendor.pkg", "1.0.0"),
        " ".repeat(MANIFEST_BYTE_LIMIT)
    );
    write_package_with(&root, "vendor.pkg", "1.0.0", &oversize);

    let inventory = scan(&[user_root(&root)]);

    assert!(coordinates(&inventory).is_empty());
    let unavailable = inventory
        .unavailable()
        .first()
        .unwrap_or_else(|| panic!("the oversize manifest must be reported"));
    assert_eq!(unavailable.reason(), &UnavailableReason::ManifestTooLarge);
}

#[test]
fn scanning_records_the_root_provenance_of_every_package() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let system = temp.path().join("system");
    let user = temp.path().join("user");
    write_package(&system, "vendor.system", "1.0.0");
    write_package(&user, "vendor.user", "1.0.0");

    let inventory = scan(&[read_only_root(&system), user_root(&user)]);

    let kinds: Vec<PluginRootKind> = inventory
        .packages()
        .iter()
        .map(InstalledPackage::root_kind)
        .collect();
    assert_eq!(kinds, vec![PluginRootKind::System, PluginRootKind::User]);
}
