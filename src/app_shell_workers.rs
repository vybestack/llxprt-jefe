//! Background worker futures and cache helpers extracted from `app_shell`
//! (issue #301). These run on the smol executor alongside the input/render
//! loop but perform all external I/O via `smol::unblock`, keeping the
//! executor free for keyboard events.

use std::sync::Arc;
use std::time::Duration;

use tracing::{debug, warn};

use jefe::domain::AgentId;
use jefe::runtime::{
    HISTORY_LINE_CAP, LivenessIdentity, RuntimeManager, capture_pane_history, strip_trailing_rows,
};
use jefe::services::capture_worker::{CaptureHandle, should_store_result};

use crate::AppContext;

/// Check whether the attached PTY has new data since the last render.
///
/// Uses `try_lock` so the timer future never blocks on the `AppContext` mutex.
/// A contended lock defers the check until the next poll iteration.
pub fn is_pty_dirty(ctx: Option<&Arc<std::sync::Mutex<AppContext>>>) -> bool {
    let Some(ctx_arc) = ctx else {
        return false;
    };
    let Ok(ctx_guard) = ctx_arc.try_lock() else {
        return false;
    };
    ctx_guard.runtime.take_dirty()
}

/// Capture frozen pane previews for newly dead local agents without holding
/// `AppContext` (issue #374).
///
/// Runtime sessions dedicate window 0 to the owning agent; embedded shells use
/// the separately named `jefe-shell` window, so dead-agent capture targets `:0`.
pub async fn capture_dead_previews(
    targets: Vec<LivenessIdentity>,
) -> Vec<(LivenessIdentity, Vec<String>)> {
    smol::unblock(move || {
        targets
            .into_iter()
            .filter_map(|target| {
                let Some(session_name) = target.binding_session_name.as_deref() else {
                    debug!(agent_id = %target.agent_id.0, "dead preview skipped without a runtime binding");
                    return None;
                };
                let pane_target = format!("{session_name}:0");
                match jefe::runtime::capture_pane_lines_result(&pane_target) {
                    Ok(lines) => Some((target, lines)),
                    Err(error) => {
                        warn!(agent_id = %target.agent_id.0, error = %error, "dead preview capture failed");
                        None
                    }
                }
            })
            .collect()
    })
    .await
}

/// Poll interval for the persistence worker drain loop.
const PERSIST_POLL_MS: u64 = 50;

/// Run the coalescing persistence worker drain loop.
///
/// Polls [`PersistHandle::take_pending`] every [`PERSIST_POLL_MS`] and
/// offloads the durable write to `smol::unblock`. When no snapshot is
/// pending, the loop yields immediately.
pub async fn run_persist_worker(
    ctx: Option<Arc<std::sync::Mutex<AppContext>>>,
    mut app_state: crate::app_input::AppStateHandle,
) {
    let Some(ctx_arc) = ctx.as_ref() else {
        return;
    };
    loop {
        smol::Timer::after(Duration::from_millis(PERSIST_POLL_MS)).await;
        let handle_and_fn = {
            let Ok(ctx_guard) = ctx_arc.lock() else {
                continue;
            };
            let handle = ctx_guard.persist_handle.clone();
            let request = handle.take_pending();
            request.map(|(request, generation)| {
                (handle.clone(), handle.persist_fn(), request, generation)
            })
        };
        let Some((handle, persist_fn, request, generation)) = handle_and_fn else {
            continue;
        };
        let completion = run_persist_cycle(&handle, persist_fn, request, generation).await;
        deliver_persist_completion(&mut app_state, completion);
    }
}

