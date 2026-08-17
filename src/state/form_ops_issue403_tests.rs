//! Issue #403 tests: duplicate agent name prevention, work-dir collision
//! detection, and version whitespace validation in agent form submission.

use super::*;
use crate::domain::{Agent, AgentStatus, Repository, RepositoryId};
use crate::state::events::AppEvent;
use crate::state::transition::TransitionExt as _;
use crate::state::types::ModalState;

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

fn seed_second_repository() -> Repository {
    Repository::new(
        RepositoryId("repo-2".to_owned()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repo 2".to_owned(),
        "repo-2".to_owned(),
        std::path::PathBuf::from("/tmp/repo-2"),
    )
}

fn existing_agent(repo_id: &RepositoryId, name: &str, work_dir: &str) -> Agent {
    let id = format!("agent-{}", name.to_lowercase().replace(' ', "-"));
    let mut agent = Agent::new(
        crate::domain::AgentId(id),
        repo_id.clone(),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        name.to_owned(),
        std::path::PathBuf::from(work_dir),
    );
    agent.status = AgentStatus::Running;
    agent
}

fn open_new_agent_form(state: &mut AppState, repo_id: &RepositoryId) {
    *state = std::mem::replace(state, AppState::test_fixture())
        .apply(AppEvent::OpenNewAgent(repo_id.clone()))
        .committed_pure();
}

fn set_form_fields(modal: &mut ModalState, name: &str, work_dir: &str) {
    let ModalState::NewAgent { fields, .. } = modal else {
        panic!("expected NewAgent modal, got {modal:?}");
    };
    fields.name = name.to_owned();
    fields.work_dir = work_dir.to_owned();
}

#[test]
fn submit_new_agent_rejects_duplicate_name_same_repository() {
    let repo = seed_repository();
    let mut state = AppState::test_fixture();
    state.repositories = vec![repo];
    state.agents.push(existing_agent(
        &RepositoryId("repo-1".to_owned()),
        "main",
        "/tmp/repo-1/main",
    ));

    open_new_agent_form(&mut state, &RepositoryId("repo-1".to_owned()));
    set_form_fields(&mut state.modal, "main", "/tmp/repo-1/main-2");
    state = state.apply(AppEvent::SubmitForm).committed_pure();

    // Modal stays open with error; no new agent added.
    assert!(
        matches!(state.modal, ModalState::NewAgent { .. }),
        "modal should stay open on duplicate name"
    );
    assert_eq!(
        state.error_message.as_deref(),
        Some("An agent named 'main' already exists in this repository")
    );
    assert_eq!(
        state.agents.len(),
        1,
        "no new agent should be added on duplicate name"
    );
}

#[test]
fn submit_new_agent_rejects_duplicate_name_case_insensitive() {
    let repo = seed_repository();
    let mut state = AppState::test_fixture();
    state.repositories = vec![repo];
    state.agents.push(existing_agent(
        &RepositoryId("repo-1".to_owned()),
        "Main",
        "/tmp/repo-1/main",
    ));

    open_new_agent_form(&mut state, &RepositoryId("repo-1".to_owned()));
    set_form_fields(&mut state.modal, "MAIN", "/tmp/repo-1/main-2");
    state = state.apply(AppEvent::SubmitForm).committed_pure();

    assert!(
        matches!(state.modal, ModalState::NewAgent { .. }),
        "modal should stay open on case-insensitive duplicate"
    );
    assert!(
        state
            .error_message
            .as_deref()
            .is_some_and(|m| m == "An agent named 'MAIN' already exists in this repository")
    );
    assert_eq!(state.agents.len(), 1);
}

#[test]
fn submit_new_agent_allows_same_name_in_different_repository() {
    let repo1 = seed_repository();
    let repo2 = seed_second_repository();
    let mut state = AppState::test_fixture();
    state.repositories = vec![repo1, repo2];
    state.agents.push(existing_agent(
        &RepositoryId("repo-1".to_owned()),
        "main",
        "/tmp/repo-1/main",
    ));

    open_new_agent_form(&mut state, &RepositoryId("repo-2".to_owned()));
    set_form_fields(&mut state.modal, "main", "/tmp/repo-2/main");
    state = state.apply(AppEvent::SubmitForm).committed_pure();

    assert!(
        matches!(state.modal, ModalState::None),
        "modal should close when name is unique within the repository"
    );
    assert!(state.error_message.is_none());
    assert_eq!(state.agents.len(), 2);
}

#[test]
fn submit_new_agent_allows_colliding_work_dir_across_different_repositories() {
    // Work-dir uniqueness is scoped per-repository (issue #403 acceptance
    // matrix A3): two agents in different repositories may share the same
    // work_dir path. This test locks that contract so a future tightening
    // does not silently break cross-repo workflows.
    let repo1 = seed_repository();
    let repo2 = seed_second_repository();
    let mut state = AppState::test_fixture();
    state.repositories = vec![repo1, repo2];
    state.agents.push(existing_agent(
        &RepositoryId("repo-1".to_owned()),
        "alpha",
        "/tmp/shared-workdir",
    ));

    open_new_agent_form(&mut state, &RepositoryId("repo-2".to_owned()));
    // Same work_dir, different repository, different name.
    set_form_fields(&mut state.modal, "beta", "/tmp/shared-workdir");
    state = state.apply(AppEvent::SubmitForm).committed_pure();

    assert!(
        matches!(state.modal, ModalState::None),
        "cross-repo work-dir reuse should be allowed"
    );
    assert!(state.error_message.is_none());
    assert_eq!(state.agents.len(), 2);
}

#[test]
fn submit_new_agent_rejects_colliding_work_dir() {
    let repo = seed_repository();
    let mut state = AppState::test_fixture();
    state.repositories = vec![repo];
    state.agents.push(existing_agent(
        &RepositoryId("repo-1".to_owned()),
        "alpha",
        "/tmp/repo-1/shared",
    ));

    open_new_agent_form(&mut state, &RepositoryId("repo-1".to_owned()));
    // Different name but same work dir.
    set_form_fields(&mut state.modal, "beta", "/tmp/repo-1/shared");
    state = state.apply(AppEvent::SubmitForm).committed_pure();

    assert!(
        matches!(state.modal, ModalState::NewAgent { .. }),
        "modal should stay open on work-dir collision"
    );
    assert!(
        state
            .error_message
            .as_deref()
            .is_some_and(|m| m.contains("already used by agent"))
    );
    assert_eq!(state.agents.len(), 1);
}

#[test]
fn submit_new_agent_with_whitespace_version_sets_error() {
    let repo = seed_repository();
    let mut state = AppState::test_fixture();
    state.repositories = vec![repo];

    open_new_agent_form(&mut state, &RepositoryId("repo-1".to_owned()));
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("expected NewAgent modal");
    };
    fields.name = "Agent One".to_owned();
    fields.work_dir = "/tmp/repo-1/agent-one".to_owned();
    fields.agent_type_id = "core.llxprt".to_owned();
    fields.llxprt_version = "0.9.0\n0".to_owned();

    state = state.apply(AppEvent::SubmitForm).committed_pure();

    assert!(
        matches!(state.modal, ModalState::NewAgent { .. }),
        "modal should stay open on whitespace version"
    );
    assert!(
        state
            .error_message
            .as_deref()
            .is_some_and(|m| m.contains("whitespace"))
    );
    assert!(state.agents.is_empty());
}

