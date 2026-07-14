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
    persist_error_message, persist_state, pid_on_success, preflight_or_prompt, to_persisted_state,
};
use jefe::runtime::sandbox_ssh_agent_warning;

/// Restart an agent: validate preflight FIRST, prepare the launch, then
/// delegate the single kill + teardown delay + spawn to the runtime manager
/// via `spawn_prepared_session_fresh` (issue #117, #269).
///
/// Preflight (selector validation, effective-target availability, and
/// prepared-launch preflight including the remote npm probe) MUST run before
/// the destructive kill so an invalid selector or unavailable target causes
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
    let relaunched = relaunch_prepared_runtime_session(
        app_state,
        ctx,
        &agent_id,
        &agent.work_dir,
        &signature,
        &prepared,
    );
    persist_relaunch_result(app_state, ctx, agent_id, relaunched);
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
) -> bool {
    let Some(ctx_arc) = ctx else {
        return false;
    };
    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        return false;
    };

    if !spawn_prepared_relaunch_session(
        &mut ctx_guard.runtime,
        agent_id,
        work_dir,
        signature,
        prepared,
    ) {
        return false;
    }
    // Avoid holding the app-state read lock across the sleep + attach.
    let _ = app_state; // app_state already borrowed read-only above; no-op
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

/// Spawn using the prepared launch via `spawn_prepared_session_fresh`: the
/// runtime manager kills once and reuses the prepared data — no
/// re-resolution. Falls back to `relaunch` if the session is already running.
fn spawn_prepared_relaunch_session(
    runtime: &mut jefe::runtime::TmuxRuntimeManager,
    agent_id: &AgentId,
    work_dir: &std::path::Path,
    signature: &LaunchSignature,
    prepared: &PreparedLaunch,
) -> bool {
    match runtime.spawn_prepared_session_fresh(agent_id, work_dir, signature, prepared) {
        Ok(()) => true,
        Err(RuntimeError::AlreadyRunning(_)) => runtime.relaunch(agent_id).is_ok(),
        Err(error) => {
            warn!(
                agent_id = %agent_id.0,
                error = %error,
                "could not spawn prepared fresh runtime session for relaunch"
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
    // Query the PID BEFORE taking the app-state write lock: worker_pid_for
    // acquires the ctx mutex, so app_state-lock → ctx-lock would be a
    // lock-ordering hazard. `pid_on_success` skips the query on the failure
    // path (no binding is persisted).
    let pid = pid_on_success(ctx, &agent_id, relaunched);
    let mut state = app_state.write();
    if relaunched {
        persist_relaunch_success(&mut state, &agent_id, relaunch_event, pid);
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

/// Extract the session-cached npm executable path from the shared context, if
/// available. This is passed to `PreparedLaunch::prepare` so the prepared
/// launch resolves npm from the same cached path that the runtime manager
/// uses, avoiding drift between restart preflight and the post-kill spawn.
fn get_npm_executable(ctx: &SharedContext) -> Option<PathBuf> {
    let ctx_arc = ctx.as_ref()?;
    let ctx_guard = ctx_arc.lock().ok()?;
    ctx_guard.runtime.npm_executable_path()
}
