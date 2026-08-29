//! Startup provider composition table (issue #390 CW-10, rows CW10-01/03/13).

use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::domain::Id;
use crate::domain::action_registry::Availability;
use crate::domain::plugin::HostTriple;
use crate::persistence::plugin_inventory::{MANIFEST_FILE_NAME, scan, selected_packages};
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

fn published_settings(
    packages: &[crate::persistence::plugin_inventory::InstalledPackage],
    selections: &[(&str, Option<&str>)],
) -> crate::persistence::settings_document::PublishedSettings {
    use std::fmt::Write as _;

    let mut source = String::from(
        "settings_schema = 2
",
    );
    for (id, version) in selections {
        let _ = writeln!(
            source,
            "
[plugins.{id:?}]
enabled = true"
        );
        if let Some(version) = version {
            let _ = writeln!(source, "version = {version:?}");
        }
    }
    let catalog = crate::config_owners::owner_catalog_with_packages(packages)
        .unwrap_or_else(|diagnostics| panic!("owner catalog must build: {diagnostics:?}"));
    crate::persistence::settings_document::SettingsDocument::parse(source.as_bytes())
        .unwrap_or_else(|error| panic!("settings must parse: {error:?}"))
        .publish(&catalog)
        .unwrap_or_else(|diagnostics| panic!("settings must publish: {diagnostics:?}"))
}

fn compose_with_settings(
    packages: &[crate::persistence::plugin_inventory::InstalledPackage],
    base: &Path,
    settings: &crate::persistence::settings_document::PublishedSettings,
) -> ProviderComposition {
    compose(&CompositionRequest {
        packages,
        settings,
        host: HostTriple::current(),
        containment: containment(base),
    })
}

fn compose_for(root: &Path, base: &Path, trusted: &[&str]) -> ProviderComposition {
    let inventory = scan(&[PluginRoot::new(root.to_path_buf(), PluginRootKind::User)]);
    let selections: Vec<(&str, Option<&str>)> = trusted.iter().map(|id| (*id, None)).collect();
    let settings = published_settings(inventory.packages(), &selections);
    let selected = selected_packages(inventory.packages(), &settings)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    compose_with_settings(&selected, base, &settings)
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
    let manifest = manifest_json(
        "vendor.oneshot",
        "1.0.0",
        "one-shot",
        &host_binaries(&host, "bin/provider"),
    )
    .replace(
        "\"arguments\": []",
        "\"arguments\": [{ \"id\": \"branch\", \"label\": \"Branch\", \"type\": \"string\", \"required\": true, \"restart\": \"none\" }]",
    );
    write_package(&root, "vendor.oneshot", "1.0.0", &manifest);

    let composition = compose_for(&root, temp.path(), &["vendor.oneshot"]);

    assert_eq!(composition.actions().len(), 1);
    assert_eq!(composition.catalog().len(), 1);
    let (_, descriptor) = composition
        .catalog()
        .iter()
        .next()
        .unwrap_or_else(|| panic!("published action must retain its descriptor"));
    assert_eq!(descriptor.arguments.len(), 1);
    assert_eq!(descriptor.arguments[0].id().as_str(), "branch");
    assert!(
        composition.persistent_candidates().is_empty(),
        "a one-shot package must contribute no startup candidate"
    );
    assert_eq!(
        availability_of(&composition, "vendor.oneshot.run"),
        Some(Availability::Available)
    );
}

/// Package selection belongs to the startup candidate. Composition consumes
/// that exact set and must not independently reinterpret Settings.
#[test]
fn composition_consumes_caller_selected_packages_without_reselecting() {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.selected",
        "1.0.0",
        &manifest_json(
            "vendor.selected",
            "1.0.0",
            "one-shot",
            &host_binaries(&host, "bin/provider"),
        ),
    );
    let inventory = scan(&[PluginRoot::new(root, PluginRootKind::User)]);
    let settings = published_settings(inventory.packages(), &[]);

    let composition = compose_with_settings(inventory.packages(), temp.path(), &settings);

    assert_eq!(composition.actions().len(), 1);
    assert_eq!(composition.catalog().len(), 1);
    assert_eq!(
        availability_of(&composition, "vendor.selected.run"),
        Some(Availability::Available)
    );
}

