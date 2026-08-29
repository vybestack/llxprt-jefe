use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::plugin::HostTriple;
use crate::domain::sha256::Sha256;
use crate::domain::{Id, TypedMap, TypedValue};
use crate::persistence::migration::migrate_settings;
use crate::persistence::paths::{PathProvenance, ResolvedFile, ResolvedPaths};
use crate::persistence::plugin_inventory::{MANIFEST_FILE_NAME, PluginInventory, scan};
use crate::persistence::plugin_roots::{PluginRoot, PluginRootKind};
use crate::persistence::settings_edit::SettingsCandidate;
use crate::persistence::writer::ExpectedHash;
use crate::published_workbench::PublishedWorkbench;
use crate::runtime::provider::Containment;
use crate::runtime::provider::protocol::HostLocal;
use crate::startup_candidate::{WorkbenchCandidateRequest, build_workbench_candidate};

use super::provider_panels::PanelLifecycle;
use super::settings_registry_validation::registry_refusals;
use crate::workbench::ActivationValues;

struct PackageVersion<'a> {
    version: &'a str,
    screen: &'a str,
    action: &'a str,
    arguments: &'a str,
    binding_context: &'a str,
    binding: &'a str,
    malformed_screen: bool,
    path_activation: bool,
}

fn provider_manifest(package: &PackageVersion<'_>) -> String {
    let activation_fields = if package.path_activation {
        r#"[{ "id": "path", "label": "Path", "type": "path", "required": true, "restart": "none" }]"#
    } else {
        "[]"
    };
    format!(
        r#"{{
          "manifest_schema": 1,
          "id": "vendor.demo",
          "version": "{}",
          "display_name": "Vendor demo",
          "host_api": {{ "minimum": "0.0.1", "maximum": "99.0.0" }},
          "protocol": 1,
          "provider": {{
            "mode": "persistent",
            "binaries": {{ "{}": "bin/provider" }}
          }},
          "actions": [{{
            "id": "{}",
            "label": "Run",
            "description": "Run the provider action",
            "category": "vendor",
            "contexts": ["vendor.demo.main"],
            "arguments": {},
            "timeout_seconds": 30,
            "destructive": false,
            "confirmation": "none",
            "handler": "run",
            "allowed_outcomes": []
          }}],
          "panels": [{{
            "id": "vendor.demo.panel",
            "model_kinds": ["list"],
            "event_schema": [
              {{ "kind": "selected", "arguments": [] }},
              {{ "kind": "retry", "arguments": [] }}
            ],
            "handler": "panel",
            "ports": []
          }}],
          "routes": [{{
            "id": "vendor.demo.open",
            "activation_fields": {activation_fields},
            "target_screen": "{}"
          }}],
          "screens": [{{
            "path": "screens/main.screen.toml",
            "screen_ids": ["{}"]
          }}]
        }}"#,
        package.version,
        HostTriple::current().as_str(),
        package.action,
        package.arguments,
        package.screen,
        package.screen,
    )
}

fn screen_definition(package: &PackageVersion<'_>) -> String {
    if package.malformed_screen {
        return "screen_schema = [not-valid".to_owned();
    }
    let activation = if package.path_activation {
        "[[activation]]\nname = \"path\"\ntype = \"path\"\n"
    } else {
        ""
    };
    format!(
        r#"screen_schema = 1
id = "{}"
title = "Provider screen"
route = "vendor.demo.open"
initial_focus = "main"
focus_order = ["main"]

[[bindings]]
context = "{}"
action = "{}"

{activation}
[[panels]]
id = "main"
type = "vendor.demo.panel"
focusable = true
required = true

[layout]
type = "leaf"
panel = "main"
"#,
        package.screen, package.binding_context, package.binding,
    )
}

