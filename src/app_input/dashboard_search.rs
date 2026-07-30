//! Dashboard "search lite" key routing (issue #405).
//!
//! Mirrors the inline search pattern used by Issues / PRs / Actions modes:
//! when `dashboard_search.input_focused` is true, chars append to the query,
//! Backspace pops, Enter blurs (keeping the query as a live filter), and Esc
//! clears a non-empty query (or just blurs when already empty). Ctrl+modifiers
//! are ignored so global shortcuts are not shadowed.

use iocraft::prelude::{KeyCode, KeyEvent, KeyModifiers};

use jefe::state::{AppEvent, AppState, ScreenMode};

use super::{
    AppStateHandle, SharedContext,
    normal::{
        KeyHandling, handle_dashboard_actions_key, handle_dashboard_issues_key,
        handle_dashboard_prs_key,
    },
};

/// Dashboard-mode entry keys (issues/PRs/actions entry, special mode, grab).
///
/// Extracted from `handle_normal_key_event` so the parent stays within the
/// clippy 60-line function budget.
pub(super) fn resolve_dashboard_mode_entry(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
    screen_mode: ScreenMode,
) -> KeyHandling {
    if let KeyHandling::Handled(event) =
        handle_dashboard_issues_key(app_state, ctx, key_event, screen_mode)
    {
        return KeyHandling::Handled(event);
    }
    if let KeyHandling::Handled(event) =
        handle_dashboard_prs_key(app_state, ctx, key_event, screen_mode)
    {
        return KeyHandling::Handled(event);
    }
    if let KeyHandling::Handled(event) =
        handle_dashboard_actions_key(app_state, ctx, key_event, screen_mode)
    {
        return KeyHandling::Handled(event);
    }
    KeyHandling::Unhandled
}

/// Resolve a key while the dashboard search input is focused.
#[must_use]
pub(super) fn resolve_dashboard_search_key(
    state: &AppState,
    key_event: &KeyEvent,
) -> Option<AppEvent> {
    // Never swallow Ctrl-/Alt-/Super-combos: let global shortcuts (quit, etc.)
    // through to the normal tier.
    if key_event.modifiers.intersects(
        KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::META,
    ) {
        return None;
    }
    match key_event.code {
        KeyCode::Enter => Some(AppEvent::BlurDashboardSearch),
        KeyCode::Esc if state.dashboard_search.query.is_empty() => {
            Some(AppEvent::BlurDashboardSearch)
        }
        KeyCode::Esc => Some(AppEvent::ClearDashboardSearch),
        KeyCode::Char(c) => {
            let mut query = state.dashboard_search.query.clone();
            query.push(c);
            Some(AppEvent::SetDashboardSearchQuery { query })
        }
        KeyCode::Backspace => {
            let mut query = state.dashboard_search.query.clone();
            query.pop();
            Some(AppEvent::SetDashboardSearchQuery { query })
        }
        _ => None,
    }
}

/// Dashboard "search lite" focus gate (issue #405).
///
/// When the dashboard search input is focused, route keys to the search
/// resolver before any other key handler. Returns `Some(evt)` when the key
/// was consumed by the search input.
#[must_use]
pub(super) fn resolve_dashboard_search_focus(
    app_state: &AppStateHandle,
    key_event: &KeyEvent,
    screen_mode: ScreenMode,
) -> Option<AppEvent> {
    let state = app_state.read();
    if screen_mode != ScreenMode::Dashboard || !state.dashboard_search.input_focused {
        return None;
    }
    let event = resolve_dashboard_search_key(&state, key_event);
    drop(state);
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use iocraft::prelude::KeyEventKind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(KeyEventKind::Press, code)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        let mut e = key(code);
        e.modifiers = KeyModifiers::CONTROL;
        e
    }

    #[test]
    fn char_appends_to_query() {
        let mut state = AppState::default();
        state.dashboard_search.query = "al".to_string();
        let evt = resolve_dashboard_search_key(&state, &key(KeyCode::Char('p')));
        match evt {
            Some(AppEvent::SetDashboardSearchQuery { query }) => assert_eq!(query, "alp"),
            other => panic!("expected SetDashboardSearchQuery, got {other:?}"),
        }
    }

    #[test]
    fn backspace_pops_last_char() {
        let mut state = AppState::default();
        state.dashboard_search.query = "alp".to_string();
        let evt = resolve_dashboard_search_key(&state, &key(KeyCode::Backspace));
        match evt {
            Some(AppEvent::SetDashboardSearchQuery { query }) => assert_eq!(query, "al"),
            other => panic!("expected SetDashboardSearchQuery, got {other:?}"),
        }
    }

    #[test]
    fn enter_blurs_keeping_query() {
        let mut state = AppState::default();
        state.dashboard_search.query = "alpha".to_string();
        let evt = resolve_dashboard_search_key(&state, &key(KeyCode::Enter));
        assert!(matches!(evt, Some(AppEvent::BlurDashboardSearch)));
    }

    #[test]
    fn esc_on_empty_blurs_only() {
        let mut state = AppState::default();
        state.dashboard_search.query = String::new();
        let evt = resolve_dashboard_search_key(&state, &key(KeyCode::Esc));
        assert!(matches!(evt, Some(AppEvent::BlurDashboardSearch)));
    }

    #[test]
    fn esc_on_nonempty_clears_query() {
        let mut state = AppState::default();
        state.dashboard_search.query = "alpha".to_string();
        let evt = resolve_dashboard_search_key(&state, &key(KeyCode::Esc));
        assert!(matches!(evt, Some(AppEvent::ClearDashboardSearch)));
    }

    #[test]
    fn ctrl_combos_pass_through() {
        let mut state = AppState::default();
        state.dashboard_search.query = String::new();
        // Ctrl+C must NOT be swallowed by the search input.
        assert!(resolve_dashboard_search_key(&state, &ctrl(KeyCode::Char('c'))).is_none());
    }
}
