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
    let work_dir = repo.effective_transient_dir().join("transient");
    let transient = Agent::new_transient(
        AgentId("transient-1".to_owned()),
        repo.id.clone(),
        work_dir,
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
fn to_persisted_state_clears_selected_agent_index_pointing_at_transient() {
    let repo = make_repo();
    let mut state = AppState::default();
    state.repositories.push(repo.clone());

    // One regular agent at index 0, one transient at index 1.
    let mut regular = Agent::new(
        AgentId("regular-1".to_owned()),
        repo.id.clone(),
        "Regular".to_owned(),
        PathBuf::from("/tmp/regular"),
    );
    regular.status = AgentStatus::Running;
    state.agents.push(regular);

    let work_dir = repo.effective_transient_dir().join("transient");
    let transient = Agent::new_transient(
        AgentId("transient-1".to_owned()),
        repo.id.clone(),
        work_dir,
        &repo,
    );
    state.agents.push(transient);

    // selected_agent_index = 1 points at the transient agent.
    state.selected_agent_index = Some(1);
    // Also set a last_selected entry for the transient agent.
    state.last_selected_agent_by_repo = vec![(repo.id.clone(), AgentId("transient-1".to_owned()))];

    let persisted = to_persisted_state(&state);

    // After filtering, only the regular agent (index 0) remains, so index 1
    // is out of bounds and must be cleared (not silently remapped).
    assert_eq!(
        persisted.selected_agent_index, None,
        "selected_agent_index pointing at a transient agent must be cleared"
    );
    // The transient agent's ID must not survive in last_selected_agent_by_repo.
    assert!(
        persisted
            .last_selected_agent_by_repo
            .iter()
            .all(|(_, id)| id != &AgentId("transient-1".to_owned())),
        "last_selected_agent_by_repo must not reference a transient agent ID"
    );
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

#[test]
fn to_persisted_state_remaps_selected_index_when_transient_precedes_persistent() {
    let repo = make_repo();
    let mut state = AppState::default();
    state.repositories.push(repo.clone());

    // Transient agent at index 0, persistent at index 1.
    let transient = {
        let mut a = Agent::new(
            AgentId("transient-1".to_owned()),
            repo.id.clone(),
            "Transient".to_owned(),
            repo.effective_transient_dir().join("jefe-transient-1"),
        );
        a.origin = jefe::domain::AgentOrigin::Transient;
        a
    };
    let persistent = Agent::new(
        AgentId("persistent-1".to_owned()),
        repo.id.clone(),
        "Persistent".to_owned(),
        PathBuf::from("/tmp/persistent-1"),
    );
    state.agents.push(transient);
    state.agents.push(persistent);

    // Selection points at index 1 (the persistent agent).
    state.selected_agent_index = Some(1);

    let persisted = to_persisted_state(&state);

    // After filtering, the persistent agent is at index 0.
    assert_eq!(persisted.agents.len(), 1);
    assert_eq!(
        persisted.selected_agent_index,
        Some(0),
        "selected_agent_index must be remapped to the persistent agent's new position"
    );
}
