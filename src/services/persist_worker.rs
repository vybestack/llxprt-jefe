//! Coalescing persistence worker (issue #301).
//!
//! Moves filesystem durability off the input/render hot path. The input path
//! calls [`PersistHandle::schedule`] (stores the latest snapshot under a short
//! lock and bumps a generation counter — **no I/O**). A background `smol`
//! future drains the pending slot via [`smol::unblock`] and invokes the
//! supplied persistence function. An [`AtomicU64`] records the applied
//! generation so a late write whose snapshot predates the latest schedule is
//! skipped (newest-wins under reordered completions).
//!
//! Design constraints (issue #301 plan, Phase 1):
//! - Newest snapshot wins even when write completions are reordered.
//! - Rapid schedules are coalesced into bounded durable writes.
//! - Persistence errors surface without blocking the caller.
//! - Orderly shutdown flushes the final slot synchronously.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::persistence::State as PersistedState;

/// Function type for the durable write boundary.
///
/// Takes the snapshot to persist and returns `Ok(())` on success or an error
/// string on failure. The actual I/O stays in `persistence/`; this abstraction
/// lets the worker be tested with controllable doubles.
pub type PersistFn = Arc<dyn Fn(&PersistedState) -> Result<(), String> + Send + Sync>;

/// Pending persistence request: the latest snapshot plus its schedule
/// generation.
struct PendingSlot {
    snapshot: Option<PersistedState>,
    generation: u64,
}

/// Shared handle for the input/render path to schedule persistence.
///
/// Cloning is cheap (it shares the inner `Arc`). Call [`Self::schedule`] to
/// enqueue a snapshot; the background worker drains it asynchronously.
#[derive(Clone)]
pub struct PersistHandle {
    inner: Arc<Inner>,
}

struct Inner {
    pending: Mutex<PendingSlot>,
    applied_generation: AtomicU64,
    schedule_generation: AtomicU64,
    persist_fn: PersistFn,
}

impl std::fmt::Debug for PersistHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PersistHandle").finish_non_exhaustive()
    }
}

/// Lock a mutex, recovering from poison by taking the inner guard.
fn lock_or_panic<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::error!("persist mutex poisoned; recovering");
            poisoned.into_inner()
        }
    }
}

impl PersistHandle {
    /// Create a new worker with the given persistence function.
    #[must_use]
    pub fn new(persist_fn: PersistFn) -> Self {
        Self {
            inner: Arc::new(Inner {
                pending: Mutex::new(PendingSlot {
                    snapshot: None,
                    generation: 0,
                }),
                applied_generation: AtomicU64::new(0),
                schedule_generation: AtomicU64::new(0),
                persist_fn,
            }),
        }
    }

    /// Schedule a snapshot for persistence. This is the input-path call — it
    /// stores the snapshot under a short lock and bumps the generation. No I/O
    /// occurs here.
    pub fn schedule(&self, snapshot: PersistedState) {
        let generation = self
            .inner
            .schedule_generation
            .fetch_add(1, Ordering::SeqCst)
            + 1;
        let mut pending = lock_or_panic(&self.inner.pending);
        pending.snapshot = Some(snapshot);
        pending.generation = generation;
    }

    /// Take the pending snapshot and its generation for off-thread writing.
    ///
    /// The caller offloads `persist_fn` to `smol::unblock`, then calls
    /// [`Self::commit`] with the generation and [`Self::clear_pending`].
    #[must_use]
    pub fn take_pending(&self) -> Option<(PersistedState, u64)> {
        let pending = lock_or_panic(&self.inner.pending);
        let snapshot = pending.snapshot.clone()?;
        Some((snapshot, pending.generation))
    }

