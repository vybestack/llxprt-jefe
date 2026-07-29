//! Relaunch orchestration and deterministic app-state persistence.

use std::path::Path;

use jefe::domain::{AgentId, AgentStatus, LaunchSignature, ProcessIdentity};
use jefe::runtime::{RuntimeError, RuntimeManager, sandbox_ssh_agent_warning};
use jefe::state::{AppEvent, AppState, ConfirmFocus, ModalState, PaneFocus};
use tracing::warn;

use super::agent_runtime::{
    clear_agent_runtime_attachment, clear_runtime_warning, mark_agent_runtime_attached,
    mark_runtime_session_dead_if_present, process_on_success, set_agent_runtime_binding,
};
use super::{
    AppStateHandle, REMOTE_ATTACH_SETTLE_DELAY, SharedContext, agent_and_signature, availability,
    durable_save_request, preflight_or_prompt, schedule_durable_save,
};

pub(super) struct ServerLostRecoveryOutcome {
    pub agent_id: AgentId,
    pub result: Result<(), RuntimeError>,
    pub pid: Option<u32>,
    pub process_identity: Option<ProcessIdentity>,
}

pub(super) fn dispatch_relaunch_agent(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
) {
    if open_server_lost_recovery_if_selected(app_state, &agent_id) {
        return;
    }
    if !relaunch_preflight_passed(app_state, ctx, &agent_id) {
        return;
    }

    let result = relaunch_runtime_session(app_state, ctx, &agent_id);
    if let Err(error) = &result {
        warn!(agent_id = %agent_id.0, error = %error, "could not relaunch runtime session");
    }
    persist_relaunch_result(app_state, ctx, agent_id, result);
}

fn open_server_lost_recovery_if_selected(
    app_state: &mut AppStateHandle,
    selected_id: &AgentId,
) -> bool {
    open_server_lost_recovery(&mut app_state.write(), selected_id)
}

pub(super) fn open_server_lost_recovery(state: &mut AppState, selected_id: &AgentId) -> bool {
    let selected_is_lost = state
        .agents
        .iter()
        .any(|agent| &agent.id == selected_id && agent.status == AgentStatus::ServerLost);
    if !selected_is_lost {
        return false;
    }
    let agent_ids = recoverable_server_lost_ids(state, None);
    if agent_ids.is_empty() {
        state.warning_message = Some("No local Server Lost agents can be recovered.".to_owned());
    } else {
        state.modal = ModalState::ConfirmServerLostRecovery {
            agent_ids,
            confirm_focus: ConfirmFocus::Cancel,
        };
    }
    true
}

