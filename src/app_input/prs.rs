//! PR-mode key event dispatch.
//!
//! Pure key-routing logic for PR Mode. Implements the 8-level precedence
//! skeleton from pseudocode component-003. Every sub-handler returns
//! `Option<AppEvent>`; `None` means the key is suppressed/consumed.
//! The public entry point `handle_prs_mode_key` mirrors
//! `issues::handle_issues_mode_key`.
//!
//! @plan PLAN-20260624-PR-MODE.P11
//! @requirement REQ-PR-001
//! @requirement REQ-PR-002
//! @requirement REQ-PR-003
//! @requirement REQ-PR-004
//! @requirement REQ-PR-010
//! @requirement REQ-PR-011
//! @requirement REQ-PR-012
//! @requirement REQ-PR-013
//! @pseudocode component-003 lines 01-14

use iocraft::prelude::*;

use jefe::state::{
    AppEvent, AppState, InlineState, PrDetailSubfocus, PrFocus, PrPropertyKind, ReadOnlyHintKind,
};

use super::{AppStateHandle, SharedContext};

/// Pure key-routing logic for PR Mode.
///
/// Implements the 8-level precedence chain from pseudocode component-003.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-001
/// @requirement REQ-PR-002
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 10-48
pub(super) fn resolve_prs_key_event(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    // P1: inline composer (direct enum sentinel, NOT Option)
    if state.prs_state.inline_state != InlineState::None {
        return handle_pr_inline_key(state, key_event);
    }
    // P2: agent chooser
    if state.prs_state.agent_chooser.is_some() {
        return handle_pr_agent_chooser_key(state, key_event);
    }
    // P2.5: merge chooser (issue #92)
    if state.prs_state.merge_chooser.is_some() {
        return handle_pr_merge_chooser_key(state, key_event);
    }
    // P2.6: property editor (issue #175) — checked after merge chooser but
    // before search/filter so the overlay is fully modal.
    if state.prs_state.property_editor.is_some() {
        return handle_pr_property_editor_key(state, key_event);
    }
    // P3: search input
    if state.prs_state.search_input_focused {
        return handle_pr_search_input_key(state, key_event);
    }
    // P4: filter controls
    if state.prs_state.filter_ui.controls_open {
        return super::prs_filter::handle_pr_filter_controls_key(state, key_event);
    }
    // P5-P7: global keys, focus-domain handlers, pane cycle.
    //
    // Reserved dashboard keys (`s`, `Ctrl-d`, `Ctrl-k`, `l`) are consumed as
    // no-ops simply by NOT matching any handler here: they resolve to `None`,
    // and the outer `handle_dashboard_prs_key` wrapper marks every PR-mode key
    // `Handled`, so the `None` is a terminal consume that never leaks to the
    // dashboard. No explicit suppression tier is required.
    resolve_pr_global_key(state, key_event)
        .or_else(|| resolve_pr_focus_key(state, key_event))
        .or_else(|| resolve_pr_pane_cycle_key(key_event))
}

/// Route key events when in PR Mode.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-001
/// @requirement REQ-PR-002
/// @pseudocode component-003 lines 10-14
pub fn handle_prs_mode_key(
    app_state: &AppStateHandle,
    _ctx: &SharedContext,
    key_event: &KeyEvent,
) -> Option<AppEvent> {
    let state_ro = app_state.read();
    let result = resolve_prs_key_event(&state_ro, key_event);
    drop(state_ro);
    result
}

/// P5 global-key resolver for PR Mode.
///
/// Handles mode-level keys that apply regardless of pane focus: `a`/`Esc`
/// exit the mode, `p`/`P` refocus the PR list (NOT a re-entry — the mode is
/// already active). `o` lives in the P6 list/detail handlers, NOT here.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-001
/// @requirement REQ-PR-002
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 23-30
fn resolve_pr_global_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Esc => Some(handle_esc_in_prs_mode(state, key_event)),
        KeyCode::Char('a') => Some(AppEvent::ExitPrsMode),
        KeyCode::Char('p' | 'P') => Some(AppEvent::RefocusPrList),
        KeyCode::Char('f') => Some(AppEvent::PrOpenFilterControls),
        // Cross-mode navigation: `i` from PRs switches to Issues mode (issue #164).
        KeyCode::Char('i' | 'I') => Some(AppEvent::EnterIssuesMode),
        // F12 defocuses the terminal or returns to the PR list (issue #164).
        KeyCode::F(12) => f12_event_for_prs(state),
        _ => None,
    }
}