    /// Record that a write at `generation` has been applied.
    ///
    /// Returns `true` if the write was committed, `false` if it was stale
    /// (a newer schedule arrived or an equal/newer generation was already
    /// applied).
    #[must_use]
    pub fn commit(&self, generation: u64) -> bool {
        if self.inner.schedule_generation.load(Ordering::SeqCst) > generation {
            return false;
        }
        loop {
            let current = self.inner.applied_generation.load(Ordering::SeqCst);
            if current >= generation {
                return false;
            }
            if self
                .inner
                .applied_generation
                .compare_exchange(current, generation, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return true;
            }
        }
    }

    /// Clear the pending slot (called after a successful drain).
    pub fn clear_pending(&self) {
        let mut pending = lock_or_panic(&self.inner.pending);
        pending.snapshot = None;
    }

    /// Get the persistence function (for the worker loop to call).
    #[must_use]
    pub fn persist_fn(&self) -> PersistFn {
        Arc::clone(&self.inner.persist_fn)
    }

    /// Get the current schedule generation (for testing).
    #[must_use]
    #[cfg(test)]
    pub fn schedule_generation(&self) -> u64 {
        self.inner.schedule_generation.load(Ordering::SeqCst)
    }

    /// Get the current applied generation (for testing).
    #[must_use]
    #[cfg(test)]
    pub fn applied_generation(&self) -> u64 {
        self.inner.applied_generation.load(Ordering::SeqCst)
    }

    /// Synchronously flush the final pending snapshot (shutdown path).
    pub fn shutdown_flush(&self) {
        let Some((snapshot, generation)) = self.take_pending() else {
            return;
        };
        if let Err(e) = (self.inner.persist_fn)(&snapshot) {
            tracing::warn!(error = %e, "shutdown persist failed");
        }
        self.clear_pending();
        let _ = self.commit(generation);
    }
}