fn write_package(root: &Path, package: &PackageVersion<'_>) {
    let directory = root.join("vendor.demo").join(package.version);
    fs::create_dir_all(directory.join("screens"))
        .unwrap_or_else(|error| panic!("package directories must write: {error}"));
    fs::write(
        directory.join(MANIFEST_FILE_NAME),
        provider_manifest(package),
    )
    .unwrap_or_else(|error| panic!("package manifest must write: {error}"));
    fs::write(
        directory.join("screens/main.screen.toml"),
        screen_definition(package),
    )
    .unwrap_or_else(|error| panic!("package screen must write: {error}"));
}

fn add_builtin_resource_conflict(root: &Path, package: &PackageVersion<'_>) {
    let resource_conflict = r#"
[[resources]]
type_id = "github.issue"
schema_version = 1
semantic_key = "number"

[[resources.fields]]
id = "number"
label = "Number"
type = "integer"
required = true
"#;
    let definition =
        screen_definition(package).replacen("screen_schema = 1", "screen_schema = 2", 1);
    let screen = root
        .join("vendor.demo")
        .join(package.version)
        .join("screens/main.screen.toml");
    fs::write(screen, format!("{definition}{resource_conflict}"))
        .unwrap_or_else(|error| panic!("conflicting candidate screen must write: {error}"));
}

fn inventory(root: &Path) -> PluginInventory {
    scan(&[PluginRoot::new(root.to_path_buf(), PluginRootKind::User)])
}

fn inventory_from_roots(roots: &[&Path]) -> PluginInventory {
    let roots = roots
        .iter()
        .map(|root| PluginRoot::new((*root).to_path_buf(), PluginRootKind::User))
        .collect::<Vec<_>>();
    scan(&roots)
}

fn paths(root: &Path) -> ResolvedPaths {
    let resolved = |name: &str| ResolvedFile {
        path: root.join(name),
        provenance: PathProvenance::ConfigArgument,
        sources: Vec::new(),
    };
    ResolvedPaths {
        settings: resolved("settings.toml"),
        state: resolved("state.json"),
        definitions: root.join("definitions"),
        plugins: root.join("plugins"),
        themes: root.join("themes"),
    }
}

fn settings_candidate(
    inventory: &PluginInventory,
    version: &str,
    action: &str,
) -> SettingsCandidate {
    settings_candidate_with_enabled(inventory, version, action, true)
}

fn settings_candidate_with_enabled(
    inventory: &PluginInventory,
    version: &str,
    action: &str,
    enabled: bool,
) -> SettingsCandidate {
    settings_candidate_with_options(
        inventory,
        version,
        "vendor.demo.main",
        action,
        enabled,
        "[\"v\"]",
    )
}

fn settings_candidate_with_options(
    inventory: &PluginInventory,
    version: &str,
    context: &str,
    action: &str,
    enabled: bool,
    chords: &str,
) -> SettingsCandidate {
    let source = format!(
        "settings_schema = 2\n[plugins.\"vendor.demo\"]\nenabled = {enabled}\nversion = \"{version}\"\n[keymap.\"{context}\"]\n\"{action}\" = {chords}\n"
    );
    settings_candidate_from_source(inventory, &source)
}

fn settings_candidate_from_source(inventory: &PluginInventory, source: &str) -> SettingsCandidate {
    let catalog = crate::config_owners::owner_catalog_with_packages(inventory.packages())
        .unwrap_or_else(|error| panic!("owner catalog fixture: {error}"));
    let migration = migrate_settings(source.as_bytes(), &catalog)
        .unwrap_or_else(|diagnostics| panic!("settings fixture must load: {diagnostics:?}"));
    SettingsCandidate::from_edits(
        &migration,
        &catalog,
        &[],
        ExpectedHash::Present(Sha256::digest(source.as_bytes())),
    )
    .unwrap_or_else(|diagnostics| panic!("settings candidate must publish: {diagnostics:?}"))
}

