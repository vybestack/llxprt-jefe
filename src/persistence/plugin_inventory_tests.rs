//! Physical-inventory table (issue #389 CW-09, acceptance rows R4–R8).

use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::persistence::plugin_roots::{PluginRoot, PluginRootKind};

/// Build one package directory `<root>/<id>/<version>/plugin.json`.
fn write_package(root: &Path, id: &str, version: &str) -> PathBuf {
    let directory = root.join(id).join(version);
    fs::create_dir_all(&directory)
        .unwrap_or_else(|error| panic!("staging {} must succeed: {error}", directory.display()));
    fs::write(directory.join(MANIFEST_FILE_NAME), b"{}")
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
    let first = write_package(&low, "vendor.pkg", "1.0.0");
    let second = write_package(&high, "vendor.pkg", "1.0.0");
    let body = b"{\"identical\":true}";
    for directory in [&first, &second] {
        if fs::write(directory.join(MANIFEST_FILE_NAME), body).is_err() {
            return;
        }
    }

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
    assert_eq!(unavailable.reason(), UnavailableReason::MissingManifest);
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
    assert_eq!(unavailable.reason(), UnavailableReason::EscapesRoot);
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
