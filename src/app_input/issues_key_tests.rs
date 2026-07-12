//! Issues-mode key dispatch unit tests (extracted from issues.rs).
//!
//! @plan PLAN-20260329-ISSUES-MODE.P10
//! @plan PLAN-20260329-ISSUES-MODE.P11
//! @requirement REQ-ISS-002

use super::*;
use iocraft::prelude::{KeyCode, KeyEventKind, KeyModifiers};
use jefe::domain::{Agent, AgentId, RepositoryId};
use jefe::input::{InputMode, input_mode_for_state};
use jefe::state::{
    AgentChooserState, AppEvent, AppState, ComposerTarget, DetailSubfocus, EditorTarget,
    InlineState, IssueFocus, IssuesState, ScreenMode,
};
use std::path::PathBuf;

// ─── Key construction helpers ───────────────────────────────────────────

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(KeyEventKind::Press, code)
}

fn key_with_mods(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    let mut evt = KeyEvent::new(KeyEventKind::Press, code);
    evt.modifiers = modifiers;
    evt
}

// ─── State construction helpers ─────────────────────────────────────────

fn issues_base_state() -> AppState {
    AppState {
        screen_mode: ScreenMode::DashboardIssues,
        issues_state: IssuesState {
            active: true,
            issue_focus: IssueFocus::IssueList,
            ..IssuesState::default()
        },
        ..AppState::default()
    }
}

fn issues_state_with_focus(focus: IssueFocus) -> AppState {
    AppState {
        screen_mode: ScreenMode::DashboardIssues,
        issues_state: IssuesState {
            active: true,
            issue_focus: focus,
            ..IssuesState::default()
        },
        ..AppState::default()
    }
}

fn issues_state_with_inline(inline: InlineState) -> AppState {
    AppState {
        screen_mode: ScreenMode::DashboardIssues,
        issues_state: IssuesState {
            active: true,
            issue_focus: IssueFocus::IssueList,
            inline_state: inline,
            ..IssuesState::default()
        },
        ..AppState::default()
    }
}

fn issues_state_with_chooser() -> AppState {
    AppState {
        screen_mode: ScreenMode::DashboardIssues,
        issues_state: IssuesState {
            active: true,
            issue_focus: IssueFocus::IssueList,
            agent_chooser: Some(AgentChooserState {
                selected_index: 0,
                agents: vec![(AgentId(String::from("a1")), String::from("Agent 1"))],
            }),
            ..IssuesState::default()
        },
        ..AppState::default()
    }
}

fn issues_state_with_detail_subfocus(subfocus: DetailSubfocus) -> AppState {
    AppState {
        screen_mode: ScreenMode::DashboardIssues,
        issues_state: IssuesState {
            active: true,
            issue_focus: IssueFocus::IssueDetail,
            detail_subfocus: subfocus,
            ..IssuesState::default()
        },
        ..AppState::default()
    }
}

fn add_agent(state: &mut AppState) {
    state.agents.push(Agent::new(
        AgentId(String::from("agent-1")),
        RepositoryId(String::from("repo-1")),
        String::from("Agent One"),
        PathBuf::from("/tmp/agent"),
    ));
}

// ═══════════════════════════════════════════════════════════════════════
// Mode Entry / Exit
// ═══════════════════════════════════════════════════════════════════════

/// `a` key in DashboardIssues mode dispatches ExitIssuesMode.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-001
/// @pseudocode component-003 lines 01-38
#[test]
fn test_a_key_exits_issues_mode() {
    let state = issues_base_state();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('a')));
    assert!(matches!(event, Some(AppEvent::ExitIssuesMode)));
}

// ═══════════════════════════════════════════════════════════════════════
// Suppression Tests (expect None — GREEN from stub)
// ═══════════════════════════════════════════════════════════════════════

