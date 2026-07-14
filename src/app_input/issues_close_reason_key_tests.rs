//! Close-reason chooser key routing tests (issue #188).

use super::*;
use iocraft::prelude::{KeyCode, KeyEventKind};
use jefe::state::{AppEvent, IssueCloseReasonChooserState, IssueFocus, IssuesState, ScreenMode};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(KeyEventKind::Press, code)
}

fn issues_state_with_close_reason_chooser() -> AppState {
    AppState {
        screen_mode: ScreenMode::DashboardIssues,
        issues_state: IssuesState {
            active: true,
            issue_focus: IssueFocus::IssueList,
            close_reason_chooser: Some(IssueCloseReasonChooserState {
                issue_number: 1,
                selected_index: 0,
                duplicate_search: None,
                awaiting_confirmation: false,
            }),
            ..IssuesState::default()
        },
        ..AppState::default()
    }
}

#[test]
fn chooser_up_resolves_to_navigate_up() {
    let state = issues_state_with_close_reason_chooser();
    let result = resolve_issues_key_event(&state, &key(KeyCode::Up));
    assert!(
        matches!(result, Some(AppEvent::CloseReasonNavigateUp)),
        "Up should resolve to CloseReasonNavigateUp, got {result:?}"
    );
}

#[test]
fn chooser_down_resolves_to_navigate_down() {
    let state = issues_state_with_close_reason_chooser();
    let result = resolve_issues_key_event(&state, &key(KeyCode::Down));
    assert!(
        matches!(result, Some(AppEvent::CloseReasonNavigateDown)),
        "Down should resolve to CloseReasonNavigateDown, got {result:?}"
    );
}

#[test]
fn chooser_enter_resolves_to_select() {
    let state = issues_state_with_close_reason_chooser();
    let result = resolve_issues_key_event(&state, &key(KeyCode::Enter));
    assert!(
        matches!(result, Some(AppEvent::CloseReasonSelect)),
        "Enter should resolve to CloseReasonSelect when not awaiting confirmation, got {result:?}"
    );
}

#[test]
fn chooser_enter_resolves_to_confirm_when_awaiting() {
    let mut state = issues_state_with_close_reason_chooser();
    state.issues_state.close_reason_chooser = Some(IssueCloseReasonChooserState {
        issue_number: 1,
        selected_index: 0,
        duplicate_search: None,
        awaiting_confirmation: true,
    });
    let result = resolve_issues_key_event(&state, &key(KeyCode::Enter));
    assert!(
        matches!(result, Some(AppEvent::CloseReasonConfirm)),
        "Enter should resolve to CloseReasonConfirm when awaiting, got {result:?}"
    );
}

#[test]
fn chooser_esc_resolves_to_cancel() {
    let state = issues_state_with_close_reason_chooser();
    let result = resolve_issues_key_event(&state, &key(KeyCode::Esc));
    assert!(
        matches!(result, Some(AppEvent::CloseReasonCancel)),
        "Esc should resolve to CloseReasonCancel, got {result:?}"
    );
}

#[test]
fn chooser_duplicate_search_digit_resolves_to_search_char() {
    let mut state = issues_state_with_close_reason_chooser();
    state.issues_state.close_reason_chooser = Some(IssueCloseReasonChooserState {
        issue_number: 1,
        selected_index: 2,
        duplicate_search: Some(jefe::state::IssueDuplicateSearchState {
            query: String::new(),
            candidates: vec![(2u64, "Issue 2".to_string())],
            selected_index: 0,
        }),
        awaiting_confirmation: false,
    });
    let result = resolve_issues_key_event(&state, &key(KeyCode::Char('2')));
    assert!(
        matches!(result, Some(AppEvent::CloseReasonDuplicateSearchChar('2'))),
        "Digit in duplicate search should resolve to CloseReasonDuplicateSearchChar, got {result:?}"
    );
}

#[test]
fn chooser_duplicate_search_backspace_resolves() {
    let mut state = issues_state_with_close_reason_chooser();
    state.issues_state.close_reason_chooser = Some(IssueCloseReasonChooserState {
        issue_number: 1,
        selected_index: 2,
        duplicate_search: Some(jefe::state::IssueDuplicateSearchState {
            query: "1".to_string(),
            candidates: vec![(1u64, "Issue 1".to_string())],
            selected_index: 0,
        }),
        awaiting_confirmation: false,
    });
    let result = resolve_issues_key_event(&state, &key(KeyCode::Backspace));
    assert!(
        matches!(result, Some(AppEvent::CloseReasonDuplicateSearchBackspace)),
        "Backspace in duplicate search should resolve to CloseReasonDuplicateSearchBackspace, got {result:?}"
    );
}

#[test]
fn chooser_duplicate_search_enter_resolves_to_confirm() {
    let mut state = issues_state_with_close_reason_chooser();
    state.issues_state.close_reason_chooser = Some(IssueCloseReasonChooserState {
        issue_number: 1,
        selected_index: 2,
        duplicate_search: Some(jefe::state::IssueDuplicateSearchState {
            query: "2".to_string(),
            candidates: vec![(2u64, "Issue 2".to_string())],
            selected_index: 0,
        }),
        awaiting_confirmation: false,
    });
    let result = resolve_issues_key_event(&state, &key(KeyCode::Enter));
    assert!(
        matches!(result, Some(AppEvent::CloseReasonConfirm)),
        "Enter in duplicate search should resolve to CloseReasonConfirm, got {result:?}"
    );
}
