//! Persisted-projection behavior: `to_persisted_state` field carriage,
//! pane-focus round-trips, and the CW01-11 stale-completion byte-equivalence
//! guarantee (issue #381).

use super::*;
use std::path::PathBuf;

#[test]
fn to_persisted_state_carries_hide_idle_toggle() {
    let state = AppState {
        hide_idle_repositories: true,
        ..AppState::default()
    };

    let persisted = to_persisted_state(&state);
    assert!(persisted.hide_idle_repositories);
}

#[test]
fn to_persisted_state_carries_pane_focus_and_terminal_focused() {
    let state = AppState {
        pane_focus: PaneFocus::Terminal,
        terminal_focused: true,
        ..AppState::default()
    };

    let persisted = to_persisted_state(&state);
    assert_eq!(persisted.pane_focus, "terminal");
    assert!(persisted.terminal_focused);
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
    let mut doomed = jefe::domain::Agent::new(
        jefe::domain::AgentId("agent-1".to_owned()),
        jefe::domain::RepositoryId("repo-1".to_owned()),
        "doomed".to_owned(),
        PathBuf::from("/tmp/agent"),
    );
    doomed.status = jefe::domain::AgentStatus::Running;
    state.agents.push(doomed);
    let transition = state
        .apply_message(AppMessage::Runtime(RuntimeMessage::KillAgent(
            jefe::domain::AgentId("agent-1".to_owned()),
        )))
        .unwrap_or_else(|error| panic!("kill must commit: {error}"));
    let issued = transition.effects[0].clone();
    let state = transition.next_state;

    let before = serde_json::to_string(&to_persisted_state(&state))
        .unwrap_or_else(|error| panic!("persisted state must serialize: {error}"));

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

    let after = serde_json::to_string(&to_persisted_state(&transition.next_state))
        .unwrap_or_else(|error| panic!("persisted state must serialize: {error}"));
    assert_eq!(
        before, after,
        "a stale completion must leave the serialized persisted state byte-identical"
    );
    assert!(transition.effects.is_empty());
}

#[test]
fn pane_focus_round_trip_all_variants() {
    for focus in [
        PaneFocus::Repositories,
        PaneFocus::Agents,
        PaneFocus::Terminal,
    ] {
        let s = pane_focus_to_persisted(focus);
        assert_eq!(
            pane_focus_from_persisted(&s),
            focus,
            "round-trip for {focus:?}"
        );
    }
}

#[test]
fn pane_focus_from_persisted_unknown_defaults_to_repositories() {
    // Older state files written before this field existed have "" or an
    // unrecognized value; both must fall back to Repositories.
    assert_eq!(pane_focus_from_persisted(""), PaneFocus::Repositories);
    assert_eq!(pane_focus_from_persisted("bogus"), PaneFocus::Repositories);
}
