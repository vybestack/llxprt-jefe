//! Provider availability survives the host's availability refresh
//! (issue #390 CW-10, row CW10-13).
//!
//! The host recomputes availability for every action in the workbench on
//! nearly every input. Its reason table describes compiled actions only, so a
//! provider action it has no opinion about must keep whatever the published
//! workbench composition decided — not silently become available.

use super::*;
use crate::domain::action_registry::{ActionAvailability, ActionId, Availability};
use crate::domain::effects::{
    Effect, EffectCompletion, EffectResponse, ProviderEffect, ProviderResponse,
};
use crate::domain::plugin::HostTriple;
use crate::messages::{AppMessage, RepositoryAgentMessage};
use crate::persistence::paths::{PathProvenance, ResolvedFile, ResolvedPaths};
use crate::persistence::plugin_inventory::{MANIFEST_FILE_NAME, scan};
use crate::persistence::plugin_roots::{PluginRoot, PluginRootKind};
use crate::runtime::provider::Containment;
use crate::startup_candidate::{WorkbenchCandidateRequest, build_workbench_candidate};
use crate::state::transition::TransitionExt;
use std::sync::Arc;

fn action_id(value: &str) -> ActionId {
    let Ok(parsed) = ActionId::parse(value) else {
        panic!("action fixture must parse");
    };
    parsed
}