/// F12 semantics in PR mode (issue #164): defocus the terminal if it is
/// focused, otherwise return to the PR list from the detail view. A no-op
/// (returns `None`) when already at the PR list with the terminal unfocused.
fn f12_event_for_prs(state: &AppState) -> Option<AppEvent> {
    if state.terminal_focused {
        Some(AppEvent::ToggleTerminalFocus)
    } else if state.prs_state.pr_focus == PrFocus::PrDetail {
        Some(AppEvent::RefocusPrList)
    } else {
        None
    }
}

/// P6 focus-domain resolver for PR Mode.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-003
/// @pseudocode component-003 lines 31-36
fn resolve_pr_focus_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match state.prs_state.pr_focus {
        PrFocus::RepoList => handle_pr_repo_key(state, key_event),
        PrFocus::PrList => handle_pr_list_key(state, key_event),
        PrFocus::PrDetail => handle_pr_detail_key(state, key_event),
    }
}

/// P7 pane-cycle resolver for PR Mode.
///
/// Tab cycles focus forward, Shift+Tab cycles reverse (issue #46). These are
/// in a dedicated tier (after focus-domain handlers) so the detail-subfocus
/// `j`/`k` and list/detail navigation take priority within their pane. The PR
/// DETAIL pane intercepts Tab/BackTab for detail subfocus in the P6 focus
/// tier (issue #150), so this P7 fallback only applies to RepoList/PrList.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-003
/// @pseudocode component-003 lines 37-42
fn resolve_pr_pane_cycle_key(key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Tab => Some(AppEvent::PrCycleFocus),
        KeyCode::BackTab => Some(AppEvent::PrCycleFocusReverse),
        _ => None,
    }
}

/// Handle keys while the RepoList pane is focused.
///
/// Up/Down navigate the repository selection (repo nav is independent of
/// pane_focus — issue #47; it reuses the shared `PrNavigateUp`/`Down` events
/// that the issues repo handler also emits). Left/Right cycle panes
/// (issue #150: Left/Right symmetric pane-focus in every pane — mirrors
/// `resolve_repo_list_key_event` arrow handling). Tab/BackTab fall through to
/// the P7 pane-cycle tier.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-003
/// @pseudocode component-003 lines 49-56
fn handle_pr_repo_key(_state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Up => Some(AppEvent::PrNavigateUp),
        KeyCode::Down => Some(AppEvent::PrNavigateDown),
        KeyCode::Left => Some(AppEvent::PrCycleFocusReverse),
        KeyCode::Right => Some(AppEvent::PrCycleFocus),
        _ => None,
    }
}

/// Handle keys while the PrList pane is focused.
///
/// Up/Down/PageUp/PageDown/Home/End navigate the list; Left/Right cycle panes
/// (mirror `resolve_issue_list_key_event` arrow handling); Enter opens detail;
/// `o` opens the selected PR in the browser (or a no-selection notice).
/// Tab/BackTab are NOT handled here — they fall through to the P7 pane-cycle
/// tier.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-003
/// @requirement REQ-PR-012
/// @pseudocode component-003 lines 57-70
fn handle_pr_list_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Up => Some(AppEvent::PrNavigateUp),
        KeyCode::Down => Some(AppEvent::PrNavigateDown),
        KeyCode::Left => Some(AppEvent::PrCycleFocusReverse),
        KeyCode::Right => Some(AppEvent::PrCycleFocus),
        KeyCode::PageUp => Some(AppEvent::PrNavigatePageUp),
        KeyCode::PageDown => Some(AppEvent::PrNavigatePageDown),
        KeyCode::Home => Some(AppEvent::PrNavigateHome),
        KeyCode::End => Some(AppEvent::PrNavigateEnd),
        KeyCode::Enter => Some(AppEvent::PrListEnter),
        KeyCode::Char('o') => Some(pr_open_in_browser_or_notice(selected_pr_present(state))),
        _ => None,
    }
}

