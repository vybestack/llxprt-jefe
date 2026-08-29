//! Policy/outcome validation and confirmation round-trip RED tests.

use crate::domain::plugin::action::{ActionConfirmation, ActionOutcome};
use crate::domain::plugin::field::{Field, FieldDraft, FieldKind, RestartScope};
use crate::domain::{Id, TypedMap, TypedValue};
use crate::runtime::provider::protocol::Outcome;

use super::super::{ConfirmInput, ProviderRequestError, ProviderRequestState};
use super::support::{
    action, confirmation_outcome, confirmation_outcome_with_schema, continuation_policy,
    default_policy, destructive_continuation_policy, do_invoke_with, empty_map, non_empty_map,
    owner, policy, screen,
};

fn required_string_field(id: &str) -> Field {
    Field::parse(FieldDraft {
        id: Id::parse(id).unwrap_or_else(|error| panic!("valid field id: {error}")),
        label: "Confirmation".to_owned(),
        description: None,
        kind: FieldKind::String,
        required: true,
        default: None,
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| panic!("valid string field: {error}"))
}

// ── policy mismatch on RequestHostConfirmation ───────────────────────────

#[test]
fn confirmation_rejects_action_without_provider_continuation_mode() {
    let mut state = ProviderRequestState::new();
    let pol = policy(
        ActionConfirmation::HostBeforeInvoke,
        &[ActionOutcome::RequestHostConfirmation],
        false,
    );
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    let result = state.record_outcome(&outcome.key, confirmation_outcome("conf.bad", false), 1000);
    assert_eq!(result, Err(ProviderRequestError::PolicyViolation));
    assert_eq!(state.pending_confirmation_count(), 0);
    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert!(request.is_terminal());
    assert!(request.unavailable_reason().is_some());
}

#[test]
fn confirmation_rejects_action_without_request_host_confirmation_outcome() {
    let mut state = ProviderRequestState::new();
    let pol = policy(
        ActionConfirmation::ProviderContinuation,
        &[ActionOutcome::Notice],
        false,
    );
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    let result = state.record_outcome(&outcome.key, confirmation_outcome("conf.bad", false), 1000);
    assert_eq!(result, Err(ProviderRequestError::PolicyViolation));
    assert_eq!(state.pending_confirmation_count(), 0);
}

#[test]
fn confirmation_rejects_destructive_flag_mismatch() {
    let mut state = ProviderRequestState::new();
    let pol = continuation_policy();
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    let result = state.record_outcome(&outcome.key, confirmation_outcome("conf.bad", true), 1000);
    assert_eq!(result, Err(ProviderRequestError::PolicyViolation));
    assert_eq!(state.pending_confirmation_count(), 0);
}

#[test]
fn destructive_confirmation_accepted_when_policy_matches() {
    let mut state = ProviderRequestState::new();
    let pol = destructive_continuation_policy();
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    let result = state.record_outcome(&outcome.key, confirmation_outcome("conf.ok", true), 1000);
    assert!(result.is_ok());
    assert_eq!(state.pending_confirmation_count(), 1);
}

// ── undeclared outcome kind ──────────────────────────────────────────────

#[test]
fn undeclared_outcome_kind_rejected_before_terminal_commit() {
    let mut state = ProviderRequestState::new();
    let pol = default_policy();
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    let navigate = Outcome::Navigate {
        route_id: Id::parse("route.unknown").unwrap_or_else(|_e| panic!("route id")),
        activation: empty_map(),
    };
    let result = state.record_outcome(&outcome.key, navigate, 1000);
    assert_eq!(result, Err(ProviderRequestError::UndeclaredOutcome));
    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert!(request.is_terminal());
    assert!(request.unavailable_reason().is_some());
}

#[test]
fn declared_outcome_kind_accepted() {
    let mut state = ProviderRequestState::new();
    let pol = policy(
        ActionConfirmation::None,
        &[ActionOutcome::NavigateDeclaredRoute, ActionOutcome::Notice],
        false,
    );
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    let navigate = Outcome::Navigate {
        route_id: Id::parse("route.known").unwrap_or_else(|_e| panic!("route id")),
        activation: empty_map(),
    };
    let result = state.record_outcome(&outcome.key, navigate, 1000);
    assert!(result.is_ok());
    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert!(request.completed_outcome().is_some());
}

// ── non-empty resource refs across confirmation ──────────────────────────

#[test]
fn non_empty_resource_refs_survive_confirmation_round_trip() {
    let mut state = ProviderRequestState::new();
    let pol = continuation_policy();
    let refs = non_empty_map();
    let outcome = do_invoke_with(&mut state, &pol, refs.clone(), empty_map());
    state
        .record_outcome(&outcome.key, confirmation_outcome("conf.refs", false), 1000)
        .unwrap_or_else(|e| panic!("outcome: {e}"));

    let conf_id = Id::parse("conf.refs").unwrap_or_else(|_e| panic!("conf id"));
    let result = state
        .confirm(
            ConfirmInput {
                owner: &owner(),
                action_id: &action(),
                context_screen: &screen(),
                context_instance: &screen(),
                context_refs: &refs,
                generation: outcome.key.generation,
                confirmation_id: &conf_id,
                values: &empty_map(),
            },
            1100,
        )
        .unwrap_or_else(|e| panic!("confirm: {e}"));

    assert_eq!(result.invocation.context_refs, refs);
    assert!(!result.invocation.context_refs.is_empty());
}

#[test]
fn confirm_rejects_mismatched_resource_refs() {
    let mut state = ProviderRequestState::new();
    let pol = continuation_policy();
    let refs = non_empty_map();
    let outcome = do_invoke_with(&mut state, &pol, refs, empty_map());
    state
        .record_outcome(&outcome.key, confirmation_outcome("conf.refs", false), 1000)
        .unwrap_or_else(|e| panic!("outcome: {e}"));

    let empty = empty_map();
    let conf_id = Id::parse("conf.refs").unwrap_or_else(|_e| panic!("conf id"));
    let result = state.confirm(
        ConfirmInput {
            owner: &owner(),
            action_id: &action(),
            context_screen: &screen(),
            context_instance: &screen(),
            context_refs: &empty,
            generation: outcome.key.generation,
            confirmation_id: &conf_id,
            values: &empty,
        },
        1100,
    );
    assert_eq!(result, Err(ProviderRequestError::StaleContext));
}

fn nested_head_context(head: &str) -> TypedMap {
    let mut value = TypedMap::new();
    value.insert(
        Id::parse("head").unwrap_or_else(|error| panic!("head field: {error}")),
        TypedValue::String(head.to_owned()),
    );
    let mut resource = TypedMap::new();
    resource.insert(
        Id::parse("semantic-key").unwrap_or_else(|error| panic!("semantic key field: {error}")),
        TypedValue::String("review-42".to_owned()),
    );
    resource.insert(
        Id::parse("value").unwrap_or_else(|error| panic!("value field: {error}")),
        TypedValue::Map(value),
    );
    let mut refs = TypedMap::new();
    refs.insert(
        Id::parse("review.selection").unwrap_or_else(|error| panic!("resource ref: {error}")),
        TypedValue::Map(resource),
    );
    refs
}

#[test]
fn destructive_confirmation_rejects_changed_head_with_same_semantic_key_without_consuming_token() {
    let mut state = ProviderRequestState::new();
    let policy = destructive_continuation_policy();
    let original = nested_head_context("head-a");
    let outcome = do_invoke_with(&mut state, &policy, original.clone(), empty_map());
    state
        .record_outcome(
            &outcome.key,
            confirmation_outcome("conf.same-key-head", true),
            1000,
        )
        .unwrap_or_else(|error| panic!("confirmation outcome: {error}"));
    let confirmation_id =
        Id::parse("conf.same-key-head").unwrap_or_else(|error| panic!("confirmation id: {error}"));
    let values = empty_map();

    let rejected = state.confirm(
        ConfirmInput {
            owner: &owner(),
            action_id: &action(),
            context_screen: &screen(),
            context_instance: &screen(),
            context_refs: &nested_head_context("head-b"),
            generation: outcome.key.generation,
            confirmation_id: &confirmation_id,
            values: &values,
        },
        1100,
    );

    assert_eq!(rejected, Err(ProviderRequestError::StaleContext));
    assert_eq!(state.pending_confirmation_count(), 1);
    let confirmed = state
        .confirm(
            ConfirmInput {
                owner: &owner(),
                action_id: &action(),
                context_screen: &screen(),
                context_instance: &screen(),
                context_refs: &original,
                generation: outcome.key.generation,
                confirmation_id: &confirmation_id,
                values: &values,
            },
            1101,
        )
        .unwrap_or_else(|error| panic!("restored exact context must confirm: {error}"));
    assert_eq!(confirmed.invocation.context_refs, original);
    assert_eq!(state.pending_confirmation_count(), 0);
}

// ── invocation B carries original arguments and context ──────────────────

#[test]
fn invocation_b_carries_original_arguments() {
    let mut state = ProviderRequestState::new();
    let pol = continuation_policy();
    let mut args = TypedMap::new();
    args.insert(
        Id::parse("branch").unwrap_or_else(|_e| panic!("arg id")),
        TypedValue::String("main".to_owned()),
    );
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), args.clone());
    state
        .record_outcome(&outcome.key, confirmation_outcome("conf.args", false), 1000)
        .unwrap_or_else(|e| panic!("outcome: {e}"));

    let empty = empty_map();
    let conf_id = Id::parse("conf.args").unwrap_or_else(|_e| panic!("conf id"));
    let result = state
        .confirm(
            ConfirmInput {
                owner: &owner(),
                action_id: &action(),
                context_screen: &screen(),
                context_instance: &screen(),
                context_refs: &empty,
                generation: outcome.key.generation,
                confirmation_id: &conf_id,
                values: &empty,
            },
            1100,
        )
        .unwrap_or_else(|e| panic!("confirm: {e}"));

    assert_eq!(result.invocation.arguments, args);
}

