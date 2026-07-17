//! Issues-mode key event dispatch.
//!
//! @plan PLAN-20260329-ISSUES-MODE.P09
//! @plan PLAN-20260329-ISSUES-MODE.P10
//! @plan PLAN-20260329-ISSUES-MODE.P11
//! @requirement REQ-ISS-002
//! @pseudocode component-003 lines 01-38

use iocraft::prelude::*;

use jefe::state::{AppEvent, AppState, DetailSubfocus, InlineState, IssueFocus, IssuePropertyKind};

use super::issues_filter::resolve_filter_key_event;
use super::list_navigation::issues_page_item_count;

use super::{AppStateHandle, SharedContext};

/// Pure key-routing logic for Issues Mode.
///
/// Given the current application state and a key event, returns the `AppEvent`
/// that should be dispatched — or `None` if the key is suppressed/no-op.
///
/// This function is side-effect-free and testable without iocraft hooks.
/// It uses the standard fallback terminal height; the live input boundary calls
/// the row-aware resolver with the current terminal height.
/// Implements the 8-level priority chain from pseudocode component-003.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-002
/// @pseudocode component-003 lines 01-38
#[cfg(test)]
pub fn resolve_issues_key_event(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    resolve_issues_key_event_for_size(state, key_event, 120, 40)
}

#[cfg(test)]
fn resolve_issues_key_event_for_rows(
    state: &AppState,
    key_event: &KeyEvent,
    terminal_rows: u16,
) -> Option<AppEvent> {
    resolve_issues_key_event_for_size(state, key_event, 120, terminal_rows)
}

#[must_use]
fn resolve_issues_key_event_for_size(
    state: &AppState,
    key_event: &KeyEvent,
    terminal_cols: u16,
    terminal_rows: u16,
) -> Option<AppEvent> {
    let page_item_count = issues_page_item_count(state, terminal_cols, terminal_rows);
    if let Some(editor) = state.issues_state.property_editor.as_ref() {
        return resolve_property_editor_key_event(editor.kind, key_event);
    }

    if state.issues_state.close_reason_chooser.is_some() {
        return resolve_close_reason_chooser_key_event(state, key_event);
    }

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
        .or_else(|| resolve_focus_key_event(state, key_event, page_item_count))
        .or_else(|| resolve_pane_cycle_key_event(key_event))
}

fn resolve_inline_key_event(key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(AppEvent::InlineCancelOrEsc)
        }
        KeyCode::Esc => Some(AppEvent::InlineCancelOrEsc),
        // Alt+Enter is the advertised terminal-portable submit key (issue #265).
        // Ctrl+Enter remains accepted for terminals that encode it distinctly.
        KeyCode::Enter
            if key_event.modifiers.contains(KeyModifiers::ALT)
                || key_event.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(AppEvent::InlineSubmit)
        }
        // Ctrl+R asks the configured default agent to rewrite the current
        // new-issue composer draft non-interactively (issue #214).
        KeyCode::Char('r') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(AppEvent::RequestIssueRewrite)
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

/// Property-editor key router (issue #175).
///
/// Mirrors the merge-chooser pattern: Up/Down navigate, Space toggles, Enter
/// confirms, Esc cancels. When `kind == Title`, character input, Backspace,
/// Delete, and Left/Right edit the title text. All other keys are consumed
/// as `None` so the overlay is modal.
fn resolve_property_editor_key_event(
    kind: IssuePropertyKind,
    key_event: &KeyEvent,
) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Up => Some(AppEvent::IssuePropertyEditorNavigateUp),
        KeyCode::Down => Some(AppEvent::IssuePropertyEditorNavigateDown),
        KeyCode::Char(' ') if kind != IssuePropertyKind::Title => {
            Some(AppEvent::IssuePropertyEditorToggle)
        }
        KeyCode::Enter => Some(AppEvent::IssuePropertyEditorConfirm),
        KeyCode::Esc => Some(AppEvent::IssuePropertyEditorCancel),
        KeyCode::Char(c) if !key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(AppEvent::IssuePropertyEditorTitleChar(c))
        }
        KeyCode::Backspace => Some(AppEvent::IssuePropertyEditorTitleBackspace),
        KeyCode::Delete => Some(AppEvent::IssuePropertyEditorTitleDelete),
        KeyCode::Left => Some(AppEvent::IssuePropertyEditorTitleCursorLeft),
        KeyCode::Right => Some(AppEvent::IssuePropertyEditorTitleCursorRight),
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