/// Handle keys while the PrDetail pane is focused.
///
/// Scroll Up/Down/PageUp/PageDown scroll the detail viewport; Tab/BackTab
/// cycle detail subfocus (issue #150: Tab owns subfocus cycling within the
/// focused pane) with `j`/`k` as vim aliases; `c` opens the comment composer
/// from comment-eligible subfocus (Body/Comment/NewComment) and surfaces a
/// read-only notice on Review/Check; `r` replies on Comment subfocus and
/// surfaces a notice elsewhere; `e` is read-only everywhere; `S` opens the
/// agent chooser; `o` opens the loaded PR in the browser. Left/Right cycle
/// panes (issue #150: symmetric pane-focus in every pane).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-003
/// @requirement REQ-PR-010
/// @requirement REQ-PR-011
/// @requirement REQ-PR-012
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 72-91
fn handle_pr_detail_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Up => Some(AppEvent::PrScrollDetailUp),
        KeyCode::Down => Some(AppEvent::PrScrollDetailDown),
        KeyCode::PageUp => Some(AppEvent::PrScrollDetailPageUp),
        KeyCode::PageDown => Some(AppEvent::PrScrollDetailPageDown),
        KeyCode::Left => Some(AppEvent::PrCycleFocusReverse),
        KeyCode::Right => Some(AppEvent::PrCycleFocus),
        KeyCode::Tab | KeyCode::Char('j') => Some(AppEvent::PrDetailSubfocusNext),
        KeyCode::BackTab | KeyCode::Char('k') => Some(AppEvent::PrDetailSubfocusPrev),
        KeyCode::Char('c') => Some(comment_event_for_subfocus(state.prs_state.detail_subfocus)),
        KeyCode::Char('r') => Some(reply_event_for_subfocus(state.prs_state.detail_subfocus)),
        KeyCode::Char('R') => Some(resolve_event_for_subfocus(state.prs_state.detail_subfocus)),
        KeyCode::Char('e') => Some(AppEvent::PrShowNotice(
            ReadOnlyHintKind::ReadOnlyNotEditable,
        )),
        KeyCode::Char('S') => Some(AppEvent::PrOpenAgentChooser),
        KeyCode::Char('o') => Some(pr_open_in_browser_or_notice(pr_detail_present(state))),
        KeyCode::Char('m') => Some(pr_merge_event_for_detail(state)),
        _ => resolve_pr_property_open_key(state, key_event),
    }
}

/// Property editor open-key shortcuts for PRs (issue #175).
///
/// Shift-letter opens the corresponding property editor overlay. Only active
/// on Body subfocus and when no overlay is already open. PR kinds: Labels,
/// Assignees, Milestone, Title, State (no Type).
fn resolve_pr_property_open_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    if state.prs_state.detail_subfocus != PrDetailSubfocus::Body
        || state.prs_state.property_editor.is_some()
    {
        return None;
    }
    let kind = match key_event.code {
        KeyCode::Char('L') => PrPropertyKind::Labels,
        KeyCode::Char('A') => PrPropertyKind::Assignees,
        KeyCode::Char('M') => PrPropertyKind::Milestone,
        KeyCode::Char('T') => PrPropertyKind::Title,
        KeyCode::Char('W') => PrPropertyKind::State,
        _ => return None,
    };
    Some(AppEvent::PrOpenPropertyEditor { kind })
}

/// Map `c` to the composer-open event for comment-eligible subfocus, or to a
/// read-only notice on Review/Check subfocus (reviews and checks are
/// read-only).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 83-89
fn comment_event_for_subfocus(subfocus: PrDetailSubfocus) -> AppEvent {
    match subfocus {
        PrDetailSubfocus::Body | PrDetailSubfocus::Comment(_) | PrDetailSubfocus::NewComment => {
            AppEvent::PrOpenNewCommentComposer
        }
        PrDetailSubfocus::Review(_)
        | PrDetailSubfocus::ReviewThread(_)
        | PrDetailSubfocus::Check(_) => AppEvent::PrShowNotice(ReadOnlyHintKind::ReadOnlyNoComment),
    }
}

/// Map `r` to the reply-composer event on Comment subfocus, or to a read-only
/// notice elsewhere (reply is only valid on a comment).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 86-87
fn reply_event_for_subfocus(subfocus: PrDetailSubfocus) -> AppEvent {
    match subfocus {
        PrDetailSubfocus::Comment(idx) => AppEvent::PrOpenReplyComposer { comment_index: idx },
        PrDetailSubfocus::ReviewThread(idx) => {
            AppEvent::PrOpenThreadReplyComposer { thread_index: idx }
        }
        _ => AppEvent::PrShowNotice(ReadOnlyHintKind::ReadOnlyReplyOnComment),
    }
}

