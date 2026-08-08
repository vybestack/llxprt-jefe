//! Startup provider publication table (issue #390 CW-10, rows CW10-01/03/04).

use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::domain::action_registry::{ActionId, ActionRegistrySnapshot, Availability};
use crate::domain::plugin::HostTriple;
use crate::persistence::plugin_inventory::{MANIFEST_FILE_NAME, scan};
use crate::persistence::plugin_roots::{PluginRoot, PluginRootKind};
use crate::persistence::settings_document::PublishedSettings;

fn manifest_json(id: &str, mode: &str, binaries: &str) -> String {
    format!(
        r#"{{
          "manifest_schema": 1,
          "id": "{id}",
          "version": "1.0.0",
          "display_name": "Package {id}",
          "host_api": {{ "minimum": "1.0.0", "maximum": "1.0.0" }},
          "protocol": 1,
          "provider": {{ "mode": "{mode}", "binaries": {binaries} }},
          "actions": [
            {{
              "id": "{id}.run",
              "label": "Run",
              "description": "Run the action",
              "category": "{id}",
              "contexts": ["dashboard"],
              "arguments": [],
              "timeout_seconds": 60,
              "destructive": false,
              "confirmation": "none",
              "handler": "run",
              "allowed_outcomes": ["notice"]
            }}
          ],
          "panels": [],
          "routes": [],
          "screens": []
        }}"#
    )
}

fn write_package(root: &Path, id: &str, manifest: &str) -> PathBuf {
    let directory = root.join(id).join("1.0.0");
    fs::create_dir_all(&directory).unwrap_or_else(|error| panic!("staging must succeed: {error}"));
    fs::write(directory.join(MANIFEST_FILE_NAME), manifest.as_bytes())
        .unwrap_or_else(|error| panic!("manifest must write: {error}"));
    directory
}

/// Published settings plus the compiled-only snapshot they compose to.
///
/// The catalog is built from the installed packages exactly as startup does,
/// because trust under `plugins.<id>` is only published for an owner the
/// catalog knows.
fn loaded(
    ids: &[&str],
    packages: &[crate::persistence::plugin_inventory::InstalledPackage],
) -> (PublishedSettings, ActionRegistrySnapshot) {
    let mut body = String::from("settings_schema = 2\n");
    for id in ids {
        use std::fmt::Write as _;
        let _ = writeln!(body, "\n[plugins.{id:?}]\nenabled = true");
    }
    let Ok(catalog) = crate::config_owners::owner_catalog_with_packages(packages) else {
        panic!("owner catalog must build");
    };
    match crate::persistence::keymap_edit::load_bytes(Some(body.as_bytes()), &catalog, "test") {
        Ok(keymap) => {
            let snapshot = keymap.composed.snapshot().clone();
            (keymap.settings, snapshot)
        }
        Err(diagnostics) => panic!("settings fixture must load: {diagnostics:?}"),
    }
}

fn containment(base: &Path) -> crate::runtime::provider::Containment {
    crate::runtime::provider::Containment {
        home: base.join("home"),
        tmpdir: base.join("tmp"),
        working_dir: base.join("work"),
        locale: "C".to_owned(),
        host_api: "1.0.0".to_owned(),
    }
}

fn action_id(value: &str) -> ActionId {
    let Ok(parsed) = ActionId::parse(value) else {
        panic!("action fixture must parse");
    };
    parsed
}

#[test]
fn one_shot_package_publishes_into_the_snapshot_and_starts_nothing() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.oneshot",
        &manifest_json(
            "vendor.oneshot",
            "one-shot",
            &format!(r#"{{ "{}": "bin/provider" }}"#, host.as_str()),
        ),
    );
    let inventory = scan(&[PluginRoot::new(root, PluginRootKind::User)]);
    let (settings, base) = loaded(&["vendor.oneshot"], inventory.packages());

    let published = publish_providers(&ProviderPublicationRequest {
        packages: inventory.packages(),
        settings: &settings,
        base_snapshot: &base,
        containment: containment(temp.path()),
    });

    assert_eq!(
        published
            .snapshot
            .availability_of(&action_id("vendor.oneshot.run")),
        Some(&Availability::Available)
    );
    assert!(
        !published.coordinator.has_persistent(),
        "a one-shot package must leave no persistent process running"
    );
    assert!(published.startup_warning.is_none());
}

#[test]
fn untrusted_package_never_reaches_the_snapshot() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.oneshot",
        &manifest_json(
            "vendor.oneshot",
            "one-shot",
            &format!(r#"{{ "{}": "bin/provider" }}"#, host.as_str()),
        ),
    );
    let inventory = scan(&[PluginRoot::new(root, PluginRootKind::User)]);
    let (settings, base) = loaded(&[], inventory.packages());

    let published = publish_providers(&ProviderPublicationRequest {
        packages: inventory.packages(),
        settings: &settings,
        base_snapshot: &base,
        containment: containment(temp.path()),
    });

    assert_eq!(
        published
            .snapshot
            .availability_of(&action_id("vendor.oneshot.run")),
        None
    );
}

#[test]
fn a_persistent_package_that_cannot_start_publishes_one_shared_reason() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    // The declared binary is never staged, so the spawn fails and publication
    // must roll back rather than half-publish.
    write_package(
        &root,
        "vendor.resident",
        &manifest_json(
            "vendor.resident",
            "persistent",
            &format!(r#"{{ "{}": "bin/absent" }}"#, host.as_str()),
        ),
    );
    let inventory = scan(&[PluginRoot::new(root, PluginRootKind::User)]);
    let (settings, base) = loaded(&["vendor.resident"], inventory.packages());

    let published = publish_providers(&ProviderPublicationRequest {
        packages: inventory.packages(),
        settings: &settings,
        base_snapshot: &base,
        containment: containment(temp.path()),
    });

    let availability = published
        .snapshot
        .availability_of(&action_id("vendor.resident.run"));
    assert!(
        matches!(availability, Some(Availability::Unavailable { .. })),
        "a candidate that never became ready must publish as unavailable, got {availability:?}"
    );
    assert!(
        !published.coordinator.has_persistent(),
        "a failed startup must leave no supervisor owning a process"
    );
    assert!(
        published.startup_warning.is_some(),
        "the operator must be told why the provider is unavailable"
    );
    assert!(
        published
            .coordinator
            .catalog()
            .get(&action_id("vendor.resident.run"))
            .is_none(),
        "a failed candidate must leave nothing invocable"
    );
}

#[test]
fn no_packages_leaves_the_base_snapshot_untouched() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let (settings, base) = loaded(&[], &[]);

    let published = publish_providers(&ProviderPublicationRequest {
        packages: &[],
        settings: &settings,
        base_snapshot: &base,
        containment: containment(temp.path()),
    });

    assert_eq!(published.snapshot, base);
    assert!(published.coordinator.catalog().is_empty());
    assert!(published.startup_warning.is_none());
}
