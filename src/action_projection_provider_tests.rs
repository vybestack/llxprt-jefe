//! Package-provider Help projection (issue #390 CW-10, row CW10-13).
//!
//! A provider action reaches the operator through the same immutable snapshot
//! every compiled action does, so the reason a provider action cannot run must
//! be byte-identical to the reason a refused keybind reports.

use crate::domain::action_registry::{
    Action, ActionAvailability, ActionId, ActionMetadata, ActionRegistrySnapshot, Availability,
    HandlerKey,
};
use crate::domain::input_context::ContextId;

use super::project_provider_help_lines;

fn action_id(value: &str) -> ActionId {
    let Ok(parsed) = ActionId::parse(value) else {
        panic!("action fixture must parse");
    };
    parsed
}

fn provider_action(id: &str, label: &str) -> Action {
    let Ok(context) = ContextId::parse("dashboard") else {
        panic!("context fixture must parse");
    };
    let metadata = ActionMetadata {
        id: action_id(id),
        label: label.to_owned(),
        description: format!("{label} description"),
        category: "vendor".to_owned(),
        contexts: vec![context],
    };
    match Action::new(metadata, HandlerKey::ProviderAction, false) {
        Ok(action) => action,
        Err(error) => panic!("provider action fixture must build: {error:?}"),
    }
}

/// Compose a snapshot carrying exactly the given provider actions.
fn snapshot(entries: Vec<(Action, Availability)>) -> ActionRegistrySnapshot {
    let bytes = b"settings_schema = 2\n";
    let Ok(catalog) = crate::config_owners::builtin_owner_catalog() else {
        panic!("owner catalog fixture must build");
    };
    let settings = match crate::persistence::keymap_edit::load_bytes(Some(bytes), &catalog, "test")
    {
        Ok(keymap) => keymap.settings,
        Err(diagnostics) => panic!("settings fixture must load: {diagnostics:?}"),
    };
    let (actions, availability): (Vec<_>, Vec<_>) = entries
        .into_iter()
        .map(|(action, availability)| {
            let entry = ActionAvailability::new(action.id.clone(), availability);
            (action, entry)
        })
        .unzip();
    match crate::persistence::keymap_edit::compose_published_with_providers(
        &settings,
        "test",
        actions,
        availability,
    ) {
        Ok(composed) => composed.snapshot().clone(),
        Err(error) => panic!("provider composition must succeed: {error}"),
    }
}

#[test]
fn a_snapshot_without_providers_projects_no_package_section() {
    let lines = project_provider_help_lines(&snapshot(Vec::new()));
    assert!(
        lines.is_empty(),
        "no package means no package section, not an empty heading: {lines:?}"
    );
}

#[test]
fn an_available_provider_action_is_listed_with_its_label() {
    let action = provider_action("vendor.pkg.run", "Run vendor task");
    let lines = project_provider_help_lines(&snapshot(vec![(action, Availability::Available)]));

    assert_eq!(lines.first().map(String::as_str), Some("Packages:"));
    assert!(
        lines.iter().any(|line| line.contains("Run vendor task")),
        "the operator must see the action label: {lines:?}"
    );
    assert!(
        lines.iter().all(|line| !line.contains("Unavailable")),
        "an available action must not be reported as unavailable: {lines:?}"
    );
}

#[test]
fn an_unavailable_provider_action_carries_the_snapshot_reason_verbatim() {
    let reason = "no binary for x86_64-unknown-linux-gnu";
    let action = provider_action("vendor.pkg.run", "Run vendor task");
    let lines = project_provider_help_lines(&snapshot(vec![(
        action,
        Availability::Unavailable {
            reason: reason.to_owned(),
        },
    )]));

    assert!(
        lines.iter().any(|line| line.contains(reason)),
        "the reason must be the snapshot's own bytes: {lines:?}"
    );
}

#[test]
fn provider_actions_are_listed_in_deterministic_id_order() {
    let entries = vec![
        (
            provider_action("vendor.zeta.run", "Zeta task"),
            Availability::Available,
        ),
        (
            provider_action("vendor.alpha.run", "Alpha task"),
            Availability::Available,
        ),
    ];
    let lines = project_provider_help_lines(&snapshot(entries));

    let alpha = lines.iter().position(|line| line.contains("Alpha task"));
    let zeta = lines.iter().position(|line| line.contains("Zeta task"));
    assert!(
        matches!((alpha, zeta), (Some(a), Some(z)) if a < z),
        "package rows must be ordered, not left to composition order: {lines:?}"
    );
}

#[test]
fn a_compiled_action_never_appears_in_the_package_section() {
    let action = provider_action("vendor.pkg.run", "Run vendor task");
    let lines = project_provider_help_lines(&snapshot(vec![(action, Availability::Available)]));

    assert!(
        lines.iter().all(|line| !line.contains("help.close")),
        "the package section lists packages only: {lines:?}"
    );
}