fn current_workbench(
    paths: &ResolvedPaths,
    inventory: &PluginInventory,
    settings: &SettingsCandidate,
) -> PublishedWorkbench {
    build_workbench_candidate(&WorkbenchCandidateRequest {
        paths,
        inventory,
        settings: settings.published(),
        host: HostTriple::current(),
        containment: Containment {
            home: PathBuf::new(),
            tmpdir: PathBuf::new(),
            working_dir: PathBuf::new(),
            locale: "C".to_owned(),
            host_api: crate::VERSION.to_owned(),
        },
    })
    .unwrap_or_else(|error| panic!("current workbench fixture must compose: {error}"))
}

fn version_pair<'a>(v1_binding: &'a str, v2_binding: &'a str) -> [PackageVersion<'a>; 2] {
    [
        PackageVersion {
            version: "1.0.0",
            screen: "vendor.demo.screen",
            action: "vendor.demo.old",
            arguments: "[]",
            binding_context: "vendor.demo.main",
            binding: v1_binding,
            malformed_screen: false,
            path_activation: false,
        },
        PackageVersion {
            version: "2.0.0",
            screen: "vendor.demo.screen",
            action: "vendor.demo.new",
            arguments: "[]",
            binding_context: "vendor.demo.main",
            binding: v2_binding,
            malformed_screen: false,
            path_activation: false,
        },
    ]
}

#[test]
fn package_screen_ordered_first_does_not_acquire_dashboard_action_authority() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp directory: {error}"));
    let package = &version_pair("vendor.demo.old", "vendor.demo.new")[0];
    write_package(root.path(), package);
    let inventory = inventory(root.path());
    let paths = paths(root.path());
    let source = format!(
        "settings_schema = 2\n[plugins.\"vendor.demo\"]\nenabled = true\nversion = \"{}\"\n[workbench]\nscreen_order = [\"vendor.demo.screen\", \"core.dashboard\"]\n[keymap.\"vendor.demo.main\"]\n\"vendor.demo.old\" = [\"v\"]\n",
        package.version
    );
    let settings = settings_candidate_from_source(&inventory, &source);
    let workbench = current_workbench(&paths, &inventory, &settings);
    let first = workbench
        .screen_registry()
        .initial_screen()
        .unwrap_or_else(|| panic!("candidate must have an initial screen"));
    assert_eq!(first.id.as_str(), "vendor.demo.screen");

    let state = crate::state::AppState::new(std::sync::Arc::new(workbench));
    assert_eq!(state.screen().as_str(), "vendor.demo.screen");
    assert!(!state.has_dashboard_action_context());
}

#[test]
fn candidate_version_rejects_its_changed_invalid_screen_binding() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp directory: {error}"));
    let packages = version_pair("vendor.demo.old", "vendor.demo.missing");
    for package in &packages {
        write_package(root.path(), package);
    }
    let inventory = inventory(root.path());
    let paths = paths(root.path());
    let current = settings_candidate(&inventory, "1.0.0", "vendor.demo.old");
    let workbench = current_workbench(&paths, &inventory, &current);
    let candidate = settings_candidate(&inventory, "2.0.0", "vendor.demo.new");

    let refusals = registry_refusals(&candidate, &workbench);

    assert!(refusals.iter().any(|diagnostic| {
        diagnostic.redacted_detail.contains("vendor.demo.screen")
            && diagnostic.redacted_detail.contains("UnknownAction")
    }));
}

#[test]
fn candidate_version_uses_its_own_valid_screen_instead_of_the_committed_descriptor() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp directory: {error}"));
    let packages = version_pair("vendor.demo.old", "vendor.demo.new");
    for package in &packages {
        write_package(root.path(), package);
    }
    let inventory = inventory(root.path());
    let paths = paths(root.path());
    let current = settings_candidate(&inventory, "1.0.0", "vendor.demo.old");
    let workbench = current_workbench(&paths, &inventory, &current);
    let candidate = settings_candidate(&inventory, "2.0.0", "vendor.demo.new");

    assert!(registry_refusals(&candidate, &workbench).is_empty());
}

