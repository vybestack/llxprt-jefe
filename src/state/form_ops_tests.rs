use super::*;
use crate::domain::{Agent, RemoteRepositorySettings, Repository, RepositoryId};
use crate::state::events::AppEvent;
use crate::state::transition::TransitionExt;
use crate::state::types::{ModalState, ScreenId};

fn seed_repository() -> Repository {
    Repository::new(
        RepositoryId("repo-1".to_owned()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repo 1".to_owned(),
        "repo-1".to_owned(),
        std::path::PathBuf::from("/tmp/repo-1"),
    )
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
    assert_eq!(state.screen(), ScreenId::Dashboard);
}

#[test]
fn default_state_terminal_unfocused() {
    let state = AppState::default();
    assert!(!state.terminal_focused);
}

#[test]
fn open_new_agent_copies_repository_code_puppy_model() {
    let mut repo = seed_repository();
    repo.default_type_id = crate::domain::shipped_agent_type(1);
    crate::domain::canonical_values::insert_json(
        &mut repo.default_values,
        "model",
        serde_json::Value::String("repo/default-model".to_owned()),
    )
    .unwrap_or_else(|error| panic!("valid model fixture: {error}"));
    let mut state = AppState {
        repositories: vec![repo],
        ..AppState::default()
    };

    state = state
        .apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())))
        .committed_pure();

    let ModalState::NewAgent { fields, cursor, .. } = state.modal else {
        panic!("expected new-agent modal, got {:?}", state.modal);
    };
    assert_eq!(fields.code_puppy_model, "repo/default-model");
    assert_eq!(
        cursor.code_puppy_model,
        "repo/default-model".chars().count()
    );
}

#[test]
fn open_new_agent_keeps_repository_version_on_its_runtime() {
    let mut repo = seed_repository();
    repo.default_type_id = crate::domain::shipped_agent_type(1);
    crate::domain::canonical_values::insert_json(
        &mut repo.default_values,
        "version_selector",
        serde_json::Value::String("0.9.0".to_owned()),
    )
    .unwrap_or_else(|error| panic!("valid version fixture: {error}"));
    let mut state = AppState {
        repositories: vec![repo],
        available_agent_type_ids: vec![
            crate::domain::shipped_agent_type(3),
            crate::domain::shipped_agent_type(1),
        ],
        ..AppState::default()
    };

    state = state
        .apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())))
        .committed_pure();

    let ModalState::NewAgent { fields, .. } = state.modal else {
        panic!("expected new-agent modal, got {:?}", state.modal);
    };
    assert_eq!(fields.code_puppy_version, "0.9.0");
    assert_eq!(fields.llxprt_version, "");
}

#[test]
fn open_new_agent_defaults_to_repo_kind_when_installed() {
    let mut repo = seed_repository();
    repo.default_type_id = crate::domain::shipped_agent_type(1);
    let mut state = AppState {
        repositories: vec![repo],
        available_agent_type_ids: vec![
            crate::domain::shipped_agent_type(3),
            crate::domain::shipped_agent_type(1),
        ],
        ..AppState::default()
    };

    state = state
        .apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())))
        .committed_pure();

    let ModalState::NewAgent { fields, .. } = state.modal else {
        panic!("expected new-agent modal, got {:?}", state.modal);
    };
    // Repository default is CodePuppy and it is installed → modal starts CP.
    assert_eq!(fields.agent_type_id, "core.code-puppy");
    // CodePuppy agents do not get the LLxprt --yolo default mode.
    assert_eq!(fields.mode, "");
}

#[test]
fn open_new_agent_falls_back_to_first_installed_when_repo_default_not_installed() {
    let mut repo = seed_repository();
    repo.default_type_id = crate::domain::shipped_agent_type(1);
    let mut state = AppState {
        repositories: vec![repo],
        // Only LLxprt is installed locally.
        available_agent_type_ids: vec![crate::domain::shipped_agent_type(3)],
        ..AppState::default()
    };

    state = state
        .apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())))
        .committed_pure();

    let ModalState::NewAgent { fields, .. } = state.modal else {
        panic!("expected new-agent modal, got {:?}", state.modal);
    };
    // Repo default is CodePuppy but it is not installed, so LLxprt is selected.
    assert_eq!(fields.agent_type_id, "core.llxprt");
    assert_eq!(fields.mode, "--yolo");
}

