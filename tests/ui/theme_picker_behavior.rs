//! Theme picker behavior tests.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-009
//!
//! These tests verify the theme picker reducer behavior (open, navigate,
//! confirm, cancel, fallback) and the pure `theme_picker_view` projection.
//! The state-transition layer is deterministic per REQ-TECH-003.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::field_reassign_with_default
)]

use jefe::state::theme_picker_view::theme_picker_view;
use jefe::state::{AppEvent, AppState, ModalState};

fn picker_state(themes: &[&str], selected: usize) -> AppState {
    let mut state = AppState::default();
    state.modal = ModalState::ThemePicker {
        available_themes: themes
            .iter()
            .map(|&s| (s.to_string(), s.to_string()))
            .collect(),
        selected_index: selected,
        active_slug: String::new(),
    };
    state
}

// ============================================================================
// Open
// ============================================================================

#[test]
fn open_theme_picker_sets_modal_with_active_theme_selected() {
    let mut state = AppState::default();
    let themes = vec![
        ("green-screen".to_string(), "Green Screen".to_string()),
        ("dracula".to_string(), "Dracula".to_string()),
        ("atom-one-dark".to_string(), "Atom One Dark".to_string()),
    ];

    state = state.apply(AppEvent::OpenThemePicker {
        available_themes: themes,
        active_slug: "dracula".to_string(),
    });

    match state.modal {
        ModalState::ThemePicker { selected_index, .. } => {
            // Active theme (dracula) should be pre-selected.
            assert_eq!(selected_index, 1);
        }
        _ => panic!("expected ThemePicker modal"),
    }
}

#[test]
fn open_theme_picker_defaults_to_first_when_active_not_found() {
    let mut state = AppState::default();
    state = state.apply(AppEvent::OpenThemePicker {
        available_themes: vec![("green-screen".to_string(), "Green Screen".to_string())],
        active_slug: "nonexistent".to_string(),
    });

    match state.modal {
        ModalState::ThemePicker { selected_index, .. } => assert_eq!(selected_index, 0),
        _ => panic!("expected ThemePicker modal"),
    }
}

// ============================================================================
// Navigation
// ============================================================================

#[test]
fn navigate_down_increments_selection() {
    let mut state = picker_state(&["a", "b", "c"], 0);
    state = state.apply(AppEvent::ThemePickerNavigateDown);
    assert_eq!(
        theme_picker_view(&state).map(|(_, s)| s),
        Some(1),
        "selection should move to index 1"
    );
}

#[test]
fn navigate_up_decrements_selection() {
    let mut state = picker_state(&["a", "b", "c"], 2);
    state = state.apply(AppEvent::ThemePickerNavigateUp);
    assert_eq!(
        theme_picker_view(&state).map(|(_, s)| s),
        Some(1),
        "selection should move to index 1"
    );
}

#[test]
fn navigate_down_clamps_at_last_theme() {
    let mut state = picker_state(&["a", "b"], 1);
    state = state.apply(AppEvent::ThemePickerNavigateDown);
    assert_eq!(
        theme_picker_view(&state).map(|(_, s)| s),
        Some(1),
        "selection should stay clamped at last index"
    );
}

#[test]
fn navigate_up_clamps_at_first_theme() {
    let mut state = picker_state(&["a", "b"], 0);
    state = state.apply(AppEvent::ThemePickerNavigateUp);
    assert_eq!(
        theme_picker_view(&state).map(|(_, s)| s),
        Some(0),
        "selection should stay clamped at first index"
    );
}

#[test]
fn navigation_is_noop_when_picker_not_open() {
    let mut state = AppState::default();
    state = state.apply(AppEvent::ThemePickerNavigateDown);
    assert_eq!(state.modal, ModalState::None);
}

// ============================================================================
// Confirm / Cancel
// ============================================================================

#[test]
fn confirm_closes_the_picker_modal() {
    let mut state = picker_state(&["a", "b"], 1);
    state = state.apply(AppEvent::ThemePickerConfirm("b".to_string()));
    assert_eq!(state.modal, ModalState::None);
}

#[test]
fn close_theme_picker_cancels() {
    let mut state = picker_state(&["a", "b"], 0);
    state = state.apply(AppEvent::CloseThemePicker);
    assert_eq!(state.modal, ModalState::None);
}

// ============================================================================
// Pure projection
// ============================================================================

#[test]
fn view_marks_only_selected_row_as_selected() {
    let state = picker_state(&["a", "b", "c"], 1);
    let (rows, _) = theme_picker_view(&state).expect("picker open");
    assert!(!rows[0].selected);
    assert!(rows[1].selected);
    assert!(!rows[2].selected);
}

#[test]
fn view_returns_none_when_no_picker() {
    let state = AppState::default();
    assert!(theme_picker_view(&state).is_none());
}
