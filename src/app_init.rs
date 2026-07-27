//! One-time application startup: state hydration and runtime session restore.

#[path = "app_init_orphan_reconcile.rs"]
mod orphan_reconcile;
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
use jefe::persistence::{PersistenceManager, Settings};
#[cfg(windows)]
use jefe::runtime::MultiplexerPlan;
use jefe::runtime::{
    ProcessLiveness, RuntimeError, RuntimeManager, RuntimeSession, TmuxRuntimeManager, pid_alive,
    platform_engine_diagnostic, process_liveness, process_liveness_indicates_alive,
};
use jefe::state::AppState;
use jefe::theme::ThemeManager;

use crate::app_input::{SharedContext, durable_save_request, schedule_durable_save};

fn launch_signature_for_agent(
    agent: &Agent,
    repository: &jefe::domain::Repository,
) -> LaunchSignature {
    LaunchSignature::for_agent(agent, repository)
}

fn append_warning(state: &mut AppState, warning: String) {
    state.warning_message = Some(match state.warning_message.take() {
        Some(existing) => format!("{existing} {warning}"),
        None => warning,
    });
}
fn apply_startup_warning(state: &mut AppState, warning: Option<String>) {
    if let Some(warning) = warning {
        append_warning(state, warning);
    }
}

#[cfg(windows)]
fn windows_multiplexer_startup_warning() -> Option<String> {
    let result = MultiplexerPlan::current().and_then(|plan| plan.preflight(&[]));
    match result {
        Ok(version) => {
            tracing::info!(%version, "native Windows multiplexer preflight succeeded");
            None
        }
        Err(error) => {
            warn!(error = %error, "native Windows multiplexer preflight failed");
            Some(format!("psmux preflight warning: {error}"))
        }
    }
}

#[cfg(not(windows))]
fn windows_multiplexer_startup_warning() -> Option<String> {
    None
}

/// Run the issue #467 AC8 startup session-host cleanup.
///
/// Supplies the persisted `RuntimeBinding.session_name` references for every
/// Running agent and probes live local sessions before any directory is
/// deleted. Remote sessions never own a local host image and are excluded
/// from the reference set because their session-host directory is never
/// staged. A cleanup failure is logged and never aborts startup.
fn run_startup_session_host_cleanup(state: &AppState, runtime: &TmuxRuntimeManager) {
    let Some(root) = runtime.session_host_root() else {
        return;
    };
    let persisted_references: Vec<String> = state
        .agents
        .iter()
        .filter(|agent| agent.status == AgentStatus::Running)
        .filter_map(|agent| agent.runtime_binding.as_ref())
        .map(|binding| binding.session_name.clone())
        .collect();
    // The manager probes live local sessions before deletion; a session whose
    // probe cannot be classified is retained rather than risk deleting a live
    // session whose probe transiently failed.
    let probe = jefe::runtime::session_liveness;
    let _ = jefe::runtime::startup_cleanup_session_hosts(root, &persisted_references, probe)
        .map_err(|error| {
            warn!(error = %error, "session-host startup cleanup failed; retained for next startup");
        });
}

fn agent_type_enabled(
    settings: &jefe::persistence::settings_document::PublishedSettings,
    type_id: &jefe::domain::agent_definition::AgentTypeId,
) -> bool {
    let Ok(owner_id) = jefe::domain::Id::parse(type_id.as_str()) else {
        return true;
    };
    settings
        .agents
        .get(&owner_id)
        .and_then(|owner| owner.enabled)
        .unwrap_or(true)
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
    /// Dead pane with surviving validated worker descendants (issue #332). The
    /// caller must reap the orphan tree and remove the stale session before
    /// marking the agent Dead; a live descendant under a dead pane must never
    /// be treated as reattachable.
    Orphaned,
}

#[must_use]
fn binding_evidence(
    binding: Option<&jefe::domain::RuntimeBinding>,
    agent_id: &AgentId,
    signature: &LaunchSignature,
    durable_signature_matches: bool,
) -> BindingEvidence {
    if !durable_signature_matches {
        return BindingEvidence::Inconsistent;
    }
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
    orphan: jefe::runtime::OrphanClassification,
) -> StartupClassification {
    use jefe::runtime::OrphanClassification as Oc;

    // A dead pane with surviving validated worker descendants is the orphan
    // state (issue #332): it must never be treated as reattachable. This takes
    // precedence over the Recoverable fallback so a live descendant under a
    // dead pane is reaped instead of left stranded. Only applies when the
    // session is not alive â€” a healthy pane is never an orphan.
    if session != SessionEvidence::Alive && orphan == Oc::DeadPaneWithOrphans {
        return StartupClassification::Orphaned;
    }
    if binding == BindingEvidence::Inconsistent {
        return StartupClassification::Inconsistent;
    }
    if !remote && process == ProcessLiveness::ReusedPid {
        return StartupClassification::Stale;
    }
    match session {
        SessionEvidence::Alive => StartupClassification::Running,
        SessionEvidence::Unavailable => StartupClassification::Recoverable,
        SessionEvidence::Missing if remote => StartupClassification::Stopped,
        SessionEvidence::Missing => match process {
            ProcessLiveness::Dead => StartupClassification::Stopped,
            ProcessLiveness::ReusedPid => StartupClassification::Stale,
            ProcessLiveness::MalformedIdentity => StartupClassification::Inconsistent,
            liveness if process_liveness_indicates_alive(liveness) => {
                StartupClassification::Recoverable
            }
            _ => StartupClassification::Inconsistent,
        },
    }
}

