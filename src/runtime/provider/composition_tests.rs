//! Startup provider composition table (issue #390 CW-10, rows CW10-01/03/13).

use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::domain::action_registry::Availability;
use crate::domain::plugin::HostTriple;
use crate::persistence::plugin_inventory::{MANIFEST_FILE_NAME, scan};
use crate::persistence::plugin_roots::{PluginRoot, PluginRootKind};

/// A manifest declaring one provider of `mode` and one action.
fn manifest_json(id: &str, version: &str, mode: &str, binaries: &str) -> String {
    format!(
        r#"{{
          "manifest_schema": 1,
          "id": "{id}",
          "version": "{version}",
          "display_name": "Package {id}",
          "host_api": {{ "minimum": "1.0.0", "maximum": "1.0.0" }},
          "protocol": 1,
          "provider": {{ "mode": "{mode}", "binaries": {binaries} }},
          "actions": [
            {{
              "id": "{id}.run",
              "label": "Run {id}",
              "description": "Run the {id} action",
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

fn write_package(root: &Path, id: &str, version: &str, manifest: &str) -> PathBuf {
    let directory = root.join(id).join(version);
    fs::create_dir_all(&directory)
        .unwrap_or_else(|error| panic!("staging {} must succeed: {error}", directory.display()));
    fs::write(directory.join(MANIFEST_FILE_NAME), manifest.as_bytes())
        .unwrap_or_else(|error| panic!("manifest must write: {error}"));
    directory
}

fn host_binaries(host: &HostTriple, relative: &str) -> String {
    format!(r#"{{ "{}": "{relative}" }}"#, host.as_str())
}

fn containment(base: &Path) -> Containment {
    Containment {
        home: base.join("home"),
        tmpdir: base.join("tmp"),
        working_dir: base.join("work"),
        locale: "C".to_owned(),
        host_api: "1.0.0".to_owned(),
    }
}

fn compose_for(root: &Path, base: &Path, trusted: &[&str]) -> ProviderComposition {
    let inventory = scan(&[PluginRoot::new(root.to_path_buf(), PluginRootKind::User)]);
    let owned: Vec<String> = trusted.iter().map(|value| (*value).to_owned()).collect();
    let settings = crate::persistence::settings_document::PublishedSettings::default();
    compose(&CompositionRequest {
        packages: inventory.packages(),
        trusted: &|id: &str| owned.iter().any(|seen| seen == id),
        settings: &settings,
        host: HostTriple::current(),
        containment: containment(base),
    })
}

fn availability_of(composition: &ProviderComposition, action: &str) -> Option<Availability> {
    composition
        .availability()
        .iter()
        .find(|entry| entry.action().as_str() == action)
        .map(|entry| entry.availability().clone())
}

#[test]
fn one_shot_package_publishes_action_and_starts_no_process() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.oneshot",
        "1.0.0",
        &manifest_json(
            "vendor.oneshot",
            "1.0.0",
            "one-shot",
            &host_binaries(&host, "bin/provider"),
        ),
    );

    let composition = compose_for(&root, temp.path(), &["vendor.oneshot"]);

    assert_eq!(composition.actions().len(), 1);
    assert_eq!(composition.catalog().len(), 1);
    assert!(
        composition.persistent_candidates().is_empty(),
        "a one-shot package must contribute no startup candidate"
    );
    assert_eq!(
        availability_of(&composition, "vendor.oneshot.run"),
        Some(Availability::Available)
    );
}

#[test]
fn untrusted_package_contributes_nothing() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.oneshot",
        "1.0.0",
        &manifest_json(
            "vendor.oneshot",
            "1.0.0",
            "one-shot",
            &host_binaries(&host, "bin/provider"),
        ),
    );

    let composition = compose_for(&root, temp.path(), &[]);

    assert!(composition.actions().is_empty());
    assert!(composition.catalog().is_empty());
    assert!(composition.availability().is_empty());
}

#[test]
fn unsupported_platform_publishes_the_shared_unavailable_reason() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    write_package(
        &root,
        "vendor.alien",
        "1.0.0",
        &manifest_json(
            "vendor.alien",
            "1.0.0",
            "one-shot",
            r#"{ "aarch64-unknown-none-elf": "bin/provider" }"#,
        ),
    );

    let composition = compose_for(&root, temp.path(), &["vendor.alien"]);

    assert_eq!(composition.actions().len(), 1);
    assert!(
        composition.catalog().is_empty(),
        "an unselectable binary must never enter the runtime catalog"
    );
    let host = HostTriple::current();
    assert_eq!(
        availability_of(&composition, "vendor.alien.run"),
        Some(Availability::Unavailable {
            reason: format!("no binary for {}", host.as_str())
        })
    );
}

#[test]
fn package_without_provider_contributes_nothing() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    write_package(
        &root,
        "vendor.inert",
        "1.0.0",
        &manifest_json("vendor.inert", "1.0.0", "none", "{}"),
    );

    let composition = compose_for(&root, temp.path(), &["vendor.inert"]);

    assert!(composition.actions().is_empty());
    assert!(composition.catalog().is_empty());
    assert!(composition.persistent_candidates().is_empty());
}

