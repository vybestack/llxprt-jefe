//! Raw Issues editor, search, property, and form mutation routing.

use iocraft::prelude::{KeyCode, KeyEvent};
use jefe::state::{
    AppEvent, AppState, ComposerTarget, DetailSubfocus, InlineState, NewIssueFormFocus,
};

#[must_use]
pub(super) fn resolve_raw_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    if let Some(editor) = state.issues_state.property_editor.as_ref() {
        return resolve_property_text(editor.kind, key_event);
    }
    if let Some(chooser) = state.issues_state.close_reason_chooser.as_ref()
        && chooser.duplicate_search.is_some()
    {
        return resolve_duplicate_search_text(key_event);
    }
    if state.issues_state.inline_state != InlineState::None {
        if state.issues_state.new_issue_form.is_some()
            && matches!(
                state.issues_state.inline_state,
                InlineState::Composer {
                    target: ComposerTarget::NewIssue,
                    ..
                }
            )
        {
            return resolve_new_issue_text(state, key_event);
        }
        return resolve_inline_text(key_event);
    }
    if state.issues_state.search_input_focused {
        return resolve_search_text(state, key_event);
    }
    if state.issues_state.filter_ui.controls_open {
        return super::issues_filter::resolve_raw_key(state, key_event);
    }
    None
}

fn resolve_inline_text(key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Enter if key_event.modifiers.is_empty() => Some(AppEvent::InlineNewline),
        KeyCode::Char(character)
            if super::raw_key_mutations::text_modifiers(key_event.modifiers) =>
        {
            Some(AppEvent::InlineChar(character))
        }
        KeyCode::Backspace => Some(AppEvent::InlineBackspace),
        KeyCode::Delete => Some(AppEvent::InlineDelete),
        KeyCode::Left => Some(AppEvent::InlineCursorLeft),
        KeyCode::Right => Some(AppEvent::InlineCursorRight),
        KeyCode::Up => Some(AppEvent::InlineCursorUp),
        KeyCode::Down => Some(AppEvent::InlineCursorDown),
        KeyCode::Home => Some(AppEvent::InlineCursorHome),
        KeyCode::End => Some(AppEvent::InlineCursorEnd),
        _ => None,
    }
}

fn resolve_new_issue_text(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    let focus = state.issues_state.new_issue_form.as_ref()?.focus;
    match key_event.code {
        KeyCode::Enter if key_event.modifiers.is_empty() && focus == NewIssueFormFocus::Body => {
            Some(AppEvent::NewIssueBodyNewline)
        }
        KeyCode::Up if focus == NewIssueFormFocus::Body => Some(AppEvent::NewIssueBodyCursorUp),
        KeyCode::Down if focus == NewIssueFormFocus::Body => Some(AppEvent::NewIssueBodyCursorDown),
        KeyCode::Left => new_issue_cursor(focus, true, false),
        KeyCode::Right => new_issue_cursor(focus, false, false),
        KeyCode::Home => new_issue_cursor(focus, true, true),
        KeyCode::End => new_issue_cursor(focus, false, true),
        KeyCode::Backspace => new_issue_delete(focus, true),
        KeyCode::Delete => new_issue_delete(focus, false),
        KeyCode::Char(character)
            if super::raw_key_mutations::text_modifiers(key_event.modifiers)
                && (character != ' '
                    || matches!(focus, NewIssueFormFocus::Title | NewIssueFormFocus::Body)) =>
        {
            new_issue_char(focus, character)
        }
        _ => None,
    }
}

fn new_issue_cursor(focus: NewIssueFormFocus, backward: bool, edge: bool) -> Option<AppEvent> {
    match (focus, backward, edge) {
        (NewIssueFormFocus::Title, true, false) => Some(AppEvent::NewIssueTitleCursorLeft),
        (NewIssueFormFocus::Title, false, false) => Some(AppEvent::NewIssueTitleCursorRight),
        (NewIssueFormFocus::Title, true, true) => Some(AppEvent::NewIssueTitleCursorHome),
        (NewIssueFormFocus::Title, false, true) => Some(AppEvent::NewIssueTitleCursorEnd),
        (NewIssueFormFocus::Body, true, false) => Some(AppEvent::NewIssueBodyCursorLeft),
        (NewIssueFormFocus::Body, false, false) => Some(AppEvent::NewIssueBodyCursorRight),
        (NewIssueFormFocus::Body, true, true) => Some(AppEvent::NewIssueBodyCursorHome),
        (NewIssueFormFocus::Body, false, true) => Some(AppEvent::NewIssueBodyCursorEnd),
        _ => None,
    }
}

