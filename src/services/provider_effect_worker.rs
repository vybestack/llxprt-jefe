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

use crate::domain::effects::{Correlation, ProviderEffect, ProviderInvocation, ProviderRequestKey};

/// One manifest-bound panel command scheduled for edge delivery.
#[derive(Debug, Clone)]
pub struct ProviderPanelWorkItem {
    /// The pure command staged by the reducer.
    pub effect: ProviderEffect,
    /// The effect correlation for completion delivery.
    pub correlation: Correlation,
}

/// One provisional pre-Configure migration scheduled on the existing provider worker.
#[derive(Debug)]
pub struct ProviderMigrationWorkItem {
    /// The fully composed provider-owned migration request.
    pub request: crate::runtime::provider::migration::MigrationRequest,
    /// Host draft identity used to correlate the eventual Settings message.
    pub draft_token: u64,
}

/// One unit of pending provider work.
#[derive(Debug, Clone)]
pub struct ProviderWorkItem {
    /// The invocation the supervisor will execute.
    pub invocation: ProviderInvocation,
    /// The effect correlation for completion delivery.
    pub correlation: Correlation,
}

/// Shared handle for the input path to schedule provider effects and cancels.
///
/// Cloning is cheap (shares the inner `Arc`). The background worker drains the
/// invoke queue asynchronously and forwards cancels to the active streaming
/// session (S17).
#[derive(Clone)]
pub struct ProviderEffectHandle {
    inner: Arc<Inner>,
}

