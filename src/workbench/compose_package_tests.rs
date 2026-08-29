//! Selected-package screen loading, lowering, and transactional composition
//! (issue #391, CW-11 Slice A: static-composition sub-slice).
//!
//! These tests prove that a screen file contributed by a settings-selected
//! installed package is loaded from the package directory, parsed through the
//! sole screen-file parser, lowered through the sole lowerer, and composed into
//! the same `ScreenRegistry` as built-in and user-definition screens — and that
//! any failure refuses the entire candidate registry without publication.

use crate::test_support::MustErr;
use std::fs;
use std::path::Path;

use super::compose::{CompositionRefused, compose_screens, compose_screens_with_package_sources};
use super::compose_fixtures::{candidate, enabled, review_definition};
use super::config::panel_insets;
use super::descriptor::OverlayKind;
use super::geometry::Insets;
use super::ids::{CustomScreenId, PanelId, PluginScreenId, ScreenIdentity};
use super::intern::intern;
use super::screens::{ScreenRegistry, builtin_screens};

use crate::config_owners::owner_catalog_with_packages;
use crate::domain::plugin::HostTriple;
use crate::persistence::diagnostic::CfgCode;
use crate::persistence::plugin_inventory::{InstalledPackage, MANIFEST_FILE_NAME, scan};
use crate::persistence::plugin_roots::{PluginRoot, PluginRootKind};
use crate::persistence::screen_files::PackageScreenSources;
use crate::persistence::settings_document::{PublishedSettings, SettingsDocument};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Build the compiled screen table.
fn compiled() -> ScreenRegistry {
    builtin_screens()
        .unwrap_or_else(|error| unreachable!("the compiled screen table is valid: {error}"))
}

/// A manifest declaring one persistent provider, one panel, and screen
/// contributions described by `screens_json`.
fn manifest_json(id: &str, version: &str, host: &HostTriple, screens_json: &str) -> String {
    format!(
        r#"{{
          "manifest_schema": 1,
          "id": "{id}",
          "version": "{version}",
          "display_name": "Package {id}",
          "host_api": {{ "minimum": "1.0.0", "maximum": "1.0.0" }},
          "protocol": 1,
          "provider": {{
            "mode": "persistent",
            "binaries": {{ "{}": "bin/provider" }}
          }},
          "actions": [],
          "panels": [
            {{
              "id": "{id}.list",
              "model_kinds": ["list"],
              "event_schema": [{{ "kind": "selected", "arguments": [] }}],
              "handler": "{id}.handler",
              "ports": [{{ "id": "{id}.port" }}]
            }}
          ],
          "routes": [],
          "screens": {screens_json}
        }}"#,
        host.as_str()
    )
}

fn manifest_with_actions(id: &str, version: &str, host: &HostTriple, screens_json: &str) -> String {
    let manifest = manifest_json(id, version, host, screens_json);
    manifest.replace(
        "\"actions\": []",
        &format!(
            r#""actions": [
              {{
                "id": "{id}.run",
                "label": "Run",
                "description": "Run here",
                "category": "tasks",
                "contexts": ["{id}.main"],
                "arguments": [],
                "timeout_seconds": 30,
                "destructive": false,
                "confirmation": "none",
                "handler": "{id}.handler",
                "allowed_outcomes": []
              }},
              {{
                "id": "{id}.elsewhere",
                "label": "Elsewhere",
                "description": "Run elsewhere",
                "category": "tasks",
                "contexts": ["core.dashboard"],
                "arguments": [],
                "timeout_seconds": 30,
                "destructive": false,
                "confirmation": "none",
                "handler": "{id}.handler",
                "allowed_outcomes": []
              }}
            ]"#
        ),
    )
}

/// A manifest with no panels or screens — a package that contributes nothing
/// screen-related.
fn manifest_bare(id: &str, version: &str, host: &HostTriple) -> String {
    format!(
        r#"{{
          "manifest_schema": 1,
          "id": "{id}",
          "version": "{version}",
          "display_name": "Package {id}",
          "host_api": {{ "minimum": "1.0.0", "maximum": "1.0.0" }},
          "protocol": 1,
          "provider": {{
            "mode": "persistent",
            "binaries": {{ "{}": "bin/provider" }}
          }},
          "actions": [],
          "panels": [],
          "routes": [],
          "screens": []
        }}"#,
        host.as_str()
    )
}

