//! Message bus routing contract tests.

use std::path::PathBuf;

use jefe::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
use jefe::messages::{
    AppMessage, MessageDomain, ModalMessage, RepositoryAgentMessage, RuntimeMessage,
    UiNavigationMessage,
};
use jefe::state::{AppEvent, AppState, ModalState, PaneFocus};

#[test]
fn app_events_route_to_domain_channels() {
    let routes = [
        (
            AppEvent::NavigateUp,
            MessageDomain::UiNavigation,
            "NavigateUp",
        ),
        (AppEvent::OpenHelp, MessageDomain::Modal, "OpenHelp"),
        (
            AppEvent::OpenNewRepository,
            MessageDomain::RepositoryAgent,
            "OpenNewRepository",
        ),
        (
            AppEvent::KillAgent(AgentId("agent-1".into())),
            MessageDomain::Runtime,
            "KillAgent",
        ),
        (
            AppEvent::PersistenceSaveFailed("disk full".into()),
            MessageDomain::Persistence,
            "PersistenceSaveFailed",
        ),
        (
            AppEvent::ThemeResolveFailed("missing".into()),
            MessageDomain::Theme,
            "ThemeResolveFailed",
        ),
        (
            AppEvent::EnterIssuesMode,
            MessageDomain::Issues,
            "EnterIssuesMode",
        ),
        (
            AppEvent::ClearWarning,
            MessageDomain::System,
            "ClearWarning",
        ),
    ];

    for (event, domain, name) in routes {
        let route = AppMessage::from(event).route();
        assert_eq!(route.domain, domain);
        assert_eq!(route.name, name);
    }
}

#[test]
fn typed_navigation_message_updates_state_without_global_event_match() {
    let state = AppState {
        repositories: vec![
            Repository::new(
                RepositoryId("repo-1".into()),
                "Repo One".into(),
                "repo-one".into(),
                PathBuf::from("/repo-one"),
            ),
            Repository::new(
                RepositoryId("repo-2".into()),
                "Repo Two".into(),
                "repo-two".into(),
                PathBuf::from("/repo-two"),
            ),
        ],
        selected_repository_index: Some(0),
        pane_focus: PaneFocus::Repositories,
        ..AppState::default()
    };

    let next = state.apply_message(AppMessage::UiNavigation(UiNavigationMessage::NavigateDown));

    assert_eq!(next.selected_repository_index, Some(1));
}

#[test]
fn typed_modal_and_repository_messages_route_to_isolated_handlers() {
    let state = AppState::default().apply_message(AppMessage::Modal(ModalMessage::OpenHelp));
    assert!(matches!(state.modal, ModalState::Help));

    let state = state.apply_message(AppMessage::RepositoryAgent(
        RepositoryAgentMessage::OpenNewRepository,
    ));
    assert!(matches!(state.modal, ModalState::NewRepository { .. }));
}

#[test]
fn typed_runtime_message_only_updates_runtime_domain_state() {
    let agent_id = AgentId("agent-1".into());
    let state = AppState {
        agents: vec![Agent::new(
            agent_id.clone(),
            RepositoryId("repo-1".into()),
            "Agent One".into(),
            PathBuf::from("/repo-one/agent-one"),
        )],
        ..AppState::default()
    };

    let next = state.apply_message(AppMessage::Runtime(RuntimeMessage::AgentStatusChanged(
        agent_id.clone(),
        AgentStatus::Running,
    )));

    assert_eq!(next.agents[0].status, AgentStatus::Running);
    assert_eq!(next.agents[0].id, agent_id);
}

#[test]
fn architecture_boundary_script_passes_in_ci_tests() {
    let output = std::process::Command::new("bash")
        .arg("scripts/check-architecture.sh")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output();
    let Ok(output) = output else {
        panic!("architecture boundary script should run");
    };

    assert!(
        output.status.success(),
        "architecture boundary script failed
stdout:
{}
stderr:
{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
