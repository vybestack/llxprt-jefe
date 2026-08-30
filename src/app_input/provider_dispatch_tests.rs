use jefe::domain::effects::{
    Effect, EffectFamily, IssuedEffect, ProviderEffect, ProviderHostOutcome, ProviderNotice,
    ProviderNoticeSeverity, RetryPolicy, SemanticKey,
};
use jefe::domain::plugin::action::{ActionConfirmation, ActionOutcome};
use jefe::domain::plugin::field::{Field, FieldDraft, FieldKind, RestartScope, Scalar};
use jefe::runtime::provider::protocol::Outcome;
use jefe::state::provider_requests::{ActionPolicy, InvokeInput};
use jefe::state::transition::TransitionExt;
use jefe::state::{AppEvent, ScreenId};

use super::*;

fn active_request(state: &mut jefe::state::AppState) -> jefe::domain::effects::ProviderRequestKey {
    active_request_with_policy(
        state,
        ActionPolicy::new(ActionConfirmation::None, vec![ActionOutcome::Notice], false),
    )
}

fn active_request_with_policy(
    state: &mut jefe::state::AppState,
    policy: ActionPolicy,
) -> jefe::domain::effects::ProviderRequestKey {
    let owner = Id::parse("host").unwrap_or_else(|error| panic!("owner: {error}"));
    let action = Id::parse("provider.notice").unwrap_or_else(|error| panic!("action: {error}"));
    let screen =
        Id::parse(state.screen().as_str()).unwrap_or_else(|error| panic!("screen: {error}"));
    let instance = Id::parse(&state.nav.current().id.to_string())
        .unwrap_or_else(|error| panic!("instance: {error}"));
    state
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
        .key
}

fn active_request_through_runtime(
    state: &mut jefe::state::AppState,
    policy: ActionPolicy,
) -> jefe::domain::effects::ProviderRequestKey {
    let transition = state
        .clone()
        .apply_message(jefe::messages::AppMessage::Provider(Box::new(
            ProviderMessage::Invoke {
                owner: Id::parse("host").unwrap_or_else(|error| panic!("owner: {error}")),
                action_id: Id::parse("provider.notice")
                    .unwrap_or_else(|error| panic!("action: {error}")),
                arguments: TypedMap::new(),
                policy,
            },
        )))
        .unwrap_or_else(|error| panic!("runtime invoke: {error}"));
    *state = transition.next_state;
    latest_request_key(state).clone()
}

