//! Deterministic state machine for the schema-2 Keys editor.
//!
//! The editor stores typed patch intent only. Complete-candidate validation and
//! persistence stay at the app-input boundary; rendering is delegated to the
//! iocraft-free `keys_view` projection.

use crate::domain::action_registry::{ActionRegistrySnapshot, Availability, Provenance};
use crate::domain::input_context::ContextId;
use crate::domain::keymap::Chord;
use crate::messages::KeysEditorMessage;

/// Lossless patch intent for one action/context binding.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum KeysBindingEdit {
    /// Keep source syntax exactly as loaded.
    #[default]
    Unchanged,
    /// Replace or insert the whole canonical chord list; an empty list unbinds.
    Set(Vec<Chord>),
    /// Remove the source assignment so the compiled binding is inherited.
    Reset,
}

/// One projected binding row owned by the editor reducer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeysBindingRow {
    pub context: ContextId,
    pub action: crate::domain::action_registry::ActionId,
    pub label: String,
    pub effective_chords: Vec<Chord>,
    pub settings_override: bool,
    pub protected: bool,
    pub availability: Availability,
    pub edit: KeysBindingEdit,
}

impl KeysBindingRow {
    /// Chords currently represented by the draft, when the row has a concrete list.
    #[must_use]
    pub fn draft_chords(&self) -> Option<&[Chord]> {
        match &self.edit {
            KeysBindingEdit::Unchanged => Some(&self.effective_chords),
            KeysBindingEdit::Set(chords) => Some(chords),
            KeysBindingEdit::Reset => None,
        }
    }
}

/// Complete-candidate validation state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeysValidation {
    Valid,
    Pending,
    Invalid(String),
}

/// Focus in the dirty-close confirmation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeysConfirmFocus {
    Save,
    Discard,
    Cancel,
}

/// Runtime-only Keys editor state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeysEditorState {
    pub rows: Vec<KeysBindingRow>,
    pub selected: usize,
    pub editing: bool,
    pub edit_input: String,
    pub validation: KeysValidation,
    pub confirmation: Option<KeysConfirmFocus>,
    pub recovery: Option<String>,
    pub status: Option<String>,
}

impl KeysEditorState {
    /// Project one immutable S5 snapshot into editable action/context rows.
    #[must_use]
    pub fn from_snapshot(snapshot: &ActionRegistrySnapshot, recovery: Option<String>) -> Self {
        let mut rows: Vec<KeysBindingRow> = snapshot
            .effective_bindings()
            .iter()
            .filter_map(|binding| {
                let action = snapshot
                    .actions()
                    .iter()
                    .find(|action| action.id == binding.action)?;
                let availability = snapshot
                    .availability_entries()
                    .iter()
                    .find(|entry| entry.action() == &binding.action)?
                    .availability()
                    .clone();
                Some(KeysBindingRow {
                    context: binding.context.clone(),
                    action: binding.action.clone(),
                    label: action.label.clone(),
                    effective_chords: binding.chords.clone(),
                    settings_override: matches!(binding.provenance, Provenance::Settings { .. }),
                    protected: action.protected,
                    availability,
                    edit: KeysBindingEdit::Unchanged,
                })
            })
            .collect();
        rows.extend(Self::unbound_rows(snapshot, &rows));
        rows.sort_by_key(|row| match row.action.as_str() {
            "core.emergency-exit" => (0_u8, String::new(), String::new()),
            "core.open-keys" => (1_u8, String::new(), String::new()),
            _ => (
                2_u8,
                row.context.as_str().to_owned(),
                row.action.as_str().to_owned(),
            ),
        });
        let validation = recovery
            .clone()
            .map_or(KeysValidation::Valid, KeysValidation::Invalid);
        Self {
            rows,
            selected: 0,
            editing: false,
            edit_input: String::new(),
            validation,
            confirmation: None,
            recovery,
            status: None,
        }
    }

    fn unbound_rows(
        snapshot: &ActionRegistrySnapshot,
        bound_rows: &[KeysBindingRow],
    ) -> Vec<KeysBindingRow> {
        snapshot
            .actions()
            .iter()
            .filter(|action| !bound_rows.iter().any(|row| row.action == action.id))
            .filter_map(|action| {
                let context = action.contexts.first()?.clone();
                let availability = snapshot
                    .availability_entries()
                    .iter()
                    .find(|entry| entry.action() == &action.id)?
                    .availability()
                    .clone();
                Some(KeysBindingRow {
                    context,
                    action: action.id.clone(),
                    label: action.label.clone(),
                    effective_chords: Vec::new(),
                    settings_override: true,
                    protected: action.protected,
                    availability,
                    edit: KeysBindingEdit::Unchanged,
                })
            })
            .collect()
    }

