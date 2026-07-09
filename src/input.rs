//! Input-mode and key-routing helpers.

use iocraft::prelude::{KeyCode, KeyEvent};

use crate::state::{AppState, InlineState, ModalState, PaneFocus, ScreenMode};

/// High-level mode used to route keyboard events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    TerminalCapture,
    Help,
    Search,
    Form,
    Confirm,
    /// Theme picker overlay.
    ThemePicker,
    /// @plan PLAN-20260329-ISSUES-MODE.P03
    /// @requirement REQ-ISS-002
    IssuesNormal,
    /// @plan PLAN-20260329-ISSUES-MODE.P03
    /// @requirement REQ-ISS-002
    IssuesInline,
    /// @plan PLAN-20260329-ISSUES-MODE.P03
    /// @requirement REQ-ISS-002
    IssuesSearch,
    /// @plan PLAN-20260329-ISSUES-MODE.P03
    /// @requirement REQ-ISS-002
    IssuesFilter,
    /// @plan PLAN-20260329-ISSUES-MODE.P03
    /// @requirement REQ-ISS-002
    IssuesChooser,
}

/// Search-mode key routing result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKeyRoute {
    CloseAndConsume,
    EditQueryChar(char),
    Backspace,
    CloseAndReroute,
    Ignore,
}

/// Resolve the active input mode from current app state.
#[must_use]
pub fn input_mode_for_state(state: &AppState) -> InputMode {
    match state.modal {
        ModalState::Help => return InputMode::Help,
        ModalState::Search { .. } => return InputMode::Search,
        ModalState::ThemePicker { .. } => return InputMode::ThemePicker,
        ModalState::NewRepository { .. }
        | ModalState::EditRepository { .. }
        | ModalState::NewAgent { .. }
        | ModalState::EditAgent { .. } => return InputMode::Form,
        ModalState::ConfirmDeleteRepository { .. }
        | ModalState::ConfirmDeleteAgent { .. }
        | ModalState::ConfirmKillAgent { .. }
        | ModalState::PreflightPrompt { .. } => return InputMode::Confirm,
        ModalState::None => {}
    }

    // Issues mode detection — must be before Normal fallback
    // @plan PLAN-20260329-ISSUES-MODE.P03
    // @requirement REQ-ISS-002
    // @pseudocode component-003 lines 01-02
    if state.screen_mode == ScreenMode::DashboardIssues {
        if state.issues_state.inline_state != InlineState::None {
            return InputMode::IssuesInline;
        }
        if state.issues_state.agent_chooser.is_some() {
            return InputMode::IssuesChooser;
        }
        if state.issues_state.search_input_focused {
            return InputMode::IssuesSearch;
        }
        if state.issues_state.filter_controls_open {
            return InputMode::IssuesFilter;
        }
        return InputMode::IssuesNormal;
    }

    if state.terminal_focused && state.pane_focus == PaneFocus::Terminal {
        InputMode::TerminalCapture
    } else {
        InputMode::Normal
    }
}

/// Route a key while search mode is active.
#[must_use]
pub fn route_search_key(key: &KeyEvent) -> SearchKeyRoute {
    match key.code {
        KeyCode::Esc | KeyCode::Enter => SearchKeyRoute::CloseAndConsume,
        KeyCode::Backspace => SearchKeyRoute::Backspace,
        KeyCode::Char(c)
            if !key.modifiers.intersects(
                iocraft::prelude::KeyModifiers::CONTROL | iocraft::prelude::KeyModifiers::ALT,
            ) =>
        {
            SearchKeyRoute::EditQueryChar(c)
        }
        KeyCode::Char(_) | KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {
            SearchKeyRoute::CloseAndReroute
        }
        _ => SearchKeyRoute::Ignore,
    }
}
