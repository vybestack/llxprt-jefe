use super::provider_requests::{ActionPolicy, InvokeInput};
use super::screen_overlays::ConfirmationRequest;
use super::transition::TransitionExt;
use super::{AppEvent, AppState, ModalState, ScreenId};
use crate::domain::plugin::action::{ActionConfirmation, ActionOutcome};
use crate::domain::plugin::field::{Field, FieldDraft, FieldKind, RestartScope};
use crate::domain::{Id, TypedMap, TypedValue};
use crate::messages::{AppMessage, ProviderMessage};
use crate::runtime::provider::protocol::Outcome;
use crate::workbench::OverlayKind;

fn state_with_overlays() -> AppState {
    AppState::new(crate::test_support::published_workbench())
}

fn register_pending_confirmation(state: AppState, confirmation_id: &str) -> AppState {
    register_pending_confirmation_with_schema(state, confirmation_id, Vec::new())
}

fn register_pending_confirmation_with_schema(
    mut state: AppState,
    confirmation_id: &str,
    continuation_schema: Vec<Field>,
) -> AppState {
    let owner = Id::parse("host").unwrap_or_else(|error| panic!("owner: {error}"));
    let action =
        Id::parse("provider.confirm").unwrap_or_else(|error| panic!("provider action: {error}"));
    let screen = Id::parse(state.screen().as_str())
        .unwrap_or_else(|error| panic!("screen identity: {error}"));
    let instance = Id::parse(&state.nav.current().id.to_string())
        .unwrap_or_else(|error| panic!("instance identity: {error}"));
    let policy = ActionPolicy::new(
        ActionConfirmation::ProviderContinuation,
        vec![ActionOutcome::RequestHostConfirmation],
        false,
    );
    let key = state
        .provider_requests
        .invoke(InvokeInput {
            owner: &owner,
            action_id: &action,
            context_screen: &screen,
            context_instance: &instance,
            context_refs: &TypedMap::new(),
            arguments: &TypedMap::new(),
            policy: &policy,
        })
        .unwrap_or_else(|error| panic!("invoke: {error}"))
        .key;

    state
        .apply_message(AppMessage::Provider(Box::new(ProviderMessage::Outcome {
            key,
            outcome: Outcome::RequestHostConfirmation {
                confirmation_id: Id::parse(confirmation_id)
                    .unwrap_or_else(|error| panic!("confirmation id: {error}")),
                title: "Confirm provider action".to_owned(),
                body: "Proceed?".to_owned(),
                confirm_label: "Proceed".to_owned(),
                destructive: false,
                continuation_schema,
            },
            now_epoch: 1,
        })))
        .unwrap_or_else(|error| panic!("provider confirmation outcome: {error}"))
        .next_state
}