/// `s` key is suppressed (returns None) in Issues Mode.
///
/// GREEN — already implemented in P09 stub.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @requirement REQ-ISS-002
/// @pseudocode component-003 lines 28-38
#[test]
fn test_s_key_suppressed_in_issues_mode() {
    let state = issues_base_state();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('s')));
    assert!(event.is_none());
}

/// `Ctrl-d` is suppressed (returns None) in Issues Mode.
///
/// GREEN — already implemented in P09 stub.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @requirement REQ-ISS-002
/// @pseudocode component-003 lines 28-38
#[test]
fn test_ctrl_d_suppressed_in_issues_mode() {
    let state = issues_base_state();
    let event = resolve_issues_key_event(
        &state,
        &key_with_mods(KeyCode::Char('d'), KeyModifiers::CONTROL),
    );
    assert!(event.is_none());
}

/// `Ctrl-k` is suppressed (returns None) in Issues Mode.
///
/// GREEN — already implemented in P09 stub.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @requirement REQ-ISS-002
/// @pseudocode component-003 lines 28-38
#[test]
fn test_ctrl_k_suppressed_in_issues_mode() {
    let state = issues_base_state();
    let event = resolve_issues_key_event(
        &state,
        &key_with_mods(KeyCode::Char('k'), KeyModifiers::CONTROL),
    );
    assert!(event.is_none());
}

/// `l` key is suppressed (returns None) in Issues Mode.
///
/// GREEN — already implemented in P09 stub.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @requirement REQ-ISS-002
/// @pseudocode component-003 lines 28-38
#[test]
fn test_l_key_suppressed_in_issues_mode() {
    let state = issues_base_state();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('l')));
    assert!(event.is_none());
}

// ═══════════════════════════════════════════════════════════════════════
// Navigation — Issue List (7 tests)
// ═══════════════════════════════════════════════════════════════════════

/// Down arrow in IssueList focus dispatches IssuesNavigateDown.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-003
/// @pseudocode component-003 lines 39-50
#[test]
fn test_down_in_issue_list_dispatches_navigate() {
    let state = issues_state_with_focus(IssueFocus::IssueList);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Down));
    assert!(matches!(event, Some(AppEvent::IssuesNavigateDown)));
}

/// Up arrow in IssueList focus dispatches IssuesNavigateUp.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-003
/// @pseudocode component-003 lines 39-50
#[test]
fn test_up_in_issue_list_dispatches_navigate() {
    let state = issues_state_with_focus(IssueFocus::IssueList);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Up));
    assert!(matches!(event, Some(AppEvent::IssuesNavigateUp)));
}

/// PageUp in IssueList focus dispatches IssuesNavigatePageUp.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-003
/// @pseudocode component-003 lines 39-50
#[test]
fn test_page_up_in_issue_list_dispatches_navigate() {
    let state = issues_state_with_focus(IssueFocus::IssueList);
    let event = resolve_issues_key_event(&state, &key(KeyCode::PageUp));
    assert!(matches!(event, Some(AppEvent::IssuesNavigatePageUp)));
}

/// PageDown in IssueList focus dispatches IssuesNavigatePageDown.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-003
/// @pseudocode component-003 lines 39-50
#[test]
fn test_page_down_in_issue_list_dispatches_navigate() {
    let state = issues_state_with_focus(IssueFocus::IssueList);
    let event = resolve_issues_key_event(&state, &key(KeyCode::PageDown));
    assert!(matches!(event, Some(AppEvent::IssuesNavigatePageDown)));
}

#[test]
fn test_down_in_issue_detail_dispatches_scroll() {
    let state = issues_state_with_focus(IssueFocus::IssueDetail);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Down));
    assert!(matches!(event, Some(AppEvent::IssuesScrollDetailDown)));
}

#[test]
fn test_page_down_in_issue_detail_dispatches_page_scroll() {
    let state = issues_state_with_focus(IssueFocus::IssueDetail);
    let event = resolve_issues_key_event(&state, &key(KeyCode::PageDown));
    assert!(matches!(event, Some(AppEvent::IssuesScrollDetailPageDown)));
}