fn durable_signature_matches(agent: &Agent, repository: &jefe::domain::Repository) -> bool {
    match agent.persisted_launch_signature.as_ref() {
        None => true,
        Some(persisted) => {
            jefe::state::durable_projection::current_launch_signature(agent, repository)
                .is_ok_and(|current| current == *persisted)
        }
    }
}

fn classify_agent_startup(
    agent: &Agent,
    repository: &jefe::domain::Repository,
    signature: &LaunchSignature,
    runtime: &TmuxRuntimeManager,
) -> StartupClassification {
    let session = runtime
        .session_liveness_for_signature(&agent.id, signature)
        .into();
    let durable_signature_matches = durable_signature_matches(agent, repository);
    let binding = binding_evidence(
        agent.runtime_binding.as_ref(),
        &agent.id,
        signature,
        durable_signature_matches,
    );
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
    let orphan = orphan_reconcile::orphan_evidence(
        session,
        signature.remote.enabled,
        agent
            .runtime_binding
            .as_ref()
            .map(|value| &value.worker_identities),
    );
    classify_startup(session, binding, signature.remote.enabled, process, orphan)
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
fn restore_persisted_state(
    state: &mut AppState,
    persisted: jefe::state::durable_projection::RestoredState,
) {
    state.repositories = persisted.repositories;
    state.agents = persisted.agents;
    state.selected_repository_index = persisted.selected_repository_index;
    state.selected_agent_index = persisted.selected_agent_index;
    state.hide_idle_repositories = persisted.hide_idle_repositories;
    state.last_selected_agent_by_repo = persisted.last_selected_agent_by_repo;
    state.durable_revision = persisted.revision;
    state.dormant_records = persisted.dormant_records;
    state.pane_focus = persisted.pane_focus;
    state.terminal_focused =
        persisted.terminal_focused && state.pane_focus == jefe::state::PaneFocus::Terminal;
    state.user_preferences = persisted.user_preferences;
}

fn observe_agent_types(
    state: &mut AppState,
    settings: &jefe::persistence::settings_document::PublishedSettings,
) -> Vec<jefe::domain::effects::IssuedEffect> {
    let repository_root = state.selected_repository().map_or_else(
        || std::path::PathBuf::from("."),
        |repository| repository.base_dir.clone(),
    );
    match jefe::agent_registry::AgentTypeRegistry::shipped() {
        Ok(registry) => {
            let startup = crate::app_input::observe_startup_agent_availability(
                &registry,
                &repository_root,
                |type_id| agent_type_enabled(settings, type_id),
            );
            state.installed_agent_kinds = jefe::agent_detection::compatible_legacy_agent_kinds(
                &startup.observations,
                registry.definitions(),
            );
            state.agent_type_availability = startup.observations;
            jefe::state::transition::commit_in_place(
                state,
                jefe::messages::AppMessage::RepositoryAgent(
                    jefe::messages::RepositoryAgentMessage::ProbeAgentAvailability(startup.probes),
                ),
            )
        }
        Err(error) => {
            append_warning(
                state,
                format!("Agent type registry could not be published: {error}"),
            );
            Vec::new()
        }
    }
}

/// tmux sessions, marking stale ones Dead.  Also activates the saved theme.
pub fn init_app_state(
    app_state: &mut HookState<AppState>,
    ctx: &SharedContext,
) -> Vec<jefe::domain::effects::IssuedEffect> {
    let multiplexer_warning = windows_multiplexer_startup_warning();
    let Some(ctx_arc) = ctx else {
        return Vec::new();
    };
    let Ok(ctx_guard) = ctx_arc.lock() else {
        return Vec::new();
    };

    let settings = ctx_guard.persistence.load_settings().unwrap_or_else(|e| {
        warn!(error = %e, "could not load settings, using defaults");
        Settings::default_with_version()
    });

    let persisted = ctx_guard
        .persistence
        .load_durable_state()
        .unwrap_or_else(|e| {
            warn!(error = %e, "could not load state, using defaults");
            jefe::state::durable_projection::RestoredState::default()
        });

    let mut state = app_state.write();
    restore_persisted_state(&mut state, persisted);
    apply_startup_warning(&mut state, multiplexer_warning);
    state.override_agent_theme = settings.override_agent_theme;
    state.rebuild_repository_agent_ids();
    state.normalize_selection_indices();
    let agent_probe_effects = observe_agent_types(&mut state, &ctx_guard.published_settings);

    // Log platform engine diagnostic at startup.
    tracing::info!("{}", platform_engine_diagnostic());

    // Normalize any persisted sandbox engines that are unsupported on this platform.
    let normalized_engines = normalize_persisted_sandbox_engines(&mut state);

    let dead_ids = reconcile_running_agents(&state, &ctx_guard.runtime);
    // Issue #467 AC8: sweep the session-host root before applying
    // reconciliations so unreferenced/dead directories and interrupted staging
    // temps are reclaimed, while live psmux sessions and persisted-reference
    // directories are retained. Best-effort: a failure is logged and never
    // aborts startup.
    run_startup_session_host_cleanup(&state, &ctx_guard.runtime);
    let should_persist = apply_dead_reconciliations(&mut state, dead_ids, normalized_engines);
    // The persist worker is not running yet, so startup reconciliation writes
    // its candidate synchronously through the manager.
    let state_to_persist = should_persist
        .then(|| durable_save_request(&mut state))
        .flatten();

    // Release state/context guards before reacquiring a mutable context lock
    // for persistence writes and theme activation.
    drop(state);
    drop(ctx_guard);
    if let Ok(mut ctx_mut) = ctx_arc.lock() {
        if let Some(request) = state_to_persist.as_ref()
            && let Err(e) = ctx_mut.persistence.save_state_v2_revisioned(
                request.candidate.as_ref(),
                request.revision,
                &|_| jefe::persistence::writer::Freshness::Current,
            )
        {
            warn!(error = %e, "could not save reconciled startup state");
        }
        if let Err(e) = ctx_mut.theme_manager.set_active(&settings.theme) {
            warn!(error = %e, theme = %settings.theme, "could not activate saved theme");
        }
    }
    agent_probe_effects
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
        match classify_agent_startup(agent, repository, &signature, runtime) {
            StartupClassification::Orphaned => {
                // Dead pane with surviving validated worker descendants
                // (issue #332): reap the orphan tree and remove the stale
                // session before marking Dead. Best-effort, agent-scoped,
                // warn-don't-fail â€” probe/kill failures never abort startup.
                orphan_reconcile::reap_orphaned_agent(agent);
                dead_ids.push(agent.id.clone());
            }
            StartupClassification::Stopped
            | StartupClassification::Stale
            | StartupClassification::Inconsistent => {
                dead_ids.push(agent.id.clone());
            }
            _ => {}
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

    match classify_agent_startup(agent, &repository, &signature, runtime) {
        StartupClassification::Stopped
        | StartupClassification::Stale
        | StartupClassification::Inconsistent
        | StartupClassification::Orphaned => RestoreOneOutcome::Dead,
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
        let request = durable_save_request(&mut app_state.write());
        schedule_durable_save(ctx, request);
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
                worker_identities: Vec::new(),
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
    fn durable_signature_rejects_changed_values_and_target() {
        let (mut agent, repository) = code_puppy_agent_and_repository();
        let current =
            jefe::state::durable_projection::current_launch_signature(&agent, &repository)
                .unwrap_or_else(|error| panic!("fixture signature must project: {error}"));
        agent.persisted_launch_signature = Some(current);
        assert!(durable_signature_matches(&agent, &repository));

        agent.code_puppy_model = "changed-model".to_owned();
        assert!(!durable_signature_matches(&agent, &repository));
        agent.code_puppy_model.clear();
        agent.work_dir = std::path::PathBuf::from("/tmp/changed-target");
        assert!(!durable_signature_matches(&agent, &repository));
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
        use jefe::runtime::OrphanClassification as Oc;
        // Local helper: fix remote=false and orphan=NoOrphan so each row is a
        // compact (session, binding, process) -> expected assertion.
        let cls = |session, process, expected| {
            assert_eq!(
                classify_startup(
                    session,
                    BindingEvidence::Coherent,
                    false,
                    process,
                    Oc::NoOrphan
                ),
                expected
            );
        };
        cls(
            SessionEvidence::Alive,
            ProcessLiveness::Dead,
            StartupClassification::Running,
        );
        cls(
            SessionEvidence::Missing,
            ProcessLiveness::Dead,
            StartupClassification::Stopped,
        );
        cls(
            SessionEvidence::Missing,
            ProcessLiveness::ReusedPid,
            StartupClassification::Stale,
        );
        cls(
            SessionEvidence::Alive,
            ProcessLiveness::ReusedPid,
            StartupClassification::Stale,
        );
        cls(
            SessionEvidence::Missing,
            ProcessLiveness::Alive,
            StartupClassification::Recoverable,
        );
        assert_eq!(
            classify_startup(
                SessionEvidence::Missing,
                BindingEvidence::Inconsistent,
                false,
                ProcessLiveness::Alive,
                Oc::NoOrphan,
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
                    liveness,
                    jefe::runtime::OrphanClassification::NoOrphan,
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
                ProcessLiveness::Alive,
                jefe::runtime::OrphanClassification::NoOrphan,
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
                ProcessLiveness::MalformedIdentity,
                jefe::runtime::OrphanClassification::NoOrphan,
            ),
            StartupClassification::Inconsistent
        );
        assert_eq!(
            classify_startup(
                SessionEvidence::Missing,
                BindingEvidence::Coherent,
                false,
                ProcessLiveness::Inaccessible,
                jefe::runtime::OrphanClassification::NoOrphan,
            ),
            StartupClassification::Recoverable
        );
    }

    #[test]
    fn live_session_with_mismatched_binding_is_rejected() {
        // A live session is reattachable only when its persisted launch
        // signature still matches current definition, values, and target.
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
                    liveness,
                    jefe::runtime::OrphanClassification::NoOrphan,
                ),
                StartupClassification::Inconsistent,
                "live session with inconsistent signature and {liveness:?} process must not reattach"
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
                ProcessLiveness::Alive,
                jefe::runtime::OrphanClassification::NoOrphan,
            ),
            StartupClassification::Inconsistent
        );
        assert_eq!(
            classify_startup(
                SessionEvidence::Missing,
                BindingEvidence::Inconsistent,
                true,
                ProcessLiveness::Alive,
                jefe::runtime::OrphanClassification::NoOrphan,
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
            worker_identities: Vec::new(),
        };
        assert_eq!(
            binding_evidence(Some(&binding), &agent.id, &signature, true),
            BindingEvidence::Coherent
        );

        binding.session_name = "jefe-wrong-agent".to_owned();
        assert_eq!(
            binding_evidence(Some(&binding), &agent.id, &signature, true),
            BindingEvidence::Inconsistent
        );
        binding.session_name = RuntimeSession::session_name_for(&agent.id);
        binding.launch_signature.profile = "wrong-profile".to_owned();
        assert_eq!(
            binding_evidence(Some(&binding), &agent.id, &signature, true),
            BindingEvidence::Inconsistent
        );
        binding.launch_signature = signature.clone();
        binding.pid = Some(42);
        assert_eq!(
            binding_evidence(Some(&binding), &agent.id, &signature, true),
            BindingEvidence::Inconsistent
        );
        assert_eq!(
            binding_evidence(None, &agent.id, &signature, true),
            BindingEvidence::Legacy
        );
        binding_evidence_rejects_different_llxprt_selector();
        binding_evidence_rejects_different_code_puppy_version();
    }

    #[test]
    fn published_agent_enablement_is_separate_from_availability() {
        let catalog = jefe::config_owners::builtin_owner_catalog()
            .unwrap_or_else(|error| panic!("owner catalog must publish: {error}"));
        let migration = jefe::persistence::migration::migrate_settings(
            b"settings_schema = 2\n[agents.\"core.codex\"]\nenabled = false\n",
            &catalog,
        )
        .unwrap_or_else(|diagnostics| panic!("settings must publish: {diagnostics:?}"));
        let type_id = jefe::domain::agent_definition::AgentTypeId::parse("core.codex")
            .unwrap_or_else(|error| panic!("type id must parse: {error}"));

        assert!(!agent_type_enabled(migration.published(), &type_id));

        let absent = jefe::domain::agent_definition::AgentTypeId::parse("core.llxprt")
            .unwrap_or_else(|error| panic!("type id must parse: {error}"));
        assert!(agent_type_enabled(migration.published(), &absent));
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
            worker_identities: Vec::new(),
        };
        assert_eq!(
            binding_evidence(Some(&binding), &agent.id, &signature, true),
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
            worker_identities: Vec::new(),
        };
        assert_eq!(
            binding_evidence(Some(&binding), &agent.id, &signature, true),
            BindingEvidence::Inconsistent
        );
    }
}