/// A well-formed screen TOML whose identity is `screen_id` and whose single
/// panel references `panel_type`.
fn screen_toml(screen_id: &str, panel_type: &str) -> String {
    format!(
        r#"screen_schema = 1
id = "{screen_id}"
title = "Package Screen"
route = "{screen_id}"
initial_focus = "list"
focus_order = ["list"]

[[panels]]
id = "list"
type = "{panel_type}"
focusable = true
required = true

[layout]
type = "leaf"
panel = "list"
"#
    )
}

fn screen_toml_with_overlays(screen_id: &str, panel_type: &str) -> String {
    let overlays = r#"[[overlays]]
kind = "help"

[[overlays]]
kind = "search"

[[overlays]]
kind = "confirmation"

[[panels]]"#;
    screen_toml(screen_id, panel_type).replace("[[panels]]", overlays)
}

fn screen_toml_with_resource(screen_id: &str, panel_type: &str) -> String {
    screen_toml(screen_id, panel_type)
        .replace("screen_schema = 1", "screen_schema = 2")
        .replace(
            "[[panels]]",
            r#"[[resources]]
type_id = "vendor.demo.item"
schema_version = 1
semantic_key = "semantic-key"

[[resources.fields]]
id = "semantic-key"
label = "Semantic key"
type = "string"
required = true

[[panels]]"#,
        )
}

/// Write one package to `root` with a manifest and, optionally, screen bytes.
fn write_package(root: &Path, id: &str, version: &str, manifest: &str, screen: Option<&str>) {
    let dir = root.join(id).join(version);
    fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create {}: {error}", dir.display()));
    fs::write(dir.join(MANIFEST_FILE_NAME), manifest.as_bytes())
        .unwrap_or_else(|error| panic!("write manifest: {error}"));
    if let Some(contents) = screen {
        let screens_dir = dir.join("screens");
        fs::create_dir_all(&screens_dir)
            .unwrap_or_else(|error| panic!("create screens dir: {error}"));
        fs::write(screens_dir.join("main.screen.toml"), contents.as_bytes())
            .unwrap_or_else(|error| panic!("write screen: {error}"));
    }
}

/// Build published settings from `[plugins.<id>]` selections.
fn settings(
    packages: &[InstalledPackage],
    selections: &[(&str, Option<&str>)],
) -> PublishedSettings {
    use std::fmt::Write as _;
    let mut source = String::from("settings_schema = 2\n");
    for (id, version) in selections {
        let _ = writeln!(source, "\n[plugins.{id:?}]\nenabled = true");
        if let Some(version) = version {
            let _ = writeln!(source, "version = {version:?}");
        }
    }
    let catalog = owner_catalog_with_packages(packages)
        .unwrap_or_else(|diagnostics| panic!("owner catalog must build: {diagnostics:?}"));
    SettingsDocument::parse(source.as_bytes())
        .unwrap_or_else(|error| panic!("settings must parse: {error:?}"))
        .publish(&catalog)
        .unwrap_or_else(|diagnostics| panic!("settings must publish: {diagnostics:?}"))
}

/// Scan a packages root.
fn scan_root(root: &Path) -> Vec<InstalledPackage> {
    scan(&[PluginRoot::new(root.to_path_buf(), PluginRootKind::User)])
        .packages()
        .to_vec()
}

/// Compose with no user-definition candidates.
fn compose(
    packages: &[InstalledPackage],
    published: &PublishedSettings,
) -> Result<super::compose::ScreenComposition, CompositionRefused> {
    let sources = PackageScreenSources::capture(packages);
    compose_screens_with_package_sources(&compiled(), &[], packages, &sources, published)
}

