//! `AppEvent` ↔ `PullRequestsMessage` conversion — TOTAL STUB.
//!
//! @plan PLAN-20260624-PR-MODE.P03
//! @requirement REQ-PR-002
//! @pseudocode component-004 lines 45-85
//!
//! P03 stub: `from_app_event` returns `None`/a fixed non-matching variant,
//! and `From<PullRequestsMessage> for AppEvent` returns a fixed deterministic
//! `AppEvent` that will NOT round-trip. The P04 round-trip RED tests fail by
//! assertion, not by panic. P05 implements the real bidirectional mapping.

use crate::state::AppEvent;

use super::PullRequestsMessage;

impl From<PullRequestsMessage> for AppEvent {
    fn from(message: PullRequestsMessage) -> Self {
        message.into_app_event()
    }
}

impl PullRequestsMessage {
    /// Convert a PR-domain [`AppEvent`] into the typed message — TOTAL STUB.
    ///
    /// @pseudocode component-004 lines 51-67
    /// P03: returns `None`-equivalent (a fixed non-matching variant) so the
    /// P04 round-trip tests fail by assertion. The caller guards with
    /// `is_prs_event` so a non-PR event never reaches this stub.
    pub(super) fn from_app_event(event: AppEvent) -> Self {
        // Deterministic wrong value: always EnterMode regardless of input.
        let _ = event;
        Self::EnterMode
    }

    /// Convert this PR-domain message back into the historical [`AppEvent`] —
    /// TOTAL STUB.
    ///
    /// @pseudocode component-004 lines 68-85
    /// P03: returns a fixed deterministic `AppEvent` that will NOT round-trip.
    fn into_app_event(self) -> AppEvent {
        // Deterministic wrong value: always EnterPrsMode regardless of input.
        let _ = self;
        AppEvent::EnterPrsMode
    }
}