#[test]
fn persistent_candidates_are_ordered_by_plugin_id() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    for id in ["vendor.zeta", "vendor.alpha"] {
        write_package(
            &root,
            id,
            "1.0.0",
            &manifest_json(id, "1.0.0", "persistent", &host_binaries(&host, "bin/p")),
        );
    }

    let composition = compose_for(&root, temp.path(), &["vendor.zeta", "vendor.alpha"]);

    let ordered: Vec<String> = composition
        .persistent_candidates()
        .iter()
        .map(|candidate| candidate.plugin_id.to_string())
        .collect();
    assert_eq!(ordered, vec!["vendor.alpha", "vendor.zeta"]);
    for candidate in composition.persistent_candidates() {
        assert!(
            candidate.generation > 0,
            "every candidate must carry a fixed positive generation"
        );
    }
}

#[test]
fn resolved_binary_is_contained_under_the_package_directory() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    let staged = write_package(
        &root,
        "vendor.oneshot",
        "1.0.0",
        &manifest_json(
            "vendor.oneshot",
            "1.0.0",
            "one-shot",
            &host_binaries(&host, "bin/provider"),
        ),
    );
    // The scan canonicalizes package directories, so compare against the
    // canonical path rather than the staging path: on macOS the temp root is
    // reached through a symlink and the two spellings differ.
    let directory = staged.canonicalize().unwrap_or(staged);

    let composition = compose_for(&root, temp.path(), &["vendor.oneshot"]);
    let action = crate::domain::action_registry::ActionId::parse("vendor.oneshot.run")
        .unwrap_or_else(|error| panic!("action id must parse: {error}"));
    let descriptor = composition
        .catalog()
        .get(&action)
        .unwrap_or_else(|| panic!("catalog must carry the published action"));

    assert_eq!(descriptor.binary, directory.join("bin").join("provider"));
    assert_eq!(descriptor.environment.provider_dir, directory.join("bin"));
    assert!(
        descriptor.binary.starts_with(&directory),
        "the selected binary must stay inside its own package directory"
    );
}

#[test]
fn failed_persistent_startup_discards_only_persistent_contributions() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.oneshot",
        "1.0.0",
        &manifest_json(
            "vendor.oneshot",
            "1.0.0",
            "one-shot",
            &host_binaries(&host, "bin/p"),
        ),
    );
    write_package(
        &root,
        "vendor.resident",
        "1.0.0",
        &manifest_json(
            "vendor.resident",
            "1.0.0",
            "persistent",
            &host_binaries(&host, "bin/p"),
        ),
    );

    let mut composition = compose_for(&root, temp.path(), &["vendor.oneshot", "vendor.resident"]);
    composition.discard_persistent_contributions();

    assert_eq!(
        availability_of(&composition, "vendor.oneshot.run"),
        Some(Availability::Available)
    );
    assert_eq!(
        availability_of(&composition, "vendor.resident.run"),
        None,
        "the failed atomic persistent set must publish no action metadata"
    );
    assert!(
        composition
            .catalog()
            .get(
                &crate::domain::action_registry::ActionId::parse("vendor.resident.run")
                    .unwrap_or_else(|error| panic!("action id must parse: {error}"))
            )
            .is_none(),
        "a failed persistent package must leave no runnable catalog entry"
    );
}
