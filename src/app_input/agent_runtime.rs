//! Runtime-binding helper functions extracted from `mod.rs` to keep that file
//! under the per-file line limit.
//!
//! These helpers mutate agent runtime-binding state on `AppState` / query the
//! shared runtime context for worker PIDs. They are shared by the launch,
//! relaunch, kill, and issue/PR send paths in `app_input` and its child modules.

use jefe::domain::{AgentId, AgentStatus, LaunchSignature};
use jefe::state::AppState;

use super::SharedContext;

pub(super) fn clear_runtime_warning(state: &mut AppState) {
    if state.warning_message.as_deref().is_some_and(|warning| {
        warning.contains("SSH_AUTH_SOCK") || warning.contains("SSH agent socket")
    }) {
        state.warning_message = None;
    }
}

pub(super) fn set_agent_runtime_binding(
    state: &mut AppState,
    agent_id: &AgentId,
    session_name: String,
    signature: LaunchSignature,
    pid: Option<u32>,
) {
    if let Some(agent) = state.agents.iter_mut().find(|agent| &agent.id == agent_id) {
        agent.runtime_binding = Some(jefe::domain::RuntimeBinding {
            session_name,
            launch_signature: signature,
            attached: false,
            last_seen: None,
            pid,
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
    }
}

pub(super) fn clear_agent_runtime_attachment(state: &mut AppState) {
    for agent in &mut state.agents {
        if let Some(binding) = agent.runtime_binding.as_mut() {
            binding.attached = false;
        }
    }
}

/// Query the runtime for an agent's worker PID (`llxprt` OS process) via the
/// shared context. Returns `None` when the context is absent, the lock is
/// poisoned, or the runtime has no PID recorded. Shared by the launch,
/// relaunch, and issue/PR send paths.
pub(super) fn worker_pid_for(ctx: &SharedContext, agent_id: &AgentId) -> Option<u32> {
    ctx.as_ref()
        .and_then(|arc| arc.lock().ok())
        .and_then(|guard| guard.runtime.worker_pid(agent_id))
}

pub(super) fn mark_runtime_session_dead_if_present(state: &mut AppState, agent_id: &AgentId) {
    if let Some(agent) = state.agents.iter_mut().find(|agent| &agent.id == agent_id) {
        agent.status = AgentStatus::Dead;
        if let Some(binding) = agent.runtime_binding.as_mut() {
            binding.attached = false;
        }
    }
}