#[test]
fn submit_new_agent_with_code_puppy_whitespace_version_sets_error() {
    let repo = seed_repository();
    let mut state = AppState::test_fixture();
    state.repositories = vec![repo];
    state.available_agent_type_ids = vec![
        crate::domain::shipped_agent_type(3),
        crate::domain::shipped_agent_type(1),
    ];

    open_new_agent_form(&mut state, &RepositoryId("repo-1".to_owned()));
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("expected NewAgent modal");
    };
    fields.name = "CP Agent".to_owned();
    fields.work_dir = "/tmp/repo-1/cp-agent".to_owned();
    fields.agent_type_id = "core.code-puppy".to_owned();
    fields.code_puppy_version = "0.0.361\n0".to_owned();

    state = state.apply(AppEvent::SubmitForm).committed_pure();

    assert!(
        matches!(state.modal, ModalState::NewAgent { .. }),
        "modal should stay open on Code Puppy whitespace version"
    );
    assert!(
        state
            .error_message
            .as_deref()
            .is_some_and(|m| m.contains("whitespace"))
    );
    assert!(state.agents.is_empty());
}

#[test]
fn submit_new_agent_clean_version_succeeds() {
    let repo = seed_repository();
    let mut state = AppState::test_fixture();
    state.repositories = vec![repo];

    open_new_agent_form(&mut state, &RepositoryId("repo-1".to_owned()));
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("expected NewAgent modal");
    };
    fields.name = "Agent One".to_owned();
    fields.work_dir = "/tmp/repo-1/agent-one".to_owned();
    fields.agent_type_id = "core.llxprt".to_owned();
    fields.llxprt_version = "0.9.0".to_owned();

    state = state.apply(AppEvent::SubmitForm).committed_pure();

    assert!(
        matches!(state.modal, ModalState::None),
        "modal should close on valid submit"
    );
    assert!(state.error_message.is_none());
    assert_eq!(state.agents.len(), 1);
}

#[test]
fn submit_new_agent_clears_stale_error_on_success() {
    let repo = seed_repository();
    let mut state = AppState::test_fixture();
    state.repositories = vec![repo];
    state.agents.push(existing_agent(
        &RepositoryId("repo-1".to_owned()),
        "main",
        "/tmp/repo-1/main",
    ));

    // First submit: duplicate name → error set.
    open_new_agent_form(&mut state, &RepositoryId("repo-1".to_owned()));
    set_form_fields(&mut state.modal, "main", "/tmp/repo-1/main-2");
    state = state.apply(AppEvent::SubmitForm).committed_pure();
    assert!(state.error_message.is_some());

    // Fix the name and resubmit: error should be cleared.
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("expected modal still open");
    };
    fields.name = "main-2".to_owned();
    state = state.apply(AppEvent::SubmitForm).committed_pure();

    assert!(matches!(state.modal, ModalState::None));
    assert!(state.error_message.is_none());
    assert_eq!(state.agents.len(), 2);
}