/// The owner-qualified `Package` identity for `<pkg>.<screen>`.
fn package_identity(pkg: &str, screen: &str) -> ScreenIdentity {
    let value = format!("{pkg}.{screen}");
    let interned =
        intern(&value).unwrap_or_else(|error| unreachable!("intern must succeed: {error}"));
    ScreenIdentity::Package(
        PluginScreenId::parse(interned)
            .unwrap_or_else(|error| unreachable!("plugin screen id must parse: {error}")),
    )
}

/// Whether a screen identity is in the registry.
fn contains(registry: &ScreenRegistry, identity: ScreenIdentity) -> bool {
    registry.get_identity(identity).is_some()
}

/// One screen contribution JSON object.
fn contribution(path: &str, screen_id: &str) -> String {
    format!(r#"[{{ "path": "{path}", "screen_ids": ["{screen_id}"] }}]"#)
}

// ---------------------------------------------------------------------------
// Acceptance tests
// ---------------------------------------------------------------------------

#[test]
fn selected_package_panel_owns_a_local_screen_panel() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.demo",
        "1.0.0",
        &manifest_json("vendor.demo", "1.0.0", &host, "[]"),
        None,
    );
    let packages = scan_root(&root);
    let mut published = settings(&packages, &[("vendor.demo", Some("1.0.0"))]);
    published.workbench.enabled_screens.push(
        crate::domain::Id::parse("local.review")
            .unwrap_or_else(|error| panic!("local screen identity: {error}")),
    );
    let local = candidate("review", &screen_toml("local.review", "vendor.demo.list"));
    let sources = PackageScreenSources::capture(&packages);

    let composition = compose_screens_with_package_sources(
        &compiled(),
        &[local],
        &packages,
        &sources,
        &published,
    )
    .unwrap_or_else(|error| panic!("compose local provider panel: {error}"));
    let identity = ScreenIdentity::Custom(
        CustomScreenId::parse("local.review")
            .unwrap_or_else(|error| panic!("local identity: {error}")),
    );
    let binding = composition
        .registry
        .panel_binding(identity, &PanelId::from_static("list"))
        .unwrap_or_else(|| panic!("selected package must own the local panel"));
    assert_eq!(binding.owner.as_str(), "vendor.demo");
    assert_eq!(binding.panel_type.as_str(), "vendor.demo.list");
    assert_eq!(binding.model_kinds.len(), 1);
}

#[test]
fn selected_package_screen_joins_registry() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.demo",
        "1.0.0",
        &manifest_json(
            "vendor.demo",
            "1.0.0",
            &host,
            &contribution("screens/main.screen.toml", "vendor.demo.screen"),
        ),
        Some(&screen_toml("vendor.demo.screen", "vendor.demo.list")),
    );
    let packages = scan_root(&root);
    let published = settings(&packages, &[("vendor.demo", Some("1.0.0"))]);
    let composition = compose(&packages, &published)
        .unwrap_or_else(|error| unreachable!("composition must succeed: {error}"));
    let identity = package_identity("vendor.demo", "screen");
    assert!(
        contains(&composition.registry, identity),
        "the selected package screen must be in the registry"
    );
    let descriptor = composition
        .registry
        .get_identity(identity)
        .unwrap_or_else(|| panic!("the selected descriptor must be retained"));
    let panel = descriptor
        .panels
        .first()
        .unwrap_or_else(|| panic!("the selected descriptor must retain its panel"));
    assert_eq!(
        panel_insets(&panel.config),
        Insets::new(1, 1, 1, 1),
        "package panel content must not overlap its host-rendered border and title"
    );
}

fn package_overlay_kinds() -> Vec<OverlayKind> {
    let Ok(temp) = tempfile::tempdir() else {
        return Vec::new();
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.demo",
        "1.0.0",
        &manifest_json(
            "vendor.demo",
            "1.0.0",
            &host,
            &contribution("screens/main.screen.toml", "vendor.demo.screen"),
        ),
        Some(&screen_toml_with_overlays(
            "vendor.demo.screen",
            "vendor.demo.list",
        )),
    );
    let packages = scan_root(&root);
    let published = settings(&packages, &[("vendor.demo", Some("1.0.0"))]);
    let composition = compose(&packages, &published)
        .unwrap_or_else(|error| unreachable!("composition must succeed: {error}"));
    composition
        .registry
        .get_identity(package_identity("vendor.demo", "screen"))
        .map_or_else(Vec::new, |descriptor| descriptor.overlays.clone())
}

