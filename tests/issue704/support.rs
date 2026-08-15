//! Shared staging fixtures for the issue #704 candidate suites.
//!
//! Everything here exists so a test stages a physical package directory,
//! publishes settings exactly the way startup does, and composes the
//! candidate with one call — without any test inventing its own inventory
//! semantics. The helpers write only inside the test's temp directory.

use std::path::{Path, PathBuf};

use jefe::domain::plugin::HostTriple;
use jefe::persistence::paths::resolve;
use jefe::persistence::plugin_inventory::{MANIFEST_FILE_NAME, PluginInventory};
use jefe::persistence::plugin_roots::{PluginRoot, PluginRootKind};
use jefe::persistence::settings_document::{PublishedSettings, SettingsDocument};
use jefe::published_workbench::PublishedWorkbench;
use jefe::runtime::provider::Containment;
use jefe::startup_candidate::{
    WorkbenchCandidateRequest, WorkbenchStaticFailure, build_workbench_candidate,
};
use jefe::startup_selection::SelectionRefused;

/// One package to stage: manifest shape plus which declarations it owns.
pub struct PackageSpec<'a> {
    pub(crate) id: &'a str,
    pub(crate) version: &'a str,
    /// `none`, `one-shot`, or `persistent`.
    pub(crate) mode: &'a str,
    pub(crate) actions: bool,
    pub(crate) config: bool,
}

impl PackageSpec<'_> {
    pub(crate) fn persistent_actions(id: &'static str) -> Self {
        Self {
            id,
            version: "1.0.0",
            mode: "persistent",
            actions: true,
            config: false,
        }
    }

    pub(crate) fn one_shot(id: &'static str) -> Self {
        Self {
            id,
            version: "1.0.0",
            mode: "one-shot",
            actions: true,
            config: false,
        }
    }

    fn action_json(&self) -> String {
        format!(
            r#"{{
              "id": "{}.run",
              "label": "Run {}",
              "description": "Run the {} action",
              "category": "{}",
              "contexts": ["dashboard"],
              "arguments": [],
              "timeout_seconds": 60,
              "destructive": false,
              "confirmation": "none",
              "handler": "run",
              "allowed_outcomes": ["notice"]
            }}"#,
            self.id, self.id, self.id, self.id,
        )
    }
}

/// The host triple this test runs on, so staged binaries are selectable.
pub fn host() -> HostTriple {
    HostTriple::current()
}

pub fn host_binaries() -> String {
    format!(r#"{{ "{}": "bin/provider" }}"#, host().as_str())
}

/// A triple no test host is, for the required-but-unavailable row.
const ALIEN_TRIPLE: &str = "aarch64-unknown-none-elf";

pub fn alien_binaries() -> String {
    format!(r#"{{ "{ALIEN_TRIPLE}": "bin/provider" }}"#)
}

pub fn manifest_of(spec: &PackageSpec<'_>, binaries: &str) -> String {
    let actions = if spec.actions {
        format!("\"actions\": [{}],", spec.action_json())
    } else {
        "\"actions\": [],".to_owned()
    };
    let config = if spec.config {
        r#""config": {
          "schema_version": 1,
          "fields": [
            { "id": "mode", "label": "Mode", "type": "string", "required": false, "restart": "none" }
          ]
        },"#
    } else {
        ""
    };
    format!(
        r#"{{
          "manifest_schema": 1,
          "id": "{}",
          "version": "{}",
          "display_name": "Package {}",
          "host_api": {{ "minimum": "1.0.0", "maximum": "1.0.0" }},
          "protocol": 1,
          "provider": {{ "mode": "{}", "binaries": {} }},
          {config}
          {actions}
          "panels": [],
          "routes": [],
          "screens": []
        }}"#,
        spec.id, spec.version, spec.id, spec.mode, binaries,
    )
}

