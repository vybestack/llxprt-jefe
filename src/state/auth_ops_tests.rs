//! State-machine tests for the in-app device-code auth dialog (issue #244).
//!
//! These exercise the deterministic reducer transitions on
//! `ModalState::Auth` / `AuthDialogState` only — no I/O, no runtime.

use super::AppState;
use super::events::AppEvent;
use super::types::{AuthDialogPhase, AuthDialogState, ModalState};

#[test]
fn open_auth_dialog_from_no_modal_sets_awaiting_code() {
    let state = AppState::default();
    assert!(matches!(state.modal, ModalState::None));

    let next = state.apply(AppEvent::OpenAuthDialog);
    match &next.modal {
        ModalState::Auth { state } => assert!(
            matches!(state.phase, AuthDialogPhase::AwaitingCode),
            "must be AwaitingCode after OpenAuthDialog"
        ),
        other => panic!("expected Auth modal, got {other:?}"),
    }
}

#[test]
fn auth_code_received_transitions_to_confirming_with_code_and_url() {
    let state = AppState::default().apply(AppEvent::OpenAuthDialog);
    let next = state.apply(AppEvent::AuthCodeReceived {
        code: "7701-C5F6".to_string(),
        url: "https://github.com/login/device".to_string(),
    });
    match &next.modal {
        ModalState::Auth { state } => match &state.phase {
            AuthDialogPhase::Confirming { code, url } => {
                assert_eq!(code, "7701-C5F6");
                assert_eq!(url, "https://github.com/login/device");
            }
            other => panic!("expected Confirming, got {other:?}"),
        },
        other => panic!("expected Auth modal, got {other:?}"),
    }
}

#[test]
fn auth_failed_from_awaiting_transitions_to_failed_with_retry() {
    let state = AppState::default().apply(AppEvent::OpenAuthDialog);
    let next = state.apply(AppEvent::AuthFailed {
        error: "the device code expired".to_string(),
    });
    match &next.modal {
        ModalState::Auth { state } => match &state.phase {
            AuthDialogPhase::Failed { error, can_retry } => {
                assert_eq!(error, "the device code expired");
                assert!(*can_retry, "transient failures must offer retry");
            }
            other => panic!("expected Failed, got {other:?}"),
        },
        other => panic!("expected Auth modal, got {other:?}"),
    }
}

#[test]
fn auth_retry_from_failed_returns_to_awaiting_code() {
    let state = AppState::default()
        .apply(AppEvent::OpenAuthDialog)
        .apply(AppEvent::AuthFailed {
            error: "network".to_string(),
        });
    let next = state.apply(AppEvent::AuthRetry);
    match &next.modal {
        ModalState::Auth { state } => assert!(
            matches!(state.phase, AuthDialogPhase::AwaitingCode),
            "retry must return to AwaitingCode"
        ),
        other => panic!("expected Auth modal, got {other:?}"),
    }
}

#[test]
fn auth_succeeded_from_confirming_closes_modal() {
    let state =
        AppState::default()
            .apply(AppEvent::OpenAuthDialog)
            .apply(AppEvent::AuthCodeReceived {
                code: "1234-5678".to_string(),
                url: "https://github.com/login/device".to_string(),
            });
    let next = state.apply(AppEvent::AuthSucceeded);
    assert!(
        matches!(next.modal, ModalState::None),
        "success must close the modal"
    );
}

#[test]
fn auth_failed_from_confirming_transitions_to_failed_with_retry() {
    // The code was shown (Confirming) but then expired or was denied while gh
    // was still running (gh exits non-zero). The dialog must NOT stick in
    // Confirming — it must surface a retryable failure (issue #244 review #1).
    let state =
        AppState::default()
            .apply(AppEvent::OpenAuthDialog)
            .apply(AppEvent::AuthCodeReceived {
                code: "1234-5678".to_string(),
                url: "https://github.com/login/device".to_string(),
            });
    let next = state.apply(AppEvent::AuthFailed {
        error: "the device code has expired".to_string(),
    });
    match &next.modal {
        ModalState::Auth { state } => match &state.phase {
            AuthDialogPhase::Failed { error, can_retry } => {
                assert_eq!(error, "the device code has expired");
                assert!(*can_retry, "expiry after display must offer retry");
            }
            other => panic!("expected Failed, got {other:?}"),
        },
        other => panic!("expected Auth modal, got {other:?}"),
    }
}

