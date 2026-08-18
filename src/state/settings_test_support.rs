use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config_owners::owner_catalog_with_packages;
use crate::domain::plugin::HostTriple;
use crate::persistence::paths::{PathProvenance, ResolvedFile, ResolvedPaths};
use crate::persistence::plugin_inventory::{MANIFEST_FILE_NAME, scan};
use crate::persistence::plugin_roots::{PluginRoot, PluginRootKind};
use crate::persistence::settings_document::SettingsDocument;
use crate::runtime::provider::Containment;
use crate::startup_candidate::{WorkbenchCandidateRequest, build_workbench_candidate};

use super::AppState;

pub(super) fn state_with_config_packages(settings_bytes: &[u8]) -> AppState {
    let root = fixture_root();
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("definitions"))
        .unwrap_or_else(|error| panic!("definitions fixture must exist: {error}"));
    write_package(&root, "1.0.0", "endpoint");
    write_package(&root, "2.0.0", "region");

    let inventory = scan(&[PluginRoot::new(root.clone(), PluginRootKind::User)]);
    let catalog = owner_catalog_with_packages(inventory.packages())
        .unwrap_or_else(|error| panic!("owner catalog fixture must compose: {error}"));
    let document = SettingsDocument::parse(settings_bytes)
        .unwrap_or_else(|error| panic!("settings fixture must parse: {error:?}"));
    let settings = document
        .publish(&catalog)
        .unwrap_or_else(|errors| panic!("settings fixture must publish: {errors:?}"));
    let paths = resolved_paths(&root);
    let workbench = build_workbench_candidate(&WorkbenchCandidateRequest {
        paths: &paths,
        inventory: &inventory,
        settings: &settings,
        host: HostTriple::current(),
        containment: Containment {
            home: root.join("home"),
            tmpdir: root.join("tmp"),
            working_dir: root.join("work"),
            locale: "C".to_owned(),
            host_api: crate::VERSION.to_owned(),
        },
    })
    .unwrap_or_else(|error| panic!("config package workbench must compose: {error}"));
    let _ = fs::remove_dir_all(root);
    AppState::new(Arc::new(workbench))
}

fn fixture_root() -> PathBuf {
    std::env::temp_dir().join(format!(
        "jefe-settings-config-package-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ))
}

fn write_package(root: &Path, version: &str, field: &str) {
    let directory = root.join("vendor.config").join(version);
    fs::create_dir_all(directory.join("bin"))
        .unwrap_or_else(|error| panic!("package fixture must exist: {error}"));
    fs::write(directory.join("bin/provider"), b"fixture")
        .unwrap_or_else(|error| panic!("provider fixture must write: {error}"));
    let manifest = format!(
        r#"{{
  "manifest_schema": 1,
  "id": "vendor.config",
  "version": "{version}",
  "display_name": "Vendor config",
  "host_api": {{ "minimum": "0.0.1", "maximum": "99.0.0" }},
  "protocol": 1,
  "provider": {{
    "mode": "persistent",
    "binaries": {{ "{}": "bin/provider" }}
  }},
  "config": {{
    "schema_version": {},
    "fields": [{{
      "id": "{field}",
      "label": "Field",
      "type": "string",
      "required": true,
      "default": "fixture",
      "restart": "provider"
    }}]
  }},
  "actions": [],
  "panels": [],
  "routes": [],
  "screens": []
}}"#,
        HostTriple::current().as_str(),
        if version == "1.0.0" { 1 } else { 2 },
    );
    fs::write(directory.join(MANIFEST_FILE_NAME), manifest)
        .unwrap_or_else(|error| panic!("manifest fixture must write: {error}"));
}

fn resolved_paths(root: &Path) -> ResolvedPaths {
    let resolved = |name: &str| ResolvedFile {
        path: root.join(name),
        provenance: PathProvenance::ConfigArgument,
        sources: Vec::new(),
    };
    ResolvedPaths {
        settings: resolved("settings.toml"),
        state: resolved("state.json"),
        definitions: root.join("definitions"),
        plugins: root.to_path_buf(),
        themes: root.join("themes"),
    }
}