/// Home in IssueList focus dispatches IssuesNavigateHome.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-003
/// @pseudocode component-003 lines 39-50
#[test]
fn test_home_in_issue_list_dispatches_navigate() {
    let state = issues_state_with_focus(IssueFocus::IssueList);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Home));
    assert!(matches!(event, Some(AppEvent::IssuesNavigateHome)));
}

/// End in IssueList focus dispatches IssuesNavigateEnd.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-003
/// @pseudocode component-003 lines 39-50
#[test]
fn test_end_in_issue_list_dispatches_navigate() {
    let state = issues_state_with_focus(IssueFocus::IssueList);
    let event = resolve_issues_key_event(&state, &key(KeyCode::End));
    assert!(matches!(event, Some(AppEvent::IssuesNavigateEnd)));
}

/// Enter in IssueList focus dispatches IssuesEnter (transitions to detail).
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-003
/// @pseudocode component-003 lines 39-50
#[test]
fn test_enter_in_issue_list_focuses_detail() {
    let state = issues_state_with_focus(IssueFocus::IssueList);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Enter));
    assert!(matches!(event, Some(AppEvent::IssuesEnter)));
}

/// `n` in IssueList focus dispatches OpenNewIssueComposer.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-010
#[test]
fn test_n_opens_new_issue_composer_from_issue_list() {
    let state = issues_state_with_focus(IssueFocus::IssueList);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('n')));
    assert!(matches!(event, Some(AppEvent::OpenNewIssueComposer)));
}

/// `N` in IssueList focus dispatches OpenNewIssueComposer.
#[test]
fn test_upper_n_opens_new_issue_composer_from_issue_list() {
    let state = issues_state_with_focus(IssueFocus::IssueList);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('N')));
    assert!(matches!(event, Some(AppEvent::OpenNewIssueComposer)));
}

// ═══════════════════════════════════════════════════════════════════════
// Tab Cycling (2 tests)
// ═══════════════════════════════════════════════════════════════════════

/// Tab dispatches IssuesCycleFocus in issues mode.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-003
/// @pseudocode component-001 lines 71-82
#[test]
fn test_tab_cycles_issues_pane_focus() {
    let state = issues_base_state();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Tab));
    assert!(matches!(event, Some(AppEvent::IssuesCycleFocus)));
}

/// j/k cycle detail subfocus in IssueDetail (issue #150 — vim aliases for
/// Tab/BackTab subfocus cycling).
#[test]
fn test_j_k_cycle_detail_subfocus_in_issue_detail() {
    let state = issues_state_with_focus(IssueFocus::IssueDetail);
    let j = resolve_issues_key_event(&state, &key(KeyCode::Char('j')));
    assert!(matches!(j, Some(AppEvent::IssueDetailSubfocusNext)));
    let k = resolve_issues_key_event(&state, &key(KeyCode::Char('k')));
    assert!(matches!(k, Some(AppEvent::IssueDetailSubfocusPrev)));
}

/// j/k are consumed as InlineChar when an inline editor is active (P1 inline
/// precedence over the P6 detail-subfocus mapping) — protects the routing
/// priority chain so the j/k subfocus alias never leaks into inline typing.
#[test]
fn test_j_k_consumed_by_inline_when_active_not_subfocus() {
    let state = issues_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    });
    let j = resolve_issues_key_event(&state, &key(KeyCode::Char('j')));
    assert!(
        matches!(j, Some(AppEvent::InlineChar('j'))),
        "Inline must consume 'j' (got {j:?})"
    );
    let k = resolve_issues_key_event(&state, &key(KeyCode::Char('k')));
    assert!(
        matches!(k, Some(AppEvent::InlineChar('k'))),
        "Inline must consume 'k' (got {k:?})"
    );
}