#[test]
fn builtin_local_and_package_overlay_declarations_lower_identically() {
    assert_eq!(
        package_overlay_kinds(),
        OverlayKind::ALL.to_vec(),
        "a package declaration must lower to the exact host vocabulary"
    );

    // Built-in arm: every compiled descriptor carries the same vocabulary.
    let compiled = compiled();
    for descriptor in compiled.screens() {
        assert_eq!(
            descriptor.overlays,
            OverlayKind::ALL.to_vec(),
            "built-in {} must declare the host overlay vocabulary",
            descriptor.id
        );
    }

    // Local arm: a local definition file lowers to the same vocabulary.
    let local_overlays = r#"
[[overlays]]
kind = "help"

[[overlays]]
kind = "search"

[[overlays]]
kind = "confirmation"

"#;
    let local_source = format!("{}{}", review_definition(), local_overlays);
    let local = compose_screens(
        &compiled,
        &[candidate("review", &local_source)],
        &enabled(&["review"]),
    )
    .unwrap_or_else(|error| unreachable!("local composition must publish: {error}"));
    let local_screen = local
        .registry
        .get_identity(ScreenIdentity::Custom(
            CustomScreenId::parse("local.review")
                .unwrap_or_else(|error| unreachable!("fixture identity must parse: {error}")),
        ))
        .unwrap_or_else(|| panic!("the lowered local screen must be registered"));
    assert_eq!(
        local_screen.overlays,
        OverlayKind::ALL.to_vec(),
        "a local declaration must lower to the exact host vocabulary"
    );
}

#[test]
fn selected_package_screen_contributes_definition_owned_resource_schemas() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.demo",
        "1.0.0",
        &manifest_json(
            "vendor.demo",
            "1.0.0",
            &host,
            &contribution("screens/main.screen.toml", "vendor.demo.screen"),
        ),
        Some(&screen_toml_with_resource(
            "vendor.demo.screen",
            "vendor.demo.list",
        )),
    );
    let packages = scan_root(&root);
    let published = settings(&packages, &[("vendor.demo", Some("1.0.0"))]);

    let composition = compose(&packages, &published)
        .unwrap_or_else(|error| unreachable!("composition must succeed: {error}"));

    assert_eq!(composition.resource_schemas.len(), 1);
    let schema = &composition.resource_schemas[0];
    assert_eq!(schema.owner_id().as_str(), "vendor.demo.screen");
    assert_eq!(schema.type_id().as_str(), "vendor.demo.item");
    assert_eq!(schema.schema_version(), 1);
}

#[test]
fn disabled_package_contributes_nothing() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.demo",
        "1.0.0",
        &manifest_json(
            "vendor.demo",
            "1.0.0",
            &host,
            &contribution("screens/main.screen.toml", "vendor.demo.screen"),
        ),
        Some(&screen_toml_with_resource(
            "vendor.demo.screen",
            "vendor.demo.list",
        )),
    );
    let packages = scan_root(&root);
    // Settings do not enable the package.
    let published = settings(&packages, &[]);
    let composition = compose(&packages, &published)
        .unwrap_or_else(|error| unreachable!("composition must succeed: {error}"));
    assert!(
        !contains(
            &composition.registry,
            package_identity("vendor.demo", "screen")
        ),
        "a disabled package must not contribute screens"
    );
    assert!(
        composition.resource_schemas.is_empty(),
        "a disabled package must not contribute resource schemas"
    );
}

