//! Theme picker behavior tests.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-009
//!
//! These tests verify the theme picker reducer behavior (open, navigate,
//! confirm, cancel, fallback) and the pure `theme_picker_view` projection.
//! The state-transition layer is deterministic per REQ-TECH-003.

use jefe::state::theme_picker_view::{theme_picker_override_view, theme_picker_view};
use jefe::state::{AppEvent, AppState, ModalState};

fn picker_state(themes: &[&str], selected: usize) -> AppState {
    AppState {
        modal: ModalState::ThemePicker {
            available_themes: themes
                .iter()
                .map(|&s| (s.to_string(), s.to_string()))
                .collect(),
            selected_index: selected,
            active_slug: String::new(),
            override_theme: false,
        },
        ..AppState::default()
    }
}

fn picker_state_with_active(themes: &[&str], selected: usize, active: &str) -> AppState {
    AppState {
        modal: ModalState::ThemePicker {
            available_themes: themes
                .iter()
                .map(|&s| (s.to_string(), s.to_string()))
                .collect(),
            selected_index: selected,
            active_slug: active.to_string(),
            override_theme: false,
        },
        ..AppState::default()
    }
}

/// A picker state whose in-dialog override checkbox starts ON (issue #179).
fn picker_state_with_override(themes: &[&str], selected: usize, override_theme: bool) -> AppState {
    AppState {
        modal: ModalState::ThemePicker {
            available_themes: themes
                .iter()
                .map(|&s| (s.to_string(), s.to_string()))
                .collect(),
            selected_index: selected,
            active_slug: String::new(),
            override_theme,
        },
        ..AppState::default()
    }
}

// ============================================================================
// Open
// ============================================================================

#[test]
fn open_theme_picker_sets_modal_with_active_theme_selected() {
    let state = AppState::default().apply(AppEvent::OpenThemePicker {
        available_themes: vec![
            ("green-screen".to_string(), "Green Screen".to_string()),
            ("dracula".to_string(), "Dracula".to_string()),
            ("atom-one-dark".to_string(), "Atom One Dark".to_string()),
        ],
        active_slug: "dracula".to_string(),
    });

    match state.modal {
        ModalState::ThemePicker { selected_index, .. } => {
            assert_eq!(selected_index, 1);
        }
        _ => panic!("expected ThemePicker modal"),
    }
}

