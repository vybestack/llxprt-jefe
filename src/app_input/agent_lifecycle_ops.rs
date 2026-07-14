//! Agent kill / restart / relaunch dispatch — extracted from `mod.rs` to keep
//! that file under the 1000-line source-file-size hard limit.
//!
//! All functions are `pub(super)` so the parent `app_input` module can call
//! them from [`dispatch_app_message`] without exposing them outside the
//! crate boundary.

use std::time::Duration;

use jefe::domain::AgentId;
use jefe::runtime::{RuntimeError, RuntimeManager, sandbox_ssh_agent_warning};
use tracing::warn;

use super::agent_runtime::{
    clear_agent_runtime_attachment, clear_runtime_warning, mark_agent_runtime_attached,
    mark_runtime_session_dead_if_present, process_on_success, set_agent_runtime_binding,
};
use super::availability;
use super::preflight::preflight_or_prompt;
use super::{
    AppEvent, AppState, AppStateHandle, LaunchSignature, PaneFocus, REMOTE_ATTACH_SETTLE_DELAY,
    SharedContext, agent_and_signature, persist_state, to_persisted_state,
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
    let agent_is_running = app_state
        .read()
        .agents
        .iter()
        .find(|a| a.id == agent_id)
        .is_some_and(jefe::domain::Agent::is_running);

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
    dispatch_relaunch_agent(app_state, ctx, agent_id);
}

pub(super) fn dispatch_relaunch_agent(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
) {
    if !relaunch_preflight_passed(app_state, ctx, &agent_id) {
        return;
    }

    let relaunched = relaunch_runtime_session(app_state, ctx, &agent_id);
    persist_relaunch_result(app_state, ctx, agent_id, relaunched);
}

fn relaunch_preflight_passed(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
) -> bool {
    let state_ro = app_state.read();
    let agent_sig = agent_and_signature(&state_ro, agent_id);
    drop(state_ro);
    let Some((_, signature)) = agent_sig else {
        return true;
    };
    if !availability::local_kind_available_or_error(
        app_state,
        signature.agent_kind,
        &signature.remote,
    ) {
        return false;
    }
    preflight_or_prompt(app_state, ctx, agent_id, &signature, None)
}

fn relaunch_runtime_session(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
) -> bool {
    let Some(ctx_arc) = ctx else {
        return false;
    };
    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        return false;
    };

    let state_ro = app_state.read();
    let Some((agent, signature)) = agent_and_signature(&state_ro, agent_id) else {
        return false;
    };
    drop(state_ro);

    if !spawn_relaunch_session(
        &mut ctx_guard.runtime,
        agent_id,
        &agent.work_dir,
        &signature,
    ) {
        return false;
    }
    std::thread::sleep(REMOTE_ATTACH_SETTLE_DELAY);
    attach_relaunched_session(&mut ctx_guard.runtime, agent_id)
}

fn spawn_relaunch_session(
    runtime: &mut jefe::runtime::TmuxRuntimeManager,
    agent_id: &AgentId,
    work_dir: &std::path::Path,
    signature: &LaunchSignature,
) -> bool {
    match runtime.spawn_session_fresh(agent_id, work_dir, signature) {
        Ok(()) => true,
        Err(RuntimeError::AlreadyRunning(_)) => runtime.relaunch(agent_id).is_ok(),
        Err(error) => {
            warn!(
                agent_id = %agent_id.0,
                error = %error,
                "could not spawn fresh runtime session for relaunch"
            );
            false
        }
    }
}

fn attach_relaunched_session(
    runtime: &mut jefe::runtime::TmuxRuntimeManager,
    agent_id: &AgentId,
) -> bool {
    match runtime.attach(agent_id) {
        Ok(()) => true,
        Err(error) => {
            warn!(agent_id = %agent_id.0, error = %error, "could not attach relaunched session");
            let _ = runtime.mark_session_dead(agent_id);
            false
        }
    }
}

fn persist_relaunch_result(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    relaunched: bool,
) {
    let relaunch_event = AppEvent::RelaunchAgent(agent_id.clone());
    // Query the PID BEFORE taking the app-state write lock: worker_process_for
    // acquires the ctx mutex, so app_state-lock -> ctx-lock would be a
    // lock-ordering hazard. `process_on_success` skips the query on the failure
    // path (no binding is persisted).
    let (pid, process_identity) = process_on_success(ctx, &agent_id, relaunched);
    let mut state = app_state.write();
    if relaunched {
        persist_relaunch_success(&mut state, &agent_id, relaunch_event, pid, process_identity);
    } else {
        persist_relaunch_failure(&mut state, &agent_id, relaunch_event);
    }
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn persist_relaunch_success(
    state: &mut AppState,
    agent_id: &AgentId,
    relaunch_event: AppEvent,
    pid: Option<u32>,
    process_identity: Option<jefe::domain::ProcessIdentity>,
) {
    // Capture agent_kind before `apply` consumes the state snapshot, so the
    // SSH-agent warning can be gated: only LLxprt uses the sandbox subsystem,
    // and CodePuppy must not trigger it from stale persisted sandbox flags.
    let agent_sig = agent_and_signature(state, agent_id);
    let relaunch_kind = agent_sig.as_ref().map(|(_, sig)| sig.agent_kind);
    if let Some((agent, signature)) = agent_sig {
        set_agent_runtime_binding(
            state,
            agent_id,
            jefe::runtime::RuntimeSession::session_name_for(&agent.id),
            signature,
            pid,
            process_identity,
        );
    }
    *state = std::mem::take(state).apply(relaunch_event);
    state.terminal_focused = false;
    clear_agent_runtime_attachment(state);
    mark_agent_runtime_attached(state, agent_id, true);
    // Gate the SSH-agent warning to LLxprt only (see comment above).
    if relaunch_kind == Some(jefe::domain::AgentKind::Llxprt) {
        if let Some(warning) = sandbox_ssh_agent_warning() {
            state.warning_message = Some(warning);
        } else {
            clear_runtime_warning(state);
        }
    }
}

fn persist_relaunch_failure(state: &mut AppState, agent_id: &AgentId, relaunch_event: AppEvent) {
    *state = std::mem::take(state).apply(relaunch_event);
    state.terminal_focused = false;
    state.pane_focus = PaneFocus::Agents;
    mark_runtime_session_dead_if_present(state, agent_id);
    if let Some(agent) = state.agents.iter_mut().find(|agent| &agent.id == agent_id) {
        agent.runtime_binding = None;
    }
}
