use super::*;

fn lowered_provider_binding_registry(
    settings: &crate::persistence::settings_document::PublishedSettings,
) -> crate::workbench::ScreenRegistry {
    let definition = with_binding(&review_definition(), "vendor.context", "vendor.action");
    let fixture = CandidateFixture::new("binding-provider", &definition);
    let paths = fixture.paths();
    crate::startup_screens::compose(&paths, &[], settings)
        .unwrap_or_else(|error| panic!("typed declaration must lower: {error}"))
        .registry
}

fn provider_binding_action(
    action_id: crate::domain::action_registry::ActionId,
    context: crate::domain::input_context::ContextId,
) -> crate::domain::action_registry::Action {
    use crate::domain::action_registry::{Action, ActionMetadata, HandlerKey};

    Action::new(
        ActionMetadata {
            id: action_id,
            label: "Vendor action".to_owned(),
            description: "Provider-contributed action".to_owned(),
            category: "Vendor".to_owned(),
            contexts: vec![context],
        },
        HandlerKey::ProviderAction,
        false,
    )
    .unwrap_or_else(|error| panic!("provider action metadata: {error}"))
}

#[test]
fn final_composition_accepts_and_resolves_a_provider_declared_binding() {
    use crate::domain::action_registry::{
        ActionAvailability, ActionId, Availability, HandlerKey, Resolution,
    };
    use crate::domain::input_context::ContextId;
    use crate::domain::keymap::Chord;
    use crate::persistence::keymap_edit::compose_published_with_providers;

    let mut settings = enabled_settings();
    settings
        .keymap
        .entry("vendor.context".to_owned())
        .or_default()
        .insert("vendor.action".to_owned(), vec!["v".to_owned()]);
    let registry = lowered_provider_binding_registry(&settings);
    let action_id =
        ActionId::parse("vendor.action").unwrap_or_else(|error| panic!("provider action: {error}"));
    let context = ContextId::parse("vendor.context")
        .unwrap_or_else(|error| panic!("provider context: {error}"));
    let action = provider_binding_action(action_id.clone(), context.clone());
    let published = compose_published_with_providers(
        &settings,
        "provider binding test",
        vec![action],
        vec![ActionAvailability::new(
            action_id.clone(),
            Availability::Available,
        )],
    )
    .unwrap_or_else(|error| panic!("provider action must compose: {error}"));
    crate::startup_candidate::validate_screen_bindings(
        &registry,
        published.snapshot(),
        &crate::runtime::provider::ProviderCatalog::new(),
    )
    .unwrap_or_else(|error| panic!("provider declaration must validate: {error}"));
    let chord = Chord::parse("v").unwrap_or_else(|error| panic!("provider chord: {error}"));
    assert_eq!(
        published
            .snapshot()
            .resolve_declared(&chord, &context, &action_id),
        Resolution::Dispatch {
            action: action_id,
            handler: HandlerKey::ProviderAction,
        }
    );
}
