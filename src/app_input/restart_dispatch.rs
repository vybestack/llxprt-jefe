//! Restart / relaunch dispatch for agent sessions (issue #117, #269).
//!
//! Extracted from `mod.rs` to keep that file under the per-file line limit.
//! `dispatch_restart_agent` validates preflight FIRST (selector validation,
//! availability, prepared-launch preflight), then delegates the destructive
//! kill + teardown + spawn to the runtime manager. This ordering ensures an
//! invalid selector or unavailable target causes NO destruction.
//!
//! The prepared-launch preflight ([`PreparedLaunch::prepare`]) resolves all
//! non-destructive prerequisites — including the remote npm probe — before
//! the kill, and the prepared data is reused for the post-kill spawn so no
//! re-resolution occurs. The exact [`RuntimeError`] from preflight is
//! surfaced to UI/persistence rather than a bool/log-only.
//!
//! Single-kill restart transaction (issue #269): the app dispatch prepares
//! ONCE and passes the [`PreparedLaunch`] into the runtime manager's
//! `spawn_prepared_session_fresh`, which owns the SOLE kill, applies the
//! teardown delay, then executes the SAME prepared data. The app dispatch
//! does NOT call `kill_runtime_agent`, does NOT sleep, and does NOT reprepare
//! — eliminating the double-kill / double-probe hazard.
//!
//! Typed relaunch outcome (blocker A remediation): the app dispatch uses
//! [`RelaunchOutcome`] — not a boolean — to distinguish a non-destructive
//! kill/preparation failure (old session may still be alive; retain Running
//! and binding) from a destructive spawn/attach failure (old session is gone;
//! mark Dead and clear binding). The classification is driven by the
//! [`RuntimeError`] variant returned by the runtime manager — no string
//! parsing.

use jefe::domain::{AgentId, LaunchSignature};
use jefe::runtime::PreparedLaunch;
use jefe::runtime::{RuntimeError, RuntimeManager, RuntimeSession};
use jefe::state::{AppEvent, AppState, PaneFocus};
use tracing::warn;

use std::path::PathBuf;

use super::agent_runtime::{
    clear_agent_runtime_attachment, clear_runtime_warning, mark_agent_runtime_attached,
    mark_runtime_session_dead_if_present, set_agent_runtime_binding,
};
use super::{
    AppStateHandle, REMOTE_ATTACH_SETTLE_DELAY, SharedContext, agent_and_signature, availability,
    persist_error_message, persist_state, preflight_or_prompt, process_on_success,
    to_persisted_state,
};
use jefe::runtime::sandbox_ssh_agent_warning;

/// Typed outcome of a relaunch/restart transaction.
///
/// Distinguishes destructive from non-destructive failures so the app state
/// persists the correct binding/liveness state without boolean reduction or
/// string parsing. The classification is driven by the [`RuntimeError`]
/// variant returned by the runtime manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelaunchOutcome {
    /// The kill → spawn → attach transaction completed; the agent is Running.
    Success,
    /// A non-destructive failure (preflight, kill, or preparation) occurred
    /// before the old session was destroyed. The agent retains its existing
    /// Running status and runtime binding so the user can retry.
    NonDestructiveFailure,
    /// A destructive failure (spawn or attach) occurred after the old session
    /// was killed. The agent is marked Dead and its binding cleared.
    DestructiveFailure,
}

/// Classify a [`RuntimeError`] from the relaunch transaction into a
/// [`RelaunchOutcome`].
///
/// The runtime manager's prepared replacement transaction returns
/// [`RuntimeError::KillFailed`] when the kill phase fails (old session may
/// still be alive) and [`RuntimeError::SpawnFailed`] when the spawn phase
/// fails (old session is gone). Attach failures return
/// [`RuntimeError::AttachFailed`]. Any other error after the kill phase is
/// conservatively treated as destructive: the prepared transaction's kill
/// phase propagates only `KillFailed`, so a different error means the kill
/// succeeded but a later step failed.
#[must_use]
fn classify_relaunch_error(error: &RuntimeError) -> RelaunchOutcome {
    match error {
        RuntimeError::ReplacementFailed {
            phase: jefe::runtime::ReplacementFailurePhase::Spawn,
            ..
        }
        | RuntimeError::SpawnFailed(_)
        | RuntimeError::AttachFailed(_) => RelaunchOutcome::DestructiveFailure,
        // Errors outside the prepared replacement transaction do not prove
        // that an existing session was destroyed.
        _ => RelaunchOutcome::NonDestructiveFailure,
    }
}

