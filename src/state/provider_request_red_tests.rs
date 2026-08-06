//! RED-first remediation tests for the handle-free provider request state
//! (issue #390 CW-10, Slice B).
//!
//! Each test proves one of the source-grounded fixes requested before commit:
//! - Non-empty resource refs survive the confirmation round-trip
//! - Policy mismatch (confirmation mode / RequestHostConfirmation / destructive)
//!   rejects the RequestHostConfirmation outcome
//! - Undeclared outcome kind is rejected before terminal commit
//! - Panel/migrated-config outcomes are rejected in CW-10
//! - Invocation B carries original arguments/context and exact continuation
//! - Post-terminal bytes are observable as PLG-E502, not silently ignored
//! - Progress fault is observable as PLG-E502 while marking generation
//!   unavailable
//! - Generation exhaustion fails typed (pure helper)
//! - No duplicate outbound queue exists in state

use crate::domain::effects::ProviderRequestKey;
use crate::domain::plugin::action::{ActionConfirmation, ActionOutcome};
use crate::domain::{Id, TypedMap, TypedValue};
use crate::runtime::provider::protocol::{Outcome, ProgressPayload};

use super::{
    ActionPolicy, ConfirmInput, InvokeInput, ProviderRequestError, ProviderRequestState,
    next_generation,
};

// ── shared helpers (duplicated from the acceptance test module) ──────────

fn owner() -> Id {
    Id::parse("host").unwrap_or_else(|_e| panic!("valid owner id"))
}

fn action() -> Id {
    Id::parse("provider.run").unwrap_or_else(|_e| panic!("valid action id"))
}

fn screen() -> Id {
    Id::parse("dashboard").unwrap_or_else(|_e| panic!("valid screen id"))
}

fn empty_map() -> TypedMap {
    TypedMap::new()
}

fn non_empty_map() -> TypedMap {
    let mut map = TypedMap::new();
    map.insert(
        Id::parse("resource.ref").unwrap_or_else(|_e| panic!("valid id")),
        TypedValue::String("issue-42".to_owned()),
    );
    map
}

fn policy(
    confirmation: ActionConfirmation,
    outcomes: &[ActionOutcome],
    destructive: bool,
) -> ActionPolicy {
    ActionPolicy::new(confirmation, outcomes.to_vec(), destructive)
}

fn default_policy() -> ActionPolicy {
    policy(ActionConfirmation::None, &[ActionOutcome::Notice], false)
}

fn continuation_policy() -> ActionPolicy {
    policy(
        ActionConfirmation::ProviderContinuation,
        &[
            ActionOutcome::RequestHostConfirmation,
            ActionOutcome::Notice,
        ],
        false,
    )
}

fn destructive_continuation_policy() -> ActionPolicy {
    policy(
        ActionConfirmation::ProviderContinuation,
        &[
            ActionOutcome::RequestHostConfirmation,
            ActionOutcome::Notice,
        ],
        true,
    )
}

fn do_invoke(state: &mut ProviderRequestState) -> super::InvokeOutcome {
    do_invoke_with(state, &default_policy(), empty_map(), empty_map())
}

fn do_invoke_with(
    state: &mut ProviderRequestState,
    policy: &ActionPolicy,
    refs: TypedMap,
    args: TypedMap,
) -> super::InvokeOutcome {
    state
        .invoke(InvokeInput {
            owner: &owner(),
            action_id: &action(),
            context_screen: &screen(),
            context_instance: &screen(),
            context_refs: &refs,
            arguments: &args,
            policy,
        })
        .unwrap_or_else(|e| panic!("invoke: {e}"))
}

fn progress(seq: u16, completed: Option<u64>, total: Option<u64>) -> ProgressPayload {
    ProgressPayload {
        sequence: seq,
        message: format!("step {seq}"),
        completed,
        total,
    }
}

fn notice_outcome() -> Outcome {
    Outcome::Notice {
        severity: crate::runtime::provider::protocol::Severity::Info,
        message: "completed".to_owned(),
    }
}

