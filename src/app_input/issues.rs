//! Issues-mode key event dispatch.
//!
//! @plan PLAN-20260329-ISSUES-MODE.P09
//! @plan PLAN-20260329-ISSUES-MODE.P10
//! @plan PLAN-20260329-ISSUES-MODE.P11
//! @requirement REQ-ISS-002
//! @pseudocode component-003 lines 01-38

use iocraft::prelude::*;

use jefe::state::{AppEvent, AppState, DetailSubfocus, InlineState, IssueFocus};

use super::issues_filter::resolve_filter_key_event;

use super::{AppStateHandle, SharedContext};

/// Pure key-routing logic for Issues Mode.
///
/// Given the current application state and a key event, returns the `AppEvent`
/// that should be dispatched — or `None` if the key is suppressed/no-op.
///
/// This function is side-effect-free and testable without iocraft hooks.
/// Implements the 8-level priority chain from pseudocode component-003.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-002
/// @pseudocode component-003 lines 01-38
pub fn resolve_issues_key_event(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    if state.issues_state.delete_confirm.is_some() {
        return resolve_delete_confirm_key_event(key_event);
    }

    if state.issues_state.inline_state != InlineState::None {
        return resolve_inline_key_event(key_event);
    }

    if state.issues_state.agent_chooser.is_some() {
        return resolve_agent_chooser_key_event(key_event);
    }

    if state.issues_state.search_input_focused {
        return resolve_search_key_event(state, key_event);
    }

    if state.issues_state.filter_ui.controls_open {
        return resolve_filter_key_event(state, key_event);
    }

    resolve_global_issues_key_event(state, key_event)
        .or_else(|| resolve_focus_key_event(state, key_event))
        .or_else(|| resolve_pane_cycle_key_event(key_event))
}

fn resolve_inline_key_event(key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(AppEvent::InlineCancelOrEsc)
        }
        KeyCode::Esc => Some(AppEvent::InlineCancelOrEsc),
        KeyCode::Enter if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(AppEvent::InlineSubmit)
        }
        KeyCode::Enter => Some(AppEvent::InlineNewline),
        KeyCode::Char(c) => Some(AppEvent::InlineChar(c)),
        KeyCode::Backspace => Some(AppEvent::InlineBackspace),
        KeyCode::Delete => Some(AppEvent::InlineDelete),
        KeyCode::Left => Some(AppEvent::InlineCursorLeft),
        KeyCode::Right => Some(AppEvent::InlineCursorRight),
        KeyCode::Up => Some(AppEvent::InlineCursorUp),
        KeyCode::Down => Some(AppEvent::InlineCursorDown),
        _ => None,
    }
}

fn resolve_agent_chooser_key_event(key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Up => Some(AppEvent::AgentChooserNavigateUp),
        KeyCode::Down => Some(AppEvent::AgentChooserNavigateDown),
        KeyCode::Enter => Some(AppEvent::AgentChooserConfirm),
        KeyCode::Esc => Some(AppEvent::AgentChooserCancel),
        _ => None,
    }
}

/// Route key events when the delete confirm overlay is open.
/// Enter confirms (arms or dispatches), Esc cancels, everything else is consumed.
fn resolve_delete_confirm_key_event(key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Enter => Some(AppEvent::IssueDeleteConfirm),
        KeyCode::Esc => Some(AppEvent::IssueDeleteCancel),
        _ => None,
    }
}

fn resolve_search_key_event(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Enter => Some(AppEvent::ApplySearch),
        KeyCode::Esc if state.issues_state.search_query.is_empty() => {
            Some(AppEvent::BlurSearchInput)
        }
        KeyCode::Esc => Some(AppEvent::ClearSearch),
        KeyCode::Char(c) => {
            let mut query = state.issues_state.search_query.clone();
            query.push(c);
            Some(AppEvent::SetSearchQuery { query })
        }
        KeyCode::Backspace => {
            let mut query = state.issues_state.search_query.clone();
            query.pop();
            Some(AppEvent::SetSearchQuery { query })
        }
        _ => None,
    }
}

fn resolve_global_issues_key_event(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Esc if state.issues_state.issue_focus == IssueFocus::IssueDetail => {
            Some(AppEvent::RefocusIssueList)
        }
        KeyCode::Char('a') | KeyCode::Esc => Some(AppEvent::ExitIssuesMode),
        KeyCode::Char('i') => Some(AppEvent::RefocusIssueList),
        KeyCode::Char('?' | 'h') | KeyCode::F(1) => Some(AppEvent::OpenHelp),
        _ => None,
    }
}

fn resolve_focus_key_event(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match state.issues_state.issue_focus {
        IssueFocus::IssueList => resolve_issue_list_key_event(key_event),
        IssueFocus::IssueDetail => resolve_issue_detail_key_event(state, key_event),
        IssueFocus::RepoList => resolve_repo_list_key_event(key_event),
    }
}