#[test]
fn open_new_agent_uses_repo_default_kind_for_remote_even_when_not_locally_installed() {
    let mut repo = seed_repository();
    repo.default_type_id = crate::domain::shipped_agent_type(1);
    repo.remote = RemoteRepositorySettings {
        enabled: true,
        login_user: "ubuntu".to_owned(),
        host: "build.example.com".to_owned(),
        ..Default::default()
    };
    let mut state = AppState {
        repositories: vec![repo],
        // Only LLxprt installed locally, but remote is authoritative.
        available_agent_type_ids: vec![crate::domain::shipped_agent_type(3)],
        ..AppState::default()
    };

    state = state
        .apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())))
        .committed_pure();

    let ModalState::NewAgent { fields, .. } = state.modal else {
        panic!("expected new-agent modal, got {:?}", state.modal);
    };
    // Remote repos offer repo default kind regardless of local install.
    assert_eq!(fields.agent_type_id, "core.code-puppy");
}

#[test]
fn open_new_repository_defaults_to_llxprt_when_installed() {
    let mut state = AppState {
        // Preference-ordered snapshot as published by the startup probe
        // (LLxprt, Code Puppy, Claude Code, Codex).
        available_agent_type_ids: vec![
            crate::domain::shipped_agent_type(3),
            crate::domain::shipped_agent_type(1),
            crate::domain::shipped_agent_type(0),
            crate::domain::shipped_agent_type(2),
        ],
        ..AppState::default()
    };

    state = state.apply(AppEvent::OpenNewRepository).committed_pure();

    let ModalState::NewRepository { fields, .. } = state.modal else {
        panic!("expected new-repository modal, got {:?}", state.modal);
    };
    assert_eq!(fields.default_type_id, "core.llxprt");
}

#[test]
fn open_new_repository_falls_back_to_llxprt_when_none_installed() {
    let mut state = AppState::default();

    state = state.apply(AppEvent::OpenNewRepository).committed_pure();

    let ModalState::NewRepository { fields, .. } = state.modal else {
        panic!("expected new-repository modal, got {:?}", state.modal);
    };
    assert_eq!(fields.default_type_id, "core.llxprt");
}

#[test]
fn new_agent_work_dir_slug_excludes_slashes_from_name() {
    let repository = seed_repository();
    let expected = repository.base_dir.join("api--worker");
    let mut state = AppState {
        repositories: vec![repository],
        ..AppState::default()
    };

    state = state
        .apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())))
        .committed_pure();
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("expected new-agent modal");
    };
    fields.name = "API / Worker".to_owned();

    state.update_agent_work_dir_from_name();

    let ModalState::NewAgent { fields, .. } = &state.modal else {
        panic!("expected new-agent modal, got {:?}", state.modal);
    };
    assert_eq!(std::path::Path::new(&fields.work_dir), expected);
}

#[cfg(windows)]
#[test]
fn automatic_agent_work_dir_joins_normalized_windows_repository_once() {
    let mut repository = seed_repository();
    repository.base_dir = std::path::PathBuf::from(r"C:\Users\Acoli Ω\somedir");
    let mut state = AppState {
        repositories: vec![repository],
        ..AppState::default()
    };

    state = state
        .apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())))
        .committed_pure();
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("expected new-agent modal");
    };
    fields.name = "branch-1".to_owned();

    state.update_agent_work_dir_from_name();

    let ModalState::NewAgent { fields, .. } = &state.modal else {
        panic!("expected new-agent modal, got {:?}", state.modal);
    };
    assert_eq!(
        std::path::Path::new(&fields.work_dir),
        std::path::Path::new(r"C:\Users\Acoli Ω\somedir\branch-1")
    );
    assert_eq!(fields.work_dir.matches("somedir").count(), 1);
}

