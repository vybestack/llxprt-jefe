//! Issue #403 Bug 3 tests: verify launch-failure errors are captured into
//! the Errors ring buffer via `capture_runtime_errors`.

use super::*;
use crate::state::AppState;

fn state_with_error(msg: &str) -> AppState {
    AppState {
        error_message: Some(msg.to_owned()),
        ..AppState::default()
    }
}

#[test]
fn capture_runtime_errors_pushes_global_error_to_ring_buffer() {
    let mut state = state_with_error("launch failed: bad version");

    capture_runtime_errors(&mut state);

    let last = state
        .errors_state
        .last_error()
        .unwrap_or_else(|| panic!("ring buffer should have captured the error"));
    assert_eq!(
        last.title, "launch failed: bad version",
        "the error title should match the error_message text"
    );
}

#[test]
fn capture_runtime_errors_dedups_unchanged_global_error() {
    let mut state = state_with_error("launch failed: bad version");

    capture_runtime_errors(&mut state);
    let count_after_first = state.errors_state.errors.len();

    // Capture again with the same message — should not add a duplicate.
    capture_runtime_errors(&mut state);
    assert_eq!(
        state.errors_state.errors.len(),
        count_after_first,
        "unchanged error_message should not produce a duplicate ring entry"
    );
}

#[test]
fn capture_runtime_errors_captures_new_error_after_change() {
    let mut state = state_with_error("first error");
    capture_runtime_errors(&mut state);

    state.error_message = Some("second error".to_owned());
    capture_runtime_errors(&mut state);

    assert_eq!(
        state.errors_state.errors.len(),
        2,
        "two distinct errors should produce two ring entries"
    );
    let last = state
        .errors_state
        .last_error()
        .unwrap_or_else(|| panic!("should have last error"));
    assert_eq!(last.title, "second error");
}

#[test]
fn capture_runtime_errors_clears_tracker_when_error_resolved() {
    let mut state = state_with_error("transient error");
    capture_runtime_errors(&mut state);

    state.error_message = None;
    capture_runtime_errors(&mut state);

    // The tracker should be reset so a future error with the same text is
    // captured again rather than deduped.
    state.error_message = Some("transient error".to_owned());
    capture_runtime_errors(&mut state);
    assert_eq!(
        state.errors_state.errors.len(),
        2,
        "re-occurring error after clear should be captured again"
    );
}
