//! Executor unit matrix (issue #381 CW01-10): serial transition order,
//! adapter-only-after-release, terminal-failure continuation, bounded
//! follow-ups, retry policy, deferred execution, and all-variant dispatch.

use std::cell::RefCell;
use std::rc::Rc;

use crate::domain::Id;
use crate::domain::effects::{
    ClipboardUrlEffect, ClipboardUrlResponse, Correlation, CorrelationId, Effect, EffectCompletion,
    EffectError, EffectErrorKind, EffectFamily, EffectResponse, GitHubEffect, GitHubResponse,
    IssuedEffect, MAX_TRANSITION_EFFECTS, PersistenceEffect, PersistenceResponse, ProbeEffect,
    ProbeResponse, ProviderEffect, ProviderResponse, RetryPolicy, RuntimeEffect, RuntimeResponse,
    SemanticKey, SshTmuxEffect, SshTmuxResponse, TimerEffect, TimerResponse,
};
use crate::domain::{Preferences, Selection, StateV2};

use super::effect_executor::{AdapterExecution, EffectAdapter, run_effects};

fn owner() -> Id {
    match Id::parse("core.llxprt") {
        Ok(id) => id,
        Err(error) => panic!("owner id: {error}"),
    }
}

fn issued(effect: Effect, id: u64, retry: RetryPolicy) -> IssuedEffect {
    let family = effect.family();
    IssuedEffect {
        effect,
        correlation: Correlation {
            correlation_id: CorrelationId::new(id),
            owner: owner(),
            screen_generation: 0,
            activation_generation: 0,
            semantic_key: SemanticKey::new(family, &format!("subject-{id}")),
        },
        retry,
    }
}

fn wakeup(id: u64) -> IssuedEffect {
    issued(
        Effect::Timer(TimerEffect::Wakeup { after_ms: id }),
        id,
        RetryPolicy::Never,
    )
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
        last_selected_agent_by_repo: std::collections::BTreeMap::new(),
        preferences: Preferences {
            hide_idle_repositories: false,
            pane_focus: String::new(),
            terminal_focused: false,
            repository_preferences: std::collections::BTreeMap::new(),
        },
        dormant_records: Vec::new(),
    }
}

fn all_family_effects() -> Vec<IssuedEffect> {
    vec![
        issued(
            Effect::Persistence(PersistenceEffect::PersistState {
                candidate: Box::new(empty_state_candidate()),
                revision: 3,
            }),
            1,
            RetryPolicy::Never,
        ),
        issued(
            Effect::AgentProbe(ProbeEffect::CheckAgentLiveness {
                agent_id: crate::domain::AgentId("agent-1".to_owned()),
                session_id: "jefe-1".to_owned(),
            }),
            2,
            RetryPolicy::Never,
        ),
        issued(
            Effect::Runtime(RuntimeEffect::AttachSession {
                agent_id: crate::domain::AgentId("agent-1".to_owned()),
            }),
            3,
            RetryPolicy::Never,
        ),
        issued(
            Effect::GitHub(GitHubEffect::RefreshIssues {
                repository: "acme/widgets".to_owned(),
            }),
            4,
            RetryPolicy::Never,
        ),
        issued(
            Effect::SshTmux(SshTmuxEffect::ProbeRemoteSession {
                target: "host".to_owned(),
                session_id: "jefe-2".to_owned(),
            }),
            5,
            RetryPolicy::Never,
        ),
        issued(
            Effect::Provider(ProviderEffect::ProbePackageAvailability {
                selector: "latest".to_owned(),
            }),
            6,
            RetryPolicy::Never,
        ),
        issued(
            Effect::ClipboardUrl(ClipboardUrlEffect::CopyText {
                text: "copy".to_owned(),
            }),
            7,
            RetryPolicy::Never,
        ),
        issued(
            Effect::Timer(TimerEffect::Wakeup { after_ms: 8 }),
            8,
            RetryPolicy::Never,
        ),
    ]
}

fn success_response(effect: &Effect) -> EffectResponse {
    match effect {
        Effect::Persistence(_) => {
            EffectResponse::Persistence(PersistenceResponse::Persisted { revision: 3 })
        }
        Effect::AgentProbe(_) => {
            EffectResponse::AgentProbe(ProbeResponse::Liveness { alive: true })
        }
        Effect::Runtime(_) => EffectResponse::Runtime(RuntimeResponse::Attached),
        Effect::GitHub(_) => EffectResponse::GitHub(GitHubResponse::IssuesRefreshed { items: 1 }),
        Effect::SshTmux(_) => {
            EffectResponse::SshTmux(SshTmuxResponse::SessionPresence { present: true })
        }
        Effect::Provider(_) => {
            EffectResponse::Provider(ProviderResponse::PackageAvailability { available: true })
        }
        Effect::ClipboardUrl(_) => EffectResponse::ClipboardUrl(ClipboardUrlResponse::Copied),
        Effect::Timer(_) => EffectResponse::Timer(TimerResponse::Elapsed),
    }
}

