//! Typed Keys-editor intents owned by the deterministic modal reducer.

use crate::domain::action_registry::ActionRegistrySnapshot;

/// Closed intents and boundary completions for the Keys editor.
#[derive(Debug, Clone)]
pub enum KeysEditorMessage {
    MoveUp,
    MoveDown,
    MoveHome,
    MoveEnd,
    BeginEdit,
    EditChar(char),
    EditBackspace,
    CommitEdit,
    CancelEdit,
    Unbind,
    Reset,
    ValidationPassed,
    ValidationFailed(String),
    RequestClose,
    ConfirmPrevious,
    ConfirmNext,
    ConfirmCancel,
    ConfirmDiscard,
    SaveFailed(String),
    SaveSucceeded(ActionRegistrySnapshot),
}
