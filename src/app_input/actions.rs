//! Raw text mutation routing for Actions search and filter controls.

use iocraft::prelude::{KeyCode, KeyEvent};
use jefe::state::{AppEvent, AppState};

#[must_use]
pub(super) fn resolve_raw_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    if state.actions_state.ui.search_input_focused {
        return resolve_search_text(state, key_event);
    }
    if state.actions_state.ui.filter_ui_open {
        return resolve_filter_text(state, key_event);
    }
    None
}

fn resolve_search_text(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    let mut query = state.actions_state.search_query.clone();
    match key_event.code {
        KeyCode::Char(character)
            if super::raw_key_mutations::text_modifiers(key_event.modifiers) =>
        {
            query.push(character);
        }
        KeyCode::Backspace => {
            query.pop();
        }
        _ => return None,
    }
    Some(AppEvent::ActionsSetSearchQuery { query })
}

fn resolve_filter_text(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    let field = match state.actions_state.ui.filter_field_index {
        0 => jefe::state::ActionsFilterField::Workflow,
        1 => jefe::state::ActionsFilterField::Status,
        2 => jefe::state::ActionsFilterField::Pr,
        _ => return None,
    };
    let mut value = match field {
        jefe::state::ActionsFilterField::Workflow => {
            state.actions_state.draft_filter.workflow.clone()
        }
        jefe::state::ActionsFilterField::Status => state.actions_state.draft_filter.status.clone(),
        jefe::state::ActionsFilterField::Pr => state
            .actions_state
            .draft_filter
            .pr_number
            .map_or_else(String::new, |number| number.to_string()),
    };
    match key_event.code {
        KeyCode::Char(character)
            if super::raw_key_mutations::text_modifiers(key_event.modifiers) =>
        {
            value.push(character);
        }
        KeyCode::Backspace => {
            value.pop();
        }
        _ => return None,
    }
    Some(AppEvent::ActionsUpdateDraftFilter { field, value })
}
