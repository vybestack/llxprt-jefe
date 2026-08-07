//! Provider effect worker handle (issue #390 CW-10, Slice D).
//!
//! Mirrors the `PersistHandle` pattern: the input path pushes pending provider
//! effects into a shared slot under a short lock; a background `use_future`
//! polls the slot and executes each effect through `smol::unblock`, routing
//! typed messages back through the reducer. No provider handle, pipe, or
//! thread ever enters `AppState`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::domain::effects::{Correlation, ProviderInvocation};

/// One unit of pending provider work.
#[derive(Debug, Clone)]
pub struct ProviderWorkItem {
    /// The invocation the supervisor will execute.
    pub invocation: ProviderInvocation,
    /// The effect correlation for completion delivery.
    pub correlation: Correlation,
}

/// Shared handle for the input path to schedule provider effects.
///
/// Cloning is cheap (shares the inner `Arc`). The background worker drains
/// the queue asynchronously.
#[derive(Clone)]
pub struct ProviderEffectHandle {
    inner: Arc<Inner>,
}

struct Inner {
    pending: Mutex<VecDeque<ProviderWorkItem>>,
    /// Bumps whenever new work is enqueued; the worker compares to detect it.
    schedule_generation: AtomicU64,
    /// Set to `true` when work is pending; cleared by the worker.
    dirty: AtomicBool,
}

impl std::fmt::Debug for ProviderEffectHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderEffectHandle")
            .finish_non_exhaustive()
    }
}

