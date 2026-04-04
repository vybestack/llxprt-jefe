use super::delete_selected_agent;
use jefe::domain::{Agent, AgentId, RemoteRepositorySettings, Repository, RepositoryId};
use jefe::state::AppState;
use std::path::PathBuf;

#[test]
fn delete_selected_agent_skips_local_directory_removal_for_remote_repository() {
    let repo_id = RepositoryId("repo-1".into());
    let agent_id = AgentId("agent-1".into());
    let missing_remote_path = PathBuf::from("/definitely/not/local/remote-agent-path");

    let mut repository = Repository::new(
        repo_id.clone(),
        "Remote Repo".into(),
        "remote-repo".into(),
        PathBuf::from("/srv/agents"),
    );
    repository.remote = RemoteRepositorySettings {
        enabled: true,
        login_user: "ubuntu".into(),
        host: "192.0.2.10".into(),
        run_as_user: "acoliver".into(),
        setup_env_default: true,
    };

    let mut agent = Agent::new(
        agent_id.clone(),
        repo_id.clone(),
        "Agent One".into(),
        missing_remote_path.clone(),
    );
    agent.status = jefe::domain::AgentStatus::Running;

    let mut state = AppState::default();
    state.repositories.push(repository);
    state.agents.push(agent);
    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(0);
    state.rebuild_repository_agent_ids();
    state.normalize_selection_indices();

    let removed = delete_selected_agent(&mut state, &agent_id, true);

    assert_eq!(removed, Some(agent_id));
    assert!(state.agents.is_empty());
    assert!(state.repositories[0].remote.enabled);
    assert_eq!(state.selected_agent_index, None);
}