/// Perform one offloaded persist attempt and map its result to a completion.
///
/// The write runs on a blocking thread so the UI loop is never stalled; the
/// returned completion carries the same correlation the save was staged with.
async fn run_persist_cycle(
    handle: &jefe::services::persist_worker::PersistHandle,
    persist_fn: jefe::services::persist_worker::PersistFn,
    request: jefe::services::persist_worker::PersistRequest,
    generation: u64,
) -> jefe::domain::effects::EffectCompletion {
    let correlation = request.correlation.clone();
    let revision = request.revision;
    // take_pending already cleared the pending slot, but a newer schedule
    // may have arrived between take_pending and the worker's offload.
    // clear_pending_if only clears if the generation still matches,
    // preserving any newer snapshot (issue #301 review feedback).
    handle.clear_pending_if(generation);
    let freshness_handle = handle.clone();
    let result = smol::unblock(move || {
        let freshness = move |revision| freshness_handle.freshness(revision);
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            persist_fn(&request, generation, &freshness)
        })) {
            Ok(inner) => inner,
            Err(payload) => {
                let msg = payload
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                    .unwrap_or("unknown panic");
                Err(format!("persist_fn panicked: {msg}"))
            }
        }
    })
    .await;
    // The completion is routed back through the reducer so the durable
    // revision advances (or not) under the same correlation the save was
    // staged with; a stale correlation is ignored there.
    match result {
        Ok(jefe::services::persist_worker::PersistResult::Authoritative) => {
            if !handle.commit(generation) {
                debug!(
                    generation,
                    "persisted generation was superseded after replacement"
                );
            }
            persist_completion(
                correlation,
                Ok(jefe::domain::effects::PersistenceResponse::Persisted { revision }),
            )
        }
        Ok(jefe::services::persist_worker::PersistResult::Stale) => {
            debug!(
                generation,
                "stale persistence candidate was not made authoritative"
            );
            persist_completion(
                correlation,
                Ok(jefe::domain::effects::PersistenceResponse::Superseded { revision }),
            )
        }
        Err(e) => {
            warn!(error = %e, generation, "background persist failed; not committing generation");
            persist_completion(
                correlation,
                Err(jefe::domain::effects::EffectError::new(
                    jefe::domain::effects::EffectErrorKind::Io,
                    false,
                    &e,
                )),
            )
        }
    }
}

/// Build a typed persistence completion for the reducer.
fn persist_completion(
    correlation: jefe::domain::effects::Correlation,
    result: Result<jefe::domain::effects::PersistenceResponse, jefe::domain::effects::EffectError>,
) -> jefe::domain::effects::EffectCompletion {
    jefe::domain::effects::EffectCompletion {
        correlation,
        result: result.map(jefe::domain::effects::EffectResponse::Persistence),
    }
}

