//! Focused closed-handler planning tests for issue #383 S3.

use jefe::domain::action_registry::HandlerKey;
use jefe::domain::keymap::Chord;
use jefe::list_viewport::PageItemCount;
use jefe::state::{AppEvent, AppState, ErrorsFocus, ScreenId};

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
        nav: crate::state::navigation::NavState::rooted(ScreenId::Repositories),
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
        nav: crate::state::navigation::NavState::rooted(ScreenId::Errors),
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

#[test]
fn s4_modal_controls_have_typed_executions() {
    let mut state = AppState::default();
    state.modal = jefe::state::ModalState::Help;
    assert!(matches!(
        execution_for(
            HandlerKey::HelpScrollDown,
            chord("Down"),
            &state,
            PageItemCount::new(1),
        ),
        HandlerExecution::Boundary(BoundaryAction::HelpScrollDown)
    ));
    assert!(matches!(
        execution_for(
            HandlerKey::HelpClose,
            chord("Esc"),
            &state,
            PageItemCount::new(1),
        ),
        HandlerExecution::Event(AppEvent::CloseModal)
    ));
}

#[test]
fn s4_workspace_handlers_produce_source_specific_events() {
    let mut issues = AppState {
        nav: crate::state::navigation::NavState::rooted(ScreenId::Issues),
        ..AppState::default()
    };
    issues.issues_state.issue_focus = jefe::state::IssueFocus::IssueDetail;
    assert!(matches!(
        execution_for(
            HandlerKey::NavigateDown,
            chord("Down"),
            &issues,
            PageItemCount::new(5),
        ),
        HandlerExecution::Event(AppEvent::IssuesScrollDetailDown)
    ));

    let mut prs = AppState {
        nav: crate::state::navigation::NavState::rooted(ScreenId::PullRequests),
        ..AppState::default()
    };
    prs.prs_state.pr_focus = jefe::state::PrFocus::PrList;
    assert!(matches!(
        execution_for(
            HandlerKey::PullRequestsOpenBrowser,
            chord("o"),
            &prs,
            PageItemCount::new(5),
        ),
        HandlerExecution::Event(AppEvent::PrShowNotice(
            jefe::state::ReadOnlyHintKind::NoSelectionToOpen
        ))
    ));

    let mut actions = AppState {
        nav: crate::state::navigation::NavState::rooted(ScreenId::Actions),
        ..AppState::default()
    };
    actions.actions_state.focus = jefe::state::ActionsFocus::Detail;
    assert!(matches!(
        execution_for(
            HandlerKey::ActionsActivate,
            chord("Right"),
            &actions,
            PageItemCount::new(5),
        ),
        HandlerExecution::Event(AppEvent::ActionsExpandJob)
    ));
}
