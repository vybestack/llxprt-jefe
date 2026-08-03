//! Which unwindable layers a screen currently has open (issue #386, CW06-04).
//!
//! [`super::navigation_unwind::resolve_back`] states the precedence; this states
//! what is actually open. Keeping the two apart means the order is a property of
//! the contract rather than of whichever mode happened to be asked, and it is
//! why every screen answers Back the same way without every screen containing
//! the answer.
//!
//! The projection is pure and reads only committed state.

use super::navigation_unwind::{BackLayer, BackResolution, resolve_back};
use super::types::{AppState, InlineState, ModalState};
use crate::workbench::ScreenId;

impl AppState {
    /// The layers Back could unwind, in no particular order.
    ///
    /// Order is deliberately not expressed here — [`BackLayer::PRECEDENCE`]
    /// owns it — so this can be read as a list of facts rather than as a chain
    /// whose sequence is load-bearing.
    #[must_use]
    pub fn open_back_layers(&self) -> Vec<BackLayer> {
        let mut open = Vec::new();
        if self.host_confirmation_open() {
            open.push(BackLayer::HostConfirmation);
        }
        if self.nav.guard().is_some() {
            open.push(BackLayer::DirtyGuard);
        }
        if self.chooser_open() {
            open.push(BackLayer::Chooser);
        }
        if self.editor_open() {
            open.push(BackLayer::Editor);
        }
        if self.search_focused() {
            open.push(BackLayer::Search);
        }
        if self.filter_open() {
            open.push(BackLayer::Filter);
        }
        if self.plain_overlay_open() {
            open.push(BackLayer::Overlay);
        }
        if self.panel_transient_open() {
            open.push(BackLayer::PanelTransient);
        }
        open
    }

    /// What one Back key press means right now.
    #[must_use]
    pub fn back_resolution(&self) -> BackResolution {
        resolve_back(&self.open_back_layers(), self.can_leave_screen())
    }

    /// Whether leaving this screen would go anywhere.
    ///
    /// The home screen with nothing stacked beneath it is the one place Back
    /// has nothing left to do.
    #[must_use]
    pub fn can_leave_screen(&self) -> bool {
        self.nav.depth() > 0 || self.screen() != ScreenId::default()
    }

    /// A modal the host owns that must be answered before anything else.
    fn host_confirmation_open(&self) -> bool {
        matches!(
            self.modal,
            ModalState::ConfirmDeleteRepository { .. }
                | ModalState::ConfirmDeleteAgent { .. }
                | ModalState::ConfirmKillAgent { .. }
                | ModalState::ConfirmServerLostRecovery { .. }
                | ModalState::PreflightPrompt { .. }
                | ModalState::ConfirmIssueDirtyCopy { .. }
                | ModalState::ConfirmIssueOriginMismatch { .. }
        )
    }

    /// A chooser or property editor is taking the keys.
    ///
    /// Only the current screen's own state is consulted. A mode the user left
    /// may still be holding a chooser or a half-typed filter, and that must not
    /// change what Back does on the screen they are actually looking at.
    fn chooser_open(&self) -> bool {
        match self.screen() {
            ScreenId::Issues => {
                self.issues_state.agent_chooser.is_some()
                    || self.issues_state.property_editor.is_some()
                    || self.issues_state.close_reason_chooser.is_some()
            }
            ScreenId::PullRequests => {
                self.prs_state.agent_chooser.is_some()
                    || self.prs_state.property_editor.is_some()
                    || self.prs_state.merge_chooser.is_some()
            }
            _ => false,
        }
    }

    /// Text is being composed or edited in place.
    fn editor_open(&self) -> bool {
        match self.screen() {
            ScreenId::Issues => {
                self.issues_state.inline_state != InlineState::None
                    || self.issues_state.new_issue_form.is_some()
            }
            ScreenId::PullRequests => self.prs_state.inline_state != InlineState::None,
            _ => false,
        }
    }

    /// A search input holds the keys.
    fn search_focused(&self) -> bool {
        match self.screen() {
            ScreenId::Issues => self.issues_state.search_input_focused,
            ScreenId::PullRequests => self.prs_state.search_input_focused,
            ScreenId::Actions => self.actions_state.ui.search_input_focused,
            ScreenId::Dashboard | ScreenId::Repositories => self.dashboard_search.input_focused,
            _ => false,
        }
    }

    /// Filter controls are open.
    fn filter_open(&self) -> bool {
        match self.screen() {
            ScreenId::Issues => self.issues_state.filter_ui.controls_open,
            ScreenId::PullRequests => self.prs_state.filter_ui.controls_open,
            ScreenId::Actions => self.actions_state.ui.filter_ui_open,
            _ => false,
        }
    }

    /// An overlay with nothing unsaved behind it is open.
    ///
    /// Host confirmations are counted by their own layer, so they are excluded
    /// here rather than counted twice.
    fn plain_overlay_open(&self) -> bool {
        !matches!(self.modal, ModalState::None) && !self.host_confirmation_open()
    }

    /// The focused panel holds transient state of its own.
    fn panel_transient_open(&self) -> bool {
        self.dashboard_grab.is_some()
            || self.split_grab_index.is_some()
            || self.selection.is_some()
            || self.detail_panel_focused()
    }

    /// A detail panel is focused, which Back returns from before leaving.
    fn detail_panel_focused(&self) -> bool {
        match self.screen() {
            ScreenId::Issues => {
                self.issues_state.issue_focus == super::types::IssueFocus::IssueDetail
            }
            ScreenId::PullRequests => matches!(
                self.prs_state.pr_focus,
                super::types::PrFocus::PrDetail | super::types::PrFocus::PrChanges
            ),
            ScreenId::Actions => self.actions_state.focus == super::types::ActionsFocus::Detail,
            ScreenId::Dashboard
            | ScreenId::Repositories
            | ScreenId::Errors
            | ScreenId::Terminals => false,
        }
    }
}
