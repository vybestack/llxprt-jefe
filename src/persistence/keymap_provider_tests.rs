//! Provider action lowering into the single registry snapshot
//! (issue #390 CW-10, rows CW10-01/CW10-13).

use crate::domain::action_registry::{
    Action, ActionAvailability, ActionId, ActionMetadata, Availability, HandlerKey,
};
use crate::domain::input_context::ContextId;

use super::keymap_edit::compose_published_with_providers;
use super::settings_document::SettingsDocument;

fn published() -> crate::persistence::settings_document::PublishedSettings {
    let bytes = b"settings_schema = 2
";
    let Ok(document) = SettingsDocument::parse(bytes) else {
        panic!("settings fixture must parse");
    };
    let Ok(catalog) = crate::config_owners::builtin_owner_catalog() else {
        panic!("owner catalog fixture must build");
    };
    match crate::persistence::settings_publish::publish_without_keymap(&document, &catalog) {
        Ok(settings) => settings,
        Err(diagnostics) => panic!("settings fixture must publish: {diagnostics:?}"),
    }
}

fn provider_action(id: &str) -> Action {
    let Ok(action_id) = ActionId::parse(id) else {
        panic!("action fixture must parse");
    };
    let Ok(context) = ContextId::parse("dashboard") else {
        panic!("context fixture must parse");
    };
    let metadata = ActionMetadata {
        id: action_id,
        label: "Run".to_owned(),
        description: "Run the provider action".to_owned(),
        category: "vendor".to_owned(),
        contexts: vec![context],
    };
    match Action::new(metadata, HandlerKey::ProviderAction, false) {
        Ok(action) => action,
        Err(error) => panic!("provider action fixture must build: {error:?}"),
    }
}

fn action_id(value: &str) -> ActionId {
    let Ok(parsed) = ActionId::parse(value) else {
        panic!("action fixture must parse");
    };
    parsed
}

#[test]
fn provider_actions_join_the_single_snapshot_with_their_availability() {
    let settings = published();
    let action = provider_action("vendor.pkg.run");
    let availability = vec![ActionAvailability::new(
        action_id("vendor.pkg.run"),
        Availability::Available,
    )];

    let composed = compose_published_with_providers(
        &settings,
        "compiled defaults",
        vec![action],
        availability,
    );

    let Ok(composed) = composed else {
        panic!("provider composition must succeed");
    };
    assert_eq!(
        composed
            .snapshot()
            .availability_of(&action_id("vendor.pkg.run")),
        Some(&Availability::Available)
    );
}

#[test]
fn unavailable_provider_action_keeps_its_exact_reason_in_the_snapshot() {
    let settings = published();
    let action = provider_action("vendor.pkg.run");
    let availability = vec![ActionAvailability::new(
        action_id("vendor.pkg.run"),
        Availability::Unavailable {
            reason: "no binary for x86_64-unknown-linux-gnu".to_owned(),
        },
    )];

    let composed = compose_published_with_providers(
        &settings,
        "compiled defaults",
        vec![action],
        availability,
    );

    let Ok(composed) = composed else {
        panic!("provider composition must succeed");
    };
    assert_eq!(
        composed
            .snapshot()
            .availability_of(&action_id("vendor.pkg.run")),
        Some(&Availability::Unavailable {
            reason: "no binary for x86_64-unknown-linux-gnu".to_owned()
        })
    );
}

#[test]
fn a_provider_action_colliding_with_a_compiled_action_is_refused() {
    let settings = published();
    let action = provider_action("help.close");
    let availability = vec![ActionAvailability::new(
        action_id("help.close"),
        Availability::Available,
    )];

    let composed = compose_published_with_providers(
        &settings,
        "compiled defaults",
        vec![action],
        availability,
    );

    assert!(
        composed.is_err(),
        "a provider must never shadow a compiled action id"
    );
}

#[test]
fn composing_without_providers_matches_the_compiled_snapshot() {
    let settings = published();

    let with_providers =
        compose_published_with_providers(&settings, "compiled defaults", Vec::new(), Vec::new());
    let plain = super::keymap_edit::compose_published(&settings, "compiled defaults");

    match (with_providers, plain) {
        (Ok(left), Ok(right)) => assert_eq!(left.snapshot(), right.snapshot()),
        _ => panic!("both compositions must succeed"),
    }
}