fn new_issue_delete(focus: NewIssueFormFocus, backward: bool) -> Option<AppEvent> {
    match (focus, backward) {
        (NewIssueFormFocus::Title, true) => Some(AppEvent::NewIssueTitleBackspace),
        (NewIssueFormFocus::Title, false) => Some(AppEvent::NewIssueTitleDelete),
        (NewIssueFormFocus::Body, true) => Some(AppEvent::NewIssueBodyBackspace),
        (NewIssueFormFocus::Body, false) => Some(AppEvent::NewIssueBodyDelete),
        _ => None,
    }
}

fn new_issue_char(focus: NewIssueFormFocus, character: char) -> Option<AppEvent> {
    match focus {
        NewIssueFormFocus::Title => Some(AppEvent::NewIssueTitleChar(character)),
        NewIssueFormFocus::Body => Some(AppEvent::NewIssueBodyChar(character)),
        _ => None,
    }
}

fn resolve_property_text(
    kind: jefe::state::IssuePropertyKind,
    key_event: &KeyEvent,
) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Char(' ') if kind != jefe::state::IssuePropertyKind::Title => None,
        KeyCode::Char(character)
            if super::raw_key_mutations::text_modifiers(key_event.modifiers) =>
        {
            Some(AppEvent::IssuePropertyEditorTitleChar(character))
        }
        KeyCode::Backspace => Some(AppEvent::IssuePropertyEditorTitleBackspace),
        KeyCode::Delete => Some(AppEvent::IssuePropertyEditorTitleDelete),
        KeyCode::Left => Some(AppEvent::IssuePropertyEditorTitleCursorLeft),
        KeyCode::Right => Some(AppEvent::IssuePropertyEditorTitleCursorRight),
        KeyCode::Home => Some(AppEvent::IssuePropertyEditorTitleCursorHome),
        KeyCode::End => Some(AppEvent::IssuePropertyEditorTitleCursorEnd),
        _ => None,
    }
}

fn resolve_duplicate_search_text(key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Char(character)
            if character.is_ascii_digit()
                && super::raw_key_mutations::text_modifiers(key_event.modifiers) =>
        {
            Some(AppEvent::CloseReasonDuplicateSearchChar(character))
        }
        KeyCode::Backspace => Some(AppEvent::CloseReasonDuplicateSearchBackspace),
        _ => None,
    }
}

fn resolve_search_text(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    let mut query = state.issues_state.search_query.clone();
    match key_event.code {
        KeyCode::Char(character)
            if super::raw_key_mutations::text_modifiers(key_event.modifiers) =>
        {
            query.push(character);
        }
        KeyCode::Backspace => {
            query.pop();
        }
        _ => return None,
    }
    Some(AppEvent::SetSearchQuery { query })
}

pub(super) fn editor_event_for_subfocus(subfocus: DetailSubfocus) -> Option<AppEvent> {
    match subfocus {
        DetailSubfocus::Body => Some(AppEvent::OpenInlineEditor {
            target: jefe::state::EditorTarget::IssueBody,
        }),
        DetailSubfocus::Comment(comment_index) => Some(AppEvent::OpenInlineEditor {
            target: jefe::state::EditorTarget::Comment { comment_index },
        }),
        DetailSubfocus::NewComment => None,
    }
}

pub(super) fn reply_event_for_subfocus(subfocus: DetailSubfocus) -> Option<AppEvent> {
    match subfocus {
        DetailSubfocus::Comment(comment_index) => {
            Some(AppEvent::OpenReplyComposer { comment_index })
        }
        _ => None,
    }
}

#[cfg(test)]
pub fn resolve_issues_key_event(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    super::resolve_test_registry_event(state, key_event, 120, 40)
}

#[cfg(test)]
fn resolve_issues_key_event_for_rows(
    state: &AppState,
    key_event: &KeyEvent,
    terminal_rows: u16,
) -> Option<AppEvent> {
    super::resolve_test_registry_event(state, key_event, 120, terminal_rows)
}

#[cfg(test)]
#[path = "issues_key_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "issues_rewrite_key_tests.rs"]
mod rewrite_key_tests;

#[cfg(test)]
#[path = "issues_property_key_tests.rs"]
mod issues_property_key_tests;

#[cfg(test)]
#[path = "issues_close_reason_key_tests.rs"]
mod close_reason_key_tests;
