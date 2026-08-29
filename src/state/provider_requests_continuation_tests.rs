use super::*;
use crate::domain::plugin::action::{ActionConfirmation, ActionOutcome};
use crate::domain::plugin::field::{Field, FieldDraft, FieldKind, RestartScope};
use crate::domain::{TypedMap, TypedValue};
use crate::runtime::provider::protocol::Outcome;

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| panic!("valid test id {value}: {error}"))
}

fn boolean_field_with_id(field_id: &str) -> Field {
    Field::parse(FieldDraft {
        id: id(field_id),
        label: "Force".to_owned(),
        description: None,
        kind: FieldKind::Boolean,
        required: false,
        default: None,
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| panic!("valid continuation field: {error}"))
}

fn boolean_field() -> Field {
    boolean_field_with_id("force")
}

fn pending_confirmation(schema: Vec<Field>) -> (ProviderRequestState, ProviderRequestKey) {
    let mut state = ProviderRequestState::new();
    let empty = TypedMap::new();
    let policy = ActionPolicy::new(
        ActionConfirmation::ProviderContinuation,
        vec![ActionOutcome::RequestHostConfirmation],
        false,
    );
    let invocation = state
        .invoke(InvokeInput {
            owner: &id("host"),
            action_id: &id("provider.run"),
            context_screen: &id("dashboard"),
            context_instance: &id("screen-1"),
            context_refs: &empty,
            arguments: &empty,
            policy: &policy,
        })
        .unwrap_or_else(|error| panic!("invoke: {error}"));
    state
        .record_outcome(
            &invocation.key,
            Outcome::RequestHostConfirmation {
                confirmation_id: id("confirm.run"),
                title: "Confirm".to_owned(),
                body: "Proceed?".to_owned(),
                confirm_label: "Proceed".to_owned(),
                destructive: false,
                continuation_schema: schema,
            },
            100,
        )
        .unwrap_or_else(|error| panic!("confirmation outcome: {error}"));
    (state, invocation.key)
}

fn confirm(
    state: &mut ProviderRequestState,
    key: &ProviderRequestKey,
    values: &TypedMap,
) -> Result<ConfirmOutcome, ProviderRequestError> {
    let empty = TypedMap::new();
    state.confirm(
        ConfirmInput {
            owner: &id("host"),
            action_id: &id("provider.run"),
            context_screen: &id("dashboard"),
            context_instance: &id("screen-1"),
            context_refs: &empty,
            generation: key.generation,
            confirmation_id: &id("confirm.run"),
            values,
        },
        101,
    )
}

fn assert_rejected_without_consuming(
    state: &mut ProviderRequestState,
    key: &ProviderRequestKey,
    values: &TypedMap,
) {
    assert_eq!(
        confirm(state, key, values),
        Err(ProviderRequestError::InvalidContinuationValues)
    );
    assert_eq!(state.pending_confirmation_count(), 1);
    assert_eq!(state.requests().len(), 1);
}

#[test]
fn continuation_rejects_a_missing_declared_value_without_consuming_the_token() {
    let (mut state, key) = pending_confirmation(vec![boolean_field()]);
    assert_rejected_without_consuming(&mut state, &key, &TypedMap::new());

    let values = [(id("force"), TypedValue::Bool(false))].into();
    assert!(confirm(&mut state, &key, &values).is_ok());
}

#[test]
fn continuation_rejects_an_extra_value_without_consuming_the_token() {
    let (mut state, key) = pending_confirmation(Vec::new());
    let extra = [(id("extra"), TypedValue::Bool(false))].into();
    assert_rejected_without_consuming(&mut state, &key, &extra);

    assert!(confirm(&mut state, &key, &TypedMap::new()).is_ok());
}

#[test]
fn continuation_rejects_a_wrong_typed_value_without_consuming_the_token() {
    let (mut state, key) = pending_confirmation(vec![boolean_field()]);
    let wrong = [(id("force"), TypedValue::String("false".to_owned()))].into();
    assert_rejected_without_consuming(&mut state, &key, &wrong);

    let values = [(id("force"), TypedValue::Bool(false))].into();
    assert!(confirm(&mut state, &key, &values).is_ok());
}

#[test]
fn continuation_schema_rejects_reserved_and_duplicate_field_ids_before_admission() {
    let record = |schema: Vec<Field>| {
        let mut state = ProviderRequestState::new();
        let empty = TypedMap::new();
        let policy = ActionPolicy::new(
            ActionConfirmation::ProviderContinuation,
            vec![ActionOutcome::RequestHostConfirmation],
            false,
        );
        let invocation = state
            .invoke(InvokeInput {
                owner: &id("host"),
                action_id: &id("provider.run"),
                context_screen: &id("dashboard"),
                context_instance: &id("screen-1"),
                context_refs: &empty,
                arguments: &empty,
                policy: &policy,
            })
            .unwrap_or_else(|error| panic!("invoke: {error}"));
        let result = state.record_outcome(
            &invocation.key,
            Outcome::RequestHostConfirmation {
                confirmation_id: id("confirm.run"),
                title: "Confirm".to_owned(),
                body: "Proceed?".to_owned(),
                confirm_label: "Proceed".to_owned(),
                destructive: false,
                continuation_schema: schema,
            },
            100,
        );
        (state, result)
    };

    for schema in [
        vec![boolean_field_with_id("decision")],
        vec![boolean_field(), boolean_field()],
    ] {
        let (state, result) = record(schema);
        assert_eq!(result, Err(ProviderRequestError::InvalidContinuationSchema));
        assert_eq!(state.pending_confirmation_count(), 0);
        assert_eq!(state.requests().len(), 1);
    }
}

#[test]
fn reused_confirmation_id_does_not_replace_another_generation() {
    let (mut state, first_key) = pending_confirmation(Vec::new());
    let empty = TypedMap::new();
    let policy = ActionPolicy::new(
        ActionConfirmation::ProviderContinuation,
        vec![ActionOutcome::RequestHostConfirmation],
        false,
    );
    let second = state
        .invoke(InvokeInput {
            owner: &id("host"),
            action_id: &id("provider.run"),
            context_screen: &id("dashboard"),
            context_instance: &id("screen-1"),
            context_refs: &empty,
            arguments: &empty,
            policy: &policy,
        })
        .unwrap_or_else(|error| panic!("second invoke: {error}"));
    state
        .record_outcome(
            &second.key,
            Outcome::RequestHostConfirmation {
                confirmation_id: id("confirm.run"),
                title: "Confirm second".to_owned(),
                body: "Proceed?".to_owned(),
                confirm_label: "Proceed".to_owned(),
                destructive: false,
                continuation_schema: Vec::new(),
            },
            100,
        )
        .unwrap_or_else(|error| panic!("second confirmation: {error}"));

    assert_eq!(state.pending_confirmation_count(), 2);
    assert_eq!(
        state
            .first_pending_confirmation_for("dashboard", "screen-1")
            .map(PendingConfirmationView::generation),
        Some(first_key.generation)
    );
    assert!(confirm(&mut state, &first_key, &empty).is_ok());
    assert_eq!(state.pending_confirmation_count(), 1);
    assert_eq!(
        state
            .first_pending_confirmation_for("dashboard", "screen-1")
            .map(PendingConfirmationView::generation),
        Some(second.key.generation)
    );
}

#[test]
fn reused_confirmation_id_does_not_replace_another_screen_instance() {
    let (mut state, _) = pending_confirmation(Vec::new());
    let empty = TypedMap::new();
    let policy = ActionPolicy::new(
        ActionConfirmation::ProviderContinuation,
        vec![ActionOutcome::RequestHostConfirmation],
        false,
    );
    let second = state
        .invoke(InvokeInput {
            owner: &id("host"),
            action_id: &id("provider.run"),
            context_screen: &id("dashboard"),
            context_instance: &id("screen-2"),
            context_refs: &empty,
            arguments: &empty,
            policy: &policy,
        })
        .unwrap_or_else(|error| panic!("second invoke: {error}"));
    state
        .record_outcome(
            &second.key,
            Outcome::RequestHostConfirmation {
                confirmation_id: id("confirm.run"),
                title: "Confirm second".to_owned(),
                body: "Proceed?".to_owned(),
                confirm_label: "Proceed".to_owned(),
                destructive: false,
                continuation_schema: Vec::new(),
            },
            100,
        )
        .unwrap_or_else(|error| panic!("second confirmation: {error}"));

    assert_eq!(state.pending_confirmation_count(), 2);
    let first_identity = state
        .first_pending_confirmation_for("dashboard", "screen-1")
        .unwrap_or_else(|| panic!("first pending confirmation"))
        .identity();
    assert!(state.cancel_confirmation(&first_identity));
    assert_eq!(state.pending_confirmation_count(), 1);
    assert_eq!(
        state
            .first_pending_confirmation_for("dashboard", "screen-2")
            .map(PendingConfirmationView::generation),
        Some(second.key.generation)
    );
}