/// Map `R` to the resolve/unresolve thread event on ReviewThread subfocus, or
/// to a read-only notice elsewhere (resolve is only valid on a review thread).
///
/// @requirement REQ-PR-009
fn resolve_event_for_subfocus(subfocus: PrDetailSubfocus) -> AppEvent {
    match subfocus {
        PrDetailSubfocus::ReviewThread(idx) => {
            AppEvent::PrToggleThreadResolve { thread_index: idx }
        }
        _ => AppEvent::PrShowNotice(ReadOnlyHintKind::ReadOnlyResolveOnThread),
    }
}

/// Map `o` to the open-in-browser event when a PR target is present, else to a
/// non-blocking `NoSelectionToOpen` notice (consume + hint, never a silent drop).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-012
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 68-69
fn pr_open_in_browser_or_notice(target_present: bool) -> AppEvent {
    if target_present {
        AppEvent::PrOpenInBrowser
    } else {
        AppEvent::PrShowNotice(ReadOnlyHintKind::NoSelectionToOpen)
    }
}

/// Whether a PR is currently selected in the list (REQ-PR-012 presence check).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-012
/// @pseudocode component-003 lines 68-69
fn selected_pr_present(state: &AppState) -> bool {
    state.prs_state.selected_pr_index().is_some()
}

/// Whether a loaded PR detail is present (REQ-PR-012 presence check).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-012
/// @pseudocode component-003 lines 88-89
fn pr_detail_present(state: &AppState) -> bool {
    state.prs_state.pr_detail.is_some()
}

/// Map `m` to the merge-chooser-open event when a loaded open PR is present,
/// else to a non-blocking notice (issue #92).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
fn pr_merge_event_for_detail(state: &AppState) -> AppEvent {
    if state.prs_state.pr_detail.is_none() {
        return AppEvent::PrShowNotice(ReadOnlyHintKind::NoPrToMerge);
    }
    if let Some(detail) = &state.prs_state.pr_detail
        && detail.state != jefe::domain::PrState::Open
    {
        return AppEvent::PrShowNotice(ReadOnlyHintKind::PrNotMergeable);
    }
    AppEvent::PrOpenMergeChooser
}

/// Handle the Esc key in PR Mode by unwinding the active overlay.
///
/// Precedence: inline composer → agent chooser → search (clear query if
/// nonempty, else blur) → filter controls → exit mode.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98
fn handle_esc_in_prs_mode(state: &AppState, _key_event: &KeyEvent) -> AppEvent {
    if state.prs_state.inline_state != InlineState::None {
        return AppEvent::PrInlineCancelOrEsc;
    }
    if state.prs_state.agent_chooser.is_some() {
        return AppEvent::PrAgentChooserCancel;
    }
    // Property editor (issue #175): Esc closes the editor before exiting
    // the mode. This is a safety net — the P2.6 tier normally intercepts
    // Esc before the global handler is reached.
    if state.prs_state.property_editor.is_some() {
        return AppEvent::PrPropertyEditorCancel;
    }
    if state.prs_state.search_input_focused {
        if state.prs_state.search_query.is_empty() {
            return AppEvent::PrBlurSearchInput;
        }
        return AppEvent::PrClearSearch;
    }
    if state.prs_state.filter_ui.controls_open {
        return AppEvent::PrCloseFilterControls;
    }
    // No overlay active: if the PrDetail pane is focused, Esc refocuses the
    // PR list instead of exiting the whole mode — mirroring issues-mode where
    // Esc on IssueDetail emits RefocusIssueList. Only a bare Esc from
    // RepoList/PrList exits the mode.
    if state.prs_state.pr_focus == PrFocus::PrDetail {
        return AppEvent::RefocusPrList;
    }
    AppEvent::ExitPrsMode
}

