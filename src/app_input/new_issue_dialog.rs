//! Key routing for the New Issue dialog modal (issue #407).
//!
//! The dialog reuses `InputMode::Form` (set by `modal_input_mode` for
//! `ModalState::NewIssue`), but its title/body/picker fields need dedicated
//! `NewIssue*` events instead of the generic `FormChar`/`FormBackspace`
//! events used by the agent/repository forms. This module translates raw
//! `KeyEvent`s into the right `NewIssue*` event for the currently-focused
//! dialog field.
//!
//! Pure: returns `Option<AppEvent>`; the caller applies + persists.

use iocraft::prelude::{KeyCode, KeyEvent, KeyModifiers};

use jefe::state::{AppEvent, ModalState, NewIssueDialogFocus};

/// Resolve a form-mode key event to a New Issue dialog event, or `None` when
/// the key does not map to a dialog action. The caller is `handle_mode_form_key`
/// when `state.modal` is `ModalState::NewIssue`.
#[must_use]
pub fn resolve_new_issue_dialog_key(modal: &ModalState, key_event: &KeyEvent) -> Option<AppEvent> {
    let ModalState::NewIssue { state, .. } = modal else {
        return None;
    };
    let focus = state.focus;
    let submit = matches!(
        key_event.code,
        KeyCode::Enter if key_event.modifiers.contains(KeyModifiers::ALT)
            || key_event.modifiers.contains(KeyModifiers::CONTROL)
    );
    if submit {
        return Some(AppEvent::NewIssueSubmit);
    }
    match key_event.code {
        KeyCode::Esc => Some(AppEvent::NewIssueCancel),
        KeyCode::Enter => Some(resolve_enter(focus)),
        KeyCode::Tab | KeyCode::Down => Some(resolve_tab_down(focus)),
        KeyCode::BackTab | KeyCode::Up => Some(resolve_backtab_up(focus)),
        KeyCode::Left => resolve_left(focus),
        KeyCode::Right => resolve_right(focus),
        KeyCode::Home => resolve_home(focus),
        KeyCode::End => resolve_end(focus),
        KeyCode::Backspace => resolve_backspace(focus),
        KeyCode::Delete => resolve_delete(focus),
        KeyCode::Char(' ') => resolve_space(focus),
        KeyCode::Char(c) if !key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            resolve_char(focus, c)
        }
        _ => None,
    }
}

fn resolve_enter(focus: NewIssueDialogFocus) -> AppEvent {
    match focus {
        NewIssueDialogFocus::Body => AppEvent::NewIssueBodyNewline,
        _ => AppEvent::NewIssueSubmit,
    }
}

fn resolve_tab_down(focus: NewIssueDialogFocus) -> AppEvent {
    match focus {
        NewIssueDialogFocus::Body => AppEvent::NewIssueBodyCursorDown,
        _ => AppEvent::NewIssueFocusNext,
    }
}

fn resolve_backtab_up(focus: NewIssueDialogFocus) -> AppEvent {
    match focus {
        NewIssueDialogFocus::Body => AppEvent::NewIssueBodyCursorUp,
        _ => AppEvent::NewIssueFocusPrev,
    }
}

fn resolve_left(focus: NewIssueDialogFocus) -> Option<AppEvent> {
    match focus {
        NewIssueDialogFocus::Title => Some(AppEvent::NewIssueTitleCursorLeft),
        NewIssueDialogFocus::Body => Some(AppEvent::NewIssueBodyCursorLeft),
        _ => None,
    }
}

fn resolve_right(focus: NewIssueDialogFocus) -> Option<AppEvent> {
    match focus {
        NewIssueDialogFocus::Title => Some(AppEvent::NewIssueTitleCursorRight),
        NewIssueDialogFocus::Body => Some(AppEvent::NewIssueBodyCursorRight),
        _ => None,
    }
}

fn resolve_home(focus: NewIssueDialogFocus) -> Option<AppEvent> {
    match focus {
        NewIssueDialogFocus::Title => Some(AppEvent::NewIssueTitleCursorHome),
        NewIssueDialogFocus::Body => Some(AppEvent::NewIssueBodyCursorHome),
        _ => None,
    }
}

fn resolve_end(focus: NewIssueDialogFocus) -> Option<AppEvent> {
    match focus {
        NewIssueDialogFocus::Title => Some(AppEvent::NewIssueTitleCursorEnd),
        NewIssueDialogFocus::Body => Some(AppEvent::NewIssueBodyCursorEnd),
        _ => None,
    }
}

fn resolve_backspace(focus: NewIssueDialogFocus) -> Option<AppEvent> {
    match focus {
        NewIssueDialogFocus::Title => Some(AppEvent::NewIssueTitleBackspace),
        NewIssueDialogFocus::Body => Some(AppEvent::NewIssueBodyBackspace),
        _ => None,
    }
}

fn resolve_delete(focus: NewIssueDialogFocus) -> Option<AppEvent> {
    match focus {
        NewIssueDialogFocus::Title => Some(AppEvent::NewIssueTitleDelete),
        NewIssueDialogFocus::Body => Some(AppEvent::NewIssueBodyDelete),
        _ => None,
    }
}

fn resolve_space(focus: NewIssueDialogFocus) -> Option<AppEvent> {
    match focus {
        NewIssueDialogFocus::Template => Some(AppEvent::NewIssueTemplateNext),
        NewIssueDialogFocus::Type => Some(AppEvent::NewIssueTypeNext),
        NewIssueDialogFocus::Title => Some(AppEvent::NewIssueTitleChar(' ')),
        NewIssueDialogFocus::Body => Some(AppEvent::NewIssueBodyChar(' ')),
        _ => None,
    }
}

fn resolve_char(focus: NewIssueDialogFocus, c: char) -> Option<AppEvent> {
    match focus {
        NewIssueDialogFocus::Title => Some(AppEvent::NewIssueTitleChar(c)),
        NewIssueDialogFocus::Body => Some(AppEvent::NewIssueBodyChar(c)),
        // Picker fields consume chars to avoid leaking into the title.
        _ => None,
    }
}
