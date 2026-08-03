//! Background attach helper extracted from `app_shell` (issue #301 Phase 3).
//!
//! [`perform_async_attach`] snapshots `AttachInputs` under a short lock,
//! builds the `AttachedViewer` on the calling background thread, then
//! reacquires the lock to install the result — never holding `AppContext`
//! across the external spawn.
//!
//! **Caller contract:** must be invoked from a background thread (e.g. via
//! `smol::unblock`), not from the input/render hot path.

use std::sync::Arc;

use tracing::{debug, warn};

use jefe::domain::AgentId;
use jefe::runtime::{
    AttachedViewer, RuntimeManager, TmuxRuntimeManager, drop_viewer_in_background_pub,
};

use crate::AppContext;

/// Outcome of a background attach/detach operation.
pub enum AsyncAttachOutcome {
    Attached(AgentId),
    Detached,
    Failed(AgentId),
}

/// Perform attach/detach on a background thread (via `smol::unblock`).
///
/// Issue #301 Phase 3: this no longer holds the `AppContext` mutex for the
/// entire attach duration. Instead it:
///
/// 1. Locks briefly to snapshot `AttachInputs` (session name, remote, dims).
/// 2. Releases the lock.
/// 3. Builds the `AttachedViewer` on the background thread.
/// 4. Reacquires the lock.
/// 5. Validates the desired target still matches (stale guard).
/// 6. Calls `apply_attach_result` to install the viewer under a short lock.
pub fn perform_async_attach(
    ctx: Arc<std::sync::Mutex<AppContext>>,
    target: Option<AgentId>,
) -> AsyncAttachOutcome {
    let Some(agent_id) = target else {
        return perform_async_detach(&ctx);
    };

    let inputs = {
        let Ok(ctx_guard) = ctx.lock() else {
            warn!(agent_id = %agent_id.0, "background: ctx mutex poisoned during attach input snapshot");
            return AsyncAttachOutcome::Failed(agent_id);
        };
        ctx_guard.runtime.attach_inputs(&agent_id)
    };

    let Some(inputs) = inputs else {
        return AsyncAttachOutcome::Failed(agent_id);
    };

    let viewer = match TmuxRuntimeManager::build_viewer(inputs) {
        Ok(v) => v,
        Err(error) => {
            // Building a viewer is jefe-side work. It can fail for reasons the
            // agent knows nothing about, so it is not evidence the session or
            // worker died, and the binding is left for liveness to judge
            // (issue #306).
            warn!(
                agent_id = %agent_id.0,
                error = %error,
                "background: build_viewer failed; leaving the binding for liveness to judge"
            );
            return AsyncAttachOutcome::Failed(agent_id);
        }
    };

    let Ok(mut ctx_guard) = ctx.lock() else {
        drop_viewer_in_background(viewer);
        return AsyncAttachOutcome::Failed(agent_id);
    };

    // Stale-attach guard: if the agent's session was removed (e.g. by a
    // kill or restart) while the viewer was being built, reject the
    // result and dispose of the viewer on a background thread.
    // Note: `apply_attach_result` also validates session existence, but
    // checking here avoids calling `apply_attach_result` at all when the
    // session is already gone, keeping the rejection path explicit and
    // the error message more specific.
    if ctx_guard.runtime.get_session(&agent_id).is_none() {
        debug!(agent_id = %agent_id.0, "background: agent session gone after viewer built; stale attach rejected");
        drop_viewer_in_background(viewer);
        return AsyncAttachOutcome::Failed(agent_id);
    }

    match ctx_guard.runtime.apply_attach_result(&agent_id, viewer) {
        Ok(()) => AsyncAttachOutcome::Attached(agent_id),
        Err(error) => {
            // Attaching is a viewer operation. Failing it says nothing about
            // the session or the worker, both of which may be running
            // perfectly well, so the binding is preserved and the agent is
            // left as it was (issue #306).
            //
            // Marking it dead here removed the runtime mapping while the
            // session and worker kept running, which is how a live agent
            // became untracked: the row said Dead, nothing owned the tree, and
            // nothing reaped it either.
            //
            // Liveness is the only thing entitled to conclude death, and it
            // re-probes every `LIVENESS_POLL_INTERVAL`, so a session that
            // really has gone is still resolved -- just by evidence rather
            // than by a failed attach.
            warn!(
                agent_id = %agent_id.0,
                error = %error,
                "background: apply_attach_result failed; leaving the binding for liveness to judge"
            );
            AsyncAttachOutcome::Failed(agent_id)
        }
    }
}

/// Detach with no running agent selected.
fn perform_async_detach(ctx: &Arc<std::sync::Mutex<AppContext>>) -> AsyncAttachOutcome {
    let Ok(mut ctx_guard) = ctx.lock() else {
        warn!("background: ctx mutex poisoned during detach; returning Detached as best-effort");
        return AsyncAttachOutcome::Detached;
    };
    debug!("background: detaching (no running agent selected)");
    if let Err(e) = ctx_guard.runtime.detach() {
        warn!(error = %e, "background: detach failed");
    }
    AsyncAttachOutcome::Detached
}

/// Drop an `AttachedViewer` on a background thread to avoid blocking during
/// `AttachedViewer::drop` child teardown (~300ms).
///
/// Delegates to the canonical `drop_viewer_in_background_pub` in
/// `runtime/manager.rs` so the background-drop policy stays centralized
/// (issue #301 review feedback).
fn drop_viewer_in_background(viewer: AttachedViewer) {
    let mut opt = Some(viewer);
    drop_viewer_in_background_pub(&mut opt);
}
