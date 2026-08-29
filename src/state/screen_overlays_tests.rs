use crate::domain::plugin::field::{Field, FieldDraft, FieldKind, RestartScope};
use crate::domain::{Id, TypedValue};
use crate::workbench::descriptor::OverlayKind;

use super::{
    AppEvent, AppState, ModalState,
    provider_requests::ProviderConfirmationIdentity,
    screen_overlays::{ActiveOverlay, ConfirmationRequest, ScreenOverlayState},
    transition::TransitionExt,
};

fn continuation_field(id: &str, kind: FieldKind, default: Option<TypedValue>) -> Field {
    Field::parse(FieldDraft {
        id: Id::parse(id).unwrap_or_else(|error| panic!("field id: {error}")),
        label: id.to_owned(),
        description: None,
        kind,
        required: true,
        default,
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| panic!("field: {error}"))
}

#[test]
fn an_instance_accepts_only_its_declared_host_overlay_kinds() {
    let mut overlays = ScreenOverlayState::new(vec![OverlayKind::Help, OverlayKind::Search]);

    assert!(overlays.open_help());
    assert_eq!(
        overlays.active(),
        Some(&ActiveOverlay::Help { viewport: 0 })
    );
    assert!(overlays.close());
    assert!(
        !overlays.open_generic_confirmation(ConfirmationRequest::DeleteRepository {
            id: crate::domain::RepositoryId("repo".to_owned()),
        })
    );
    assert_eq!(overlays.active(), None);
}

#[test]
fn cloned_suspended_instances_retain_independent_overlay_presentation_state() {
    let mut first = ScreenOverlayState::new(vec![OverlayKind::Search]);
    assert!(first.open_search());
    assert!(first.replace_search("provider health".to_owned(), 8));

    let suspended = first.clone();
    assert!(first.replace_search("other query".to_owned(), 3));

    assert_eq!(
        suspended.active(),
        Some(&ActiveOverlay::Search {
            query: "provider health".to_owned(),
            cursor: 8,
        })
    );
    assert_eq!(
        first.active(),
        Some(&ActiveOverlay::Search {
            query: "other query".to_owned(),
            cursor: 3,
        })
    );
}

#[test]
fn search_query_and_visibility_are_owned_by_the_current_screen_instance() {
    let state = AppState::test_fixture()
        .apply(AppEvent::OpenSearch)
        .committed_pure()
        .apply(AppEvent::FormChar('é'))
        .committed_pure();

    assert_eq!(
        state.active_overlay_kind(),
        Some(crate::workbench::OverlayKind::Search)
    );
    assert_eq!(state.search_query(), Some("é"));

    let state = state
        .apply(AppEvent::FormBackspace)
        .committed_pure()
        .apply(AppEvent::CloseModal)
        .committed_pure();
    assert_eq!(state.search_query(), None);
    assert!(matches!(state.modal, ModalState::None));
}

#[test]
fn dashboard_filter_reads_the_declared_search_overlay_query() {
    let repository = |id: &str, name: &str| {
        crate::domain::Repository::new(
            crate::domain::RepositoryId(id.to_owned()),
            crate::domain::shipped_agent_type(3),
            crate::domain::TypedMap::new(),
            name.to_owned(),
            name.to_owned(),
            std::path::PathBuf::from(format!("/tmp/{id}")),
        )
    };
    let mut state = AppState::test_fixture();
    state.repositories = vec![repository("alpha", "Alpha"), repository("beta", "Beta")];

    let state = state
        .apply(AppEvent::OpenSearch)
        .committed_pure()
        .apply(AppEvent::FormChar('b'))
        .committed_pure();

    assert_eq!(state.visible_repository_indices(), vec![1]);
}

#[test]
fn root_overlay_admission_comes_only_from_its_descriptor_declarations() {
    let mut state = AppState::test_fixture();
    state.nav = super::navigation::NavState::default();

    let state = state.apply(AppEvent::OpenHelp).committed_pure();

    assert_eq!(state.active_overlay_kind(), None);
    assert!(matches!(state.modal, ModalState::None));
}

#[test]
fn provider_confirmation_uses_defaults_without_inventing_missing_typed_values() {
    let fields = vec![
        continuation_field(
            "with-default",
            FieldKind::String,
            Some(TypedValue::String("ready".to_owned())),
        ),
        continuation_field("boolean", FieldKind::Boolean, None),
        continuation_field("secret", FieldKind::SecretReference, None),
    ];
    let mut overlays = ScreenOverlayState::new(vec![OverlayKind::Confirmation]);
    let confirmation_id = Id::parse("confirmation.defaults")
        .unwrap_or_else(|error| panic!("confirmation id: {error}"));

    assert!(overlays.open_provider_confirmation(
        ProviderConfirmationIdentity::test_fixture(confirmation_id),
        &fields,
    ));
    let values = overlays
        .confirmation_values()
        .unwrap_or_else(|| panic!("confirmation values"));
    assert_eq!(values.len(), 1);
    assert_eq!(
        values.get(fields[0].id()),
        Some(&TypedValue::String("ready".to_owned()))
    );
    assert!(!values.contains_key(fields[1].id()));
    assert!(!values.contains_key(fields[2].id()));
}