/// Tab/BackTab cycle detail subfocus in IssueDetail (issue #150).
#[test]
fn test_tab_cycles_detail_subfocus_in_issue_detail() {
    let state = issues_state_with_focus(IssueFocus::IssueDetail);
    let tab = resolve_issues_key_event(&state, &key(KeyCode::Tab));
    assert!(matches!(tab, Some(AppEvent::IssueDetailSubfocusNext)));
    let back = resolve_issues_key_event(&state, &key(KeyCode::BackTab));
    assert!(matches!(back, Some(AppEvent::IssueDetailSubfocusPrev)));
}

/// Right arrow forward-cycles panes from IssueDetail (issue #150 —
/// Left/Right symmetric pane-focus in every pane).
#[test]
fn test_right_arrow_forward_cycles_pane_from_issue_detail() {
    let state = issues_state_with_focus(IssueFocus::IssueDetail);
    let right = resolve_issues_key_event(&state, &key(KeyCode::Right));
    assert!(matches!(right, Some(AppEvent::IssuesCycleFocus)));
}

/// Left arrow reverse-cycles panes from RepoList (issue #150 — Left/Right
/// symmetric pane-focus in every pane).
#[test]
fn test_left_arrow_reverse_cycles_pane_from_issue_repo_list() {
    let state = issues_state_with_focus(IssueFocus::RepoList);
    let left = resolve_issues_key_event(&state, &key(KeyCode::Left));
    assert!(matches!(left, Some(AppEvent::IssuesCycleFocusReverse)));
}

/// Shift+Tab dispatches IssuesCycleFocusReverse in issues mode.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-003
/// @pseudocode component-001 lines 71-82
#[test]
fn test_shift_tab_reverse_cycles() {
    let state = issues_base_state();
    let event = resolve_issues_key_event(&state, &key(KeyCode::BackTab));
    assert!(matches!(event, Some(AppEvent::IssuesCycleFocusReverse)));
}

// ═══════════════════════════════════════════════════════════════════════
// Search / Filter (3 tests)
// ═══════════════════════════════════════════════════════════════════════

/// `/` key dispatches FocusSearchInput in issues mode.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-002
/// @requirement REQ-ISS-008
/// @pseudocode component-003 lines 112-119
#[test]
fn test_slash_focuses_search_in_issues_mode() {
    let state = issues_base_state();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('/')));
    assert!(matches!(event, Some(AppEvent::FocusSearchInput)));
}

/// `f` from IssueList focus dispatches OpenFilterControls.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-008
/// @pseudocode component-003 lines 120-127
#[test]
fn test_f_opens_filter_from_issue_list_focus() {
    let state = issues_state_with_focus(IssueFocus::IssueList);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('f')));
    assert!(matches!(event, Some(AppEvent::OpenFilterControls)));
}

/// `f` from IssueDetail focus returns None (no-op).
///
/// GREEN — stub returns None for all non-suppression keys.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @requirement REQ-ISS-008
/// @pseudocode component-003 lines 120-127
#[test]
fn test_f_noop_from_non_issue_list_focus() {
    let state = issues_state_with_focus(IssueFocus::IssueDetail);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('f')));
    assert!(event.is_none());
}

// ═══════════════════════════════════════════════════════════════════════
// Esc Priority (2 tests)
// ═══════════════════════════════════════════════════════════════════════

/// Esc with inline editor active dispatches InlineCancelOrEsc, not ExitIssuesMode.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-002
/// @requirement REQ-ISS-004
/// @pseudocode component-003 lines 01-17
/// @pseudocode component-001 lines 115-127
#[test]
fn test_esc_inline_priority_over_mode_exit() {
    let state = issues_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    });
    let event = resolve_issues_key_event(&state, &key(KeyCode::Esc));
    // Must be InlineCancelOrEsc, not ExitIssuesMode
    assert!(matches!(event, Some(AppEvent::InlineCancelOrEsc)));
}

