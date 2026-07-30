//! Pure, iocraft-free projection for the Keys editor modal.

use unicode_width::UnicodeWidthChar;

use crate::domain::action_registry::Availability;
use crate::state::{KeysBindingEdit, KeysConfirmFocus, KeysEditorState, KeysValidation};

/// Render-ready Keys editor projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeysView {
    pub title: String,
    pub lines: Vec<String>,
    pub footer: String,
}

/// Project the complete editor into a bounded viewport.
#[must_use]
pub fn project_keys_view(state: &KeysEditorState, cols: u16, rows: u16) -> KeysView {
    let compact = cols < 60 || rows < 14;
    let title = if compact {
        "Keys (compact)".to_owned()
    } else {
        "Keys - Keyboard Bindings".to_owned()
    };
    let mut lines = status_lines(state);
    let confirmation_rows = usize::from(state.confirmation.is_some()) * 3;
    let line_budget = usize::from(rows).saturating_sub(6).max(1);
    let row_capacity = line_budget
        .saturating_sub(lines.len())
        .saturating_sub(confirmation_rows)
        .max(1);
    lines.extend(visible_rows(state, row_capacity, compact));
    if let Some(focus) = state.confirmation {
        lines.push("Save / Discard / Cancel".to_owned());
        lines.push("Save changes before leaving?".to_owned());
        lines.push(confirm_buttons(focus, state.is_save_enabled()));
    }
    let content_width = usize::from(cols.saturating_sub(8).max(1));
    for line in &mut lines {
        *line = truncate_to_width(line, content_width);
    }
    let footer = if compact {
        "Esc Back | Ctrl-Q Quit".to_owned()
    } else if state.editing {
        "Enter Apply | Esc Back/Cancel edit | Ctrl-Q Quit".to_owned()
    } else {
        format!(
            "Enter Edit | U Unbind | R Reset | S {} | Esc Back | Ctrl-Q Quit",
            if state.is_save_enabled() {
                "Save enabled"
            } else {
                "Save disabled"
            }
        )
    };
    KeysView {
        title,
        lines,
        footer,
    }
}

fn status_lines(state: &KeysEditorState) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(recovery) = &state.recovery {
        lines.push(format!("Recovery: {recovery}"));
    }
    match &state.validation {
        KeysValidation::Invalid(error) if state.recovery.as_ref() != Some(error) => {
            lines.push(error.clone());
        }
        KeysValidation::Pending => lines.push("Validating complete candidate...".to_owned()),
        KeysValidation::Valid | KeysValidation::Invalid(_) => {}
    }
    if state.editing
        && let Some(row) = state.rows.get(state.selected)
    {
        lines.push(format!("Editing {}", row.action.as_str()));
    }
    if state.is_dirty() {
        lines.push("Unsaved changes".to_owned());
    }
    if !state.is_save_enabled() {
        lines.push("Save disabled".to_owned());
    }
    lines
}

fn visible_rows(state: &KeysEditorState, capacity: usize, compact: bool) -> Vec<String> {
    let start = state.selected.saturating_sub(capacity.saturating_sub(1));
    state
        .rows
        .iter()
        .enumerate()
        .skip(start)
        .take(capacity)
        .map(|(index, row)| render_row(state, index, row, compact))
        .collect()
}

fn render_row(
    state: &KeysEditorState,
    index: usize,
    row: &crate::state::KeysBindingRow,
    compact: bool,
) -> String {
    let marker = if index == state.selected { ">" } else { " " };
    let chords = if state.editing && index == state.selected {
        format!("[{}]", state.edit_input)
    } else {
        binding_label(row)
    };
    let flags = row_flags(row);
    if compact {
        format!("{marker} {} {chords}{flags}", row.action.as_str())
    } else {
        format!(
            "{marker} {:<24} {:<24} {chords}{flags}",
            row.action.as_str(),
            row.context.as_str()
        )
    }
}

fn binding_label(row: &crate::state::KeysBindingRow) -> String {
    match &row.edit {
        KeysBindingEdit::Unchanged => chord_list(&row.effective_chords),
        KeysBindingEdit::Set(chords) if chords.is_empty() => "Unbound".to_owned(),
        KeysBindingEdit::Set(chords) => chord_list(chords),
        KeysBindingEdit::Reset => "Inherit (Reset)".to_owned(),
    }
}

fn chord_list(chords: &[crate::domain::keymap::Chord]) -> String {
    if chords.is_empty() {
        "Unbound".to_owned()
    } else {
        chords
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn row_flags(row: &crate::state::KeysBindingRow) -> String {
    let mut flags = String::new();
    if row.protected {
        flags.push_str(" [Protected]");
    }
    if row.settings_override {
        flags.push_str(" [Override]");
    }
    if let Availability::Unavailable { reason } = &row.availability {
        flags.push_str(" [Unavailable: ");
        flags.push_str(reason.as_str());
        flags.push(']');
    }
    flags
}

fn confirm_buttons(focus: KeysConfirmFocus, save_enabled: bool) -> String {
    let save = button("Save", focus == KeysConfirmFocus::Save, save_enabled);
    let discard = button("Discard", focus == KeysConfirmFocus::Discard, true);
    let cancel = button("Cancel", focus == KeysConfirmFocus::Cancel, true);
    format!("{save}  {discard}  {cancel}")
}

fn truncate_to_width(text: &str, max_width: usize) -> String {
    let mut width = 0usize;
    text.chars()
        .take_while(|character| {
            let character_width = UnicodeWidthChar::width(*character).unwrap_or(0);
            if width.saturating_add(character_width) > max_width {
                return false;
            }
            width = width.saturating_add(character_width);
            true
        })
        .collect()
}

fn button(label: &str, focused: bool, enabled: bool) -> String {
    let text = if enabled {
        label.to_owned()
    } else {
        format!("{label} disabled")
    };
    if focused {
        format!("( {text} )")
    } else {
        format!("[ {text} ]")
    }
}
