//! Integration tests for the production generated agent form Create path
//! (issue #382 S6).
//!
//! Proves that an enabled Create routes through canonical agent creation
//! (exactly one agent, modal closed, selection updated) and that an
//! unsupported Create has zero state/runtime/persistence effects.

use jefe::agent_status_view::AgentAvailabilityObservation;
use jefe::domain::agent_definition::{AgentDefinition, AgentTypeId, Availability, Operation};
use jefe::domain::{Id, Repository, RepositoryId, TypedMap};
use jefe::state::generated_agent_form::{
    GeneratedAgentFormFocus, GeneratedAgentFormIntent, GeneratedTarget,
};
use jefe::state::transition::TransitionExt;
use jefe::state::{AppEvent, AppState, ModalState, PaneFocus};

fn llxprt_definition() -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == "core.llxprt")
        .unwrap_or_else(|| panic!("LLxprt definition must be shipped"))
}

fn claude_definition() -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == "core.claude-code")
        .unwrap_or_else(|| panic!("Claude definition must be shipped"))
}

fn compatible_with_capability(capability: &str) -> Availability {
    Availability::InstalledCompatible {
        identity: "0.10.0".to_string(),
        capabilities: vec![capability.to_string()],
        generation: 1,
    }
}

fn compatible_without_capabilities() -> Availability {
    Availability::InstalledCompatible {
        identity: "2.1.212 (Claude Code)".to_string(),
        capabilities: Vec::new(),
        generation: 9,
    }
}

fn test_repository(type_id: AgentTypeId) -> Repository {
    Repository::new(
        RepositoryId("test-repo".to_string()),
        type_id,
        TypedMap::new(),
        "Test Repo".to_string(),
        "test-repo".to_string(),
        std::path::PathBuf::from("/tmp/jefe-test-generated-submit"),
    )
}

fn state_with_llxprt_form_open_and_create_enabled() -> AppState {
    let definition = llxprt_definition();
    let availability = compatible_with_capability("prompt-interactive");
    let observation = AgentAvailabilityObservation::new(&definition, true, availability);
    let repository = test_repository(definition.id.clone());
    AppState {
        repositories: vec![repository],
        selected_repository_index: Some(0),
        agent_type_availability: vec![observation],
        pane_focus: PaneFocus::Agents,
        ..AppState::default()
    }
    .apply(AppEvent::OpenAgentTypeForm(definition.id))
    .committed_pure()
}

fn focus_create_and_activate(state: &mut AppState) {
    let ModalState::GeneratedAgent { form, .. } = &mut state.modal else {
        panic!("generated agent form must be open");
    };
    while !matches!(form.focus(), GeneratedAgentFormFocus::Create) {
        form.apply(GeneratedAgentFormIntent::Next);
    }
    form.apply(GeneratedAgentFormIntent::Activate);
}

#[test]
fn enabled_create_creates_exactly_one_agent_and_closes_modal() {
    let mut state = state_with_llxprt_form_open_and_create_enabled();
    let before_agents = state.agents.len();
    let before_repositories = state.repositories.len();

    // Navigate to Create and activate so the validated_result is staged.
    focus_create_and_activate(&mut state);

    // Apply SubmitForm — this consumes the validated_result through the
    // canonical path: TypedMap conversion, agent creation, modal close.
    let submitted = state.apply(AppEvent::SubmitForm).committed_pure();

    assert_eq!(
        submitted.modal,
        ModalState::None,
        "modal must close on success"
    );
    assert_eq!(
        submitted.agents.len(),
        before_agents + 1,
        "exactly one agent created"
    );
    assert_eq!(
        submitted.repositories.len(),
        before_repositories,
        "no repository side effects"
    );

    let agent = &submitted.agents[0];
    assert_eq!(agent.name, "LLxprt");
    assert_eq!(agent.type_id, llxprt_definition().id);
    assert_eq!(agent.repository_id, RepositoryId("test-repo".to_string()));

    // TypedMap normalization: underscored field IDs become hyphenated typed IDs.
    let version_selector =
        Id::parse("version-selector").unwrap_or_else(|error| panic!("valid Id: {error}"));
    assert!(
        agent.values.contains_key(&version_selector),
        "version_selector must be normalized to version-selector in TypedMap"
    );
}

#[test]
fn enabled_create_consumes_validated_result_exactly_once() {
    let mut state = state_with_llxprt_form_open_and_create_enabled();
    focus_create_and_activate(&mut state);

    // After Activate on Create, the validated_result is staged.
    if let ModalState::GeneratedAgent { form, .. } = &state.modal {
        assert!(form.validated_result().is_some());
    }

    // SubmitForm consumes it.
    let submitted = state.apply(AppEvent::SubmitForm).committed_pure();
    assert_eq!(submitted.agents.len(), 1);

    // A second SubmitForm must not create another agent (modal is closed,
    // so it falls through to the generic no-op path).
    let resubmitted = submitted.apply(AppEvent::SubmitForm).committed_pure();
    assert_eq!(
        resubmitted.agents.len(),
        1,
        "validated_result must be consumed exactly once"
    );
}

