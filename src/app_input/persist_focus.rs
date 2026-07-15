//! Persistence helpers: state serialization and pane-focus conversion.
//!
//! `pane_focus_to_persisted` / `pane_focus_from_persisted` bridge the state-layer
//! `PaneFocus` enum and the string form stored in the persisted `State` DTO. They
//! live in the app-shell layer (not in `persistence/`) because the persistence
//! module is restricted to `domain/` dependencies and cannot reference
//! `state::PaneFocus`. See issue #160.

use jefe::persistence::State as PersistedState;
use jefe::state::PaneFocus;

use super::SharedContext;

/// Serialize a `PaneFocus` to its persisted string form.
#[must_use]
pub fn pane_focus_to_persisted(focus: PaneFocus) -> String {
    match focus {
        PaneFocus::Repositories => "repositories",
        PaneFocus::Agents => "agents",
        PaneFocus::Terminal => "terminal",
    }
    .to_owned()
}

/// Parse a persisted pane-focus string back into `PaneFocus`.
///
/// Unknown or empty strings (e.g. older state files written before this field
/// existed) fall back to `Repositories`, matching the pre-existing default.
#[must_use]
pub fn pane_focus_from_persisted(value: &str) -> PaneFocus {
    match value {
        "agents" => PaneFocus::Agents,
        "terminal" => PaneFocus::Terminal,
        _ => PaneFocus::Repositories,
    }
}

/// Persist the current state to disk via the shared context's persistence
/// manager.
///
/// When a coalescing [`PersistHandle`] is present in the context (issue #301),
/// the snapshot is scheduled for asynchronous durable write instead of calling
/// `save_state` synchronously. This keeps the input/render path from blocking
/// on `fsync`. Persistence failures are surfaced by the background worker
/// (logged via `tracing::warn`); the input path never blocks on I/O.
///
/// If `schedule` returns `false` (the handle was not initialized), the
/// snapshot is silently dropped — the background worker was never set up,
/// so there is no durable write path. This only happens in edge cases like
/// startup before the worker is wired.
pub fn persist_state(ctx: &SharedContext, persisted: &PersistedState) {
    let Some(ctx_arc) = ctx else {
        return;
    };
    let Ok(ctx_guard) = ctx_arc.lock() else {
        return;
    };
    // Issue #301: schedule the snapshot for the coalescing background worker
    // instead of performing a synchronous durable write. The worker drains
    // the slot and writes asynchronously.
    if !ctx_guard.persist_handle.schedule(persisted.clone()) {
        tracing::trace!("persist_state: persist handle not initialized; skipping durable write");
    }
}
