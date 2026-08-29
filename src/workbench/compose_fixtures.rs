//! Definition-file fixtures shared by the lowering and composition tests
//! (issue #385).

use std::fs;
use std::path::{Path, PathBuf};

use crate::config_owners::owner_catalog_with_packages;
use crate::domain::Id;
use crate::domain::plugin::HostTriple;
use crate::persistence::plugin_inventory::{InstalledPackage, MANIFEST_FILE_NAME, scan};
use crate::persistence::plugin_roots::{PluginRoot, PluginRootKind};
use crate::persistence::screen_files::{
    PackageScreenSources, ScreenFileCandidate, ScreenFileRejection,
};
use crate::persistence::settings_document::{
    PublishedSettings, PublishedWorkbenchSettings, SettingsDocument,
};

use super::compose::{CompositionRefused, ScreenComposition, compose_screens_with_package_sources};
use super::screens::ScreenRegistry;

/// The worked example, as it sits on disk.
const REVIEW_SOURCE: &str = include_str!("testdata/local-review.screen.toml");

/// A complete, valid `local.review` definition.
///
/// It lives beside this file as real TOML rather than as a string literal, so
/// the same bytes can be embedded here and written to disk by the startup
/// tests, and so an author can read it as the worked example it is.
///
/// Line endings are normalized because tests build variants of it with literal
/// multi-line search strings, and a checkout that converted them would turn
/// every such edit into a silent no-op.
#[must_use]
pub fn review_definition() -> String {
    REVIEW_SOURCE.replace("\r\n", "\n")
}

/// A candidate holding the given text under `<root>/<member>.screen.toml`.
pub fn candidate(member: &str, text: &str) -> ScreenFileCandidate {
    ScreenFileCandidate {
        path: PathBuf::from("/definitions").join(format!("{member}.screen.toml")),
        member: member.to_owned(),
        text: Ok(text.to_owned()),
    }
}

/// A candidate whose bytes discovery refused.
pub fn unreadable_candidate(member: &str, rejection: ScreenFileRejection) -> ScreenFileCandidate {
    ScreenFileCandidate {
        path: PathBuf::from("/definitions").join(format!("{member}.screen.toml")),
        member: member.to_owned(),
        text: Err(rejection),
    }
}

/// Published settings enabling exactly the given members.
pub fn enabled(members: &[&str]) -> PublishedSettings {
    PublishedSettings {
        workbench: PublishedWorkbenchSettings {
            enabled_screens: members
                .iter()
                .map(|member| {
                    Id::parse(&format!("local.{member}")).unwrap_or_else(|error| {
                        unreachable!("fixture owner id must parse: {error}")
                    })
                })
                .collect(),
            ..PublishedWorkbenchSettings::default()
        },
        ..PublishedSettings::default()
    }
}

const CONTROL_ORIGIN_SOURCE: &str = include_str!("testdata/local-control-origin.screen.toml");

/// A real local definition that places the selected package's all-nine panel.
#[must_use]
pub fn control_origin_definition() -> String {
    CONTROL_ORIGIN_SOURCE.replace("\r\n", "\n")
}

/// Compose compiled, local, and selected-package control-origin fixtures through
/// the production composition boundary.
#[must_use]
pub fn control_origin_composition(compiled: &ScreenRegistry) -> ScreenComposition {
    try_control_origin_composition(compiled)
        .unwrap_or_else(|error| panic!("control-origin composition: {error}"))
}

/// Try the real control-origin composition boundary for refusal tests.
pub fn try_control_origin_composition(
    compiled: &ScreenRegistry,
) -> Result<ScreenComposition, CompositionRefused> {
    try_control_origin_composition_with_definitions(
        compiled,
        &control_origin_definition(),
        control_package_screen(),
    )
}

