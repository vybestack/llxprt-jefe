//! In-app device-code auth dialog state-machine transitions (issue #244).
//!
//! These methods are the deterministic reducer for `ModalState::Auth` /
//! `AuthDialogState`. They perform NO I/O. The runtime layer owns the
//! `gh auth login --web` subprocess; the dispatch layer delivers the events.
//!
//! State machine:
//!
//! ```text
//! Idle --OpenAuthDialog--> AwaitingCode
//! AwaitingCode --AuthCodeReceived--> Confirming{code,url}
//! AwaitingCode --AuthFailed--> Failed{error,can_retry=true}
//! Confirming --AuthSucceeded--> (modal closed)
//! Confirming --AuthFailed--> Failed{error,can_retry=true}
//! Failed --AuthRetry--> AwaitingCode
//! * (auth) --AuthCancelled--> (modal closed, actionable error_message set)
//! ```
//!
//! Stray events that arrive when no auth modal is open are ignored (the modal
//! may have been closed by success/cancel while a late subprocess event was
//! in flight).

use crate::messages::SystemMessage;

use super::AppState;
use super::types::{AuthDialogPhase, AuthDialogState, ModalState};

/// The actionable message shown when the user cancels the auth dialog. It
/// still tells them how to authenticate manually, so cancellation is never a
/// dead end.
const AUTH_CANCELLED_MESSAGE: &str =
    "GitHub authentication cancelled. Run `gh auth login` in a terminal to authenticate manually.";

impl AppState {
    /// Apply a [`SystemMessage`] auth variant to the auth dialog state machine.
    pub(super) fn apply_auth_message(&mut self, message: SystemMessage) {
        match message {
            SystemMessage::OpenAuthDialog => self.open_auth_dialog(),
            SystemMessage::AuthCodeReceived { code, url } => self.auth_code_received(code, url),
            SystemMessage::AuthSucceeded => self.auth_succeeded(),
            SystemMessage::AuthFailed { error } => self.auth_failed(error),
            SystemMessage::AuthCancelled => self.auth_cancelled(),
            SystemMessage::AuthRetry => self.auth_retry(),
            // Non-auth system messages are handled by the caller.
            SystemMessage::Quit | SystemMessage::ClearError | SystemMessage::ClearWarning => {}
        }
    }

    fn open_auth_dialog(&mut self) {
        // Defense-in-depth: never replace an existing modal from underneath
        // the user. The dispatch layer only opens auth when no other modal is
        // active, but the reducer guards independently.
        if !matches!(self.modal, ModalState::None) {
            return;
        }
        self.modal = ModalState::Auth {
            state: AuthDialogState::awaiting_code(),
        };
    }

    fn auth_code_received(&mut self, code: String, url: String) {
        if let ModalState::Auth { state } = &mut self.modal {
            state.phase = AuthDialogPhase::Confirming { code, url };
        }
    }

    fn auth_succeeded(&mut self) {
        if matches!(self.modal, ModalState::Auth { .. }) {
            self.modal = ModalState::None;
        }
    }

    fn auth_failed(&mut self, error: String) {
        if let ModalState::Auth { state } = &mut self.modal {
            state.phase = AuthDialogPhase::Failed {
                error,
                can_retry: true,
            };
        }
    }

    fn auth_retry(&mut self) {
        if let ModalState::Auth { state } = &mut self.modal {
            state.phase = AuthDialogPhase::AwaitingCode;
            // Clear any stale top-level error so the retried flow starts clean
            // (issue #244 OCR review).
            self.error_message = None;
        }
    }

    fn auth_cancelled(&mut self) {
        if matches!(self.modal, ModalState::Auth { .. }) {
            self.modal = ModalState::None;
            self.error_message = Some(AUTH_CANCELLED_MESSAGE.to_string());
        }
    }
}
