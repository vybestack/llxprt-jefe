//! Agent kill / restart dispatch — extracted from `mod.rs` to keep
//! that file under the 1000-line source-file-size hard limit.
//!
//! Kill flows follow the bounded-transition contract (issue #381): the
//! reducer commits the `KillAgent` state change and stages a typed
//! `RuntimeEffect::KillSession`; only after every state guard is released
//! does the root executor run the effect and route its typed completion back
//! through the reducer.
//!
//! All functions are `pub(super)` so the parent `app_input` module can call
//! them from [`dispatch_app_message`] without exposing them outside the
//! crate boundary.

use std::time::Duration;

use jefe::domain::AgentId;
use jefe::domain::effects::IssuedEffect;
use jefe::messages::AppMessage;
use jefe::services::effect_executor::run_effects;
use jefe::services::runtime_effect_adapter::RuntimeEffectAdapter;
use jefe::state::transition;
use tracing::warn;

use super::availability;
use super::{
    AppEvent, AppStateHandle, SharedContext, agent_and_signature, durable_save_request,
    schedule_durable_save,
};

pub(super) fn dispatch_kill_agent(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
) {
    let effects = commit_kill_and_persist(app_state, ctx, agent_id);
    execute_runtime_effects(app_state, ctx, effects);
}

/// Commit the kill transition, persist the committed state, and return the
/// staged effects. All state guards are released before this returns.
fn commit_kill_and_persist(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
) -> Vec<IssuedEffect> {
    let mut state = app_state.write();
    let effects = transition::commit_in_place(&mut state, (AppEvent::KillAgent(agent_id)).into());
    state.terminal_focused = false;
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(ctx, persisted);
    effects
}

/// Execute committed effects through the root runtime adapter, routing each
/// typed completion back through the reducer. Returns `true` when every
/// delivered completion succeeded.
///
/// The state guard is only re-borrowed inside completion delivery — never
/// while the adapter is executing.
pub(super) fn execute_runtime_effects(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    effects: Vec<IssuedEffect>,
) -> bool {
    if effects.is_empty() {
        return true;
    }
    let Some(ctx_arc) = ctx else {
        let mut state = app_state.write();
        transition::reject_unexecuted_effects(&mut state, effects);
        return false;
    };
    match ctx_arc.lock() {
        Ok(mut ctx_guard) => {
            let mut adapter = RuntimeEffectAdapter {
                runtime: &mut ctx_guard.runtime,
            };
            let mut any_failed = false;
            run_effects(effects, &mut adapter, |completion| {
                any_failed |= completion.error().is_some();
                let mut state = app_state.write();
                transition::commit_in_place(
                    &mut state,
                    AppMessage::EffectCompletion(Box::new(completion)),
                )
            });
            !any_failed
        }
        Err(error) => {
            let mut state = app_state.write();
            state.error_message = Some(format!("application context lock poisoned: {error}"));
            false
        }
    }
}

pub(super) fn persist_error_message(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    error: String,
) {
    let mut state = app_state.write();
    state.error_message = Some(error);
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(ctx, persisted);
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
        let effects = commit_kill_and_persist(app_state, ctx, agent_id.clone());
        if !execute_runtime_effects(app_state, ctx, effects) {
            warn!(agent_id = %agent_id.0, "restart: kill effect failed");
            return;
        }

        // Wait for session teardown before relaunching (issue says 1-2s).
        std::thread::sleep(Duration::from_millis(1500));
    }

    // Relaunch with fresh config (reuses existing relaunch plumbing).
    super::relaunch::dispatch_relaunch_agent(app_state, ctx, agent_id);
}