fn confirmation_outcome(conf_id: &str, destructive: bool) -> Outcome {
    Outcome::RequestHostConfirmation {
        confirmation_id: Id::parse(conf_id).unwrap_or_else(|_e| panic!("valid conf id")),
        title: "Confirm Action".to_owned(),
        body: "Are you sure?".to_owned(),
        confirm_label: "Yes, proceed".to_owned(),
        destructive,
        continuation_schema: vec![],
    }
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

// ── panel/migrated-config outcomes rejected in CW-10 ─────────────────────

#[test]
fn replace_panel_outcome_rejected() {
    let mut state = ProviderRequestState::new();
    let pol = policy(
        ActionConfirmation::None,
        &[ActionOutcome::ReplaceOwnedPanel, ActionOutcome::Notice],
        false,
    );
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    let replace = Outcome::ReplacePanel {
        panel_instance_id: Id::parse("panel.1").unwrap_or_else(|_e| panic!("panel id")),
        snapshot: crate::runtime::provider::protocol::PanelSnapshot(empty_map()),
    };
    let result = state.record_outcome(&outcome.key, replace, 1000);
    assert_eq!(result, Err(ProviderRequestError::UnsupportedOutcome));
    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert!(request.is_terminal());
    assert!(request.unavailable_reason().is_some());
}

#[test]
fn close_panel_outcome_rejected() {
    let mut state = ProviderRequestState::new();
    let pol = policy(
        ActionConfirmation::None,
        &[ActionOutcome::CloseOwnedPanel, ActionOutcome::Notice],
        false,
    );
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    let close = Outcome::ClosePanel {
        panel_instance_id: Id::parse("panel.1").unwrap_or_else(|_e| panic!("panel id")),
    };
    let result = state.record_outcome(&outcome.key, close, 1000);
    assert_eq!(result, Err(ProviderRequestError::UnsupportedOutcome));
}

#[test]
fn migrated_config_outcome_rejected() {
    let mut state = ProviderRequestState::new();
    let pol = policy(ActionConfirmation::None, &[ActionOutcome::Notice], false);
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    let migrated = Outcome::MigratedConfig {
        migration: crate::runtime::provider::protocol::MigratedConfig(empty_map()),
    };
    let result = state.record_outcome(&outcome.key, migrated, 1000);
    assert_eq!(result, Err(ProviderRequestError::UnsupportedOutcome));
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
    assert_eq!(result, Err(ProviderRequestError::ConfirmationNotFound));
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
        .record_outcome(&outcome.key, confirmation_outcome("conf.val", false), 1000)
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

// ── post-terminal bytes observable as PLG-E502 ───────────────────────────

#[test]
fn progress_after_terminal_is_post_terminal_not_silent() {
    let mut state = ProviderRequestState::new();
    let outcome = do_invoke(&mut state);
    state
        .record_outcome(&outcome.key, notice_outcome(), 1000)
        .unwrap_or_else(|e| panic!("terminal outcome: {e}"));

    let result = state.record_progress(&outcome.key, progress(1, None, None));
    assert_eq!(result, Err(ProviderRequestError::PostTerminal));
    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert!(request.completed_outcome().is_some());
}

#[test]
fn outcome_after_terminal_is_post_terminal_not_silent() {
    let mut state = ProviderRequestState::new();
    let outcome = do_invoke(&mut state);
    state
        .record_error(&outcome.key, "first failure".to_owned())
        .unwrap_or_else(|e| panic!("terminal error: {e}"));

    let result = state.record_outcome(&outcome.key, notice_outcome(), 1000);
    assert_eq!(result, Err(ProviderRequestError::PostTerminal));
    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert_eq!(request.failed_message(), Some("first failure"));
}

#[test]
fn error_after_terminal_is_post_terminal_not_silent() {
    let mut state = ProviderRequestState::new();
    let outcome = do_invoke(&mut state);
    state
        .record_outcome(&outcome.key, notice_outcome(), 1000)
        .unwrap_or_else(|e| panic!("terminal outcome: {e}"));

    let result = state.record_error(&outcome.key, "late error".to_owned());
    assert_eq!(result, Err(ProviderRequestError::PostTerminal));
    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert!(request.completed_outcome().is_some());
}

#[test]
fn post_terminal_error_message_contains_plg_e502_code() {
    let error = ProviderRequestError::PostTerminal;
    assert!(
        error.to_string().contains("PLG-E502"),
        "PostTerminal error must carry the PLG-E502 code, got: {error}"
    );
}

// ── progress fault visible as PLG-E502 ───────────────────────────────────

#[test]
fn progress_fault_error_message_contains_plg_e502_code() {
    let error = ProviderRequestError::ProgressFault;
    assert!(
        error.to_string().contains("PLG-E502"),
        "ProgressFault error must carry the PLG-E502 code, got: {error}"
    );
}

#[test]
fn progress_fault_marks_generation_unavailable() {
    let mut state = ProviderRequestState::new();
    let outcome = do_invoke(&mut state);
    state
        .record_progress(&outcome.key, progress(1, Some(1), Some(4)))
        .unwrap_or_else(|e| panic!("first progress: {e}"));
    let result = state.record_progress(&outcome.key, progress(3, Some(3), Some(4)));
    assert_eq!(result, Err(ProviderRequestError::ProgressFault));
    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert!(request.is_terminal());
    assert_eq!(
        request.unavailable_reason(),
        Some(super::UnavailableReason::Protocol)
    );
}

// ── generation exhaustion fails typed ─────────────────────────────────────

#[test]
fn generation_exhaustion_at_u64_max() {
    assert_eq!(
        next_generation(u64::MAX),
        Err(ProviderRequestError::GenerationExhausted)
    );
}

#[test]
fn generation_succeeds_below_max() {
    assert_eq!(next_generation(0), Ok(1));
    assert_eq!(next_generation(1), Ok(2));
    assert_eq!(next_generation(u64::MAX - 1), Ok(u64::MAX));
}

// ── no duplicate outbound queue ───────────────────────────────────────────

#[test]
fn no_outbound_queue_in_state() {
    let mut state = ProviderRequestState::new();
    do_invoke(&mut state);
    assert_eq!(state.active_count(), 1);
    assert_eq!(state.pending_confirmation_count(), 0);
}

// ── confirm must not advance the generation counter on failure ───────────
//
// An invalid (wrong binding) or expired confirm must not allocate a
// generation: the counter is advanced only when the single-use token is
// validated, consumed, and invocation B is committed atomically.

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

    // The failed confirm must not have allocated a generation.
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
        1000 + super::CONFIRMATION_TTL_SECONDS + 1,
    );
    assert!(matches!(result, Err(ProviderRequestError::Expired { .. })));

    let next = do_invoke(&mut state);
    assert_eq!(
        next.key.generation,
        outcome.key.generation + 1,
        "expired confirm must not advance the generation counter"
    );
}