/// Stage a one-shot `vendor.deploy` package whose manifest either ships a
/// binary for this host or only for an alien one, and return its root.
fn stage_provider_package(with_host_binary: bool) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let binaries = if with_host_binary {
        format!(
            r#"{{ "{}": "bin/provider" }}"#,
            HostTriple::current().as_str()
        )
    } else {
        r#"{ "aarch64-unknown-none-elf": "bin/provider" }"#.to_owned()
    };
    let manifest = format!(
        r#"{{
          "manifest_schema": 1,
          "id": "vendor.deploy",
          "version": "1.0.0",
          "display_name": "Package vendor.deploy",
          "host_api": {{ "minimum": "1.0.0", "maximum": "1.0.0" }},
          "protocol": 1,
          "provider": {{ "mode": "one-shot", "binaries": {binaries} }},
          "actions": [
            {{
              "id": "vendor.deploy.ship",
              "label": "Ship release",
              "description": "Ship the selected release",
              "category": "vendor",
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
    );
    let directory = temp
        .path()
        .join("packages")
        .join("vendor.deploy")
        .join("1.0.0");
    std::fs::create_dir_all(&directory)
        .unwrap_or_else(|error| panic!("staging must succeed: {error}"));
    std::fs::write(directory.join(MANIFEST_FILE_NAME), manifest.as_bytes())
        .unwrap_or_else(|error| panic!("manifest must write: {error}"));
    temp
}

/// One explicit workbench whose published registry carries the provider
/// action `vendor.deploy.ship`, composed exactly as startup would publish
/// it: a one-shot provider package whose manifest either ships a binary for
/// this host or does not.
fn provider_state(with_host_binary: bool) -> AppState {
    let temp = stage_provider_package(with_host_binary);
    let root = temp.path().join("packages");
    let inventory = scan(&[PluginRoot::new(root, PluginRootKind::User)]);
    let catalog = crate::config_owners::owner_catalog_with_packages(inventory.packages())
        .unwrap_or_else(|diagnostics| panic!("owner catalog must build: {diagnostics:?}"));
    let settings = crate::persistence::settings_document::SettingsDocument::parse(
        "settings_schema = 2\n\n[plugins.\"vendor.deploy\"]\nenabled = true\n".as_bytes(),
    )
    .unwrap_or_else(|error| panic!("settings must parse: {error:?}"))
    .publish(&catalog)
    .unwrap_or_else(|diagnostics| panic!("settings must publish: {diagnostics:?}"));

    let file = |name: &str| ResolvedFile {
        path: temp.path().join(name),
        provenance: PathProvenance::ConfigArgument,
        sources: Vec::new(),
    };
    let paths = ResolvedPaths {
        settings: file("settings.toml"),
        state: file("state.json"),
        definitions: temp.path().join("definitions"),
        plugins: temp.path().join("plugins"),
        themes: temp.path().join("themes"),
    };
    let candidate = build_workbench_candidate(&WorkbenchCandidateRequest {
        paths: &paths,
        inventory: &inventory,
        settings: &settings,
        host: HostTriple::current(),
        containment: Containment {
            home: temp.path().join("home"),
            tmpdir: temp.path().join("tmp"),
            working_dir: temp.path().join("work"),
            locale: "C".to_owned(),
            host_api: "1.0.0".to_owned(),
        },
    })
    .unwrap_or_else(|error| panic!("provider workbench must compose: {error}"));
    AppState::new(Arc::new(candidate))
}

/// The regression that matters: an action whose provider has no binary for this
/// host must not be re-published as available by a refresh that never heard of
/// it. Doing so would offer the operator an action that cannot possibly run.
#[test]
fn an_unavailable_provider_action_is_not_made_available_by_a_refresh() {
    let reason = format!("no binary for {}", HostTriple::current().as_str());
    let state = provider_state(false);

    let entries = availability_entries(&state, state.action_registry().actions());

    let entry = entries
        .iter()
        .find(|entry| entry.action() == &action_id("vendor.deploy.ship"));
    assert_eq!(
        entry.map(ActionAvailability::availability),
        Some(&Availability::Unavailable { reason }),
        "the refresh must preserve the provider reason composition published"
    );
}

/// An available provider action stays available; preserving is not freezing.
#[test]
fn an_available_provider_action_stays_available() {
    let state = provider_state(true);

    let entries = availability_entries(&state, state.action_registry().actions());

    let entry = entries
        .iter()
        .find(|entry| entry.action() == &action_id("vendor.deploy.ship"));
    assert_eq!(
        entry.map(ActionAvailability::availability),
        Some(&Availability::Available)
    );
}

/// A post-Ready persistent crash overrides startup availability without
/// mutating the immutable action declaration or restarting the provider.
#[test]
fn persistent_health_failure_makes_the_provider_action_unavailable() {
    let mut state = provider_state(true);
    state.provider_action_health.insert(
        action_id("vendor.deploy.ship"),
        "provider stopped after ready".to_owned(),
    );

    let entries = availability_entries(&state, state.action_registry().actions());

    let entry = entries
        .iter()
        .find(|entry| entry.action() == &action_id("vendor.deploy.ship"));
    assert_eq!(
        entry.map(ActionAvailability::availability),
        Some(&Availability::Unavailable {
            reason: "provider stopped after ready".to_owned()
        })
    );
}

/// A post-publication crash is committed only as a runtime generation. The
/// declaration graph and its owning aggregate retain their exact identity.
#[test]
fn provider_crash_updates_only_runtime_availability_generation() {
    let mut initial = provider_state(true);
    let provider_id = action_id("vendor.deploy.ship");
    let workbench = Arc::clone(initial.published_workbench());
    let declarations = initial.action_registry().actions().to_vec();
    initial.provider_action_health.insert(
        provider_id.clone(),
        "provider stopped after ready".to_owned(),
    );

    let transition = initial.apply_message(AppMessage::RepositoryAgent(
        RepositoryAgentMessage::ProjectActionAvailability,
    ));
    let Ok(transition) = transition else {
        panic!("crash availability projection must commit: {transition:?}");
    };
    let Some(issued) = transition.effects.first() else {
        panic!("crash availability projection must stage one effect");
    };
    let Effect::Provider(ProviderEffect::ProjectActionAvailability { entries }) = &issued.effect
    else {
        panic!("crash availability must use the closed provider effect");
    };
    let completion = EffectCompletion {
        correlation: issued.correlation.clone(),
        result: Ok(EffectResponse::Provider(
            ProviderResponse::ActionAvailability {
                entries: entries.clone(),
            },
        )),
    };

    let committed = transition
        .next_state
        .apply_message(AppMessage::EffectCompletion(Box::new(completion)))
        .committed_pure();

    assert!(Arc::ptr_eq(committed.published_workbench(), &workbench));
    assert_eq!(committed.action_registry().actions(), declarations);
    assert!(committed.action_availability_generation().is_some());
    assert_eq!(
        committed.action_availability(&provider_id),
        Some(&Availability::Unavailable {
            reason: "provider stopped after ready".to_owned()
        })
    );
}

#[test]
fn refusing_a_provider_action_retains_identity_for_the_unavailable_surface() {
    let mut state = provider_state(false);
    let provider_id = action_id("vendor.deploy.ship");

    state.record_unavailable_action(
        Some(provider_id.clone()),
        "provider stopped after ready".to_owned(),
    );

    assert_eq!(state.provider_surface_action, Some(provider_id));
}

/// Compiled actions must still be recomputed from host state, or the refresh
/// would stop doing its job.
#[test]
fn compiled_actions_are_still_recomputed_from_host_state() {
    let mut state = provider_state(true);
    state.nav = crate::state::navigation::NavState::rooted(crate::state::ScreenId::Issues);
    state.issues_state.issue_focus = crate::state::IssueFocus::IssueList;

    let entries = availability_entries(&state, state.action_registry().actions());

    let entry = entries
        .iter()
        .find(|entry| entry.action() == &action_id("issues.list-send-agent"));
    assert_eq!(
        entry.map(ActionAvailability::availability),
        Some(&Availability::Unavailable {
            reason: "No issue selected".to_owned()
        })
    );
}
