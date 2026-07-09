use super::*;
use crate::domain::{RemoteRepositorySettings, RepositoryId};
use crate::state::types::{AppEvent, ModalState, ScreenMode};

fn seed_repository() -> Repository {
    Repository {
        id: RepositoryId("repo-1".to_owned()),
        name: "Repo 1".to_owned(),
        slug: "repo-1".to_owned(),
        base_dir: std::path::PathBuf::from("/tmp/repo-1"),
        default_profile: String::new(),
        github_repo: String::new(),
        remote: RemoteRepositorySettings::default(),
        issue_base_prompt: String::new(),
        agent_ids: Vec::new(),
    }
}

#[test]
fn default_state_has_no_selection() {
    let state = AppState::default();
    assert!(state.selected_repository_index.is_none());
    assert!(state.selected_agent_index.is_none());
}

#[test]
fn default_state_is_dashboard_mode() {
    let state = AppState::default();
    assert_eq!(state.screen_mode, ScreenMode::Dashboard);
}

#[test]
fn default_state_terminal_unfocused() {
    let state = AppState::default();
    assert!(!state.terminal_focused);
}

#[test]
fn open_new_agent_initializes_llxprt_debug_blank() {
    let mut state = AppState {
        repositories: vec![seed_repository()],
        ..AppState::default()
    };

    state = state.apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())));

    let ModalState::NewAgent { fields, .. } = state.modal else {
        panic!("expected new-agent modal, got {:?}", state.modal);
    };
    assert!(fields.llxprt_debug.is_empty());
}

#[test]
fn llxprt_debug_is_trimmed_when_creating_agent() {
    let mut state = AppState {
        repositories: vec![seed_repository()],
        ..AppState::default()
    };

    state = state.apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())));
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("expected new-agent modal");
    };
    fields.name = "Agent One".to_owned();
    fields.work_dir = "/tmp/agent-one".to_owned();
    fields.llxprt_debug = "   trace=1   ".to_owned();

    state = state.apply(AppEvent::SubmitForm);
    let Some(created) = state.agents.last() else {
        panic!("agent should be created");
    };
    assert_eq!(created.llxprt_debug, "trace=1");
}

#[test]
fn llxprt_debug_is_trimmed_to_empty_when_blank() {
    let mut state = AppState {
        repositories: vec![seed_repository()],
        ..AppState::default()
    };

    state = state.apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())));
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("expected new-agent modal");
    };
    fields.name = "Agent Two".to_owned();
    fields.work_dir = "/tmp/agent-two".to_owned();
    fields.llxprt_debug = "   ".to_owned();

    state = state.apply(AppEvent::SubmitForm);
    let Some(created) = state.agents.last() else {
        panic!("agent should be created");
    };
    assert!(created.llxprt_debug.is_empty());
}

#[test]
fn new_agent_work_dir_slug_excludes_slashes_from_name() {
    let mut state = AppState {
        repositories: vec![seed_repository()],
        ..AppState::default()
    };

    state = state.apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())));
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("expected new-agent modal");
    };
    fields.name = "API / Worker".to_owned();

    state.update_agent_work_dir_from_name();

    let ModalState::NewAgent { fields, .. } = &state.modal else {
        panic!("expected new-agent modal, got {:?}", state.modal);
    };
    assert_eq!(fields.work_dir, "/tmp/repo-1/api--worker");
}

#[test]
fn remote_repository_creation_preserves_remote_base_dir_without_local_expansion() {
    let fields = RepositoryFormFields {
        name: "Remote Repo".to_owned(),
        base_dir: "~/remote/worktrees".to_owned(),
        default_profile: "ship".to_owned(),
        github_repo: String::new(),
        remote_enabled: true,
        login_user: "ubuntu".to_owned(),
        host: "170.9.234.179".to_owned(),
        run_as_user: "acoliver".to_owned(),
        setup_env_default: true,
    };

    let Some(repository) = AppState::create_repository_from_fields(&fields) else {
        panic!("repository should be created");
    };

    assert_eq!(
        repository.base_dir,
        std::path::PathBuf::from("~/remote/worktrees")
    );
    assert!(repository.remote.enabled);
    assert_eq!(repository.remote.login_user, "ubuntu");
    assert_eq!(repository.remote.host, "170.9.234.179");
    assert_eq!(repository.remote.run_as_user, "acoliver");
    assert!(repository.remote.setup_env_default);
}

