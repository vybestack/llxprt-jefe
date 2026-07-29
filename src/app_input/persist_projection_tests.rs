//! Durable-projection behavior: staged-candidate field carriage,
//! pane-focus round-trips, and the CW01-11 stale-completion byte-equivalence
//! guarantee (issue #381).

use super::*;
use std::path::PathBuf;

#[test]
fn staged_candidate_carries_hide_idle_toggle() {
    let mut state = AppState {
        hide_idle_repositories: true,
        ..AppState::default()
    };

    let persisted = require_request(&mut state);
    assert!(persisted.candidate.preferences.hide_idle_repositories);
}

#[test]
fn staged_candidate_carries_pane_focus_and_terminal_focused() {
    let mut state = AppState {
        pane_focus: PaneFocus::Terminal,
        terminal_focused: true,
        ..AppState::default()
    };

    let persisted = require_request(&mut state);
    assert_eq!(persisted.candidate.preferences.pane_focus, "terminal");
    assert!(persisted.candidate.preferences.terminal_focused);
}

/// Stage a durable save that the projection is expected to produce.
fn require_request(state: &mut AppState) -> jefe::services::persist_worker::PersistRequest {
    match durable_save_request(state) {
        Some(request) => request,
        None => panic!(
            "durable projection should stage a candidate: {:?}",
            state.error_message
        ),
    }
}

/// A stale effect completion must leave the persisted projection
/// byte-for-byte identical — no persisted mutation and no revision change
/// (issue #381 CW01-11).
#[test]
fn stale_completion_keeps_persisted_state_byte_identical() {
    use jefe::domain::effects::{
        Correlation, CorrelationId, EffectCompletion, EffectError, EffectErrorKind,
    };
    use jefe::messages::{AppMessage, RuntimeMessage};

    let mut state = AppState::default();
    state.repositories.push(jefe::domain::Repository::new(
        jefe::domain::RepositoryId("repo-1".to_owned()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Test".to_owned(),
        "test".to_owned(),
        PathBuf::from("/tmp/repo"),
    ));
    let mut doomed = jefe::domain::Agent::new(
        jefe::domain::AgentId("agent-1".to_owned()),
        jefe::domain::RepositoryId("repo-1".to_owned()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "doomed".to_owned(),
        PathBuf::from("/tmp/agent"),
    );
    doomed.status = jefe::domain::AgentStatus::Running;
    state.agents.push(doomed);
    state.rebuild_repository_agent_ids();
    let transition = state
        .apply_message(AppMessage::Runtime(RuntimeMessage::KillAgent(
            jefe::domain::AgentId("agent-1".to_owned()),
        )))
        .unwrap_or_else(|error| panic!("kill must commit: {error}"));
    let issued = transition.effects[0].clone();
    let state = transition.next_state;

    let mut before_state = state.clone();
    let before = durable_bytes(&mut before_state);

    let stale_completion = EffectCompletion {
        correlation: Correlation {
            correlation_id: CorrelationId::new(issued.correlation.correlation_id.get() + 1),
            ..issued.correlation
        },
        result: Err(EffectError::new(
            EffectErrorKind::Io,
            false,
            "stale failure",
        )),
    };
    let transition = state
        .apply_message(AppMessage::EffectCompletion(stale_completion.into()))
        .unwrap_or_else(|error| panic!("stale completion must commit: {error}"));

    let mut after_state = transition.next_state;
    let after = durable_bytes(&mut after_state);
    assert_eq!(
        before, after,
        "a stale completion must leave the serialized persisted state byte-identical"
    );
    assert!(transition.effects.is_empty());
}

/// Serialize the durable candidate this state would stage.
fn durable_bytes(state: &mut AppState) -> String {
    let request = durable_save_request(state).unwrap_or_else(|| {
        panic!(
            "durable projection should stage a candidate: {:?}",
            state.error_message
        )
    });
    serde_json::to_string(&request.candidate)
        .unwrap_or_else(|error| panic!("durable candidate must serialize: {error}"))
}

#[test]
fn pane_focus_round_trips_through_the_durable_candidate() {
    for focus in [
        PaneFocus::Repositories,
        PaneFocus::Agents,
        PaneFocus::Terminal,
    ] {
        let mut state = AppState {
            pane_focus: focus,
            ..AppState::default()
        };
        let request = durable_save_request(&mut state)
            .unwrap_or_else(|| panic!("durable projection should stage a candidate"));
        let restored = jefe::state::durable_restore::from_durable_state(request.candidate.as_ref())
            .unwrap_or_else(|error| panic!("candidate must restore: {error}"));
        assert_eq!(restored.pane_focus, focus, "round-trip for {focus:?}");
    }
}