#[test]
fn startup_refuses_key_binding_to_provider_action_with_declared_arguments() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp directory: {error}"));
    let mut packages = version_pair("vendor.demo.old", "vendor.demo.new");
    packages[0].arguments = r#"[{ "id": "target", "label": "Target", "type": "string", "required": true, "restart": "none" }]"#;
    write_package(root.path(), &packages[0]);
    let inventory = inventory(root.path());
    let paths = paths(root.path());
    let settings = settings_candidate(&inventory, "1.0.0", "vendor.demo.old");

    let Err(failure) = build_workbench_candidate(&WorkbenchCandidateRequest {
        paths: &paths,
        inventory: &inventory,
        settings: settings.published(),
        host: HostTriple::current(),
        containment: Containment {
            home: PathBuf::new(),
            tmpdir: PathBuf::new(),
            working_dir: PathBuf::new(),
            locale: "C".to_owned(),
            host_api: crate::VERSION.to_owned(),
        },
    }) else {
        panic!("a key dispatch cannot supply provider action arguments");
    };

    assert!(matches!(
        failure,
        crate::startup_candidate::WorkbenchStaticFailure::Actions(_)
    ));
    assert!(
        failure
            .to_string()
            .contains("cannot invoke provider action with declared arguments")
    );
}

#[test]
fn settings_candidate_refuses_key_binding_to_provider_action_with_declared_arguments() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp directory: {error}"));
    let mut packages = version_pair("vendor.demo.old", "vendor.demo.new");
    packages[1].arguments = r#"[{ "id": "target", "label": "Target", "type": "string", "required": true, "restart": "none" }]"#;
    for package in &packages {
        write_package(root.path(), package);
    }
    let inventory = inventory(root.path());
    let paths = paths(root.path());
    let current = settings_candidate(&inventory, "1.0.0", "vendor.demo.old");
    let workbench = current_workbench(&paths, &inventory, &current);
    let candidate = settings_candidate(&inventory, "2.0.0", "vendor.demo.new");

    let refusals = registry_refusals(&candidate, &workbench);

    assert!(refusals.iter().any(|diagnostic| {
        diagnostic
            .redacted_detail
            .contains("cannot invoke provider action with declared arguments")
    }));
}

#[test]
fn candidate_new_screen_absent_from_committed_registry_is_validated() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp directory: {error}"));
    let mut packages = version_pair("vendor.demo.old", "vendor.demo.missing");
    packages[1].screen = "vendor.demo.replacement";
    for package in &packages {
        write_package(root.path(), package);
    }
    let inventory = inventory(root.path());
    let paths = paths(root.path());
    let current = settings_candidate(&inventory, "1.0.0", "vendor.demo.old");
    let workbench = current_workbench(&paths, &inventory, &current);
    let candidate = settings_candidate(&inventory, "2.0.0", "vendor.demo.new");

    let refusals = registry_refusals(&candidate, &workbench);

    assert!(refusals.iter().any(|diagnostic| {
        diagnostic
            .redacted_detail
            .contains("vendor.demo.replacement")
            && diagnostic.redacted_detail.contains("UnknownAction")
    }));
}

#[test]
fn candidate_reports_independent_registry_and_layout_refusals_together() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp directory: {error}"));
    let packages = version_pair("vendor.demo.old", "vendor.demo.new");
    for package in &packages {
        write_package(root.path(), package);
    }
    add_builtin_resource_conflict(root.path(), &packages[1]);
    let inventory = inventory(root.path());
    let paths = paths(root.path());
    let current = settings_candidate(&inventory, "1.0.0", "vendor.demo.old");
    let workbench = current_workbench(&paths, &inventory, &current);
    let source = r#"settings_schema = 2
