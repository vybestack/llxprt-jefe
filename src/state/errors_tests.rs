//! Unit tests for the errors-mode state and reducer (issue #292).

use crate::domain::ErrorSource;
use crate::messages::{ErrorsMessage, NavDir, ScrollDir};
use crate::state::events::AppEvent;
use crate::state::{AppState, ErrorsFocus, ScreenMode};

/// In-place apply helper to avoid the take/replace dance on owned AppState.
fn apply_in_place(state: &mut AppState, event: AppEvent) {
    let old = std::mem::take(state);
    *state = old.apply(event);
}

/// A fresh `ErrorsState` starts empty with no selection.
#[test]
fn default_errors_state_is_empty() {
    let state = AppState::default();
    assert!(state.errors_state.is_empty());
    assert_eq!(state.errors_state.count(), 0);
    assert!(state.errors_state.selected_index.is_none());
}

/// Pushing an error adds it at the head and selects it.
#[test]
fn push_adds_to_head_and_selects() {
    let mut state = AppState::default();
    state.errors_state.push(
        "First error".to_string(),
        "detail one".to_string(),
        ErrorSource::Issues,
        "2025-01-01T00:00:00Z".to_string(),
    );
    assert_eq!(state.errors_state.count(), 1);
    assert_eq!(state.errors_state.selected_index, Some(0));
    assert_eq!(
        state
            .errors_state
            .last_error()
            .map_or(String::new(), |e| e.title.clone()),
        "First error"
    );

    state.errors_state.push(
        "Second error".to_string(),
        "detail two".to_string(),
        ErrorSource::PullRequests,
        "2025-01-01T00:00:01Z".to_string(),
    );
    assert_eq!(state.errors_state.count(), 2);
    assert_eq!(
        state
            .errors_state
            .last_error()
            .map_or(String::new(), |e| e.title.clone()),
        "Second error"
    );
}

/// Pushing more than `ERROR_STORE_CAPACITY` errors evicts the oldest.
#[test]
fn push_evicts_oldest_at_capacity() {
    let mut state = AppState::default();
    for i in 0..crate::domain::ERROR_STORE_CAPACITY + 5 {
        state.errors_state.push(
            format!("error {i}"),
            format!("detail {i}"),
            ErrorSource::Other,
            "ts".to_string(),
        );
    }
    assert_eq!(
        state.errors_state.count(),
        crate::domain::ERROR_STORE_CAPACITY
    );
    // The newest error should be the last pushed.
    let last_title = state
        .errors_state
        .last_error()
        .map_or(String::new(), |e| e.title.clone());
    assert_eq!(
        last_title,
        format!("error {}", crate::domain::ERROR_STORE_CAPACITY + 4)
    );
}

/// `EnterErrorsMode` switches screen mode, activates state, and focuses list.
#[test]
fn enter_errors_mode_sets_screen_and_focus() {
    let mut state = AppState::default();
    apply_in_place(&mut state, AppEvent::EnterErrorsMode);
    assert_eq!(state.screen_mode, ScreenMode::DashboardErrors);
    assert!(state.errors_state.active);
    assert_eq!(state.errors_state.focus, ErrorsFocus::ErrorList);
}

/// `EnterErrorsMode` saves prior focus; `ExitErrorsMode` restores it.
#[test]
fn enter_exit_restores_focus() {
    let mut state = AppState::default();
    state.pane_focus = crate::state::PaneFocus::Repositories;
    state.selected_repository_index = Some(0);

    apply_in_place(&mut state, AppEvent::EnterErrorsMode);
    assert!(state.errors_state.prior_agent_focus.is_some());

    apply_in_place(&mut state, AppEvent::ExitErrorsMode);
    assert_eq!(state.screen_mode, ScreenMode::Dashboard);
    assert!(!state.errors_state.active);
    // Prior focus restored.
    assert_eq!(state.pane_focus, crate::state::PaneFocus::Repositories);
}

/// Navigation down/up moves the selected error index.
#[test]
fn navigate_down_up_moves_selection() {
    let mut state = AppState::default();
    for i in 0..5 {
        state.errors_state.push(
            format!("err {i}"),
            format!("detail {i}"),
            ErrorSource::Other,
            "ts".to_string(),
        );
    }
    // Errors are newest-first, so index 0 = "err 4", index 4 = "err 0".
    apply_in_place(&mut state, AppEvent::EnterErrorsMode);

    // Start at index 0 (newest).
    assert_eq!(state.errors_state.selected_index, Some(0));

    // Navigate down to index 1.
    state.apply_errors_message(ErrorsMessage::Navigate(NavDir::Down));
    assert_eq!(state.errors_state.selected_index, Some(1));

    // Navigate down further to index 2.
    state.apply_errors_message(ErrorsMessage::Navigate(NavDir::Down));
    assert_eq!(state.errors_state.selected_index, Some(2));

    // Navigate up back to index 1.
    state.apply_errors_message(ErrorsMessage::Navigate(NavDir::Up));
    assert_eq!(state.errors_state.selected_index, Some(1));
}

/// Navigation clamps at the last item.
#[test]
fn navigate_clamps_at_end() {
    let mut state = AppState::default();
    state
        .errors_state
        .push("a".into(), "da".into(), ErrorSource::Other, "ts".into());
    state
        .errors_state
        .push("b".into(), "db".into(), ErrorSource::Other, "ts".into());
    apply_in_place(&mut state, AppEvent::EnterErrorsMode);

    state.apply_errors_message(ErrorsMessage::Navigate(NavDir::End));
    assert_eq!(state.errors_state.selected_index, Some(1));

    // Down past end stays at last.
    state.apply_errors_message(ErrorsMessage::Navigate(NavDir::Down));
    assert_eq!(state.errors_state.selected_index, Some(1));
}

