//! Message bus routing contract tests.

use std::path::PathBuf;

use jefe::domain::{
    Agent, AgentId, AgentStatus, DEFAULT_SANDBOX_FLAGS, LaunchSignature, RemoteRepositorySettings,
    Repository, RepositoryId, RuntimeBinding, SandboxEngine,
};
use jefe::messages::{
    AppMessage, IssuesMessage, MessageDomain, ModalMessage, PersistenceMessage,
    RepositoryAgentMessage, RuntimeMessage, UiNavigationMessage,
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
fn typed_kill_agent_clears_runtime_binding() {
    let agent_id = AgentId("agent-1".into());
    let mut agent = Agent::new(
        agent_id.clone(),
        RepositoryId("repo-1".into()),
        "Agent One".into(),
        PathBuf::from("/repo-one/agent-one"),
    );
    agent.runtime_binding = Some(RuntimeBinding {
        session_name: "sess-agent-1".to_string(),
        launch_signature: LaunchSignature {
            work_dir: PathBuf::from("/repo-one/agent-one"),
            profile: String::new(),
            code_puppy_model: String::new(),
            code_puppy_yolo: Some(false),
            mode_flags: vec!["--yolo".to_string()],
            llxprt_debug: String::new(),
            pass_continue: true,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: DEFAULT_SANDBOX_FLAGS.to_string(),
            remote: RemoteRepositorySettings::default(),
            agent_kind: jefe::domain::AgentKind::Llxprt,
        },
        attached: true,
        last_seen: None,
        pid: None,
    });

    let state = AppState {
        agents: vec![agent],
        ..AppState::default()
    };

    let next = state.apply_message(AppMessage::Runtime(RuntimeMessage::KillAgent(agent_id)));

    assert_eq!(next.agents[0].status, AgentStatus::Dead);
    assert!(next.agents[0].runtime_binding.is_none());
}

#[test]
fn typed_persistence_save_success_clears_stale_errors() {
    let state = AppState::default().apply_message(AppMessage::Persistence(
        PersistenceMessage::SaveFailed("disk full".to_string()),
    ));
    assert_eq!(state.error_message.as_deref(), Some("disk full"));

    let state = state.apply_message(AppMessage::Persistence(PersistenceMessage::SaveSuccess));

    assert!(state.error_message.is_none());
}

#[test]
fn typed_apply_search_commits_query_and_starts_reload() {
    let mut state = AppState::default();
    state.issues_state.search_input_focused = true;
    state.issues_state.search_query = "  open bug  ".to_string();
    state.issues_state.selected_issue_index = Some(0);
    state.issues_state.list_cursor = Some("cursor".to_string());
    state.issues_state.has_more_issues = true;

    let state = state.apply_message(AppMessage::Issues(IssuesMessage::ApplySearch));

    assert_eq!(state.issues_state.committed_filter.query_text, "open bug");
    assert!(!state.issues_state.search_input_focused);
    assert!(state.issues_state.issues.is_empty());
    assert_eq!(state.issues_state.selected_issue_index, None);
    assert!(state.issues_state.issue_detail.is_none());
    assert_eq!(state.issues_state.list_cursor, None);
    assert!(!state.issues_state.has_more_issues);
    assert!(state.issues_state.loading.list);
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
