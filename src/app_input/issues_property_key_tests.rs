//! Issues-mode property-editor key dispatch tests (issue #175).
//!
//! Extracted from `issues_key_tests.rs` to keep that file under the per-file
//! line limit. Tests the Shift-letter open keys, overlay key routing, and
//! modal suppression for the issue property editor.

use super::*;
use iocraft::prelude::{KeyCode, KeyEventKind};
use jefe::state::{
    DetailSubfocus, IssueFocus, IssuePropertyEditorState, IssuePropertyKind, IssuesState,
    ScreenMode,
};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(KeyEventKind::Press, code)
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

fn issues_detail_body_state() -> AppState {
    issues_state_with_detail_subfocus(DetailSubfocus::Body)
}

// ═══════════════════════════════════════════════════════════════════════
// Property Editor — Open keys (issue #175)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_shift_l_opens_labels_editor() {
    let state = issues_detail_body_state();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('L')));
    assert!(matches!(
        event,
        Some(AppEvent::IssueOpenPropertyEditor {
            kind: IssuePropertyKind::Labels
        })
    ));
}

#[test]
fn test_shift_a_opens_assignees_editor() {
    let state = issues_detail_body_state();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('A')));
    assert!(matches!(
        event,
        Some(AppEvent::IssueOpenPropertyEditor {
            kind: IssuePropertyKind::Assignees
        })
    ));
}

#[test]
fn test_shift_m_opens_milestone_editor() {
    let state = issues_detail_body_state();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('M')));
    assert!(matches!(
        event,
        Some(AppEvent::IssueOpenPropertyEditor {
            kind: IssuePropertyKind::Milestone
        })
    ));
}

#[test]
fn test_shift_t_opens_title_editor() {
    let state = issues_detail_body_state();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('T')));
    assert!(matches!(
        event,
        Some(AppEvent::IssueOpenPropertyEditor {
            kind: IssuePropertyKind::Title
        })
    ));
}

#[test]
fn test_shift_y_opens_type_editor() {
    let state = issues_detail_body_state();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('Y')));
    assert!(matches!(
        event,
        Some(AppEvent::IssueOpenPropertyEditor {
            kind: IssuePropertyKind::Type
        })
    ));
}

#[test]
fn test_shift_w_opens_state_editor() {
    let state = issues_detail_body_state();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('W')));
    assert!(matches!(
        event,
        Some(AppEvent::IssueOpenPropertyEditor {
            kind: IssuePropertyKind::State
        })
    ));
}

/// Property editor keys only fire on Body subfocus, not Comment subfocus.
#[test]
fn test_property_keys_noop_on_comment_subfocus() {
    let state = issues_state_with_detail_subfocus(DetailSubfocus::Comment(0));
    let l = resolve_issues_key_event(&state, &key(KeyCode::Char('L')));
    assert!(l.is_none(), "L on Comment subfocus should be None");
}

// ═══════════════════════════════════════════════════════════════════════
// Property Editor — Overlay key routing (issue #175)
// ═══════════════════════════════════════════════════════════════════════

/// Helper: issues state with the property editor already open.
fn issues_state_with_property_editor() -> AppState {
    let mut state = issues_detail_body_state();
    state.issues_state.property_editor = Some(IssuePropertyEditorState {
        kind: IssuePropertyKind::Labels,
        options: vec![],
        selected_index: 0,
        title_text: String::new(),
        title_cursor: 0,
        error: None,
        baseline: Vec::new(),
        loading_failed: false,
        options_loading: false,
        load_request_id: 0,
    });
    state
}

#[test]
fn test_property_editor_up_navigates() {
    let state = issues_state_with_property_editor();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Up));
    assert!(matches!(
        event,
        Some(AppEvent::IssuePropertyEditorNavigateUp)
    ));
}

#[test]
fn test_property_editor_down_navigates() {
    let state = issues_state_with_property_editor();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Down));
    assert!(matches!(
        event,
        Some(AppEvent::IssuePropertyEditorNavigateDown)
    ));
}

#[test]
fn test_property_editor_space_toggles() {
    let state = issues_state_with_property_editor();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char(' ')));
    assert!(matches!(event, Some(AppEvent::IssuePropertyEditorToggle)));
}

#[test]
fn test_title_property_editor_routes_spaces_at_every_cursor_position() {
    for (text, cursor) in [("word", 0), ("word", 2), ("word", 4)] {
        let mut state = issues_state_with_property_editor();
        let Some(editor) = state.issues_state.property_editor.as_mut() else {
            panic!("test property editor should be present");
        };
        editor.kind = IssuePropertyKind::Title;
        editor.title_text = text.to_string();
        editor.title_cursor = cursor;

        let event = resolve_issues_key_event(&state, &key(KeyCode::Char(' ')));
        assert!(
            matches!(event, Some(AppEvent::IssuePropertyEditorTitleChar(' '))),
            "title space at cursor {cursor} should edit the title, got {event:?}"
        );
    }
}

#[test]
fn test_property_editor_enter_confirms() {
    let state = issues_state_with_property_editor();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Enter));
    assert!(matches!(event, Some(AppEvent::IssuePropertyEditorConfirm)));
}

#[test]
fn test_property_editor_esc_cancels() {
    let state = issues_state_with_property_editor();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Esc));
    assert!(matches!(event, Some(AppEvent::IssuePropertyEditorCancel)));
}

#[test]
fn test_property_editor_char_routes_to_title() {
    let state = issues_state_with_property_editor();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('x')));
    assert!(matches!(
        event,
        Some(AppEvent::IssuePropertyEditorTitleChar('x'))
    ));
}

#[test]
fn test_property_editor_backspace_routes_to_title() {
    let state = issues_state_with_property_editor();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Backspace));
    assert!(matches!(
        event,
        Some(AppEvent::IssuePropertyEditorTitleBackspace)
    ));
}

#[test]
fn test_property_editor_delete_routes_to_title() {
    let state = issues_state_with_property_editor();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Delete));
    assert!(matches!(
        event,
        Some(AppEvent::IssuePropertyEditorTitleDelete)
    ));
}

#[test]
fn test_property_editor_left_routes_to_title() {
    let state = issues_state_with_property_editor();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Left));
    assert!(matches!(
        event,
        Some(AppEvent::IssuePropertyEditorTitleCursorLeft)
    ));
}

#[test]
fn test_property_editor_right_routes_to_title() {
    let state = issues_state_with_property_editor();
    let event = resolve_issues_key_event(&state, &key(KeyCode::Right));
    assert!(matches!(
        event,
        Some(AppEvent::IssuePropertyEditorTitleCursorRight)
    ));
}

/// Property editor is modal: char keys route to title editing, other keys
/// are suppressed (None).
#[test]
fn test_property_editor_routes_chars_and_suppresses_others() {
    let state = issues_state_with_property_editor();
    // 'e' routes to title char editing (not suppressed)
    let e = resolve_issues_key_event(&state, &key(KeyCode::Char('e')));
    assert!(
        matches!(e, Some(AppEvent::IssuePropertyEditorTitleChar('e'))),
        "property editor should route 'e' to title editing, got {e:?}"
    );
    // Backspace routes to title backspace
    let bs = resolve_issues_key_event(&state, &key(KeyCode::Backspace));
    assert!(
        matches!(bs, Some(AppEvent::IssuePropertyEditorTitleBackspace)),
        "property editor should route Backspace to title editing, got {bs:?}"
    );
    // Tab is still suppressed
    let tab = resolve_issues_key_event(&state, &key(KeyCode::Tab));
    assert!(tab.is_none(), "property editor should suppress Tab");
}
