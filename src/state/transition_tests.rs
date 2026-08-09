//! Transition-boundary tests: effect bound, pending correlation records, and
//! stale-completion no-op (issue #381 CW01-10/CW01-11).

use crate::domain::Id;
use crate::domain::effects::{
    Correlation, CorrelationId, Effect, EffectFamily, IssuedEffect, RetryPolicy, SemanticKey,
    TimerEffect,
};
use crate::messages::{AppMessage, UiNavigationMessage};
use crate::state::AppState;

use super::transition::{CompletionOutcome, MAX_TRANSITION_EFFECTS, Transition, TransitionError};

trait TestResultExt<T, E> {
    fn value_or_panic(self, context: &str) -> T;
    fn error_or_panic(self, context: &str) -> E;
}

impl<T, E: std::fmt::Debug> TestResultExt<T, E> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }

    fn error_or_panic(self, context: &str) -> E {
        match self {
            Ok(_) => panic!("{context}: expected error"),
            Err(error) => error,
        }
    }
}

fn wakeup(after_ms: u64) -> Effect {
    Effect::Timer(TimerEffect::Wakeup { after_ms })
}

fn owner() -> Id {
    match Id::parse("core.llxprt") {
        Ok(id) => id,
        Err(error) => panic!("owner id: {error}"),
    }
}

fn issued_wakeup(after_ms: u64) -> IssuedEffect {
    IssuedEffect {
        effect: wakeup(after_ms),
        correlation: Correlation {
            correlation_id: CorrelationId::new(after_ms),
            owner: owner(),
            screen_generation: 0,
            activation_generation: 0,
            semantic_key: SemanticKey::new(EffectFamily::Timer, &format!("wakeup-{after_ms}")),
        },
        retry: RetryPolicy::Never,
    }
}

#[test]
fn transition_accepts_exactly_sixty_four_effects() {
    let effects: Vec<IssuedEffect> = (0..MAX_TRANSITION_EFFECTS as u64)
        .map(issued_wakeup)
        .collect();
    let transition =
        Transition::new(AppState::default(), effects).value_or_panic("64 effects must be accepted");
    assert_eq!(transition.effects.len(), MAX_TRANSITION_EFFECTS);
}

#[test]
fn transition_rejects_sixty_five_effects_and_returns_the_state() {
    let state = AppState {
        hide_idle_repositories: true,
        ..Default::default()
    };
    let effects: Vec<IssuedEffect> = (0..=MAX_TRANSITION_EFFECTS as u64)
        .map(issued_wakeup)
        .collect();
    let error =
        Transition::new(state, effects).error_or_panic("65 effects must reject the transition");
    let TransitionError::EffectLimitExceeded { state, attempted } = error;
    assert_eq!(attempted, MAX_TRANSITION_EFFECTS + 1);
    assert!(
        state.hide_idle_repositories,
        "rejected transition must hand back the untouched state"
    );
}

#[test]
fn apply_message_returns_a_pure_bounded_transition() {
    let state = AppState::default();
    let transition = state
        .apply_message(AppMessage::UiNavigation(UiNavigationMessage::NavigateDown))
        .value_or_panic("navigation must commit");
    assert!(
        transition.effects.is_empty(),
        "pure navigation emits no effects"
    );
}

#[test]
fn pending_registration_assigns_unique_correlations_and_bounds_records() {
    let mut state = AppState::default();
    let key = SemanticKey::new(EffectFamily::Timer, "wakeup");
    let first = state
        .register_pending_effect(owner(), key.clone(), wakeup(1), RetryPolicy::Never)
        .value_or_panic("first registration");
    let second = state
        .register_pending_effect(owner(), key.clone(), wakeup(2), RetryPolicy::Never)
        .value_or_panic("second registration");
    assert_ne!(
        first.correlation_id, second.correlation_id,
        "correlation ids must be unique"
    );
    assert_eq!(
        state.pending_effects.len(),
        1,
        "same semantic key supersedes the older pending record"
    );

    for index in 0..MAX_TRANSITION_EFFECTS as u64 {
        let key = SemanticKey::new(EffectFamily::Timer, &format!("wakeup-{index}"));
        let _ = state.register_pending_effect(owner(), key, wakeup(index), RetryPolicy::Never);
    }
    assert!(
        state.pending_effects.len() <= MAX_TRANSITION_EFFECTS,
        "pending records must stay bounded"
    );
}

#[test]
fn exact_completion_applies_once_and_clears_the_pending_record() {
    let mut state = AppState::default();
    let key = SemanticKey::new(EffectFamily::Timer, "wakeup");
    let correlation = state
        .register_pending_effect(owner(), key, wakeup(5), RetryPolicy::Never)
        .value_or_panic("registration");

    assert!(state.pending_effects.is_pending(&correlation));
    let outcome = state.apply_effect_completion(&correlation);
    assert_eq!(outcome, CompletionOutcome::Applied);
    assert!(state.pending_effects.is_empty());
    assert!(!state.pending_effects.is_pending(&correlation));

    let duplicate = state.apply_effect_completion(&correlation);
    assert_eq!(
        duplicate,
        CompletionOutcome::StaleIgnored,
        "a duplicate completion must not apply twice"
    );
}

