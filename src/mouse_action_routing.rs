//! Pure S7 routing from approved rendered hit targets to snapshot resolutions.

use jefe::domain::action_registry::{ActionId, ActionRegistrySnapshot, Resolution};
use jefe::domain::input_context::ContextStack;
use jefe::domain::keymap::Chord;
use jefe::pane_content_projection::projected_pane_content;
use jefe::persistence::settings_document::PublishedSettings;
use jefe::selection::{SelectablePane, point_to_content_coords};
use jefe::state::AppState;
use jefe::state::keys_editor_project::{ChordText, project_keys};

/// One mouse target after resolving its `ActionId` through the current snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MouseActionRoute {
    pub chord: Chord,
    pub resolution: Resolution,
    /// Stable identity of the surface that produced the action, recorded by
    /// the strict-harness capture so a hit is provably not a no-op.
    pub hit: &'static str,
    /// The `ActionId` the hit surface contributed.
    pub action: ActionId,
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
    let action = confirm_action_at(state, up_col, up_row, cols, rows)?;
    resolve_action(snapshot, &action, "confirm.button")
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

fn resolve_action(
    snapshot: &ActionRegistrySnapshot,
    target: &ActionId,
    hit: &'static str,
) -> Option<MouseActionRoute> {
    let rows = project_keys(snapshot, &PublishedSettings::default());
    let row = rows.iter().find(|row| &row.action == target)?;
    let stack = ContextStack::from_ordered([row.context.as_str()], false).ok()?;
    // Only a chord the grammar read can be resolved; text it could not read
    // names nothing to dispatch, which is exactly why the row keeps showing it.
    row.chords
        .iter()
        .filter_map(|chord| match chord {
            ChordText::Chord(chord) => Some(*chord),
            ChordText::Unreadable(_) => None,
        })
        .find_map(|chord| {
            let resolution = snapshot.resolve(&chord, &stack);
            resolution_targets(&resolution, target).then_some(MouseActionRoute {
                chord,
                resolution,
                hit,
                action: target.clone(),
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
