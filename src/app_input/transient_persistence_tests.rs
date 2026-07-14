//! Tests for transient agent persistence filtering (issue #213).

use jefe::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
use jefe::state::AppState;
use std::path::PathBuf;

use super::to_persisted_state;

fn make_repo() -> Repository {
    Repository::new(
        RepositoryId("repo-1".to_owned()),
        "Test".to_owned(),
        "test".to_owned(),
        PathBuf::from("/tmp/repo"),
    )
}

#[test]
fn to_persisted_state_filters_out_transient_agents() {
    let repo = make_repo();
    let mut state = AppState::default();
    state.repositories.push(repo.clone());

    // Regular agent
    let mut regular = Agent::new(
        AgentId("regular-1".to_owned()),
        repo.id.clone(),
        "Regular".to_owned(),
        PathBuf::from("/tmp/regular"),
    );
    regular.status = AgentStatus::Running;
    state.agents.push(regular);

    // Transient agent
    let transient = Agent::new_transient(
        AgentId("transient-1".to_owned()),
        repo.id.clone(),
        PathBuf::from("/tmp/transient"),
        &repo,
    );
    state.agents.push(transient);

    let persisted = to_persisted_state(&state);
    assert_eq!(
        persisted.agents.len(),
        1,
        "only non-transient agents should persist"
    );
    assert!(!persisted.agents[0].is_transient());
}

#[test]
fn to_persisted_state_keeps_all_non_transient_agents() {
    let repo = make_repo();
    let mut state = AppState::default();
    state.repositories.push(repo.clone());

    for i in 0..3 {
        let mut agent = Agent::new(
            AgentId(format!("agent-{i}")),
            repo.id.clone(),
            format!("Agent {i}"),
            PathBuf::from("/tmp/agent"),
        );
        agent.status = AgentStatus::Running;
        state.agents.push(agent);
    }

    let persisted = to_persisted_state(&state);
    assert_eq!(persisted.agents.len(), 3);
}