/// Adapter that records execution order and asserts state access is released
/// (the deliver callback must never be active while the adapter executes).
struct RecordingAdapter {
    order: Vec<u64>,
    delivering: Rc<RefCell<bool>>,
    fail_ids: Vec<u64>,
    defer_ids: Vec<u64>,
    attempts: Vec<u64>,
    fail_first_attempts: usize,
    retryable: bool,
}

impl RecordingAdapter {
    fn new(delivering: Rc<RefCell<bool>>) -> Self {
        Self {
            order: Vec::new(),
            delivering,
            fail_ids: Vec::new(),
            defer_ids: Vec::new(),
            attempts: Vec::new(),
            fail_first_attempts: 0,
            retryable: false,
        }
    }
}

impl EffectAdapter for RecordingAdapter {
    fn execute(&mut self, issued: &IssuedEffect) -> AdapterExecution {
        assert!(
            !*self.delivering.borrow(),
            "adapter must only run after state access is released"
        );
        let id = issued.correlation.correlation_id.get();
        self.order.push(id);
        self.attempts.push(id);
        if self.defer_ids.contains(&id) {
            return AdapterExecution::Deferred;
        }
        let attempts_so_far = self.attempts.iter().filter(|seen| **seen == id).count();
        if self.fail_ids.contains(&id) || attempts_so_far <= self.fail_first_attempts {
            return AdapterExecution::Completed(Err(EffectError::new(
                EffectErrorKind::Io,
                self.retryable,
                "adapter failed",
            )));
        }
        AdapterExecution::Completed(Ok(success_response(&issued.effect)))
    }
}