    /// Apply one typed editor intent without I/O.
    pub fn apply(&mut self, message: KeysEditorMessage) {
        match message {
            KeysEditorMessage::MoveUp => self.move_selection(-1),
            KeysEditorMessage::MoveDown => self.move_selection(1),
            KeysEditorMessage::MoveHome => self.selected = 0,
            KeysEditorMessage::MoveEnd => self.select_last(),
            KeysEditorMessage::BeginEdit => self.begin_edit(),
            KeysEditorMessage::EditChar(character) => self.edit_input.push(character),
            KeysEditorMessage::EditBackspace => {
                self.edit_input.pop();
            }
            KeysEditorMessage::CommitEdit => self.commit_edit(),
            KeysEditorMessage::CancelEdit => self.cancel_edit(),
            KeysEditorMessage::Unbind => self.set_selected_edit(KeysBindingEdit::Set(Vec::new())),
            KeysEditorMessage::Reset => self.set_selected_edit(KeysBindingEdit::Reset),
            KeysEditorMessage::ValidationPassed => self.validation_passed(),
            KeysEditorMessage::ValidationFailed(error) | KeysEditorMessage::SaveFailed(error) => {
                self.validation = KeysValidation::Invalid(error);
            }
            KeysEditorMessage::RequestClose => self.request_close(),
            KeysEditorMessage::ConfirmPrevious => self.cycle_confirmation(-1),
            KeysEditorMessage::ConfirmNext => self.cycle_confirmation(1),
            KeysEditorMessage::ConfirmCancel => self.confirmation = None,
            KeysEditorMessage::ConfirmDiscard | KeysEditorMessage::SaveSucceeded(_) => {}
        }
    }

    /// Selected row, if the snapshot contains bindings.
    #[must_use]
    pub fn selected_row(&self) -> Option<&KeysBindingRow> {
        self.rows.get(self.selected)
    }

    /// Mutable selected row.
    fn selected_row_mut(&mut self) -> Option<&mut KeysBindingRow> {
        self.rows.get_mut(self.selected)
    }

    /// Whether any syntax-changing intent is staged.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.rows
            .iter()
            .any(|row| row.edit != KeysBindingEdit::Unchanged)
    }

    /// Whether Save may execute at the side-effect boundary.
    #[must_use]
    pub fn is_save_enabled(&self) -> bool {
        self.is_dirty() && matches!(self.validation, KeysValidation::Valid)
    }

    /// Current typed diagnostic, if validation has failed.
    #[must_use]
    pub fn validation_message(&self) -> Option<&str> {
        match &self.validation {
            KeysValidation::Invalid(message) => Some(message),
            KeysValidation::Valid | KeysValidation::Pending => None,
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.rows.is_empty() || self.editing || self.confirmation.is_some() {
            return;
        }
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(self.rows.len().saturating_sub(1));
    }

    fn select_last(&mut self) {
        self.selected = self.rows.len().saturating_sub(1);
    }

    fn begin_edit(&mut self) {
        let Some(row) = self.selected_row() else {
            return;
        };
        if row.protected {
            self.protected_error();
            return;
        }
        self.edit_input = row.draft_chords().map_or_else(String::new, format_chords);
        self.editing = true;
        self.status = None;
    }

    fn cancel_edit(&mut self) {
        self.editing = false;
        self.edit_input.clear();
        self.status = None;
    }

    fn commit_edit(&mut self) {
        let parsed = parse_chord_list(&self.edit_input);
        match parsed {
            Ok(chords) => self.set_selected_edit(KeysBindingEdit::Set(chords)),
            Err(error) => self.validation = KeysValidation::Invalid(error),
        }
    }

    fn set_selected_edit(&mut self, edit: KeysBindingEdit) {
        let protected = self.selected_row().is_some_and(|row| row.protected);
        if protected {
            self.protected_error();
            return;
        }
        if let Some(row) = self.selected_row_mut() {
            row.edit = edit;
            self.validation = KeysValidation::Pending;
            self.status = Some("Unsaved changes".to_owned());
        }
    }

    fn protected_error(&mut self) {
        self.validation = KeysValidation::Invalid(
            "KEY-E401: protected controls are read-only and cannot be unbound or shadowed"
                .to_owned(),
        );
    }

    fn validation_passed(&mut self) {
        self.validation = KeysValidation::Valid;
        self.recovery = None;
        self.editing = false;
        self.edit_input.clear();
    }

    fn request_close(&mut self) {
        if self.is_dirty() {
            self.confirmation = Some(KeysConfirmFocus::Cancel);
        }
    }

    fn cycle_confirmation(&mut self, delta: isize) {
        let Some(focus) = self.confirmation else {
            return;
        };
        let index = match focus {
            KeysConfirmFocus::Save => 0usize,
            KeysConfirmFocus::Discard => 1,
            KeysConfirmFocus::Cancel => 2,
        };
        let next = index.saturating_add_signed(delta).min(2);
        self.confirmation = Some(match next {
            0 => KeysConfirmFocus::Save,
            1 => KeysConfirmFocus::Discard,
            _ => KeysConfirmFocus::Cancel,
        });
    }
}

fn parse_chord_list(input: &str) -> Result<Vec<Chord>, String> {
    if input.trim().is_empty() {
        return Err("KEY-E401: enter canonical chord(s), or use Unbind".to_owned());
    }
    input
        .split_ascii_whitespace()
        .map(|value| Chord::parse(value).map_err(|error| format!("KEY-E401: {error}")))
        .collect()
}

fn format_chords(chords: &[Chord]) -> String {
    chords
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}