[plugins."vendor.demo"]
enabled = true
version = "2.0.0"
[keymap."vendor.demo.main"]
"vendor.demo.new" = []
[workbench.layout_overrides]
"core.dashboard" = { type = "leaf", panel = "missing" }
"core.repositories" = { type = "leaf", panel = "missing" }
"#;
    let candidate = settings_candidate_from_source(&inventory, source);
    assert_eq!(candidate.published().workbench.layout_overrides.len(), 2);

    let refusals = registry_refusals(&candidate, &workbench);

    assert_eq!(
        refusals.len(),
        4,
        "all independent owners answer: {refusals:?}"
    );
    assert!(refusals.iter().any(|diagnostic| {
        diagnostic
            .redacted_detail
            .contains("resource schema github.issue@1 is declared twice")
    }));
    assert!(
        refusals
            .iter()
            .any(|diagnostic| diagnostic.redacted_detail.contains("DeclaredUnbound"))
    );
    for screen in ["core.dashboard", "core.repositories"] {
        assert!(refusals.iter().any(|diagnostic| {
            diagnostic.path.as_str() == format!("/workbench/layout_overrides/{screen}")
                && diagnostic.redacted_detail.contains("SCR-E")
        }));
    }
    assert!(refusals.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[test]
fn candidate_screen_lowering_diagnostic_is_a_settings_refusal() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp directory: {error}"));
    let mut packages = version_pair("vendor.demo.old", "vendor.demo.new");
    packages[1].malformed_screen = true;
    for package in &packages {
        write_package(root.path(), package);
    }
    let inventory = inventory(root.path());
    let paths = paths(root.path());
    let current = settings_candidate(&inventory, "1.0.0", "vendor.demo.old");
    let workbench = current_workbench(&paths, &inventory, &current);
    let candidate = settings_candidate(&inventory, "2.0.0", "vendor.demo.new");

    let refusals = registry_refusals(&candidate, &workbench);

    assert_eq!(refusals.len(), 1);
    assert_eq!(
        refusals[0].code,
        crate::persistence::diagnostic::CfgCode::E006
    );
    assert!(
        refusals[0]
            .path
            .as_str()
            .ends_with("vendor.demo/2.0.0/screens/main.screen.toml")
    );
    assert_eq!(refusals[0].redacted_detail, "invalid array\nexpected `]`");
    assert_eq!(refusals[0].span, Some(crate::domain::ByteSpan::new(17, 18)));
}

#[test]
fn disabling_a_package_screen_allows_the_same_compiled_action_unbind() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp directory: {error}"));
    let package = PackageVersion {
        version: "1.0.0",
        screen: "vendor.demo.screen",
        action: "vendor.demo.open-action",
        arguments: "[]",
        binding_context: "dashboard",
        binding: "dashboard.open-errors",
        malformed_screen: false,
        path_activation: false,
    };
    write_package(root.path(), &package);
    let inventory = inventory(root.path());
    let paths = paths(root.path());
    let current = settings_candidate(&inventory, "1.0.0", "vendor.demo.open-action");
    let workbench = current_workbench(&paths, &inventory, &current);
    let active = settings_candidate_with_options(
        &inventory,
        "1.0.0",
        "dashboard",
        "dashboard.open-errors",
        true,
        "[]",
    );
    let disabled = settings_candidate_with_options(
        &inventory,
        "1.0.0",
        "dashboard",
        "dashboard.open-errors",
        false,
        "[]",
    );

    assert!(!registry_refusals(&active, &workbench).is_empty());
    let disabled_refusals = registry_refusals(&disabled, &workbench);
    assert!(
        disabled_refusals.is_empty(),
        "disabled package screen must not retain action ownership: {disabled_refusals:#?}"
    );
}

#[test]
fn candidate_validation_uses_startup_captured_screen_bytes_after_disk_changes() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp directory: {error}"));
    let packages = version_pair("vendor.demo.old", "vendor.demo.new");
    for package in &packages {
        write_package(root.path(), package);
    }
    let inventory = inventory(root.path());
    let paths = paths(root.path());
    let current = settings_candidate(&inventory, "1.0.0", "vendor.demo.old");
    let workbench = current_workbench(&paths, &inventory, &current);
    let candidate = settings_candidate(&inventory, "2.0.0", "vendor.demo.new");
    let dormant_screen = root
        .path()
        .join("vendor.demo/2.0.0/screens/main.screen.toml");
    fs::remove_file(&dormant_screen)
        .unwrap_or_else(|error| panic!("dormant screen must be removable: {error}"));

    assert!(registry_refusals(&candidate, &workbench).is_empty());
}

