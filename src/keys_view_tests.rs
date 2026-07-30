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