/// Restart an agent: validate preflight FIRST, prepare the launch, then
/// delegate the single kill + teardown delay + spawn to the runtime manager
/// via `spawn_prepared_session_fresh` (issue #117, #269).
///
/// Preflight (selector validation, effective-target availability, and
/// prepared-launch preflight including the remote npm probe) MUST run before
/// the destructive kill so an invalid selector or unavailable runtime causes
/// no destruction. Surfaces the exact [`RuntimeError`] if any step fails.
///
/// The runtime manager is the SOLE kill owner. The app dispatch does not kill
/// or sleep. The `RelaunchAgent` transition applied after a successful
/// transaction supersedes the intermediate `KillAgent` transition (Running →
/// Running or Dead → Running).
pub(super) fn dispatch_restart_agent(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
) {
    // Validate selector + effective-target availability BEFORE kill so an
    // invalid selector or unavailable runtime causes no destruction.
    if !relaunch_preflight_passed(app_state, ctx, &agent_id) {
        return;
    }

    // Prepared-launch preflight: resolve and validate ALL non-destructive
    // launch prerequisites (multiplexer, local executable / remote npm probe
    // + command) BEFORE the destructive kill. The exact RuntimeError is
    // surfaced to UI/persistence rather than bool/log-only. Missing remote
    // npm causes no kill.
    let state_ro = app_state.read();
    let agent_sig = agent_and_signature(&state_ro, &agent_id);
    drop(state_ro);
    let Some((agent, signature)) = agent_sig else {
        return;
    };
    let session_name = RuntimeSession::session_name_for(&agent_id);
    let prepared = match PreparedLaunch::prepare(
        &session_name,
        &agent.work_dir,
        &signature,
        get_npm_executable(ctx).as_deref(),
    ) {
        Ok(prepared) => prepared,
        Err(error) => {
            persist_error_message(app_state, ctx, error.to_string());
            return;
        }
    };

    // The runtime manager is the SOLE kill owner: `spawn_prepared_session_fresh`
    // performs the single kill → 1.5s teardown delay → spawn transaction using
    // the prepared data. The app dispatch does NOT call kill_runtime_agent and
    // does NOT sleep. The RelaunchAgent transition applied after success
    // supersedes the intermediate KillAgent transition (Running → Running or
    // Dead → Running), so no separate kill state transition is needed.
    let outcome = relaunch_prepared_runtime_session(
        app_state,
        ctx,
        &agent_id,
        &agent.work_dir,
        &signature,
        &prepared,
    );
    persist_relaunch_result(app_state, ctx, agent_id, outcome);
}

pub(super) fn dispatch_relaunch_agent(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
) {
    if !relaunch_preflight_passed(app_state, ctx, &agent_id) {
        return;
    }

    let outcome = relaunch_runtime_session(app_state, ctx, &agent_id);
    persist_relaunch_result(app_state, ctx, agent_id, outcome);
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
    // Validate the version selector BEFORE the destructive kill so an invalid
    // selector causes no destruction (issue #269). This mirrors the same
    // validation that spawn_session_fresh runs — but here it runs earlier so
    // the restart kill is skipped for a structurally unrepresentable selector.
    if let Err(error) = jefe::domain::validate_version_selector(&signature.llxprt_version) {
        persist_error_message(app_state, ctx, error.to_string());
        return false;
    }
    if !availability::local_kind_available_or_error(
        app_state,
        signature.agent_kind,
        &signature.llxprt_version,
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
) -> RelaunchOutcome {
    let Some(ctx_arc) = ctx else {
        return RelaunchOutcome::NonDestructiveFailure;
    };
    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        return RelaunchOutcome::NonDestructiveFailure;
    };

    let state_ro = app_state.read();
    let Some((agent, signature)) = agent_and_signature(&state_ro, agent_id) else {
        return RelaunchOutcome::NonDestructiveFailure;
    };
    drop(state_ro);

    if let Err(error) = spawn_relaunch_session(
        &mut ctx_guard.runtime,
        agent_id,
        &agent.work_dir,
        &signature,
    ) {
        return classify_relaunch_error(&error);
    }
    std::thread::sleep(REMOTE_ATTACH_SETTLE_DELAY);
    match attach_relaunched_session(&mut ctx_guard.runtime, agent_id) {
        Ok(()) => RelaunchOutcome::Success,
        Err(()) => RelaunchOutcome::DestructiveFailure,
    }
}