/// Write one `<id>/<version>/plugin.json` package directory under a root.
pub fn stage(root: &Path, spec: &PackageSpec<'_>, binaries: &str) -> PathBuf {
    let directory = root.join(spec.id).join(spec.version);
    std::fs::create_dir_all(&directory)
        .unwrap_or_else(|error| panic!("staging must succeed: {error}"));
    std::fs::write(
        directory.join(MANIFEST_FILE_NAME),
        manifest_of(spec, binaries),
    )
    .unwrap_or_else(|error| panic!("manifest must write: {error}"));
    directory
}

pub fn scan_roots(roots: &[PathBuf]) -> PluginInventory {
    let ordered = roots
        .iter()
        .map(|root| PluginRoot::new(root.clone(), PluginRootKind::User))
        .collect::<Vec<_>>();
    jefe::persistence::plugin_inventory::scan(&ordered)
}

pub fn publish_settings(inventory: &PluginInventory, source: &str) -> PublishedSettings {
    let catalog = jefe::config_owners::owner_catalog_with_packages(inventory.packages())
        .unwrap_or_else(|error| panic!("owner catalog must build: {error:?}"));
    SettingsDocument::parse(source.as_bytes())
        .unwrap_or_else(|error| panic!("settings must parse: {error:?}"))
        .publish(&catalog)
        .unwrap_or_else(|error| panic!("settings must publish: {error:?}"))
}

pub fn containment(base: &Path) -> Containment {
    Containment {
        home: base.join("home"),
        tmpdir: base.join("tmp"),
        working_dir: base.join("work"),
        locale: "C".to_owned(),
        host_api: "1.0.0".to_owned(),
    }
}

pub fn build(
    paths: &jefe::persistence::paths::ResolvedPaths,
    inventory: &PluginInventory,
    settings: &PublishedSettings,
    base: &Path,
) -> Result<PublishedWorkbench, WorkbenchStaticFailure> {
    build_workbench_candidate(&WorkbenchCandidateRequest {
        paths,
        inventory,
        settings,
        host: host(),
        containment: containment(base),
    })
}

pub fn config_root(temp: &Path) -> PathBuf {
    temp.join("config")
}

pub fn plugins_root(config: &Path) -> PathBuf {
    config.join("plugins").join("installed")
}

/// Stage a config directory's installed packages and definitions, then scan
/// exactly its plugin root.
pub fn stage_config(temp: &Path, specs: &[(&PackageSpec<'_>, &str)]) -> PluginInventory {
    let root = plugins_root(&config_root(temp));
    for (spec, binaries) in specs {
        stage(&root, spec, binaries);
    }
    scan_roots(&[root])
}

pub fn resolve_paths(config: &Path) -> jefe::persistence::paths::ResolvedPaths {
    resolve(Some(config)).unwrap_or_else(|error| panic!("paths must resolve: {error:?}"))
}

/// Settings text enabling one owner, optionally pinned to a version.
pub fn selection_toml(id: &str, version: Option<&str>) -> String {
    let pin = version.map_or_else(String::new, |version| format!("version = \"{version}\"\n"));
    format!("settings_schema = 2\n\n[plugins.\"{id}\"]\nenabled = true\n{pin}")
}

pub fn selected_owner<'a>(
    candidate: &'a PublishedWorkbench,
    id: &str,
) -> &'a jefe::startup_selection::SelectedOwner {
    candidate
        .selected_owners()
        .iter()
        .find(|owner| owner.owner().as_str() == id)
        .unwrap_or_else(|| panic!("owner {id} must be selected"))
}

pub fn expect_selection_refusal(
    result: Result<PublishedWorkbench, WorkbenchStaticFailure>,
) -> SelectionRefused {
    match result {
        Ok(_) => panic!("an unresolvable active selection must refuse the candidate"),
        Err(WorkbenchStaticFailure::Selection(refusal)) => refusal.clone(),
        Err(other) => panic!("expected a selection refusal, got: {other}"),
    }
}