/// The newest-wins logic for reordered completions, as a pure function.
#[must_use]
pub fn should_commit(applied_generation: u64, request_generation: u64) -> bool {
    request_generation > applied_generation
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn make_state(label: &str) -> PersistedState {
        let mut state = PersistedState::default_with_version();
        state.hide_idle_repositories = label.contains("hide");
        state
    }

    fn counting_persist_fn() -> (PersistFn, Arc<AtomicUsize>, Arc<Mutex<PersistedState>>) {
        let count = Arc::new(AtomicUsize::new(0));
        let last: Arc<Mutex<PersistedState>> =
            Arc::new(Mutex::new(PersistedState::default_with_version()));
        let count_clone = Arc::clone(&count);
        let last_clone = Arc::clone(&last);
        let f: PersistFn = Arc::new(move |state: &PersistedState| {
            count_clone.fetch_add(1, Ordering::SeqCst);
            let Ok(mut guard) = last_clone.lock() else {
                return Err("lock poisoned".to_string());
            };
            *guard = state.clone();
            Ok(())
        });
        (f, count, last)
    }

    fn require_pending(handle: &PersistHandle) -> (PersistedState, u64) {
        let Some(pending) = handle.take_pending() else {
            panic!("pending should exist");
        };
        pending
    }

    #[test]
    fn persist_worker_orders_newest_wins_under_reordered_completions() {
        let (f, count, last) = counting_persist_fn();
        let handle = PersistHandle::new(f);

        let state_a = make_state("state-a");
        let state_b = make_state("state-b-hide");
        handle.schedule(state_a.clone());
        handle.schedule(state_b.clone());

        assert!(
            !handle.commit(1),
            "gen 1 is stale because gen 2 was scheduled after it"
        );
        assert!(handle.commit(2), "gen 2 is the newest and should commit");
        assert_eq!(handle.applied_generation(), 2);

        let persist_result = (handle.persist_fn())(&state_b);
        assert!(persist_result.is_ok(), "persist should succeed");
        let Ok(durable_guard) = last.lock() else {
            panic!("lock poisoned");
        };
        let durable = durable_guard.clone();
        assert_eq!(
            durable.hide_idle_repositories, state_b.hide_idle_repositories,
            "durable state must be gen 2 (newest)"
        );
        assert!(count.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn persist_worker_coalesces_rapid_schedules() {
        let (f, _count, _last) = counting_persist_fn();
        let handle = PersistHandle::new(f);

        for i in 0..99 {
            handle.schedule(make_state(&format!("state-{i}")));
        }
        handle.schedule(make_state("state-99-hide"));

        let (snapshot, generation) = require_pending(&handle);
        assert_eq!(
            generation, 100,
            "generation must be 100 after 100 schedules"
        );
        assert!(
            snapshot.hide_idle_repositories,
            "latest snapshot (state-99-hide) must have hide_idle"
        );

        let (_snapshot2, gen2) = require_pending(&handle);
        assert_eq!(gen2, 100);

        handle.clear_pending();
        assert!(
            handle.take_pending().is_none(),
            "cleared slot must be empty"
        );
    }

    #[test]
    fn persist_worker_reports_failure_without_blocking() {
        let fail_count = Arc::new(AtomicUsize::new(0));
        let fail_clone = Arc::clone(&fail_count);
        let f: PersistFn = Arc::new(move |_state: &PersistedState| {
            fail_clone.fetch_add(1, Ordering::SeqCst);
            Err("disk full".to_string())
        });
        let handle = PersistHandle::new(f);

        handle.schedule(make_state("test"));
        assert_eq!(
            fail_count.load(Ordering::SeqCst),
            0,
            "schedule must not invoke persist_fn"
        );

        let (snapshot, generation) = require_pending(&handle);
        let result = (handle.persist_fn())(&snapshot);
        assert!(result.is_err(), "persist should fail");

        let _ = handle.commit(generation);
        handle.clear_pending();
        handle.schedule(make_state("test2"));
        let (_snapshot2, gen2) = require_pending(&handle);
        assert_eq!(gen2, 2, "generation continues after failure");
    }

    #[test]
    fn persist_worker_shutdown_flushes_final_snapshot() {
        let (f, _count, last) = counting_persist_fn();
        let handle = PersistHandle::new(f);

        let final_state = make_state("final-hide");
        handle.schedule(final_state.clone());

        let Ok(durable_guard) = last.lock() else {
            panic!("lock poisoned");
        };
        assert!(
            !durable_guard.hide_idle_repositories,
            "durable state should be default before flush"
        );
        drop(durable_guard);

        handle.shutdown_flush();

        let Ok(durable_guard) = last.lock() else {
            panic!("lock poisoned");
        };
        let durable = durable_guard.clone();
        assert_eq!(
            durable.hide_idle_repositories, final_state.hide_idle_repositories,
            "shutdown_flush must write the final snapshot"
        );
        assert!(
            handle.take_pending().is_none(),
            "pending slot must be empty after shutdown_flush"
        );
    }

    #[test]
    fn persist_worker_shutdown_flush_noop_when_empty() {
        let (f, count, _last) = counting_persist_fn();
        let handle = PersistHandle::new(f);

        handle.shutdown_flush();
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "shutdown_flush with no pending must be a no-op"
        );
    }

    #[test]
    fn should_commit_rejects_older_generation() {
        assert!(!should_commit(5, 3));
    }

    #[test]
    fn should_commit_rejects_equal_generation() {
        assert!(!should_commit(5, 5));
    }

    #[test]
    fn should_commit_accepts_newer_generation() {
        assert!(should_commit(3, 5));
    }

    #[test]
    fn commit_is_monotonic_and_rejects_stale() {
        let (f, _count, _last) = counting_persist_fn();
        let handle = PersistHandle::new(f);

        assert!(handle.commit(5), "gen 5 should commit (initial)");
        assert!(!handle.commit(3), "gen 3 is stale");
        assert!(!handle.commit(5), "gen 5 already applied");
        assert!(handle.commit(10), "gen 10 should commit");
        assert_eq!(handle.applied_generation(), 10);
    }
}
