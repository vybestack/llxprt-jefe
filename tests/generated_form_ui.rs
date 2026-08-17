//! Behavioral tests for issue #382 S6's generated agent form UI state.

#[path = "common/app_state.rs"]
mod common_app_state;

use jefe::agent_status_view::AgentAvailabilityObservation;
use jefe::domain::agent_definition::{AgentDefinition, Availability, FieldValue, Operation};
use jefe::state::generated_agent_form::{
    GeneratedAgentFormFocus, GeneratedAgentFormIntent, GeneratedTarget,
};
use jefe::state::transition::TransitionExt;
use jefe::state::{AppEvent, AppState, ModalState, PaneFocus};

fn claude_definition() -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == "core.claude-code")
        .unwrap_or_else(|| panic!("Claude definition must be shipped"))
}

fn compatible() -> Availability {
    Availability::InstalledCompatible {
        identity: "2.1.212 (Claude Code)".to_string(),
        generation: 9,
    }
}

fn generated_state() -> AppState {
    let definition = claude_definition();
    {
        let mut state = crate::common_app_state::app_state();
        state.pane_focus = PaneFocus::Repositories;
        state.agent_type_availability = vec![AgentAvailabilityObservation::new(
            &definition,
            true,
            compatible(),
        )];
        state
    }
    .apply(AppEvent::OpenAgentTypeForm(definition.id))
    .committed_pure()
}

#[test]
fn selected_definition_generates_visible_unsupported_cells_and_fields() {
    let state = generated_state();
    let ModalState::GeneratedAgent { form, .. } = &state.modal else {
        panic!("selected definition should open a generated agent form");
    };

    assert_eq!(form.draft().display_name(), "Claude Code");
    assert_eq!(form.selected_operation(), Operation::Resume);
    assert_eq!(form.selected_target(), GeneratedTarget::Local);
    // Declared support now stands on its own: the definition declares Resume,
    // and a reachable executable is no longer second-guessed (#657).
    assert!(form.create_enabled());
    assert_eq!(form.operation_support(Operation::Resume).reason(), None);
    // Fixture-proven positional prompt, so fresh-issue carries no reason (#620).
    assert_eq!(form.operation_support(Operation::FreshIssue).reason(), None);
    assert_eq!(
        form.target_support(GeneratedTarget::Remote).reason(),
        Some("Claude remote/setup is not fixture-verified")
    );

    let ids: Vec<&str> = form
        .draft()
        .fields()
        .iter()
        .filter(|field| field.visible())
        .map(|field| field.id().as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["model", "permission_mode", "version_selector", "prompt"]
    );
}

#[test]
fn generated_reducer_cycles_focus_edits_typed_values_and_restores_exact_focus() {
    let mut state = generated_state();
    let (repositories_before, agents_before) = (state.repositories.len(), state.agents.len());
    let ModalState::GeneratedAgent { form, .. } = &mut state.modal else {
        panic!("generated form must be open");
    };

    assert_eq!(
        form.focus(),
        &GeneratedAgentFormFocus::Operation(Operation::Resume)
    );
    form.apply(GeneratedAgentFormIntent::Next);
    assert_eq!(
        form.focus(),
        &GeneratedAgentFormFocus::Operation(Operation::FreshIssue)
    );
    form.apply(GeneratedAgentFormIntent::Previous);
    assert_eq!(
        form.focus(),
        &GeneratedAgentFormFocus::Operation(Operation::Resume)
    );

    while !matches!(form.focus(), GeneratedAgentFormFocus::Field(id) if id.as_str() == "prompt") {
        form.apply(GeneratedAgentFormIntent::Next);
    }
    form.apply(GeneratedAgentFormIntent::Insert('é'));
    let prompt = form
        .draft()
        .fields()
        .iter()
        .find(|field| field.id().as_str() == "prompt")
        .unwrap_or_else(|| panic!("prompt field must remain visible"));
    assert_eq!(prompt.value(), &FieldValue::String("é".to_string()));

    form.apply(GeneratedAgentFormIntent::Activate);
    assert!(form.validated_result().is_none());
    assert_eq!(state.repositories.len(), repositories_before);
    assert_eq!(state.agents.len(), agents_before);

    state = state.apply(AppEvent::CloseModal).committed_pure();
    assert_eq!(state.modal, ModalState::None);
    assert_eq!(state.pane_focus, PaneFocus::Repositories);
}

#[test]
fn generated_modal_is_trapped_and_unsupported_submit_has_zero_state_effects() {
    let state = generated_state();
    let before_agents = state.agents.len();
    let before_repositories = state.repositories.len();
    let submitted = state.apply(AppEvent::SubmitForm).committed_pure();

    assert!(matches!(submitted.modal, ModalState::GeneratedAgent { .. }));
    assert_eq!(submitted.agents.len(), before_agents);
    assert_eq!(submitted.repositories.len(), before_repositories);
}