/// Route key events when the close-reason chooser overlay is open.
///
/// When `duplicate_search` is active (Duplicate reason selected), digits
/// update the search query, Backspace deletes, Up/Down navigate candidates,
/// and Enter confirms the duplicate selection. Otherwise, Up/Down navigate
/// the reason list, Enter selects/confirms, and Esc cancels.
fn resolve_close_reason_chooser_key_event(
    state: &AppState,
    key_event: &KeyEvent,
) -> Option<AppEvent> {
    let chooser = state.issues_state.close_reason_chooser.as_ref()?;
    if chooser.duplicate_search.is_some() {
        return match key_event.code {
            KeyCode::Char(c) if c.is_ascii_digit() => {
                Some(AppEvent::CloseReasonDuplicateSearchChar(c))
            }
            KeyCode::Backspace => Some(AppEvent::CloseReasonDuplicateSearchBackspace),
            KeyCode::Up => Some(AppEvent::CloseReasonDuplicateSearchNavigateUp),
            KeyCode::Down => Some(AppEvent::CloseReasonDuplicateSearchNavigateDown),
            KeyCode::Enter => Some(AppEvent::CloseReasonConfirm),
            KeyCode::Esc => Some(AppEvent::CloseReasonCancel),
            _ => None,
        };
    }
    if chooser.awaiting_confirmation {
        return match key_event.code {
            KeyCode::Enter => Some(AppEvent::CloseReasonConfirm),
            KeyCode::Esc => Some(AppEvent::CloseReasonCancel),
            _ => None,
        };
    }
    match key_event.code {
        KeyCode::Up => Some(AppEvent::CloseReasonNavigateUp),
        KeyCode::Down => Some(AppEvent::CloseReasonNavigateDown),
        KeyCode::Enter => Some(AppEvent::CloseReasonSelect),
        KeyCode::Esc => Some(AppEvent::CloseReasonCancel),
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
        // Cross-mode navigation: `p` from Issues switches to PR mode (issue #164).
        KeyCode::Char('p') => Some(AppEvent::EnterPrsMode),
        // F12 defocuses the terminal or returns to the issue list (issue #164).
        KeyCode::F(12) => f12_event_for_issues(state),
        KeyCode::Char('?' | 'h') | KeyCode::F(1) => Some(AppEvent::OpenHelp),
        _ => None,
    }
}

/// F12 semantics in Issues mode (issue #164): defocus the terminal if it is
/// focused, otherwise return to the issue list from the detail view. A no-op
/// (returns `None`) when already at the issue list with the terminal
/// unfocused.
fn f12_event_for_issues(state: &AppState) -> Option<AppEvent> {
    if state.terminal_focused {
        Some(AppEvent::ToggleTerminalFocus)
    } else if state.issues_state.issue_focus == IssueFocus::IssueDetail {
        Some(AppEvent::RefocusIssueList)
    } else {
        None
    }
}

fn resolve_focus_key_event(
    state: &AppState,
    key_event: &KeyEvent,
    page_item_count: jefe::list_viewport::PageItemCount,
) -> Option<AppEvent> {
    match state.issues_state.issue_focus {
        IssueFocus::IssueList => resolve_issue_list_key_event(key_event, page_item_count),
        IssueFocus::IssueDetail => resolve_issue_detail_key_event(state, key_event),
        IssueFocus::RepoList => resolve_repo_list_key_event(key_event),
    }
}

fn resolve_issue_list_key_event(
    key_event: &KeyEvent,
    page_item_count: jefe::list_viewport::PageItemCount,
) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Up => Some(AppEvent::IssuesNavigateUp),
        KeyCode::Down => Some(AppEvent::IssuesNavigateDown),
        KeyCode::Left => Some(AppEvent::IssuesCycleFocusReverse),
        KeyCode::Right => Some(AppEvent::IssuesCycleFocus),
        KeyCode::PageUp => Some(AppEvent::IssuesNavigatePageUp(page_item_count)),
        KeyCode::PageDown => Some(AppEvent::IssuesNavigatePageDown(page_item_count)),
        KeyCode::Home => Some(AppEvent::IssuesNavigateHome),
        KeyCode::End => Some(AppEvent::IssuesNavigateEnd),
        KeyCode::Enter => Some(AppEvent::IssuesEnter),
        KeyCode::Char('n' | 'N') => Some(AppEvent::OpenNewIssueComposer),
        KeyCode::Char('f') => Some(AppEvent::OpenFilterControls),
        KeyCode::Char('/') => Some(AppEvent::FocusSearchInput),
        KeyCode::Char('C') => Some(AppEvent::OpenCloseReasonChooser),
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
        // S always expresses the Send-to-Agent intent; the app_input layer
        // builds typed chooser entries (including dirty status via
        // GitRepoInfo::resolve), and the reducer validates/opens state.
        KeyCode::Char('S') => Some(AppEvent::OpenAgentChooser {
            metadata: super::build_chooser_metadata(state),
        }),
        KeyCode::Char('C') => Some(AppEvent::OpenCloseReasonChooser),
        KeyCode::Char('D') => Some(AppEvent::OpenDeleteIssueConfirm),
        KeyCode::Tab | KeyCode::Char('j') => Some(AppEvent::IssueDetailSubfocusNext),
        KeyCode::BackTab | KeyCode::Char('k') => Some(AppEvent::IssueDetailSubfocusPrev),
        _ => resolve_issue_property_open_key(state, key_event),
    }
}

/// Property editor open-key shortcuts (issue #175).
///
/// Shift-letter opens the corresponding property editor overlay. Only active
/// on Body subfocus and when no overlay is already open.
fn resolve_issue_property_open_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    if state.issues_state.detail_subfocus != DetailSubfocus::Body
        || state.issues_state.property_editor.is_some()
        || state.issues_state.close_reason_chooser.is_some()
        || state.issues_state.delete_confirm.is_some()
    {
        return None;
    }
    let kind = match key_event.code {
        KeyCode::Char('L') => IssuePropertyKind::Labels,
        KeyCode::Char('A') => IssuePropertyKind::Assignees,
        KeyCode::Char('M') => IssuePropertyKind::Milestone,
        KeyCode::Char('T') => IssuePropertyKind::Title,
        KeyCode::Char('Y') => IssuePropertyKind::Type,
        KeyCode::Char('W') => IssuePropertyKind::State,
        _ => return None,
    };
    Some(AppEvent::IssueOpenPropertyEditor { kind })
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
    let (terminal_cols, terminal_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let result =
        resolve_issues_key_event_for_size(&state_ro, key_event, terminal_cols, terminal_rows);
    drop(state_ro);
    result
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