#[test]
fn invocation_b_carries_exact_continuation_values() {
    let mut state = ProviderRequestState::new();
    let pol = continuation_policy();
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    state
        .record_outcome(
            &outcome.key,
            confirmation_outcome_with_schema(
                "conf.val",
                false,
                vec![required_string_field("confirm.text")],
            ),
            1000,
        )
        .unwrap_or_else(|e| panic!("outcome: {e}"));

    let mut values = TypedMap::new();
    values.insert(
        Id::parse("confirm.text").unwrap_or_else(|_e| panic!("value id")),
        TypedValue::String("yes".to_owned()),
    );
    let conf_id = Id::parse("conf.val").unwrap_or_else(|_e| panic!("conf id"));
    let result = state
        .confirm(
            ConfirmInput {
                owner: &owner(),
                action_id: &action(),
                context_screen: &screen(),
                context_instance: &screen(),
                context_refs: &empty_map(),
                generation: outcome.key.generation,
                confirmation_id: &conf_id,
                values: &values,
            },
            1100,
        )
        .unwrap_or_else(|e| panic!("confirm: {e}"));

    let continuation = result
        .invocation
        .continuation
        .as_ref()
        .unwrap_or_else(|| panic!("continuation present"));
    assert_eq!(continuation.values, values);
    assert!(continuation.approved);
}

#[test]
fn invocation_b_carries_original_context_screen_and_instance() {
    let mut state = ProviderRequestState::new();
    let pol = continuation_policy();
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    state
        .record_outcome(&outcome.key, confirmation_outcome("conf.ctx", false), 1000)
        .unwrap_or_else(|e| panic!("outcome: {e}"));

    let empty = empty_map();
    let conf_id = Id::parse("conf.ctx").unwrap_or_else(|_e| panic!("conf id"));
    let result = state
        .confirm(
            ConfirmInput {
                owner: &owner(),
                action_id: &action(),
                context_screen: &screen(),
                context_instance: &screen(),
                context_refs: &empty,
                generation: outcome.key.generation,
                confirmation_id: &conf_id,
                values: &empty,
            },
            1100,
        )
        .unwrap_or_else(|e| panic!("confirm: {e}"));

    assert_eq!(result.invocation.context_screen, screen());
    assert_eq!(result.invocation.context_instance, screen());
}
