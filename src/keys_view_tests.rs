//! Pure Keys-editor layout projection tests.

use crate::keys_view::project_keys_view;
use crate::messages::KeysEditorMessage;
use crate::state::KeysEditorState;

fn editor() -> KeysEditorState {
    KeysEditorState::from_snapshot(&crate::action_projection::test_snapshot(), None)
}

#[test]
fn normal_and_focused_views_include_action_identity_and_controls() {
    let mut state = editor();
    state.apply(KeysEditorMessage::MoveDown);
    state.apply(KeysEditorMessage::BeginEdit);
    let view = project_keys_view(&state, 120, 36);
    let rendered = view.lines.join("\n");
    assert_eq!(view.title, "Keys - Keyboard Bindings");
    assert!(rendered.contains("> core.open-keys"));
    assert!(rendered.contains("Editing core.open-keys"));
    assert!(view.footer.contains("Esc Back"));
    assert!(view.footer.contains("Ctrl-Q Quit"));

    let target = view
        .action_targets
        .iter()
        .find(|target| target.action.as_str() == "core.open-keys")
        .unwrap_or_else(|| panic!("visible core.open-keys row must carry its ActionId"));
    assert_eq!(view.lines[target.line].chars().next(), Some('>'));
    assert_eq!(target.columns.start, 0);
    assert!(target.columns.end > target.columns.start);
}

#[test]
fn clipped_rows_emit_targets_only_for_their_rendered_lines() {
    let mut state = editor();
    state.apply(KeysEditorMessage::MoveEnd);
    let view = project_keys_view(&state, 44, 10);

    assert!(!view.action_targets.is_empty());
    assert!(view.action_targets.len() <= view.lines.len());
    assert!(view.action_targets.iter().all(|target| {
        target.line < view.lines.len()
            && target.columns.end <= view.lines[target.line].chars().count()
    }));
    assert!(view.action_targets.iter().any(|target| {
        target.action
            == state
                .selected_row()
                .unwrap_or_else(|| panic!("fixture has a selected row"))
                .action
    }));
}

#[test]
fn invalid_dirty_and_recovery_states_are_explicit() {
    let mut state = editor();
    state.apply(KeysEditorMessage::MoveDown);
    state.apply(KeysEditorMessage::Unbind);
    state.apply(KeysEditorMessage::ValidationFailed(
        "KEY-E401: conflict".to_owned(),
    ));
    let invalid = project_keys_view(&state, 100, 30).lines.join("\n");
    assert!(invalid.contains("KEY-E401: conflict"));
    assert!(invalid.contains("Unsaved changes"));
    assert!(invalid.contains("Save disabled"));

    let recovery = KeysEditorState::from_snapshot(
        &crate::action_projection::test_snapshot(),
        Some("KEY-E401: malformed override".to_owned()),
    );
    assert!(
        project_keys_view(&recovery, 100, 30)
            .lines
            .join("\n")
            .contains("Recovery")
    );
}

#[test]
fn tiny_layout_always_renders_back_and_ctrl_q() {
    let view = project_keys_view(&editor(), 44, 10);
    assert_eq!(view.title, "Keys (compact)");
    assert!(view.footer.contains("Esc Back"));
    assert!(view.footer.contains("Ctrl-Q Quit"));
    assert!(view.lines.len() <= 5);
}
