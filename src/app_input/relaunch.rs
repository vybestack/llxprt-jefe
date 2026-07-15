//! Relaunch orchestration and deterministic app-state persistence.

use std::path::Path;

use jefe::domain::{AgentId, LaunchSignature, ProcessIdentity};
use jefe::runtime::{RuntimeError, RuntimeManager, sandbox_ssh_agent_warning};
use jefe::state::{AppEvent, AppState, PaneFocus};
use tracing::warn;

use super::agent_runtime::{
    clear_agent_runtime_attachment, clear_runtime_warning, mark_agent_runtime_attached,
    mark_runtime_session_dead_if_present, process_on_success, set_agent_runtime_binding,
};
use super::{
    AppStateHandle, REMOTE_ATTACH_SETTLE_DELAY, SharedContext, agent_and_signature, availability,
    persist_state, preflight_or_prompt, to_persisted_state,
};

pub(super) fn dispatch_relaunch_agent(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
) {
    if !relaunch_preflight_passed(app_state, ctx, &agent_id) {
        return;
    }

    let result = relaunch_runtime_session(app_state, ctx, &agent_id);
    if let Err(error) = &result {
        warn!(agent_id = %agent_id.0, error = %error, "could not relaunch runtime session");
    }
    persist_relaunch_result(app_state, ctx, agent_id, result);
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
    if !availability::launch_available_or_error(
        app_state,
        signature.agent_kind,
        signature.llxprt_version.as_ref(),
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
) -> Result<(), RuntimeError> {
    let ctx_arc = ctx.as_ref().ok_or_else(|| {
        RuntimeError::SpawnFailed("runtime context unavailable during relaunch".to_owned())
    })?;
    let mut ctx_guard = ctx_arc.lock().map_err(|_| {
        RuntimeError::SpawnFailed("runtime context lock unavailable during relaunch".to_owned())
    })?;

    let state_ro = app_state.read();
    let (agent, signature) = agent_and_signature(&state_ro, agent_id)
        .ok_or_else(|| RuntimeError::SessionNotFound(agent_id.0.clone()))?;
    drop(state_ro);

    spawn_relaunch_session(
        &mut ctx_guard.runtime,
        agent_id,
        &agent.work_dir,
        &signature,
    )?;
    std::thread::sleep(REMOTE_ATTACH_SETTLE_DELAY);
    if let Err(error) = attach_relaunched_session(&mut ctx_guard.runtime, agent_id) {
        let _ = ctx_guard.runtime.mark_session_dead(agent_id);
        drop(ctx_guard);
        return Err(error);
    }
    drop(ctx_guard);
    Ok(())
}

pub(super) fn spawn_relaunch_session<R: RuntimeManager>(
    runtime: &mut R,
    agent_id: &AgentId,
    work_dir: &Path,
    signature: &LaunchSignature,
) -> Result<(), RuntimeError> {
    match runtime.spawn_session_fresh(agent_id, work_dir, signature) {
        Ok(()) => Ok(()),
        Err(RuntimeError::AlreadyRunning(_)) => runtime.relaunch(agent_id),
        Err(error) => Err(error),
    }
}

pub(super) fn attach_relaunched_session<R: RuntimeManager>(
    runtime: &mut R,
    agent_id: &AgentId,
) -> Result<(), RuntimeError> {
    runtime.attach(agent_id)
}

fn persist_relaunch_result(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    result: Result<(), RuntimeError>,
) {
    let relaunch_event = AppEvent::RelaunchAgent(agent_id.clone());
    let (pid, process_identity) = process_on_success(ctx, &agent_id, result.is_ok());
    let mut state = app_state.write();
    match result {
        Ok(()) => {
            persist_relaunch_success(&mut state, &agent_id, relaunch_event, pid, process_identity);
        }
        Err(error) => persist_relaunch_failure(&mut state, &agent_id, relaunch_event, &error),
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
    process_identity: Option<ProcessIdentity>,
) {
    let agent_sig = agent_and_signature(state, agent_id);
    let relaunch_kind = agent_sig
        .as_ref()
        .map(|(_, signature)| signature.agent_kind);
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
    if relaunch_kind == Some(jefe::domain::AgentKind::Llxprt) {
        if let Some(warning) = sandbox_ssh_agent_warning() {
            state.warning_message = Some(warning);
        } else {
            clear_runtime_warning(state);
        }
    }
}

pub(super) fn persist_relaunch_failure(
    state: &mut AppState,
    agent_id: &AgentId,
    relaunch_event: AppEvent,
    error: &RuntimeError,
) {
    *state = std::mem::take(state).apply(relaunch_event);
    state.terminal_focused = false;
    state.pane_focus = PaneFocus::Agents;
    state.error_message = Some(error.to_string());
    mark_runtime_session_dead_if_present(state, agent_id);
    if let Some(agent) = state.agents.iter_mut().find(|agent| &agent.id == agent_id) {
        agent.runtime_binding = None;
    }
}
