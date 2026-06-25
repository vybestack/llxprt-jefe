//! Pull Requests mode state operations — TOTAL STUB.
//!
//! @plan PLAN-20260624-PR-MODE.P03
//! @requirement REQ-PR-001
//! @requirement REQ-PR-003
//! @requirement REQ-PR-006
//! @requirement REQ-PR-008
//!
//! P03 stub: every function is a total no-op that mutates NO observable state.
//! The hub `apply_prs_message` returns `true` from a TOTAL no-op `match` over
//! every `PullRequestsMessage` variant so the `apply_message` arm compiles and
//! is panic-free. Behavioral reducer tests (P04) fail by ASSERTION (observed
//! state unchanged), not by panic. P05 replaces these stubs with real logic.

use super::AppState;
use crate::messages::PullRequestsMessage;

impl AppState {
    /// Clear loaded PR data after a repo change (TOTAL STUB — no-op in P03).
    ///
    /// @pseudocode component-001 lines 88-98
    /// P03: signature only; NOT called from `select_repository_by_index` yet
    /// (that wiring is P05). The body mutates NOTHING per the TOTAL-STUB rule.
    pub(super) fn reset_prs_for_repo_change(&mut self) {
        // P03 TOTAL-STUB: mutates NOTHING. The reborrow keeps the `&mut self`
        // contract (required for the P05 GREEN wiring) without altering state.
        let _ = &mut *self;
    }

    /// Handle all PR-mode messages (TOTAL STUB).
    ///
    /// @pseudocode component-004 lines 86-94
    /// P03: no-op match over every variant; returns `true` so the
    /// `apply_message` arm needs no `debug_assert!` and cannot debug-panic.
    /// P05 implements the real reducer dispatch.
    pub(super) fn apply_prs_message(&mut self, message: PullRequestsMessage) -> bool {
        match message {
            // P03 TOTAL-STUB: every arm is an inert no-op that mutates NO state.
            // ExitMode calls the (no-op) reset helper so the helper stays
            // reachable; the helper mutates nothing (P05 implements real reset).
            // All other arms fall through empty. P05 implements real dispatch.
            PullRequestsMessage::ExitMode => self.reset_prs_for_repo_change(),
            PullRequestsMessage::EnterMode
            | PullRequestsMessage::RefocusList
            | PullRequestsMessage::Navigate(_)
            | PullRequestsMessage::Enter
            | PullRequestsMessage::CycleFocus
            | PullRequestsMessage::CycleFocusReverse
            | PullRequestsMessage::ScrollDetail(_)
            | PullRequestsMessage::DetailSubfocusNext
            | PullRequestsMessage::DetailSubfocusPrev
            | PullRequestsMessage::ListLoaded { .. }
            | PullRequestsMessage::ListLoadFailed { .. }
            | PullRequestsMessage::ListPageLoaded { .. }
            | PullRequestsMessage::DetailLoaded { .. }
            | PullRequestsMessage::DetailLoadFailed { .. }
            | PullRequestsMessage::CommentsPageLoaded { .. }
            | PullRequestsMessage::CommentsPageFailed { .. }
            | PullRequestsMessage::OpenFilterControls
            | PullRequestsMessage::CloseFilterControls
            | PullRequestsMessage::ApplyFilter
            | PullRequestsMessage::ClearFilter
            | PullRequestsMessage::FilterNavigate(_)
            | PullRequestsMessage::CycleFilterState
            | PullRequestsMessage::CycleDraftFilter
            | PullRequestsMessage::CycleReviewFilter
            | PullRequestsMessage::CycleChecksFilter
            | PullRequestsMessage::UpdateDraftFilter { .. }
            | PullRequestsMessage::FocusSearchInput
            | PullRequestsMessage::BlurSearchInput
            | PullRequestsMessage::SetSearchQuery { .. }
            | PullRequestsMessage::ApplySearch
            | PullRequestsMessage::ClearSearch
            | PullRequestsMessage::OpenNewCommentComposer
            | PullRequestsMessage::OpenReplyComposer { .. }
            | PullRequestsMessage::Inline(_)
            | PullRequestsMessage::CommentCreated { .. }
            | PullRequestsMessage::CommentCreateFailed { .. }
            | PullRequestsMessage::MutationFailed { .. }
            | PullRequestsMessage::ShowNotice(_)
            | PullRequestsMessage::OpenAgentChooser
            | PullRequestsMessage::AgentChooserNavigate(_)
            | PullRequestsMessage::AgentChooserConfirm
            | PullRequestsMessage::AgentChooserCancel
            | PullRequestsMessage::SendToAgentCompleted
            | PullRequestsMessage::SendToAgentFailed { .. }
            | PullRequestsMessage::OpenInBrowser
            | PullRequestsMessage::OpenedInBrowser { .. }
            | PullRequestsMessage::OpenInBrowserFailed { .. } => {
                // Intentional no-op stub — P05 implements real handling.
            }
        }
        true
    }
}