/// Esc with agent chooser active (no inline) dispatches AgentChooserCancel.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-002
/// @requirement REQ-ISS-004
/// @pseudocode component-003 lines 01-17
/// @pseudocode component-001 lines 115-127
#[test]
fn test_esc_chooser_priority_over_mode_exit() {
    let state = issues_state_with_chooser();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Esc));
    // Must be AgentChooserCancel, not ExitIssuesMode
    assert!(matches!(event, Some(AppEvent::AgentChooserCancel)));
}

// ═══════════════════════════════════════════════════════════════════════
// Inline Mutation (6 tests)
// ═══════════════════════════════════════════════════════════════════════

/// `e` with body subfocus dispatches OpenInlineEditor for IssueBody.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-010
/// @pseudocode component-003 lines 51-72
#[test]
fn test_e_opens_editor_on_body() {
    let state = issues_state_with_detail_subfocus(DetailSubfocus::Body);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('e')));
    assert!(matches!(
        event,
        Some(AppEvent::OpenInlineEditor {
            target: EditorTarget::IssueBody
        })
    ));
}

/// `e` with comment subfocus dispatches OpenInlineEditor for that comment.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-010
/// @pseudocode component-003 lines 51-72
#[test]
fn test_e_opens_editor_on_comment() {
    let state = issues_state_with_detail_subfocus(DetailSubfocus::Comment(2));
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('e')));
    assert!(matches!(
        event,
        Some(AppEvent::OpenInlineEditor {
            target: EditorTarget::Comment { comment_index: 2 }
        })
    ));
}

/// `r` with comment subfocus dispatches OpenReplyComposer.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-010
/// @pseudocode component-003 lines 51-72
/// @pseudocode component-003 lines 136-137
#[test]
fn test_r_opens_reply_on_comment() {
    let state = issues_state_with_detail_subfocus(DetailSubfocus::Comment(1));
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('r')));
    assert!(matches!(
        event,
        Some(AppEvent::OpenReplyComposer { comment_index: 1 })
    ));
}

/// `r` with body subfocus returns None (no reply on body).
///
/// GREEN — stub returns None for all unimplemented keys.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @requirement REQ-ISS-010
/// @pseudocode component-003 lines 51-72
#[test]
fn test_r_noop_when_not_on_comment() {
    let state = issues_state_with_detail_subfocus(DetailSubfocus::Body);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('r')));
    assert!(event.is_none());
}

/// Ctrl+Enter when inline active dispatches InlineSubmit.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-010
/// @pseudocode component-003 lines 73-77
#[test]
fn test_ctrl_enter_submits_inline() {
    let state = issues_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::from("hello"),
        cursor: 5,
    });
    let event = resolve_issues_key_event(
        &state,
        &key_with_mods(KeyCode::Enter, KeyModifiers::CONTROL),
    );
    assert!(matches!(event, Some(AppEvent::InlineSubmit)));
}

#[test]
fn test_ctrl_c_cancels_inline_instead_of_typing_c() {
    let state = issues_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::from("hello"),
        cursor: 5,
    });
    let event = resolve_issues_key_event(
        &state,
        &key_with_mods(KeyCode::Char('c'), KeyModifiers::CONTROL),
    );
    assert!(matches!(event, Some(AppEvent::InlineCancelOrEsc)));
}

/// Esc when inline editor active dispatches InlineCancelOrEsc.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-010
/// @pseudocode component-003 lines 73-77
#[test]
fn test_esc_cancels_inline_editor() {
    let state = issues_state_with_inline(InlineState::Editor {
        target: EditorTarget::IssueBody,
        text: String::from("draft"),
        cursor: 0,
    });
    let event = resolve_issues_key_event(&state, &key(KeyCode::Esc));
    assert!(matches!(event, Some(AppEvent::InlineCancelOrEsc)));
}

// ═══════════════════════════════════════════════════════════════════════
// Agent Chooser (3 tests)
// ═══════════════════════════════════════════════════════════════════════

