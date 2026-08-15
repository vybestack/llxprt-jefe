//! Runtime-binding helper functions extracted from `mod.rs` to keep that file
//! under the per-file line limit.
//!
//! These helpers mutate agent runtime-binding state on `AppState` / query the
//! shared runtime context for worker PIDs. They are shared by the launch,
//! relaunch, kill, and issue/PR send paths in `app_input` and its child modules.

use jefe::domain::{
    AgentId, AgentStatus, LaunchSignatureV1, PaneProcessIdentity, WorkerProcessIdentity,
};
use jefe::state::AppState;

use super::SharedContext;

/// The process anchors a runtime binding carries, resolved together so no
/// caller can supply one role's evidence where another role is expected
/// (issue #543).
#[derive(Debug, Default, Clone)]
pub(super) struct BoundIdentities {
    /// The pane leader: a shell or session host, not the agent.
    pub(super) pane: Option<PaneProcessIdentity>,
    /// The agent process itself, when this platform can identify it.
    pub(super) worker: Option<WorkerProcessIdentity>,
    /// PID-reuse-safe descendant anchors used by the orphan reaper.
    pub(super) worker_identities: Vec<WorkerProcessIdentity>,
}

pub(super) fn set_agent_runtime_binding(
    state: &mut AppState,
    agent_id: &AgentId,
    session_name: String,
    signature: LaunchSignatureV1,
    identities: BoundIdentities,
) {
    if let Some(agent) = state.agents.iter_mut().find(|agent| &agent.id == agent_id) {
        agent.runtime_binding = Some(jefe::domain::RuntimeBinding {
            session_name,
            launch_signature: signature,
            attached: false,
            last_seen: None,
            pane_identity: identities.pane,
            worker_identity: identities.worker,
            lifecycle_generation: 0,
            // Carried across from the runtime rather than reset: dropping these
            // would erase the anchors the orphan reaper validates against and
            // silently disable the relaunch guard (issue #543 V8).
            worker_identities: identities.worker_identities,
        });
    }
}

pub(super) fn mark_agent_runtime_attached(
    state: &mut AppState,
    agent_id: &AgentId,
    attached: bool,
) {
    if let Some(agent) = state.agents.iter_mut().find(|agent| &agent.id == agent_id)
        && let Some(binding) = agent.runtime_binding.as_mut()
    {
        binding.attached = attached;
        if attached {
            agent.status = AgentStatus::Running;
        }
    }
}

pub(super) fn clear_agent_runtime_attachment(state: &mut AppState) {
    for agent in &mut state.agents {
        if let Some(binding) = agent.runtime_binding.as_mut() {
            binding.attached = false;
        }
    }
}

/// Query the runtime for an agent's process anchors via the shared context.
///
/// Returns empty anchors when the context is absent, the lock is poisoned, or
/// the runtime recorded nothing. Shared by the launch, relaunch, and issue/PR
/// send paths.
pub(super) fn bound_identities_for(ctx: &SharedContext, agent_id: &AgentId) -> BoundIdentities {
    let Some(guard) = ctx.as_ref().and_then(|arc| arc.lock().ok()) else {
        return BoundIdentities::default();
    };
    BoundIdentities {
        pane: guard.runtime.pane_process_identity(agent_id),
        worker: guard.runtime.worker_process_identity(agent_id),
        worker_identities: guard.runtime.worker_identities(agent_id),
    }
}

/// Resolve the process anchors to persist on a runtime binding, gated on launch
/// success.
///
/// All launch/relaunch persistence paths share the same invariant: the anchors
/// must be queried from the runtime **before** the caller takes the
/// `app_state` write lock, because `bound_identities_for` acquires the shared
/// context mutex and `app_state-lock → ctx-lock` would be a lock-ordering
/// hazard. Centralizing the success-gated query here guarantees that ordering
/// is respected at every call site. On the failure path no binding is
/// persisted, so the query is skipped.
pub(super) fn process_on_success(
    ctx: &SharedContext,
    agent_id: &AgentId,
    success: bool,
) -> BoundIdentities {
    if success {
        bound_identities_for(ctx, agent_id)
    } else {
        BoundIdentities::default()
    }
}

pub(super) fn mark_runtime_session_dead_if_present(state: &mut AppState, agent_id: &AgentId) {
    if let Some(agent) = state.agents.iter_mut().find(|agent| &agent.id == agent_id) {
        agent.status = AgentStatus::Dead;
        if let Some(binding) = agent.runtime_binding.as_mut() {
            binding.attached = false;
        }
    }
}