/// Relaunch using the SAME prepared data: call `spawn_prepared_session_fresh`
/// so the runtime manager kills once and executes the prepared launch without
/// re-resolution, then attach.
fn relaunch_prepared_runtime_session(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    work_dir: &std::path::Path,
    signature: &LaunchSignature,
    prepared: &PreparedLaunch,
) -> RelaunchOutcome {
    let Some(ctx_arc) = ctx else {
        return RelaunchOutcome::NonDestructiveFailure;
    };
    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        return RelaunchOutcome::NonDestructiveFailure;
    };

    if let Err(error) = spawn_prepared_relaunch_session(
        &mut ctx_guard.runtime,
        agent_id,
        work_dir,
        signature,
        prepared,
    ) {
        return classify_relaunch_error(&error);
    }
    // Avoid holding the app-state read lock across the sleep + attach.
    let _ = app_state; // app_state already borrowed read-only above; no-op
    std::thread::sleep(REMOTE_ATTACH_SETTLE_DELAY);
    match attach_relaunched_session(&mut ctx_guard.runtime, agent_id) {
        Ok(()) => RelaunchOutcome::Success,
        Err(()) => RelaunchOutcome::DestructiveFailure,
    }
}

fn spawn_relaunch_session(
    runtime: &mut jefe::runtime::TmuxRuntimeManager,
    agent_id: &AgentId,
    work_dir: &std::path::Path,
    signature: &LaunchSignature,
) -> Result<(), RuntimeError> {
    match runtime.spawn_session_fresh(agent_id, work_dir, signature) {
        Ok(()) => Ok(()),
        Err(RuntimeError::AlreadyRunning(_)) => runtime.relaunch(agent_id),
        Err(error) => {
            warn!(
                agent_id = %agent_id.0,
                error = %error,
                "could not spawn fresh runtime session for relaunch"
            );
            Err(error)
        }
    }
}

/// Spawn using the prepared launch via `spawn_prepared_session_fresh`: the
/// runtime manager kills once and reuses the prepared data — no
/// re-resolution. Falls back to `relaunch` if the session is already running.
fn spawn_prepared_relaunch_session(
    runtime: &mut jefe::runtime::TmuxRuntimeManager,
    agent_id: &AgentId,
    work_dir: &std::path::Path,
    signature: &LaunchSignature,
    prepared: &PreparedLaunch,
) -> Result<(), RuntimeError> {
    match runtime.spawn_prepared_session_fresh(agent_id, work_dir, signature, prepared) {
        Ok(()) => Ok(()),
        Err(RuntimeError::AlreadyRunning(_)) => runtime.relaunch(agent_id),
        Err(error) => {
            warn!(
                agent_id = %agent_id.0,
                error = %error,
                "could not spawn prepared fresh runtime session for relaunch"
            );
            Err(error)
        }
    }
}

/// Attach to the relaunched session. An attach failure is destructive: the
/// old session was killed and the new session was spawned, so the agent is
/// marked Dead and the binding cleared by the caller via
/// [`RelaunchOutcome::DestructiveFailure`].
fn attach_relaunched_session(
    runtime: &mut jefe::runtime::TmuxRuntimeManager,
    agent_id: &AgentId,
) -> Result<(), ()> {
    match runtime.attach(agent_id) {
        Ok(()) => Ok(()),
        Err(error) => {
            warn!(agent_id = %agent_id.0, error = %error, "could not attach relaunched session");
            let _ = runtime.mark_session_dead(agent_id);
            Err(())
        }
    }
}

