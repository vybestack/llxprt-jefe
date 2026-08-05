//! Serialization gate between viewer teardown and viewer spawn.
//!
//! Issue #664: two mutually unaware paths attach viewers — the background
//! `AttachScheduler` and the synchronous `RuntimeManager::attach` reached from
//! input handling. The scheduler serializes only itself, so the incident log
//! shows a spawn at 17:02:28.578, a second spawn at 17:02:29.330, and the
//! detach of the first landing between them at 17:02:29.373.
//!
//! Teardown is what makes the overlap dangerous: dropping an `AttachedViewer`
//! kills its multiplexer child and waits for the child to exit, and that runs
//! on a detached thread nobody joins. A new viewer can therefore be attaching
//! to the multiplexer while the previous viewer's teardown is still killing
//! processes.
//!
//! This module records teardown as explicitly in flight and lets spawn wait for
//! it. The wait is **bounded**: a teardown that never finishes must not freeze
//! the UI, so an expired wait proceeds and is reported rather than deadlocking.

use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// How long a spawn waits for in-flight viewer teardown before proceeding.
///
/// `AttachedViewer::drop` allows its child up to 300ms to exit, so this bound
/// covers ordinary teardown with margin while still capping the stall a wedged
/// teardown can impose on the UI.
pub(super) const VIEWER_TEARDOWN_WAIT: Duration = Duration::from_millis(500);

/// Count of viewer teardowns currently in flight.
pub(super) struct ViewerTeardown {
    in_flight: Mutex<usize>,
    idle: Condvar,
}

/// Evidence that one viewer teardown is in flight. Releasing it records the
/// teardown as finished.
pub(super) struct TeardownGuard<'a> {
    owner: &'a ViewerTeardown,
}

impl ViewerTeardown {
    pub(super) const fn new() -> Self {
        Self {
            in_flight: Mutex::new(0),
            idle: Condvar::new(),
        }
    }

    /// Record a teardown as in flight until the returned guard is released.
    ///
    /// Callers must call this **before** handing the viewer to the thread that
    /// drops it; registering inside that thread would leave a window in which a
    /// spawn sees an idle gate while a teardown is already committed.
    pub(super) fn begin(&self) -> TeardownGuard<'_> {
        let mut in_flight = self
            .in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *in_flight += 1;
        drop(in_flight);
        TeardownGuard { owner: self }
    }

    /// Wait up to `bound` for every in-flight teardown to finish.
    ///
    /// Returns `true` when the gate became idle and `false` when the bound
    /// expired first. A `false` result is not an error: the caller proceeds,
    /// because refusing to attach is worse than attaching during a teardown
    /// that has already outlived its own deadline.
    pub(super) fn wait_until_idle(&self, bound: Duration) -> bool {
        let deadline = Instant::now() + bound;
        let mut in_flight = self
            .in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        while *in_flight > 0 {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            let (next, outcome) = self
                .idle
                .wait_timeout(in_flight, remaining)
                .unwrap_or_else(PoisonError::into_inner);
            in_flight = next;
            if outcome.timed_out() && *in_flight > 0 {
                return false;
            }
        }
        true
    }
}

impl Drop for TeardownGuard<'_> {
    fn drop(&mut self) {
        let mut in_flight = self
            .owner
            .in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *in_flight = in_flight.saturating_sub(1);
        if *in_flight == 0 {
            self.owner.idle.notify_all();
        }
    }
}

static VIEWER_TEARDOWN: ViewerTeardown = ViewerTeardown::new();

/// The process-wide gate. Both attach paths and every teardown site share it,
/// which is the point: a per-path gate is what let the incident happen.
pub(super) fn viewer_teardown() -> &'static ViewerTeardown {
    &VIEWER_TEARDOWN
}
