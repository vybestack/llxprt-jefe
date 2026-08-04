//! Key-routing coverage for the optional PR Changes drill-down.

use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind};
use jefe::state::{AppEvent, AppState, PrChangesFocus, PrFocus, PullRequestsState, ScreenId};

use super::{resolve_prs_key_event, resolve_prs_key_event_for_rows};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(KeyEventKind::Press, code)
}

fn state_with_focus(focus: PrFocus) -> AppState {
    AppState {
        nav: crate::state::navigation::NavState::rooted(ScreenId::PullRequests),
        prs_state: PullRequestsState {
            active: true,
            pr_focus: focus,
            ..PullRequestsState::default()
        },
        ..AppState::default()
    }
}

fn state_with_changes(focus: PrChangesFocus) -> AppState {
    let mut state = state_with_focus(PrFocus::PrChanges);
    state.prs_state.changes.focus = focus;
    state.prs_state.changes.selected_file = Some(0);
    state
}

#[test]
fn changes_keys_drill_down_navigation_and_back() {
    let detail = state_with_focus(PrFocus::PrDetail);
    assert!(matches!(
        resolve_prs_key_event(&detail, &key(KeyCode::Char('d'))),
        Some(AppEvent::PrOpenChanges)
    ));

    let files = state_with_changes(PrChangesFocus::FileList);
    assert!(matches!(
        resolve_prs_key_event(&files, &key(KeyCode::Enter)),
        Some(AppEvent::PrChangesFocusContent)
    ));
    assert!(matches!(
        resolve_prs_key_event(&files, &key(KeyCode::Char('v'))),
        Some(AppEvent::PrChangesToggleView)
    ));
    assert!(matches!(
        resolve_prs_key_event(&files, &key(KeyCode::Esc)),
        Some(AppEvent::PrChangesBack)
    ));

    let content = state_with_changes(PrChangesFocus::Content);
    assert!(matches!(
        resolve_prs_key_event(&content, &key(KeyCode::BackTab)),
        Some(AppEvent::PrChangesFocusFiles)
    ));
    assert!(matches!(
        resolve_prs_key_event(&content, &key(KeyCode::Esc)),
        Some(AppEvent::PrChangesBack)
    ));
}

#[test]
fn changes_file_list_consumes_focus_keys_without_a_selection() {
    let mut state = state_with_focus(PrFocus::PrChanges);
    // Explicitly assert FileList focus so the test does not silently test the
    // wrong state if the enum's default changes (issue #376 OCR finding).
    state.prs_state.changes.focus = PrChangesFocus::FileList;
    assert!(resolve_prs_key_event(&state, &key(KeyCode::Enter)).is_none());
    assert!(resolve_prs_key_event(&state, &key(KeyCode::Tab)).is_none());
    assert!(resolve_prs_key_event(&state, &key(KeyCode::BackTab)).is_none());
}

#[test]
fn changes_page_navigation_tracks_the_terminal_viewport() {
    let state = state_with_changes(PrChangesFocus::FileList);
    let Some(AppEvent::PrNavigatePageDown(small)) =
        resolve_prs_key_event_for_rows(&state, &key(KeyCode::PageDown), 18)
    else {
        panic!("small Changes viewport should page down");
    };
    let Some(AppEvent::PrNavigatePageDown(large)) =
        resolve_prs_key_event_for_rows(&state, &key(KeyCode::PageDown), 50)
    else {
        panic!("large Changes viewport should page down");
    };

    assert_ne!(
        small.get(),
        large.get(),
        "Changes page size must derive from visible terminal rows"
    );
}
