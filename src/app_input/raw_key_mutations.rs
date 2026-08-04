//! Raw text and cursor mutation routing for registry-owned S4 contexts.
//!
//! This module deliberately excludes navigation, chooser, lifecycle, submit,
//! cancel, and other action controls. Those inputs resolve through the action
//! registry before reaching the typed executor.

use iocraft::prelude::{KeyCode, KeyEvent, KeyModifiers};
use jefe::input::{InputMode, input_mode_for_state};
use jefe::state::{AppEvent, AppState, ModalState};

#[must_use]
pub fn resolve(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match input_mode_for_state(state) {
        InputMode::DashboardSearch => super::dashboard_search::resolve_raw_key(state, key_event),
        InputMode::IssuesInline
        | InputMode::IssuesSearch
        | InputMode::IssuesFilter
        | InputMode::IssuesChooser
        | InputMode::IssuesNormal => super::issues::resolve_raw_key(state, key_event),
        InputMode::PrsInline
        | InputMode::PrsSearch
        | InputMode::PrsFilter
        | InputMode::PrsChooser
        | InputMode::PrsNormal => super::prs::resolve_raw_key(state, key_event),
        InputMode::ActionsSearch | InputMode::ActionsFilter | InputMode::ActionsNormal => {
            super::actions::resolve_raw_key(state, key_event)
        }
        InputMode::Search | InputMode::Form => resolve_modal_raw_key(state, key_event),
        InputMode::TerminalCapture
        | InputMode::Normal
        | InputMode::Help
        | InputMode::Confirm
        | InputMode::Auth => None,
    }
}

fn resolve_modal_raw_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match state.modal {
        ModalState::Search { .. } => resolve_search_modal_raw_key(key_event),
        ModalState::NewRepository { .. }
        | ModalState::EditRepository { .. }
        | ModalState::NewAgent { .. }
        | ModalState::EditAgent { .. }
        | ModalState::GeneratedAgent { .. }
        | ModalState::WorkflowDispatch { .. } => resolve_form_modal_raw_key(key_event),
        _ => None,
    }
}

fn resolve_search_modal_raw_key(key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Backspace => Some(AppEvent::FormBackspace),
        KeyCode::Char(character) if text_modifiers(key_event.modifiers) => {
            Some(AppEvent::FormChar(character))
        }
        _ => None,
    }
}

fn resolve_form_modal_raw_key(key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Left => Some(AppEvent::FormMoveCursorLeft),
        KeyCode::Right => Some(AppEvent::FormMoveCursorRight),
        KeyCode::Home => Some(AppEvent::FormMoveCursorStart),
        KeyCode::End => Some(AppEvent::FormMoveCursorEnd),
        KeyCode::Backspace => Some(AppEvent::FormBackspace),
        KeyCode::Delete => Some(AppEvent::FormDelete),
        KeyCode::Char(character) if character != ' ' && text_modifiers(key_event.modifiers) => {
            Some(AppEvent::FormChar(character))
        }
        _ => None,
    }
}

pub(super) fn text_modifiers(modifiers: KeyModifiers) -> bool {
    !modifiers.intersects(
        KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::META,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use iocraft::prelude::KeyEventKind;
    use jefe::state::{InlineState, IssuesState, RepositoryFormCursor, ScreenId};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(KeyEventKind::Press, code)
    }

    #[test]
    fn inline_text_is_raw_but_submit_and_cancel_are_not() {
        let state = AppState {
            nav: crate::state::navigation::NavState::rooted(ScreenId::Issues),
            issues_state: IssuesState {
                active: true,
                inline_state: InlineState::Composer {
                    target: jefe::state::ComposerTarget::NewComment,
                    text: String::new(),
                    cursor: 0,
                },
                ..IssuesState::default()
            },
            ..AppState::default()
        };
        assert!(matches!(
            resolve(&state, &key(KeyCode::Char('x'))),
            Some(AppEvent::InlineChar('x'))
        ));
        assert!(resolve(&state, &key(KeyCode::Esc)).is_none());
        let mut submit = key(KeyCode::Enter);
        submit.modifiers = KeyModifiers::ALT;
        assert!(resolve(&state, &submit).is_none());
    }

    #[test]
    fn form_cursor_and_text_are_raw_but_navigation_is_not() {
        let mut state = AppState::default();
        state.modal = ModalState::NewRepository {
            fields: jefe::state::RepositoryFormFields::default(),
            focus: jefe::state::RepositoryFormFocus::Name,
            cursor: RepositoryFormCursor::default(),
        };
        assert!(matches!(
            resolve(&state, &key(KeyCode::Left)),
            Some(AppEvent::FormMoveCursorLeft)
        ));
        assert!(matches!(
            resolve(&state, &key(KeyCode::Char('x'))),
            Some(AppEvent::FormChar('x'))
        ));
        assert!(resolve(&state, &key(KeyCode::Tab)).is_none());
        assert!(resolve(&state, &key(KeyCode::Enter)).is_none());
    }
}