#[test]
fn open_theme_picker_defaults_to_first_when_active_not_found() {
    let state = AppState::default().apply(AppEvent::OpenThemePicker {
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
    let state = picker_state(&["a", "b", "c"], 0).apply(AppEvent::ThemePickerNavigateDown);
    let rows = theme_picker_view(&state).unwrap_or_else(|| panic!("picker open"));
    assert!(!rows[0].selected, "row 0 should not be selected");
    assert!(rows[1].selected, "row 1 should be selected");
}

#[test]
fn navigate_up_decrements_selection() {
    let state = picker_state(&["a", "b", "c"], 2).apply(AppEvent::ThemePickerNavigateUp);
    let rows = theme_picker_view(&state).unwrap_or_else(|| panic!("picker open"));
    assert!(rows[1].selected, "row 1 should be selected");
    assert!(!rows[2].selected, "row 2 should not be selected");
}

#[test]
fn navigate_down_clamps_at_last_theme() {
    let state = picker_state(&["a", "b"], 1).apply(AppEvent::ThemePickerNavigateDown);
    let rows = theme_picker_view(&state).unwrap_or_else(|| panic!("picker open"));
    assert!(
        rows[1].selected,
        "selection should stay clamped at last index"
    );
}

#[test]
fn navigate_up_clamps_at_first_theme() {
    let state = picker_state(&["a", "b"], 0).apply(AppEvent::ThemePickerNavigateUp);
    let rows = theme_picker_view(&state).unwrap_or_else(|| panic!("picker open"));
    assert!(
        rows[0].selected,
        "selection should stay clamped at first index"
    );
}

#[test]
fn navigation_is_noop_when_picker_not_open() {
    let state = AppState::default().apply(AppEvent::ThemePickerNavigateDown);
    assert_eq!(state.modal, ModalState::None);
}

// ============================================================================
// Confirm / Cancel
// ============================================================================

#[test]
fn confirm_closes_the_picker_modal() {
    let state = picker_state(&["a", "b"], 1).apply(AppEvent::ThemePickerConfirm);
    assert_eq!(state.modal, ModalState::None);
}

#[test]
fn close_theme_picker_cancels() {
    let state = picker_state(&["a", "b"], 0).apply(AppEvent::CloseThemePicker);
    assert_eq!(state.modal, ModalState::None);
}

// ============================================================================
// Pure projection
// ============================================================================

#[test]
fn view_marks_only_selected_row_as_selected() {
    let state = picker_state(&["a", "b", "c"], 1);
    let rows = theme_picker_view(&state).unwrap_or_else(|| panic!("picker open"));
    assert!(!rows[0].selected);
    assert!(rows[1].selected);
    assert!(!rows[2].selected);
}

#[test]
fn view_returns_none_when_no_picker() {
    let state = AppState::default();
    assert!(theme_picker_view(&state).is_none());
}

#[test]
fn view_marks_active_theme_independently_of_selection() {
    // Active slug is "b" but selection is on "a".
    let state = picker_state_with_active(&["a", "b"], 0, "b");
    let rows = theme_picker_view(&state).unwrap_or_else(|| panic!("picker open"));
    assert!(rows[0].selected, "row 0 should be selected");
    assert!(!rows[0].active, "row 0 should not be active");
    assert!(!rows[1].selected, "row 1 should not be selected");
    assert!(rows[1].active, "row 1 should be active");
}

// ============================================================================
// Override theme toggle (issue #179)
// ============================================================================

#[test]
fn toggle_override_flips_override_flag() {
    let state = picker_state(&["a", "b"], 0).apply(AppEvent::ThemePickerToggleOverride);
    let override_val = theme_picker_override_view(&state)
        .unwrap_or_else(|| panic!("picker should be open with override field"));
    assert!(
        override_val,
        "toggle should flip override_theme from false to true"
    );
}

#[test]
fn toggle_override_twice_returns_to_false() {
    let state = picker_state(&["a", "b"], 0)
        .apply(AppEvent::ThemePickerToggleOverride)
        .apply(AppEvent::ThemePickerToggleOverride);
    let override_val =
        theme_picker_override_view(&state).unwrap_or_else(|| panic!("picker should still be open"));
    assert!(
        !override_val,
        "double toggle should return override_theme to false"
    );
}

#[test]
fn toggle_override_is_noop_when_picker_not_open() {
    // Explicit preconditions: no modal and override off. The toggle must be a
    // no-op that leaves both unchanged.
    let state = AppState {
        modal: ModalState::None,
        override_agent_theme: false,
        ..AppState::default()
    };
    let next = state.apply(AppEvent::ThemePickerToggleOverride);
    assert_eq!(
        next.modal,
        ModalState::None,
        "toggle should be a no-op when picker is not open"
    );
    assert!(
        !next.override_agent_theme,
        "app-level override flag must be unchanged when the picker is closed"
    );
}

#[test]
fn open_picker_initializes_override_from_app_state() {
    // AppState.override_agent_theme is the runtime mirror. When the picker
    // opens, its override_theme should be initialized from it.
    let base = AppState {
        override_agent_theme: true,
        ..AppState::default()
    };
    let state = base.apply(AppEvent::OpenThemePicker {
        available_themes: vec![("a".to_string(), "A".to_string())],
        active_slug: "a".to_string(),
    });
    let override_val =
        theme_picker_override_view(&state).unwrap_or_else(|| panic!("picker should be open"));
    assert!(
        override_val,
        "picker override_theme must be initialized from AppState.override_agent_theme"
    );
}

#[test]
fn confirm_closes_picker_after_override_toggle() {
    // Toggling override then confirming still closes the modal (existing
    // behavior unchanged by the new field).
    let state = picker_state(&["a", "b"], 0)
        .apply(AppEvent::ThemePickerToggleOverride)
        .apply(AppEvent::ThemePickerConfirm);
    assert_eq!(state.modal, ModalState::None);
}

#[test]
fn confirm_commits_override_to_app_state() {
    // The in-dialog override toggle is committed to the runtime mirror on
    // confirm (issue #179). The reducer owns this transition deterministically.
    let state = picker_state(&["a", "b"], 0)
        .apply(AppEvent::ThemePickerToggleOverride)
        .apply(AppEvent::ThemePickerConfirm);
    assert!(
        state.override_agent_theme,
        "confirm must commit the toggled override to AppState"
    );
}

#[test]
fn confirm_commits_override_false_to_app_state() {
    // Reverse direction (issue #179 symmetry): starting with the in-dialog
    // override ON, toggling it OFF and confirming must commit false.
    let state = picker_state_with_override(&["a", "b"], 0, true)
        .apply(AppEvent::ThemePickerToggleOverride)
        .apply(AppEvent::ThemePickerConfirm);
    assert!(
        !state.override_agent_theme,
        "confirm must commit the toggled-off override (false) to AppState"
    );
}

#[test]
fn cancel_does_not_commit_override_toggle() {
    // Cancel discards the in-dialog toggle; the runtime mirror is unchanged.
    let state = picker_state(&["a", "b"], 0)
        .apply(AppEvent::ThemePickerToggleOverride)
        .apply(AppEvent::CloseThemePicker);
    assert_eq!(state.modal, ModalState::None);
    assert!(
        !state.override_agent_theme,
        "cancel must not commit the in-dialog override toggle"
    );
}