// ── fail-fast single-use expiry: an expired token is consumed once ───────
//
// A repeated expired attempt on the same id must see ConfirmationNotFound,
// never Expired again — the token is gone, so it cannot be probed or reused.

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
        1000 + super::CONFIRMATION_TTL_SECONDS + 1,
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
        1000 + super::CONFIRMATION_TTL_SECONDS + 2,
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
//
// First terminal wins. A cancel that arrives after the request already
// completed/failed/cancelled/became-unavailable is an explicit no-effect
// result: the reducer returns AlreadyTerminal and the AppState handler stages
// no CancelRequest.

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
        Ok(super::CancelOutcome::AlreadyTerminal {
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
        .mark_unavailable(&outcome.key, super::UnavailableReason::Timeout)
        .unwrap_or_else(|e| panic!("mark: {e}"));

    let result = state.cancel(&outcome.key);
    assert_eq!(
        result,
        Ok(super::CancelOutcome::AlreadyTerminal {
            key: outcome.key.clone()
        })
    );
    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert_eq!(
        request.unavailable_reason(),
        Some(super::UnavailableReason::Timeout)
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
        Ok(super::CancelOutcome::Cancelled {
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

    let mut state = AppState::default();
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
    // And the first terminal result remains authoritative.
    state = transition.next_state;
    let request = state
        .provider_requests
        .request(&key)
        .unwrap_or_else(|| panic!("request present"));
    assert!(request.completed_outcome().is_some());
    assert!(!request.is_cancelled());
}
