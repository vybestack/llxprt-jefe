use iocraft::prelude::*;
use jefe::state::{ActionsFilterField, ActionsFocus, AppEvent, AppState};

use super::filter_controls::{FilterControlCommand, FilterEditorKind, resolve_filter_control_key};
use super::list_navigation::actions_page_item_count;
use super::{AppStateHandle, SharedContext};

/// Pure key resolver for Actions mode.
#[cfg(test)]
pub fn resolve_actions_key_event(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    resolve_actions_key_event_for_size(state, key_event, 120, 40)
}

#[cfg(test)]
fn resolve_actions_key_event_for_rows(
    state: &AppState,
    key_event: &KeyEvent,
    terminal_rows: u16,
) -> Option<AppEvent> {
    resolve_actions_key_event_for_size(state, key_event, 120, terminal_rows)
}

#[must_use]
fn resolve_actions_key_event_for_size(
    state: &AppState,
    key_event: &KeyEvent,
    terminal_cols: u16,
    terminal_rows: u16,
) -> Option<AppEvent> {
    let page_item_count = actions_page_item_count(state, terminal_cols, terminal_rows);
    if state.actions_state.ui.search_input_focused {
        return resolve_search_key_event(state, key_event);
    }

    if state.actions_state.ui.filter_ui_open {
        return resolve_filter_key_event(state, key_event);
    }

    resolve_global_actions_key_event(state, key_event)
        .or_else(|| resolve_focus_key_event(state, key_event, page_item_count))
}

/// Route an Actions-mode key using the current terminal row count.
pub fn handle_actions_mode_key(
    app_state: &AppStateHandle,
    _ctx: &SharedContext,
    key_event: &KeyEvent,
) -> Option<AppEvent> {
    let (terminal_cols, terminal_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let state = app_state.read();
    resolve_actions_key_event_for_size(&state, key_event, terminal_cols, terminal_rows)
}

fn resolve_search_key_event(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Enter => Some(AppEvent::ActionsApplySearch),
        KeyCode::Esc if state.actions_state.search_query.is_empty() => {
            Some(AppEvent::ActionsBlurSearchInput)
        }
        KeyCode::Esc => Some(AppEvent::ActionsClearSearch),
        // Only accept the character as search text when no Ctrl/Alt modifier
        // is held — otherwise Ctrl+Q / Ctrl+C etc. would leak their base letter
        // into the query. Mirrors the guard in input.rs route_search_key.
        KeyCode::Char(c)
            if !key_event
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            let mut query = state.actions_state.search_query.clone();
            query.push(c);
            Some(AppEvent::ActionsSetSearchQuery { query })
        }
        KeyCode::Backspace => {
            let mut query = state.actions_state.search_query.clone();
            query.pop();
            Some(AppEvent::ActionsSetSearchQuery { query })
        }
        _ => None,
    }
}

fn resolve_filter_key_event(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match resolve_filter_control_key(FilterEditorKind::Choice, key_event)? {
        FilterControlCommand::Apply => Some(AppEvent::ActionsApplyFilter),
        FilterControlCommand::Cancel => Some(AppEvent::ActionsCloseFilterControls),
        FilterControlCommand::Next => Some(AppEvent::ActionsFilterNavigateNext),
        FilterControlCommand::Previous => Some(AppEvent::ActionsFilterNavigatePrev),
        FilterControlCommand::ClearAll => Some(AppEvent::ActionsClearDraftFilter),
        FilterControlCommand::ClearCurrent => Some(AppEvent::ActionsUpdateDraftFilter {
            field: match state.actions_state.ui.filter_field_index {
                0 => ActionsFilterField::Workflow,
                1 => ActionsFilterField::Status,
                2 => ActionsFilterField::Pr,
                _ => return None,
            },
            value: String::new(),
        }),
        FilterControlCommand::CycleNext | FilterControlCommand::CyclePrevious => {
            Some(AppEvent::ActionsCycleFilterStatus)
        }
        _ => None,
    }
}

fn resolve_global_actions_key_event(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Esc if state.actions_state.focus != ActionsFocus::Detail => {
            Some(AppEvent::ExitActionsMode)
        }
        KeyCode::Char('r') => Some(AppEvent::ActionsReload),
        KeyCode::Char('f') => Some(AppEvent::ActionsOpenFilterControls),
        KeyCode::Char('/') => Some(AppEvent::ActionsFocusSearchInput),
        KeyCode::Char('d') => {
            if let Some(detail) = &state.actions_state.run_detail {
                let wf = state
                    .actions_state
                    .workflows
                    .iter()
                    .find(|w| w.name == detail.run.workflow_name)
                    .cloned();
                wf.map(AppEvent::OpenWorkflowDispatch)
            } else {
                state
                    .actions_state
                    .workflows
                    .first()
                    .cloned()
                    .map(AppEvent::OpenWorkflowDispatch)
            }
        }
        _ => None,
    }
}