#[test]
fn all_eight_families_execute_serially_in_transition_order() {
    let delivering = Rc::new(RefCell::new(false));
    let mut adapter = RecordingAdapter::new(Rc::clone(&delivering));
    let mut delivered: Vec<EffectCompletion> = Vec::new();

    let report = run_effects(all_family_effects(), &mut adapter, |completion| {
        *delivering.borrow_mut() = true;
        delivered.push(completion);
        *delivering.borrow_mut() = false;
        Vec::new()
    });

    assert_eq!(adapter.order, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(report.delivered, 8);
    assert_eq!(report.deferred, 0);
    assert_eq!(report.rejected_follow_ups, 0);
    let families: Vec<EffectFamily> = delivered.iter().map(EffectCompletion::family).collect();
    assert_eq!(
        families,
        vec![
            EffectFamily::Persistence,
            EffectFamily::AgentProbe,
            EffectFamily::Runtime,
            EffectFamily::GitHub,
            EffectFamily::SshTmux,
            EffectFamily::Provider,
            EffectFamily::ClipboardUrl,
            EffectFamily::Timer,
        ]
    );
    assert!(delivered.iter().all(|message| message.error().is_none()));
}

#[test]
fn terminal_failure_delivers_completion_and_continues_to_next_original_effect() {
    let delivering = Rc::new(RefCell::new(false));
    let mut adapter = RecordingAdapter::new(Rc::clone(&delivering));
    adapter.fail_ids = vec![2];
    let mut delivered: Vec<(u64, bool)> = Vec::new();

    let report = run_effects(
        vec![wakeup(1), wakeup(2), wakeup(3)],
        &mut adapter,
        |completion| {
            delivered.push((
                completion.correlation().correlation_id.get(),
                completion.error().is_some(),
            ));
            Vec::new()
        },
    );

    assert_eq!(adapter.order, vec![1, 2, 3]);
    assert_eq!(
        delivered,
        vec![(1, false), (2, true), (3, false)],
        "failure must deliver its completion and execution must continue"
    );
    assert_eq!(report.delivered, 3);
}

#[test]
fn follow_ups_append_after_the_original_batch() {
    let delivering = Rc::new(RefCell::new(false));
    let mut adapter = RecordingAdapter::new(Rc::clone(&delivering));
    let mut first = true;

    let _report = run_effects(vec![wakeup(1), wakeup(2)], &mut adapter, |_completion| {
        if first {
            first = false;
            vec![wakeup(9)]
        } else {
            Vec::new()
        }
    });

    assert_eq!(
        adapter.order,
        vec![1, 2, 9],
        "the follow-up staged by the first completion must run after the whole original batch"
    );
}

#[test]
fn combined_follow_ups_are_bounded_at_sixty_four() {
    let delivering = Rc::new(RefCell::new(false));
    let mut adapter = RecordingAdapter::new(Rc::clone(&delivering));
    let mut deliveries = 0u64;

    let report = run_effects(vec![wakeup(1)], &mut adapter, |_completion| {
        deliveries += 1;
        if deliveries == 1 {
            (0..MAX_TRANSITION_EFFECTS as u64)
                .map(|index| wakeup(100 + index))
                .collect()
        } else if deliveries == 2 {
            vec![wakeup(999)]
        } else {
            Vec::new()
        }
    });

    assert_eq!(
        report.rejected_follow_ups, 1,
        "the follow-up past the 64 bound must be rejected observably"
    );
    assert!(
        !adapter.order.contains(&999),
        "a rejected follow-up must never execute"
    );
    assert_eq!(
        adapter.order.len(),
        1 + MAX_TRANSITION_EFFECTS,
        "exactly the original effect plus 64 accepted follow-ups execute"
    );
}

#[test]
fn idempotent_query_retries_retryable_failures_up_to_max_attempts() {
    let delivering = Rc::new(RefCell::new(false));
    let mut adapter = RecordingAdapter::new(Rc::clone(&delivering));
    adapter.fail_first_attempts = 2;
    adapter.retryable = true;
    let retry = match RetryPolicy::idempotent_query(3) {
        Ok(policy) => policy,
        Err(error) => panic!("retry policy: {error}"),
    };
    let mut delivered: Vec<bool> = Vec::new();

    let report = run_effects(
        vec![issued(
            Effect::GitHub(GitHubEffect::RefreshIssues {
                repository: "acme/widgets".to_owned(),
            }),
            7,
            retry,
        )],
        &mut adapter,
        |completion| {
            delivered.push(completion.error().is_none());
            Vec::new()
        },
    );

    assert_eq!(
        adapter.order,
        vec![7, 7, 7],
        "two retryable failures then success is three total attempts"
    );
    assert_eq!(
        delivered,
        vec![true],
        "only the final completion is delivered"
    );
    assert_eq!(report.delivered, 1);
}

#[test]
fn never_policy_and_non_retryable_errors_execute_exactly_once() {
    let delivering = Rc::new(RefCell::new(false));
    let mut adapter = RecordingAdapter::new(Rc::clone(&delivering));
    adapter.fail_first_attempts = 3;
    adapter.retryable = true;
    let mut errors = 0usize;
    let _ = run_effects(vec![wakeup(1)], &mut adapter, |completion| {
        if completion.error().is_some() {
            errors += 1;
        }
        Vec::new()
    });
    assert_eq!(adapter.order, vec![1], "Never policy must not retry");
    assert_eq!(errors, 1);

    let mut adapter = RecordingAdapter::new(Rc::clone(&delivering));
    adapter.fail_first_attempts = 3;
    adapter.retryable = false;
    let retry = match RetryPolicy::idempotent_query(3) {
        Ok(policy) => policy,
        Err(error) => panic!("retry policy: {error}"),
    };
    let _ = run_effects(
        vec![issued(
            Effect::GitHub(GitHubEffect::RefreshIssues {
                repository: "acme/widgets".to_owned(),
            }),
            2,
            retry,
        )],
        &mut adapter,
        |_completion| Vec::new(),
    );
    assert_eq!(
        adapter.order,
        vec![2],
        "a non-retryable error must not be retried even under an idempotent policy"
    );
}

#[test]
fn deferred_execution_consumes_the_effect_without_a_completion() {
    let delivering = Rc::new(RefCell::new(false));
    let mut adapter = RecordingAdapter::new(Rc::clone(&delivering));
    adapter.defer_ids = vec![2];
    let mut delivered: Vec<u64> = Vec::new();

    let report = run_effects(
        vec![wakeup(1), wakeup(2), wakeup(3)],
        &mut adapter,
        |completion| {
            delivered.push(completion.correlation().correlation_id.get());
            Vec::new()
        },
    );

    assert_eq!(adapter.order, vec![1, 2, 3]);
    assert_eq!(
        delivered,
        vec![1, 3],
        "deferred effects deliver their completion later via the bus"
    );
    assert_eq!(report.deferred, 1);
    assert_eq!(report.delivered, 2);
}

#[test]
fn mismatched_adapter_response_family_becomes_a_typed_error_completion() {
    struct MismatchAdapter;
    impl EffectAdapter for MismatchAdapter {
        fn execute(&mut self, _issued: &IssuedEffect) -> AdapterExecution {
            AdapterExecution::Completed(Ok(EffectResponse::Timer(TimerResponse::Elapsed)))
        }
    }
    let mut adapter = MismatchAdapter;
    let mut errors: Vec<EffectErrorKind> = Vec::new();

    let _ = run_effects(
        vec![issued(
            Effect::GitHub(GitHubEffect::RefreshIssues {
                repository: "acme/widgets".to_owned(),
            }),
            1,
            RetryPolicy::Never,
        )],
        &mut adapter,
        |completion| {
            if let Some(error) = completion.error() {
                errors.push(error.kind);
            }
            Vec::new()
        },
    );

    assert_eq!(
        errors,
        vec![EffectErrorKind::Validation],
        "family mismatch must surface as a typed validation error"
    );
}
