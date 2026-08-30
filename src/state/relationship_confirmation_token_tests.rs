use crate::domain::effects::{Effect, ProviderEffect, ProviderRequestKey};
use crate::domain::plugin::action::{ActionConfirmation, ActionOutcome};
use crate::domain::plugin::field::{Field, FieldDraft, FieldKind, RestartScope};
use crate::domain::{Id, TypedMap, TypedValue};
use crate::messages::ProviderMessage;
use crate::runtime::provider::protocol::Outcome;

use super::provider_requests::{ActionPolicy, ProviderConfirmationIdentity};
use super::relationship_runtime_tests::{
    DestructiveConfirmationFixture, apply_provider, destructive_confirmation_fixture,
};
use super::transition::Transition;
use super::{AppState, screen_overlays::ScreenOverlayState};

struct QueuedConfirmations {
    fixture: DestructiveConfirmationFixture,
    state: AppState,
    hidden: Id,
    hidden_key: ProviderRequestKey,
    hidden_field: Id,
}

struct ConfirmationSnapshot {
    presented: Id,
    hidden: Id,
    screen: String,
    instance: String,
    overlay: ScreenOverlayState,
    requests: String,
}
fn hidden_continuation_field() -> Field {
    Field::parse(FieldDraft {
        id: Id::parse("hidden-value").unwrap_or_else(|error| panic!("field id: {error}")),
        label: "Hidden value".to_owned(),
        description: None,
        kind: FieldKind::String,
        required: true,
        default: Some(TypedValue::String("queued".to_owned())),
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| panic!("hidden continuation field: {error}"))
}

fn queued_confirmations() -> QueuedConfirmations {
    let fixture = destructive_confirmation_fixture();
    let hidden = fixture.confirmation_id.clone();
    let hidden_field = hidden_continuation_field();
    let policy = ActionPolicy::new(
        ActionConfirmation::ProviderContinuation,
        vec![ActionOutcome::RequestHostConfirmation],
        true,
    );
    let invoked = apply_provider(
        fixture.state.clone(),
        ProviderMessage::Invoke {
            owner: fixture.owner.clone(),
            action_id: fixture.action_id.clone(),
            arguments: TypedMap::new(),
            policy,
        },
    );
    let hidden_key = match &invoked.effects[0].effect {
        Effect::Provider(ProviderEffect::InvokeAction { invocation }) => invocation.key.clone(),
        other => panic!("expected second invoke effect, got {other:?}"),
    };
    let state = apply_provider(
        invoked.next_state,
        ProviderMessage::Outcome {
            key: hidden_key.clone(),
            outcome: Outcome::RequestHostConfirmation {
                confirmation_id: hidden.clone(),
                title: "Hidden confirmation".to_owned(),
                body: "This token must remain queued.".to_owned(),
                confirm_label: "Proceed".to_owned(),
                destructive: true,
                continuation_schema: vec![hidden_field.clone()],
            },
            now_epoch: 101,
        },
    )
    .next_state;
    QueuedConfirmations {
        fixture,
        state,
        hidden,
        hidden_key,
        hidden_field: hidden_field.id().clone(),
    }
}

fn assert_queued(state: &AppState, presented: &Id, hidden: &Id, screen: &str, instance: &str) {
    assert_eq!(
        state.nav.current().overlays().provider_confirmation_id(),
        Some(presented)
    );
    assert_eq!(state.provider_requests.pending_confirmation_count(), 2);
    assert_eq!(
        hidden, presented,
        "the queued token deliberately reuses the public id"
    );
    assert_eq!(state.nav.current().screen.as_str(), screen);
    assert_eq!(state.nav.current().id.to_string(), instance);
}

fn assert_rejected_unchanged(rejected: &Transition, before: &ConfirmationSnapshot) {
    assert!(rejected.effects.is_empty());
    assert_eq!(
        rejected.next_state.nav.current().overlays(),
        &before.overlay
    );
    assert_queued(
        &rejected.next_state,
        &before.presented,
        &before.hidden,
        &before.screen,
        &before.instance,
    );
    assert_eq!(
        format!("{:?}", rejected.next_state.provider_requests),
        before.requests
    );
}

#[test]
fn hidden_valid_confirmation_cannot_replace_or_consume_the_presented_token() {
    let queued = queued_confirmations();
    let before = ConfirmationSnapshot {
        presented: queued.fixture.confirmation_id.clone(),
        hidden: queued.hidden.clone(),
        screen: queued.state.screen().as_str().to_owned(),
        instance: queued.state.nav.current().id.to_string(),
        overlay: queued.state.nav.current().overlays().clone(),
        requests: format!("{:?}", queued.state.provider_requests),
    };
    assert_queued(
        &queued.state,
        &before.presented,
        &before.hidden,
        &before.screen,
        &before.instance,
    );

    let rejected = apply_provider(
        queued.state,
        ProviderMessage::Confirm {
            owner: queued.fixture.owner,
            action_id: queued.fixture.action_id,
            generation: queued.hidden_key.generation,
            confirmation_id: queued.hidden.clone(),
            values: TypedMap::new(),
            now_epoch: 102,
        },
    );
    assert_rejected_unchanged(&rejected, &before);
}

fn suspend_and_restore_presented(
    mut state: AppState,
    presented: &ProviderConfirmationIdentity,
) -> AppState {
    let owner_instance = state.nav.current().id;
    let route = state.nav.current().activation.route;
    state.enter_provider_route(route, crate::workbench::ActivationValues::empty());
    assert_ne!(state.nav.current().id, owner_instance);
    assert!(
        state
            .nav
            .current()
            .overlays()
            .provider_confirmation()
            .is_none()
    );
    state.leave_screen();
    assert_eq!(state.nav.current().id, owner_instance);
    assert_eq!(
        state.nav.current().overlays().provider_confirmation(),
        Some(presented)
    );
    assert_eq!(state.provider_requests.pending_confirmation_count(), 2);
    state
}

fn reject_hidden_edit(
    state: AppState,
    hidden_field: &Id,
    presented: &ProviderConfirmationIdentity,
) -> AppState {
    let rejected = apply_provider(
        state,
        ProviderMessage::EditConfirmationField {
            field_id: hidden_field.clone(),
            value: TypedValue::String("must-not-reach-hidden".to_owned()),
        },
    );
    assert!(rejected.effects.is_empty());
    assert_eq!(
        rejected
            .next_state
            .nav
            .current()
            .overlays()
            .provider_confirmation(),
        Some(presented)
    );
    assert_eq!(
        rejected
            .next_state
            .provider_requests
            .pending_confirmation_count(),
        2
    );
    rejected.next_state
}

fn cancel_and_promote_hidden(
    state: AppState,
    hidden: &Id,
    hidden_key: &ProviderRequestKey,
    presented: &ProviderConfirmationIdentity,
) -> AppState {
    let cancelled = apply_provider(state, ProviderMessage::CancelConfirmation);
    assert!(cancelled.effects.is_empty());
    assert_eq!(
        cancelled
            .next_state
            .provider_requests
            .pending_confirmation_count(),
        1
    );
    let next = cancelled
        .next_state
        .nav
        .current()
        .overlays()
        .provider_confirmation()
        .unwrap_or_else(|| panic!("queued confirmation must be presented next"));
    assert_eq!(next.confirmation_id(), hidden);
    assert_eq!(next.generation(), hidden_key.generation);
    assert_ne!(next, presented);
    cancelled.next_state
}

#[test]
fn same_id_cancel_and_edit_follow_the_presented_identity_and_fifo_order() {
    let queued = queued_confirmations();
    let presented_identity = queued
        .state
        .nav
        .current()
        .overlays()
        .provider_confirmation()
        .unwrap_or_else(|| panic!("presented confirmation identity"))
        .clone();
    assert_ne!(
        presented_identity.generation(),
        queued.hidden_key.generation
    );

    let restored = suspend_and_restore_presented(queued.state, &presented_identity);
    let rejected_edit = reject_hidden_edit(restored, &queued.hidden_field, &presented_identity);
    let promoted = cancel_and_promote_hidden(
        rejected_edit,
        &queued.hidden,
        &queued.hidden_key,
        &presented_identity,
    );

    let edited = apply_provider(
        promoted,
        ProviderMessage::EditConfirmationField {
            field_id: queued.hidden_field.clone(),
            value: TypedValue::String("now-presented".to_owned()),
        },
    );
    let mut expected = TypedMap::new();
    expected.insert(
        queued.hidden_field,
        TypedValue::String("now-presented".to_owned()),
    );
    assert_eq!(
        edited
            .next_state
            .nav
            .current()
            .overlays()
            .confirmation_values(),
        Some(&expected)
    );
}
