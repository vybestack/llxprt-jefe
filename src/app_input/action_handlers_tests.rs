//! Focused closed-handler planning tests for issue #383 S3.

use jefe::domain::action_registry::HandlerKey;
use jefe::domain::keymap::Chord;
use jefe::list_viewport::PageItemCount;
use jefe::state::{AppEvent, AppState, ErrorsFocus, ScreenMode};

use super::action_handlers::{BoundaryAction, HandlerExecution, execution_for};

fn chord(text: &str) -> Chord {
    let result = Chord::parse(text);
    let Ok(chord) = result else {
        panic!("test chord should parse, got {result:?}");
    };
    chord
}

#[test]
fn page_navigation_produces_typed_page_event() {
    let state = AppState {
        screen_mode: ScreenMode::Split,
        ..AppState::default()
    };
    let execution = execution_for(
        HandlerKey::NavigatePageDown,
        chord("PageDown"),
        &state,
        PageItemCount::new(7),
    );
    assert!(matches!(
        execution,
        HandlerExecution::Event(AppEvent::NavigatePageDown(count))
            if count == PageItemCount::new(7)
    ));
}

#[test]
fn errors_back_and_reverse_cycle_preserve_focus_behavior() {
    let mut state = AppState {
        screen_mode: ScreenMode::DashboardErrors,
        ..AppState::default()
    };
    state.errors_state.focus = ErrorsFocus::ErrorDetail;
    assert!(matches!(
        execution_for(
            HandlerKey::ErrorsBack,
            chord("Esc"),
            &state,
            PageItemCount::new(1),
        ),
        HandlerExecution::Event(AppEvent::RefocusErrorList)
    ));
    assert!(matches!(
        execution_for(
            HandlerKey::ErrorsCyclePane,
            chord("Left"),
            &state,
            PageItemCount::new(1),
        ),
        HandlerExecution::Event(AppEvent::ErrorsCycleFocusReverse)
    ));
    state.errors_state.focus = ErrorsFocus::ErrorList;
    assert!(matches!(
        execution_for(
            HandlerKey::ErrorsDown,
            chord("j"),
            &state,
            PageItemCount::new(1),
        ),
        HandlerExecution::Noop
    ));
}

#[test]
fn terminal_tail_at_follow_tail_forwards_to_pty() {
    let state = AppState::default();
    assert!(matches!(
        execution_for(
            HandlerKey::TerminalScrollTail,
            chord("End"),
            &state,
            PageItemCount::new(1),
        ),
        HandlerExecution::Boundary(BoundaryAction::ForwardToPty)
    ));
}