/// Navigation home goes to index 0.
#[test]
fn navigate_home_goes_to_first() {
    let mut state = AppState::default();
    for i in 0..3 {
        state.errors_state.push(
            format!("e{i}"),
            format!("d{i}"),
            ErrorSource::Other,
            "ts".into(),
        );
    }
    apply_in_place(&mut state, AppEvent::EnterErrorsMode);
    state.errors_state.selected_index = Some(2);

    state.apply_errors_message(ErrorsMessage::Navigate(NavDir::Home));
    assert_eq!(state.errors_state.selected_index, Some(0));
}

/// Cycle focus rotates RepoList → ErrorList → ErrorDetail → RepoList.
#[test]
fn cycle_focus_rotates() {
    let mut state = AppState::default();
    state
        .errors_state
        .push("e".into(), "d".into(), ErrorSource::Other, "ts".into());
    apply_in_place(&mut state, AppEvent::EnterErrorsMode);
    assert_eq!(state.errors_state.focus, ErrorsFocus::ErrorList);

    state.apply_errors_message(ErrorsMessage::CycleFocus);
    assert_eq!(state.errors_state.focus, ErrorsFocus::ErrorDetail);

    state.apply_errors_message(ErrorsMessage::CycleFocus);
    assert_eq!(state.errors_state.focus, ErrorsFocus::RepoList);

    state.apply_errors_message(ErrorsMessage::CycleFocus);
    assert_eq!(state.errors_state.focus, ErrorsFocus::ErrorList);
}

/// Enter on the error list moves focus to detail.
#[test]
fn enter_on_list_moves_to_detail() {
    let mut state = AppState::default();
    state
        .errors_state
        .push("e".into(), "d".into(), ErrorSource::Other, "ts".into());
    apply_in_place(&mut state, AppEvent::EnterErrorsMode);

    state.apply_errors_message(ErrorsMessage::Enter);
    assert_eq!(state.errors_state.focus, ErrorsFocus::ErrorDetail);
}

/// ClearAll empties the error log and resets selection.
#[test]
fn clear_all_empties_errors() {
    let mut state = AppState::default();
    state
        .errors_state
        .push("e1".into(), "d1".into(), ErrorSource::Other, "ts".into());
    state
        .errors_state
        .push("e2".into(), "d2".into(), ErrorSource::Other, "ts".into());

    state.apply_errors_message(ErrorsMessage::ClearAll);
    assert!(state.errors_state.is_empty());
    assert!(state.errors_state.selected_index.is_none());
}

/// Capture dedup: same error text doesn't create duplicate entries.
#[test]
fn capture_global_dedup() {
    let mut state = AppState::default();
    let pushed1 = state
        .errors_state
        .capture_global("disk full", ErrorSource::Persistence, "ts1");
    assert!(pushed1);
    assert_eq!(state.errors_state.count(), 1);

    // Same text → not captured again.
    let pushed2 = state
        .errors_state
        .capture_global("disk full", ErrorSource::Persistence, "ts2");
    assert!(!pushed2);
    assert_eq!(state.errors_state.count(), 1);

    // Different text → captured.
    let pushed3 = state
        .errors_state
        .capture_global("network error", ErrorSource::Other, "ts3");
    assert!(pushed3);
    assert_eq!(state.errors_state.count(), 2);
}

/// `capture_global` resets the tracker when called with `reset_*_tracker`.
#[test]
fn reset_global_tracker_allows_recapture() {
    let mut state = AppState::default();
    state
        .errors_state
        .capture_global("err", ErrorSource::Other, "ts");
    state.errors_state.reset_global_tracker();
    // After reset, same text is captured again.
    let pushed = state
        .errors_state
        .capture_global("err", ErrorSource::Other, "ts");
    assert!(pushed);
}

/// Sequence numbers are monotonically increasing.
#[test]
fn seq_numbers_increase() {
    let mut state = AppState::default();
    state
        .errors_state
        .push("a".into(), "da".into(), ErrorSource::Other, "ts".into());
    state
        .errors_state
        .push("b".into(), "db".into(), ErrorSource::Other, "ts".into());
    let seqs: Vec<u64> = state.errors_state.errors.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, vec![2, 1]);
}

/// Scroll detail clamps at bounds.
#[test]
fn scroll_detail_clamps() {
    let mut state = AppState::default();
    let long_detail = (0..50)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    state
        .errors_state
        .push("e".into(), long_detail, ErrorSource::Other, "ts".into());
    apply_in_place(&mut state, AppEvent::EnterErrorsMode);
    state.errors_state.detail_viewport_rows = 10;

    // Scroll down should increase offset.
    state.apply_errors_message(ErrorsMessage::ScrollDetail(ScrollDir::Down));
    assert_eq!(state.errors_state.detail_scroll_offset, 1);

    // Page down should jump by VIEWPORT_PAGE_JUMP.
    state.apply_errors_message(ErrorsMessage::ScrollDetail(ScrollDir::PageDown));
    assert!(state.errors_state.detail_scroll_offset > 1);

    // Scroll up from 0 stays at 0.
    state.errors_state.detail_scroll_offset = 0;
    state.apply_errors_message(ErrorsMessage::ScrollDetail(ScrollDir::Up));
    assert_eq!(state.errors_state.detail_scroll_offset, 0);
}