struct Inner {
    pending: Mutex<VecDeque<ProviderWorkItem>>,
    panel_commands: Mutex<VecDeque<ProviderPanelWorkItem>>,
    migrations: Mutex<VecDeque<ProviderMigrationWorkItem>>,
    /// Pending cancel requests forwarded to the active session (S17).
    cancels: Mutex<Vec<ProviderRequestKey>>,
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
                panel_commands: Mutex::new(VecDeque::new()),
                migrations: Mutex::new(VecDeque::new()),
                cancels: Mutex::new(Vec::new()),
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
    /// Enqueue one panel command for edge delivery.
    pub fn schedule_panel(&self, item: ProviderPanelWorkItem) {
        if let Ok(mut pending) = self.inner.panel_commands.lock() {
            pending.push_back(item);
            self.inner.dirty.store(true, Ordering::SeqCst);
            self.inner
                .schedule_generation
                .fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Enqueue one provisional config migration on the existing provider worker.
    pub fn schedule_migration(&self, item: ProviderMigrationWorkItem) {
        if let Ok(mut pending) = self.inner.migrations.lock() {
            pending.push_back(item);
            self.inner.dirty.store(true, Ordering::SeqCst);
            self.inner
                .schedule_generation
                .fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Drain pending provisional config migrations in reducer order.
    #[must_use]
    pub fn drain_migrations(&self) -> Vec<ProviderMigrationWorkItem> {
        self.inner.migrations.lock().map_or_else(
            |_poisoned| Vec::new(),
            |mut pending| pending.drain(..).collect(),
        )
    }

    /// Drain pending panel commands in reducer order.
    #[must_use]
    pub fn drain_panel_commands(&self) -> Vec<ProviderPanelWorkItem> {
        self.inner.panel_commands.lock().map_or_else(
            |_poisoned| Vec::new(),
            |mut pending| pending.drain(..).collect(),
        )
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

    /// Enqueue a cancel for the active streaming session (S17). The worker
    /// forwards this to the session whose key matches, setting its cancel flag.
    pub fn schedule_cancel(&self, key: ProviderRequestKey) {
        if let Ok(mut cancels) = self.inner.cancels.lock() {
            cancels.push(key);
        } else {
            tracing::error!("provider cancellation queue poisoned; cancel rejected");
        }
    }

    /// Drain all pending cancel requests. Called by the background worker each
    /// poll iteration so cancel remains observable while an invocation runs.
    #[must_use]
    pub fn drain_cancels(&self) -> Vec<ProviderRequestKey> {
        if let Ok(mut cancels) = self.inner.cancels.lock() {
            std::mem::take(&mut *cancels)
        } else {
            tracing::error!("provider cancellation queue poisoned; cancel drain skipped");
            Vec::new()
        }
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
    /// A bounded cleanup/lifecycle fault observed after the terminal result
    /// (S26). Never replaces the first terminal message; surfaced as a
    /// separate diagnostic so the full shutdown/ack/EOF/reap evidence is
    /// visible alongside (not instead of) the authoritative result.
    pub cleanup_diagnostic: Option<crate::runtime::provider::supervisor::CleanupFailure>,
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
    build_execution_result_with_progress(correlation, result, key, true)
}

/// Build a terminal execution result after progress was already delivered live.
#[must_use]
pub fn build_streaming_execution_result(
    correlation: Correlation,
    result: &crate::runtime::provider::supervisor::OneShotResult,
    key: &crate::domain::effects::ProviderRequestKey,
) -> ProviderExecutionResult {
    build_execution_result_with_progress(correlation, result, key, false)
}

fn build_execution_result_with_progress(
    correlation: Correlation,
    result: &crate::runtime::provider::supervisor::OneShotResult,
    key: &crate::domain::effects::ProviderRequestKey,
    replay_progress: bool,
) -> ProviderExecutionResult {
    use crate::messages::ProviderMessage;
    use crate::runtime::provider::supervisor::OneShotOutcome;

    let mut messages = Vec::new();

    if replay_progress {
        for payload in result.transcript.progress() {
            messages.push(ProviderMessage::Progress {
                key: key.clone(),
                payload: payload.clone(),
            });
        }
    }

    // Every one-shot lifecycle ends in exactly one terminal: a completed
    // outcome, a typed provider error, a host cancel, or a supervisor failure.
    // For a cancel, the reducer already marked the request Cancelled before the
    // session observed the signal (S17). A Cancel must NOT be reported as
    // unavailable — no `GenerationFailed` is pushed, so the reducer's Cancelled
    // state stays authoritative. The effect completion closes the work item.
    match &result.outcome {
        OneShotOutcome::Completed(outcome) => messages.push(ProviderMessage::Outcome {
            key: key.clone(),
            outcome: outcome.clone(),
            now_epoch: epoch_seconds(),
        }),
        OneShotOutcome::ProviderError(error) => messages.push(ProviderMessage::Error {
            key: key.clone(),
            message: format!("{} {}", error.code, error.message)
                .trim()
                .to_owned(),
        }),
        OneShotOutcome::Cancelled => {}
        OneShotOutcome::Failed(failure) => messages.push(ProviderMessage::GenerationFailed {
            key: key.clone(),
            reason: map_supervisor_failure(failure),
        }),
    }

    ProviderExecutionResult {
        correlation,
        key: key.clone(),
        messages,
        process_reaped: result.process_reaped,
        terminal: true,
        cleanup_diagnostic: result.cleanup_failure.clone(),
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
        | SupervisorFailure::Containment { .. }
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
    use crate::domain::effects::ProviderRequestKey;
    use crate::messages::ProviderMessage;
    use crate::runtime::provider::outcome::LifecycleTranscript;
    use crate::runtime::provider::protocol::{Outcome, Severity};
    use crate::runtime::provider::supervisor::{
        CleanupFailure, OneShotOutcome, OneShotResult, SupervisorFailure,
    };
    use crate::state::provider_requests::UnavailableReason;

    fn id(value: &str) -> crate::domain::Id {
        match crate::domain::Id::parse(value) {
            Ok(parsed) => parsed,
            Err(error) => panic!("id fixture {value:?} must parse: {error}"),
        }
    }

    fn correlation() -> Correlation {
        Correlation {
            correlation_id: crate::domain::effects::CorrelationId::new(1),
            owner: id("host"),
            screen_generation: 0,
            activation_generation: 0,
            semantic_key: crate::domain::effects::SemanticKey::new(
                crate::domain::effects::EffectFamily::Provider,
                "test",
            ),
        }
    }

    fn key() -> ProviderRequestKey {
        ProviderRequestKey {
            owner: id("host"),
            action_id: id("provider.action"),
            generation: 1,
        }
    }

    fn notice_result_with_cleanup(cleanup: Option<CleanupFailure>) -> OneShotResult {
        OneShotResult {
            outcome: OneShotOutcome::Completed(Outcome::Notice {
                severity: Severity::Info,
                message: "done".to_owned(),
            }),
            transcript: LifecycleTranscript::default(),
            retained_stderr: String::new(),
            stderr_truncated: false,
            process_reaped: true,
            exit_code: Some(0),
            cleanup_failure: cleanup,
        }
    }

    #[test]
    fn handle_drain_returns_enqueued_items() {
        let handle = ProviderEffectHandle::new();
        assert!(!handle.is_dirty());

        let invocation = crate::domain::effects::ProviderInvocation {
            key: key(),
            arguments: crate::domain::TypedMap::new(),
            context_screen: id("core.dashboard"),
            context_instance: id("instance-1"),
            context_refs: crate::domain::TypedMap::new(),
            continuation: None,
        };

        handle.schedule(ProviderWorkItem {
            invocation,
            correlation: correlation(),
        });
        assert!(handle.is_dirty());

        let drained = handle.drain();
        assert_eq!(drained.len(), 1);
        assert!(!handle.is_dirty());

        let more = handle.drain();
        assert!(more.is_empty());
    }

    // ── S26: cleanup diagnostic preserved alongside the terminal result ────

    #[test]
    fn build_result_preserves_cleanup_diagnostic_alongside_terminal() {
        let result = notice_result_with_cleanup(Some(CleanupFailure::DrainTimeout));
        let exec = build_execution_result(correlation(), &result, &key());
        // The terminal outcome is still delivered as the authoritative message.
        assert!(
            exec.messages
                .iter()
                .any(|m| matches!(m, ProviderMessage::Outcome { .. })),
            "terminal outcome must be present: {:?}",
            exec.messages
        );
        // The cleanup fault is surfaced separately and never replaces it (S26).
        assert!(
            matches!(&exec.cleanup_diagnostic, Some(CleanupFailure::DrainTimeout)),
            "cleanup diagnostic preserved: {:?}",
            exec.cleanup_diagnostic
        );
        assert!(exec.process_reaped);
        assert!(exec.terminal);
    }

    #[test]
    fn build_result_without_cleanup_carries_no_diagnostic() {
        let result = notice_result_with_cleanup(None);
        let exec = build_execution_result(correlation(), &result, &key());
        assert!(exec.cleanup_diagnostic.is_none());
    }

    #[test]
    fn build_result_cancelled_outcome_does_not_report_unavailable() {
        let result = OneShotResult {
            outcome: OneShotOutcome::Cancelled,
            ..notice_result_with_cleanup(None)
        };
        let exec = build_execution_result(correlation(), &result, &key());
        // A host cancel must NOT be reported as unavailable: no
        // GenerationFailed message is pushed. The reducer already holds the
        // Cancelled state (S17), and the effect completion closes the work
        // item without overwriting first-terminal semantics.
        assert!(
            exec.messages
                .iter()
                .all(|message| !matches!(message, ProviderMessage::GenerationFailed { .. })),
            "cancelled must not produce a GenerationFailed message: {:?}",
            exec.messages
        );
        assert!(exec.terminal);
    }

    #[test]
    fn build_result_maps_supervisor_failure_to_generation_failed() {
        let result = OneShotResult {
            outcome: OneShotOutcome::Failed(SupervisorFailure::InvocationTimeout),
            ..notice_result_with_cleanup(None)
        };
        let exec = build_execution_result(correlation(), &result, &key());
        assert!(
            exec.messages.iter().any(|m| matches!(
                m,
                ProviderMessage::GenerationFailed {
                    reason: UnavailableReason::Timeout,
                    ..
                }
            )),
            "invocation timeout must map to GenerationFailed(Timeout): {:?}",
            exec.messages
        );
    }
}

#[cfg(test)]
#[path = "provider_effect_worker_defer_tests.rs"]
mod defer_tests;
