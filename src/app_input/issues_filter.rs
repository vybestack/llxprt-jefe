//! Filter key routing for issues mode.
//! @requirement REQ-ISS-008

use iocraft::prelude::*;

use jefe::state::{AppEvent, AppState};

/// Filter field names indexed by `filter_field_index`.
/// 0=state (cycle-only), 1..4 are text fields.
const FILTER_FIELD_NAMES: [&str; 5] = ["state", "author", "assignee", "labels", "query_text"];

/// Resolve a key event while filter controls are open.
/// @requirement REQ-ISS-008
pub(super) fn resolve_filter_key_event(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    let field_idx = state.issues_state.filter_ui.field_index;

    match key_event.code {
        KeyCode::Enter => Some(AppEvent::ApplyFilter),
        KeyCode::Esc => Some(AppEvent::CloseFilterControls),
        KeyCode::Tab => Some(AppEvent::FilterNavigateNext),
        KeyCode::BackTab => Some(AppEvent::FilterNavigatePrev),
        KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(AppEvent::ClearFilter)
        }
        // Field-specific input
        KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if field_idx == 0 => {
            // State field: cycle through open/closed/all
            Some(AppEvent::CycleFilterState)
        }
        KeyCode::Char(c) if field_idx > 0 => {
            let &field_name = FILTER_FIELD_NAMES.get(field_idx)?;
            let mut value = current_filter_field_value(state, field_name);
            value.push(c);
            Some(AppEvent::UpdateDraftFilter {
                field: field_name.to_string(),
                value,
            })
        }
        KeyCode::Backspace if field_idx > 0 => {
            let &field_name = FILTER_FIELD_NAMES.get(field_idx)?;
            let mut value = current_filter_field_value(state, field_name);
            value.pop();
            Some(AppEvent::UpdateDraftFilter {
                field: field_name.to_string(),
                value,
            })
        }
        _ => None, // consumed, no leak
    }
}

/// Read the current value of a draft filter text field.
/// For labels, reads the raw editing string to preserve trailing commas.
fn current_filter_field_value(state: &AppState, field_name: &str) -> String {
    match field_name {
        "author" => state.issues_state.draft_filter.author.clone(),
        "assignee" => state.issues_state.draft_filter.assignee.clone(),
        "labels" => state.issues_state.filter_ui.draft_labels_text.clone(),
        "query_text" => state.issues_state.draft_filter.query_text.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iocraft::prelude::{KeyCode, KeyEventKind, KeyModifiers};
    use jefe::state::{AppState, IssueFilterUiState, ScreenMode};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(KeyEventKind::Press, code)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        let mut evt = KeyEvent::new(KeyEventKind::Press, code);
        evt.modifiers = KeyModifiers::CONTROL;
        evt
    }

    fn filter_state() -> AppState {
        AppState {
            screen_mode: ScreenMode::DashboardIssues,
            issues_state: jefe::state::IssuesState {
                active: true,
                filter_ui: IssueFilterUiState {
                    controls_open: true,
                    field_index: 0,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn test_filter_enter_applies() {
        let state = filter_state();
        let evt = resolve_filter_key_event(&state, &key(KeyCode::Enter));
        assert!(matches!(evt, Some(AppEvent::ApplyFilter)));
    }

    #[test]
    fn test_filter_esc_closes() {
        let state = filter_state();
        let evt = resolve_filter_key_event(&state, &key(KeyCode::Esc));
        assert!(matches!(evt, Some(AppEvent::CloseFilterControls)));
    }

    #[test]
    fn test_filter_tab_navigates_next() {
        let state = filter_state();
        let evt = resolve_filter_key_event(&state, &key(KeyCode::Tab));
        assert!(matches!(evt, Some(AppEvent::FilterNavigateNext)));
    }

    #[test]
    fn test_filter_backtab_navigates_prev() {
        let state = filter_state();
        let evt = resolve_filter_key_event(&state, &key(KeyCode::BackTab));
        assert!(matches!(evt, Some(AppEvent::FilterNavigatePrev)));
    }

    #[test]
    fn test_filter_ctrl_c_clears() {
        let state = filter_state();
        let evt = resolve_filter_key_event(&state, &ctrl(KeyCode::Char('c')));
        assert!(matches!(evt, Some(AppEvent::ClearFilter)));
    }

    #[test]
    fn test_filter_space_on_state_field_cycles() {
        let state = filter_state();
        assert_eq!(state.issues_state.filter_ui.field_index, 0);
        let evt = resolve_filter_key_event(&state, &key(KeyCode::Char(' ')));
        assert!(matches!(evt, Some(AppEvent::CycleFilterState)));
    }

    #[test]
    fn test_filter_left_on_state_field_cycles() {
        let state = filter_state();
        let evt = resolve_filter_key_event(&state, &key(KeyCode::Left));
        assert!(matches!(evt, Some(AppEvent::CycleFilterState)));
    }

    #[test]
    fn test_filter_char_on_text_field_appends() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 1; // author
        state.issues_state.draft_filter.author = "al".to_string();

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Char('i')));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "author");
                assert_eq!(value, "ali");
            }
            _ => panic!("expected UpdateDraftFilter"),
        }
    }

    #[test]
    fn test_filter_backspace_on_text_field_pops() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 2; // assignee
        state.issues_state.draft_filter.assignee = "bob".to_string();

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Backspace));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "assignee");
                assert_eq!(value, "bo");
            }
            _ => panic!("expected UpdateDraftFilter"),
        }
    }

    #[test]
    fn test_filter_char_on_state_field_not_text_input() {
        let state = filter_state();
        assert_eq!(state.issues_state.filter_ui.field_index, 0);
        // Typing a regular letter on the state field (idx 0) should be consumed
        let evt = resolve_filter_key_event(&state, &key(KeyCode::Char('x')));
        assert!(
            evt.is_none(),
            "non-special keys on state field are consumed"
        );
    }

    #[test]
    fn test_filter_labels_field_text_input() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 3; // labels
        state.issues_state.filter_ui.draft_labels_text = "bug".to_string();

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Char(',')));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "labels");
                assert_eq!(value, "bug,");
            }
            _ => panic!("expected UpdateDraftFilter"),
        }
    }
}
