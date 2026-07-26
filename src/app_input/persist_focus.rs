//! Persistence helpers: staging and scheduling durable saves.
//!
//! These live in the app-shell layer because scheduling a write needs the
//! shared context, which neither `state/` nor `persistence/` may depend on.

use jefe::services::persist_worker::PersistRequest;
use jefe::state::AppState;

use super::SharedContext;

/// Persist the current state to disk via the shared context's persistence
/// manager.
///
/// When a coalescing [`PersistHandle`] is present in the context (issue #301),
/// the candidate is scheduled for asynchronous durable write instead of
/// writing on the input path. This keeps the input/render path from blocking
/// on `fsync`. Persistence failures are surfaced by the background worker
/// (logged via `tracing::warn`); the input path never blocks on I/O.
///
/// If `schedule` returns `false` (the handle was not initialized), the
/// snapshot is silently dropped — the background worker was never set up,
/// so there is no durable write path. This only happens in edge cases like
/// startup before the worker is wired.
pub fn persist_state(ctx: &SharedContext, request: PersistRequest) {
    let Some(ctx_arc) = ctx else {
        return;
    };
    let Ok(ctx_guard) = ctx_arc.lock() else {
        return;
    };
    // Issue #301: schedule the snapshot for the coalescing background worker
    // instead of performing a synchronous durable write. The worker drains
    // the slot and writes asynchronously.
    if !ctx_guard.persist_handle.schedule(request) {
        tracing::trace!("persist_state: persist handle not initialized; skipping durable write");
    }
}

/// Stage a durable save and schedule the resulting candidate (issue #381).
///
/// Staging happens on the committed state so the reducer decides what the
/// durable document contains; only the bounded [`PersistRequest`] crosses into
/// the worker. A projection failure surfaces through the state's error channel
/// rather than writing a degraded document.
/// Stage a durable save on the committed state (issue #381).
///
/// Called while the state guard is held; the returned request is scheduled by
/// [`schedule_durable_save`] *after* the guard is released, so the state and
/// context locks are never held simultaneously.
#[must_use]
pub fn durable_save_request(state: &mut AppState) -> Option<PersistRequest> {
    let (candidate, revision, correlation) = state.take_durable_save_request()?;
    Some(PersistRequest {
        candidate,
        revision,
        correlation,
    })
}

/// Schedule a staged durable save for the background worker.
///
/// A `None` request means the reducer declined to stage one (a projection
/// failure already surfaced through the state's error channel), so nothing is
/// written rather than writing a degraded document.
pub fn schedule_durable_save(ctx: &SharedContext, request: Option<PersistRequest>) {
    let Some(request) = request else {
        return;
    };
    persist_state(ctx, request);
}
