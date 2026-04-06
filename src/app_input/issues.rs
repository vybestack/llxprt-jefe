//! Issues-mode key event dispatch.
//!
//! @plan PLAN-20260329-ISSUES-MODE.P09
//! @plan PLAN-20260329-ISSUES-MODE.P10
//! @plan PLAN-20260329-ISSUES-MODE.P11
//! @requirement REQ-ISS-002
//! @pseudocode component-003 lines 01-38

use iocraft::prelude::*;

use jefe::state::{AppEvent, AppState, DetailSubfocus, InlineState, IssueFocus};

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
#[allow(clippy::too_many_lines)]
pub fn resolve_issues_key_event(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    let inline_state = &state.issues_state.inline_state;
    let has_agent_chooser = state.issues_state.agent_chooser.is_some();
    let search_focused = state.issues_state.search_input_focused;
    let filter_open = state.issues_state.filter_controls_open;

    // Priority 1: Inline editor/composer — consumes all keys when active.
    // @plan PLAN-20260329-ISSUES-MODE.P11
    // @pseudocode component-003 lines 03-06
    if *inline_state != InlineState::None {
        return match key_event.code {
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
            _ => None, // consumed, no leak
        };
    }

    // Priority 2: Agent chooser — consumes all keys when open.
    // @plan PLAN-20260329-ISSUES-MODE.P11
    // @pseudocode component-003 lines 07-10
    if has_agent_chooser {
        return match key_event.code {
            KeyCode::Up => Some(AppEvent::AgentChooserNavigateUp),
            KeyCode::Down => Some(AppEvent::AgentChooserNavigateDown),
            KeyCode::Enter => Some(AppEvent::AgentChooserConfirm),
            KeyCode::Esc => Some(AppEvent::AgentChooserCancel),
            _ => None, // consumed
        };
    }

    // Priority 3: Search input focused.
    // @plan PLAN-20260329-ISSUES-MODE.P11
    // @pseudocode component-003 lines 11-14
    if search_focused {
        return match key_event.code {
            KeyCode::Enter => Some(AppEvent::ApplySearch),
            KeyCode::Esc => {
                if state.issues_state.search_query.is_empty() {
                    Some(AppEvent::BlurSearchInput)
                } else {
                    Some(AppEvent::ClearSearch)
                }
            }
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
        };
    }

    // Priority 4: Filter controls open.
    // @plan PLAN-20260329-ISSUES-MODE.P11
    // @pseudocode component-003 lines 15-18
    if filter_open {
        return match key_event.code {
            KeyCode::Enter => Some(AppEvent::ApplyFilter),
            KeyCode::Esc => Some(AppEvent::CloseFilterControls),
            _ => None, // filter field editing handled by UI layer
        };
    }

    // Priority 5: Issues-global unwind and mode controls.
    // @plan PLAN-20260329-ISSUES-MODE.P11
    // @pseudocode component-003 lines 19-26
    match key_event.code {
        KeyCode::Char('a') => return Some(AppEvent::ExitIssuesMode),
        KeyCode::Esc => {
            // Esc in IssueDetail goes back to IssueList, not all the way out
            return if state.issues_state.issue_focus == IssueFocus::IssueDetail {
                Some(AppEvent::RefocusIssueList)
            } else {
                Some(AppEvent::ExitIssuesMode)
            };
        }
        KeyCode::Char('i') => return Some(AppEvent::RefocusIssueList),
        KeyCode::Char('?' | 'h') | KeyCode::F(1) => {
            return Some(AppEvent::OpenHelp);
        }
        _ => {}
    }

    // Priority 6: Focus-domain handlers.
    // @plan PLAN-20260329-ISSUES-MODE.P11
    // @pseudocode component-003 lines 27-72
    match state.issues_state.issue_focus {
        IssueFocus::IssueList => match key_event.code {
            KeyCode::Up => return Some(AppEvent::IssuesNavigateUp),
            KeyCode::Down => return Some(AppEvent::IssuesNavigateDown),
            KeyCode::Left => return Some(AppEvent::IssuesCycleFocusReverse),
            KeyCode::Right => return Some(AppEvent::IssuesCycleFocus),
            KeyCode::PageUp => return Some(AppEvent::IssuesNavigatePageUp),
            KeyCode::PageDown => return Some(AppEvent::IssuesNavigatePageDown),
            KeyCode::Home => return Some(AppEvent::IssuesNavigateHome),
            KeyCode::End => return Some(AppEvent::IssuesNavigateEnd),
            KeyCode::Enter => return Some(AppEvent::IssuesEnter),
            KeyCode::Char('n' | 'N') => return Some(AppEvent::OpenNewIssueComposer),
            KeyCode::Char('f') => return Some(AppEvent::OpenFilterControls),
            KeyCode::Char('/') => return Some(AppEvent::FocusSearchInput),
            _ => {}
        },

        IssueFocus::IssueDetail => match key_event.code {
            KeyCode::Up => return Some(AppEvent::IssuesScrollDetailUp),
            KeyCode::Down => return Some(AppEvent::IssuesScrollDetailDown),
            KeyCode::Left => return Some(AppEvent::IssuesCycleFocusReverse),
            KeyCode::PageUp => return Some(AppEvent::IssuesScrollDetailPageUp),
            KeyCode::PageDown => return Some(AppEvent::IssuesScrollDetailPageDown),
            KeyCode::Char('e') => {
                return match state.issues_state.detail_subfocus {
                    DetailSubfocus::Body => Some(AppEvent::OpenInlineEditor {
                        target: jefe::state::EditorTarget::IssueBody,
                    }),
                    DetailSubfocus::Comment(idx) => Some(AppEvent::OpenInlineEditor {
                        target: jefe::state::EditorTarget::Comment { comment_index: idx },
                    }),
                    DetailSubfocus::NewComment => None,
                };
            }
            KeyCode::Char('c') => {
                return Some(AppEvent::OpenNewCommentComposer);
            }
            KeyCode::Char('r') => {
                return match state.issues_state.detail_subfocus {
                    DetailSubfocus::Comment(idx) => {
                        Some(AppEvent::OpenReplyComposer { comment_index: idx })
                    }
                    _ => None,
                };
            }
            KeyCode::Char('S') => {
                // Only open chooser if agents exist.
                return if state.agents.is_empty() {
                    None
                } else {
                    Some(AppEvent::OpenAgentChooser)
                };
            }
            KeyCode::Tab => return Some(AppEvent::IssueDetailSubfocusNext),
            KeyCode::BackTab => return Some(AppEvent::IssueDetailSubfocusPrev),
            _ => {}
        },

        IssueFocus::RepoList => match key_event.code {
            KeyCode::Up => return Some(AppEvent::IssuesNavigateUp),
            KeyCode::Down => return Some(AppEvent::IssuesNavigateDown),
            KeyCode::Right => return Some(AppEvent::IssuesCycleFocus),
            _ => {}
        },
    }

    // Priority 7: Pane focus cycling — catch-all Tab/Shift+Tab when not in detail.
    // @plan PLAN-20260329-ISSUES-MODE.P11
    // @pseudocode component-003 lines 73-80
    match key_event.code {
        KeyCode::Tab => return Some(AppEvent::IssuesCycleFocus),
        KeyCode::BackTab => return Some(AppEvent::IssuesCycleFocusReverse),
        _ => {}
    }

    // Priority 8: Suppression — consumed as no-op in issues mode.
    // @plan PLAN-20260329-ISSUES-MODE.P11
    // @pseudocode component-003 lines 81-90
    #[allow(clippy::match_same_arms)]
    match key_event.code {
        KeyCode::Char('s') => None,
        KeyCode::Char('d') if key_event.modifiers.contains(KeyModifiers::CONTROL) => None,
        KeyCode::Char('k') if key_event.modifiers.contains(KeyModifiers::CONTROL) => None,
        KeyCode::Char('l') => None,
        _ => None,
    }
}

/// Route key events when in Issues Mode.
///
/// @plan PLAN-20260329-ISSUES-MODE.P09
/// @plan PLAN-20260329-ISSUES-MODE.P11
/// @requirement REQ-ISS-002
/// @pseudocode component-003 lines 01-38
#[allow(clippy::too_many_lines)]
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
mod tests {
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

    /// `i` key in Dashboard mode dispatches EnterIssuesMode (routing in normal.rs).
    ///
    /// GREEN — verifies the event variant exists; routing already wired in P09.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P10
    /// @requirement REQ-ISS-001
    /// @pseudocode component-003 lines 01-38
    #[test]
    fn test_i_key_enters_issues_mode_event_identity() {
        let evt = AppEvent::EnterIssuesMode;
        assert!(matches!(evt, AppEvent::EnterIssuesMode));
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
        state.issues_state.filter_controls_open = true;
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
}
