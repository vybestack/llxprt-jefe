//! Agent kill / restart dispatch — extracted from `mod.rs` to keep
//! that file under the 1000-line source-file-size hard limit.
//!
//! All functions are `pub(super)` so the parent `app_input` module can call
//! them from [`dispatch_app_message`] without exposing them outside the
//! crate boundary.

use std::time::Duration;

use jefe::domain::AgentId;
use jefe::runtime::RuntimeManager;
use tracing::warn;

use super::availability;
use super::{
    AppEvent, AppStateHandle, SharedContext, agent_and_signature, persist_state, to_persisted_state,
};

pub(super) fn dispatch_kill_agent(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
) {
    if let Err(error) = kill_runtime_agent(ctx, &agent_id) {
        warn!(agent_id = %agent_id.0, error = %error, "could not kill runtime session");
        persist_error_message(app_state, ctx, error);
        return;
    }

    let mut state = app_state.write();
    *state = std::mem::take(&mut *state).apply(AppEvent::KillAgent(agent_id));
    state.terminal_focused = false;
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

pub(super) fn kill_runtime_agent(ctx: &SharedContext, agent_id: &AgentId) -> Result<(), String> {
    let Some(ctx_arc) = ctx else {
        return Ok(());
    };
    match ctx_arc.lock() {
        Ok(mut ctx_guard) => ctx_guard.runtime.kill(agent_id).map_err(|e| e.to_string()),
        Err(error) => Err(format!("application context lock poisoned: {error}")),
    }
}

pub(super) fn persist_error_message(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    error: String,
) {
    let mut state = app_state.write();
    state.error_message = Some(error);
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

/// Restart an agent: kill, wait for session teardown, then relaunch with fresh
/// config/env (issue #117). Surfaces an error if any step fails.
pub(super) fn dispatch_restart_agent(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
) {
    // Only kill if the agent is currently running; dead agents skip straight
    // to relaunch (tolerating Ctrl-r on already-dead agents).
    let state = app_state.read();
    let agent_is_running = state
        .agents
        .iter()
        .find(|a| a.id == agent_id)
        .is_some_and(jefe::domain::Agent::is_running);
    let signature = agent_and_signature(&state, &agent_id).map(|(_, signature)| signature);
    drop(state);

    if let Some(signature) = &signature {
        if !availability::launch_available_or_error(
            app_state,
            signature.agent_kind,
            signature.llxprt_version.as_ref(),
            &signature.code_puppy_version,
            &signature.remote,
        ) {
            return;
        }
        if let Err(error) = jefe::runtime::require_launch_package_available(signature) {
            persist_error_message(app_state, ctx, error.to_string());
            return;
        }
    }

    if agent_is_running {
        if let Err(error) = kill_runtime_agent(ctx, &agent_id) {
            warn!(agent_id = %agent_id.0, error = %error, "restart: kill failed");
            persist_error_message(app_state, ctx, error);
            return;
        }

        // Apply kill state transition so the UI reflects the kill immediately.
        {
            let mut state = app_state.write();
            *state = std::mem::take(&mut *state).apply(AppEvent::KillAgent(agent_id.clone()));
            state.terminal_focused = false;
            let persisted = to_persisted_state(&state);
            drop(state);
            persist_state(ctx, &persisted);
        }

        // Wait for session teardown before relaunching (issue says 1-2s).
        std::thread::sleep(Duration::from_millis(1500));
    }

    // Relaunch with fresh config (reuses existing relaunch plumbing).
    super::relaunch::dispatch_relaunch_agent(app_state, ctx, agent_id);
}