fn agent(id: &str, status: crate::domain::AgentStatus) -> crate::domain::Agent {
    let mut agent = crate::domain::Agent::new(
        crate::domain::AgentId(id.to_owned()),
        crate::domain::RepositoryId("repo-1".to_owned()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        id.to_owned(),
        std::path::PathBuf::from("/tmp/agent"),
    );
    agent.status = status;
    agent
}

#[test]
fn kill_agent_commits_dead_state_and_stages_a_kill_session_effect() {
    let mut state = AppState::default();
    state
        .agents
        .push(agent("agent-7", crate::domain::AgentStatus::Running));

    let transition = state
        .apply_message(AppMessage::Runtime(
            crate::messages::RuntimeMessage::KillAgent(crate::domain::AgentId(
                "agent-7".to_owned(),
            )),
        ))
        .value_or_panic("kill must commit");

    assert_eq!(
        transition.next_state.agents[0].status,
        crate::domain::AgentStatus::Dead,
        "the committed next state must already show the agent dead"
    );
    assert_eq!(
        transition.effects.len(),
        1,
        "kill stages exactly one effect"
    );
    let issued = &transition.effects[0];
    assert_eq!(
        issued.effect,
        Effect::Runtime(crate::domain::effects::RuntimeEffect::KillSession {
            agent_id: crate::domain::AgentId("agent-7".to_owned()),
        })
    );
    assert_eq!(
        issued.correlation.semantic_key.family(),
        EffectFamily::Runtime
    );
    assert_eq!(issued.correlation.semantic_key.subject(), "agent-7");
    assert_eq!(issued.retry, RetryPolicy::Never);
    assert_eq!(
        transition.next_state.pending_effects.len(),
        1,
        "the staged effect must stay pending until its completion arrives"
    );
}

#[test]
fn exact_runtime_failure_completion_surfaces_a_typed_error_once() {
    let mut state = AppState::default();
    state
        .agents
        .push(agent("agent-7", crate::domain::AgentStatus::Running));
    let transition = state
        .apply_message(AppMessage::Runtime(
            crate::messages::RuntimeMessage::KillAgent(crate::domain::AgentId(
                "agent-7".to_owned(),
            )),
        ))
        .value_or_panic("kill must commit");
    let issued = transition.effects[0].clone();
    let state = transition.next_state;

    let completion = crate::domain::effects::EffectCompletion {
        correlation: issued.correlation.clone(),
        result: Err(crate::domain::effects::EffectError::new(
            crate::domain::effects::EffectErrorKind::Unavailable,
            false,
            "runtime session was not found",
        )),
    };
    let transition = state
        .apply_message(AppMessage::EffectCompletion(completion.clone().into()))
        .value_or_panic("completion must commit");
    let state = transition.next_state;
    assert!(
        state
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("runtime session was not found")),
        "an exact failure completion must surface its redacted detail"
    );
    assert!(state.pending_effects.is_empty());

    // Debug-string equality is a valid no-op proof only while the `HashSet`
    // fields hold at most one element, since iteration order is randomized
    // above that. Assert the precondition rather than letting a future fixture
    // turn this into a flaky comparison.
    assert!(
        state.sticky_dead_agent_ids.len() <= 1 && state.sticky_empty_repository_ids.len() <= 1,
        "debug-equality comparison requires deterministic hash-set ordering"
    );
    let before = format!("{state:?}");
    let transition = state
        .apply_message(AppMessage::EffectCompletion(completion.into()))
        .value_or_panic("duplicate completion must still commit");
    assert_eq!(
        format!("{:?}", transition.next_state),
        before,
        "a duplicate completion must be a byte-equivalent no-op"
    );
    assert!(transition.effects.is_empty());
}

#[test]
fn stale_completion_message_is_a_full_reducer_no_op() {
    let mut state = AppState::default();
    state
        .agents
        .push(agent("agent-7", crate::domain::AgentStatus::Running));
    let transition = state
        .apply_message(AppMessage::Runtime(
            crate::messages::RuntimeMessage::KillAgent(crate::domain::AgentId(
                "agent-7".to_owned(),
            )),
        ))
        .value_or_panic("kill must commit");
    let issued = transition.effects[0].clone();
    let state = transition.next_state;
    let before = format!("{state:?}");

    let stale_completion = crate::domain::effects::EffectCompletion {
        correlation: Correlation {
            screen_generation: issued.correlation.screen_generation + 1,
            ..issued.correlation
        },
        result: Err(crate::domain::effects::EffectError::new(
            crate::domain::effects::EffectErrorKind::Io,
            false,
            "stale failure",
        )),
    };
    let transition = state
        .apply_message(AppMessage::EffectCompletion(stale_completion.into()))
        .value_or_panic("stale completion must commit as a no-op");
    assert_eq!(
        format!("{:?}", transition.next_state),
        before,
        "a stale completion must leave the state byte-equivalent"
    );
    assert!(transition.effects.is_empty());
}

#[test]
fn stale_completion_leaves_state_byte_equivalent() {
    let mut state = AppState::default();
    let key = SemanticKey::new(EffectFamily::Timer, "wakeup");
    let correlation = state
        .register_pending_effect(owner(), key.clone(), wakeup(9), RetryPolicy::Never)
        .value_or_panic("registration");
    let before = format!("{state:?}");

    let mismatches = [
        Correlation {
            correlation_id: CorrelationId::new(correlation.correlation_id.get() + 1),
            ..correlation.clone()
        },
        Correlation {
            screen_generation: correlation.screen_generation + 1,
            ..correlation.clone()
        },
        Correlation {
            activation_generation: correlation.activation_generation + 1,
            ..correlation.clone()
        },
        Correlation {
            semantic_key: SemanticKey::new(EffectFamily::Timer, "other"),
            ..correlation.clone()
        },
    ];
    for stale in mismatches {
        let outcome = state.apply_effect_completion(&stale);
        assert_eq!(outcome, CompletionOutcome::StaleIgnored);
        assert_eq!(
            format!("{state:?}"),
            before,
            "stale completion must not change state"
        );
    }
}
