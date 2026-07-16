//! Message bus routing contract tests.

use std::path::PathBuf;

use jefe::domain::{
    ActionsFilter, Agent, AgentId, AgentStatus, DEFAULT_SANDBOX_FLAGS, LaunchSignature,
    RemoteRepositorySettings, Repository, RepositoryId, RuntimeBinding, SandboxEngine,
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
        (
            AppEvent::IssueSelfAssignmentFailed {
                owner_repo: "owner/repo".into(),
                issue_number: 42,
                error: "boom".into(),
            },
            MessageDomain::Issues,
            "IssueSelfAssignmentFailed",
        ),
    ];

    for (event, domain, name) in routes {
        let route = AppMessage::from(event).route();
        assert_eq!(route.domain, domain);
        assert_eq!(route.name, name);
    }
}

#[test]
fn actions_job_inspection_intents_route_to_actions_channel() {
    for (event, name) in [
        (AppEvent::ActionsExpandJob, "ActionsExpandJob"),
        (AppEvent::ActionsCollapseJob, "ActionsCollapseJob"),
        (AppEvent::ActionsDetailEscape, "ActionsDetailEscape"),
    ] {
        let route = AppMessage::from(event).route();
        assert_eq!(route.domain, MessageDomain::Actions);
        assert_eq!(route.name, name);
    }
}

#[test]
fn actions_page_results_route_to_actions_channel() {
    let loaded = AppEvent::ActionsRunsPageLoaded {
        scope_repo_id: RepositoryId("repo-1".into()),
        filter: Box::new(ActionsFilter::default()),
        page: 2,
        request_id: 2,
        runs: Vec::new(),
        has_more: false,
    };
    let failed = AppEvent::ActionsRunsPageLoadFailed {
        scope_repo_id: RepositoryId("repo-1".into()),
        filter: Box::new(ActionsFilter::default()),
        page: 2,
        request_id: 2,
        error: "network".into(),
    };

    for (event, name) in [
        (loaded, "ActionsRunsPageLoaded"),
        (failed, "ActionsRunsPageLoadFailed"),
    ] {
        let route = AppMessage::from(event).route();
        assert_eq!(route.domain, MessageDomain::Actions);
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
            code_puppy_version: String::new(),
            code_puppy_yolo: Some(false),
            code_puppy_quick_resume: false,
            mode_flags: vec!["--yolo".to_string()],
            llxprt_debug: String::new(),
            pass_continue: true,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: DEFAULT_SANDBOX_FLAGS.to_string(),
            remote: RemoteRepositorySettings::default(),
            agent_kind: jefe::domain::AgentKind::Llxprt,
            llxprt_version: None,
        },
        attached: true,
        last_seen: None,
        process_identity: None,
        pid: None,
        lifecycle_generation: 0,
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
fn typed_apply_search_commits_query_and_clears_list() {
    use jefe::domain::IssueFilter;

    let mut state = AppState::default();
    state.issues_state.search_input_focused = true;
    state.issues_state.search_query = "  open bug  ".to_string();
    // Establish list state (cursor + selection) with an empty item set so we
    // can verify ApplySearch clears the cursor/selection. items_mut is
    // crate-private, so drive through the public reload+load API instead.
    let repo_id = RepositoryId("repo-1".to_string());
    let filter = IssueFilter::default();
    state.mark_issue_list_reload_loading(repo_id.clone(), filter.clone(), 1);
    let mut state = state.apply(AppEvent::IssueListLoaded {
        scope_repo_id: repo_id,
        filter: Box::new(filter),
        request_id: 1,
        issues: vec![],
        cursor: Some("cursor".to_string()),
        has_more: true,
    });
    state.issues_state.list.set_selected_index(Some(0));

    let state = state.apply_message(AppMessage::Issues(IssuesMessage::ApplySearch));

    assert_eq!(state.issues_state.committed_filter.query_text, "open bug");
    assert!(!state.issues_state.search_input_focused);
    assert!(state.issues_state.issues().is_empty());
    assert_eq!(state.issues_state.selected_issue_index(), None);
    assert!(state.issues_state.issue_detail.is_none());
    assert!(!state.issues_state.has_more_issues());
    assert!(!state.issues_state.list_loading());
}

#[test]
fn issue_self_assignment_failed_round_trips_through_message_bus() {
    // The non-blocking self-assignment failure must survive the
    // AppEvent -> AppMessage -> AppEvent conversion with all fields intact
    // (issue #186), so the reducer always sees owner_repo + issue_number +
    // error for the warning message.
    let event = AppEvent::IssueSelfAssignmentFailed {
        owner_repo: "owner/repo".to_string(),
        issue_number: 42,
        error: "repo restricts assignees".to_string(),
    };

    let message: AppMessage = event.into();
    let AppMessage::Issues(IssuesMessage::IssueSelfAssignmentFailed {
        owner_repo,
        issue_number,
        error,
    }) = message
    else {
        panic!("event must convert to an Issues::IssueSelfAssignmentFailed message");
    };
    assert_eq!(owner_repo, "owner/repo");
    assert_eq!(issue_number, 42);
    assert_eq!(error, "repo restricts assignees");

    let round_tripped: AppEvent = IssuesMessage::IssueSelfAssignmentFailed {
        owner_repo,
        issue_number,
        error,
    }
    .into();
    let AppEvent::IssueSelfAssignmentFailed {
        owner_repo,
        issue_number,
        error,
    } = round_tripped
    else {
        panic!("message must convert back to AppEvent::IssueSelfAssignmentFailed");
    };
    assert_eq!(owner_repo, "owner/repo");
    assert_eq!(issue_number, 42);
    assert_eq!(error, "repo restricts assignees");
}

#[cfg(unix)]
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