/// Commit a persistence completion, rejecting any effect it stages.
///
/// The persist worker owns no adapter, so a transition that stages further
/// effects here would silently drop them; routing through `commit_pure_site`
/// makes that a loud contract violation instead.
fn deliver_persist_completion(
    app_state: &mut crate::app_input::AppStateHandle,
    completion: jefe::domain::effects::EffectCompletion,
) {
    let mut state = app_state.write();
    jefe::state::transition::commit_pure_site(
        &mut state,
        jefe::messages::AppMessage::EffectCompletion(Box::new(completion)),
    );
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
    let Some(ctx_arc) = ctx.as_ref() else {
        return;
    };
    loop {
        smol::Timer::after(Duration::from_millis(CAPTURE_POLL_MS)).await;
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
        let current_session_name = current_agent.and_then(|a| {
            ctx_guard
                .runtime
                .get_session(a)
                .map(|s| s.session_name.as_str())
        });
        let is_current = should_store_result(
            &agent_id,
            &request.session_name,
            generation,
            current_agent,
            current_session_name,
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

/// Resolve history lines from exact-generation cache, falling back to any
/// generation for the same agent. Pure helper so render/selection callers and
/// unit tests share one policy (avoids flashing empty scrollback on a cold
/// exact miss while a prior capture still exists).
#[must_use]
pub fn resolve_cached_history_lines(
    exact: Option<&[String]>,
    fallback: Option<&[String]>,
) -> Vec<String> {
    exact
        .or(fallback)
        .map(<[String]>::to_vec)
        .unwrap_or_default()
}

/// Under `AppContext` lock contention, keep the last successfully read
/// scrollback instead of returning an empty vec (which flashes the pane and
/// can corrupt mouse selection copy mid-frame).
#[must_use]
pub fn history_lines_under_contention(last_good: Option<&[String]>) -> Vec<String> {
    last_good.map(<[String]>::to_vec).unwrap_or_default()
}

fn last_history_under_contention() -> Vec<String> {
    LAST_HISTORY_LINES.with(|cell| {
        history_lines_under_contention(cell.borrow().as_ref().map(|(_, _, lines)| lines.as_ref()))
    })
}

fn store_last_history_if_changed(agent: &AgentId, generation: u64, lines: &[String]) {
    LAST_HISTORY_LINES.with(|cell| {
        let mut slot = cell.borrow_mut();
        let refresh = match slot.as_ref() {
            Some((aid, cached_generation, _)) => aid != agent || *cached_generation != generation,
            None => true,
        };
        if refresh {
            *slot = Some((agent.clone(), generation, Arc::<[String]>::from(lines)));
        }
    });
}

fn request_capture_if_needed(
    handle: &CaptureHandle,
    attached_agent: &AgentId,
    session_name: String,
    generation: u64,
) {
    let need_request = LAST_CAPTURE_REQUEST.with(|cell| {
        let prev = cell.borrow();
        let changed = prev
            .as_ref()
            .is_some_and(|(a, g)| a != attached_agent || *g != generation)
            || prev.is_none();
        drop(prev);
        if changed {
            *cell.borrow_mut() = Some((attached_agent.clone(), generation));
        }
        changed
    });
    if need_request {
        handle.request(attached_agent.clone(), session_name, generation);
    }
}

/// Read history lines from the runtime cache (issue #301 Phase 2).
///
/// The render path calls this instead of `capture_history` (which shells out
/// to `tmux capture-pane` synchronously). This function:
/// 1. Requests a background capture via the `CaptureHandle` (cheap, no I/O).
/// 2. Reads the runtime's `HistoryCache` directly (non-blocking).
///
/// Contended `try_lock` and exact-generation cache misses preserve prior lines
/// (matching [`try_capture_history_geometry_from_cache`]'s fallback policy).
/// Last-good scrollback is shared via `Arc` and refreshed only when
/// `(agent_id, generation)` changes.
#[must_use]
pub fn capture_history_from_cache(ctx: Option<&Arc<std::sync::Mutex<AppContext>>>) -> Vec<String> {
    let Some(ctx_arc) = ctx else {
        clear_last_history_lines();
        return Vec::new();
    };
    let Ok(ctx_guard) = ctx_arc.try_lock() else {
        tracing::trace!(
            "capture_history_from_cache: ctx try_lock contended; preserving last-good scrollback"
        );
        return last_history_under_contention();
    };
    let Some(agent_id) = ctx_guard.runtime.attached_agent() else {
        clear_last_history_lines();
        return Vec::new();
    };
    let Some(session) = ctx_guard.runtime.get_session(agent_id) else {
        clear_last_history_lines();
        return Vec::new();
    };
    let attached_agent = agent_id.clone();
    let session_name = session.session_name.clone();
    let generation = ctx_guard.runtime.output_generation();
    request_capture_if_needed(
        &ctx_guard.capture_handle,
        &attached_agent,
        session_name,
        generation,
    );
    let lines = resolve_cached_history_lines(
        ctx_guard
            .runtime
            .history_cache_get(&attached_agent, generation)
            .map(Vec::as_slice),
        ctx_guard
            .runtime
            .history_cache_fallback(&attached_agent)
            .map(Vec::as_slice),
    );
    store_last_history_if_changed(&attached_agent, generation, &lines);
    lines
}

fn clear_last_history_lines() {
    LAST_HISTORY_LINES.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

/// Try to read cached history geometry for the attached session without a
/// multiplexer subprocess. Contention, no attachment, and a cold cache all
/// return `None` so mouse routing preserves its prior geometry (issue #374 S3).
///
/// Unlike a cold exact-generation miss that used to zero history lines in
/// [`capture_history_from_cache`], this preserves prior geometry on a cold
/// miss: callers (mouse scroll/selection geometry) can return early instead
/// of zeroing `history_count`, which would clear the scroll offset and jump
/// to follow-tail during attach. (`capture_history_from_cache` now also
/// falls back and preserves last-good under contention.)
#[must_use]
pub fn try_capture_history_geometry_from_cache(
    ctx: Option<&Arc<std::sync::Mutex<AppContext>>>,
) -> Option<(usize, usize)> {
    let ctx_arc = ctx?;
    let ctx_guard = ctx_arc.try_lock().ok()?;
    let attached_agent = ctx_guard.runtime.attached_agent()?;
    let session = ctx_guard.runtime.get_session(attached_agent)?;
    let generation = ctx_guard.runtime.output_generation();
    let handle: &CaptureHandle = &ctx_guard.capture_handle;
    let need_request = LAST_CAPTURE_REQUEST.with(|cell| {
        let prev = cell.borrow();
        let changed = prev
            .as_ref()
            .is_some_and(|(a, g)| a != attached_agent || *g != generation)
            || prev.is_none();
        drop(prev);
        if changed {
            *cell.borrow_mut() = Some((attached_agent.clone(), generation));
        }
        changed
    });
    if need_request {
        handle.request(
            attached_agent.clone(),
            session.session_name.clone(),
            generation,
        );
    }
    let history_count = ctx_guard
        .runtime
        .history_cache_get(attached_agent, generation)
        .or_else(|| ctx_guard.runtime.history_cache_fallback(attached_agent))?
        .len();
    let live_rows = ctx_guard.runtime.snapshot()?.rows;
    drop(ctx_guard);
    Some((history_count, live_rows))
}

thread_local! {
    /// Cache of the last (agent_id, generation) requested by
    /// `capture_history_from_cache` to avoid redundant `CaptureHandle::request`
    /// calls on every render frame (issue #301 review feedback).
    static LAST_CAPTURE_REQUEST: std::cell::RefCell<Option<(AgentId, u64)>> =
        const { std::cell::RefCell::new(None) };

    /// Last successfully resolved scrollback for an attached `(agent, generation)`.
    /// Refreshed only when that pair changes so steady-state frames avoid a
    /// second full-history clone; used when `try_lock` is contended.
    static LAST_HISTORY_LINES: std::cell::RefCell<Option<(AgentId, u64, Arc<[String]>)>> =
        const { std::cell::RefCell::new(None) };
}

/// Synchronously flush the persist worker's pending snapshot.
///
/// Called from the shutdown path so the final state is durable before exit.
pub fn shutdown_flush_persist(ctx: Option<&Arc<std::sync::Mutex<AppContext>>>) {
    let Some(ctx_arc) = ctx else {
        return;
    };
    let Ok(ctx_guard) = ctx_arc.lock() else {
        warn!("shutdown_flush_persist: ctx mutex poisoned; skipping final persist");
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
        warn!("shutdown_flush_capture: ctx mutex poisoned; skipping capture drain");
        return;
    };
    // Take and discard the pending request — the cache already holds the
    // last good snapshot, and a synchronous capture on shutdown would block
    // the exit path.
    let _ = ctx_guard.capture_handle.take_pending();
}

#[cfg(test)]
mod history_cache_resolve_tests {
    use super::{history_lines_under_contention, resolve_cached_history_lines};

    #[test]
    fn resolve_prefers_exact_generation_over_fallback() {
        let exact = vec!["exact".to_string()];
        let fallback = vec!["fallback".to_string()];
        assert_eq!(
            resolve_cached_history_lines(Some(exact.as_slice()), Some(fallback.as_slice())),
            exact
        );
    }

    #[test]
    fn resolve_uses_fallback_when_exact_missing() {
        let fallback = vec!["keep".to_string(), "scrollback".to_string()];
        assert_eq!(
            resolve_cached_history_lines(None, Some(fallback.as_slice())),
            fallback
        );
    }

    #[test]
    fn resolve_empty_when_both_missing() {
        assert!(resolve_cached_history_lines(None, None).is_empty());
    }

    #[test]
    fn contention_preserves_last_good_lines() {
        let last = vec!["prior".to_string()];
        assert_eq!(history_lines_under_contention(Some(last.as_slice())), last);
    }

    #[test]
    fn contention_empty_when_no_prior() {
        assert!(history_lines_under_contention(None).is_empty());
    }
}