#[test]
fn auth_cancelled_closes_modal_and_sets_actionable_message() {
    let state = AppState::default().apply(AppEvent::OpenAuthDialog);
    let next = state.apply(AppEvent::AuthCancelled);
    assert!(
        matches!(next.modal, ModalState::None),
        "cancel must close the modal"
    );
    let msg = next
        .error_message
        .as_deref()
        .unwrap_or_else(|| panic!("cancel must set an actionable error_message"));
    assert!(
        msg.to_lowercase().contains("cancel"),
        "error_message must mention cancellation, got: {msg}"
    );
    assert!(
        msg.to_lowercase().contains("gh auth login"),
        "error_message must still tell the user how to authenticate manually, got: {msg}"
    );
}

#[test]
fn auth_failed_then_succeeded_closes_modal() {
    // Expired code → retry → new code → success.
    let state = AppState::default()
        .apply(AppEvent::OpenAuthDialog)
        .apply(AppEvent::AuthFailed {
            error: "expired".to_string(),
        })
        .apply(AppEvent::AuthRetry)
        .apply(AppEvent::AuthCodeReceived {
            code: "ABCD-EFGH".to_string(),
            url: "https://github.com/login/device".to_string(),
        });
    let next = state.apply(AppEvent::AuthSucceeded);
    assert!(matches!(next.modal, ModalState::None));
}

#[test]
fn open_auth_dialog_does_not_clobber_existing_form_modal() {
    // A form modal (e.g. NewRepository) must not be replaced by the auth
    // dialog from underneath the user. The dispatch layer is responsible for
    // only opening auth when no other modal is active; the reducer defends by
    // ignoring OpenAuthDialog when a non-None modal is already open.
    let state = AppState {
        modal: ModalState::Help,
        ..AppState::default()
    };
    let next = state.apply(AppEvent::OpenAuthDialog);
    assert!(
        matches!(next.modal, ModalState::Help),
        "OpenAuthDialog must not replace an existing modal"
    );
}

#[test]
fn auth_code_received_ignored_when_no_auth_modal() {
    // Stray late-arriving code events must not corrupt state when the modal
    // has already been closed.
    let state = AppState::default();
    let next = state.apply(AppEvent::AuthCodeReceived {
        code: "0000-0000".to_string(),
        url: "https://github.com/login/device".to_string(),
    });
    assert!(matches!(next.modal, ModalState::None));
}

#[test]
fn auth_succeeded_ignored_when_no_auth_modal() {
    let next = AppState::default().apply(AppEvent::AuthSucceeded);
    assert!(matches!(next.modal, ModalState::None));
}

#[test]
fn auth_dialog_state_default_is_idle() {
    let state = AuthDialogState::default();
    assert!(matches!(state.phase, AuthDialogPhase::Idle));
}

#[test]
fn auth_dialog_phase_debug_redacts_device_code() {
    // The one-time code is a short-lived bearer credential; it must never
    // leak through Debug output (logs / crash reports / snapshots). The
    // verification URL is not secret and may appear (issue #244 OCR review).
    let phase = AuthDialogPhase::Confirming {
        code: "7701-C5F6".to_string(),
        url: "https://github.com/login/device".to_string(),
    };
    let debug = format!("{phase:?}");
    assert!(
        !debug.contains("7701-C5F6"),
        "Debug must not leak the device code, got: {debug}"
    );
    assert!(
        debug.contains("<redacted>"),
        "Debug should mark the redacted field, got: {debug}"
    );
    assert!(
        debug.contains("github.com/login/device"),
        "Debug should keep the non-secret URL, got: {debug}"
    );
}

#[test]
fn auth_dialog_phase_debug_redacts_code_in_failed_error() {
    // Defense-in-depth: even if a code-shaped string reaches the Failed error
    // text, the Debug impl must scrub it (issue #244 OCR review).
    let phase = AuthDialogPhase::Failed {
        error: "code 7701-C5F6 expired".to_string(),
        can_retry: true,
    };
    let debug = format!("{phase:?}");
    assert!(
        !debug.contains("7701-C5F6"),
        "Failed Debug must not leak a code-shaped substring, got: {debug}"
    );
    assert!(
        debug.contains("<redacted>"),
        "Failed Debug should mark the redacted code, got: {debug}"
    );
}