fn resolve_focus_key_event(
    state: &AppState,
    key_event: &KeyEvent,
    page_item_count: jefe::list_viewport::PageItemCount,
) -> Option<AppEvent> {
    match state.actions_state.focus {
        ActionsFocus::RepoList => match key_event.code {
            KeyCode::Up => Some(AppEvent::ActionsNavigateUp),
            KeyCode::Down => Some(AppEvent::ActionsNavigateDown),
            KeyCode::Left => Some(AppEvent::ActionsCycleFocusReverse),
            KeyCode::Right | KeyCode::Tab => Some(AppEvent::ActionsCycleFocus),
            _ => None,
        },
        ActionsFocus::RunList => match key_event.code {
            KeyCode::Up => Some(AppEvent::ActionsNavigateUp),
            KeyCode::Down => Some(AppEvent::ActionsNavigateDown),
            KeyCode::PageUp => Some(AppEvent::ActionsNavigatePageUp(page_item_count)),
            KeyCode::PageDown => Some(AppEvent::ActionsNavigatePageDown(page_item_count)),
            KeyCode::Home => Some(AppEvent::ActionsNavigateHome),
            KeyCode::End => Some(AppEvent::ActionsNavigateEnd),
            KeyCode::Enter => Some(AppEvent::ActionsEnter),
            KeyCode::Left => Some(AppEvent::ActionsCycleFocusReverse),
            KeyCode::Right | KeyCode::Tab => Some(AppEvent::ActionsCycleFocus),
            _ => None,
        },
        ActionsFocus::Detail => match key_event.code {
            KeyCode::Up => Some(AppEvent::ActionsNavigateJobUp),
            KeyCode::Down => Some(AppEvent::ActionsNavigateJobDown),
            KeyCode::PageUp => Some(AppEvent::ActionsScrollDetailUp),
            KeyCode::PageDown => Some(AppEvent::ActionsScrollDetailDown),
            KeyCode::Enter | KeyCode::Right => Some(AppEvent::ActionsExpandJob),
            KeyCode::Left => Some(AppEvent::ActionsCollapseJob),
            KeyCode::Esc => Some(AppEvent::ActionsDetailEscape),
            KeyCode::Tab => Some(AppEvent::ActionsCycleFocus),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iocraft::prelude::KeyEventKind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(KeyEventKind::Press, code)
    }

    #[test]
    fn actions_filter_supports_advertised_clear_commands() {
        let mut state = AppState::default();
        state.actions_state.ui.filter_ui_open = true;
        state.actions_state.ui.filter_field_index = 0;

        assert!(matches!(
            resolve_actions_key_event(&state, &key(KeyCode::Delete)),
            Some(AppEvent::ActionsUpdateDraftFilter {
                field: ActionsFilterField::Workflow,
                value
            }) if value.is_empty()
        ));

        state.actions_state.ui.filter_field_index = 1;
        assert!(matches!(
            resolve_actions_key_event(&state, &key(KeyCode::Delete)),
            Some(AppEvent::ActionsUpdateDraftFilter {
                field: ActionsFilterField::Status,
                value
            }) if value.is_empty()
        ));

        state.actions_state.ui.filter_field_index = 2;
        assert!(matches!(
            resolve_actions_key_event(&state, &key(KeyCode::Delete)),
            Some(AppEvent::ActionsUpdateDraftFilter {
                field: ActionsFilterField::Pr,
                value
            }) if value.is_empty()
        ));

        let mut clear_all = key(KeyCode::Char('l'));
        clear_all.modifiers = KeyModifiers::CONTROL;
        assert!(matches!(
            resolve_actions_key_event(&state, &clear_all),
            Some(AppEvent::ActionsClearDraftFilter)
        ));
    }
    #[test]
    fn page_keys_carry_actual_actions_list_capacity() {
        let mut state = AppState::default();
        state.actions_state.focus = ActionsFocus::RunList;
        let up = resolve_actions_key_event_for_rows(&state, &key(KeyCode::PageUp), 22);
        assert!(matches!(up, Some(AppEvent::ActionsNavigatePageUp(page)) if page.get() == 3));
        let down = resolve_actions_key_event_for_rows(&state, &key(KeyCode::PageDown), 36);
        assert!(matches!(down, Some(AppEvent::ActionsNavigatePageDown(page)) if page.get() == 7));
    }

    #[test]
    fn run_list_enter_opens_detail() {
        let mut state = AppState::default();
        state.actions_state.focus = ActionsFocus::RunList;
        assert!(matches!(
            resolve_actions_key_event(&state, &key(KeyCode::Enter)),
            Some(AppEvent::ActionsEnter)
        ));
    }

    #[test]
    fn detail_enter_and_right_are_expand_only_intents() {
        let mut state = AppState::default();
        state.actions_state.focus = ActionsFocus::Detail;
        for code in [KeyCode::Enter, KeyCode::Right] {
            assert!(matches!(
                resolve_actions_key_event(&state, &key(code)),
                Some(AppEvent::ActionsExpandJob)
            ));
        }
    }

    #[test]
    fn detail_left_collapses_and_escape_uses_contextual_detail_intent() {
        let mut state = AppState::default();
        state.actions_state.focus = ActionsFocus::Detail;
        assert!(matches!(
            resolve_actions_key_event(&state, &key(KeyCode::Left)),
            Some(AppEvent::ActionsCollapseJob)
        ));
        assert!(matches!(
            resolve_actions_key_event(&state, &key(KeyCode::Esc)),
            Some(AppEvent::ActionsDetailEscape)
        ));
    }

    #[test]
    fn run_list_escape_still_exits_actions_mode() {
        let mut state = AppState::default();
        state.actions_state.focus = ActionsFocus::RunList;
        assert!(matches!(
            resolve_actions_key_event(&state, &key(KeyCode::Esc)),
            Some(AppEvent::ExitActionsMode)
        ));
    }
}
