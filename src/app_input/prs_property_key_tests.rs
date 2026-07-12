//! PR-mode property-editor key dispatch tests (issue #175).
//!
//! Extracted from `prs_key_tests.rs` to keep that file under the per-file
//! line limit. Tests the Shift-letter open keys, overlay key routing, and
//! modal suppression for the PR property editor.

use super::*;
use jefe::state::{PrPropertyEditorState, PrPropertyKind, PullRequestsState, ScreenMode};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(KeyEventKind::Press, code)
}

fn prs_base_state() -> AppState {
    AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        prs_state: PullRequestsState {
            active: true,
            pr_focus: PrFocus::PrList,
            ..PullRequestsState::default()
        },
        ..AppState::default()
    }
}

fn prs_detail_body_state() -> AppState {
    let mut state = prs_base_state();
    state.prs_state.pr_focus = PrFocus::PrDetail;
    state.prs_state.detail_subfocus = PrDetailSubfocus::Body;
    state
}

fn prs_state_with_detail_subfocus(subfocus: PrDetailSubfocus) -> AppState {
    let mut state = prs_base_state();
    state.prs_state.pr_focus = PrFocus::PrDetail;
    state.prs_state.detail_subfocus = subfocus;
    state
}

fn prs_state_with_property_editor() -> AppState {
    let mut state = prs_detail_body_state();
    state.prs_state.property_editor = Some(PrPropertyEditorState {
        kind: PrPropertyKind::Labels,
        options: vec![],
        selected_index: 0,
        title_text: String::new(),
        title_cursor: 0,
        error: None,
        baseline: Vec::new(),
        loading_failed: false,
        load_request_id: 0,
    });
    state
}

// ═══════════════════════════════════════════════════════════════════════
// Property Editor — Open keys (issue #175)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_pr_shift_l_opens_labels_editor() {
    let state = prs_detail_body_state();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('L')));
    assert!(matches!(
        event,
        Some(AppEvent::PrOpenPropertyEditor {
            kind: PrPropertyKind::Labels
        })
    ));
}

#[test]
fn test_pr_shift_a_opens_assignees_editor() {
    let state = prs_detail_body_state();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('A')));
    assert!(matches!(
        event,
        Some(AppEvent::PrOpenPropertyEditor {
            kind: PrPropertyKind::Assignees
        })
    ));
}

#[test]
fn test_pr_shift_m_opens_milestone_editor() {
    let state = prs_detail_body_state();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('M')));
    assert!(matches!(
        event,
        Some(AppEvent::PrOpenPropertyEditor {
            kind: PrPropertyKind::Milestone
        })
    ));
}

#[test]
fn test_pr_shift_t_opens_title_editor() {
    let state = prs_detail_body_state();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('T')));
    assert!(matches!(
        event,
        Some(AppEvent::PrOpenPropertyEditor {
            kind: PrPropertyKind::Title
        })
    ));
}

#[test]
fn test_pr_shift_w_opens_state_editor() {
    let state = prs_detail_body_state();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('W')));
    assert!(matches!(
        event,
        Some(AppEvent::PrOpenPropertyEditor {
            kind: PrPropertyKind::State
        })
    ));
}

/// Property editor keys only fire on Body subfocus, not Comment subfocus.
#[test]
fn test_pr_property_keys_noop_on_comment_subfocus() {
    let state = prs_state_with_detail_subfocus(PrDetailSubfocus::Comment(0));
    let l = resolve_prs_key_event(&state, &key(KeyCode::Char('L')));
    assert!(l.is_none(), "L on Comment subfocus should be None");
}

// ═══════════════════════════════════════════════════════════════════════
// Property Editor — Overlay key routing (issue #175)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_pr_property_editor_up_navigates() {
    let state = prs_state_with_property_editor();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Up));
    assert!(matches!(event, Some(AppEvent::PrPropertyEditorNavigateUp)));
}

#[test]
fn test_pr_property_editor_down_navigates() {
    let state = prs_state_with_property_editor();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Down));
    assert!(matches!(
        event,
        Some(AppEvent::PrPropertyEditorNavigateDown)
    ));
}

#[test]
fn test_pr_property_editor_space_toggles() {
    let state = prs_state_with_property_editor();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char(' ')));
    assert!(matches!(event, Some(AppEvent::PrPropertyEditorToggle)));
}

#[test]
fn test_pr_property_editor_enter_confirms() {
    let state = prs_state_with_property_editor();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Enter));
    assert!(matches!(event, Some(AppEvent::PrPropertyEditorConfirm)));
}

#[test]
fn test_pr_property_editor_esc_cancels() {
    let state = prs_state_with_property_editor();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Esc));
    assert!(matches!(event, Some(AppEvent::PrPropertyEditorCancel)));
}

#[test]
fn test_pr_property_editor_char_routes_to_title() {
    let state = prs_state_with_property_editor();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('x')));
    assert!(matches!(
        event,
        Some(AppEvent::PrPropertyEditorTitleChar('x'))
    ));
}

#[test]
fn test_pr_property_editor_backspace_routes_to_title() {
    let state = prs_state_with_property_editor();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Backspace));
    assert!(matches!(
        event,
        Some(AppEvent::PrPropertyEditorTitleBackspace)
    ));
}

#[test]
fn test_pr_property_editor_delete_routes_to_title() {
    let state = prs_state_with_property_editor();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Delete));
    assert!(matches!(event, Some(AppEvent::PrPropertyEditorTitleDelete)));
}

#[test]
fn test_pr_property_editor_left_routes_to_title() {
    let state = prs_state_with_property_editor();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Left));
    assert!(matches!(
        event,
        Some(AppEvent::PrPropertyEditorTitleCursorLeft)
    ));
}

#[test]
fn test_pr_property_editor_right_routes_to_title() {
    let state = prs_state_with_property_editor();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Right));
    assert!(matches!(
        event,
        Some(AppEvent::PrPropertyEditorTitleCursorRight)
    ));
}

/// Property editor is modal: char keys route to title editing, other keys
/// are suppressed (None).
#[test]
fn test_pr_property_editor_routes_chars_and_suppresses_others() {
    let state = prs_state_with_property_editor();
    let e = resolve_prs_key_event(&state, &key(KeyCode::Char('e')));
    assert!(
        matches!(e, Some(AppEvent::PrPropertyEditorTitleChar('e'))),
        "property editor should route 'e' to title editing, got {e:?}"
    );
    let bs = resolve_prs_key_event(&state, &key(KeyCode::Backspace));
    assert!(
        matches!(bs, Some(AppEvent::PrPropertyEditorTitleBackspace)),
        "property editor should route Backspace to title editing, got {bs:?}"
    );
    let tab = resolve_prs_key_event(&state, &key(KeyCode::Tab));
    assert!(tab.is_none(), "property editor should suppress Tab");
}
