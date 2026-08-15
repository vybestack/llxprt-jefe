//! Confirm atomicity (generation counter), retry of an unknown old key, and
//! cancel-after-terminal no-effect semantics.

use crate::domain::Id;
use crate::domain::effects::ProviderRequestKey;
use crate::runtime::provider::protocol::Outcome;

use super::super::{
    CONFIRMATION_TTL_SECONDS, CancelOutcome, ConfirmInput, InvokeInput, ProviderRequestError,
    ProviderRequestState, UnavailableReason,
};
use super::support::{
    action, confirmation_outcome, continuation_policy, default_policy, do_invoke, do_invoke_with,
    empty_map, notice_outcome, owner, screen,
};

// ── confirm must not advance the generation counter on failure ───────────

#[test]
fn confirm_wrong_binding_does_not_advance_generation() {
    let mut state = ProviderRequestState::new();
    let pol = continuation_policy();
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    state
        .record_outcome(&outcome.key, confirmation_outcome("conf.gen", false), 1000)
        .unwrap_or_else(|e| panic!("outcome: {e}"));

    let wrong_owner = Id::parse("other").unwrap_or_else(|_e| panic!("owner id"));
    let empty = empty_map();
    let conf_id = Id::parse("conf.gen").unwrap_or_else(|_e| panic!("conf id"));
    let result = state.confirm(
        ConfirmInput {
            owner: &wrong_owner,
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
    assert_eq!(result, Err(ProviderRequestError::ConfirmationNotFound));

    let next = do_invoke(&mut state);
    assert_eq!(
        next.key.generation,
        outcome.key.generation + 1,
        "invalid confirm must not advance the generation counter"
    );
}

#[test]
fn confirm_expired_does_not_advance_generation() {
    let mut state = ProviderRequestState::new();
    let pol = continuation_policy();
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    state
        .record_outcome(&outcome.key, confirmation_outcome("conf.exp", false), 1000)
        .unwrap_or_else(|e| panic!("outcome: {e}"));

    let empty = empty_map();
    let conf_id = Id::parse("conf.exp").unwrap_or_else(|_e| panic!("conf id"));
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
        1000 + CONFIRMATION_TTL_SECONDS + 1,
    );
    assert!(matches!(result, Err(ProviderRequestError::Expired { .. })));

    let next = do_invoke(&mut state);
    assert_eq!(
        next.key.generation,
        outcome.key.generation + 1,
        "expired confirm must not advance the generation counter"
    );
}

#[test]
fn confirm_expired_consumes_token_single_use() {
    let mut state = ProviderRequestState::new();
    let pol = continuation_policy();
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    state
        .record_outcome(&outcome.key, confirmation_outcome("conf.su", false), 1000)
        .unwrap_or_else(|e| panic!("outcome: {e}"));

    let empty = empty_map();
    let conf_id = Id::parse("conf.su").unwrap_or_else(|_e| panic!("conf id"));
    let first = state.confirm(
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
        1000 + CONFIRMATION_TTL_SECONDS + 1,
    );
    assert!(matches!(first, Err(ProviderRequestError::Expired { .. })));
    assert_eq!(state.pending_confirmation_count(), 0);

    let second = state.confirm(
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
        1000 + CONFIRMATION_TTL_SECONDS + 2,
    );
    assert_eq!(
        second,
        Err(ProviderRequestError::ConfirmationNotFound),
        "an expired token must be consumed once, not reusable"
    );
}

// ── retry of an unknown old key must not start a new request ─────────────

#[test]
fn retry_unknown_old_key_returns_unknown_generation() {
    let mut state = ProviderRequestState::new();
    let first = do_invoke(&mut state);

    let unknown_key = ProviderRequestKey {
        owner: owner(),
        action_id: action(),
        generation: first.key.generation + 999,
    };
    let pol = default_policy();
    let empty = empty_map();
    let result = state.retry(
        &unknown_key,
        InvokeInput {
            owner: &owner(),
            action_id: &action(),
            context_screen: &screen(),
            context_instance: &screen(),
            context_refs: &empty,
            arguments: &empty,
            policy: &pol,
        },
    );
    assert_eq!(
        result,
        Err(ProviderRequestError::UnknownGeneration),
        "retry of an unknown old key must not start a new request"
    );
    assert_eq!(
        state.active_count(),
        1,
        "no new request may be registered for an unknown old key"
    );
}

// ── cancel after a terminal request must stage no effect ─────────────────

#[test]
fn cancel_after_terminal_is_no_effect_result() {
    let mut state = ProviderRequestState::new();
    let outcome = do_invoke(&mut state);
    state
        .record_outcome(&outcome.key, notice_outcome(), 1000)
        .unwrap_or_else(|e| panic!("outcome: {e}"));

    let result = state.cancel(&outcome.key);
    assert_eq!(
        result,
        Ok(CancelOutcome::AlreadyTerminal {
            key: outcome.key.clone()
        })
    );

    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert!(request.completed_outcome().is_some());
    assert!(!request.is_cancelled());
}

#[test]
fn cancel_after_unavailable_is_no_effect_result() {
    let mut state = ProviderRequestState::new();
    let outcome = do_invoke(&mut state);
    state
        .mark_unavailable(&outcome.key, UnavailableReason::Timeout)
        .unwrap_or_else(|e| panic!("mark: {e}"));

    let result = state.cancel(&outcome.key);
    assert_eq!(
        result,
        Ok(CancelOutcome::AlreadyTerminal {
            key: outcome.key.clone()
        })
    );
    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert_eq!(
        request.unavailable_reason(),
        Some(UnavailableReason::Timeout)
    );
    assert!(!request.is_cancelled());
}

#[test]
fn cancel_live_request_stages_cancelled_outcome() {
    let mut state = ProviderRequestState::new();
    let outcome = do_invoke(&mut state);
    let result = state.cancel(&outcome.key);
    assert_eq!(
        result,
        Ok(CancelOutcome::Cancelled {
            key: outcome.key.clone()
        })
    );
    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert!(request.is_cancelled());
}

#[test]
fn cancel_after_terminal_stages_no_cancel_effect_at_app_state() {
    use crate::domain::effects::{Effect, ProviderEffect};
    use crate::messages::{AppMessage, ProviderMessage};
    use crate::runtime::provider::protocol::Severity;
    use crate::state::AppState;

    let owner = owner();
    let action = action();
    let screen = screen();
    let empty = empty_map();
    let policy = continuation_policy();

    let mut state = AppState::test_fixture();
    let transition = state
        .clone()
        .apply_message(AppMessage::Provider(Box::new(ProviderMessage::Invoke {
            owner: owner.clone(),
            action_id: action.clone(),
            context_screen: screen.clone(),
            context_instance: screen.clone(),
            context_refs: empty.clone(),
            arguments: empty.clone(),
            policy: policy.clone(),
        })))
        .unwrap_or_else(|e| panic!("invoke transition: {e:?}"));
    state = transition.next_state;

    let key = ProviderRequestKey {
        owner: owner.clone(),
        action_id: action.clone(),
        generation: 1,
    };
    let transition = state
        .clone()
        .apply_message(AppMessage::Provider(Box::new(ProviderMessage::Outcome {
            key: key.clone(),
            outcome: Outcome::Notice {
                severity: Severity::Info,
                message: "done".to_owned(),
            },
            now_epoch: 1000,
        })))
        .unwrap_or_else(|e| panic!("outcome transition: {e:?}"));
    state = transition.next_state;

    let transition = state
        .apply_message(AppMessage::Provider(Box::new(ProviderMessage::Cancel {
            key: key.clone(),
        })))
        .unwrap_or_else(|e| panic!("cancel transition: {e:?}"));
    assert!(
        transition.effects.iter().all(|issued| !matches!(
            issued.effect,
            Effect::Provider(ProviderEffect::CancelRequest { .. })
        )),
        "cancel after a terminal request must not stage a CancelRequest effect"
    );
    state = transition.next_state;
    let request = state
        .provider_requests
        .request(&key)
        .unwrap_or_else(|| panic!("request present"));
    assert!(request.completed_outcome().is_some());
    assert!(!request.is_cancelled());
}