fn persist_relaunch_result(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    outcome: RelaunchOutcome,
) {
    let relaunch_event = AppEvent::RelaunchAgent(agent_id.clone());
    // Query the PID BEFORE taking the app-state write lock: worker_pid_for
    // acquires the ctx mutex, so app_state-lock → ctx-lock would be a
    // lock-ordering hazard. The PID/identity query is only meaningful on
    // success — failure paths do not persist a new binding.
    let (pid, process_identity) = if outcome == RelaunchOutcome::Success {
        process_on_success(ctx, &agent_id, true)
    } else {
        Default::default()
    };
    let mut state = app_state.write();
    match outcome {
        RelaunchOutcome::Success => {
            persist_relaunch_success(&mut state, &agent_id, relaunch_event, pid, process_identity);
        }
        RelaunchOutcome::NonDestructiveFailure => {
            persist_relaunch_non_destructive_failure(&mut state, &agent_id);
        }
        RelaunchOutcome::DestructiveFailure => {
            persist_relaunch_destructive_failure(&mut state, &agent_id);
        }
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

/// Non-destructive failure (kill/preparation): the old session may still be
/// alive. Retain the Running status and runtime binding so the user can retry
/// the restart. Only clear the attachment and focus.
fn persist_relaunch_non_destructive_failure(state: &mut AppState, _agent_id: &AgentId) {
    state.terminal_focused = false;
    state.pane_focus = PaneFocus::Agents;
    clear_agent_runtime_attachment(state);
}

/// Destructive failure (spawn/attach): the old session was killed and the new
/// session could not be established. Mark Dead and clear the binding so the
/// stale mapping does not mislead liveness checks. The dead relaunch
/// signature was already preserved in the runtime manager's
/// `dead_signatures` LRU, so the agent is relaunchable.
fn persist_relaunch_destructive_failure(state: &mut AppState, agent_id: &AgentId) {
    state.terminal_focused = false;
    state.pane_focus = PaneFocus::Agents;
    mark_runtime_session_dead_if_present(state, agent_id);
    if let Some(agent) = state.agents.iter_mut().find(|agent| &agent.id == agent_id) {
        agent.runtime_binding = None;
    }
}

/// Extract the session-cached npm executable path from the shared context, if
/// available. This is passed to `PreparedLaunch::prepare` so the prepared
/// launch resolves npm from the same cached path that the runtime manager
/// uses, avoiding drift between restart preflight and the post-kill spawn.
fn get_npm_executable(ctx: &SharedContext) -> Option<PathBuf> {
    let ctx_arc = ctx.as_ref()?;
    let ctx_guard = ctx_arc.lock().ok()?;
    ctx_guard.runtime.npm_executable_path()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jefe::runtime::RuntimeError;

    #[test]
    fn classify_kill_failed_is_non_destructive() {
        assert_eq!(
            classify_relaunch_error(&RuntimeError::KillFailed("boom".to_owned())),
            RelaunchOutcome::NonDestructiveFailure
        );
        classify_remote_execution_failed_is_non_destructive();
        classify_spawn_failed_is_destructive();
        classify_attach_failed_is_destructive();
        classify_other_error_is_non_destructive();
    }

    fn classify_remote_execution_failed_is_non_destructive() {
        assert_eq!(
            classify_relaunch_error(&RuntimeError::RemoteExecutionFailed("ssh".to_owned())),
            RelaunchOutcome::NonDestructiveFailure
        );
    }

    fn classify_spawn_failed_is_destructive() {
        assert_eq!(
            classify_relaunch_error(&RuntimeError::SpawnFailed("boom".to_owned())),
            RelaunchOutcome::DestructiveFailure
        );
    }

    fn classify_attach_failed_is_destructive() {
        assert_eq!(
            classify_relaunch_error(&RuntimeError::AttachFailed("gone".to_owned())),
            RelaunchOutcome::DestructiveFailure
        );
    }

    fn classify_other_error_is_non_destructive() {
        assert_eq!(
            classify_relaunch_error(&RuntimeError::NoAttachedViewer),
            RelaunchOutcome::NonDestructiveFailure
        );
    }
}