#[test]
fn repository_name_that_normalizes_to_empty_slug_is_rejected() {
    let fields = RepositoryFormFields {
        name: "///".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        github_repo: String::new(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
    };

    assert!(AppState::create_repository_from_fields(&fields).is_none());
}

#[test]
fn create_agent_rejects_whitespace_only_work_dir() {
    let repository = seed_repository();
    let fields = AgentFormFields {
        shortcut_slot: None,
        name: "Agent One".to_owned(),
        description: String::new(),
        work_dir: "   \t ".to_owned(),
        profile: String::new(),
        mode: "--yolo".to_owned(),
        llxprt_debug: String::new(),
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: "podman".to_owned(),
        sandbox_flags: String::new(),
    };

    assert!(AppState::create_agent_from_fields(&repository, &fields, 1).is_none());
}

#[test]
fn update_agent_ignores_whitespace_only_work_dir() {
    let repository = seed_repository();
    let mut agent = Agent {
        id: crate::domain::AgentId("agent-1".to_owned()),
        display_id: "#1".to_owned(),
        repository_id: repository.id.clone(),
        shortcut_slot: None,
        name: "Agent One".to_owned(),
        description: String::new(),
        work_dir: std::path::PathBuf::from("/tmp/agent-one"),
        profile: String::new(),
        mode_flags: vec!["--yolo".to_owned()],
        llxprt_debug: String::new(),
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: crate::domain::SandboxEngine::Podman,
        sandbox_flags: String::new(),
        status: crate::domain::AgentStatus::Running,
        runtime_binding: None,
    };

    let fields = AgentFormFields {
        shortcut_slot: None,
        name: "Agent One".to_owned(),
        description: String::new(),
        work_dir: "   ".to_owned(),
        profile: String::new(),
        mode: "--yolo".to_owned(),
        llxprt_debug: String::new(),
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: "podman".to_owned(),
        sandbox_flags: String::new(),
    };

    AppState::update_agent_from_fields(&mut agent, &repository, &fields);
    assert_eq!(agent.work_dir, std::path::PathBuf::from("/tmp/agent-one"));
}

#[test]
fn repository_checkbox_toggle_updates_remote_fields() {
    let mut state = AppState {
        repositories: vec![seed_repository()],
        ..AppState::default()
    };
    state = state.apply(AppEvent::OpenNewRepository);
    state = state.apply(AppEvent::FormNextField); // Name → BaseDir
    state = state.apply(AppEvent::FormNextField); // BaseDir → DefaultProfile
    state = state.apply(AppEvent::FormNextField); // DefaultProfile → GitHubRepo
    state = state.apply(AppEvent::FormNextField); // GitHubRepo → RemoteEnabled
    state = state.apply(AppEvent::FormToggleCheckbox);
    state = state.apply(AppEvent::FormNextField);
    state = state.apply(AppEvent::FormChar('u'));
    state = state.apply(AppEvent::FormChar('b'));
    state = state.apply(AppEvent::FormNextField);
    state = state.apply(AppEvent::FormChar('1'));
    state = state.apply(AppEvent::FormChar('.'));
    state = state.apply(AppEvent::FormNextField);
    state = state.apply(AppEvent::FormChar('a'));
    state = state.apply(AppEvent::FormNextField);
    state = state.apply(AppEvent::FormToggleCheckbox);

    let ModalState::NewRepository {
        fields,
        focus,
        cursor,
    } = state.modal
    else {
        panic!("expected new-repository modal, got {:?}", state.modal);
    };
    assert_eq!(focus, RepositoryFormFocus::SetupEnvDefault);
    assert!(fields.remote_enabled);
    assert_eq!(fields.login_user, "ub");
    assert_eq!(fields.host, "1.");
    assert_eq!(fields.run_as_user, "a");
    assert!(fields.setup_env_default);
    assert_eq!(cursor.login_user, 2);
    assert_eq!(cursor.host, 2);
    assert_eq!(cursor.run_as_user, 1);
}

#[test]
fn create_repository_rejects_invalid_github_repo_without_slash() {
    let fields = RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        github_repo: "foo".to_owned(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
    };
    assert!(AppState::create_repository_from_fields(&fields).is_none());
}

#[test]
fn create_repository_rejects_github_repo_with_extra_slash() {
    let fields = RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        github_repo: "owner/repo/extra".to_owned(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
    };
    assert!(AppState::create_repository_from_fields(&fields).is_none());
}

#[test]
fn create_repository_rejects_github_repo_missing_owner() {
    let fields = RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        github_repo: "/repo".to_owned(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
    };
    assert!(AppState::create_repository_from_fields(&fields).is_none());
}

#[test]
fn create_repository_rejects_github_repo_missing_repo_name() {
    let fields = RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        github_repo: "owner/".to_owned(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
    };
    assert!(AppState::create_repository_from_fields(&fields).is_none());
}

#[test]
fn create_repository_accepts_empty_github_repo() {
    let fields = RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        github_repo: String::new(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
    };
    assert!(AppState::create_repository_from_fields(&fields).is_some());
}

#[test]
fn create_repository_accepts_well_formed_github_repo() {
    let fields = RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        github_repo: "owner/repo".to_owned(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
    };
    let Some(repo) = AppState::create_repository_from_fields(&fields) else {
        panic!("valid repo");
    };
    assert_eq!(repo.github_repo, "owner/repo");
}
#[test]
fn create_repository_rejects_github_repo_with_internal_whitespace_in_owner() {
    let fields = RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        github_repo: "own er/repo".to_owned(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
    };
    assert!(AppState::create_repository_from_fields(&fields).is_none());
}

#[test]
fn create_repository_rejects_github_repo_with_whitespace_around_slash() {
    for value in ["owner /repo", "owner/ repo", "owner / repo"] {
        let fields = RepositoryFormFields {
            name: "Repo".to_owned(),
            base_dir: String::new(),
            default_profile: String::new(),
            github_repo: value.to_owned(),
            remote_enabled: false,
            login_user: String::new(),
            host: String::new(),
            run_as_user: String::new(),
            setup_env_default: false,
        };
        assert!(
            AppState::create_repository_from_fields(&fields).is_none(),
            "expected {value:?} to be rejected"
        );
    }
}

#[test]
fn create_repository_accepts_github_repo_with_surrounding_whitespace_and_trims_it() {
    let fields = RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        github_repo: "  owner/repo  ".to_owned(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
    };
    let Some(repo) = AppState::create_repository_from_fields(&fields) else {
        panic!("valid repo with surrounding whitespace");
    };
    assert_eq!(repo.github_repo, "owner/repo");
}

#[test]
fn update_repository_rejects_invalid_github_repo_keeping_existing() {
    let mut repo = seed_repository();
    repo.github_repo = "owner/existing".to_owned();
    let fields = RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        github_repo: "no-slash".to_owned(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
    };
    assert!(!AppState::update_repository_from_fields(&mut repo, &fields));
    // Existing value preserved because update was rejected.
    assert_eq!(repo.github_repo, "owner/existing");
}

#[test]
fn update_repository_accepts_well_formed_github_repo_after_invalid_rejection() {
    let mut repo = seed_repository();
    repo.github_repo = "owner/existing".to_owned();
    let invalid = RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        github_repo: "no-slash".to_owned(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
    };
    assert!(!AppState::update_repository_from_fields(
        &mut repo, &invalid
    ));
    assert_eq!(repo.github_repo, "owner/existing");

    let valid = RepositoryFormFields {
        github_repo: "owner/new".to_owned(),
        ..invalid
    };
    assert!(AppState::update_repository_from_fields(&mut repo, &valid));
    assert_eq!(repo.github_repo, "owner/new");
}

#[test]
fn submit_edit_repository_keeps_modal_open_when_github_repo_invalid() {
    let mut state = AppState {
        repositories: vec![Repository {
            github_repo: "owner/existing".to_owned(),
            ..seed_repository()
        }],
        selected_repository_index: Some(0),
        ..AppState::default()
    };

    state = state.apply(AppEvent::OpenEditRepository(RepositoryId(
        "repo-1".to_owned(),
    )));
    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("expected edit-repository modal");
    };
    fields.github_repo = "owner/repo/extra".to_owned();

    state = state.apply(AppEvent::SubmitForm);

    assert_eq!(state.repositories[0].github_repo, "owner/existing");
    assert!(matches!(state.modal, ModalState::EditRepository { .. }));
}

#[test]
fn submit_edit_repository_closes_modal_when_github_repo_valid() {
    let mut state = AppState {
        repositories: vec![Repository {
            github_repo: "owner/existing".to_owned(),
            ..seed_repository()
        }],
        selected_repository_index: Some(0),
        ..AppState::default()
    };

    state = state.apply(AppEvent::OpenEditRepository(RepositoryId(
        "repo-1".to_owned(),
    )));
    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("expected edit-repository modal");
    };
    fields.github_repo = "owner/new".to_owned();

    state = state.apply(AppEvent::SubmitForm);

    assert_eq!(state.repositories[0].github_repo, "owner/new");
    assert!(matches!(state.modal, ModalState::None));
}