fn resolve_issue_list_key_event(key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Up => Some(AppEvent::IssuesNavigateUp),
        KeyCode::Down => Some(AppEvent::IssuesNavigateDown),
        KeyCode::Left => Some(AppEvent::IssuesCycleFocusReverse),
        KeyCode::Right => Some(AppEvent::IssuesCycleFocus),
        KeyCode::PageUp => Some(AppEvent::IssuesNavigatePageUp),
        KeyCode::PageDown => Some(AppEvent::IssuesNavigatePageDown),
        KeyCode::Home => Some(AppEvent::IssuesNavigateHome),
        KeyCode::End => Some(AppEvent::IssuesNavigateEnd),
        KeyCode::Enter => Some(AppEvent::IssuesEnter),
        KeyCode::Char('n' | 'N') => Some(AppEvent::OpenNewIssueComposer),
        KeyCode::Char('f') => Some(AppEvent::OpenFilterControls),
        KeyCode::Char('/') => Some(AppEvent::FocusSearchInput),
        KeyCode::Char('C') => Some(AppEvent::CloseIssue),
        KeyCode::Char('D') => Some(AppEvent::OpenDeleteIssueConfirm),
        _ => None,
    }
}

fn resolve_issue_detail_key_event(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Up => Some(AppEvent::IssuesScrollDetailUp),
        KeyCode::Down => Some(AppEvent::IssuesScrollDetailDown),
        KeyCode::Left => Some(AppEvent::IssuesCycleFocusReverse),
        KeyCode::Right => Some(AppEvent::IssuesCycleFocus),
        KeyCode::PageUp => Some(AppEvent::IssuesScrollDetailPageUp),
        KeyCode::PageDown => Some(AppEvent::IssuesScrollDetailPageDown),
        KeyCode::Char('e') => editor_event_for_subfocus(state.issues_state.detail_subfocus),
        KeyCode::Char('c') => Some(AppEvent::OpenNewCommentComposer),
        KeyCode::Char('r') => reply_event_for_subfocus(state.issues_state.detail_subfocus),
        KeyCode::Char('S') if !state.agents.is_empty() => Some(AppEvent::OpenAgentChooser),
        KeyCode::Char('C') => Some(AppEvent::CloseIssue),
        KeyCode::Char('D') => Some(AppEvent::OpenDeleteIssueConfirm),
        KeyCode::Tab | KeyCode::Char('j') => Some(AppEvent::IssueDetailSubfocusNext),
        KeyCode::BackTab | KeyCode::Char('k') => Some(AppEvent::IssueDetailSubfocusPrev),
        _ => None,
    }
}

fn editor_event_for_subfocus(subfocus: DetailSubfocus) -> Option<AppEvent> {
    match subfocus {
        DetailSubfocus::Body => Some(AppEvent::OpenInlineEditor {
            target: jefe::state::EditorTarget::IssueBody,
        }),
        DetailSubfocus::Comment(idx) => Some(AppEvent::OpenInlineEditor {
            target: jefe::state::EditorTarget::Comment { comment_index: idx },
        }),
        DetailSubfocus::NewComment => None,
    }
}

fn reply_event_for_subfocus(subfocus: DetailSubfocus) -> Option<AppEvent> {
    match subfocus {
        DetailSubfocus::Comment(idx) => Some(AppEvent::OpenReplyComposer { comment_index: idx }),
        _ => None,
    }
}

fn resolve_repo_list_key_event(key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Up => Some(AppEvent::IssuesNavigateUp),
        KeyCode::Down => Some(AppEvent::IssuesNavigateDown),
        KeyCode::Left => Some(AppEvent::IssuesCycleFocusReverse),
        KeyCode::Right => Some(AppEvent::IssuesCycleFocus),
        _ => None,
    }
}

fn resolve_pane_cycle_key_event(key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Tab => Some(AppEvent::IssuesCycleFocus),
        KeyCode::BackTab => Some(AppEvent::IssuesCycleFocusReverse),
        _ => None,
    }
}

/// Route key events when in Issues Mode.
///
/// @plan PLAN-20260329-ISSUES-MODE.P09
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-002
/// @pseudocode component-003 lines 01-38
pub fn handle_issues_mode_key(
    app_state: &AppStateHandle,
    _ctx: &SharedContext,
    key_event: &KeyEvent,
) -> Option<AppEvent> {
    let state_ro = app_state.read();
    let result = resolve_issues_key_event(&state_ro, key_event);
    drop(state_ro);
    result
}

#[cfg(test)]
#[path = "issues_key_tests.rs"]
mod tests;
