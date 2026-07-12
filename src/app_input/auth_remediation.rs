//! Dispatch wiring for the in-app device-code auth remediation dialog
//! (issue #244).
//!
//! When a GitHub operation fails with `NotAuthenticated`, the dispatch layer
//! opens the auth modal (`OpenAuthDialog`) and spawns the non-interactive
//! device-code flow off the UI thread. The parsed one-time code + URL and the
//! final exit status are delivered back as `AuthCodeReceived` /
//! `AuthSucceeded` / `AuthFailed` events.
//!
//! The state layer owns the dialog state machine; the runtime layer
//! (`runtime::gh_auth`) owns the `gh auth login --web` subprocess; this module
//! is the seam that connects them.

use jefe::github::{is_not_authenticated_error, redact_device_codes};
use jefe::runtime::run_device_auth;
use jefe::state::AppEvent;

use super::{AppStateHandle, SharedContext, apply_and_persist, gh_async};

/// Open the auth remediation dialog when no other modal is active, then start
/// the device-code flow.
///
/// Returns `true` when the dialog was opened (the caller should NOT also surface
/// a bare error string in that case — the dialog is the remediation surface).
/// Returns `false` when a modal is already open (the caller surfaces the error
/// normally) — this keeps the auth dialog from clobbering an in-flight form.
pub(super) fn offer_auth_remediation(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    error: &str,
) -> bool {
    if !should_offer_auth_remediation(error, app_state) {
        return false;
    }
    apply_and_persist(app_state, ctx, AppEvent::OpenAuthDialog);
    spawn_device_auth_flow(app_state, ctx);
    true
}

/// Pure decision: should we offer the auth dialog for this error, given the
/// current modal state? Returns `true` when the error indicates gh is
/// unauthenticated AND no modal is currently open.
///
/// Split out so the decision is unit-testable without spawning `gh` or
/// constructing an iocraft `HookState`.
pub(super) fn should_offer_auth_remediation(error: &str, app_state: &AppStateHandle) -> bool {
    is_auth_remediation_candidate(error, &app_state.read().modal)
}

/// Pure predicate: the error indicates gh is unauthenticated and no modal is
/// open, so the auth remediation dialog should be offered.
#[must_use]
pub(super) fn is_auth_remediation_candidate(error: &str, modal: &jefe::state::ModalState) -> bool {
    is_not_authenticated_error(error) && *modal == jefe::state::ModalState::None
}

/// Spawn the non-interactive device-code flow off the UI thread, delivering
/// `AuthCodeReceived` / `AuthSucceeded` / `AuthFailed` events back to state.
///
/// The flow blocks until `gh auth login --web` exits (success → exit 0; the
/// user authorizes in a browser; failure → non-zero). Because it runs via
/// `spawn_gh_task_with_panic` → `smol::unblock`, the UI is never blocked.
pub(super) fn spawn_device_auth_flow(app_state: &AppStateHandle, ctx: &SharedContext) {
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        |mut app_state, ctx| {
            let result = run_device_auth();
            deliver_auth_result(&mut app_state, &ctx, result);
        },
        |mut app_state, ctx, message| {
            // A panic in the auth flow is a transient failure — surface it and
            // let the user retry, rather than leaving the dialog stuck.
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::AuthFailed {
                    error: format!("GitHub auth task panicked: {message}"),
                },
            );
        },
    );
}

/// Translate the `run_device_auth` outcome into the appropriate auth event and
/// apply it. Called on the background thread.
fn deliver_auth_result(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    result: Result<jefe::runtime::AuthRunResult, jefe::github::GhError>,
) {
    match result {
        Ok(run) => {
            // First, surface the code + URL as soon as they're available so the
            // dialog shows them while the user authorizes in a browser.
            if let Some(device_code) = run.code {
                apply_and_persist(
                    app_state,
                    ctx,
                    AppEvent::AuthCodeReceived {
                        code: device_code.code,
                        url: device_code.verification_url,
                    },
                );
            }
            // Then the terminal exit status decides success vs retryable
            // failure. A non-zero exit after a code was shown means the code
            // expired or the user denied — surface it so the dialog offers a
            // retry instead of sticking in Confirming (issue #244).
            if run.exit_success {
                apply_and_persist(app_state, ctx, AppEvent::AuthSucceeded);
            } else {
                apply_and_persist(
                    app_state,
                    ctx,
                    AppEvent::AuthFailed {
                        error: failure_message(&run.stderr),
                    },
                );
            }
        }
        Err(error) => {
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::AuthFailed {
                    error: redact_device_codes(&error.to_string()),
                },
            );
        }
    }
}

/// Build a human-readable failure message from captured stderr.
///
/// Scrubs the GitHub device-code shape so a code that `gh` echoed back on a
/// failed/expired flow cannot leak into state/logs (issue #244 OCR review).
fn failure_message(stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        "GitHub authentication did not complete.".to_string()
    } else {
        redact_device_codes(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jefe::state::ModalState;

    #[test]
    fn failure_message_uses_stderr_when_present() {
        assert_eq!(
            failure_message("  the device code expired  "),
            "the device code expired"
        );
    }

    #[test]
    fn failure_message_falls_back_when_stderr_empty() {
        assert_eq!(
            failure_message("   "),
            "GitHub authentication did not complete."
        );
    }

    #[test]
    fn failure_message_redacts_device_code_from_stderr() {
        // gh may echo the one-time code back on a failed/expired flow; it must
        // not survive into state (issue #244 OCR review).
        assert_eq!(
            failure_message("error: code WDJB-MJHT has expired"),
            "error: code <redacted> has expired"
        );
    }

    #[test]
    fn is_auth_remediation_candidate_true_for_unauth_when_no_modal() {
        assert!(is_auth_remediation_candidate(
            "gh is not authenticated. Run: gh auth login",
            &ModalState::None
        ));
    }

    #[test]
    fn is_auth_remediation_candidate_false_for_unrelated_error() {
        assert!(!is_auth_remediation_candidate(
            "network error: could not resolve host",
            &ModalState::None
        ));
    }

    #[test]
    fn is_auth_remediation_candidate_false_when_a_modal_is_already_open() {
        // Never clobber an existing modal (e.g. a form the user is editing).
        assert!(!is_auth_remediation_candidate(
            "gh is not authenticated. Run: gh auth login",
            &ModalState::Help
        ));
    }
}
