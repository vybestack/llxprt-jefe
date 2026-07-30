//! Pure S7 routing from approved rendered hit targets to snapshot resolutions.

use jefe::domain::action_registry::{ActionId, ActionRegistrySnapshot, Resolution};
use jefe::domain::input_context::ContextStack;
use jefe::domain::keymap::Chord;
use jefe::pane_content_projection::projected_pane_content;
use jefe::selection::{SelectablePane, point_to_content_coords};
use jefe::state::{AppState, KeysEditorState, ModalState};

/// One mouse target after resolving its `ActionId` through the current snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MouseActionRoute {
    pub chord: Chord,
    pub resolution: Resolution,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MouseClickInput {
    pub down: Option<(u16, u16)>,
    pub up: (u16, u16),
    pub terminal: (u16, u16),
}

/// Resolve a zero-length click on an approved action surface.
#[must_use]
pub(super) fn resolve_action_click(
    state: &AppState,
    snapshot: &ActionRegistrySnapshot,
    click: MouseClickInput,
) -> Option<MouseActionRoute> {
    if click.down != Some(click.up) {
        return None;
    }
    let (up_col, up_row) = click.up;
    let (cols, rows) = click.terminal;
    let action = confirm_action_at(state, up_col, up_row, cols, rows)
        .or_else(|| keys_action_at(state, up_col, up_row, cols, rows))?;
    resolve_action(snapshot, &action)
}

fn confirm_action_at(
    state: &AppState,
    col: u16,
    row: u16,
    cols: u16,
    rows: u16,
) -> Option<ActionId> {
    let (pane, geometry) = super::resolve_pane(state, col, row, cols, rows, false)?;
    if pane != SelectablePane::ConfirmModal {
        return None;
    }
    let (line, content_col) = point_to_content_coords(col, row, 0, &geometry);
    let content = projected_pane_content(pane, state, None, &[], cols, rows);
    let buttons = content.lines.get(line)?;
    let action = if button_contains(buttons, "Cancel", content_col) {
        "confirm.cancel"
    } else if button_contains(buttons, "Confirm", content_col) {
        "confirm.accept"
    } else {
        return None;
    };
    ActionId::parse(action).ok()
}

fn button_contains(line: &str, label: &str, column: usize) -> bool {
    let Some(label_start) = line.find(label) else {
        return false;
    };
    let start = label_start.saturating_sub(2);
    let end = label_start.saturating_add(label.len()).saturating_add(2);
    (start..end).contains(&column)
}

fn keys_action_at(state: &AppState, col: u16, row: u16, cols: u16, rows: u16) -> Option<ActionId> {
    let ModalState::Keys { editor } = &state.modal else {
        return None;
    };
    let view = crate::keys_view::project_keys_view(editor, cols, rows);
    let line = usize::from(row.checked_sub(3)?);
    let column = usize::from(col.checked_sub(2)?);
    view.action_targets
        .iter()
        .find(|target| target.line == line && target.columns.contains(&column))
        .map(|target| target.action.clone())
}

fn resolve_action(
    snapshot: &ActionRegistrySnapshot,
    target: &ActionId,
) -> Option<MouseActionRoute> {
    let editor = KeysEditorState::from_snapshot(snapshot, None);
    let row = editor.rows.iter().find(|row| &row.action == target)?;
    let stack = ContextStack::from_ordered([row.context.as_str()], false).ok()?;
    row.effective_chords.iter().find_map(|chord| {
        let resolution = snapshot.resolve(chord, &stack);
        resolution_targets(&resolution, target).then_some(MouseActionRoute {
            chord: *chord,
            resolution,
        })
    })
}

fn resolution_targets(resolution: &Resolution, target: &ActionId) -> bool {
    match resolution {
        Resolution::Dispatch { action, .. } | Resolution::Unavailable { action, .. } => {
            action == target
        }
        Resolution::ForwardToPty | Resolution::Unbound => false,
    }
}

#[cfg(test)]
#[path = "mouse_action_routing_tests.rs"]
mod tests;