#[test]
fn unselected_version_contributes_nothing() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    // Two versions of the same package.
    write_package(
        &root,
        "vendor.demo",
        "1.0.0",
        &manifest_json(
            "vendor.demo",
            "1.0.0",
            &host,
            &contribution("screens/main.screen.toml", "vendor.demo.screen"),
        ),
        Some("the unselected version is deliberately malformed ]][["),
    );
    write_package(
        &root,
        "vendor.demo",
        "2.0.0",
        &manifest_json(
            "vendor.demo",
            "2.0.0",
            &host,
            &contribution("screens/main.screen.toml", "vendor.demo.screen"),
        ),
        Some(&screen_toml_with_resource(
            "vendor.demo.screen",
            "vendor.demo.list",
        )),
    );
    let packages = scan_root(&root);
    // Select version 2.0.0 — version 1.0.0 contributes nothing.
    let published = settings(&packages, &[("vendor.demo", Some("2.0.0"))]);
    let composition = compose(&packages, &published)
        .unwrap_or_else(|error| unreachable!("composition must succeed: {error}"));
    assert!(
        contains(
            &composition.registry,
            package_identity("vendor.demo", "screen")
        ),
        "the selected version's screen must be in the registry"
    );
    assert_eq!(
        composition.resource_schemas.len(),
        1,
        "only the selected version may contribute its resource schema"
    );
}

#[test]
fn malformed_package_screen_refuses_whole_registry() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.demo",
        "1.0.0",
        &manifest_json(
            "vendor.demo",
            "1.0.0",
            &host,
            &contribution("screens/main.screen.toml", "vendor.demo.screen"),
        ),
        Some("this is not valid TOML ]][["),
    );
    let packages = scan_root(&root);
    let published = settings(&packages, &[("vendor.demo", Some("1.0.0"))]);
    let result = compose(&packages, &published);
    assert!(
        result.is_err(),
        "a malformed package screen must refuse the whole registry"
    );
    let refusal = result.must_err("expected failure");
    assert_eq!(
        refusal.configuration.code,
        CfgCode::E006,
        "a syntax error is a reference failure (E006)"
    );
}

#[test]
fn package_panel_type_not_in_manifest_is_refused() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.demo",
        "1.0.0",
        &manifest_json(
            "vendor.demo",
            "1.0.0",
            &host,
            &contribution("screens/main.screen.toml", "vendor.demo.screen"),
        ),
        // The panel type references a built-in panel, not the manifest's
        // declared "vendor.demo.list".
        Some(&screen_toml("vendor.demo.screen", "repository-list")),
    );
    let packages = scan_root(&root);
    let published = settings(&packages, &[("vendor.demo", Some("1.0.0"))]);
    let result = compose(&packages, &published);
    assert!(
        result.is_err(),
        "a panel type not declared by the manifest must refuse the registry"
    );
    let refusal = result.must_err("expected failure");
    assert_eq!(
        refusal.configuration.code,
        CfgCode::E005,
        "an undeclared panel type is an ownership failure (E005)"
    );
}

#[test]
fn package_screen_identity_not_in_manifest_is_refused() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.demo",
        "1.0.0",
        &manifest_json(
            "vendor.demo",
            "1.0.0",
            &host,
            r#"[{ "path": "screens/main.screen.toml", "screen_ids": ["vendor.demo.screen"] }]"#,
        ),
        // The file declares a screen id that does not match the manifest-declared
        // screen id.
        Some(&screen_toml("vendor.demo.wrong", "vendor.demo.list")),
    );
    let packages = scan_root(&root);
    let published = settings(&packages, &[("vendor.demo", Some("1.0.0"))]);
    let result = compose(&packages, &published);
    assert!(
        result.is_err(),
        "a screen identity not declared by the manifest must refuse the registry"
    );
    let refusal = result.must_err("expected failure");
    assert_eq!(
        refusal.configuration.code,
        CfgCode::E005,
        "an undeclared screen identity is an ownership failure (E005)"
    );
    assert!(
        refusal
            .screen
            .redacted_detail
            .contains("vendor.demo.screen")
    );
    assert!(!refusal.screen.redacted_detail.contains("vendor.demo.wrong"));
}

