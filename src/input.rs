//! Input-mode and key-routing helpers.

use iocraft::prelude::{KeyCode, KeyEvent};

use crate::state::{AppState, ModalState, PaneFocus};

/// High-level mode used to route keyboard events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    TerminalCapture,
    Help,
    Search,
    Form,
    Confirm,
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
        ModalState::NewRepository { .. }
        | ModalState::EditRepository { .. }
        | ModalState::NewAgent { .. }
        | ModalState::EditAgent { .. } => return InputMode::Form,
        ModalState::ConfirmDeleteRepository { .. }
        | ModalState::ConfirmDeleteAgent { .. }
        | ModalState::ConfirmKillAgent { .. } => return InputMode::Confirm,
        ModalState::None => {}
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
