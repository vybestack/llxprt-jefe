//! Deterministic Keys-editor reducer tests.

use crate::messages::KeysEditorMessage;
use crate::state::{KeysBindingEdit, KeysConfirmFocus, KeysEditorState, KeysValidation};

fn editor() -> KeysEditorState {
    KeysEditorState::from_snapshot(&crate::action_projection::test_snapshot(), None)
}

#[test]
fn snapshot_projection_is_ordered_and_protected_controls_are_read_only() {
    let mut state = editor();
    assert_eq!(
        state.selected_row().map(|row| row.action.as_str()),
        Some("core.emergency-exit")
    );
    state.apply(KeysEditorMessage::Unbind);
    assert!(!state.is_dirty());
    assert!(matches!(state.validation, KeysValidation::Invalid(_)));
    assert!(
        state
            .validation_message()
            .is_some_and(|message| message.contains("KEY-E401"))
    );
}

#[test]
fn edit_parses_canonical_chords_then_waits_for_complete_validation() {
    let mut state = editor();
    state.apply(KeysEditorMessage::MoveDown);
    state.apply(KeysEditorMessage::BeginEdit);
    state.apply(KeysEditorMessage::EditBackspace);
    for character in "Alt+K Ctrl+J".chars() {
        state.apply(KeysEditorMessage::EditChar(character));
    }
    state.apply(KeysEditorMessage::CommitEdit);

    assert!(state.is_dirty());
    assert!(matches!(state.validation, KeysValidation::Pending));
    assert!(matches!(
        state.selected_row().map(|row| &row.edit),
        Some(KeysBindingEdit::Set(chords)) if chords.len() == 2
    ));
}

#[test]
fn unbind_and_reset_are_distinct_lossless_intents() {
    let mut state = editor();
    state.apply(KeysEditorMessage::MoveDown);
    state.apply(KeysEditorMessage::Unbind);
    assert!(matches!(
        state.selected_row().map(|row| &row.edit),
        Some(KeysBindingEdit::Set(chords)) if chords.is_empty()
    ));
    state.apply(KeysEditorMessage::Reset);
    assert!(matches!(
        state.selected_row().map(|row| &row.edit),
        Some(KeysBindingEdit::Reset)
    ));
}

#[test]
fn dirty_escape_defaults_to_cancel_and_escape_there_returns_to_editor() {
    let mut state = editor();
    state.apply(KeysEditorMessage::MoveDown);
    state.apply(KeysEditorMessage::Unbind);
    state.apply(KeysEditorMessage::ValidationPassed);
    state.apply(KeysEditorMessage::RequestClose);
    assert_eq!(state.confirmation, Some(KeysConfirmFocus::Cancel));
    state.apply(KeysEditorMessage::ConfirmCancel);
    assert_eq!(state.confirmation, None);
    assert!(state.is_dirty());
}

#[test]
fn recovery_and_candidate_errors_disable_save_until_validation_passes() {
    let mut state = KeysEditorState::from_snapshot(
        &crate::action_projection::test_snapshot(),
        Some("KEY-E401: malformed chord".to_owned()),
    );
    assert!(!state.is_save_enabled());
    state.apply(KeysEditorMessage::MoveDown);
    state.apply(KeysEditorMessage::Reset);
    state.apply(KeysEditorMessage::ValidationPassed);
    assert!(state.is_save_enabled());
}
