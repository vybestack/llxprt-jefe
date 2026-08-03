//! Behavioral tests for the runtime-family effect adapter (issue #381 S9).
//!
//! The adapter is the composition-side boundary that executes committed
//! `RuntimeEffect` operations against a [`RuntimeManager`] and reports typed
//! completions. It must never be reachable while state is borrowed — it holds
//! only a runtime handle.

use crate::domain::effects::{
    Correlation, CorrelationId, Effect, EffectError, EffectErrorKind, EffectFamily, EffectResponse,
    IssuedEffect, RetryPolicy, RuntimeEffect, RuntimeResponse, SemanticKey, TimerEffect,
};
use crate::domain::{AgentId, Id};
use crate::runtime::{RuntimeManager, StubRuntimeManager};
use crate::services::effect_executor::{AdapterExecution, EffectAdapter};
use crate::services::runtime_effect_adapter::RuntimeEffectAdapter;

fn completed_or_panic(execution: AdapterExecution) -> Result<EffectResponse, EffectError> {
    match execution {
        AdapterExecution::Completed(result) => result,
        AdapterExecution::Deferred => panic!("runtime adapter must complete synchronously"),
    }
}

fn issued_runtime(effect: RuntimeEffect, subject: &str) -> IssuedEffect {
    issued(Effect::Runtime(effect), EffectFamily::Runtime, subject)
}

fn issued(effect: Effect, family: EffectFamily, subject: &str) -> IssuedEffect {
    let owner = match Id::parse("core.dashboard") {
        Ok(owner) => owner,
        Err(error) => panic!("owner id must parse: {error:?}"),
    };
    IssuedEffect {
        effect,
        correlation: Correlation {
            correlation_id: CorrelationId::new(1),
            owner,
            screen_generation: 0,
            activation_generation: 0,
            semantic_key: SemanticKey::new(family, subject),
        },
        retry: RetryPolicy::Never,
    }
}

fn manager_with_session(agent_id: &AgentId) -> StubRuntimeManager {
    let mut manager = StubRuntimeManager::default();
    let plan = crate::domain::agent_definition::AgentLaunchPlan {
        cwd: std::path::PathBuf::from("/tmp/agent"),
        target: crate::domain::agent_definition::Target::Remote(
            crate::domain::agent_definition::RemoteTarget {
                user: "fixture".to_owned(),
                host: "example.invalid".to_owned(),
                port: None,
                run_as_user: String::new(),
                canonical_cwd: std::path::PathBuf::from("/tmp/agent"),
            },
        ),
        ..crate::domain::agent_definition::AgentLaunchPlan::default()
    };
    let authorized = crate::runtime::test_support::authorized_launch_plan(&plan);
    if let Err(error) = manager.spawn_session(agent_id, &authorized, None) {
        panic!("spawn must succeed: {error}");
    }
    manager
}

/// KillSession against a live session removes it and reports `Killed`.
#[test]
fn kill_session_effect_kills_the_runtime_session_and_reports_killed() {
    let agent_id = AgentId("agent-kill".into());
    let mut manager = manager_with_session(&agent_id);
    let mut adapter = RuntimeEffectAdapter {
        runtime: &mut manager,
    };

    let execution = adapter.execute(&issued_runtime(
        RuntimeEffect::KillSession {
            agent_id: agent_id.clone(),
        },
        "agent-kill",
    ));

    let result = completed_or_panic(execution);
    match result {
        Ok(EffectResponse::Runtime(RuntimeResponse::Killed)) => {}
        other => panic!("expected Killed response, got {other:?}"),
    }
    assert!(
        !manager.has_session_record(&agent_id),
        "session must be removed by the kill effect"
    );
}

/// KillSession against a missing session reports a typed non-retryable
/// Unavailable error carrying the redacted runtime detail.
#[test]
fn kill_session_effect_on_missing_session_reports_typed_unavailable_error() {
    let mut manager = StubRuntimeManager::default();
    let mut adapter = RuntimeEffectAdapter {
        runtime: &mut manager,
    };

    let execution = adapter.execute(&issued_runtime(
        RuntimeEffect::KillSession {
            agent_id: AgentId("agent-missing".into()),
        },
        "agent-missing",
    ));

    let result = completed_or_panic(execution);
    let error = match result {
        Err(error) => error,
        Ok(response) => panic!("expected typed error, got {response:?}"),
    };
    assert_eq!(error.kind, EffectErrorKind::Unavailable);
    assert!(!error.retryable);
    assert!(
        error.redacted_detail.contains("session not found"),
        "detail must carry the runtime error: {}",
        error.redacted_detail
    );
}

/// Families this composition has not wired report a typed Unavailable error
/// instead of being silently dropped.
#[test]
fn unwired_effect_family_reports_typed_unavailable_error() {
    let mut manager = StubRuntimeManager::default();
    let mut adapter = RuntimeEffectAdapter {
        runtime: &mut manager,
    };

    let execution = adapter.execute(&issued(
        Effect::Timer(TimerEffect::Wakeup { after_ms: 5 }),
        EffectFamily::Timer,
        "wakeup",
    ));

    let result = completed_or_panic(execution);
    let error = match result {
        Err(error) => error,
        Ok(response) => panic!("expected typed error, got {response:?}"),
    };
    assert_eq!(error.kind, EffectErrorKind::Unavailable);
    assert!(!error.retryable);
    assert!(
        error.redacted_detail.contains("Timer"),
        "detail must name the unwired family: {}",
        error.redacted_detail
    );
}
