//! Post-terminal bytes, progress fault, generation exhaustion, and
//! no-duplicate-queue RED tests.

use super::super::{ProviderRequestError, ProviderRequestState, next_generation};
use super::support::{do_invoke, notice_outcome, progress};

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
        Some(super::super::UnavailableReason::Protocol)
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