#[test]
fn remote_repository_creation_preserves_remote_base_dir_without_local_expansion() {
    let fields = RepositoryFormFields {
        name: "Remote Repo".to_owned(),
        base_dir: "~/remote/worktrees".to_owned(),
        default_profile: "ship".to_owned(),
        default_code_puppy_model: String::new(),
        default_type_id: "core.llxprt".to_owned(),
        github_repo: String::new(),
        github_issue_pr_repo: String::new(),
        remote_enabled: true,
        login_user: "ubuntu".to_owned(),
        host: "170.9.234.179".to_owned(),
        run_as_user: "acoliver".to_owned(),
        setup_env_default: true,
        ..RepositoryFormFields::default()
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
        default_code_puppy_model: String::new(),
        default_type_id: "core.llxprt".to_owned(),
        github_repo: String::new(),
        github_issue_pr_repo: String::new(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
        ..RepositoryFormFields::default()
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
        code_puppy_model: String::new(),
        code_puppy_version: String::new(),
        code_puppy_yolo: false,
        code_puppy_quick_resume: crate::domain::QuickResume::default(),
        agent_type_id: "core.llxprt".to_owned(),
        mode: "--yolo".to_owned(),
        llxprt_debug: String::new(),
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: "podman".to_owned(),
        sandbox_flags: String::new(),
        llxprt_version: String::new(),
    };

    assert!(AppState::create_agent_from_fields(&repository, &fields, 1).is_none());
}

#[test]
fn agent_form_rejects_unknown_enabled_sandbox_engine() {
    let fields = AgentFormFields {
        sandbox_enabled: true,
        sandbox_engine: "unknown".to_owned(),
        ..AgentFormFields::default()
    };

    let error = AppState::validate_agent_form_fields(&fields)
        .err()
        .unwrap_or_else(|| panic!("unknown enabled engine must be rejected"));
    assert_eq!(error, "Sandbox engine must be Podman, Docker, or Seatbelt");
}

#[test]
fn rejected_sandbox_engine_does_not_partially_update_agent() {
    let repository = seed_repository();
    let mut agent = Agent::new(
        crate::domain::AgentId("agent-invalid-sandbox".to_owned()),
        repository.id.clone(),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Original".to_owned(),
        std::path::PathBuf::from("/tmp/original"),
    );
    agent.description = "Original description".to_owned();
    agent.shortcut_slot = Some(1);
    let fields = AgentFormFields {
        name: "Changed".to_owned(),
        description: "Changed description".to_owned(),
        work_dir: "/tmp/changed".to_owned(),
        agent_type_id: "core.llxprt".to_owned(),
        sandbox_enabled: true,
        sandbox_engine: "unknown".to_owned(),
        shortcut_slot: Some(2),
        ..AgentFormFields::default()
    };

    assert!(!AppState::update_agent_from_fields(
        &mut agent,
        &repository,
        &fields
    ));
    assert_eq!(agent.name, "Original");
    assert_eq!(agent.description, "Original description");
    assert_eq!(agent.work_dir, std::path::PathBuf::from("/tmp/original"));
    assert_eq!(agent.shortcut_slot, Some(1));
}

#[test]
fn update_agent_ignores_whitespace_only_work_dir() {
    let repository = seed_repository();
    let mut agent = Agent::new(
        crate::domain::AgentId("agent-1".to_owned()),
        repository.id.clone(),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Agent One".to_owned(),
        std::path::PathBuf::from("/tmp/agent-one"),
    );
    let fields = AgentFormFields {
        name: "Agent One".to_owned(),
        work_dir: "   ".to_owned(),
        agent_type_id: "core.llxprt".to_owned(),
        mode: "--yolo".to_owned(),
        ..AgentFormFields::default()
    };

    AppState::update_agent_from_fields(&mut agent, &repository, &fields);
    assert_eq!(agent.work_dir, std::path::PathBuf::from("/tmp/agent-one"));
}

#[test]
fn update_agent_empty_mode_disables_declared_yolo_value() {
    let repository = seed_repository();
    let mut values = crate::domain::TypedMap::new();
    crate::domain::canonical_values::insert_json(
        &mut values,
        "yolo",
        serde_json::Value::Bool(true),
    )
    .unwrap_or_else(|error| panic!("valid yolo fixture: {error}"));
    let mut agent = Agent::new(
        crate::domain::AgentId("agent-yolo".to_owned()),
        repository.id.clone(),
        crate::domain::shipped_agent_type(3),
        values,
        "Agent Two".to_owned(),
        std::path::PathBuf::from("/tmp/agent-two"),
    );
    let fields = AgentFormFields {
        name: "Agent Two".to_owned(),
        work_dir: "/tmp/agent-two".to_owned(),
        agent_type_id: "core.llxprt".to_owned(),
        mode: "   ".to_owned(),
        ..AgentFormFields::default()
    };

    AppState::update_agent_from_fields(&mut agent, &repository, &fields);
    assert_eq!(
        crate::domain::canonical_values::typed_field(&agent.values, "yolo"),
        Some(&crate::domain::TypedValue::Bool(false))
    );
}

#[test]
fn update_llxprt_agent_replaces_obsolete_prompt_interactive_value() {
    let repository = seed_repository();
    let mut values = crate::domain::TypedMap::new();
    crate::domain::canonical_values::insert_json(
        &mut values,
        "prompt_interactive",
        serde_json::Value::Bool(true),
    )
    .unwrap_or_else(|error| panic!("valid obsolete prompt fixture: {error}"));
    crate::domain::canonical_values::insert_json(
        &mut values,
        "future_metadata",
        serde_json::Value::String("preserve me".to_owned()),
    )
    .unwrap_or_else(|error| panic!("valid undeclared metadata fixture: {error}"));
    let mut agent = Agent::new(
        crate::domain::AgentId("agent-obsolete-prompt".to_owned()),
        repository.id.clone(),
        crate::domain::shipped_agent_type(3),
        values,
        "Agent".to_owned(),
        std::path::PathBuf::from("/tmp/agent"),
    );
    let fields = AgentFormFields {
        name: "Agent".to_owned(),
        work_dir: "/tmp/agent".to_owned(),
        agent_type_id: "core.llxprt".to_owned(),
        pass_continue: false,
        ..AgentFormFields::default()
    };

    AppState::update_agent_from_fields(&mut agent, &repository, &fields);

    assert_eq!(
        crate::domain::canonical_values::typed_field(&agent.values, "continue"),
        Some(&crate::domain::TypedValue::Bool(false))
    );
    assert!(
        crate::domain::canonical_values::typed_field(&agent.values, "prompt_interactive").is_some(),
        "prompt_interactive is a declared agent field and should be present"
    );
    assert_eq!(
        crate::domain::canonical_values::typed_field(&agent.values, "future_metadata"),
        Some(&crate::domain::TypedValue::String("preserve me".to_owned()))
    );
}
#[test]
fn llxprt_sandbox_values_survive_create_and_edit_projection() {
    let repository = seed_repository();
    let fields = AgentFormFields {
        name: "Sandboxed LLxprt".to_owned(),
        work_dir: "/tmp/sandboxed-llxprt".to_owned(),
        agent_type_id: "core.llxprt".to_owned(),
        sandbox_enabled: true,
        sandbox_engine: "Docker".to_owned(),
        sandbox_flags: "--network none".to_owned(),
        ..AgentFormFields::default()
    };
    let Some(agent) = AppState::create_agent_from_fields(&repository, &fields, 1) else {
        panic!("valid LLxprt form should create an agent");
    };
    assert_eq!(
        crate::domain::canonical_values::typed_field(&agent.values, "sandbox_enabled"),
        Some(&crate::domain::TypedValue::Bool(true))
    );
    assert_eq!(
        crate::domain::canonical_values::typed_field(&agent.values, "sandbox_engine"),
        Some(&crate::domain::TypedValue::String("docker".to_owned()))
    );
    assert_eq!(
        crate::domain::canonical_values::typed_field(&agent.values, "sandbox_flags"),
        Some(&crate::domain::TypedValue::String(
            "--network none".to_owned()
        ))
    );

    let id = agent.id.clone();
    let mut state = AppState {
        repositories: vec![repository],
        agents: vec![agent],
        ..AppState::default()
    };
    state = state.apply(AppEvent::OpenEditAgent(id)).committed_pure();
    let ModalState::EditAgent { fields, .. } = state.modal else {
        panic!("expected edit-agent modal");
    };
    assert!(fields.sandbox_enabled);
    assert_eq!(fields.sandbox_engine, "Docker");
    assert_eq!(fields.sandbox_flags, "--network none");
}

#[test]
fn repository_checkbox_toggle_updates_remote_fields() {
    let mut state = AppState {
        repositories: vec![seed_repository()],
        ..AppState::default()
    };
    state = state.apply(AppEvent::OpenNewRepository).committed_pure();
    state = state.apply(AppEvent::FormNextField).committed_pure(); // Name → BaseDir
    state = state.apply(AppEvent::FormNextField).committed_pure(); // BaseDir → DefaultProfile
    state = state.apply(AppEvent::FormNextField).committed_pure(); // DefaultProfile → DefaultAgentTypeId (skips CodePuppyModel for Llxprt)
    state = state.apply(AppEvent::FormNextField).committed_pure(); // DefaultAgentTypeId → DefaultLlxprtMode
    state = state.apply(AppEvent::FormNextField).committed_pure(); // DefaultLlxprtMode → DefaultLlxprtVersion
    state = state.apply(AppEvent::FormNextField).committed_pure(); // DefaultLlxprtVersion → GitHubRepo
    state = state.apply(AppEvent::FormNextField).committed_pure(); // GitHubRepo → IssuePrRepo
    state = state.apply(AppEvent::FormNextField).committed_pure(); // IssuePrRepo → RemoteEnabled
    state = state.apply(AppEvent::FormToggleCheckbox).committed_pure(); // toggle remote_enabled
    state = state.apply(AppEvent::FormNextField).committed_pure(); // RemoteEnabled → LoginUser
    state = state.apply(AppEvent::FormChar('u')).committed_pure();
    state = state.apply(AppEvent::FormChar('b')).committed_pure();
    state = state.apply(AppEvent::FormNextField).committed_pure(); // LoginUser → Host
    state = state.apply(AppEvent::FormChar('1')).committed_pure();
    state = state.apply(AppEvent::FormChar('.')).committed_pure();
    state = state.apply(AppEvent::FormNextField).committed_pure(); // Host → SshPort
    state = state.apply(AppEvent::FormNextField).committed_pure(); // SshPort → IdentityFile
    state = state.apply(AppEvent::FormNextField).committed_pure(); // IdentityFile → SshOptions
    state = state.apply(AppEvent::FormNextField).committed_pure(); // SshOptions → RunAsUser
    state = state.apply(AppEvent::FormChar('a')).committed_pure();
    state = state.apply(AppEvent::FormNextField).committed_pure(); // RunAsUser → SetupEnvDefault
    state = state.apply(AppEvent::FormToggleCheckbox).committed_pure(); // toggle setup_env_default

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
        default_code_puppy_model: String::new(),
        default_type_id: "core.llxprt".to_owned(),
        github_repo: "foo".to_owned(),
        github_issue_pr_repo: String::new(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
        ..RepositoryFormFields::default()
    };
    assert!(AppState::create_repository_from_fields(&fields).is_none());
}

#[test]
fn create_repository_rejects_github_repo_with_extra_slash() {
    let fields = RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        default_code_puppy_model: String::new(),
        default_type_id: "core.llxprt".to_owned(),
        github_repo: "owner/repo/extra".to_owned(),
        github_issue_pr_repo: String::new(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
        ..RepositoryFormFields::default()
    };
    assert!(AppState::create_repository_from_fields(&fields).is_none());
}

#[test]
fn create_repository_rejects_github_repo_missing_owner() {
    let fields = RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        default_code_puppy_model: String::new(),
        default_type_id: "core.llxprt".to_owned(),
        github_repo: "/repo".to_owned(),
        github_issue_pr_repo: String::new(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
        ..RepositoryFormFields::default()
    };
    assert!(AppState::create_repository_from_fields(&fields).is_none());
}

#[test]
fn create_repository_rejects_github_repo_missing_repo_name() {
    let fields = RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        default_code_puppy_model: String::new(),
        default_type_id: "core.llxprt".to_owned(),
        github_repo: "owner/".to_owned(),
        github_issue_pr_repo: String::new(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
        ..RepositoryFormFields::default()
    };
    assert!(AppState::create_repository_from_fields(&fields).is_none());
}

#[test]
fn create_repository_accepts_empty_github_repo() {
    let fields = RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        default_code_puppy_model: String::new(),
        default_type_id: "core.llxprt".to_owned(),
        github_repo: String::new(),
        github_issue_pr_repo: String::new(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
        ..RepositoryFormFields::default()
    };
    assert!(AppState::create_repository_from_fields(&fields).is_some());
}

#[test]
fn create_repository_accepts_well_formed_github_repo() {
    let fields = RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        default_code_puppy_model: String::new(),
        default_type_id: "core.llxprt".to_owned(),
        github_repo: "owner/repo".to_owned(),
        github_issue_pr_repo: String::new(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
        ..RepositoryFormFields::default()
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
        default_code_puppy_model: String::new(),
        default_type_id: "core.llxprt".to_owned(),
        github_repo: "own er/repo".to_owned(),
        github_issue_pr_repo: String::new(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
        ..RepositoryFormFields::default()
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
            default_code_puppy_model: String::new(),
            default_type_id: "core.llxprt".to_owned(),
            github_repo: value.to_owned(),
            github_issue_pr_repo: String::new(),
            remote_enabled: false,
            login_user: String::new(),
            host: String::new(),
            run_as_user: String::new(),
            setup_env_default: false,
            ..RepositoryFormFields::default()
        };
        assert!(
            AppState::create_repository_from_fields(&fields).is_none(),
            "expected {value:?} to be rejected"
        );
    }
}

#[test]
fn create_repository_rejects_github_repo_with_at_sign() {
    // `@` is not valid in GitHub owner/repo names.
    let fields = RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        default_code_puppy_model: String::new(),
        default_type_id: "core.llxprt".to_owned(),
        github_repo: "acme@org/widgets".to_owned(),
        github_issue_pr_repo: String::new(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
        ..RepositoryFormFields::default()
    };
    assert!(AppState::create_repository_from_fields(&fields).is_none());
}

#[test]
fn create_repository_accepts_github_repo_with_surrounding_whitespace_and_trims_it() {
    let fields = RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        default_code_puppy_model: String::new(),
        default_type_id: "core.llxprt".to_owned(),
        github_repo: "  owner/repo  ".to_owned(),
        github_issue_pr_repo: String::new(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
        ..RepositoryFormFields::default()
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
        default_code_puppy_model: String::new(),
        default_type_id: "core.llxprt".to_owned(),
        github_repo: "no-slash".to_owned(),
        github_issue_pr_repo: String::new(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
        ..RepositoryFormFields::default()
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
        default_code_puppy_model: String::new(),
        default_type_id: "core.llxprt".to_owned(),
        github_repo: "no-slash".to_owned(),
        github_issue_pr_repo: String::new(),
        remote_enabled: false,
        login_user: String::new(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
        ..RepositoryFormFields::default()
    };
    assert!(!AppState::update_repository_from_fields(
        &mut repo, &invalid
    ));
    assert_eq!(repo.github_repo, "owner/existing");

    let valid = RepositoryFormFields {
        github_repo: "owner/new".to_owned(),
        github_issue_pr_repo: String::new(),
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
            github_issue_pr_repo: String::new(),
            ..seed_repository()
        }],
        selected_repository_index: Some(0),
        ..AppState::default()
    };

    state = state
        .apply(AppEvent::OpenEditRepository(RepositoryId(
            "repo-1".to_owned(),
        )))
        .committed_pure();
    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("expected edit-repository modal");
    };
    fields.github_repo = "owner/repo/extra".to_owned();

    state = state.apply(AppEvent::SubmitForm).committed_pure();

    assert_eq!(state.repositories[0].github_repo, "owner/existing");
    assert!(matches!(state.modal, ModalState::EditRepository { .. }));
}