#[test]
fn unsupported_create_creates_zero_agents_and_keeps_modal_open() {
    // Claude with no capabilities: Resume is unsupported, so create_enabled()
    // is false. SubmitForm must have zero state effects.
    let definition = claude_definition();
    let observation =
        AgentAvailabilityObservation::new(&definition, true, compatible_without_capabilities());
    let state = AppState {
        repositories: vec![test_repository(definition.id.clone())],
        selected_repository_index: Some(0),
        agent_type_availability: vec![observation],
        pane_focus: PaneFocus::Agents,
        ..AppState::default()
    }
    .apply(AppEvent::OpenAgentTypeForm(definition.id))
    .committed_pure();

    let before_agents = state.agents.len();
    let before_repositories = state.repositories.len();

    let submitted = state.apply(AppEvent::SubmitForm).committed_pure();

    // Zero state effects: modal stays open, no agents/repositories added.
    assert!(
        matches!(submitted.modal, ModalState::GeneratedAgent { .. }),
        "unsupported Create must keep the modal open"
    );
    assert_eq!(
        submitted.agents.len(),
        before_agents,
        "unsupported Create must create zero agents"
    );
    assert_eq!(
        submitted.repositories.len(),
        before_repositories,
        "unsupported Create must not touch repositories"
    );
}

#[test]
fn enabled_create_restores_pane_focus_from_modal() {
    let mut state = state_with_llxprt_form_open_and_create_enabled();
    focus_create_and_activate(&mut state);

    let submitted = state.apply(AppEvent::SubmitForm).committed_pure();
    // return_focus was PaneFocus::Agents when the modal opened.
    assert_eq!(
        submitted.pane_focus,
        PaneFocus::Agents,
        "pane focus must be restored from the modal's return_focus"
    );
}

#[test]
fn enabled_create_selects_remote_target_respects_repository_remote() {
    let definition = llxprt_definition();
    let availability = compatible_with_capability("prompt-interactive");
    let observation = AgentAvailabilityObservation::new(&definition, true, availability);
    let mut repository = test_repository(definition.id.clone());
    // LLxprt supports remote targets; we do not invent remote settings, but
    // the agent's work_dir for a remote target reuses the repository base dir.
    repository.remote.enabled = true;

    let mut state = AppState {
        repositories: vec![repository],
        selected_repository_index: Some(0),
        agent_type_availability: vec![observation],
        pane_focus: PaneFocus::Agents,
        ..AppState::default()
    }
    .apply(AppEvent::OpenAgentTypeForm(definition.id))
    .committed_pure();

    // Select the Remote target.
    {
        let ModalState::GeneratedAgent { form, .. } = &mut state.modal else {
            panic!("generated form must be open");
        };
        while !matches!(
            form.focus(),
            GeneratedAgentFormFocus::Target(GeneratedTarget::Remote)
        ) {
            form.apply(GeneratedAgentFormIntent::Next);
        }
        form.apply(GeneratedAgentFormIntent::Activate);
        assert_eq!(form.selected_target(), GeneratedTarget::Remote);

        while !matches!(form.focus(), GeneratedAgentFormFocus::Create) {
            form.apply(GeneratedAgentFormIntent::Next);
        }
        form.apply(GeneratedAgentFormIntent::Activate);
    }

    let submitted = state.apply(AppEvent::SubmitForm).committed_pure();
    assert_eq!(submitted.agents.len(), 1);
    // Remote target: work_dir reuses repository base dir (no path joining).
    assert_eq!(
        submitted.agents[0].work_dir,
        std::path::PathBuf::from("/tmp/jefe-test-generated-submit")
    );
}

#[test]
fn enabled_create_preserves_selected_operation_in_result() {
    let mut state = state_with_llxprt_form_open_and_create_enabled();

    // Select Normal operation (LLxprt supports all operations).
    {
        let ModalState::GeneratedAgent { form, .. } = &mut state.modal else {
            panic!("generated form must be open");
        };
        while !matches!(
            form.focus(),
            GeneratedAgentFormFocus::Operation(Operation::Normal)
        ) {
            form.apply(GeneratedAgentFormIntent::Next);
        }
        form.apply(GeneratedAgentFormIntent::Activate);
        assert_eq!(form.selected_operation(), Operation::Normal);

        while !matches!(form.focus(), GeneratedAgentFormFocus::Create) {
            form.apply(GeneratedAgentFormIntent::Next);
        }
        form.apply(GeneratedAgentFormIntent::Activate);
        let Some(result) = form.validated_result() else {
            panic!("validated_result must be staged after Create activate");
        };
        assert_eq!(result.operation, Operation::Normal);
    }

    let submitted = state.apply(AppEvent::SubmitForm).committed_pure();
    assert_eq!(
        submitted.agents.len(),
        1,
        "Normal operation must be supported and create one agent"
    );
}

#[test]
fn submit_without_repository_has_zero_effects() {
    // No repositories at all — agent creation must fail gracefully with
    // zero state effects.
    let definition = llxprt_definition();
    let availability = compatible_with_capability("prompt-interactive");
    let observation = AgentAvailabilityObservation::new(&definition, true, availability);
    let mut state = AppState {
        repositories: Vec::new(),
        agent_type_availability: vec![observation],
        ..AppState::default()
    }
    .apply(AppEvent::OpenAgentTypeForm(definition.id))
    .committed_pure();

    focus_create_and_activate(&mut state);

    let before_agents = state.agents.len();
    let submitted = state.apply(AppEvent::SubmitForm).committed_pure();

    assert_eq!(
        submitted.agents.len(),
        before_agents,
        "no repository means zero agents created"
    );
}