#[test]
fn missing_package_screen_file_refuses_whole_registry() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.demo",
        "1.0.0",
        &manifest_json(
            "vendor.demo",
            "1.0.0",
            &host,
            &contribution("screens/main.screen.toml", "vendor.demo.screen"),
        ),
        // No screen file written — the manifest declares a path that does not
        // exist on disk.
        None,
    );
    let packages = scan_root(&root);
    let published = settings(&packages, &[("vendor.demo", Some("1.0.0"))]);
    let result = compose(&packages, &published);
    assert!(
        result.is_err(),
        "a missing package screen file must refuse the whole registry"
    );
    let refusal = result.must_err("expected failure");
    assert_eq!(
        refusal.configuration.code,
        CfgCode::E006,
        "a missing screen file is a reference failure (E006)"
    );
}
#[cfg(unix)]
#[test]
fn package_screen_path_does_not_traverse_an_intermediate_symlink() {
    use std::os::unix::fs::symlink;

    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.demo",
        "1.0.0",
        &manifest_json(
            "vendor.demo",
            "1.0.0",
            &host,
            &contribution("screens/main.screen.toml", "vendor.demo.screen"),
        ),
        None,
    );
    let outside = temp.path().join("outside");
    fs::create_dir_all(&outside)
        .unwrap_or_else(|error| panic!("create outside directory: {error}"));
    fs::write(
        outside.join("main.screen.toml"),
        screen_toml("vendor.demo.screen", "vendor.demo.list"),
    )
    .unwrap_or_else(|error| panic!("write outside screen: {error}"));
    let package_directory = root.join("vendor.demo").join("1.0.0");
    symlink(&outside, package_directory.join("screens"))
        .unwrap_or_else(|error| panic!("link screen directory: {error}"));

    let packages = scan_root(&root);
    let published = settings(&packages, &[("vendor.demo", Some("1.0.0"))]);

    let result = compose(&packages, &published);
    assert!(
        result.is_err(),
        "a selected package screen must not escape through an intermediate symlink"
    );
    let refusal = result.must_err("expected failure");
    assert_eq!(
        refusal.configuration.code,
        CfgCode::E006,
        "a symlinked screen directory is a reference failure (E006)"
    );
}

#[test]
fn package_without_screens_contributes_nothing() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.demo",
        "1.0.0",
        &manifest_bare("vendor.demo", "1.0.0", &host),
        None,
    );
    let packages = scan_root(&root);
    let published = settings(&packages, &[("vendor.demo", Some("1.0.0"))]);
    let composition = compose(&packages, &published)
        .unwrap_or_else(|error| unreachable!("composition must succeed: {error}"));
    // The registry must still contain the compiled screens.
    assert!(
        composition
            .registry
            .get_identity(crate::workbench::DASHBOARD_IDENTITY)
            .is_some(),
        "built-in screens must still be in the registry"
    );
}

#[test]
fn two_selected_packages_each_contribute_their_own_screens() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    // Two packages with distinct namespaces, each contributing one screen.
    write_package(
        &root,
        "vendor.alpha",
        "1.0.0",
        &manifest_json(
            "vendor.alpha",
            "1.0.0",
            &host,
            &contribution("screens/main.screen.toml", "vendor.alpha.screen"),
        ),
        Some(&screen_toml("vendor.alpha.screen", "vendor.alpha.list")),
    );
    write_package(
        &root,
        "vendor.beta",
        "1.0.0",
        &manifest_json(
            "vendor.beta",
            "1.0.0",
            &host,
            &contribution("screens/main.screen.toml", "vendor.beta.screen"),
        ),
        Some(&screen_toml("vendor.beta.screen", "vendor.beta.list")),
    );
    let packages = scan_root(&root);
    let published = settings(
        &packages,
        &[
            ("vendor.alpha", Some("1.0.0")),
            ("vendor.beta", Some("1.0.0")),
        ],
    );
    let composition = compose(&packages, &published)
        .unwrap_or_else(|error| unreachable!("composition must succeed: {error}"));
    assert!(
        contains(
            &composition.registry,
            package_identity("vendor.alpha", "screen")
        ),
        "the alpha package screen must be present"
    );
    assert!(
        contains(
            &composition.registry,
            package_identity("vendor.beta", "screen")
        ),
        "the beta package screen must be present"
    );
}