fn boolean_field_with_default(field_id: &str, value: bool) -> Field {
    Field::parse(FieldDraft {
        id: Id::parse(field_id).unwrap_or_else(|error| panic!("field id: {error}")),
        label: field_id.to_owned(),
        description: None,
        kind: FieldKind::Boolean,
        required: true,
        default: Some(TypedValue::Bool(value)),
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| panic!("continuation field: {error}"))
}

fn assert_provider_confirmation_is_active(state: &AppState) {
    assert_eq!(state.modal, ModalState::None);
    assert_eq!(state.active_overlay_kind(), Some(OverlayKind::Confirmation));
    assert!(state.blocking_overlay_owns_mouse());
    assert_eq!(state.provider_requests.pending_confirmation_count(), 1);
}

fn close_blocker(state: AppState) -> AppState {
    state.apply(AppEvent::CloseModal).committed_pure()
}

#[test]
fn pending_provider_confirmation_is_admitted_after_help_closes() {
    let state = state_with_overlays()
        .apply(AppEvent::OpenHelp)
        .committed_pure();
    assert_eq!(state.active_overlay_kind(), Some(OverlayKind::Help));

    let state = register_pending_confirmation(state, "confirmation.after-help");
    assert_eq!(state.active_overlay_kind(), Some(OverlayKind::Help));

    assert_provider_confirmation_is_active(&close_blocker(state));
}

#[test]
fn pending_provider_confirmation_is_admitted_after_search_closes() {
    let state = state_with_overlays()
        .apply(AppEvent::OpenSearch)
        .committed_pure();
    assert_eq!(state.active_overlay_kind(), Some(OverlayKind::Search));

    let state = register_pending_confirmation(state, "confirmation.after-search");
    assert_eq!(state.active_overlay_kind(), Some(OverlayKind::Search));

    assert_provider_confirmation_is_active(&close_blocker(state));
}

#[test]
fn pending_provider_confirmation_is_admitted_after_a_generic_form_closes() {
    let state = state_with_overlays()
        .apply(AppEvent::OpenNewRepository)
        .committed_pure();
    assert!(matches!(state.modal, ModalState::NewRepository { .. }));

    let state = register_pending_confirmation(state, "confirmation.after-form");
    assert!(matches!(state.modal, ModalState::NewRepository { .. }));

    assert_provider_confirmation_is_active(&close_blocker(state));
}

#[test]
fn pending_provider_confirmation_is_admitted_after_a_generic_confirmation_closes() {
    let mut state = state_with_overlays();
    assert!(
        state.open_confirmation_payload(ConfirmationRequest::ServerLostRecovery {
            agent_ids: Vec::new(),
        })
    );

    let state = register_pending_confirmation(state, "confirmation.after-confirmation");
    assert!(matches!(
        state.nav.current().overlays().generic_confirmation(),
        Some(ConfirmationRequest::ServerLostRecovery { .. })
    ));

    assert_provider_confirmation_is_active(&close_blocker(state));
}

#[test]
fn provider_confirmation_is_admitted_while_a_generic_confirmation_owner_is_suspended() {
    let mut state = state_with_overlays();
    assert!(
        state.open_confirmation_payload(ConfirmationRequest::KillAgent {
            id: crate::domain::AgentId("owner-a".to_owned()),
        })
    );
    state.enter_screen(ScreenId::Issues);

    let state = register_pending_confirmation(state, "confirmation.current-owner");

    assert_provider_confirmation_is_active(&state);
}

#[test]
fn opening_a_form_cannot_split_an_active_provider_confirmation() {
    let state =
        register_pending_confirmation(state_with_overlays(), "confirmation.blocks-form-split");
    let before_overlay = state.nav.current().overlays().active().cloned();

    let state = state.apply(AppEvent::OpenNewRepository).committed_pure();

    assert_eq!(state.modal, ModalState::None);
    assert_eq!(
        state.nav.current().overlays().active(),
        before_overlay.as_ref()
    );
    assert_provider_confirmation_is_active(&state);
}

#[test]
fn a_second_provider_confirmation_waits_for_the_exact_active_confirmation() {
    let first_field = boolean_field_with_default("first-field", true);
    let second_field = boolean_field_with_default("second-field", false);
    let first_id = first_field.id().clone();
    let second_id = second_field.id().clone();
    let state = register_pending_confirmation_with_schema(
        state_with_overlays(),
        "confirmation.active-first",
        vec![first_field],
    );
    let state = register_pending_confirmation_with_schema(
        state,
        "confirmation.queued-second",
        vec![second_field],
    );
    assert_eq!(state.provider_requests.pending_confirmation_count(), 2);
    let active = state
        .current_provider_confirmation()
        .unwrap_or_else(|| panic!("first confirmation must remain active"));
    assert_eq!(
        active.confirmation_id().as_str(),
        "confirmation.active-first"
    );
    assert_eq!(active.continuation_schema()[0].id(), &first_id);
    let active_values = state
        .nav
        .current()
        .overlays()
        .confirmation_values()
        .unwrap_or_else(|| panic!("active confirmation must own displayed values"));
    assert_eq!(active_values.get(&first_id), Some(&TypedValue::Bool(true)));
    assert_eq!(active_values.get(&second_id), None);

    let state = state
        .apply_message(AppMessage::Provider(Box::new(
            ProviderMessage::CancelConfirmation,
        )))
        .unwrap_or_else(|error| panic!("cancel active confirmation: {error}"))
        .next_state;
    let pending = state
        .current_provider_confirmation()
        .unwrap_or_else(|| panic!("queued confirmation must become active"));
    assert_eq!(
        pending.confirmation_id().as_str(),
        "confirmation.queued-second"
    );
    assert_eq!(pending.continuation_schema()[0].id(), &second_id);
    let queued_values = state
        .nav
        .current()
        .overlays()
        .confirmation_values()
        .unwrap_or_else(|| panic!("queued confirmation must own displayed values"));
    assert_eq!(queued_values.get(&first_id), None);
    assert_eq!(
        queued_values.get(&second_id),
        Some(&TypedValue::Bool(false))
    );
    assert_provider_confirmation_is_active(&state);
}

#[test]
fn pending_provider_confirmation_is_admitted_when_its_suspended_owner_is_restored() {
    let mut state = state_with_overlays();
    let owner_screen = state.screen();
    let owner_instance = state.nav.current().id;
    let owner = Id::parse("host").unwrap_or_else(|error| panic!("owner: {error}"));
    let action =
        Id::parse("provider.confirm").unwrap_or_else(|error| panic!("provider action: {error}"));
    let screen =
        Id::parse(owner_screen.as_str()).unwrap_or_else(|error| panic!("screen identity: {error}"));
    let instance = Id::parse(&owner_instance.to_string())
        .unwrap_or_else(|error| panic!("instance identity: {error}"));
    let policy = ActionPolicy::new(
        ActionConfirmation::ProviderContinuation,
        vec![ActionOutcome::RequestHostConfirmation],
        false,
    );
    let key = state
        .provider_requests
        .invoke(InvokeInput {
            owner: &owner,
            action_id: &action,
            context_screen: &screen,
            context_instance: &instance,
            context_refs: &TypedMap::new(),
            arguments: &TypedMap::new(),
            policy: &policy,
        })
        .unwrap_or_else(|error| panic!("invoke: {error}"))
        .key;

    state.enter_screen(ScreenId::Issues);
    assert_ne!(state.nav.current().id, owner_instance);
    state = state
        .apply_message(AppMessage::Provider(Box::new(ProviderMessage::Outcome {
            key,
            outcome: Outcome::RequestHostConfirmation {
                confirmation_id: Id::parse("confirmation.after-restore")
                    .unwrap_or_else(|error| panic!("confirmation id: {error}")),
                title: "Confirm provider action".to_owned(),
                body: "Proceed?".to_owned(),
                confirm_label: "Proceed".to_owned(),
                destructive: false,
                continuation_schema: Vec::new(),
            },
            now_epoch: 1,
        })))
        .unwrap_or_else(|error| panic!("provider confirmation outcome: {error}"))
        .next_state;
    assert_ne!(state.active_overlay_kind(), Some(OverlayKind::Confirmation));

    state = state.apply(AppEvent::Back).committed_pure();
    assert_eq!(state.nav.current().id, owner_instance);
    assert_provider_confirmation_is_active(&state);
}

#[test]
fn reused_confirmation_id_closes_only_the_exact_instance_and_restores_suspended_owner() {
    let confirmation_id = "confirmation.reused";
    let expected_confirmation_id =
        Id::parse(confirmation_id).unwrap_or_else(|error| panic!("confirmation id: {error}"));
    let mut state = register_pending_confirmation(state_with_overlays(), confirmation_id);
    let first_instance = state.nav.current().id;
    assert_eq!(state.provider_requests.pending_confirmation_count(), 1);
    assert_eq!(
        state.nav.current().overlays().provider_confirmation_id(),
        Some(&expected_confirmation_id)
    );

    state.enter_screen(ScreenId::Issues);
    let second_instance = state.nav.current().id;
    assert_ne!(second_instance, first_instance);
    state = register_pending_confirmation(state, confirmation_id);
    assert_eq!(state.provider_requests.pending_confirmation_count(), 2);
    assert_eq!(
        state.nav.current().overlays().provider_confirmation_id(),
        Some(&expected_confirmation_id)
    );

    state = state
        .apply_message(AppMessage::Provider(Box::new(
            ProviderMessage::CancelConfirmation,
        )))
        .unwrap_or_else(|error| panic!("cancel current confirmation: {error}"))
        .next_state;
    assert_eq!(state.nav.current().id, second_instance);
    assert_eq!(state.provider_requests.pending_confirmation_count(), 1);
    assert!(
        state
            .nav
            .current()
            .overlays()
            .provider_confirmation_id()
            .is_none(),
        "closing the current token must close only its instance overlay"
    );

    state = state.apply(AppEvent::Back).committed_pure();
    assert_eq!(state.nav.current().id, first_instance);
    assert_eq!(
        state.nav.current().overlays().provider_confirmation_id(),
        Some(&expected_confirmation_id),
        "restoring the first instance must restore its independently bound token"
    );
    state = state
        .apply_message(AppMessage::Provider(Box::new(
            ProviderMessage::CancelConfirmation,
        )))
        .unwrap_or_else(|error| panic!("cancel restored confirmation: {error}"))
        .next_state;
    assert_eq!(state.provider_requests.pending_confirmation_count(), 0);
    assert!(
        state
            .nav
            .current()
            .overlays()
            .provider_confirmation_id()
            .is_none()
    );
}