#[test]
fn submit_edit_repository_closes_modal_when_github_repo_valid() {
    let mut state = AppState {
        repositories: vec![Repository {
            github_repo: "owner/existing".to_owned(),
            github_issue_pr_repo: String::new(),
            ..seed_repository()
        }],
        selected_repository_index: Some(0),
        ..AppState::default()
    };

    state = state
        .apply(AppEvent::OpenEditRepository(RepositoryId(
            "repo-1".to_owned(),
        )))
        .committed_pure();
    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("expected edit-repository modal");
    };
    fields.github_repo = "owner/new".to_owned();

    state = state.apply(AppEvent::SubmitForm).committed_pure();

    assert_eq!(state.repositories[0].github_repo, "owner/new");
    assert!(matches!(state.modal, ModalState::None));
}

#[test]
fn code_puppy_yolo_focus_toggles_typed_boolean() {
    let mut fields = AgentFormFields::default();
    assert!(!fields.code_puppy_yolo);

    AppState::toggle_agent_checkbox_fields(&mut fields, AgentFormFocus::CodePuppyYolo);
    assert!(fields.code_puppy_yolo);

    AppState::toggle_agent_checkbox_fields(&mut fields, AgentFormFocus::CodePuppyYolo);
    assert!(!fields.code_puppy_yolo);

    crate::state::form_runtime::cycle_agent_field(
        &[],
        &mut fields,
        AgentFormFocus::CodePuppyYolo,
        'x',
    );
    assert!(fields.code_puppy_yolo);
}