#[test]
fn selected_package_screen_preserves_builtin_screens() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_package(
        &root,
        "vendor.demo",
        "1.0.0",
        &manifest_json(
            "vendor.demo",
            "1.0.0",
            &host,
            &contribution("screens/main.screen.toml", "vendor.demo.screen"),
        ),
        Some(&screen_toml("vendor.demo.screen", "vendor.demo.list")),
    );
    let packages = scan_root(&root);
    let published = settings(&packages, &[("vendor.demo", Some("1.0.0"))]);
    let composition = compose(&packages, &published)
        .unwrap_or_else(|error| unreachable!("composition must succeed: {error}"));
    // Both the package screen AND all compiled screens must be present.
    assert!(
        contains(
            &composition.registry,
            package_identity("vendor.demo", "screen")
        ),
        "the package screen must be present"
    );
    assert!(
        composition
            .registry
            .get_identity(crate::workbench::DASHBOARD_IDENTITY)
            .is_some(),
        "compiled screens must still be present"
    );
}

#[test]
fn panel_action_authority_contains_only_actions_available_on_its_screen() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    let screens = contribution("screens/main.screen.toml", "vendor.pkg.main");
    let manifest = manifest_with_actions("vendor.pkg", "1.0.0", &host, &screens);
    write_package(
        &root,
        "vendor.pkg",
        "1.0.0",
        &manifest,
        Some(&screen_toml("vendor.pkg.main", "vendor.pkg.list")),
    );
    let packages = scan_root(&root);
    let published = settings(&packages, &[("vendor.pkg", Some("1.0.0"))]);

    let composition =
        compose(&packages, &published).unwrap_or_else(|error| panic!("compose: {error}"));
    let binding = composition
        .registry
        .panel_binding(
            package_identity("vendor.pkg", "main"),
            &PanelId::from_static("list"),
        )
        .unwrap_or_else(|| panic!("package panel binding must exist"));

    assert_eq!(binding.action_authority.len(), 1);
    assert_eq!(binding.action_authority[0].as_str(), "vendor.pkg.run");
}

#[test]
fn selected_provider_panel_declaration_binds_identically_to_local_and_package_screens() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    let screens = contribution("screens/main.screen.toml", "vendor.demo.main");
    let manifest = manifest_json("vendor.demo", "1.0.0", &host, &screens).replace(
        r#""model_kinds": ["list"]"#,
        r#""model_kinds": ["list", "tree", "detail", "structured-diff", "form", "status", "progress", "empty", "error"]"#,
    );
    write_package(
        &root,
        "vendor.demo",
        "1.0.0",
        &manifest,
        Some(&screen_toml("vendor.demo.main", "vendor.demo.list")),
    );
    let packages = scan_root(&root);
    let mut published = settings(&packages, &[("vendor.demo", Some("1.0.0"))]);
    published.workbench.enabled_screens.push(
        crate::domain::Id::parse("local.fixture")
            .unwrap_or_else(|error| panic!("local identity: {error}")),
    );
    let local = candidate("fixture", &screen_toml("local.fixture", "vendor.demo.list"));
    let sources = PackageScreenSources::capture(&packages);

    let composition = compose_screens_with_package_sources(
        &compiled(),
        &[local],
        &packages,
        &sources,
        &published,
    )
    .unwrap_or_else(|error| panic!("cross-origin composition: {error}"));
    let local_identity = ScreenIdentity::Custom(
        CustomScreenId::parse("local.fixture")
            .unwrap_or_else(|error| panic!("custom screen identity: {error}")),
    );
    let local_binding = composition
        .registry
        .panel_binding(local_identity, &PanelId::from_static("list"))
        .unwrap_or_else(|| panic!("local provider panel binding must exist"));
    let package_binding = composition
        .registry
        .panel_binding(
            package_identity("vendor.demo", "main"),
            &PanelId::from_static("list"),
        )
        .unwrap_or_else(|| panic!("package provider panel binding must exist"));

    assert_eq!(local_binding.model_kinds, package_binding.model_kinds);
    assert_eq!(local_binding.model_kinds.len(), 9);
    assert_eq!(local_binding.owner, package_binding.owner);
    assert_eq!(local_binding.panel_type, package_binding.panel_type);
    assert_eq!(local_binding.event_schema, package_binding.event_schema);
    assert_eq!(
        local_binding.action_authority,
        package_binding.action_authority
    );
}
