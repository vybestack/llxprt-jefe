//! Key-routing tests for the agent-driven issue draft rewrite (issue #214).

use super::*;
use iocraft::prelude::{KeyCode, KeyEventKind, KeyModifiers};
use jefe::state::{AppEvent, ComposerTarget, InlineState, IssueFocus, IssuesState, ScreenMode};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(KeyEventKind::Press, code)
}

fn key_with_mods(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    let mut evt = KeyEvent::new(KeyEventKind::Press, code);
    evt.modifiers = modifiers;
    evt
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

/// Ctrl+R when the new-issue composer is active dispatches
/// RequestIssueRewrite (issue #214).
#[test]
fn test_ctrl_r_requests_issue_rewrite_from_new_issue_composer() {
    let state = issues_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewIssue,
        text: String::from("rough draft"),
        cursor: 10,
    });
    let event = resolve_issues_key_event(
        &state,
        &key_with_mods(KeyCode::Char('r'), KeyModifiers::CONTROL),
    );
    assert!(matches!(event, Some(AppEvent::RequestIssueRewrite)));
}

/// A plain lowercase 'r' (no modifier) in the composer must still insert a
/// character, never trigger the rewrite (issue #214).
#[test]
fn test_plain_r_types_a_character_in_composer() {
    let state = issues_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewIssue,
        text: String::new(),
        cursor: 0,
    });
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('r')));
    assert!(matches!(event, Some(AppEvent::InlineChar('r'))));
}