impl Default for ProviderEffectHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderEffectHandle {
    /// Create a new empty handle.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                pending: Mutex::new(VecDeque::new()),
                schedule_generation: AtomicU64::new(0),
                dirty: AtomicBool::new(false),
            }),
        }
    }

    /// Enqueue one provider effect for background execution.
    pub fn schedule(&self, item: ProviderWorkItem) {
        if let Ok(mut pending) = self.inner.pending.lock() {
            pending.push_back(item);
            self.inner.dirty.store(true, Ordering::SeqCst);
            self.inner
                .schedule_generation
                .fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Return already-drained work to the front of the queue.
    ///
    /// The worker drains a batch and then needs the context lock to resolve
    /// each descriptor. When that lock is momentarily held by the input path
    /// the work has not failed — it simply has not run yet — so it goes back
    /// rather than being reported as a closed provider stream. It is restored
    /// ahead of anything scheduled since, because it was dispatched first
    /// (issue #390).
    pub fn defer_all(&self, items: Vec<ProviderWorkItem>) {
        if items.is_empty() {
            return;
        }
        if let Ok(mut pending) = self.inner.pending.lock() {
            for item in items.into_iter().rev() {
                pending.push_front(item);
            }
            self.inner.dirty.store(true, Ordering::SeqCst);
            self.inner
                .schedule_generation
                .fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Drain all pending work items. Called by the background worker.
    #[must_use]
    pub fn drain(&self) -> Vec<ProviderWorkItem> {
        let items = self.inner.pending.lock().map_or_else(
            |_poisoned| Vec::new(),
            |mut pending| pending.drain(..).collect::<Vec<_>>(),
        );
        self.inner.dirty.store(false, Ordering::SeqCst);
        items
    }

    /// Whether work is pending.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.inner.dirty.load(Ordering::SeqCst)
    }

    /// The schedule generation counter (for change detection).
    #[must_use]
    pub fn schedule_generation(&self) -> u64 {
        self.inner.schedule_generation.load(Ordering::SeqCst)
    }
}

/// The typed result of executing one provider work item.
///
/// Carries the typed provider messages the background worker routes back
/// through the reducer, plus the effect completion for correlation.
#[derive(Debug, Clone)]
pub struct ProviderExecutionResult {
    /// The original correlation for completion delivery.
    pub correlation: Correlation,
    /// The request key for building the effect completion.
    pub key: crate::domain::effects::ProviderRequestKey,
    /// Typed messages to route back through the reducer, in lifecycle order.
    pub messages: Vec<crate::messages::ProviderMessage>,
    /// Whether the provider process tree was reaped.
    pub process_reaped: bool,
    /// Whether the invocation reached a terminal outcome.
    pub terminal: bool,
}

/// Build a [`ProviderExecutionResult`] from the one-shot supervisor's
/// [`OneShotResult`], translating the lifecycle transcript into typed
/// [`ProviderMessage`] variants for the reducer.
///
/// Progress events are extracted from the transcript and routed as
/// [`ProviderMessage::Progress`]. The terminal outcome/error is routed as
/// [`ProviderMessage::Outcome`] or [`ProviderMessage::Error`]. A supervisor
/// failure is routed as [`ProviderMessage::GenerationFailed`] with the
/// appropriate [`UnavailableReason`].
#[must_use]
pub fn build_execution_result(
    correlation: Correlation,
    result: &crate::runtime::provider::supervisor::OneShotResult,
    key: &crate::domain::effects::ProviderRequestKey,
) -> ProviderExecutionResult {
    use crate::messages::ProviderMessage;
    use crate::runtime::provider::supervisor::OneShotOutcome;

    let mut messages = Vec::new();

    // The provider's own progress payloads, in order and already redacted by
    // the supervisor. Rebuilding them from the transcript's sequence numbers
    // would deliver progress the operator cannot read.
    for payload in result.transcript.progress() {
        messages.push(ProviderMessage::Progress {
            key: key.clone(),
            payload: payload.clone(),
        });
    }

    // Every one-shot lifecycle ends in exactly one terminal: a completed
    // outcome, a typed provider error, or a supervisor failure. There is no
    // fourth shape, which is why the request is always terminal once the
    // supervisor returns.
    messages.push(match &result.outcome {
        OneShotOutcome::Completed(outcome) => ProviderMessage::Outcome {
            key: key.clone(),
            outcome: outcome.clone(),
            now_epoch: epoch_seconds(),
        },
        OneShotOutcome::ProviderError(error) => ProviderMessage::Error {
            key: key.clone(),
            message: error.message.clone(),
        },
        OneShotOutcome::Failed(failure) => ProviderMessage::GenerationFailed {
            key: key.clone(),
            reason: map_supervisor_failure(failure),
        },
    });

    ProviderExecutionResult {
        correlation,
        key: key.clone(),
        messages,
        process_reaped: result.process_reaped,
        terminal: true,
    }
}

/// Map a [`SupervisorFailure`] to the reducer's [`UnavailableReason`].
fn map_supervisor_failure(
    failure: &crate::runtime::provider::supervisor::SupervisorFailure,
) -> crate::state::provider_requests::UnavailableReason {
    use crate::runtime::provider::supervisor::SupervisorFailure;
    use crate::state::provider_requests::UnavailableReason;
    match failure {
        SupervisorFailure::Crashed { .. } => UnavailableReason::Crash,
        SupervisorFailure::Protocol(_) => UnavailableReason::Protocol,
        SupervisorFailure::HandshakeTimeout | SupervisorFailure::InvocationTimeout => {
            UnavailableReason::Timeout
        }
        SupervisorFailure::Io(_)
        | SupervisorFailure::Spawn(_)
        | SupervisorFailure::Environment(_) => UnavailableReason::Eof,
        SupervisorFailure::ShutdownTimeout => UnavailableReason::Timeout,
    }
}

/// Current epoch seconds for deterministic outcome timestamps.
fn epoch_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> crate::domain::Id {
        match crate::domain::Id::parse(value) {
            Ok(parsed) => parsed,
            Err(error) => panic!("id fixture {value:?} must parse: {error}"),
        }
    }

    #[test]
    fn handle_drain_returns_enqueued_items() {
        let handle = ProviderEffectHandle::new();
        assert!(!handle.is_dirty());

        let correlation = Correlation {
            correlation_id: crate::domain::effects::CorrelationId::new(1),
            owner: id("host"),
            screen_generation: 0,
            activation_generation: 0,
            semantic_key: crate::domain::effects::SemanticKey::new(
                crate::domain::effects::EffectFamily::Provider,
                "test",
            ),
        };
        let invocation = crate::domain::effects::ProviderInvocation {
            key: crate::domain::effects::ProviderRequestKey {
                owner: id("host"),
                action_id: id("provider.action"),
                generation: 1,
            },
            arguments: crate::domain::TypedMap::new(),
            context_screen: id("core.dashboard"),
            context_instance: id("instance-1"),
            context_refs: crate::domain::TypedMap::new(),
            continuation: None,
        };

        handle.schedule(ProviderWorkItem {
            invocation,
            correlation,
        });
        assert!(handle.is_dirty());

        let drained = handle.drain();
        assert_eq!(drained.len(), 1);
        assert!(!handle.is_dirty());

        let more = handle.drain();
        assert!(more.is_empty());
    }
}

#[cfg(test)]
#[path = "provider_effect_worker_defer_tests.rs"]
mod defer_tests;