#[test]
fn candidate_missing_exact_package_version_is_refused_by_shared_selection() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp directory: {error}"));
    let packages = version_pair("vendor.demo.old", "vendor.demo.new");
    for package in &packages {
        write_package(root.path(), package);
    }
    let inventory = inventory(root.path());
    let paths = paths(root.path());
    let current = settings_candidate(&inventory, "1.0.0", "vendor.demo.old");
    let workbench = current_workbench(&paths, &inventory, &current);
    let candidate = settings_candidate(&inventory, "3.0.0", "vendor.demo.new");

    let refusals = registry_refusals(&candidate, &workbench);

    assert_eq!(refusals.len(), 1);
    assert!(
        refusals[0]
            .redacted_detail
            .contains("no installed package provides")
    );
}

#[test]
fn candidate_ambiguous_exact_package_version_is_refused_by_shared_selection() {
    let first = tempfile::tempdir().unwrap_or_else(|error| panic!("first root: {error}"));
    let second = tempfile::tempdir().unwrap_or_else(|error| panic!("second root: {error}"));
    let packages = version_pair("vendor.demo.old", "vendor.demo.new");
    for package in &packages {
        write_package(first.path(), package);
    }
    write_package(second.path(), &packages[1]);
    let candidate_inventory = inventory(first.path());
    let current = settings_candidate(&candidate_inventory, "1.0.0", "vendor.demo.old");
    let candidate = settings_candidate(&candidate_inventory, "2.0.0", "vendor.demo.new");
    let retained_inventory = inventory_from_roots(&[first.path(), second.path()]);
    let paths = paths(first.path());
    let workbench = current_workbench(&paths, &retained_inventory, &current);

    let refusals = registry_refusals(&candidate, &workbench);

    assert_eq!(refusals.len(), 1);
    assert!(
        refusals[0]
            .redacted_detail
            .contains("distinct installed packages")
    );
}

#[test]
fn candidate_unavailable_exact_package_version_is_refused_by_shared_selection() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp directory: {error}"));
    let packages = version_pair("vendor.demo.old", "vendor.demo.new");
    for package in &packages {
        write_package(root.path(), package);
    }
    let candidate_inventory = inventory(root.path());
    let current = settings_candidate(&candidate_inventory, "1.0.0", "vendor.demo.old");
    let candidate = settings_candidate(&candidate_inventory, "2.0.0", "vendor.demo.new");
    fs::write(
        root.path()
            .join("vendor.demo/2.0.0")
            .join(MANIFEST_FILE_NAME),
        "not-json",
    )
    .unwrap_or_else(|error| panic!("manifest must become unavailable: {error}"));
    let retained_inventory = inventory(root.path());
    let paths = paths(root.path());
    let workbench = current_workbench(&paths, &retained_inventory, &current);

    let refusals = registry_refusals(&candidate, &workbench);

    assert_eq!(refusals.len(), 1);
    assert!(refusals[0].redacted_detail.contains("unavailable package"));
}

