//! Keys editor input translation, complete-candidate validation, and save boundary.

use iocraft::prelude::{KeyCode, KeyEvent, KeyModifiers};
use jefe::config_owners::builtin_owner_catalog;
use jefe::messages::{AppMessage, KeysEditorMessage, ModalMessage};
use jefe::persistence::settings_document::SettingsDocument;
use jefe::persistence::writer::{ExpectedHash, Freshness, WriteOutcome};
use jefe::persistence::{KeymapCandidate, KeymapEdit};
use jefe::state::{KeysBindingEdit, KeysConfirmFocus, ModalState};

use super::{AppStateHandle, SharedContext};

#[must_use]
pub fn handle_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) -> bool {
    let editor = {
        let state = app_state.read();
        let ModalState::Keys { editor } = &state.modal else {
            return false;
        };
        let editor = editor.clone();
        drop(state);
        editor
    };
    if key_event.modifiers == KeyModifiers::CONTROL
        && matches!(key_event.code, KeyCode::Char('q' | 'Q'))
    {
        return false;
    }
    let save_key = matches!(key_event.code, KeyCode::Char('s' | 'S'))
        && !editor.editing
        && editor.confirmation.is_none();
    let confirm_save =
        key_event.code == KeyCode::Enter && editor.confirmation == Some(KeysConfirmFocus::Save);
    if save_key || confirm_save {
        save(app_state, ctx);
        return true;
    }
    let message = if editor.confirmation.is_some() {
        confirmation_message(&editor, key_event)
    } else if editor.editing {
        editing_message(key_event)
    } else {
        normal_message(key_event)
    };
    let Some(message) = message else {
        return true;
    };
    apply_keys_reducer(app_state, &message);
    if requires_validation(&message) {
        validate(app_state, ctx);
    }
    true
}

pub fn open(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let recovery = ctx
        .as_ref()
        .and_then(|context| context.lock().ok())
        .and_then(|context| context.keymap_recovery.clone());
    dispatch(
        app_state,
        AppMessage::Modal(ModalMessage::OpenKeys { recovery }),
    );
}

fn normal_message(key: &KeyEvent) -> Option<KeysEditorMessage> {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => Some(KeysEditorMessage::MoveUp),
        KeyCode::Down | KeyCode::Char('j') => Some(KeysEditorMessage::MoveDown),
        KeyCode::Home => Some(KeysEditorMessage::MoveHome),
        KeyCode::End => Some(KeysEditorMessage::MoveEnd),
        KeyCode::Enter | KeyCode::Char('e' | 'E') => Some(KeysEditorMessage::BeginEdit),
        KeyCode::Char('u' | 'U') => Some(KeysEditorMessage::Unbind),
        KeyCode::Char('r' | 'R') => Some(KeysEditorMessage::Reset),
        KeyCode::Esc => Some(KeysEditorMessage::RequestClose),
        _ => None,
    }
}

fn editing_message(key: &KeyEvent) -> Option<KeysEditorMessage> {
    match key.code {
        KeyCode::Enter => Some(KeysEditorMessage::CommitEdit),
        KeyCode::Esc => Some(KeysEditorMessage::CancelEdit),
        KeyCode::Backspace => Some(KeysEditorMessage::EditBackspace),
        KeyCode::Char(character) if key.modifiers.is_empty() => {
            Some(KeysEditorMessage::EditChar(character))
        }
        KeyCode::Char(character) if key.modifiers == KeyModifiers::SHIFT => {
            Some(KeysEditorMessage::EditChar(character.to_ascii_uppercase()))
        }
        _ => None,
    }
}

fn confirmation_message(
    editor: &jefe::state::KeysEditorState,
    key: &KeyEvent,
) -> Option<KeysEditorMessage> {
    match key.code {
        KeyCode::Esc => Some(KeysEditorMessage::ConfirmCancel),
        KeyCode::Left | KeyCode::BackTab => Some(KeysEditorMessage::ConfirmPrevious),
        KeyCode::Right | KeyCode::Tab => Some(KeysEditorMessage::ConfirmNext),
        KeyCode::Enter => match editor.confirmation {
            Some(KeysConfirmFocus::Save) => None,
            Some(KeysConfirmFocus::Discard) => Some(KeysEditorMessage::ConfirmDiscard),
            Some(KeysConfirmFocus::Cancel) | None => Some(KeysEditorMessage::ConfirmCancel),
        },
        _ => None,
    }
}

fn apply_keys_reducer(app_state: &mut AppStateHandle, message: &KeysEditorMessage) {
    dispatch(
        app_state,
        AppMessage::Modal(ModalMessage::Keys(message.clone())),
    );
}

