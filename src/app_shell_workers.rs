//! Background worker futures and cache helpers extracted from `app_shell`
//! (issue #301). These run on the smol executor alongside the input/render
//! loop but perform all external I/O via `smol::unblock`, keeping the
//! executor free for keyboard events.

use std::sync::Arc;
use std::time::Duration;

use tracing::warn;

use jefe::runtime::{HISTORY_LINE_CAP, RuntimeManager, capture_pane_history, strip_trailing_rows};
use jefe::services::capture_worker::{CaptureHandle, should_store_result};

use crate::AppContext;

/// Poll interval for the persistence worker drain loop.
const PERSIST_POLL_MS: u64 = 50;

/// Run the coalescing persistence worker drain loop.
///
/// Polls [`PersistHandle::take_pending`] every [`PERSIST_POLL_MS`] and
/// offloads the durable write to `smol::unblock`. When no snapshot is
/// pending, the loop yields immediately.
pub async fn run_persist_worker(ctx: Option<Arc<std::sync::Mutex<AppContext>>>) {
    loop {
        smol::Timer::after(Duration::from_millis(PERSIST_POLL_MS)).await;
        let Some(ctx_arc) = &ctx else {
            continue;
        };
        let handle_and_fn = {
            let Ok(ctx_guard) = ctx_arc.lock() else {
                continue;
            };
            let handle = ctx_guard.persist_handle.clone();
            let request = handle.take_pending();
            request
                .map(|(state, generation)| (handle.clone(), handle.persist_fn(), state, generation))
        };
        let Some((handle, persist_fn, state, generation)) = handle_and_fn else {
            continue;
        };
        // Clear the pending slot only if its generation matches — a newer
        // schedule arriving between take_pending and this clear must not be
        // discarded.
        handle.clear_pending_if(generation);
        let result = smol::unblock(move || persist_fn(&state)).await;
        if let Err(e) = result {
            warn!(error = %e, "background persist failed");
        }
        let _ = handle.commit(generation);
    }
}

/// Poll interval for the capture worker drain loop.
const CAPTURE_POLL_MS: u64 = 50;

/// Run the background capture worker drain loop (issue #301 Phase 2).
///
/// Polls the `CaptureHandle` pending slot and offloads `capture_pane_history`
/// to `smol::unblock`. The result is stored in the runtime's `HistoryCache`
/// only if the `(agent_id, generation)` still matches the currently attached
/// session (stale-result guard).
pub async fn run_capture_worker(ctx: Option<Arc<std::sync::Mutex<AppContext>>>) {
    loop {
        smol::Timer::after(Duration::from_millis(CAPTURE_POLL_MS)).await;
        let Some(ctx_arc) = &ctx else {
            continue;
        };
        let capture_request = {
            let Ok(ctx_guard) = ctx_arc.lock() else {
                continue;
            };
            ctx_guard.capture_handle.take_pending()
        };
        let Some(request) = capture_request else {
            continue;
        };
        let session_name = request.session_name.clone();
        let agent_id = request.agent_id.clone();
        let generation = request.generation;
        let captured =
            smol::unblock(move || capture_pane_history(&session_name, HISTORY_LINE_CAP)).await;
        if captured.is_none() {
            warn!(session_name = %request.session_name, "background capture-pane failed; preserving prior cache");
        }
        let Ok(mut ctx_guard) = ctx_arc.lock() else {
            continue;
        };
        let current_agent = ctx_guard.runtime.attached_agent();
        let current_generation = ctx_guard.runtime.output_generation();
        let is_current = should_store_result(
            &agent_id,
            generation,
            current_agent,
            Some(current_generation),
        );
        if is_current && let Some(raw_lines) = captured {
            let live_rows = ctx_guard.runtime.snapshot().map_or(0, |s| s.rows);
            let lines = strip_trailing_rows(raw_lines, live_rows);
            ctx_guard
                .runtime
                .history_cache_store(&agent_id, generation, Some(lines));
        }
    }
}

/// Read history lines from the runtime cache (issue #301 Phase 2).
///
/// The render path calls this instead of `capture_history` (which shells out
/// to `tmux capture-pane` synchronously). This function:
/// 1. Requests a background capture via the `CaptureHandle` (cheap, no I/O).
/// 2. Reads the runtime's `HistoryCache` directly (non-blocking).
#[must_use]
pub fn capture_history_from_cache(ctx: Option<&Arc<std::sync::Mutex<AppContext>>>) -> Vec<String> {
    let Some(ctx_arc) = ctx else {
        return Vec::new();
    };
    let Ok(ctx_guard) = ctx_arc.try_lock() else {
        return Vec::new();
    };
    let handle: &CaptureHandle = &ctx_guard.capture_handle;
    let (attached_agent, session_name, generation) = match ctx_guard.runtime.attached_agent() {
        Some(agent_id) => {
            let Some(session) = ctx_guard.runtime.get_session(agent_id) else {
                return Vec::new();
            };
            (
                agent_id.clone(),
                session.session_name.clone(),
                ctx_guard.runtime.output_generation(),
            )
        }
        None => return Vec::new(),
    };
    handle.request(attached_agent.clone(), session_name, generation);
    ctx_guard
        .runtime
        .history_cache_get(&attached_agent, generation)
        .cloned()
        .unwrap_or_default()
}

/// Synchronously flush the persist worker's pending snapshot.
///
/// Called from the shutdown path so the final state is durable before exit.
pub fn shutdown_flush_persist(ctx: Option<&Arc<std::sync::Mutex<AppContext>>>) {
    let Some(ctx_arc) = ctx else {
        return;
    };
    let Ok(ctx_guard) = ctx_arc.lock() else {
        return;
    };
    ctx_guard.persist_handle.shutdown_flush();
}

/// Synchronously drain any pending capture request (shutdown path).
///
/// Called from the shutdown path so a pending capture does not leave the
/// capture worker mid-flight on exit. This is best-effort: if the capture
/// cannot complete, the prior cache is preserved.
pub fn shutdown_flush_capture(ctx: Option<&Arc<std::sync::Mutex<AppContext>>>) {
    let Some(ctx_arc) = ctx else {
        return;
    };
    let Ok(ctx_guard) = ctx_arc.lock() else {
        return;
    };
    // Take and discard the pending request — the cache already holds the
    // last good snapshot, and a synchronous capture on shutdown would block
    // the exit path.
    let _ = ctx_guard.capture_handle.take_pending();
}