/// `S` from IssueDetail focus dispatches OpenAgentChooser when agents exist.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-011
/// @pseudocode component-003 lines 102-111
#[test]
fn test_s_opens_agent_chooser() {
    let mut state = issues_state_with_focus(IssueFocus::IssueDetail);
    add_agent(&mut state);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('S')));
    assert!(matches!(event, Some(AppEvent::OpenAgentChooser)));
}

/// `S` with inline active is consumed by inline handler, NOT agent chooser.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @requirement REQ-ISS-011
/// @pseudocode component-003 lines 138-141
#[test]
fn test_s_noop_when_inline_active() {
    let mut state = issues_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    });
    add_agent(&mut state);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('S')));
    // Inline handler consumes the key as InlineChar('S'), NOT OpenAgentChooser
    assert!(
        matches!(event, Some(AppEvent::InlineChar('S'))),
        "Expected InlineChar('S'), got {event:?}"
    );
}

/// `S` with no agents returns None (shows message instead of opening chooser).
///
/// GREEN — stub returns None for all unimplemented keys regardless of agent count.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @requirement REQ-ISS-011
/// @pseudocode component-003 lines 102-111
#[test]
fn test_s_shows_message_when_no_agents() {
    let state = issues_state_with_focus(IssueFocus::IssueDetail);
    // No agents in state
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('S')));
    assert!(event.is_none());
}

// ═══════════════════════════════════════════════════════════════════════
// InputMode Detection (5 tests — all GREEN, already implemented)
// ═══════════════════════════════════════════════════════════════════════

/// DashboardIssues mode with nothing special active → IssuesNormal.
///
/// GREEN — input_mode_for_state is already fully implemented.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @requirement REQ-ISS-002
/// @pseudocode component-003 lines 01-17
#[test]
fn test_input_mode_issues_normal() {
    let state = issues_base_state();
    assert!(matches!(
        input_mode_for_state(&state),
        InputMode::IssuesNormal
    ));
}

/// DashboardIssues with inline active → IssuesInline.
///
/// GREEN — input_mode_for_state is already fully implemented.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @requirement REQ-ISS-002
/// @pseudocode component-003 lines 01-17
#[test]
fn test_input_mode_issues_inline() {
    let state = issues_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    });
    assert!(matches!(
        input_mode_for_state(&state),
        InputMode::IssuesInline
    ));
}

/// DashboardIssues with agent chooser active → IssuesChooser.
///
/// GREEN — input_mode_for_state is already fully implemented.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @requirement REQ-ISS-002
/// @pseudocode component-003 lines 01-17
#[test]
fn test_input_mode_issues_chooser() {
    let state = issues_state_with_chooser();
    assert!(matches!(
        input_mode_for_state(&state),
        InputMode::IssuesChooser
    ));
}

/// DashboardIssues with search input focused → IssuesSearch.
///
/// GREEN — input_mode_for_state is already fully implemented.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @requirement REQ-ISS-002
/// @requirement REQ-ISS-008
/// @pseudocode component-003 lines 10-12
#[test]
fn test_input_mode_issues_search() {
    let mut state = issues_base_state();
    state.issues_state.search_input_focused = true;
    assert!(matches!(
        input_mode_for_state(&state),
        InputMode::IssuesSearch
    ));
}

/// DashboardIssues with filter controls open → IssuesFilter.
///
/// GREEN — input_mode_for_state is already fully implemented.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @requirement REQ-ISS-002
/// @requirement REQ-ISS-008
/// @pseudocode component-003 lines 14-16
#[test]
fn test_input_mode_issues_filter() {
    let mut state = issues_base_state();
    state.issues_state.filter_ui.controls_open = true;
    assert!(matches!(
        input_mode_for_state(&state),
        InputMode::IssuesFilter
    ));
}

// ═══════════════════════════════════════════════════════════════════════
// Miscellaneous
// ═══════════════════════════════════════════════════════════════════════