pub(super) fn recoverable_server_lost_ids(
    state: &AppState,
    requested: Option<&[AgentId]>,
) -> Vec<AgentId> {
    state
        .agents
        .iter()
        .filter(|agent| agent.status == AgentStatus::ServerLost)
        .filter(|agent| {
            agent.runtime_binding.as_ref().is_some_and(|binding| {
                !binding.launch_signature.remote.enabled
                    && requested.map_or(true, |ids| ids.contains(&agent.id))
            })
        })
        .map(|agent| agent.id.clone())
        .collect()
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
        &signature.code_puppy_version,
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

    // Relaunch guard (issue #332): if a validated orphan worker descendant is
    // still alive, spawning now would create a duplicate --continue worker.
    // Best-effort reap first; if a validated orphan survives, block relaunch
    // with a user-facing error instead of spawning.
    if let Some(binding) = agent.runtime_binding.as_ref()
        && relaunch_blocked_by_orphan(agent_id, &binding.worker_identities)
    {
        return Err(RuntimeError::OrphanBlocked(agent_id.clone()));
    }

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

/// Whether relaunch must be blocked because a validated orphan worker is still
/// alive (issue #332, AC14/AC15).
///
/// Attempts a best-effort reap of the recorded worker identities first; only if
/// a validated descendant survives the reap does this return `true`. An empty
/// identity list, or one where no anchor is validated-alive, returns `false`.
pub(super) fn relaunch_blocked_by_orphan(
    agent_id: &AgentId,
    worker_identities: &[jefe::domain::ProcessIdentity],
) -> bool {
    use jefe::runtime::{descendant_still_matches_anchor, reap_orphan_tree};
    if worker_identities.is_empty() {
        return false;
    }
    let any_alive = worker_identities
        .iter()
        .any(|identity| descendant_still_matches_anchor(*identity));
    if !any_alive {
        return false;
    }
    // Best-effort reap, then re-check: block only if a validated orphan
    // genuinely survived the reap attempt.
    let _ = reap_orphan_tree(worker_identities);
    let still_alive = worker_identities
        .iter()
        .any(|identity| descendant_still_matches_anchor(*identity));
    if still_alive {
        warn!(
            agent_id = %agent_id.0,
            "relaunch blocked: validated orphan worker still alive after reap attempt"
        );
    }
    still_alive
}

pub(super) fn dispatch_server_lost_recovery(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    requested: Vec<AgentId>,
) {
    let agent_ids = {
        let mut state = app_state.write();
        state.modal = ModalState::None;
        let ids = recoverable_server_lost_ids(&state, Some(&requested));
        drop(state);
        ids
    };
    if agent_ids.is_empty() {
        app_state.write().warning_message =
            Some("No Server Lost agents remain to recover.".to_owned());
        return;
    }

    let mut outcomes = Vec::with_capacity(agent_ids.len());
    for agent_id in agent_ids {
        let result = recover_server_lost_runtime(app_state, ctx, &agent_id);
        if let Err(error) = &result {
            warn!(agent_id = %agent_id.0, error = %error, "psmux server-loss recovery failed");
        }
        let (pid, process_identity) = process_on_success(ctx, &agent_id, result.is_ok());
        outcomes.push(ServerLostRecoveryOutcome {
            agent_id,
            result,
            pid,
            process_identity,
        });
    }
    persist_server_lost_recovery(app_state, ctx, outcomes);
}

fn recover_server_lost_runtime(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
) -> Result<(), RuntimeError> {
    let worker_identities = app_state
        .read()
        .agents
        .iter()
        .find(|agent| &agent.id == agent_id)
        .and_then(|agent| agent.runtime_binding.as_ref())
        .map(|binding| binding.worker_identities.clone())
        .ok_or_else(|| RuntimeError::SessionNotFound(agent_id.0.clone()))?;
    if relaunch_blocked_by_orphan(agent_id, &worker_identities) {
        return Err(RuntimeError::OrphanBlocked(agent_id.clone()));
    }

    let ctx_arc = ctx.as_ref().ok_or_else(|| {
        RuntimeError::SpawnFailed("runtime context unavailable during recovery".to_owned())
    })?;
    let mut ctx_guard = ctx_arc.lock().map_err(|_| {
        RuntimeError::SpawnFailed("runtime context lock unavailable during recovery".to_owned())
    })?;
    // The ServerLost state deliberately preserves the manager's live record.
    // Move that exact signature to its retained cache before recreating it.
    let _ = ctx_guard.runtime.mark_session_dead(agent_id);
    ctx_guard.runtime.relaunch(agent_id)?;
    std::thread::sleep(REMOTE_ATTACH_SETTLE_DELAY);
    if let Err(error) = ctx_guard.runtime.attach(agent_id) {
        let _ = ctx_guard.runtime.mark_session_dead(agent_id);
        drop(ctx_guard);
        return Err(error);
    }
    drop(ctx_guard);
    Ok(())
}

fn persist_server_lost_recovery(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    outcomes: Vec<ServerLostRecoveryOutcome>,
) {
    let mut state = app_state.write();
    apply_server_lost_recovery_outcomes(&mut state, outcomes);
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(ctx, persisted);
}

pub(super) fn apply_server_lost_recovery_outcomes(
    state: &mut AppState,
    outcomes: Vec<ServerLostRecoveryOutcome>,
) {
    let mut successes = 0usize;
    let mut failures = 0usize;
    for outcome in outcomes {
        if outcome.result.is_ok() {
            persist_relaunch_success(
                state,
                &outcome.agent_id,
                AppEvent::RelaunchAgent(outcome.agent_id.clone()),
                outcome.pid,
                outcome.process_identity,
            );
            successes = successes.saturating_add(1);
        } else {
            if let Some(agent) = state
                .agents
                .iter_mut()
                .find(|agent| agent.id == outcome.agent_id)
                && let Some(binding) = agent.runtime_binding.as_mut()
            {
                binding.attached = false;
            }
            failures = failures.saturating_add(1);
        }
    }
    let summary = if failures == 0 {
        format!("Recovered {successes} psmux agent(s).")
    } else {
        format!("Recovered {successes} psmux agent(s); {failures} failed and remain Server Lost.")
    };
    let message = state
        .warning_message
        .take()
        .map_or_else(|| summary.clone(), |warning| format!("{summary} {warning}"));
    if failures == 0 {
        state.warning_message = Some(message);
        state.error_message = None;
    } else {
        state.error_message = Some(message);
    }
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
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(ctx, persisted);
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
    jefe::state::transition::commit_pure_site(state, (relaunch_event).into());
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
    jefe::state::transition::commit_pure_site(state, (relaunch_event).into());
    state.terminal_focused = false;
    state.pane_focus = PaneFocus::Agents;
    state.error_message = Some(error.to_string());
    mark_runtime_session_dead_if_present(state, agent_id);
    if let Some(agent) = state.agents.iter_mut().find(|agent| &agent.id == agent_id) {
        agent.runtime_binding = None;
    }
}