fn latest_request_key(state: &jefe::state::AppState) -> &jefe::domain::effects::ProviderRequestKey {
    let Some(request) = state.provider_requests.requests().last() else {
        panic!("pending request");
    };
    request.key()
}
fn continuation_string_field(id: &str, default: &str) -> Field {
    Field::parse(FieldDraft {
        id: Id::parse(id).unwrap_or_else(|error| panic!("field id: {error}")),
        label: "Release note".to_owned(),
        description: None,
        kind: FieldKind::String,
        required: true,
        default: Some(TypedValue::String(default.to_owned())),
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| panic!("field: {error}"))
}
fn bounded_continuation_string_field(id: &str, default: &str, min: i64) -> Field {
    Field::parse(FieldDraft {
        id: Id::parse(id).unwrap_or_else(|error| panic!("field id: {error}")),
        label: "Release note".to_owned(),
        description: None,
        kind: FieldKind::String,
        required: true,
        default: Some(TypedValue::String(default.to_owned())),
        min: Some(Scalar::Integer(min)),
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| panic!("field: {error}"))
}

fn continuation_field(
    id: &str,
    kind: FieldKind,
    default: TypedValue,
    choices: Vec<Scalar>,
) -> Field {
    Field::parse(FieldDraft {
        id: Id::parse(id).unwrap_or_else(|error| panic!("field id: {error}")),
        label: id.to_owned(),
        description: None,
        kind,
        required: true,
        default: Some(default),
        min: None,
        max: None,
        choices,
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| panic!("field: {error}"))
}

#[test]
fn keybind_invocation_never_synthesizes_values_for_declared_arguments() {
    assert_eq!(keybind_invocation_arguments(&[]), Some(TypedMap::new()));
    let argument = continuation_string_field("branch", "main");
    assert_eq!(keybind_invocation_arguments(&[argument]), None);
}

#[test]
fn keybind_runtime_never_dispatches_provider_action_with_declared_arguments() {
    let action = jefe::domain::action_registry::ActionId::parse("provider.with-argument")
        .unwrap_or_else(|error| panic!("action: {error}"));
    let argument = continuation_string_field("branch", "main");
    let policy = ActionPolicy::new(ActionConfirmation::None, Vec::new(), false);

    assert!(!dispatch_keybind_invocation(
        &action,
        &[argument],
        &policy,
        |_| panic!("provider message must not be dispatched"),
    ));
}

fn provider_confirmation_with_string_default(default: &str) -> (jefe::state::AppState, Id) {
    provider_confirmation_with_field(continuation_string_field("release-note", default))
}

fn provider_confirmation_with_field(field: Field) -> (jefe::state::AppState, Id) {
    let field_id = field.id().clone();
    let state = provider_confirmation_with_fields(vec![field]);
    (state, field_id)
}

fn provider_confirmation_with_fields(fields: Vec<Field>) -> jefe::state::AppState {
    let mut state = crate::test_app_state();
    let policy = ActionPolicy::new(
        ActionConfirmation::ProviderContinuation,
        vec![ActionOutcome::RequestHostConfirmation],
        false,
    );
    let key = active_request_with_policy(&mut state, policy);
    state
        .apply_message(jefe::messages::AppMessage::Provider(Box::new(
            ProviderMessage::Outcome {
                key,
                outcome: Outcome::RequestHostConfirmation {
                    confirmation_id: Id::parse("confirmation.values")
                        .unwrap_or_else(|error| panic!("confirmation id: {error}")),
                    title: "Confirm".to_owned(),
                    body: "Proceed?".to_owned(),
                    confirm_label: "Proceed".to_owned(),
                    destructive: false,
                    continuation_schema: fields,
                },
                now_epoch: 1,
            },
        )))
        .unwrap_or_else(|error| panic!("record confirmation: {error}"))
        .next_state
}

#[test]
fn retry_from_a_different_instance_of_the_same_screen_is_rejected_without_effect_or_generation() {
    let mut state = crate::test_app_state();
    let old_key = active_request(&mut state);
    let request_count = state.provider_requests.requests().len();
    let original_instance = state.nav.current().id;
    let current_screen = state.screen();
    let route = state.nav.current().activation.route;
    let _ = state.enter_provider_route(route, jefe::workbench::ActivationValues::empty());
    assert_eq!(state.screen(), current_screen);
    assert_ne!(state.nav.current().id, original_instance);

    let transition = state
        .apply_message(jefe::messages::AppMessage::Provider(Box::new(
            ProviderMessage::Retry {
                old_key: old_key.clone(),
            },
        )))
        .unwrap_or_else(|error| panic!("retry transition: {error}"));
    let state = transition.next_state;

    assert_eq!(state.provider_requests.requests().len(), request_count);
    assert_eq!(latest_request_key(&state), &old_key);
    assert!(transition.effects.is_empty());
    assert!(state.error_message.as_deref().is_some_and(|message| {
        message.contains("context no longer matches the authorized intent")
    }));
}

#[test]
fn provider_confirmation_controls_are_interpreted_by_the_form_control() {
    let mut state = crate::test_app_state();
    let policy = ActionPolicy::new(
        ActionConfirmation::ProviderContinuation,
        vec![ActionOutcome::RequestHostConfirmation],
        false,
    );
    let key = active_request_with_policy(&mut state, policy);
    state = state
        .apply_message(jefe::messages::AppMessage::Provider(Box::new(
            ProviderMessage::Outcome {
                key,
                outcome: Outcome::RequestHostConfirmation {
                    confirmation_id: Id::parse("confirmation.one")
                        .unwrap_or_else(|error| panic!("confirmation id: {error}")),
                    title: "Confirm".to_owned(),
                    body: "Proceed?".to_owned(),
                    confirm_label: "Proceed".to_owned(),
                    destructive: false,
                    continuation_schema: Vec::new(),
                },
                now_epoch: 1,
            },
        )))
        .unwrap_or_else(|error| panic!("record confirmation: {error}"))
        .next_state;
    assert!(matches!(
        provider_surface_message(&state, ProviderSurfaceControl::CycleConfirmationFocus),
        Some(ProviderMessage::CycleConfirmationFocus)
    ));
    assert!(matches!(
        provider_surface_message(&state, ProviderSurfaceControl::ActivateConfirmation),
        Some(ProviderMessage::CancelConfirmation)
    ));
    state = state
        .apply_message(jefe::messages::AppMessage::Provider(Box::new(
            ProviderMessage::CycleConfirmationFocus,
        )))
        .unwrap_or_else(|error| panic!("cycle confirmation: {error}"))
        .next_state;
    assert!(matches!(
        provider_surface_message(&state, ProviderSurfaceControl::ActivateConfirmation),
        Some(ProviderMessage::Confirm { .. })
    ));
}

fn admit_confirmation(
    state: jefe::state::AppState,
    key: jefe::domain::effects::ProviderRequestKey,
    confirmation_id: &str,
    now_epoch: u64,
) -> jefe::state::AppState {
    state
        .apply_message(jefe::messages::AppMessage::Provider(Box::new(
            ProviderMessage::Outcome {
                key,
                outcome: Outcome::RequestHostConfirmation {
                    confirmation_id: Id::parse(confirmation_id)
                        .unwrap_or_else(|error| panic!("confirmation id: {error}")),
                    title: confirmation_id.to_owned(),
                    body: format!("Confirm {confirmation_id}?"),
                    confirm_label: confirmation_id.to_owned(),
                    destructive: false,
                    continuation_schema: Vec::new(),
                },
                now_epoch,
            },
        )))
        .unwrap_or_else(|error| panic!("record confirmation: {error}"))
        .next_state
}

fn displayed_confirm(
    state: &jefe::state::AppState,
    key: &jefe::domain::effects::ProviderRequestKey,
    expected_id: &str,
) -> ProviderMessage {
    let Some(message @ ProviderMessage::Confirm { .. }) =
        provider_surface_message(state, ProviderSurfaceControl::ActivateConfirmation)
    else {
        panic!("displayed confirmation must submit");
    };
    let ProviderMessage::Confirm {
        action_id,
        generation,
        confirmation_id,
        ..
    } = &message
    else {
        unreachable!();
    };
    assert_eq!(action_id, &key.action_id);
    assert_eq!(*generation, key.generation);
    assert_eq!(confirmation_id.as_str(), expected_id);
    message
}

fn apply_provider_message(
    state: jefe::state::AppState,
    message: ProviderMessage,
) -> jefe::state::AppState {
    state
        .apply_message(jefe::messages::AppMessage::Provider(Box::new(message)))
        .unwrap_or_else(|error| panic!("apply provider message: {error}"))
        .next_state
}

#[test]
fn confirmation_dispatch_uses_the_displayed_tokens_binding_not_the_latest_request() {
    let mut state = crate::test_app_state();
    let policy = ActionPolicy::new(
        ActionConfirmation::ProviderContinuation,
        vec![ActionOutcome::RequestHostConfirmation],
        false,
    );
    let now = current_epoch_seconds().unwrap_or_else(|| panic!("system epoch"));
    let first_key = active_request_through_runtime(&mut state, policy.clone());
    state = admit_confirmation(state, first_key.clone(), "confirmation.first", now);
    let second_key = active_request_through_runtime(&mut state, policy);
    state = admit_confirmation(state, second_key.clone(), "confirmation.second", now);
    state = apply_provider_message(state, ProviderMessage::CycleConfirmationFocus);

    let first_message = displayed_confirm(&state, &first_key, "confirmation.first");
    assert_eq!(latest_request_key(&state), &second_key);
    state = apply_provider_message(state, first_message);
    assert_eq!(
        state.nav.current().overlays().provider_confirmation_id(),
        Some(
            &Id::parse("confirmation.second")
                .unwrap_or_else(|error| panic!("confirmation id: {error}"))
        )
    );

    state = apply_provider_message(state, ProviderMessage::CycleConfirmationFocus);
    let second_message = displayed_confirm(&state, &second_key, "confirmation.second");
    state = apply_provider_message(state, second_message);
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

#[test]
fn provider_confirmation_submits_the_exact_instance_owned_displayed_values() {
    let (mut state, field_id) = provider_confirmation_with_string_default("ready");
    let mut expected = TypedMap::new();
    expected.insert(field_id.clone(), TypedValue::String("ready".to_owned()));
    assert_eq!(
        state.nav.current().overlays().confirmation_values(),
        Some(&expected)
    );
    assert_eq!(
        state.provider_confirmation_field_edit(
            field_id.clone(),
            TypedValue::String("ready!".to_owned()),
        ),
        Some((field_id.clone(), TypedValue::String("ready!".to_owned()),)),
        "the shared Form control must accept the exact declared typed edit",
    );
    state = state
        .apply_message(jefe::messages::AppMessage::Provider(Box::new(
            ProviderMessage::EditConfirmationField {
                field_id: field_id.clone(),
                value: TypedValue::String("ready!".to_owned()),
            },
        )))
        .unwrap_or_else(|error| panic!("edit provider field: {error}"))
        .next_state;
    expected.insert(field_id, TypedValue::String("ready!".to_owned()));
    assert_eq!(
        state.nav.current().overlays().confirmation_values(),
        Some(&expected)
    );
    state = state
        .apply_message(jefe::messages::AppMessage::Provider(Box::new(
            ProviderMessage::CycleConfirmationFocus,
        )))
        .unwrap_or_else(|error| panic!("cycle to provider field: {error}"))
        .next_state;
    assert!(
        provider_surface_message(&state, ProviderSurfaceControl::ActivateConfirmation).is_none(),
        "Enter on a focused provider field must not choose Cancel or Confirm"
    );
    state = state
        .apply_message(jefe::messages::AppMessage::Provider(Box::new(
            ProviderMessage::CycleConfirmationFocus,
        )))
        .unwrap_or_else(|error| panic!("cycle to confirm: {error}"))
        .next_state;
    let Some(ProviderMessage::Confirm { values, .. }) =
        provider_surface_message(&state, ProviderSurfaceControl::ActivateConfirmation)
    else {
        panic!("focused confirmation must submit");
    };
    assert_eq!(values, expected);
}

fn scalar_continuation_field_edits() -> Vec<(Field, TypedValue)> {
    let decimal = |value: &str| {
        jefe::domain::CanonicalDecimal::parse(value)
            .unwrap_or_else(|error| panic!("decimal: {error}"))
    };
    vec![
        (
            continuation_field(
                "enabled",
                FieldKind::Boolean,
                TypedValue::Bool(false),
                Vec::new(),
            ),
            TypedValue::Bool(true),
        ),
        (
            continuation_field(
                "note",
                FieldKind::String,
                TypedValue::String("draft".to_owned()),
                Vec::new(),
            ),
            TypedValue::String("ready".to_owned()),
        ),
        (
            continuation_field(
                "path",
                FieldKind::Path,
                TypedValue::String("/tmp/old".to_owned()),
                Vec::new(),
            ),
            TypedValue::String("/tmp/new".to_owned()),
        ),
        (
            continuation_field(
                "count",
                FieldKind::Integer,
                TypedValue::Integer(1),
                Vec::new(),
            ),
            TypedValue::Integer(2),
        ),
        (
            continuation_field(
                "ratio",
                FieldKind::FiniteNumber,
                TypedValue::Decimal(decimal("1.5")),
                Vec::new(),
            ),
            TypedValue::Decimal(decimal("2.5")),
        ),
    ]
}

fn choice_continuation_field_edits() -> Vec<(Field, TypedValue)> {
    let secret = |env: &str| {
        TypedValue::SecretRef(jefe::domain::SecretRef {
            env: jefe::domain::plugin::SecretReference::parse(env)
                .unwrap_or_else(|error| panic!("secret reference: {error}")),
        })
    };
    vec![
        (
            continuation_field(
                "channel",
                FieldKind::Enum,
                TypedValue::String("blue".to_owned()),
                vec![
                    Scalar::Text("blue".to_owned()),
                    Scalar::Text("green".to_owned()),
                ],
            ),
            TypedValue::String("green".to_owned()),
        ),
        (
            continuation_field(
                "tags",
                FieldKind::StringList,
                TypedValue::List(vec![TypedValue::String("one".to_owned())]),
                Vec::new(),
            ),
            TypedValue::List(vec![
                TypedValue::String("one".to_owned()),
                TypedValue::String("two".to_owned()),
            ]),
        ),
        (
            continuation_field(
                "secret",
                FieldKind::SecretReference,
                secret("TOKEN_OLD"),
                Vec::new(),
            ),
            secret("TOKEN_NEW"),
        ),
    ]
}

#[test]
fn provider_confirmation_submits_all_declared_field_kinds_exactly_as_displayed() {
    let mut fields_and_edits = scalar_continuation_field_edits();
    fields_and_edits.extend(choice_continuation_field_edits());
    let fields = fields_and_edits
        .iter()
        .map(|(field, _)| field.clone())
        .collect();
    let mut state = provider_confirmation_with_fields(fields);
    let mut expected = TypedMap::new();

    for (field, value) in &fields_and_edits {
        let field_id = field.id().clone();
        assert_eq!(
            state.provider_confirmation_field_edit(field_id.clone(), value.clone()),
            Some((field_id.clone(), value.clone()))
        );
        state = state
            .apply_message(jefe::messages::AppMessage::Provider(Box::new(
                ProviderMessage::EditConfirmationField {
                    field_id: field_id.clone(),
                    value: value.clone(),
                },
            )))
            .unwrap_or_else(|error| panic!("edit provider field: {error}"))
            .next_state;
        expected.insert(field_id, value.clone());
    }
    assert_eq!(
        state.nav.current().overlays().confirmation_values(),
        Some(&expected)
    );

    for _ in 0..=fields_and_edits.len() {
        state = state
            .apply_message(jefe::messages::AppMessage::Provider(Box::new(
                ProviderMessage::CycleConfirmationFocus,
            )))
            .unwrap_or_else(|error| panic!("cycle provider confirmation focus: {error}"))
            .next_state;
    }
    let Some(ProviderMessage::Confirm { values, .. }) =
        provider_surface_message(&state, ProviderSurfaceControl::ActivateConfirmation)
    else {
        panic!("focused confirmation must submit all displayed values");
    };
    assert_eq!(values, expected);
}

#[test]
fn invalid_or_undeclared_confirmation_edits_preserve_exact_request_and_overlay_state() {
    let (mut state, field_id) = provider_confirmation_with_string_default("ready");
    let request_key = latest_request_key(&state).clone();
    let expected_values = state
        .nav
        .current()
        .overlays()
        .confirmation_values()
        .cloned();
    let unknown =
        Id::parse("unknown-field").unwrap_or_else(|error| panic!("unknown field id: {error}"));

    for (field_id, value) in [
        (field_id, TypedValue::Bool(true)),
        (unknown, TypedValue::String("ignored".to_owned())),
    ] {
        let transition = state
            .apply_message(jefe::messages::AppMessage::Provider(Box::new(
                ProviderMessage::EditConfirmationField { field_id, value },
            )))
            .unwrap_or_else(|error| panic!("reject provider field edit: {error}"));
        assert!(transition.effects.is_empty());
        state = transition.next_state;
    }

    assert_eq!(state.provider_requests.pending_confirmation_count(), 1);
    assert_eq!(latest_request_key(&state), &request_key);
    assert_eq!(
        state.nav.current().overlays().confirmation_values(),
        expected_values.as_ref()
    );
}

#[test]
fn constraint_invalid_confirmation_edit_preserves_request_and_overlay_state() {
    let field = bounded_continuation_string_field("release-note", "ready", 5);
    let (state, field_id) = provider_confirmation_with_field(field);
    let request_key = latest_request_key(&state).clone();
    let expected_values = state
        .nav
        .current()
        .overlays()
        .confirmation_values()
        .cloned();

    assert_eq!(
        state
            .provider_confirmation_field_edit(field_id.clone(), TypedValue::String("x".to_owned())),
        Some((field_id.clone(), TypedValue::String("x".to_owned())))
    );
    let transition = state
        .apply_message(jefe::messages::AppMessage::Provider(Box::new(
            ProviderMessage::EditConfirmationField {
                field_id,
                value: TypedValue::String("x".to_owned()),
            },
        )))
        .unwrap_or_else(|error| panic!("reject constrained provider field edit: {error}"));
    assert!(transition.effects.is_empty());
    assert_eq!(latest_request_key(&transition.next_state), &request_key);
    assert_eq!(
        transition
            .next_state
            .nav
            .current()
            .overlays()
            .confirmation_values(),
        expected_values.as_ref()
    );
}

#[test]
fn confirmation_draft_and_focus_restore_with_the_exact_same_definition_instance() {
    let field = bounded_continuation_string_field("release-note", "ready", 5);
    let (mut state, field_id) = provider_confirmation_with_field(field);
    let request_key = latest_request_key(&state).clone();
    let owner_instance = state.nav.current().id;

    assert!(
        state
            .set_provider_confirmation_draft(field_id.clone(), TypedValue::String("x".to_owned()),)
    );
    state = state
        .apply_message(jefe::messages::AppMessage::Provider(Box::new(
            ProviderMessage::CycleConfirmationFocus,
        )))
        .unwrap_or_else(|error| panic!("focus continuation field: {error}"))
        .next_state;
    assert_eq!(
        state.nav.current().overlays().confirmation_focused_field(),
        Some(&field_id)
    );

    state.enter_provider_route(
        jefe::workbench::RouteId::from_static("dashboard"),
        jefe::workbench::ActivationValues::empty(),
    );
    assert_ne!(state.nav.current().id, owner_instance);
    assert!(
        state
            .nav
            .current()
            .overlays()
            .confirmation_values()
            .is_none()
    );
    state.leave_screen();

    assert_eq!(state.nav.current().id, owner_instance);
    assert_eq!(
        state
            .nav
            .current()
            .overlays()
            .confirmation_values()
            .and_then(|values| values.get(&field_id)),
        Some(&TypedValue::String("x".to_owned()))
    );
    assert_eq!(
        state.nav.current().overlays().confirmation_focused_field(),
        Some(&field_id)
    );
    assert_eq!(latest_request_key(&state), &request_key);
    assert_eq!(state.provider_requests.pending_confirmation_count(), 1);
    assert!(state.take_staged_effects().is_empty());
}

#[test]
fn confirmation_draft_boundary_rejects_complete_and_wrong_syntax_without_mutation() {
    let field = bounded_continuation_string_field("release-note", "ready", 5);
    let (mut state, field_id) = provider_confirmation_with_field(field);
    let request_key = latest_request_key(&state).clone();
    let initial_values = state
        .nav
        .current()
        .overlays()
        .confirmation_values()
        .cloned();

    assert!(
        !state.set_provider_confirmation_draft(
            field_id.clone(),
            TypedValue::String("ready".to_owned()),
        )
    );
    assert!(!state.set_provider_confirmation_draft(field_id, TypedValue::Bool(true)));
    assert_eq!(
        state.nav.current().overlays().confirmation_values(),
        initial_values.as_ref()
    );
    assert_eq!(latest_request_key(&state), &request_key);
    assert_eq!(state.provider_requests.pending_confirmation_count(), 1);
    assert!(state.take_staged_effects().is_empty());
}

#[test]
fn accepted_notice_applies_only_while_exact_screen_instance_is_current() {
    let mut state = crate::test_app_state();
    let key = active_request(&mut state);
    let notice = ProviderNotice {
        severity: ProviderNoticeSeverity::Info,
        message: "completed".to_owned(),
    };

    let applied = prepare_provider_host_outcome_state(
        &mut state,
        &key,
        ProviderHostOutcome::Notice(notice.clone()),
    );
    assert!(matches!(applied, Ok(ProviderHostAction::None)));
    assert_eq!(state.provider_notice, Some(notice));
    assert_eq!(state.warning_message.as_deref(), Some("completed"));

    state.show_screen(ScreenId::Issues);
    state.provider_notice = None;
    state.warning_message = None;
    let refusal = prepare_provider_host_outcome_state(
        &mut state,
        &key,
        ProviderHostOutcome::Notice(ProviderNotice {
            severity: ProviderNoticeSeverity::Warning,
            message: "stale".to_owned(),
        }),
    );
    assert_eq!(
        refusal,
        Err("provider outcome authority is stale".to_owned())
    );
    assert!(state.provider_notice.is_none());
    assert!(state.warning_message.is_none());
}

#[test]
fn provider_activation_conversion_rejects_nested_and_wrong_kind_values() {
    let name = Id::parse("query").unwrap_or_else(|error| panic!("field: {error}"));
    let schema = vec![jefe::workbench::ActivationField {
        name: name.clone(),
        kind: jefe::workbench::ActivationKind::Text,
    }];
    let mut nested = TypedMap::new();
    nested.insert(name.clone(), TypedValue::Map(TypedMap::new()));
    assert!(provider_activation_values(&schema, nested).is_err());

    let mut valid = TypedMap::new();
    valid.insert(name, TypedValue::String("open".to_owned()));
    assert!(provider_activation_values(&schema, valid).is_ok());
}

#[test]
fn refresh_requires_the_exact_current_resource_and_supported_screen() {
    let mut state = crate::test_app_state();
    state.show_screen(ScreenId::Issues);
    let key = active_request(&mut state);

    let accepted = prepare_provider_host_outcome_state(
        &mut state,
        &key,
        ProviderHostOutcome::Refresh {
            resource_ref: TypedMap::new(),
        },
    );
    assert_eq!(accepted, Ok(ProviderHostAction::Refresh(ScreenId::Issues)));

    let mut different = TypedMap::new();
    different.insert(
        Id::parse("repository").unwrap_or_else(|error| panic!("field: {error}")),
        TypedValue::String("other/repository".to_owned()),
    );
    let refused = prepare_provider_host_outcome_state(
        &mut state,
        &key,
        ProviderHostOutcome::Refresh {
            resource_ref: different,
        },
    );
    assert_eq!(
        refused,
        Err("provider refresh no longer owns the current resource".to_owned())
    );
}

#[test]
fn provider_navigation_rejects_core_local_and_foreign_package_routes() {
    let declared =
        Id::parse("vendor.pkg.open").unwrap_or_else(|error| panic!("declared route: {error}"));
    let policy = ActionPolicy::new(
        ActionConfirmation::None,
        vec![ActionOutcome::NavigateDeclaredRoute],
        false,
    )
    .with_declared_routes(vec![declared.clone()]);
    let mut state = crate::test_app_state();
    let key = active_request_with_policy(&mut state, policy);

    for route in ["actions", "local.open", "vendor.other.open"] {
        let refusal = prepare_provider_host_outcome_state(
            &mut state,
            &key,
            ProviderHostOutcome::Navigate {
                route_id: Id::parse(route).unwrap_or_else(|error| panic!("route {route}: {error}")),
                activation: TypedMap::new(),
            },
        );
        assert_eq!(
            refusal,
            Err("provider requested a route not declared by its package".to_owned())
        );
    }

    let declared_but_not_composed = prepare_provider_host_outcome_state(
        &mut state,
        &key,
        ProviderHostOutcome::Navigate {
            route_id: declared,
            activation: TypedMap::new(),
        },
    );
    assert_eq!(
        declared_but_not_composed,
        Err("provider requested an unknown route".to_owned())
    );
}

#[test]
fn provider_navigation_refuses_to_bypass_the_dirty_guard() {
    use jefe::state::navigation_dirty::{DraftToken, SaveIntent};

    let mut state = crate::test_app_state();
    let key = active_request(&mut state);
    let original_screen = state.screen();
    state.mark_screen_dirty(
        DraftToken::next(),
        SaveIntent::Unavailable {
            reason: "test draft has no save target",
        },
    );

    let refused = prepare_provider_host_outcome_state(
        &mut state,
        &key,
        ProviderHostOutcome::Navigate {
            route_id: Id::parse("actions").unwrap_or_else(|error| panic!("route: {error}")),
            activation: TypedMap::new(),
        },
    );
    assert_eq!(
        refused,
        Err("provider navigation is blocked by unsaved changes".to_owned())
    );
    assert_eq!(state.screen(), original_screen);
}

#[test]
fn outcome_completion_closes_the_ledger_before_navigation_changes_generation() {
    let mut state = crate::test_app_state();
    let route = Id::parse("actions").unwrap_or_else(|error| panic!("declared route: {error}"));
    let policy = ActionPolicy::new(
        ActionConfirmation::None,
        vec![ActionOutcome::NavigateDeclaredRoute],
        false,
    )
    .with_declared_routes(vec![route.clone()]);
    let key = active_request_with_policy(&mut state, policy);
    let owner = key.owner.clone();
    let correlation = state
        .pending_effects
        .register(
            owner,
            SemanticKey::new(EffectFamily::Provider, "outcome-provider.notice-1"),
            RetryPolicy::Never,
        )
        .unwrap_or_else(|error| panic!("register effect: {error}"));
    let issued = IssuedEffect {
        effect: Effect::Provider(ProviderEffect::ApplyOutcome {
            key: key.clone(),
            outcome: ProviderHostOutcome::Navigate {
                route_id: route,
                activation: TypedMap::new(),
            },
        }),
        correlation: correlation.clone(),
        retry: RetryPolicy::Never,
    };
    let action = prepare_provider_host_outcome_state(
        &mut state,
        &key,
        match &issued.effect {
            Effect::Provider(ProviderEffect::ApplyOutcome { outcome, .. }) => outcome.clone(),
            _ => panic!("fixture must carry a provider host outcome"),
        },
    )
    .unwrap_or_else(|error| panic!("prepare outcome: {error}"));

    complete_provider_outcome_state(
        &mut state,
        &issued,
        Ok(ProviderResponse::OutcomeApplied { key }),
    );
    assert!(!state.pending_effects.is_pending(&correlation));

    apply_provider_host_action_state(&mut state, &action);
    assert_eq!(state.screen(), ScreenId::Actions);
    assert!(!state.pending_effects.is_pending(&correlation));

    state = state.apply(AppEvent::Back).committed_pure();
    assert!(state.nav.current().provider_surface_action().is_none());
    assert!(state.latest_current_provider_request().is_some());
}
