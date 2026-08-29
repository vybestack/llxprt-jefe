//! Which unwindable layers a screen currently has open (issue #386, CW06-04).
//!
//! [`super::navigation_unwind::resolve_back`] states the precedence; this states
//! what is actually open. Keeping the two apart means the order is a property of
//! the contract rather than of whichever mode happened to be asked.
//!
//! Only the current screen's own state is consulted. A mode the user left still
//! holds its composer, chooser, search, and filter, and none of that belongs to
//! the screen they are now looking at.
//!
//! The projection is pure and reads only committed state. Every semantic Back
//! handler enters this shared authority; provider requests, terminal capture,
//! and Settings-owned transient editors retain their higher-priority routing.

use super::navigation_unwind::{BackLayer, BackResolution, LocalIntent, resolve_back};
use super::types::{ActionsFocus, AppState, InlineState, IssueFocus, ModalState, PrFocus};
use super::{AppEvent, ErrorsFocus, PrLifecycleEvent};
use crate::messages::{AppMessage, SettingsMessage};
use crate::workbench::{OverlayKind, ScreenId};

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

    /// Apply one shared Back decision and no lower-priority fallback.
    pub(super) fn apply_back(&mut self) {
        match self.back_resolution() {
            BackResolution::Local(LocalIntent::ResolveDirty(choice)) => {
                if self.compiled_screen() == Some(ScreenId::Settings) {
                    self.reduce_message_body(AppMessage::Settings(Box::new(
                        SettingsMessage::ResolveDirty(choice),
                    )));
                } else {
                    let _ = self.resolve_dirty(choice);
                }
            }
            BackResolution::Local(LocalIntent::ClearPanelTransient)
                if self.compiled_screen() == Some(ScreenId::Settings) =>
            {
                self.reduce_message_body(AppMessage::Settings(Box::new(
                    SettingsMessage::ReloadCancelled,
                )));
            }
            BackResolution::Local(intent) => {
                let event = self.local_back_event(intent);
                debug_assert!(event.is_some(), "projected Back layer has no owner");
                if let Some(event) = event {
                    self.reduce_message_body(event.into());
                }
            }
            BackResolution::Leave
                if self.compiled_screen() != Some(ScreenId::Settings)
                    && self.nav.current().dirty.is_dirty() =>
            {
                let _ = self.leave_screen();
            }
            BackResolution::Leave if self.compiled_screen() == Some(ScreenId::Settings) => {
                self.reduce_message_body(AppMessage::Settings(Box::new(SettingsMessage::Back)));
            }
            BackResolution::Leave => {
                if let Some(event) = self.leave_back_event() {
                    self.reduce_message_body(event.into());
                } else {
                    let _ = self.leave_screen();
                }
            }
            BackResolution::Nothing => {}
        }
    }

    fn local_back_event(&self, intent: LocalIntent) -> Option<AppEvent> {
        match intent {
            LocalIntent::CloseHostConfirmation => Some(AppEvent::CloseModal),
            LocalIntent::ResolveDirty(_) => None,
            LocalIntent::CloseChooser => self.close_chooser_event(),
            LocalIntent::CloseEditor => self.close_editor_event(),
            LocalIntent::CloseSearch => self.close_search_event(),
            LocalIntent::CloseFilterControls => self.close_filter_event(),
            LocalIntent::CloseOverlay => Some(if matches!(self.modal, ModalState::Auth { .. }) {
                AppEvent::AuthCancelled
            } else {
                AppEvent::CloseModal
            }),
            LocalIntent::ClearPanelTransient => self.clear_panel_transient_event(),
        }
    }

    fn leave_back_event(&self) -> Option<AppEvent> {
        match self.compiled_screen() {
            Some(ScreenId::Repositories) => Some(AppEvent::ExitSplitMode),
            Some(ScreenId::Issues) => Some(AppEvent::ExitIssuesMode),
            Some(ScreenId::PullRequests) => Some(AppEvent::ExitPrsMode),
            Some(ScreenId::Actions) => Some(AppEvent::ExitActionsMode),
            Some(ScreenId::Errors) => Some(AppEvent::ExitErrorsMode),
            Some(ScreenId::Terminals) => Some(AppEvent::ExitTerminalManagerMode),
            Some(ScreenId::Settings) | None => None,
        }
    }

    /// Whether leaving this screen would go anywhere.
    ///
    /// The home screen with nothing stacked beneath it is the one place Back
    /// has nothing left to do.
    #[must_use]
    pub fn can_leave_screen(&self) -> bool {
        self.nav.depth() > 0 || !self.current_is_composition_root()
    }

    /// A generic confirmation the current screen instance owns.
    fn host_confirmation_open(&self) -> bool {
        self.nav
            .current()
            .overlays()
            .generic_confirmation()
            .is_some()
    }

    fn close_chooser_event(&self) -> Option<AppEvent> {
        match self.compiled_screen() {
            Some(ScreenId::Issues) if self.issues_state.property_editor.is_some() => {
                Some(AppEvent::IssuePropertyEditorCancel)
            }
            Some(ScreenId::Issues) if self.issues_state.close_reason_chooser.is_some() => {
                Some(AppEvent::CloseReasonCancel)
            }
            Some(ScreenId::Issues) if self.issues_state.delete_confirm.is_some() => {
                Some(AppEvent::IssueDeleteCancel)
            }
            Some(ScreenId::Issues) if self.issues_state.agent_chooser.is_some() => {
                Some(AppEvent::AgentChooserCancel)
            }
            Some(ScreenId::PullRequests) if self.prs_state.delete_confirm.is_some() => {
                Some(PrLifecycleEvent::DeleteCancel.into())
            }
            Some(ScreenId::PullRequests) if self.prs_state.property_editor.is_some() => {
                Some(AppEvent::PrPropertyEditorCancel)
            }
            Some(ScreenId::PullRequests) if self.prs_state.merge_chooser.is_some() => {
                Some(PrLifecycleEvent::MergeCancel.into())
            }
            Some(ScreenId::PullRequests) if self.prs_state.agent_chooser.is_some() => {
                Some(AppEvent::PrAgentChooserCancel)
            }
            _ => None,
        }
    }

    fn close_editor_event(&self) -> Option<AppEvent> {
        match self.compiled_screen() {
            Some(ScreenId::Issues) if self.issues_state.new_issue_form.is_some() => {
                Some(AppEvent::NewIssueCancel)
            }
            Some(ScreenId::Issues) => Some(AppEvent::InlineCancelOrEsc),
            Some(ScreenId::PullRequests) if self.prs_state.new_pr_form.is_some() => {
                Some(PrLifecycleEvent::NewFormCancel.into())
            }
            Some(ScreenId::PullRequests) => Some(AppEvent::PrInlineCancelOrEsc),
            _ => None,
        }
    }

    fn close_search_event(&self) -> Option<AppEvent> {
        if self.active_overlay_kind() == Some(OverlayKind::Search) {
            return Some(AppEvent::CloseModal);
        }
        match self.compiled_screen() {
            Some(ScreenId::Issues) => Some(if self.issues_state.search_query.is_empty() {
                AppEvent::BlurSearchInput
            } else {
                AppEvent::ClearSearch
            }),
            Some(ScreenId::PullRequests) => Some(if self.prs_state.search_query.is_empty() {
                AppEvent::PrBlurSearchInput
            } else {
                AppEvent::PrClearSearch
            }),
            Some(ScreenId::Actions) => Some(if self.actions_state.search_query.is_empty() {
                AppEvent::ActionsBlurSearchInput
            } else {
                AppEvent::ActionsClearSearch
            }),
            _ => None,
        }
    }

    fn close_filter_event(&self) -> Option<AppEvent> {
        match self.compiled_screen() {
            Some(ScreenId::Issues) => Some(AppEvent::CloseFilterControls),
            Some(ScreenId::PullRequests) => Some(AppEvent::PrCloseFilterControls),
            Some(ScreenId::Actions) => Some(AppEvent::ActionsCloseFilterControls),
            _ => None,
        }
    }

    fn clear_panel_transient_event(&self) -> Option<AppEvent> {
        match self.compiled_screen() {
            Some(ScreenId::Issues) if self.issues_state.issue_focus == IssueFocus::IssueDetail => {
                Some(AppEvent::RefocusIssueList)
            }
            Some(ScreenId::PullRequests) if self.prs_state.pr_focus == PrFocus::PrChanges => {
                Some(AppEvent::PrChangesBack)
            }
            Some(ScreenId::PullRequests) if self.prs_state.pr_focus == PrFocus::PrDetail => {
                Some(AppEvent::RefocusPrList)
            }
            Some(ScreenId::Actions) if self.actions_state.focus == ActionsFocus::Detail => {
                Some(AppEvent::ActionsDetailEscape)
            }
            Some(ScreenId::Errors) if self.errors_state.focus == ErrorsFocus::ErrorDetail => {
                Some(AppEvent::RefocusErrorList)
            }
            Some(ScreenId::Settings) if self.settings_state.reload_confirm => None,
            _ => None,
        }
    }

    /// A chooser or property editor is taking the keys.
    ///
    /// Only the current screen's own state is consulted. A mode the user left
    /// may still be holding a chooser or a half-typed filter, and that must not
    /// change what Back does on the screen they are actually looking at.
    fn chooser_open(&self) -> bool {
        match self.compiled_screen() {
            Some(ScreenId::Issues) => {
                self.issues_state.agent_chooser.is_some()
                    || self.issues_state.delete_confirm.is_some()
                    || self.issues_state.property_editor.is_some()
                    || self.issues_state.close_reason_chooser.is_some()
            }
            Some(ScreenId::PullRequests) => {
                self.prs_state.agent_chooser.is_some()
                    || self.prs_state.delete_confirm.is_some()
                    || self.prs_state.property_editor.is_some()
                    || self.prs_state.merge_chooser.is_some()
            }
            _ => false,
        }
    }

    /// Text is being composed or edited in place.
    fn editor_open(&self) -> bool {
        match self.compiled_screen() {
            Some(ScreenId::Issues) => {
                self.issues_state.inline_state != InlineState::None
                    || self.issues_state.new_issue_form.is_some()
            }
            Some(ScreenId::PullRequests) => {
                self.prs_state.inline_state != InlineState::None
                    || self.prs_state.new_pr_form.is_some()
            }
            _ => false,
        }
    }

    /// A search input holds the keys.
    fn search_focused(&self) -> bool {
        if self.active_overlay_kind() == Some(OverlayKind::Search) {
            return true;
        }
        match self.compiled_screen() {
            Some(ScreenId::Issues) => self.issues_state.search_input_focused,
            Some(ScreenId::PullRequests) => self.prs_state.search_input_focused,
            Some(ScreenId::Actions) => self.actions_state.ui.search_input_focused,
            _ => false,
        }
    }

    /// Filter controls are open.
    fn filter_open(&self) -> bool {
        match self.compiled_screen() {
            Some(ScreenId::Issues) => self.issues_state.filter_ui.controls_open,
            Some(ScreenId::PullRequests) => self.prs_state.filter_ui.controls_open,
            Some(ScreenId::Actions) => self.actions_state.ui.filter_ui_open,
            _ => false,
        }
    }

    /// An overlay with nothing unsaved behind it is open.
    ///
    /// Host confirmations are counted by their own layer, so they are excluded
    /// here rather than counted twice.
    fn plain_overlay_open(&self) -> bool {
        self.active_overlay_kind() == Some(OverlayKind::Help)
            || !matches!(self.modal, ModalState::None) && !self.host_confirmation_open()
    }

    /// The focused panel holds transient state of its own.
    fn panel_transient_open(&self) -> bool {
        self.detail_panel_focused()
            || self.compiled_screen() == Some(ScreenId::Settings)
                && self.settings_state.reload_confirm
    }

    /// A detail panel is focused, which Back returns from before leaving.
    fn detail_panel_focused(&self) -> bool {
        match self.compiled_screen() {
            Some(ScreenId::Issues) => {
                self.issues_state.issue_focus == super::types::IssueFocus::IssueDetail
            }
            Some(ScreenId::PullRequests) => matches!(
                self.prs_state.pr_focus,
                super::types::PrFocus::PrDetail | super::types::PrFocus::PrChanges
            ),
            Some(ScreenId::Actions) => {
                self.actions_state.focus == super::types::ActionsFocus::Detail
            }
            Some(ScreenId::Errors) => self.errors_state.focus == ErrorsFocus::ErrorDetail,
            Some(ScreenId::Repositories | ScreenId::Terminals | ScreenId::Settings) | None => false,
        }
    }
}
