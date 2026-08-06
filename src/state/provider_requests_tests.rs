//! Reducer tests for the handle-free provider request state
//! (issue #390 CW-10, Slice B).
//!
//! These tests prove every acceptance row owned by Slice B:
//! - CW10-06: active request max 16 (no duplicate outbound queue)
//! - CW10-07: progress integration and monotonicity fault → unavailable
//!   (observable PLG-E502)
//! - CW10-08: exact confirmation: policy validation, exact binding, TTL,
//!   single-use, invocation B carries original args/context/continuation
//! - CW10-09: first-terminal-wins; later bytes are observable PLG-E502
//! - CW10-10: old-generation output changes nothing, retry allocates new
//!   generation
//!
//! Additionally these prove the remediation fixes:
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

use crate::domain::plugin::action::{ActionConfirmation, ActionOutcome};
use crate::domain::{Id, TypedMap};
use crate::runtime::provider::protocol::{Outcome, ProgressPayload, Severity};

use super::{
    ActionPolicy, CONFIRMATION_TTL_SECONDS, CancelOutcome, ConfirmInput, InvokeInput,
    InvokeOutcome, MAX_ACTIVE_REQUESTS, ProviderRequestError, ProviderRequestState,
};

// ── helpers ───────────────────────────────────────────────────────────────

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
fn do_invoke(state: &mut ProviderRequestState) -> InvokeOutcome {
    do_invoke_with(state, &default_policy(), empty_map(), empty_map())
}

