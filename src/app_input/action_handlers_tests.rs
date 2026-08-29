//! Focused closed-handler planning tests for issue #383 S3.

use jefe::domain::action_registry::HandlerKey;
use jefe::domain::keymap::Chord;
use jefe::list_viewport::PageItemCount;
use jefe::state::{AppEvent, ErrorsFocus, ScreenId, transition::TransitionExt};

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
    let mut state = crate::test_app_state();
    state.nav = crate::state::navigation::NavState::rooted(ScreenId::Repositories);
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
fn every_back_handler_enters_the_one_typed_back_reducer() {
    let mut state = crate::test_app_state();
    state.nav = crate::state::navigation::NavState::rooted(ScreenId::Errors);
    state.errors_state.focus = ErrorsFocus::ErrorDetail;
    for handler in [
        HandlerKey::SettingsBack,
        HandlerKey::WorkbenchBack,
        HandlerKey::HelpClose,
        HandlerKey::ExitSplit,
        HandlerKey::ErrorsBack,
        HandlerKey::TerminalManagerBack,
        HandlerKey::ConfirmCancel,
        HandlerKey::AuthCancel,
        HandlerKey::FormCancel,
        HandlerKey::SearchCancel,
        HandlerKey::FilterCancel,
        HandlerKey::IssuesExit,
        HandlerKey::IssuesBack,
        HandlerKey::IssuesCancelInline,
        HandlerKey::IssuesChooserCancel,
        HandlerKey::PullRequestsExit,
        HandlerKey::PullRequestsBack,
        HandlerKey::PullRequestsCancelInline,
        HandlerKey::PullRequestsChooserCancel,
        HandlerKey::ActionsExit,
        HandlerKey::ActionsBack,
    ] {
        let execution = execution_for(handler, chord("Esc"), &state, PageItemCount::new(1));
        assert!(
            matches!(execution, HandlerExecution::Event(AppEvent::Back)),
            "{handler:?} produced {execution:?}"
        );
    }
}

#[test]
fn terminal_manager_f12_preserves_its_non_back_exit_behavior() {
    let state = crate::test_app_state();
    assert!(matches!(
        execution_for(
            HandlerKey::TerminalManagerBack,
            chord("F12"),
            &state,
            PageItemCount::new(1),
        ),
        HandlerExecution::Event(AppEvent::ExitTerminalManagerMode)
    ));
}

#[test]
fn reverse_cycle_preserves_focus_behavior() {
    let mut state = crate::test_app_state();
    state.nav = crate::state::navigation::NavState::rooted(ScreenId::Errors);
    state.errors_state.focus = ErrorsFocus::ErrorDetail;
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
    let state = crate::test_app_state();
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
fn active_search_submit_is_interpreted_by_the_form_control() {
    let state = crate::test_app_state()
        .apply(AppEvent::OpenSearch)
        .committed_pure()
        .apply(AppEvent::FormChar('x'))
        .committed_pure();

    assert!(matches!(
        execution_for(
            HandlerKey::SearchApply,
            chord("Enter"),
            &state,
            PageItemCount::new(1),
        ),
        HandlerExecution::Event(AppEvent::CloseModal)
    ));
}

#[test]
fn s4_modal_controls_have_typed_executions() {
    let state = crate::test_app_state()
        .apply(jefe::state::AppEvent::OpenHelp)
        .committed_pure();
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
        HandlerExecution::Event(AppEvent::Back)
    ));
}

#[test]
fn s4_workspace_handlers_produce_source_specific_events() {
    let mut issues = crate::test_app_state();
    issues.nav = crate::state::navigation::NavState::rooted(ScreenId::Issues);
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

    let mut prs = crate::test_app_state();
    prs.nav = crate::state::navigation::NavState::rooted(ScreenId::PullRequests);
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

    let mut actions = crate::test_app_state();
    actions.nav = crate::state::navigation::NavState::rooted(ScreenId::Actions);
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

#[test]
fn dashboard_activation_respects_repository_focus_when_no_agents_exist() {
    let definition = jefe::domain::agent_definition::AgentDefinition::shipped()
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("shipped definitions must not be empty"));
    let repository_id = jefe::domain::RepositoryId("repo-empty".to_string());
    let mut state = crate::test_app_state();
    state.repositories = vec![jefe::domain::Repository::new(
        repository_id.clone(),
        definition.id.clone(),
        jefe::domain::TypedMap::new(),
        "Empty repository".to_string(),
        "repo-empty".to_string(),
        std::path::PathBuf::from("/tmp/repo-empty"),
    )];
    state.selected_repository_index = Some(0);
    state.pane_focus = jefe::state::PaneFocus::Repositories;
    state.agent_type_availability = vec![
        jefe::agent_status_view::AgentAvailabilityObservation::not_found(&definition, true, 1),
    ];

    assert!(matches!(
        execution_for(
            HandlerKey::ActivateDashboardSelection,
            chord("Enter"),
            &state,
            PageItemCount::new(1),
        ),
        HandlerExecution::Event(AppEvent::OpenEditRepository(id)) if id == repository_id
    ));

    state.pane_focus = jefe::state::PaneFocus::Agents;
    assert!(matches!(
        execution_for(
            HandlerKey::ActivateDashboardSelection,
            chord("Enter"),
            &state,
            PageItemCount::new(1),
        ),
        HandlerExecution::Event(AppEvent::OpenAgentTypeForm(id)) if id == definition.id
    ));
}
