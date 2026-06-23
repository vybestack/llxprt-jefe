//! One-time application startup: state hydration and runtime session restore.

use iocraft::hooks::State as HookState;
use tracing::warn;

use jefe::domain::{
    Agent, AgentId, AgentStatus, LaunchSignature, PlatformCapabilities, RemoteRepositorySettings,
    SandboxEngine,
};
use jefe::persistence::{PersistenceManager, Settings, State as PersistedState};
use jefe::runtime::{
    RuntimeError, RuntimeManager, RuntimeSession, TmuxRuntimeManager, platform_engine_diagnostic,
};
use jefe::state::AppState;
use jefe::theme::ThemeManager;

use crate::app_input::{SharedContext, persist_state, to_persisted_state};

fn launch_signature_for_agent(agent: &Agent, remote: &RemoteRepositorySettings) -> LaunchSignature {
    LaunchSignature {
        work_dir: agent.work_dir.clone(),
        profile: agent.profile.clone(),
        mode_flags: agent.mode_flags.clone(),
        llxprt_debug: agent.llxprt_debug.clone(),
        pass_continue: agent.pass_continue,
        sandbox_enabled: agent.sandbox_enabled,
        sandbox_engine: agent.sandbox_engine,
        sandbox_flags: agent.sandbox_flags.clone(),
        remote: remote.clone(),
    }
}

fn append_warning(state: &mut AppState, warning: String) {
    state.warning_message = Some(match state.warning_message.take() {
        Some(existing) => format!("{existing} {warning}"),
        None => warning,
    });
}

fn normalize_persisted_sandbox_engines(state: &mut AppState) -> bool {
    let caps = PlatformCapabilities::current();
    let mut normalized_agent_count = 0usize;

    for agent in &mut state.agents {
        if !caps.is_engine_supported(agent.sandbox_engine) {
            warn!(
                agent = %agent.name,
                engine = agent.sandbox_engine.label(),
                platform = caps.platform_label(),
                "persisted sandbox engine not supported on this platform, normalizing to Podman"
            );
            agent.sandbox_engine = caps
                .normalize_engine(agent.sandbox_engine)
                .unwrap_or(SandboxEngine::Podman);
            normalized_agent_count += 1;
        }
    }

    if normalized_agent_count == 0 {
        return false;
    }

    append_warning(
        state,
        format!(
            "Normalized {normalized_agent_count} unsupported sandbox engine setting(s) to Podman for this platform."
        ),
    );
    true
}

/// Load persisted state and settings into `app_state` exactly once.
///
/// Reconciles any agents that were persisted as Running against actual live
/// tmux sessions, marking stale ones Dead.  Also activates the saved theme.
pub fn init_app_state(app_state: &mut HookState<AppState>, ctx: &SharedContext) {
    let Some(ctx_arc) = ctx else {
        return;
    };
    let Ok(ctx_guard) = ctx_arc.lock() else {
        return;
    };

    let settings = ctx_guard.persistence.load_settings().unwrap_or_else(|e| {
        warn!(error = %e, "could not load settings, using defaults");
        Settings::default_with_version()
    });

    let persisted = ctx_guard.persistence.load_state().unwrap_or_else(|e| {
        warn!(error = %e, "could not load state, using defaults");
        PersistedState::default_with_version()
    });

    let mut state = app_state.write();
    state.repositories = persisted.repositories;
    state.agents = persisted.agents;
    state.selected_repository_index = persisted.selected_repository_index;
    state.selected_agent_index = persisted.selected_agent_index;
    state.hide_idle_repositories = persisted.hide_idle_repositories;
    state.last_selected_agent_by_repo = persisted.last_selected_agent_by_repo;
    state.terminal_focused = false;
    state.rebuild_repository_agent_ids();
    state.normalize_selection_indices();

    // Log platform engine diagnostic at startup.
    tracing::info!("{}", platform_engine_diagnostic());

    // Normalize any persisted sandbox engines that are unsupported on this platform.
    let normalized_engines = normalize_persisted_sandbox_engines(&mut state);

    let dead_ids = reconcile_running_agents(&state, &ctx_guard.runtime);
    let should_persist = apply_dead_reconciliations(&mut state, dead_ids, normalized_engines);
    let state_to_persist = should_persist.then(|| to_persisted_state(&state));

    // Release state/context guards before reacquiring a mutable context lock
    // for persistence writes and theme activation.
    drop(state);
    drop(ctx_guard);
    if let Ok(mut ctx_mut) = ctx_arc.lock() {
        if let Some(persisted_state) = state_to_persist.as_ref()
            && let Err(e) = ctx_mut.persistence.save_state(persisted_state)
        {
            warn!(error = %e, "could not save reconciled startup state");
        }
        if let Err(e) = ctx_mut.theme_manager.set_active(&settings.theme) {
            warn!(error = %e, theme = %settings.theme, "could not activate saved theme");
        }
    }
}

