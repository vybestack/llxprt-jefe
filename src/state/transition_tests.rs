//! Transition-boundary tests: effect bound, pending correlation records, and
//! stale-completion no-op (issue #381 CW01-10/CW01-11).

use crate::domain::Id;
use crate::domain::effects::{
    Correlation, CorrelationId, Effect, EffectFamily, RetryPolicy, SemanticKey, TimerEffect,
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

#[test]
fn transition_accepts_exactly_sixty_four_effects() {
    let effects: Vec<Effect> = (0..MAX_TRANSITION_EFFECTS as u64).map(wakeup).collect();
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
    let effects: Vec<Effect> = (0..=MAX_TRANSITION_EFFECTS as u64).map(wakeup).collect();
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

    let outcome = state.apply_effect_completion(&correlation);
    assert_eq!(outcome, CompletionOutcome::Applied);
    assert!(state.pending_effects.is_empty());

    let duplicate = state.apply_effect_completion(&correlation);
    assert_eq!(
        duplicate,
        CompletionOutcome::StaleIgnored,
        "a duplicate completion must not apply twice"
    );
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