/// `o` key in issue detail returns None (no action defined).
///
/// GREEN — stub returns None for all unimplemented keys.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @requirement REQ-ISS-009
/// @pseudocode component-003 lines 50-70
#[test]
fn test_o_key_noop_in_issue_detail() {
    let state = issues_state_with_focus(IssueFocus::IssueDetail);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('o')));
    assert!(event.is_none());
}

/// Esc in IssueDetail focus goes back to IssueList (not exit mode).
#[test]
fn test_esc_in_detail_goes_back_to_list() {
    let state = issues_state_with_focus(IssueFocus::IssueDetail);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Esc));
    assert!(matches!(event, Some(AppEvent::RefocusIssueList)));
}

/// Esc in IssueList focus exits issues mode entirely.
#[test]
fn test_esc_in_list_exits_mode() {
    let state = issues_state_with_focus(IssueFocus::IssueList);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Esc));
    assert!(matches!(event, Some(AppEvent::ExitIssuesMode)));
}

/// Esc in RepoList focus exits issues mode entirely.
#[test]
fn test_esc_in_repo_list_exits_mode() {
    let state = issues_state_with_focus(IssueFocus::RepoList);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Esc));
    assert!(matches!(event, Some(AppEvent::ExitIssuesMode)));
}

/// Up/Down arrows in inline mode dispatch cursor movement events.
#[test]
fn test_up_down_in_inline_dispatches_cursor_vertical() {
    // cursor 8 lands on the second line ("line1\n" is 6 bytes; index 8 is in
    // "line2") — a valid position so Up/Down dispatch vertical-cursor events.
    // The assertions only check the dispatched event, not cursor math.
    let state = issues_state_with_inline(InlineState::Editor {
        target: EditorTarget::IssueBody,
        text: String::from(
            "line1
line2",
        ),
        cursor: 8,
    });
    let up = resolve_issues_key_event(&state, &key(KeyCode::Up));
    assert!(matches!(up, Some(AppEvent::InlineCursorUp)));
    let down = resolve_issues_key_event(&state, &key(KeyCode::Down));
    assert!(matches!(down, Some(AppEvent::InlineCursorDown)));
}

// ═══════════════════════════════════════════════════════════════════════
// F12 mode-aware behavior + cross-mode `p` (issue #164)
// ═══════════════════════════════════════════════════════════════════════

/// F12 in IssueDetail focus returns to the issue list (issue #164).
#[test]
fn f12_in_issue_detail_returns_to_list() {
    let state = issues_state_with_focus(IssueFocus::IssueDetail);
    let event = resolve_issues_key_event(&state, &key(KeyCode::F(12)));
    assert!(
        matches!(event, Some(AppEvent::RefocusIssueList)),
        "F12 in IssueDetail must yield RefocusIssueList, got {event:?}"
    );
}

/// F12 at the issue list with the terminal unfocused is a no-op (issue #164).
#[test]
fn f12_in_issue_list_is_noop() {
    let mut state = issues_base_state();
    state.terminal_focused = false;
    let event = resolve_issues_key_event(&state, &key(KeyCode::F(12)));
    assert!(
        event.is_none(),
        "F12 at IssueList (terminal unfocused) must be None, got {event:?}"
    );
}

/// F12 while the terminal is focused defocuses it (issue #164).
#[test]
fn f12_while_terminal_focused_defocuses() {
    let mut state = issues_base_state();
    state.terminal_focused = true;
    let event = resolve_issues_key_event(&state, &key(KeyCode::F(12)));
    assert!(
        matches!(event, Some(AppEvent::ToggleTerminalFocus)),
        "F12 with terminal focused must yield ToggleTerminalFocus, got {event:?}"
    );
}

/// F12 does not fire when the inline composer is open (overlay owns the key).
#[test]
fn f12_does_not_fire_when_inline_composer_open() {
    let state = issues_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    });
    let event = resolve_issues_key_event(&state, &key(KeyCode::F(12)));
    assert!(
        event.is_none(),
        "F12 must be suppressed by inline composer, got {event:?}"
    );
}