#[test]
fn remote_repository_form_preserves_validated_ssh_transport_fields() {
    let fields = RepositoryFormFields {
        name: "Remote SSH".to_owned(),
        default_type_id: "core.llxprt".to_owned(),
        remote_enabled: true,
        login_user: "ubuntu".to_owned(),
        host: "linux.example".to_owned(),
        ssh_port: "2222".to_owned(),
        identity_file: r"C:\Keys Ω\agent key".to_owned(),
        ssh_options: "Compression=yes LogLevel=ERROR".to_owned(),
        ..RepositoryFormFields::default()
    };
    let Some(repository) = AppState::create_repository_from_fields(&fields) else {
        panic!("valid SSH fields should create a repository");
    };
    assert_eq!(repository.remote.port, Some(2222));
    assert_eq!(
        repository.remote.identity_file,
        std::path::PathBuf::from(r"C:\Keys Ω\agent key")
    );
    assert_eq!(
        repository.remote.options,
        vec!["Compression=yes", "LogLevel=ERROR"]
    );
}

#[test]
fn remote_repository_form_rejects_invalid_port_and_unsafe_option() {
    let mut fields = RepositoryFormFields {
        name: "Remote SSH".to_owned(),
        remote_enabled: true,
        login_user: "ubuntu".to_owned(),
        host: "linux.example".to_owned(),
        ssh_port: "not-a-port".to_owned(),
        ..RepositoryFormFields::default()
    };
    assert!(AppState::create_repository_from_fields(&fields).is_none());

    fields.ssh_port = "22".to_owned();
    fields.ssh_options = "ProxyCommand=credential-helper".to_owned();
    assert!(AppState::create_repository_from_fields(&fields).is_none());
}

#[test]
fn local_repository_ignores_stale_invalid_ssh_port() {
    let fields = RepositoryFormFields {
        name: "Local Repository".to_owned(),
        default_type_id: "core.llxprt".to_owned(),
        remote_enabled: false,
        ssh_port: "not-a-port".to_owned(),
        ..RepositoryFormFields::default()
    };
    let Some(repository) = AppState::create_repository_from_fields(&fields) else {
        panic!("disabled remote settings must not block a local repository");
    };
    assert_eq!(repository.remote.port, None);
}