#[cfg(unix)]
#[test]
fn failed_provider_panel_preparation_leaves_navigation_and_runtime_unchanged() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::sync::Arc;

    use crate::domain::Id;
    use crate::workbench::{ActivationValue, ActivationValues, PluginScreenId, ScreenIdentity};

    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp directory: {error}"));
    let package = PackageVersion {
        version: "1.0.0",
        screen: "vendor.demo.screen",
        action: "vendor.demo.run",
        arguments: "[]",
        binding_context: "vendor.demo.main",
        binding: "vendor.demo.run",
        malformed_screen: false,
        path_activation: true,
    };
    write_package(root.path(), &package);
    let inventory = inventory(root.path());
    let settings = settings_candidate(&inventory, package.version, package.action);
    let published = Arc::new(current_workbench(
        &paths(root.path()),
        &inventory,
        &settings,
    ));
    let identity = ScreenIdentity::Package(
        PluginScreenId::parse(package.screen)
            .unwrap_or_else(|error| panic!("package screen: {error}")),
    );
    let route = published
        .screen_registry()
        .get_identity(identity)
        .unwrap_or_else(|| panic!("package screen must publish"))
        .route;
    let mut state = super::AppState::new(published);
    let prior_instance = state.nav.current().id;
    let prior_pending = state.pending_effects.iter().count();
    let path_id = Id::parse("path").unwrap_or_else(|error| panic!("path field: {error}"));
    let values = ActivationValues::new([(
        path_id,
        ActivationValue::Path(PathBuf::from(OsString::from_vec(vec![0xff]))),
    )])
    .unwrap_or_else(|error| panic!("bounded activation: {error}"));

    state.enter_provider_route(route, values);

    assert_eq!(state.nav.current().id, prior_instance);
    assert_eq!(state.pending_effects.iter().count(), prior_pending);
    assert!(
        state
            .provider_panels()
            .panels_for_screen(prior_instance.get())
            .is_empty()
    );
    assert_eq!(
        state.error_message.as_deref(),
        Some("NAV-E001: activation path is not UTF-8")
    );
}

pub(super) fn active_package_provider_fixture() -> (
    super::AppState,
    crate::domain::input_context::ContextStack,
    crate::domain::keymap::Chord,
    crate::domain::action_registry::ActionId,
) {
    use std::sync::Arc;

    use crate::domain::action_registry::ActionId;
    use crate::domain::input_context::ContextStack;
    use crate::domain::keymap::Chord;
    use crate::workbench::{ActivationValues, PluginScreenId, ScreenIdentity};

    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp directory: {error}"));
    let package = PackageVersion {
        version: "1.0.0",
        screen: "vendor.demo.screen",
        action: "vendor.demo.run",
        arguments: "[]",
        binding_context: "vendor.demo.main",
        binding: "vendor.demo.run",
        malformed_screen: false,
        path_activation: false,
    };
    write_package(root.path(), &package);
    let inventory = inventory(root.path());
    let settings = settings_candidate(&inventory, package.version, package.action);
    let published = Arc::new(current_workbench(
        &paths(root.path()),
        &inventory,
        &settings,
    ));
    let identity = ScreenIdentity::Package(
        PluginScreenId::parse(package.screen)
            .unwrap_or_else(|error| panic!("package screen: {error}")),
    );
    let route = published
        .screen_registry()
        .get_identity(identity)
        .unwrap_or_else(|| panic!("package screen must publish"))
        .route;
    let mut state = super::AppState::new(Arc::clone(&published));
    state.enter_provider_route(route, ActivationValues::default());
    let stack = ContextStack::from_ordered(["workbench", "global"], false)
        .unwrap_or_else(|error| panic!("context stack: {error}"));
    let chord = Chord::parse("v").unwrap_or_else(|error| panic!("provider chord: {error}"));
    let action =
        ActionId::parse(package.action).unwrap_or_else(|error| panic!("provider action: {error}"));
    (state, stack, chord, action)
}

fn provider_form_host_local() -> HostLocal {
    let query_id =
        Id::parse("vendor.demo.query").unwrap_or_else(|error| panic!("query id: {error}"));
    let mut form_draft = TypedMap::new();
    form_draft.insert(
        query_id.clone(),
        TypedValue::String("first-instance query".to_string()),
    );
    HostLocal {
        focus_target: Some(query_id),
        scroll_offset: 7,
        selected_id: None,
        form_draft: Some(form_draft),
    }
}