fn requires_validation(message: &KeysEditorMessage) -> bool {
    matches!(
        message,
        KeysEditorMessage::CommitEdit | KeysEditorMessage::Unbind | KeysEditorMessage::Reset
    )
}

fn validate(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    match candidate(app_state, ctx) {
        Ok(_) => dispatch(
            app_state,
            AppMessage::Modal(ModalMessage::Keys(KeysEditorMessage::ValidationPassed)),
        ),
        Err(error) => dispatch(
            app_state,
            AppMessage::Modal(ModalMessage::Keys(KeysEditorMessage::ValidationFailed(
                error,
            ))),
        ),
    }
}

fn save(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    if !editor_save_enabled(app_state) {
        return;
    }
    let candidate = match candidate(app_state, ctx) {
        Ok(candidate) => candidate,
        Err(error) => {
            dispatch_save_failed(app_state, error);
            return;
        }
    };
    let Some(context) = ctx else {
        dispatch_save_failed(
            app_state,
            "KEY-E401: settings context unavailable".to_owned(),
        );
        return;
    };
    let outcome = match context.lock() {
        Ok(mut context) => write_candidate(&mut context, &candidate),
        Err(_) => Err("KEY-E401: settings context lock failed".to_owned()),
    };
    match outcome {
        Ok(snapshot) => dispatch(
            app_state,
            AppMessage::Modal(ModalMessage::Keys(KeysEditorMessage::SaveSucceeded(
                snapshot,
            ))),
        ),
        Err(error) => dispatch_save_failed(app_state, error),
    }
}

fn write_candidate(
    context: &mut crate::AppContext,
    candidate: &KeymapCandidate,
) -> Result<jefe::domain::action_registry::ActionRegistrySnapshot, String> {
    let revision = context.settings_revision.saturating_add(1);
    let freshness = |_revision| Freshness::Current;
    match context
        .persistence
        .save_keymap_candidate_revisioned(candidate, revision, &freshness)
        .map_err(|error| error.to_string())?
    {
        WriteOutcome::Authoritative { hash, .. } => {
            let document = SettingsDocument::parse(candidate.bytes())
                .map_err(|diagnostic| diagnostic.redacted_detail.clone())?;
            let snapshot = candidate.snapshot().clone();
            context.settings_document = document;
            context.settings_expected_hash = ExpectedHash::Present(hash);
            context.published_settings = candidate.published().clone();
            context.keymap_recovery = None;
            context.settings_revision = revision;
            context.keymap_snapshot = Some(snapshot.clone());
            Ok(snapshot)
        }
        WriteOutcome::Stale { .. } => Err("KEY-E401: settings save was superseded".to_owned()),
    }
}

fn candidate(app_state: &AppStateHandle, ctx: &SharedContext) -> Result<KeymapCandidate, String> {
    let edits = editor_edits(app_state)?;
    let Some(context) = ctx else {
        return Err("KEY-E401: settings context unavailable".to_owned());
    };
    let context = context
        .lock()
        .map_err(|_| "KEY-E401: settings context lock failed".to_owned())?;
    let catalog = builtin_owner_catalog().map_err(|error| format!("KEY-E401: {error}"))?;
    KeymapCandidate::from_edits(
        &context.settings_document,
        &catalog,
        &edits,
        context.settings_expected_hash,
        "settings",
    )
    .map_err(|error| error.to_string())
}

fn editor_edits(app_state: &AppStateHandle) -> Result<Vec<KeymapEdit>, String> {
    let state = app_state.read();
    let ModalState::Keys { editor } = &state.modal else {
        return Err("KEY-E401: Keys editor is closed".to_owned());
    };
    let rows = editor.rows.clone();
    drop(state);
    Ok(rows
        .iter()
        .filter_map(|row| match &row.edit {
            KeysBindingEdit::Unchanged => None,
            KeysBindingEdit::Set(chords) => Some(KeymapEdit::set(
                row.context.clone(),
                row.action.clone(),
                chords.clone(),
            )),
            KeysBindingEdit::Reset => {
                Some(KeymapEdit::reset(row.context.clone(), row.action.clone()))
            }
        })
        .collect())
}

fn editor_save_enabled(app_state: &AppStateHandle) -> bool {
    let state = app_state.read();
    matches!(&state.modal, ModalState::Keys { editor } if editor.is_save_enabled())
}

fn dispatch(app_state: &mut AppStateHandle, message: AppMessage) {
    let mut state = app_state.write();
    jefe::state::transition::commit_pure_site(&mut state, message);
}

fn dispatch_save_failed(app_state: &mut AppStateHandle, error: String) {
    dispatch(
        app_state,
        AppMessage::Modal(ModalMessage::Keys(KeysEditorMessage::SaveFailed(error))),
    );
}
