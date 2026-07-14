//! Background attach helper extracted from `app_shell` (issue #301 Phase 3).
//!
//! [`perform_async_attach`] snapshots `AttachInputs` under a short lock,
//! builds the `AttachedViewer` on the calling background thread, then
//! reacquires the lock to install the result — never holding `AppContext`
//! across the external spawn.

use std::sync::Arc;

use tracing::{debug, warn};

use jefe::domain::AgentId;
use jefe::runtime::{RuntimeManager, TmuxRuntimeManager};

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
    if let Some(agent_id) = target {
        let inputs = {
            let Ok(ctx_guard) = ctx.lock() else {
                return AsyncAttachOutcome::Failed(agent_id);
            };
            ctx_guard.runtime.attach_inputs(&agent_id)
        };

        let Some(inputs) = inputs else {
            return AsyncAttachOutcome::Failed(agent_id);
        };

        let viewer = match TmuxRuntimeManager::build_viewer(&inputs) {
            Ok(v) => v,
            Err(error) => {
                warn!(
                    agent_id = %agent_id.0,
                    error = %error,
                    "background: build_viewer failed"
                );
                if let Ok(mut ctx_guard) = ctx.lock() {
                    let _ = ctx_guard.runtime.mark_session_dead(&agent_id);
                }
                return AsyncAttachOutcome::Failed(agent_id);
            }
        };

        let Ok(mut ctx_guard) = ctx.lock() else {
            std::thread::spawn(move || drop(viewer));
            return AsyncAttachOutcome::Failed(agent_id);
        };

        if ctx_guard.runtime.get_session(&agent_id).is_none() {
            debug!(
                agent_id = %agent_id.0,
                "background: agent session gone after viewer built; stale attach rejected"
            );
            std::thread::spawn(move || drop(viewer));
            return AsyncAttachOutcome::Failed(agent_id);
        }

        match ctx_guard.runtime.apply_attach_result(&agent_id, viewer) {
            Ok(()) => AsyncAttachOutcome::Attached(agent_id),
            Err(error) => {
                warn!(
                    agent_id = %agent_id.0,
                    error = %error,
                    "background: apply_attach_result failed"
                );
                let _ = ctx_guard.runtime.mark_session_dead(&agent_id);
                AsyncAttachOutcome::Failed(agent_id)
            }
        }
    } else {
        let Ok(mut ctx_guard) = ctx.lock() else {
            return AsyncAttachOutcome::Detached;
        };
        debug!("background: detaching (no running agent selected)");
        let _ = ctx_guard.runtime.detach();
        AsyncAttachOutcome::Detached
    }
}