#[test]
fn provider_panels_are_owned_and_restored_by_the_exact_screen_instance() {
    let (mut state, _, _, _) = active_package_provider_fixture();
    let package_instance = state.nav.current().id;
    assert_eq!(
        state
            .nav
            .current()
            .provider_panels()
            .panels_for_screen(package_instance.get())
            .len(),
        1
    );

    let owner_panel = state
        .provider_panels()
        .panels_for_screen(package_instance.get())[0];
    let owner_host_local = provider_form_host_local();
    state
        .provider_panels_mut()
        .update_host_local(owner_panel, owner_host_local.clone())
        .unwrap_or_else(|error| panic!("host-local update: {error}"));
    let package_route = state.nav.current().activation.route;
    state.enter_provider_route(package_route, ActivationValues::default());

    let current_instance = state.nav.current().id;
    assert_ne!(current_instance, package_instance);
    let current_panels = state
        .provider_panels()
        .panels_for_screen(current_instance.get());
    assert_eq!(current_panels.len(), 1);
    assert_ne!(current_panels[0], owner_panel);
    assert_eq!(state.provider_panels().host_local(current_panels[0]), None);
    assert!(state.provider_panels().lifecycle(owner_panel).is_none());
    let routed_owner = state
        .provider_panels_for_panel_mut(owner_panel)
        .unwrap_or_else(|| panic!("suspended owner panel must remain exactly routable"));
    assert_eq!(
        routed_owner.lifecycle(owner_panel),
        Some(PanelLifecycle::Suspended)
    );

    state.leave_screen();

    assert_eq!(state.nav.current().id, package_instance);
    assert_eq!(
        state
            .nav
            .current()
            .provider_panels()
            .panels_for_screen(package_instance.get())
            .len(),
        1
    );
    assert_eq!(
        state.provider_panels().host_local(owner_panel),
        Some(&owner_host_local)
    );
}

#[test]
fn persistent_owner_failure_marks_current_and_suspended_instances() {
    use crate::domain::Id;
    use crate::workbench::ActivationValues;

    let (mut state, _, _, _) = active_package_provider_fixture();
    let suspended_screen = state.nav.current().id;
    let suspended_panel = state
        .provider_panels()
        .panels_for_screen(suspended_screen.get())[0];
    let route = state.nav.current().activation.route;
    state.enter_provider_route(route, ActivationValues::default());
    let current_screen = state.nav.current().id;
    let current_panel = state
        .provider_panels()
        .panels_for_screen(current_screen.get())[0];
    let owner = Id::parse("vendor.demo").unwrap_or_else(|error| panic!("provider owner: {error}"));

    state.fail_provider_panels_for_owner(&owner);

    assert_eq!(
        state.provider_panels().lifecycle(current_panel),
        Some(PanelLifecycle::Failed)
    );
    assert_eq!(
        state
            .provider_panels_for_panel_mut(suspended_panel)
            .and_then(|panels| panels.lifecycle(suspended_panel)),
        Some(PanelLifecycle::Failed),
        "persistent owner failure must reach a suspended exact owner"
    );
}

#[test]
fn active_package_screen_routes_its_provider_action_and_runtime_unavailability() {
    use crate::domain::Id;
    use crate::domain::action_registry::{
        ActionAvailability, Availability, AvailabilityGeneration, HandlerKey, Resolution,
    };
    use crate::domain::effects::{Correlation, CorrelationId, EffectFamily, SemanticKey};

    let (mut state, stack, chord, action) = active_package_provider_fixture();
    assert_eq!(
        state.resolve_action(&chord, &stack),
        Resolution::Dispatch {
            action: action.clone(),
            handler: HandlerKey::ProviderAction,
        }
    );
    state.action_availability = Some(AvailabilityGeneration::new(
        Correlation {
            correlation_id: CorrelationId::new(705),
            owner: Id::parse("vendor.demo")
                .unwrap_or_else(|error| panic!("provider owner: {error}")),
            screen_generation: 1,
            activation_generation: 1,
            semantic_key: SemanticKey::new(EffectFamily::Provider, "provider-binding"),
        },
        vec![ActionAvailability::new(
            action.clone(),
            Availability::Unavailable {
                reason: "provider unavailable".to_owned(),
            },
        )],
    ));
    assert_eq!(
        state.resolve_action(&chord, &stack),
        Resolution::Unavailable {
            action,
            reason: "provider unavailable".to_owned(),
        }
    );
}