/// Try composition with explicit local and package definitions for ownership refusals.
pub fn try_control_origin_composition_with_definitions(
    compiled: &ScreenRegistry,
    local_definition: &str,
    package_definition: &str,
) -> Result<ScreenComposition, CompositionRefused> {
    let temp =
        tempfile::tempdir().unwrap_or_else(|error| panic!("control fixture tempdir: {error}"));
    let root = temp.path().join("packages");
    let host = HostTriple::current();
    write_control_package(&root, &host, package_definition);
    let packages = scan(&[PluginRoot::new(root, PluginRootKind::User)])
        .packages()
        .to_vec();
    assert_eq!(packages.len(), 1, "control fixture package must scan");
    let mut published = control_origin_settings(&packages);
    published.workbench.enabled_screens.push(
        Id::parse("local.control-fixture")
            .unwrap_or_else(|error| unreachable!("fixture screen id must parse: {error}")),
    );
    let sources = PackageScreenSources::capture(&packages);
    compose_screens_with_package_sources(
        compiled,
        &[candidate("control-fixture", local_definition)],
        &packages,
        &sources,
        &published,
    )
}

fn write_control_package(root: &Path, host: &HostTriple, package_definition: &str) {
    let dir = root.join("vendor.fixture").join("1.0.0");
    let screens = dir.join("screens");
    fs::create_dir_all(&screens).unwrap_or_else(|error| panic!("create control fixture: {error}"));
    fs::write(
        dir.join(MANIFEST_FILE_NAME),
        control_manifest(host).as_bytes(),
    )
    .unwrap_or_else(|error| panic!("write control manifest: {error}"));
    fs::write(
        screens.join("control.screen.toml"),
        package_definition.as_bytes(),
    )
    .unwrap_or_else(|error| panic!("write control screen: {error}"));
}

fn control_manifest(host: &HostTriple) -> String {
    format!(
        r#"{{
  "manifest_schema": 1,
  "id": "vendor.fixture",
  "version": "1.0.0",
  "display_name": "Control Fixture",
  "host_api": {{ "minimum": "1.0.0", "maximum": "1.0.0" }},
  "protocol": 1,
  "provider": {{
    "mode": "persistent",
    "binaries": {{ "{}": "bin/provider" }}
  }},
  "actions": [{{
    "id": "vendor.fixture.act",
    "label": "Act",
    "description": "Exercise the control",
    "category": "tests",
    "contexts": ["core.dashboard", "local.control-fixture", "vendor.fixture.screen"],
    "arguments": [],
    "timeout_seconds": 30,
    "destructive": false,
    "confirmation": "none",
    "handler": "vendor.fixture.handler",
    "allowed_outcomes": []
  }}],
  "panels": [{{
    "id": "vendor.fixture.control",
    "model_kinds": ["list", "tree", "detail", "structured-diff", "form", "status", "progress", "empty", "error"],
    "event_schema": [
      {{ "kind": "selected", "arguments": [] }},
      {{ "kind": "action", "arguments": [] }},
      {{ "kind": "submit", "arguments": [] }},
      {{ "kind": "cancel", "arguments": [] }},
      {{ "kind": "retry", "arguments": [] }}
    ],
    "handler": "vendor.fixture.handler",
    "ports": [{{ "id": "vendor.fixture.port" }}]
  }}],
  "routes": [],
  "screens": [{{
    "path": "screens/control.screen.toml",
    "screen_ids": ["vendor.fixture.screen"]
  }}]
}}"#,
        host.as_str()
    )
}

fn control_package_screen() -> &'static str {
    r#"screen_schema = 1
id = "vendor.fixture.screen"
title = "Control Fixture"
route = "vendor.fixture.screen"
initial_focus = "control"
focus_order = ["control"]

[[panels]]
id = "control"
type = "vendor.fixture.control"
focusable = true
required = true

[layout]
type = "leaf"
panel = "control"
"#
}

fn control_origin_settings(packages: &[InstalledPackage]) -> PublishedSettings {
    let catalog = owner_catalog_with_packages(packages)
        .unwrap_or_else(|diagnostics| panic!("control owner catalog: {diagnostics:?}"));
    SettingsDocument::parse(
        b"settings_schema = 2\n\n[plugins.\"vendor.fixture\"]\nenabled = true\nversion = \"1.0.0\"\n",
    )
    .unwrap_or_else(|error| panic!("control settings parse: {error:?}"))
    .publish(&catalog)
    .unwrap_or_else(|diagnostics| panic!("control settings publish: {diagnostics:?}"))
}