/// Find Running agents whose tmux sessions no longer exist.
///
/// Agents persisted as Running without a backing repository are also stale.
/// Returns the collected dead agent IDs; does not mutate `state`.
fn reconcile_running_agents(state: &AppState, runtime: &TmuxRuntimeManager) -> Vec<AgentId> {
    let mut running_agents: Vec<(AgentId, LaunchSignature)> = Vec::new();
    let mut dead_ids = Vec::new();
    for agent in state
        .agents
        .iter()
        .filter(|agent| agent.status == AgentStatus::Running)
    {
        let Some(repository) = state.repository_by_id(&agent.repository_id) else {
            dead_ids.push(agent.id.clone());
            continue;
        };

        running_agents.push((
            agent.id.clone(),
            launch_signature_for_agent(agent, &repository.remote),
        ));
    }
    for (agent_id, signature) in running_agents {
        if !runtime.session_exists_for_signature(&agent_id, &signature) {
            dead_ids.push(agent_id);
        }
    }
    dead_ids
}

/// Mark reconciled dead agents Dead and rebuild indices when needed.
///
/// Returns whether state changed and should be persisted.
fn apply_dead_reconciliations(
    state: &mut AppState,
    dead_ids: Vec<AgentId>,
    normalized_engines: bool,
) -> bool {
    if dead_ids.is_empty() {
        return normalized_engines;
    }
    for agent_id in dead_ids {
        if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == agent_id) {
            agent.status = AgentStatus::Dead;
            agent.runtime_binding = None;
        }
    }
    state.rebuild_repository_agent_ids();
    state.normalize_selection_indices();
    true
}

/// Restore the runtime session map from persisted agent statuses exactly once.
///
/// Running agents prefer reattach to existing live tmux sessions by stable ID;
/// if missing, a new session is spawned.
/// Dead/non-running agents are intentionally NOT spawned.
pub fn restore_runtime_sessions(app_state: &mut HookState<AppState>, ctx: &SharedContext) {
    let Some(ctx_arc) = ctx else {
        return;
    };

    let (agents, repositories) = {
        let state = app_state.read();
        (state.agents.clone(), state.repositories.clone())
    };

    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        return;
    };

    let mut revived_running: Vec<(AgentId, LaunchSignature)> = Vec::new();
    let mut newly_dead = Vec::new();
    let mut runtime_warning: Option<String> = None;

    for agent in agents {
        if agent.status != AgentStatus::Running {
            continue;
        }

        let Some(repository) = repositories
            .iter()
            .find(|repository| repository.id == agent.repository_id)
            .cloned()
        else {
            newly_dead.push(agent.id.clone());
            continue;
        };
        let signature = launch_signature_for_agent(&agent, &repository.remote);

        if !ctx_guard
            .runtime
            .session_exists_for_signature(&agent.id, &signature)
        {
            newly_dead.push(agent.id.clone());
            continue;
        }

        match revive_agent_session(
            &agent,
            &signature,
            &mut ctx_guard.runtime,
            &mut runtime_warning,
        ) {
            ReviveOutcome::Revived => revived_running.push((agent.id.clone(), signature)),
            ReviveOutcome::Died => newly_dead.push(agent.id.clone()),
        }
    }

    drop(ctx_guard);

    if revived_running.is_empty() && newly_dead.is_empty() && runtime_warning.is_none() {
        return;
    }

    let mut state = app_state.write();
    apply_restored_state(&mut state, revived_running, newly_dead, runtime_warning);

    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

/// Outcome of attempting to revive a single Running agent's session.
enum ReviveOutcome {
    Revived,
    Died,
}

/// Attempt to reattach/respawn one agent's session.
///
/// `spawn_session` is the registration path into the runtime manager's
/// in-memory map; `AlreadyRunning` means the session is already tracked.
fn revive_agent_session(
    agent: &jefe::domain::Agent,
    signature: &LaunchSignature,
    runtime: &mut TmuxRuntimeManager,
    runtime_warning: &mut Option<String>,
) -> ReviveOutcome {
    match runtime.spawn_session(&agent.id, &agent.work_dir, signature) {
        Ok(()) | Err(RuntimeError::AlreadyRunning(_)) => {
            if runtime_warning.is_none() {
                *runtime_warning = jefe::runtime::sandbox_ssh_agent_warning();
            }
            ReviveOutcome::Revived
        }
        Err(e) => {
            warn!(agent_id = %agent.id.0, error = %e, "could not restore session");
            ReviveOutcome::Died
        }
    }
}

/// Apply restored session results to app state and persist.
fn apply_restored_state(
    state: &mut AppState,
    revived_running: Vec<(AgentId, LaunchSignature)>,
    newly_dead: Vec<AgentId>,
    runtime_warning: Option<String>,
) {
    for (agent_id, signature) in revived_running {
        if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == agent_id) {
            agent.status = AgentStatus::Running;
            let session_name = RuntimeSession::session_name_for(&agent_id);
            agent.runtime_binding = Some(jefe::domain::RuntimeBinding {
                session_name,
                launch_signature: signature,
                attached: false,
                last_seen: None,
            });
        }
    }
    for agent_id in newly_dead {
        if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == agent_id) {
            agent.status = AgentStatus::Dead;
            agent.runtime_binding = None;
        }
    }

    state.rebuild_repository_agent_ids();
    state.normalize_selection_indices();
    if let Some(warning) = runtime_warning {
        append_warning(state, warning);
    }
}
