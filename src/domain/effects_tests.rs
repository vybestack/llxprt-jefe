//! Contract tests for the closed post-commit effect boundary (issue #381
//! CW01-10/CW01-11/CW01-12).

use std::collections::BTreeMap;

use super::effects::{
    Completion, Correlation, CorrelationId, Effect, EffectError, EffectErrorKind, EffectFamily,
    MAX_TRANSITION_EFFECTS, PersistenceEffect, PersistenceResponse, RetryPolicy, SemanticKey,
    TimerEffect,
};
use super::{Id, Preferences, Selection, StateV2};

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

fn owner() -> Id {
    Id::parse("core.llxprt").value_or_panic("owner id")
}

fn semantic_key(subject: &str) -> SemanticKey {
    SemanticKey::new(EffectFamily::Persistence, subject)
}

fn correlation(id: u64) -> Correlation {
    Correlation {
        correlation_id: CorrelationId::new(id),
        owner: owner(),
        screen_generation: 3,
        activation_generation: 7,
        semantic_key: semantic_key("state"),
    }
}

fn empty_state_candidate() -> StateV2 {
    StateV2 {
        state_schema: 2,
        revision: 1,
        repositories: Vec::new(),
        agents: Vec::new(),
        selection: Selection {
            repository_id: None,
            agent_id: None,
            screen_id: None,
        },
        last_selected_agent_by_repo: BTreeMap::new(),
        preferences: Preferences {
            hide_idle_repositories: false,
            pane_focus: String::new(),
            terminal_focused: false,
            repository_preferences: BTreeMap::new(),
        },
        dormant_records: Vec::new(),
    }
}

#[test]
fn transition_effect_bound_is_exactly_sixty_four() {
    assert_eq!(MAX_TRANSITION_EFFECTS, 64);
}

#[test]
fn retry_policy_accepts_only_one_through_three_attempts() {
    for attempts in 1..=3u8 {
        let policy = RetryPolicy::idempotent_query(attempts)
            .value_or_panic("attempts within 1..=3 must construct");
        let RetryPolicy::IdempotentQuery { max_attempts } = policy else {
            panic!("constructor must produce IdempotentQuery");
        };
        assert_eq!(max_attempts.get(), attempts);
    }
    let _ = RetryPolicy::idempotent_query(0).error_or_panic("zero attempts must be rejected");
    let _ = RetryPolicy::idempotent_query(4).error_or_panic("four attempts must be rejected");
}

#[test]
fn correlation_matches_only_when_all_five_fields_match() {
    let pending = correlation(9);
    assert!(pending.matches(&correlation(9)), "identical must match");

    let mut wrong_id = correlation(9);
    wrong_id.correlation_id = CorrelationId::new(10);
    assert!(!pending.matches(&wrong_id), "correlation_id must match");

    let mut wrong_owner = correlation(9);
    wrong_owner.owner = Id::parse("core.code-puppy").value_or_panic("other owner");
    assert!(!pending.matches(&wrong_owner), "owner must match");

    let mut wrong_screen = correlation(9);
    wrong_screen.screen_generation = 4;
    assert!(
        !pending.matches(&wrong_screen),
        "screen generation must match"
    );

    let mut wrong_activation = correlation(9);
    wrong_activation.activation_generation = 8;
    assert!(
        !pending.matches(&wrong_activation),
        "activation generation must match"
    );

    let mut wrong_key = correlation(9);
    wrong_key.semantic_key = semantic_key("other");
    assert!(!pending.matches(&wrong_key), "semantic key must match");
}

#[test]
fn effect_error_reports_kind_and_retryability() {
    let error = EffectError::new(EffectErrorKind::Conflict, false, "revision superseded");
    assert_eq!(error.kind, EffectErrorKind::Conflict);
    assert!(!error.retryable);
    assert_eq!(error.redacted_detail, "revision superseded");
}

#[test]
fn completion_carries_correlation_and_typed_result() {
    let completion = Completion {
        correlation: correlation(1),
        result: Ok(PersistenceResponse::Persisted { revision: 5 }),
    };
    assert!(completion.result.is_ok());
    assert_eq!(
        completion.correlation.semantic_key,
        semantic_key("state"),
        "semantic key must round-trip"
    );
}

#[test]
fn every_effect_family_is_constructible_and_reports_its_family() {
    let persistence = Effect::Persistence(PersistenceEffect::PersistState {
        candidate: Box::new(empty_state_candidate()),
        revision: 2,
    });
    let timer = Effect::Timer(TimerEffect::Wakeup { after_ms: 150 });
    assert_eq!(persistence.family(), EffectFamily::Persistence);
    assert_eq!(timer.family(), EffectFamily::Timer);
}
