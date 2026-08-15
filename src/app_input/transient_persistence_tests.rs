//! Tests for transient agent persistence filtering (issue #213).

use jefe::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
use jefe::state::AppState;
use std::path::PathBuf;

use super::durable_save_request;

fn id_of(key: &str) -> jefe::domain::Id {
    match jefe::domain::Id::parse(key) {
        Ok(id) => id,
        Err(error) => panic!("{key} must be a valid id: {error:?}"),
    }
}

/// Stage a durable save, failing loudly when the projection declined.
fn require_candidate(state: &mut AppState) -> Box<jefe::domain::StateV2> {
    match durable_save_request(state) {
        Some(request) => request.candidate,
        None => panic!(
            "durable projection should stage a candidate: {:?}",
            state.error_message
        ),
    }
}

fn make_repo() -> Repository {
    Repository::new(
        RepositoryId("repo-1".to_owned()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Test".to_owned(),
        "test".to_owned(),
        PathBuf::from("/tmp/repo"),
    )
}

#[test]
fn durable_candidate_filters_out_transient_agents() {
    let repo = make_repo();
    let mut state = crate::test_app_state();
    state.repositories.push(repo.clone());

    // Regular agent
    let mut regular = Agent::new(
        AgentId("regular-1".to_owned()),
        repo.id.clone(),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
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

    let persisted = require_candidate(&mut state);
    assert_eq!(
        persisted.agents.len(),
        1,
        "only non-transient agents should persist"
    );
    // A count alone would also pass if the transient agent were the survivor,
    // so prove which agent was kept via its persisted origin and work dir.
    let origin_key = id_of("origin");
    let survivor = &persisted.agents[0];
    assert_eq!(
        survivor.values.get(&origin_key),
        Some(&jefe::domain::TypedValue::String("persistent".to_owned())),
        "the surviving agent must be the persistent one"
    );
    let work_dir_key = id_of("work-dir");
    assert_eq!(
        survivor.values.get(&work_dir_key),
        Some(&jefe::domain::TypedValue::String("/tmp/regular".to_owned())),
        "the surviving agent must be the one rooted at the non-transient work dir"
    );
}

#[test]
fn durable_candidate_clears_selection_pointing_at_transient() {
    let repo = make_repo();
    let mut state = crate::test_app_state();
    state.repositories.push(repo.clone());

    // One regular agent at index 0, one transient at index 1.
    let mut regular = Agent::new(
        AgentId("regular-1".to_owned()),
        repo.id.clone(),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
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

    let persisted = require_candidate(&mut state);

    // After filtering, the transient agent is gone, so the selection that
    // pointed at it must be cleared (not silently remapped).
    assert_eq!(
        persisted.selection.agent_id, None,
        "a selection pointing at a transient agent must be cleared"
    );
    assert!(
        persisted.last_selected_agent_by_repo.is_empty(),
        "remembered selections must not reference a transient agent"
    );
}

#[test]
fn durable_candidate_keeps_all_non_transient_agents() {
    let repo = make_repo();
    let mut state = crate::test_app_state();
    state.repositories.push(repo.clone());

    for i in 0..3 {
        let mut agent = Agent::new(
            AgentId(format!("agent-{i}")),
            repo.id.clone(),
            jefe::domain::shipped_agent_type(3),
            jefe::domain::TypedMap::new(),
            format!("Agent {i}"),
            PathBuf::from("/tmp/agent"),
        );
        agent.status = AgentStatus::Running;
        state.agents.push(agent);
    }

    let persisted = require_candidate(&mut state);
    assert_eq!(persisted.agents.len(), 3);
}

#[test]
fn durable_candidate_remaps_selection_when_transient_precedes_persistent() {
    let repo = make_repo();
    let mut state = crate::test_app_state();
    state.repositories.push(repo.clone());

    // Transient agent at index 0, persistent at index 1.
    let transient = {
        let mut a = Agent::new(
            AgentId("transient-1".to_owned()),
            repo.id.clone(),
            jefe::domain::shipped_agent_type(3),
            jefe::domain::TypedMap::new(),
            "Transient".to_owned(),
            repo.effective_transient_dir().join("jefe-transient-1"),
        );
        a.origin = jefe::domain::AgentOrigin::Transient;
        a
    };
    let persistent = Agent::new(
        AgentId("persistent-1".to_owned()),
        repo.id.clone(),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Persistent".to_owned(),
        PathBuf::from("/tmp/persistent-1"),
    );
    state.agents.push(transient);
    state.agents.push(persistent);

    // Selection points at index 1 (the persistent agent).
    state.selected_agent_index = Some(1);

    let persisted = require_candidate(&mut state);

    // After filtering, the persistent agent is the only durable record and the
    // selection must still resolve to it.
    assert_eq!(persisted.agents.len(), 1);
    assert_eq!(
        persisted.selection.agent_id.as_ref(),
        Some(&persisted.agents[0].id),
        "the selection must follow the surviving persistent agent"
    );
}