#[test]
fn action_policy_retains_only_its_package_declared_routes() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    let manifest = manifest_json(
        "vendor.oneshot",
        "1.0.0",
        "one-shot",
        &host_binaries(&host, "bin/provider"),
    )
    .replace(
        "\"routes\": [],",
        "\"routes\": [{ \"id\": \"vendor.oneshot.open\", \"activation_fields\": [], \"target_screen\": \"vendor.oneshot.main\" }],",
    )
    .replace(
        "\"screens\": []",
        "\"screens\": [{ \"path\": \"screens/main.screen.toml\", \"screen_ids\": [\"vendor.oneshot.main\"] }]",
    );
    write_package(&root, "vendor.oneshot", "1.0.0", &manifest);

    let composition = compose_for(&root, temp.path(), &["vendor.oneshot"]);
    let descriptor = composition.catalog().iter().next().map_or_else(
        || panic!("the selected action must have a runtime descriptor"),
        |(_, descriptor)| descriptor,
    );
    let owned =
        Id::parse("vendor.oneshot.open").unwrap_or_else(|error| panic!("owned route: {error}"));
    let foreign =
        Id::parse("vendor.other.open").unwrap_or_else(|error| panic!("foreign route: {error}"));

    assert!(descriptor.policy.allows_route(&owned));
    assert!(!descriptor.policy.allows_route(&foreign));
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
fn composition_uses_only_the_exact_settings_selected_package_version() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    for version in ["2.0.0", "1.0.0"] {
        write_package(
            &root,
            "vendor.selected",
            version,
            &manifest_json(
                "vendor.selected",
                version,
                "one-shot",
                &host_binaries(&host, "bin/provider"),
            ),
        );
    }
    let inventory = scan(&[PluginRoot::new(root, PluginRootKind::User)]);
    let settings = published_settings(inventory.packages(), &[("vendor.selected", Some("1.0.0"))]);
    let selected = selected_packages(inventory.packages(), &settings)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();

    let composition = compose_with_settings(&selected, temp.path(), &settings);

    assert_eq!(composition.actions().len(), 1);
    assert_eq!(composition.catalog().len(), 1);
    let action = crate::domain::action_registry::ActionId::parse("vendor.selected.run")
        .unwrap_or_else(|error| panic!("action id must parse: {error}"));
    let descriptor = composition
        .catalog()
        .get(&action)
        .unwrap_or_else(|| panic!("selected action must be runnable"));
    assert!(
        descriptor
            .binary
            .ends_with("vendor.selected/1.0.0/bin/provider"),
        "provider composition must use only the exact selected package, got {}",
        descriptor.binary.display()
    );
}

#[test]
fn invalid_config_unavailability_names_the_field_and_reason_without_its_value() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    let manifest = manifest_json(
        "vendor.invalid-config",
        "1.0.0",
        "one-shot",
        &host_binaries(&host, "bin/provider"),
    )
    .replacen(
        "\"actions\": [",
        r#""config": {
            "schema_version": 1,
            "fields": [
              { "id": "mode", "label": "Mode", "type": "string", "required": true, "restart": "none" }
            ]
          },
          "actions": ["#,
        1,
    );
    write_package(&root, "vendor.invalid-config", "1.0.0", &manifest);
    let inventory = scan(&[PluginRoot::new(root, PluginRootKind::User)]);
    let catalog = crate::config_owners::owner_catalog_with_packages(inventory.packages())
        .unwrap_or_else(|diagnostics| panic!("owner catalog must build: {diagnostics:?}"));
    let source = br#"
settings_schema = 2
[plugins."vendor.invalid-config"]
enabled = true
version = "1.0.0"
[plugins."vendor.invalid-config".config]
mode = 42
"#;
    let settings = crate::persistence::settings_document::SettingsDocument::parse(source)
        .unwrap_or_else(|error| panic!("settings must parse: {error:?}"))
        .publish(&catalog)
        .unwrap_or_else(|diagnostics| panic!("settings must publish: {diagnostics:?}"));

    let composition = compose_with_settings(inventory.packages(), temp.path(), &settings);
    let Some(Availability::Unavailable { reason }) =
        availability_of(&composition, "vendor.invalid-config.run")
    else {
        panic!("invalid active config must publish an unavailable action");
    };

    assert!(reason.contains("mode (value has the wrong type)"));
    assert!(!reason.contains("42"), "diagnostics must not echo values");
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
