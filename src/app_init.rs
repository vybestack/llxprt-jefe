//! One-time application startup: state hydration and runtime session restore.

#[path = "app_init_process_binding.rs"]
mod process_binding;
#[path = "app_init_shell_reconcile.rs"]
mod shell_reconcile;

use iocraft::hooks::State as HookState;
use tracing::warn;

use jefe::domain::{
    Agent, AgentId, AgentStatus, LaunchSignature, PlatformCapabilities, ProcessIdentity,
    SandboxEngine,
};
use jefe::persistence::{PersistenceManager, Settings, State as PersistedState};
use jefe::runtime::{
    ProcessLiveness, RuntimeError, RuntimeManager, RuntimeSession, TmuxRuntimeManager, pid_alive,
    platform_engine_diagnostic, process_liveness, process_liveness_indicates_alive,
};
use jefe::state::AppState;
use jefe::theme::ThemeManager;

use crate::app_input::{SharedContext, persist_state, to_persisted_state};

fn launch_signature_for_agent(
    agent: &Agent,
    repository: &jefe::domain::Repository,
) -> LaunchSignature {
    LaunchSignature {
        work_dir: agent.work_dir.clone(),
        profile: agent.profile.clone(),
        code_puppy_model: agent.code_puppy_model.trim().to_owned(),
        code_puppy_version: agent.code_puppy_version.trim().to_owned(),
        code_puppy_yolo: agent.code_puppy_yolo,
        code_puppy_quick_resume: agent.code_puppy_quick_resume,
        mode_flags: agent.mode_flags.clone(),
        llxprt_debug: agent.llxprt_debug.clone(),
        pass_continue: agent.pass_continue,
        sandbox_enabled: agent.sandbox_enabled,
        sandbox_engine: agent.sandbox_engine,
        sandbox_flags: agent.sandbox_flags.clone(),
        remote: repository.remote.clone(),
        agent_kind: agent.agent_kind,
        llxprt_version: agent.llxprt_version.clone(),
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionEvidence {
    Alive,
    Missing,
    Unavailable,
}

impl From<jefe::runtime::SessionLiveness> for SessionEvidence {
    fn from(value: jefe::runtime::SessionLiveness) -> Self {
        match value {
            jefe::runtime::SessionLiveness::Alive => Self::Alive,
            jefe::runtime::SessionLiveness::Missing => Self::Missing,
            jefe::runtime::SessionLiveness::Unavailable => Self::Unavailable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindingEvidence {
    Coherent,
    Legacy,
    Inconsistent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupClassification {
    Running,
    Stopped,
    Stale,
    Recoverable,
    Inconsistent,
}

#[must_use]
fn binding_evidence(
    binding: Option<&jefe::domain::RuntimeBinding>,
    agent_id: &AgentId,
    signature: &LaunchSignature,
) -> BindingEvidence {
    let Some(binding) = binding else {
        return BindingEvidence::Legacy;
    };
    if binding.session_name != RuntimeSession::session_name_for(agent_id)
        || binding.launch_signature != *signature
    {
        return BindingEvidence::Inconsistent;
    }
    match (binding.pid, binding.process_identity) {
        (Some(pid), Some(identity)) if pid != identity.pid => BindingEvidence::Inconsistent,
        (Some(_) | None, None) => BindingEvidence::Legacy,
        (None, Some(_)) => BindingEvidence::Inconsistent,
        (Some(_), Some(_)) => BindingEvidence::Coherent,
    }
}

#[must_use]
fn classify_startup(
    session: SessionEvidence,
    binding: BindingEvidence,
    remote: bool,
    process: ProcessLiveness,
) -> StartupClassification {
    if binding == BindingEvidence::Inconsistent {
        // A live session is ground truth: the agent is still running even if
        // the persisted binding signature drifted (e.g. a new binary recomputed
        // LaunchSignature fields differently). Returning Running lets
        // restore_runtime_sessions reattach the live session and refresh the
        // binding instead of marking the agent Dead (issue #323).
        if session == SessionEvidence::Alive {
            return StartupClassification::Running;
        }
        return StartupClassification::Inconsistent;
    }
    if !remote && process == ProcessLiveness::ReusedPid {
        return StartupClassification::Stale;
    }
    match session {
        SessionEvidence::Alive => StartupClassification::Running,
        SessionEvidence::Unavailable => StartupClassification::Recoverable,
        SessionEvidence::Missing if remote => StartupClassification::Stopped,
        SessionEvidence::Missing if process_liveness_indicates_alive(process) => {
            StartupClassification::Recoverable
        }
        SessionEvidence::Missing => match process {
            ProcessLiveness::Dead => StartupClassification::Stopped,
            ProcessLiveness::ReusedPid => StartupClassification::Stale,
            ProcessLiveness::MalformedIdentity => StartupClassification::Inconsistent,
            ProcessLiveness::Alive
            | ProcessLiveness::Inaccessible
            | ProcessLiveness::ProbeFailure => StartupClassification::Recoverable,
        },
    }
}

fn classify_agent_startup(
    agent: &Agent,
    signature: &LaunchSignature,
    runtime: &TmuxRuntimeManager,
) -> StartupClassification {
    let session = runtime
        .session_liveness_for_signature(&agent.id, signature)
        .into();
    let binding = binding_evidence(agent.runtime_binding.as_ref(), &agent.id, signature);
    let process = if signature.remote.enabled {
        ProcessLiveness::MalformedIdentity
    } else {
        process_liveness_for_binding(
            agent.runtime_binding.as_ref().and_then(|value| value.pid),
            agent
                .runtime_binding
                .as_ref()
                .and_then(|value| value.process_identity),
        )
    };
    classify_startup(session, binding, signature.remote.enabled, process)
}

fn process_liveness_for_binding(
    pid: Option<u32>,
    process_identity: Option<ProcessIdentity>,
) -> ProcessLiveness {
    if process_identity.is_some() {
        return process_liveness(process_identity);
    }
    match pid {
        Some(pid) if pid_alive(pid) => ProcessLiveness::Alive,
        Some(_) => ProcessLiveness::Dead,
        None => ProcessLiveness::MalformedIdentity,
    }
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
    state.installed_agent_kinds = jefe::agent_detection::installed_agent_kinds().to_vec();
    state.selected_repository_index = persisted.selected_repository_index;
    state.selected_agent_index = persisted.selected_agent_index;
    state.hide_idle_repositories = persisted.hide_idle_repositories;
    state.last_selected_agent_by_repo = persisted.last_selected_agent_by_repo;
    // Restore the persisted pane focus and terminal-focus so an explicitly
    // focused view survives restart (issue #160). `terminal_focused` is only
    // meaningful when the terminal pane is active, so clamp an inconsistent
    // persisted pair (terminal_focused=true but pane != Terminal) back to false;
    // the per-keypress defensive guard in app_shell would clear it anyway.
    state.pane_focus = crate::app_input::pane_focus_from_persisted(&persisted.pane_focus);
    state.terminal_focused =
        persisted.terminal_focused && state.pane_focus == jefe::state::PaneFocus::Terminal;
    state.user_preferences = persisted.user_preferences;
    // Mirror the persisted "apply jefe theme to agent" toggle (issue #179).
    // settings.toml is the source of truth; this runtime copy is read every
    // render frame by the terminal view.
    state.override_agent_theme = settings.override_agent_theme;
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
/// For LOCAL agents whose tmux session is gone, the persisted worker PID is
/// consulted as a liveness fallback: if the worker process is still alive
/// (reparented to launchd after the jefe tmux server died), the agent is left
/// Running rather than demoted to Dead. Remote agents stay on the
/// tmux/SSH-only path.
///
/// Returns the collected dead agent IDs; does not mutate `state`.
fn reconcile_running_agents(state: &AppState, runtime: &TmuxRuntimeManager) -> Vec<AgentId> {
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
        let signature = launch_signature_for_agent(agent, repository);
        if matches!(
            classify_agent_startup(agent, &signature, runtime),
            StartupClassification::Stopped
                | StartupClassification::Stale
                | StartupClassification::Inconsistent
        ) {
            dead_ids.push(agent.id.clone());
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

/// Outcome of processing a single agent during [`restore_runtime_sessions`].
enum RestoreOneOutcome {
    /// Agent was revived/reattached; carries its signature and worker PID.
    Revived {
        signature: Box<LaunchSignature>,
        pid: Option<u32>,
        process_identity: Option<ProcessIdentity>,
    },
    /// Agent should be marked Dead (binding cleared).
    Dead,
    /// Agent should be left as-is (non-running, or local orphan kept Running).
    Skip,
}

/// Process one agent during restore: decide Dead / Skip / Revive and, when
/// reviving, drive the runtime and capture the worker PID.
fn restore_one_agent(
    agent: &Agent,
    repositories: &[jefe::domain::Repository],
    runtime: &mut TmuxRuntimeManager,
    runtime_warning: &mut Option<String>,
) -> RestoreOneOutcome {
    if agent.status != AgentStatus::Running {
        return RestoreOneOutcome::Skip;
    }
    let Some(repository) = repositories
        .iter()
        .find(|repository| repository.id == agent.repository_id)
        .cloned()
    else {
        return RestoreOneOutcome::Dead;
    };
    let signature = launch_signature_for_agent(agent, &repository);
    let binding = agent.runtime_binding.as_ref();
    let persisted_process = process_binding::ProcessBindingObservation::new(
        binding.and_then(|value| value.pid),
        binding.and_then(|value| value.process_identity),
    );

    match classify_agent_startup(agent, &signature, runtime) {
        StartupClassification::Stopped
        | StartupClassification::Stale
        | StartupClassification::Inconsistent => RestoreOneOutcome::Dead,
        StartupClassification::Recoverable => RestoreOneOutcome::Skip,
        StartupClassification::Running => {
            match revive_agent_session(agent, &signature, runtime, runtime_warning) {
                ReviveOutcome::Revived => {
                    let fresh_process = process_binding::ProcessBindingObservation::new(
                        runtime.worker_pid(&agent.id),
                        runtime.worker_process_identity(&agent.id),
                    );
                    let process =
                        process_binding::resolve_process_binding(fresh_process, persisted_process);
                    RestoreOneOutcome::Revived {
                        signature: Box::new(signature),
                        pid: process.pid,
                        process_identity: process.identity,
                    }
                }
                ReviveOutcome::Died => RestoreOneOutcome::Dead,
            }
        }
    }
}

/// Restore the runtime session map from persisted agent statuses exactly once.
///
/// Running agents prefer reattach to existing live tmux sessions by stable ID;
/// if missing, a new session is spawned.
/// Dead/non-running agents are intentionally NOT spawned.
/// Local agents whose tmux session is gone but whose persisted worker PID is
/// still alive are left Running with their binding preserved (PID-liveness
/// fallback), rather than being marked Dead or revived.
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

    let mut revived_running: Vec<(
        AgentId,
        LaunchSignature,
        Option<u32>,
        Option<ProcessIdentity>,
    )> = Vec::new();
    let mut newly_dead = Vec::new();
    let mut runtime_warning: Option<String> = None;

    for agent in agents {
        match restore_one_agent(
            &agent,
            &repositories,
            &mut ctx_guard.runtime,
            &mut runtime_warning,
        ) {
            RestoreOneOutcome::Revived {
                signature,
                pid,
                process_identity,
            } => {
                revived_running.push((agent.id.clone(), *signature, pid, process_identity));
            }
            RestoreOneOutcome::Dead => newly_dead.push(agent.id.clone()),
            RestoreOneOutcome::Skip => {}
        }
    }

    drop(ctx_guard);

    let state_changed =
        !revived_running.is_empty() || !newly_dead.is_empty() || runtime_warning.is_some();
    if state_changed {
        let mut state = app_state.write();
        apply_restored_state(&mut state, revived_running, newly_dead, runtime_warning);
    }

    if let Some(warning) = shell_reconcile::reconcile_shell_inventory(app_state, ctx) {
        append_warning(&mut app_state.write(), warning);
    }
    if state_changed {
        let state = app_state.read();
        persist_state(ctx, &to_persisted_state(&state));
    }
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
            // SSH-agent warning is only relevant for LLxprt sandbox sessions;
            // CodePuppy does not use the LLxprt sandbox subsystem.
            if runtime_warning.is_none() && agent.agent_kind == jefe::domain::AgentKind::Llxprt {
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
    revived_running: Vec<(
        AgentId,
        LaunchSignature,
        Option<u32>,
        Option<ProcessIdentity>,
    )>,
    newly_dead: Vec<AgentId>,
    runtime_warning: Option<String>,
) {
    for (agent_id, signature, pid, process_identity) in revived_running {
        if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == agent_id) {
            agent.status = AgentStatus::Running;
            let session_name = RuntimeSession::session_name_for(&agent_id);
            agent.runtime_binding = Some(jefe::domain::RuntimeBinding {
                session_name,
                launch_signature: signature,
                attached: false,
                last_seen: None,
                process_identity,
                pid,
                lifecycle_generation: 0,
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

#[cfg(test)]
mod tests {
    use super::*;
    use jefe::domain::{AgentKind, Repository, RepositoryId};

    fn code_puppy_agent_and_repository() -> (Agent, Repository) {
        let repository_id = RepositoryId("repo-model".to_owned());
        let mut repository = Repository::new(
            repository_id.clone(),
            "Model Repo".to_owned(),
            "model-repo".to_owned(),
            std::path::PathBuf::from("/tmp/model-repo"),
        );
        repository.default_code_puppy_model = "  repo/default-model  ".to_owned();

        let mut agent = Agent::new(
            AgentId("agent-model".to_owned()),
            repository_id,
            "Model Agent".to_owned(),
            std::path::PathBuf::from("/tmp/model-agent"),
        );
        agent.agent_kind = AgentKind::CodePuppy;
        (agent, repository)
    }

    #[test]
    fn launch_signature_uses_agent_code_puppy_model() {
        let (mut agent, repository) = code_puppy_agent_and_repository();
        agent.code_puppy_model = "  agent/model  ".to_owned();

        let signature = launch_signature_for_agent(&agent, &repository);

        assert_eq!(signature.code_puppy_model, "agent/model");
    }

    #[test]
    fn launch_signature_does_not_dynamically_inherit_repository_model() {
        let (agent, repository) = code_puppy_agent_and_repository();

        let signature = launch_signature_for_agent(&agent, &repository);

        assert!(signature.code_puppy_model.is_empty());
    }

    #[test]
    fn legacy_pid_only_binding_uses_conservative_native_probe() {
        let pid = std::process::id();
        assert_eq!(
            process_liveness_for_binding(Some(pid), None),
            ProcessLiveness::Alive
        );
        assert_eq!(
            process_liveness_for_binding(Some(2_000_000_000), None),
            ProcessLiveness::Dead
        );
        assert_eq!(
            process_liveness_for_binding(None, None),
            ProcessLiveness::MalformedIdentity
        );
    }

    #[test]
    fn startup_classification_covers_required_lifecycle_states() {
        let coherent = BindingEvidence::Coherent;
        assert_eq!(
            classify_startup(
                SessionEvidence::Alive,
                coherent,
                false,
                ProcessLiveness::Dead
            ),
            StartupClassification::Running
        );
        assert_eq!(
            classify_startup(
                SessionEvidence::Missing,
                coherent,
                false,
                ProcessLiveness::Dead
            ),
            StartupClassification::Stopped
        );
        assert_eq!(
            classify_startup(
                SessionEvidence::Missing,
                coherent,
                false,
                ProcessLiveness::ReusedPid
            ),
            StartupClassification::Stale
        );
        assert_eq!(
            classify_startup(
                SessionEvidence::Alive,
                coherent,
                false,
                ProcessLiveness::ReusedPid
            ),
            StartupClassification::Stale
        );
        assert_eq!(
            classify_startup(
                SessionEvidence::Missing,
                coherent,
                false,
                ProcessLiveness::Alive
            ),
            StartupClassification::Recoverable
        );
        assert_eq!(
            classify_startup(
                SessionEvidence::Missing,
                BindingEvidence::Inconsistent,
                false,
                ProcessLiveness::Alive
            ),
            StartupClassification::Inconsistent
        );
    }

    #[test]
    fn unavailable_runtime_probe_is_recoverable_not_phantom_dead() {
        for liveness in [ProcessLiveness::Dead, ProcessLiveness::ProbeFailure] {
            assert_eq!(
                classify_startup(
                    SessionEvidence::Unavailable,
                    BindingEvidence::Coherent,
                    false,
                    liveness
                ),
                StartupClassification::Recoverable
            );
        }
    }

    #[test]
    fn missing_remote_session_is_stopped_without_local_pid_fallback() {
        assert_eq!(
            classify_startup(
                SessionEvidence::Missing,
                BindingEvidence::Coherent,
                true,
                ProcessLiveness::Alive
            ),
            StartupClassification::Stopped
        );
    }

    #[test]
    fn malformed_or_inaccessible_process_identity_is_classified_conservatively() {
        assert_eq!(
            classify_startup(
                SessionEvidence::Missing,
                BindingEvidence::Coherent,
                false,
                ProcessLiveness::MalformedIdentity
            ),
            StartupClassification::Inconsistent
        );
        assert_eq!(
            classify_startup(
                SessionEvidence::Missing,
                BindingEvidence::Coherent,
                false,
                ProcessLiveness::Inaccessible
            ),
            StartupClassification::Recoverable
        );
    }

    #[test]
    fn live_session_survives_mismatched_binding_for_reattach() {
        // Issue #323: a live tmux session must not be killed just because the
        // persisted binding signature drifted. The session is the ground truth;
        // the binding can be refreshed during restore.
        for liveness in [
            ProcessLiveness::Alive,
            ProcessLiveness::Dead,
            ProcessLiveness::ReusedPid,
        ] {
            assert_eq!(
                classify_startup(
                    SessionEvidence::Alive,
                    BindingEvidence::Inconsistent,
                    false,
                    liveness
                ),
                StartupClassification::Running,
                "Alive session with Inconsistent binding and {liveness:?} process should be Running"
            );
        }
    }

    #[test]
    fn missing_session_with_inconsistent_binding_still_inconsistent() {
        // Negative case: without a live session there is nothing to rescue,
        // so the Inconsistent classification is preserved (existing behavior).
        assert_eq!(
            classify_startup(
                SessionEvidence::Missing,
                BindingEvidence::Inconsistent,
                false,
                ProcessLiveness::Alive
            ),
            StartupClassification::Inconsistent
        );
        assert_eq!(
            classify_startup(
                SessionEvidence::Missing,
                BindingEvidence::Inconsistent,
                true,
                ProcessLiveness::Alive
            ),
            StartupClassification::Inconsistent
        );
    }

    #[test]
    fn binding_evidence_rejects_wrong_session_signature_and_pid() {
        let (agent, repository) = code_puppy_agent_and_repository();
        let signature = launch_signature_for_agent(&agent, &repository);
        let mut binding = jefe::domain::RuntimeBinding {
            session_name: RuntimeSession::session_name_for(&agent.id),
            launch_signature: signature.clone(),
            attached: false,
            last_seen: None,
            pid: Some(41),
            process_identity: Some(ProcessIdentity::new(41, 900)),
            lifecycle_generation: 0,
        };
        assert_eq!(
            binding_evidence(Some(&binding), &agent.id, &signature),
            BindingEvidence::Coherent
        );
        binding.session_name = "jefe-wrong-agent".to_owned();
        assert_eq!(
            binding_evidence(Some(&binding), &agent.id, &signature),
            BindingEvidence::Inconsistent
        );
        binding.session_name = RuntimeSession::session_name_for(&agent.id);
        binding.launch_signature.profile = "wrong-profile".to_owned();
        assert_eq!(
            binding_evidence(Some(&binding), &agent.id, &signature),
            BindingEvidence::Inconsistent
        );
        binding.launch_signature = signature.clone();
        binding.pid = Some(42);
        assert_eq!(
            binding_evidence(Some(&binding), &agent.id, &signature),
            BindingEvidence::Inconsistent
        );
        assert_eq!(
            binding_evidence(None, &agent.id, &signature),
            BindingEvidence::Legacy
        );
        binding_evidence_rejects_different_llxprt_selector();
        binding_evidence_rejects_different_code_puppy_version();
    }

    fn binding_evidence_rejects_different_code_puppy_version() {
        let (mut agent, repository) = code_puppy_agent_and_repository();
        agent.code_puppy_version = "0.0.361".to_owned();
        let signature = launch_signature_for_agent(&agent, &repository);
        assert_eq!(signature.code_puppy_version, "0.0.361");
        let mut bound_signature = signature.clone();
        bound_signature.code_puppy_version = "0.0.360".to_owned();
        let binding = jefe::domain::RuntimeBinding {
            session_name: RuntimeSession::session_name_for(&agent.id),
            launch_signature: bound_signature,
            attached: false,
            last_seen: None,
            pid: Some(41),
            process_identity: Some(ProcessIdentity::new(41, 900)),
            lifecycle_generation: 0,
        };
        assert_eq!(
            binding_evidence(Some(&binding), &agent.id, &signature),
            BindingEvidence::Inconsistent
        );
    }

    fn binding_evidence_rejects_different_llxprt_selector() {
        let (mut agent, repository) = code_puppy_agent_and_repository();
        agent.agent_kind = jefe::domain::AgentKind::Llxprt;
        agent.llxprt_version = jefe::domain::LlxprtNpmPackageSelector::normalize("nightly");
        let signature = launch_signature_for_agent(&agent, &repository);
        assert_eq!(signature.llxprt_version, agent.llxprt_version);
        let mut bound_signature = signature.clone();
        bound_signature.llxprt_version =
            jefe::domain::LlxprtNpmPackageSelector::normalize("latest");
        let binding = jefe::domain::RuntimeBinding {
            session_name: RuntimeSession::session_name_for(&agent.id),
            launch_signature: bound_signature,
            attached: false,
            last_seen: None,
            pid: Some(41),
            process_identity: Some(ProcessIdentity::new(41, 900)),
            lifecycle_generation: 0,
        };
        assert_eq!(
            binding_evidence(Some(&binding), &agent.id, &signature),
            BindingEvidence::Inconsistent
        );
    }
}
