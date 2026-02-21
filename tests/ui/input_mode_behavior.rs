//! Behavioral tests for high-level input modes and routing.
//!
//! These tests define contract-level behavior for mode resolution and
//! search key routing so the app cannot regress into sticky state bugs.

use iocraft::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use jefe::input::{InputMode, SearchKeyRoute, input_mode_for_state, route_search_key};
use jefe::state::{AppState, ModalState, PaneFocus};

fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    let mut event = KeyEvent::new(KeyEventKind::Press, code);
    event.modifiers = modifiers;
    event
}

#[test]
fn mode_prefers_search_over_terminal_capture() {
    let mut state = AppState {
        terminal_focused: true,
        pane_focus: PaneFocus::Terminal,
        modal: ModalState::Search {
            query: String::from("abc"),
        },
        ..AppState::default()
    };

    assert_eq!(input_mode_for_state(&state), InputMode::Search);

    state.modal = ModalState::None;
    assert_eq!(input_mode_for_state(&state), InputMode::TerminalCapture);
}

#[test]
fn mode_for_terminal_focused_without_terminal_pane_is_normal() {
    let state = AppState {
        terminal_focused: true,
        pane_focus: PaneFocus::Agents,
        modal: ModalState::None,
        ..AppState::default()
    };

    assert_eq!(input_mode_for_state(&state), InputMode::Normal);
}

#[test]
fn search_route_esc_closes_and_consumes() {
    let key = key_event(KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(route_search_key(&key), SearchKeyRoute::CloseAndConsume);
}

#[test]
fn search_route_enter_closes_and_consumes() {
    let key = key_event(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(route_search_key(&key), SearchKeyRoute::CloseAndConsume);
}

#[test]
fn search_route_backspace_edits_query() {
    let key = key_event(KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!(route_search_key(&key), SearchKeyRoute::Backspace);
}

#[test]
fn search_route_printable_char_edits_query() {
    let key = key_event(KeyCode::Char('x'), KeyModifiers::NONE);
    assert_eq!(route_search_key(&key), SearchKeyRoute::EditQueryChar('x'));
}

#[test]
fn search_route_control_char_closes_and_reroutes() {
    let key = key_event(KeyCode::Char('n'), KeyModifiers::CONTROL);
    assert_eq!(route_search_key(&key), SearchKeyRoute::CloseAndReroute);
}

#[test]
fn search_route_arrow_closes_and_reroutes() {
    let key = key_event(KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(route_search_key(&key), SearchKeyRoute::CloseAndReroute);
}
