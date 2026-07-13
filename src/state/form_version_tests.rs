use super::{AppState, RepositoryFormFields};
use crate::domain::{AgentKind, RemoteRepositorySettings, Repository, RepositoryId};
use crate::state::events::AppEvent;
use crate::state::types::{ModalState, NpmAvailability};

fn seed_repository() -> Repository {
    Repository {
        id: RepositoryId("repo-1".to_owned()),
        name: "Repo 1".to_owned(),
        slug: "repo-1".to_owned(),
        base_dir: std::path::PathBuf::from("/tmp/repo-1"),
        default_profile: String::new(),
        default_code_puppy_model: String::new(),
        default_llxprt_version: String::new(),
        github_repo: String::new(),
        remote: RemoteRepositorySettings::default(),
        issue_base_prompt: String::new(),
        default_agent_kind: AgentKind::Llxprt,
        agent_ids: Vec::new(),
    }
}

fn valid_repo_fields_with_version(version: &str) -> RepositoryFormFields {
    RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: "/tmp/repo".to_owned(),
        default_llxprt_version: version.to_owned(),
        default_agent_kind: "LLxprt".to_owned(),
        ..RepositoryFormFields::default()
    }
}

#[test]
fn create_repository_rejects_nul_in_default_llxprt_version() {
    let fields = valid_repo_fields_with_version("0.9.0\x00; rm -rf /");
    assert!(
        AppState::create_repository_from_fields(&fields).is_none(),
        "repository creation must reject embedded NUL in default_llxprt_version"
    );
}

#[test]
fn update_repository_rejects_nul_in_default_llxprt_version() {
    let Some(mut repo) =
        AppState::create_repository_from_fields(&valid_repo_fields_with_version("0.9.0"))
    else {
        panic!("valid fields must create a repository");
    };
    let mut fields = valid_repo_fields_with_version("0.10.0\x00");
    fields.name.clone_from(&repo.name);
    assert!(!AppState::update_repository_from_fields(&mut repo, &fields));
    assert_eq!(repo.default_llxprt_version, "0.9.0");
}

#[test]
fn create_repository_trims_default_llxprt_version() {
    let Some(repo) =
        AppState::create_repository_from_fields(&valid_repo_fields_with_version("  0.9.0  "))
    else {
        panic!("valid fields with surrounding whitespace must succeed");
    };
    assert_eq!(repo.default_llxprt_version, "0.9.0");
}

#[test]
fn create_repository_accepts_nightly_default_llxprt_version() {
    let nightly = "0.10.0-nightly.260712.21cb698b6";
    let Some(repo) =
        AppState::create_repository_from_fields(&valid_repo_fields_with_version(nightly))
    else {
        panic!("nightly selectors must be accepted");
    };
    assert_eq!(repo.default_llxprt_version, nightly);
}

#[test]
fn open_new_agent_copies_repository_default_llxprt_version_into_form() {
    let mut repository = seed_repository();
    repository.default_llxprt_version = "0.9.0".to_owned();
    let state = AppState {
        repositories: vec![repository],
        installed_agent_kinds: vec![AgentKind::Llxprt],
        ..AppState::default()
    }
    .apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())));

    let ModalState::NewAgent { fields, cursor, .. } = state.modal else {
        panic!("expected new-agent modal");
    };
    assert_eq!(fields.llxprt_version, "0.9.0");
    assert_eq!(cursor.llxprt_version, "0.9.0".chars().count());
}

#[test]
fn open_new_agent_default_llxprt_version_blank_without_repository_default() {
    let state = AppState {
        repositories: vec![seed_repository()],
        installed_agent_kinds: vec![AgentKind::Llxprt],
        ..AppState::default()
    }
    .apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())));

    let ModalState::NewAgent { fields, .. } = state.modal else {
        panic!("expected new-agent modal");
    };
    assert!(fields.llxprt_version.is_empty());
}

#[test]
fn submitted_new_agent_persists_trimmed_llxprt_version() {
    let mut state = AppState {
        repositories: vec![seed_repository()],
        installed_agent_kinds: vec![AgentKind::Llxprt],
        ..AppState::default()
    }
    .apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())));
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("expected new-agent modal");
    };
    fields.name = "Versioned Agent".to_owned();
    fields.work_dir = "/tmp/agent-versioned".to_owned();
    fields.llxprt_version = "  0.10.0-nightly.260712.21cb698b6  ".to_owned();

    state = state.apply(AppEvent::SubmitForm);
    let Some(created) = state.agents.last() else {
        panic!("agent should be created");
    };
    assert_eq!(created.llxprt_version, "0.10.0-nightly.260712.21cb698b6");
}

#[test]
fn editing_repository_default_does_not_change_existing_agents() {
    let mut state = AppState {
        repositories: vec![seed_repository()],
        installed_agent_kinds: vec![AgentKind::Llxprt],
        ..AppState::default()
    }
    .apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())));
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("expected new-agent modal");
    };
    fields.name = "Existing Agent".to_owned();
    fields.work_dir = "/tmp/existing".to_owned();
    state = state.apply(AppEvent::SubmitForm);
    let Some(agent_id) = state.agents.last().map(|agent| agent.id.clone()) else {
        panic!("agent should exist");
    };

    state = state.apply(AppEvent::OpenEditRepository(RepositoryId(
        "repo-1".to_owned(),
    )));
    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("expected edit-repository modal");
    };
    fields.default_llxprt_version = "0.9.0".to_owned();
    state = state.apply(AppEvent::SubmitForm);

    let Some(existing) = state.agents.iter().find(|agent| agent.id == agent_id) else {
        panic!("existing agent must survive repository edit");
    };
    assert!(existing.llxprt_version.is_empty());
    assert_eq!(state.repositories[0].default_llxprt_version, "0.9.0");
}

#[test]
fn versioned_llxprt_default_uses_npm_without_direct_llxprt() {
    let mut repository = seed_repository();
    repository.default_llxprt_version = "0.9.0".to_owned();
    let state = AppState {
        repositories: vec![repository],
        installed_agent_kinds: vec![AgentKind::CodePuppy],
        npm_availability: NpmAvailability::Available,
        ..AppState::default()
    }
    .apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())));

    let ModalState::NewAgent { fields, .. } = state.modal else {
        panic!("expected new-agent modal");
    };
    assert_eq!(fields.agent_kind, "LLxprt");
    assert_eq!(fields.llxprt_version, "0.9.0");
}

#[test]
fn versioned_llxprt_default_falls_back_without_npm_or_direct_llxprt() {
    let mut repository = seed_repository();
    repository.default_llxprt_version = "0.9.0".to_owned();
    let state = AppState {
        repositories: vec![repository],
        installed_agent_kinds: vec![AgentKind::CodePuppy],
        npm_availability: NpmAvailability::Unavailable,
        ..AppState::default()
    }
    .apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())));

    let ModalState::NewAgent { fields, .. } = state.modal else {
        panic!("expected new-agent modal");
    };
    assert_eq!(fields.agent_kind, "code_puppy");
}