/// F12 does not fire when the search input is focused (overlay owns the key).
#[test]
fn f12_does_not_fire_when_search_input_focused() {
    let mut state = issues_base_state();
    state.issues_state.search_input_focused = true;
    let event = resolve_issues_key_event(&state, &key(KeyCode::F(12)));
    assert!(
        event.is_none(),
        "F12 must be suppressed by search input focus, got {event:?}"
    );
}

/// `p` from Issues mode enters PR mode (issue #164 cross-mode navigation).
#[test]
fn p_from_issues_enters_prs_mode() {
    let state = issues_base_state();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('p')));
    assert!(
        matches!(event, Some(AppEvent::EnterPrsMode)),
        "'p' from Issues must yield EnterPrsMode, got {event:?}"
    );
}

/// `p` does not fire when the inline composer is open (overlay owns the key).
#[test]
fn p_from_issues_does_not_fire_when_composer_open() {
    let state = issues_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    });
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('p')));
    assert!(
        matches!(event, Some(AppEvent::InlineChar('p'))),
        "'p' with inline open must yield InlineChar('p'), got {event:?}"
    );
}

/// `i` from IssueDetail still refocuses the issue list (regression, issue #164).
#[test]
fn i_from_issues_still_refocuses_list() {
    let state = issues_state_with_focus(IssueFocus::IssueDetail);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('i')));
    assert!(
        matches!(event, Some(AppEvent::RefocusIssueList)),
        "'i' in IssueDetail must yield RefocusIssueList, got {event:?}"
    );
}

// ─── Overlay precedence for cross-mode keys (issue #164 review Finding 4) ──

/// F12 while the terminal is focused AND in IssueDetail defocuses the
/// terminal first (one-layer-at-a-time). The detail view stays — only the
/// terminal defocus wins.
#[test]
fn f12_while_terminal_focused_and_in_detail_defocuses_terminal_first() {
    let mut state = issues_state_with_focus(IssueFocus::IssueDetail);
    state.terminal_focused = true;
    let event = resolve_issues_key_event(&state, &key(KeyCode::F(12)));
    assert!(
        matches!(event, Some(AppEvent::ToggleTerminalFocus)),
        "F12 with terminal focused must yield ToggleTerminalFocus even in IssueDetail, got {event:?}"
    );
}

/// `p` while the search input is focused types into the query — it must NOT
/// switch to PR mode (overlay owns the key before the global tier).
#[test]
fn p_in_search_input_does_not_switch_modes() {
    let mut state = issues_base_state();
    state.issues_state.search_input_focused = true;
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('p')));
    assert!(
        matches!(event, Some(AppEvent::SetSearchQuery { .. })),
        "'p' with search focused must yield SetSearchQuery, got {event:?}"
    );
    assert!(
        !matches!(event, Some(AppEvent::EnterPrsMode)),
        "'p' with search focused must NOT yield EnterPrsMode"
    );
}

/// `p` while the filter controls are open must NOT switch to PR mode (the
/// filter resolver owns the key before the global tier).
#[test]
fn p_in_filter_controls_does_not_switch_modes() {
    let mut state = issues_base_state();
    state.issues_state.filter_ui.controls_open = true;
    // Default field_index is 0 (state cycle field); 'p' is consumed as None.
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('p')));
    assert!(
        !matches!(event, Some(AppEvent::EnterPrsMode)),
        "'p' with filter controls open must NOT yield EnterPrsMode, got {event:?}"
    );
}

/// `p` while the inline composer is active types the character into the
/// composer — it must NOT switch to PR mode.
#[test]
fn p_in_inline_composer_types_char() {
    let state = issues_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    });
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('p')));
    assert!(
        matches!(event, Some(AppEvent::InlineChar('p'))),
        "'p' with inline composer must yield InlineChar('p'), got {event:?}"
    );
}