/// Handle keys while an inline composer/editor is active.
///
/// Mirrors the issues inline key router: Esc cancels, Ctrl+Enter submits,
/// Enter inserts a newline, chars/backspace/delete/cursor keys edit.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-003 lines 99-108
fn handle_pr_inline_key(_state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Esc => Some(AppEvent::PrInlineCancelOrEsc),
        KeyCode::Enter if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(AppEvent::PrInlineSubmit)
        }
        KeyCode::Enter => Some(AppEvent::PrInlineNewline),
        KeyCode::Char(c) => Some(AppEvent::PrInlineChar(c)),
        KeyCode::Backspace => Some(AppEvent::PrInlineBackspace),
        KeyCode::Delete => Some(AppEvent::PrInlineDelete),
        KeyCode::Left => Some(AppEvent::PrInlineCursorLeft),
        KeyCode::Right => Some(AppEvent::PrInlineCursorRight),
        KeyCode::Up => Some(AppEvent::PrInlineCursorUp),
        KeyCode::Down => Some(AppEvent::PrInlineCursorDown),
        _ => None,
    }
}

/// Handle keys while the agent chooser is open.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 120-126
fn handle_pr_agent_chooser_key(_state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Up => Some(AppEvent::PrAgentChooserNavigateUp),
        KeyCode::Down => Some(AppEvent::PrAgentChooserNavigateDown),
        KeyCode::Enter => Some(AppEvent::PrAgentChooserConfirm),
        KeyCode::Esc => Some(AppEvent::PrAgentChooserCancel),
        _ => None,
    }
}

/// Handle keys while the merge chooser is open (issue #92).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
fn handle_pr_merge_chooser_key(_state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Up => Some(AppEvent::PrMergeNavigateUp),
        KeyCode::Down => Some(AppEvent::PrMergeNavigateDown),
        KeyCode::Enter => Some(AppEvent::PrMergeConfirm),
        KeyCode::Esc => Some(AppEvent::PrMergeCancel),
        _ => None,
    }
}

/// Handle keys while the property editor is open (issue #175).
///
/// Mirrors the merge-chooser key router: Up/Down navigate, Space toggles,
/// Enter confirms, Esc cancels. Title editing keys (char, backspace, delete,
/// cursor left/right) are also routed. All other keys are consumed as `None`.
fn handle_pr_property_editor_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    let kind = state.prs_state.property_editor.as_ref()?.kind;
    match key_event.code {
        KeyCode::Up => Some(AppEvent::PrPropertyEditorNavigateUp),
        KeyCode::Down => Some(AppEvent::PrPropertyEditorNavigateDown),
        KeyCode::Char(' ') if kind != PrPropertyKind::Title => {
            Some(AppEvent::PrPropertyEditorToggle)
        }
        KeyCode::Enter => Some(AppEvent::PrPropertyEditorConfirm),
        KeyCode::Esc => Some(AppEvent::PrPropertyEditorCancel),
        KeyCode::Char(c) if !key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(AppEvent::PrPropertyEditorTitleChar(c))
        }
        KeyCode::Backspace => Some(AppEvent::PrPropertyEditorTitleBackspace),
        KeyCode::Delete => Some(AppEvent::PrPropertyEditorTitleDelete),
        KeyCode::Left => Some(AppEvent::PrPropertyEditorTitleCursorLeft),
        KeyCode::Right => Some(AppEvent::PrPropertyEditorTitleCursorRight),
        _ => None,
    }
}

/// Handle keys while the search input is focused.
///
/// Routes chars to the query (PrSetSearchQuery), Enter to apply, Esc to
/// clear/blur, and Backspace to pop the last char — mirroring the issues
/// search-input router.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-002
/// @requirement REQ-PR-008
/// @pseudocode component-003 lines 127-133
fn handle_pr_search_input_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Enter => Some(AppEvent::PrApplySearch),
        KeyCode::Esc if state.prs_state.search_query.is_empty() => {
            Some(AppEvent::PrBlurSearchInput)
        }
        KeyCode::Esc => Some(AppEvent::PrClearSearch),
        KeyCode::Char(c) => {
            let mut query = state.prs_state.search_query.clone();
            query.push(c);
            Some(AppEvent::PrSetSearchQuery { query })
        }
        KeyCode::Backspace => {
            let mut query = state.prs_state.search_query.clone();
            query.pop();
            Some(AppEvent::PrSetSearchQuery { query })
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "prs_key_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "prs_property_key_tests.rs"]
mod prs_property_key_tests;
