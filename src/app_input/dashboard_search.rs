//! Raw Dashboard search text mutation routing (issue #405).

use iocraft::prelude::{KeyCode, KeyEvent};
use jefe::state::{AppEvent, AppState};

#[must_use]
pub(super) fn resolve_raw_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    if !state.dashboard_search.input_focused
        || !super::raw_key_mutations::text_modifiers(key_event.modifiers)
    {
        return None;
    }
    let mut query = state.dashboard_search.query.clone();
    match key_event.code {
        KeyCode::Char(character) => query.push(character),
        KeyCode::Backspace => {
            query.pop();
        }
        _ => return None,
    }
    Some(AppEvent::SetDashboardSearchQuery { query })
}

#[cfg(test)]
mod tests {
    use super::*;
    use iocraft::prelude::KeyEventKind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(KeyEventKind::Press, code)
    }

    #[test]
    fn text_mutations_remain_raw() {
        let mut state = AppState::default();
        state.dashboard_search.input_focused = true;
        state.dashboard_search.query = "al".to_owned();
        assert!(matches!(
            resolve_raw_key(&state, &key(KeyCode::Char('p'))),
            Some(AppEvent::SetDashboardSearchQuery { query }) if query == "alp"
        ));
        assert!(matches!(
            resolve_raw_key(&state, &key(KeyCode::Backspace)),
            Some(AppEvent::SetDashboardSearchQuery { query }) if query == "a"
        ));
    }

    #[test]
    fn apply_and_cancel_are_registry_owned() {
        let mut state = AppState::default();
        state.dashboard_search.input_focused = true;
        assert!(resolve_raw_key(&state, &key(KeyCode::Enter)).is_none());
        assert!(resolve_raw_key(&state, &key(KeyCode::Esc)).is_none());
    }
}
