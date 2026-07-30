//! Focused one-resolution production-route tests for issue #383 S3.

use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use jefe::domain::action_registry::{HandlerKey, Resolution};
use jefe::state::{AppState, ErrorsFocus, PaneFocus, ScreenMode};

use super::resolve_compiled_registry_key;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(KeyEventKind::Press, code)
}

fn modified(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    let mut event = key(code);
    event.modifiers = modifiers;
    event
}

fn assert_handler(state: &AppState, event: &KeyEvent, expected: HandlerKey) {
    let resolved = resolve_compiled_registry_key(state, event);
    assert!(
        matches!(resolved.resolution, Resolution::Dispatch { handler, .. } if handler == expected),
        "unexpected resolution: {:?}",
        resolved.resolution
    );
}

#[test]
fn dashboard_and_split_use_registry_handlers() {
    assert_handler(
        &AppState::default(),
        &key(KeyCode::Down),
        HandlerKey::NavigateDown,
    );
    let split = AppState {
        screen_mode: ScreenMode::Split,
        ..AppState::default()
    };
    assert_handler(
        &split,
        &key(KeyCode::PageDown),
        HandlerKey::NavigatePageDown,
    );
    assert_handler(
        &split,
        &modified(KeyCode::Char('r'), KeyModifiers::CONTROL),
        HandlerKey::RestartSelectedAgent,
    );
}

#[test]
fn errors_reverse_cycle_and_detail_scroll_use_registry_handlers() {
    let mut state = AppState {
        screen_mode: ScreenMode::DashboardErrors,
        ..AppState::default()
    };
    assert_handler(&state, &key(KeyCode::Left), HandlerKey::ErrorsCyclePane);
    state.errors_state.focus = ErrorsFocus::ErrorDetail;
    assert_handler(&state, &key(KeyCode::Char('j')), HandlerKey::ErrorsDown);
}

#[test]
fn terminal_and_actions_pre_mode_use_registry_handlers() {
    let terminal = AppState {
        pane_focus: PaneFocus::Terminal,
        terminal_focused: true,
        ..AppState::default()
    };
    assert_handler(
        &terminal,
        &key(KeyCode::End),
        HandlerKey::TerminalScrollTail,
    );
    let actions = AppState {
        screen_mode: ScreenMode::DashboardActions,
        ..AppState::default()
    };
    assert_handler(
        &actions,
        &key(KeyCode::F(12)),
        HandlerKey::ToggleTerminalFocus,
    );
}
