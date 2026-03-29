//! One-time application startup: state hydration and runtime session restore.

use iocraft::hooks::State as HookState;
use tracing::warn;

use jefe::domain::{AgentId, AgentStatus, LaunchSignature};
use jefe::persistence::{PersistenceManager, Settings, State as PersistedState};
use jefe::runtime::{RuntimeError, RuntimeManager, RuntimeSession};
use jefe::state::{AppEvent, AppState};
use jefe::theme::ThemeManager;

use crate::app_input::{SharedContext, persist_state_snapshot, to_persisted_state};

/// Load persisted state and settings into `app_state` exactly once.
///
/// Reconciles any agents that were persisted as Running against actual live
/// tmux sessions, marking stale ones Dead.  Also activates the saved theme.
#[allow(clippy::too_many_lines)]
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

    // Reconcile persisted Running statuses against actual tmux sessions.
    // Running agents without a backing repository are stale and must be marked
    // Dead during startup reconciliation.
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
            LaunchSignature {
                work_dir: agent.work_dir.clone(),
                profile: agent.profile.clone(),
                mode_flags: agent.mode_flags.clone(),
                llxprt_debug: agent.llxprt_debug.clone(),
                pass_continue: agent.pass_continue,
                sandbox_enabled: agent.sandbox_enabled,
                sandbox_engine: agent.sandbox_engine,
                sandbox_flags: agent.sandbox_flags.clone(),
                remote: repository.remote.clone(),
            },
        ));
    }
    for (agent_id, signature) in running_agents {
        if !ctx_guard
            .runtime
            .session_exists_for_signature(&agent_id, &signature)
        {
            dead_ids.push(agent_id);
        }
    }

    let state_to_persist: Option<PersistedState> = if dead_ids.is_empty() {
        None
    } else {
        for agent_id in dead_ids {
            *state = std::mem::take(&mut *state)
                .apply(AppEvent::AgentStatusChanged(agent_id, AgentStatus::Dead));
        }
        Some(to_persisted_state(&state))
    };

    drop(state);
    drop(ctx_guard);
    if let Ok(mut ctx_mut) = ctx_arc.lock() {
        if let Some(persisted_state) = state_to_persist.as_ref()
            && let Err(e) = ctx_mut.persistence.save_state(persisted_state)
        {
            warn!(error = %e, "could not save reconciled startup state");
        }
        let _ = ctx_mut.theme_manager.set_active(&settings.theme);
    }
}

/// Restore the runtime session map from persisted agent statuses exactly once.
///
/// Running agents prefer reattach to existing live tmux sessions by stable ID;
/// if missing, a new session is spawned.
/// Dead/non-running agents are intentionally NOT spawned.
#[allow(clippy::too_many_lines)]
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
        let signature = LaunchSignature {
            work_dir: agent.work_dir.clone(),
            profile: agent.profile.clone(),
            mode_flags: agent.mode_flags.clone(),
            llxprt_debug: agent.llxprt_debug.clone(),
            pass_continue: agent.pass_continue,
            sandbox_enabled: agent.sandbox_enabled,
            sandbox_engine: agent.sandbox_engine,
            sandbox_flags: agent.sandbox_flags.clone(),
            remote: repository.remote.clone(),
        };

        if !ctx_guard
            .runtime
            .session_exists_for_signature(&agent.id, &signature)
        {
            newly_dead.push(agent.id.clone());
            continue;
        }

        // This call intentionally runs even after session_exists_for_signature:
        // spawn_session is the registration path into the runtime manager's
        // in-memory map. AlreadyRunning means the session is already tracked.

        match ctx_guard
            .runtime
            .spawn_session(&agent.id, &agent.work_dir, &signature)
        {
            Ok(()) | Err(RuntimeError::AlreadyRunning(_)) => {
                revived_running.push((agent.id.clone(), signature));
                if runtime_warning.is_none() {
                    runtime_warning = jefe::runtime::sandbox_ssh_agent_warning();
                }
            }
            Err(e) => {
                warn!(agent_id = %agent.id.0, error = %e, "could not restore session");
                newly_dead.push(agent.id.clone());
            }
        }
    }

    drop(ctx_guard);

    if revived_running.is_empty() && newly_dead.is_empty() && runtime_warning.is_none() {
        return;
    }

    let mut state = app_state.write();
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
        state.warning_message = Some(warning);
    }

    persist_state_snapshot(ctx, &state);
}