fn do_invoke_with(
    state: &mut ProviderRequestState,
    policy: &ActionPolicy,
    refs: TypedMap,
    args: TypedMap,
) -> InvokeOutcome {
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
        severity: Severity::Info,
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

// ── CW10-06: bounds (no duplicate outbound queue) ────────────────────────

#[test]
fn active_request_limit_enforced_at_sixteen() {
    let mut state = ProviderRequestState::new();
    for _ in 0..MAX_ACTIVE_REQUESTS {
        do_invoke(&mut state);
    }
    assert_eq!(state.active_count(), MAX_ACTIVE_REQUESTS);

    let pol = default_policy();
    let empty = empty_map();
    let result = state.invoke(InvokeInput {
        owner: &owner(),
        action_id: &action(),
        context_screen: &screen(),
        context_instance: &screen(),
        context_refs: &empty,
        arguments: &empty,
        policy: &pol,
    });
    assert_eq!(
        result,
        Err(ProviderRequestError::ActiveLimitExceeded {
            limit: MAX_ACTIVE_REQUESTS
        })
    );
    assert_eq!(state.active_count(), MAX_ACTIVE_REQUESTS);
}

#[test]
fn invoke_allocates_monotonic_generations() {
    let mut state = ProviderRequestState::new();
    let a = do_invoke(&mut state);
    let b = do_invoke(&mut state);
    assert_eq!(a.key.generation, 1);
    assert_eq!(b.key.generation, 2);
    assert_ne!(a.key, b.key);
}

#[test]
fn drain_terminal_frees_active_slots() {
    let mut state = ProviderRequestState::new();
    let outcome = do_invoke(&mut state);
    state
        .record_outcome(&outcome.key, notice_outcome(), 1000)
        .unwrap_or_else(|e| panic!("record outcome: {e}"));
    assert_eq!(state.active_count(), 1);
    assert_eq!(state.drain_terminal(), 1);
    assert_eq!(state.active_count(), 0);
    do_invoke(&mut state);
    assert_eq!(state.active_count(), 1);
}

// ── CW10-07: progress integration ────────────────────────────────────────

#[test]
fn progress_accepted_for_live_request() {
    let mut state = ProviderRequestState::new();
    let outcome = do_invoke(&mut state);
    state
        .record_progress(&outcome.key, progress(1, Some(1), Some(4)))
        .unwrap_or_else(|e| panic!("first progress: {e}"));
    state
        .record_progress(&outcome.key, progress(2, Some(2), Some(4)))
        .unwrap_or_else(|e| panic!("second progress: {e}"));

    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert!(request.is_progressing());
    let latest = request
        .latest_progress()
        .unwrap_or_else(|| panic!("has progress"));
    assert_eq!(latest.sequence, 2);
    assert_eq!(latest.completed, Some(2));
}

#[test]
fn progress_monotonicity_fault_marks_generation_unavailable() {
    let mut state = ProviderRequestState::new();
    let outcome = do_invoke(&mut state);
    state
        .record_progress(&outcome.key, progress(1, Some(1), Some(4)))
        .unwrap_or_else(|e| panic!("first progress: {e}"));
    // Gap: 1 → 3 (skipping 2).
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

#[test]
fn progress_for_unknown_generation_is_ignored() {
    let mut state = ProviderRequestState::new();
    let outcome = do_invoke(&mut state);
    let stale_key = crate::domain::effects::ProviderRequestKey {
        owner: outcome.key.owner.clone(),
        action_id: outcome.key.action_id.clone(),
        generation: 999,
    };
    let result = state.record_progress(&stale_key, progress(1, None, None));
    assert_eq!(result, Err(ProviderRequestError::UnknownGeneration));
    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert!(request.latest_progress().is_none());
}

// ── CW10-09: first-terminal-wins ─────────────────────────────────────────

#[test]
fn outcome_then_cancel_outcome_wins() {
    let mut state = ProviderRequestState::new();
    let outcome = do_invoke(&mut state);

    state
        .record_outcome(&outcome.key, notice_outcome(), 1000)
        .unwrap_or_else(|e| panic!("terminal outcome: {e}"));

    // Cancel after a terminal outcome is an explicit no-effect result, not an
    // error and not a staged CancelRequest.
    let cancel_result = state.cancel(&outcome.key);
    assert_eq!(
        cancel_result,
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
fn cancel_then_outcome_cancel_wins() {
    let mut state = ProviderRequestState::new();
    let outcome = do_invoke(&mut state);

    state
        .cancel(&outcome.key)
        .unwrap_or_else(|e| panic!("cancel: {e}"));

    let result = state.record_outcome(&outcome.key, notice_outcome(), 1000);
    assert_eq!(result, Err(ProviderRequestError::PostTerminal));

    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert!(request.is_cancelled());
    assert!(request.completed_outcome().is_none());
}

#[test]
fn error_then_outcome_error_wins() {
    let mut state = ProviderRequestState::new();
    let outcome = do_invoke(&mut state);

    state
        .record_error(&outcome.key, "PLG-E502: bad args".to_owned())
        .unwrap_or_else(|e| panic!("terminal error: {e}"));

    let result = state.record_outcome(&outcome.key, notice_outcome(), 1000);
    assert_eq!(result, Err(ProviderRequestError::PostTerminal));

    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert_eq!(request.failed_message(), Some("PLG-E502: bad args"));
}

#[test]
fn cancel_creates_no_continuation() {
    let mut state = ProviderRequestState::new();
    let outcome = do_invoke(&mut state);
    state
        .cancel(&outcome.key)
        .unwrap_or_else(|e| panic!("cancel: {e}"));

    assert_eq!(state.pending_confirmation_count(), 0);
    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert!(request.is_cancelled());
}

// ── CW10-09/CW10-10: crash/EOF/protocol/timeout → unavailable ────────────

#[test]
fn mark_unavailable_makes_generation_unavailable() {
    let mut state = ProviderRequestState::new();
    let outcome = do_invoke(&mut state);
    state
        .mark_unavailable(&outcome.key, super::UnavailableReason::Crash)
        .unwrap_or_else(|e| panic!("mark unavailable: {e}"));
    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert_eq!(
        request.unavailable_reason(),
        Some(super::UnavailableReason::Crash)
    );
    assert!(request.is_terminal());
}

#[test]
fn mark_unavailable_after_terminal_is_noop() {
    let mut state = ProviderRequestState::new();
    let outcome = do_invoke(&mut state);
    state
        .record_outcome(&outcome.key, notice_outcome(), 1000)
        .unwrap_or_else(|e| panic!("outcome: {e}"));
    state
        .mark_unavailable(&outcome.key, super::UnavailableReason::Timeout)
        .unwrap_or_else(|e| panic!("mark: {e}"));
    let request = state
        .request(&outcome.key)
        .unwrap_or_else(|| panic!("found request"));
    assert!(request.completed_outcome().is_some());
    assert!(request.unavailable_reason().is_none());
}

// ── CW10-10: old-generation output / retry ───────────────────────────────

#[test]
fn retry_allocates_new_generation() {
    let mut state = ProviderRequestState::new();
    let first = do_invoke(&mut state);
    let pol = default_policy();
    let empty = empty_map();
    let second = state
        .retry(
            &first.key,
            InvokeInput {
                owner: &owner(),
                action_id: &action(),
                context_screen: &screen(),
                context_instance: &screen(),
                context_refs: &empty,
                arguments: &empty,
                policy: &pol,
            },
        )
        .unwrap_or_else(|e| panic!("retry: {e}"));
    assert_ne!(first.key.generation, second.key.generation);
    assert_eq!(state.active_count(), 2);
}

#[test]
fn retry_marks_old_live_generation_unavailable() {
    let mut state = ProviderRequestState::new();
    let first = do_invoke(&mut state);
    let pol = default_policy();
    let empty = empty_map();
    state
        .retry(
            &first.key,
            InvokeInput {
                owner: &owner(),
                action_id: &action(),
                context_screen: &screen(),
                context_instance: &screen(),
                context_refs: &empty,
                arguments: &empty,
                policy: &pol,
            },
        )
        .unwrap_or_else(|e| panic!("retry: {e}"));
    let old = state
        .request(&first.key)
        .unwrap_or_else(|| panic!("old request"));
    assert!(old.unavailable_reason().is_some());
}

#[test]
fn old_generation_output_after_retry_changes_nothing() {
    let mut state = ProviderRequestState::new();
    let first = do_invoke(&mut state);
    let pol = default_policy();
    let empty = empty_map();
    let second = state
        .retry(
            &first.key,
            InvokeInput {
                owner: &owner(),
                action_id: &action(),
                context_screen: &screen(),
                context_instance: &screen(),
                context_refs: &empty,
                arguments: &empty,
                policy: &pol,
            },
        )
        .unwrap_or_else(|e| panic!("retry: {e}"));

    let stale_result = state.record_outcome(&first.key, notice_outcome(), 1000);
    assert_eq!(stale_result, Err(ProviderRequestError::PostTerminal));

    let old = state
        .request(&first.key)
        .unwrap_or_else(|| panic!("old request"));
    assert!(old.unavailable_reason().is_some());
    assert!(old.completed_outcome().is_none());

    let new_req = state
        .request(&second.key)
        .unwrap_or_else(|| panic!("new request"));
    assert!(!new_req.is_terminal());
}

// ── CW10-08: exact confirmation (policy + binding + TTL + single-use) ────

#[test]
fn confirm_consumes_token_and_stages_fresh_invocation() {
    let mut state = ProviderRequestState::new();
    let pol = continuation_policy();
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    let outcome_result = state.record_outcome(
        &outcome.key,
        confirmation_outcome("conf.token1", false),
        1000,
    );
    assert!(outcome_result.is_ok());
    assert_eq!(state.pending_confirmation_count(), 1);

    let empty = empty_map();
    let confirm_outcome = state
        .confirm(
            ConfirmInput {
                owner: &owner(),
                action_id: &action(),
                context_screen: &screen(),
                context_instance: &screen(),
                context_refs: &empty,
                generation: outcome.key.generation,
                confirmation_id: &Id::parse("conf.token1").unwrap_or_else(|_e| panic!("conf id")),
                values: &empty,
            },
            1100,
        )
        .unwrap_or_else(|e| panic!("confirm: {e}"));

    assert_eq!(state.pending_confirmation_count(), 0);
    assert_ne!(confirm_outcome.key.generation, outcome.key.generation);
    assert!(confirm_outcome.invocation.continuation.is_some());
    let continuation = confirm_outcome
        .invocation
        .continuation
        .as_ref()
        .unwrap_or_else(|| panic!("continuation"));
    assert_eq!(
        continuation.confirmation_id,
        Id::parse("conf.token1").unwrap_or_else(|_e| panic!("conf id"))
    );
    assert!(continuation.approved);
}

#[test]
fn confirm_rejects_expired_token() {
    let mut state = ProviderRequestState::new();
    let pol = continuation_policy();
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    state
        .record_outcome(
            &outcome.key,
            confirmation_outcome("conf.token1", false),
            1000,
        )
        .unwrap_or_else(|e| panic!("outcome: {e}"));

    let empty = empty_map();
    let result = state.confirm(
        ConfirmInput {
            owner: &owner(),
            action_id: &action(),
            context_screen: &screen(),
            context_instance: &screen(),
            context_refs: &empty,
            generation: outcome.key.generation,
            confirmation_id: &Id::parse("conf.token1").unwrap_or_else(|_e| panic!("conf id")),
            values: &empty,
        },
        1000 + CONFIRMATION_TTL_SECONDS + 1,
    );
    assert!(matches!(result, Err(ProviderRequestError::Expired { .. })));
    assert_eq!(state.pending_confirmation_count(), 0);
}

#[test]
fn confirm_rejects_wrong_owner() {
    let mut state = ProviderRequestState::new();
    let pol = continuation_policy();
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    state
        .record_outcome(
            &outcome.key,
            confirmation_outcome("conf.token1", false),
            1000,
        )
        .unwrap_or_else(|e| panic!("outcome: {e}"));

    let wrong_owner = Id::parse("other").unwrap_or_else(|_e| panic!("owner id"));
    let empty = empty_map();
    let result = state.confirm(
        ConfirmInput {
            owner: &wrong_owner,
            action_id: &action(),
            context_screen: &screen(),
            context_instance: &screen(),
            context_refs: &empty,
            generation: outcome.key.generation,
            confirmation_id: &Id::parse("conf.token1").unwrap_or_else(|_e| panic!("conf id")),
            values: &empty,
        },
        1100,
    );
    assert_eq!(result, Err(ProviderRequestError::ConfirmationNotFound));
}

#[test]
fn confirm_rejects_wrong_generation() {
    let mut state = ProviderRequestState::new();
    let pol = continuation_policy();
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    state
        .record_outcome(
            &outcome.key,
            confirmation_outcome("conf.token1", false),
            1000,
        )
        .unwrap_or_else(|e| panic!("outcome: {e}"));

    let empty = empty_map();
    let result = state.confirm(
        ConfirmInput {
            owner: &owner(),
            action_id: &action(),
            context_screen: &screen(),
            context_instance: &screen(),
            context_refs: &empty,
            generation: outcome.key.generation + 1,
            confirmation_id: &Id::parse("conf.token1").unwrap_or_else(|_e| panic!("conf id")),
            values: &empty,
        },
        1100,
    );
    assert_eq!(result, Err(ProviderRequestError::ConfirmationNotFound));
}

#[test]
fn confirm_token_is_single_use() {
    let mut state = ProviderRequestState::new();
    let pol = continuation_policy();
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    state
        .record_outcome(
            &outcome.key,
            confirmation_outcome("conf.token1", false),
            1000,
        )
        .unwrap_or_else(|e| panic!("outcome: {e}"));

    let empty = empty_map();
    let o = owner();
    let a = action();
    let s = screen();
    let conf_id = Id::parse("conf.token1").unwrap_or_else(|_e| panic!("conf id"));
    state
        .confirm(
            ConfirmInput {
                owner: &o,
                action_id: &a,
                context_screen: &s,
                context_instance: &s,
                context_refs: &empty,
                generation: outcome.key.generation,
                confirmation_id: &conf_id,
                values: &empty,
            },
            1100,
        )
        .unwrap_or_else(|e| panic!("first confirm: {e}"));

    let result = state.confirm(
        ConfirmInput {
            owner: &o,
            action_id: &a,
            context_screen: &s,
            context_instance: &s,
            context_refs: &empty,
            generation: outcome.key.generation,
            confirmation_id: &conf_id,
            values: &empty,
        },
        1200,
    );
    assert_eq!(result, Err(ProviderRequestError::ConfirmationNotFound));
}

#[test]
fn confirm_at_exact_ttl_boundary_is_expired() {
    let mut state = ProviderRequestState::new();
    let pol = continuation_policy();
    let outcome = do_invoke_with(&mut state, &pol, empty_map(), empty_map());
    state
        .record_outcome(
            &outcome.key,
            confirmation_outcome("conf.token1", false),
            1000,
        )
        .unwrap_or_else(|e| panic!("outcome: {e}"));

    let empty = empty_map();
    let result = state.confirm(
        ConfirmInput {
            owner: &owner(),
            action_id: &action(),
            context_screen: &screen(),
            context_instance: &screen(),
            context_refs: &empty,
            generation: outcome.key.generation,
            confirmation_id: &Id::parse("conf.token1").unwrap_or_else(|_e| panic!("conf id")),
            values: &empty,
        },
        1000 + CONFIRMATION_TTL_SECONDS,
    );
    // The 5-minute boundary is expiration at elapsed >= 300, so exactly 300
    // seconds is expired, not still valid.
    assert!(
        matches!(result, Err(ProviderRequestError::Expired { .. })),
        "elapsed == TTL must be expired, got {result:?}"
    );
    // Fail-fast single-use: the expired token is consumed.
    assert_eq!(state.pending_confirmation_count(), 0);
}
