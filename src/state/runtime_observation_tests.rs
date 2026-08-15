use crate::domain::AgentId;
use crate::jsp::v1::reducer::ReferenceReducer;
use crate::messages::{AppMessage, RuntimeMessage};

use super::AppState;
use super::transition::TransitionExt;

fn observation() -> crate::domain::observation::AgentObservation {
    let snapshot = crate::jsp::parse_snapshot(include_bytes!(
        "../../dev-docs/jsp/v1/fixtures/snapshot_full.json"
    ))
    .unwrap_or_else(|error| panic!("snapshot fixture: {error}"));
    let mut reducer = ReferenceReducer::new();
    reducer.apply_snapshot(&snapshot);
    reducer.observation()
}

#[test]
fn stale_generation_update_and_clear_cannot_replace_current_observation() {
    let agent_id = AgentId("agent-alex".to_owned());
    let mut current = observation();
    let mut replacement = current.clone();
    replacement
        .identity
        .as_mut()
        .unwrap_or_else(|| panic!("fixture identity"))
        .lifecycle_generation = 8;
    current.health = crate::domain::observation::ObservationHealth::Stale;

    let state = AppState::test_fixture();
    let state = state
        .apply_message(AppMessage::Runtime(RuntimeMessage::ObservationCleared(
            agent_id.clone(),
            8,
        )))
        .committed_pure();
    let state = state
        .apply_message(AppMessage::Runtime(RuntimeMessage::ObservationUpdated(
            agent_id.clone(),
            7,
            Box::new(current),
        )))
        .committed_pure();
    assert!(!state.observations.contains_key(&agent_id));

    let state = state
        .apply_message(AppMessage::Runtime(RuntimeMessage::ObservationUpdated(
            agent_id.clone(),
            8,
            Box::new(replacement.clone()),
        )))
        .committed_pure();
    let state = state
        .apply_message(AppMessage::Runtime(RuntimeMessage::ObservationCleared(
            agent_id.clone(),
            7,
        )))
        .committed_pure();
    assert_eq!(state.observations.get(&agent_id), Some(&replacement));
}

#[test]
fn observation_identity_must_match_ingress_scope() {
    let agent_id = AgentId("other-agent".to_owned());
    let state = AppState::test_fixture();
    let state = state
        .apply_message(AppMessage::Runtime(RuntimeMessage::ObservationUpdated(
            agent_id.clone(),
            7,
            Box::new(observation()),
        )))
        .committed_pure();
    assert!(!state.observations.contains_key(&agent_id));
}
