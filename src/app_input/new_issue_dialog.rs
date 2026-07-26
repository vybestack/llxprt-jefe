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

use super::AppStateHandle;

/// Outcome of resolving a key against the currently-active modal.
#[derive(Debug)]
pub enum DialogKey {
    /// The New Issue dialog handled (consumed) the key; the wrapped event is
    /// `None` when the key was intentionally a no-op.
    Handled(Option<AppEvent>),
    /// The active modal is not the New Issue dialog; the caller should run the
    /// generic form key handler.
    NotHandled,
}

/// Resolve a form-mode key event against the active modal. When the modal is
/// the New Issue dialog, returns `DialogKey::Handled(event)` so the caller
/// (`handle_mode_form_key`) can apply + persist without duplicating the modal
/// check. Returns `DialogKey::NotHandled` for any other modal.
#[must_use]
pub fn resolve_key_for_modal(app_state: &AppStateHandle, key_event: &KeyEvent) -> DialogKey {
    let state = app_state.read();
    if matches!(state.modal, ModalState::NewIssue { .. }) {
        DialogKey::Handled(resolve_new_issue_dialog_key(&state.modal, key_event))
    } else {
        DialogKey::NotHandled
    }
}

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
        // Tab always cycles field focus so the user can navigate out of the
        // body editor; Down moves the body cursor when Body is focused
        // (issue #407).
        KeyCode::Tab => Some(AppEvent::NewIssueFocusNext),
        KeyCode::Down => Some(resolve_down(focus)),
        KeyCode::BackTab => Some(AppEvent::NewIssueFocusPrev),
        KeyCode::Up => Some(resolve_up(focus)),
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

/// Down moves the body cursor down when Body is focused; otherwise cycles
/// focus forward (issue #407).
fn resolve_down(focus: NewIssueDialogFocus) -> AppEvent {
    match focus {
        NewIssueDialogFocus::Body => AppEvent::NewIssueBodyCursorDown,
        _ => AppEvent::NewIssueFocusNext,
    }
}

/// Up moves the body cursor up when Body is focused; otherwise cycles focus
/// backward (issue #407).
fn resolve_up(focus: NewIssueDialogFocus) -> AppEvent {
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
        // Multi-select pickers are not implemented in this slice; Space is a
        // no-op until they are.
        NewIssueDialogFocus::Labels
        | NewIssueDialogFocus::Milestone
        | NewIssueDialogFocus::Project
        | NewIssueDialogFocus::Assignees => None,
    }
}

fn resolve_char(focus: NewIssueDialogFocus, c: char) -> Option<AppEvent> {
    match focus {
        NewIssueDialogFocus::Title => Some(AppEvent::NewIssueTitleChar(c)),
        NewIssueDialogFocus::Body => Some(AppEvent::NewIssueBodyChar(c)),
        // Picker fields (Template, Type, Labels, Milestone, Project,
        // Assignees) are cycled via Space, not free text. Returning None
        // intentionally drops typed chars so they do not leak into Title.
        NewIssueDialogFocus::Template
        | NewIssueDialogFocus::Type
        | NewIssueDialogFocus::Labels
        | NewIssueDialogFocus::Milestone
        | NewIssueDialogFocus::Project
        | NewIssueDialogFocus::Assignees => None,
    }
}
