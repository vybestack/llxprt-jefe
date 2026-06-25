//! PR-mode key event dispatch (stub surface).
//!
//! Compiling, panic-free stubs that establish the key-routing surface for
//! PR Mode. Every sub-handler returns `Option<AppEvent>` (`None` here); real
//! behavior is filled in by the P10 RED -> P11 GREEN cycle. The public entry
//! point `handle_prs_mode_key` mirrors `issues::handle_issues_mode_key`.
//!
//! @plan PLAN-20260624-PR-MODE.P09
//! @requirement REQ-PR-001
//! @requirement REQ-PR-002
//! @requirement REQ-PR-003
//! @requirement REQ-PR-004
//! @requirement REQ-PR-012
//! @pseudocode component-003 lines 01-14

use iocraft::prelude::*;

use jefe::state::{AppEvent, AppState, InlineState, PrFocus, ReadOnlyHintKind};

use super::{AppStateHandle, SharedContext};

/// Pure key-routing logic for PR Mode (stub).
///
/// Implements the 8-level precedence skeleton from pseudocode component-003.
/// Returns `None` for every key until P11 fills in the real arms.
///
/// @plan PLAN-20260624-PR-MODE.P09
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
    // P3: search input
    if state.prs_state.search_input_focused {
        return handle_pr_search_input_key(state, key_event);
    }
    // P4: filter controls
    if state.prs_state.filter_ui.controls_open {
        return super::prs_filter::handle_pr_filter_controls_key(state, key_event);
    }
    // P5-P8: global keys, focus-domain handlers, pane cycle, suppression
    resolve_pr_global_key(state, key_event)
        .or_else(|| resolve_pr_focus_key(state, key_event))
        .or_else(|| resolve_pr_pane_cycle_key(key_event))
        .or_else(|| resolve_pr_suppressed_key(key_event))
}

/// P8 suppression tier for PR Mode: reserved dashboard keys (`s`, `Ctrl-d`,
/// `Ctrl-k`, `l`) are CONSUMED as no-ops so they never leak to the dashboard.
///
/// Returns `None` (consumed-no-op) for the reserved keys. This tier is required
/// structure even at stub (mirrors the Issues reserved-key precedent).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-002
/// @pseudocode component-003 lines 43-48
fn resolve_pr_suppressed_key(key_event: &KeyEvent) -> Option<AppEvent> {
    // s / Ctrl-d / Ctrl-k / l are reserved dashboard keys: identified explicitly
    // via `is_pr_suppressed_key` and consumed (None) so they never leak to the
    // dashboard. The consume itself is the terminal `None` (the outer
    // `handle_dashboard_prs_key` wrapper marks the key `Handled`).
    let _consumed = is_pr_suppressed_key(key_event);
    None
}

/// Whether `key_event` is a reserved dashboard key suppressed in PR Mode
/// (`s`, `Ctrl-d`, `Ctrl-k`, `l`).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-002
/// @pseudocode component-003 lines 43-48
fn is_pr_suppressed_key(key_event: &KeyEvent) -> bool {
    let ctrl = key_event.modifiers.contains(KeyModifiers::CONTROL);
    match key_event.code {
        KeyCode::Char('s' | 'l') => true,
        KeyCode::Char('d' | 'k') => ctrl,
        _ => false,
    }
}

/// Route key events when in PR Mode.
///
/// @plan PLAN-20260624-PR-MODE.P09
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

/// P5 global-key resolver for PR Mode (stub).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-001
/// @requirement REQ-PR-002
/// @pseudocode component-003 lines 23-30
fn resolve_pr_global_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        // Esc delegates to the unwind helper within the P5 tier (pseudocode L27),
        // before focus-domain and pane-cycle handlers in P6/P7. All other P5
        // global mappings (p|P, a, ?/h/F1, /, f) are deferred to P10 RED -> P11
        // GREEN; `o` lives in the P6 list/detail handlers, NOT here.
        KeyCode::Esc => handle_esc_in_prs_mode(state, key_event),
        _ => None,
    }
}

/// P6 focus-domain resolver for PR Mode (stub).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-003
/// @pseudocode component-003 lines 31-36
fn resolve_pr_focus_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match state.prs_state.pr_focus {
        PrFocus::RepoList => handle_pr_repo_key(state, key_event),
        PrFocus::PrList => handle_pr_list_key(state, key_event),
        PrFocus::PrDetail => handle_pr_detail_key(state, key_event),
    }
}

/// P7 pane-cycle resolver for PR Mode (stub).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-003
/// @pseudocode component-003 lines 37-42
fn resolve_pr_pane_cycle_key(key_event: &KeyEvent) -> Option<AppEvent> {
    // Tab / Shift+Tab pane-cycling is identified here so the P7 tier exists in
    // the precedence chain, but mapping to PrCycleFocus / PrCycleFocusReverse is
    // deferred to P11 (stub returns None so the P10 RED test stays red).
    let _is_pane_cycle = matches!(key_event.code, KeyCode::Tab | KeyCode::BackTab);
    None
}

/// Handle keys while the RepoList pane is focused (stub).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-003
/// @pseudocode component-003 lines 49-56
fn handle_pr_repo_key(_state: &AppState, _key_event: &KeyEvent) -> Option<AppEvent> {
    None
}

/// Handle keys while the PrList pane is focused (stub).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-003
/// @requirement REQ-PR-012
/// @pseudocode component-003 lines 57-70
fn handle_pr_list_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        // REQ-PR-012: open the selected PR in the browser, else surface a hint.
        KeyCode::Char('o') => Some(pr_open_in_browser_or_notice(selected_pr_present(state))),
        _ => None,
    }
}

/// Handle keys while the PrDetail pane is focused (stub).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-003
/// @requirement REQ-PR-012
/// @pseudocode component-003 lines 72-91
fn handle_pr_detail_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    // P11 reads `state.prs_state.detail_subfocus` for r/c/e notice routing.
    match key_event.code {
        // REQ-PR-012: open the selected PR in the browser, else surface a hint.
        KeyCode::Char('o') => Some(pr_open_in_browser_or_notice(pr_detail_present(state))),
        _ => None,
    }
}

/// Map `o` to the open-in-browser event when a PR target is present, else to a
/// non-blocking `NoSelectionToOpen` notice (consume + hint, never a silent drop).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-012
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
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-012
/// @pseudocode component-003 lines 68-69
fn selected_pr_present(state: &AppState) -> bool {
    state.prs_state.selected_pr_index.is_some()
}

/// Whether a loaded PR detail is present (REQ-PR-012 presence check).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-012
/// @pseudocode component-003 lines 88-89
fn pr_detail_present(state: &AppState) -> bool {
    state.prs_state.pr_detail.is_some()
}

/// Handle the Esc key in PR Mode (stub).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98
fn handle_esc_in_prs_mode(_state: &AppState, _key_event: &KeyEvent) -> Option<AppEvent> {
    None
}

/// Handle keys while an inline composer/editor is active (stub).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-010
/// @pseudocode component-003 lines 99-108
fn handle_pr_inline_key(_state: &AppState, _key_event: &KeyEvent) -> Option<AppEvent> {
    None
}

/// Handle keys while the agent chooser is open (stub).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 120-126
fn handle_pr_agent_chooser_key(_state: &AppState, _key_event: &KeyEvent) -> Option<AppEvent> {
    None
}

/// Handle keys while the search input is focused (stub).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-002
/// @pseudocode component-003 lines 127-133
fn handle_pr_search_input_key(_state: &AppState, _key_event: &KeyEvent) -> Option<AppEvent> {
    None
}
