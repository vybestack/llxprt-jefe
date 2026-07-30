//! One-time application startup: state hydration and runtime session restore.

#[path = "app_init_orphan_reconcile.rs"]
mod orphan_reconcile;
#[path = "app_init_shell_reconcile.rs"]
mod shell_reconcile;
#[path = "app_init_signature_reconcile.rs"]
mod signature_reconcile;

#[cfg(test)]
use self::signature_reconcile::{BindingEvidence, DurableSignatureEvidence};
use self::signature_reconcile::{
    SessionEvidence, StartupClassification, binding_evidence, classify_startup,
    durable_signature_evidence,
};
use iocraft::hooks::State as HookState;
use tracing::warn;

use jefe::domain::{Agent, AgentId, AgentLaunchRequest, AgentStatus, ProcessIdentity};
use jefe::persistence::{PersistenceManager, Settings};
#[cfg(windows)]
use jefe::runtime::MultiplexerPlan;
use jefe::runtime::{
    ProcessLiveness, RuntimeError, RuntimeManager, TmuxRuntimeManager, pid_alive,
    platform_engine_diagnostic, process_liveness,
};
use jefe::state::AppState;
use jefe::theme::ThemeManager;

use crate::app_input::{SharedContext, durable_save_request, schedule_durable_save};

fn launch_signature_for_agent(
    agent: &Agent,
    repository: &jefe::domain::Repository,
) -> AgentLaunchRequest {
    AgentLaunchRequest::for_agent(agent, repository)
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

fn normalize_persisted_sandbox_engines(_state: &mut AppState) -> bool {
    false
}

fn classify_agent_startup(
    agent: &Agent,
    repository: &jefe::domain::Repository,
    signature: &AgentLaunchRequest,
    runtime: &TmuxRuntimeManager,
) -> StartupClassification {
    let session = runtime
        .session_liveness_for_signature(&agent.id, signature)
        .into();
    let durable_signature = durable_signature_evidence(agent, repository);
    let binding = binding_evidence(
        agent.runtime_binding.as_ref(),
        &agent.id,
        signature,
        agent.persisted_launch_signature.as_ref(),
        durable_signature,
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
            let startup = match crate::app_input::observe_startup_agent_availability(
                &registry,
                &repository_root,
                state.agent_probe_generation,
                |type_id| agent_type_enabled(settings, type_id),
            ) {
                Ok(startup) => startup,
                Err(error) => {
                    append_warning(state, error);
                    return Vec::new();
                }
            };
            state.agent_probe_generation = startup.latest_generation;
            state.available_agent_type_ids =
                jefe::agent_detection::compatible_agent_type_ids(&startup.observations);
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
    /// Agent was revived/reattached with the runtime's authoritative binding.
    Revived(jefe::domain::RuntimeBinding),
    /// Agent should be marked Dead (binding cleared).
    Dead,
    /// Agent should be left as-is (non-running, or local orphan kept Running).
    Skip,
}
struct RevivedAgent {
    agent_id: AgentId,
    binding: jefe::domain::RuntimeBinding,
}

/// Process one agent during restore: decide Dead / Skip / Revive and, when
/// reviving, drive the runtime and capture the worker PID.
fn restore_one_agent(
    agent: &Agent,
    repositories: &[jefe::domain::Repository],
    runtime: &mut TmuxRuntimeManager,
    runtime_warning: Option<&String>,
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

    match classify_agent_startup(agent, &repository, &signature, runtime) {
        StartupClassification::Stopped
        | StartupClassification::Stale
        | StartupClassification::Inconsistent
        | StartupClassification::Orphaned => RestoreOneOutcome::Dead,
        StartupClassification::Recoverable => RestoreOneOutcome::Skip,
        StartupClassification::DefinitionDrift => {
            let Some(persisted_signature) = agent.persisted_launch_signature.clone() else {
                return RestoreOneOutcome::Dead;
            };
            match runtime.register_existing_local_session(
                &agent.id,
                &agent.work_dir,
                persisted_signature,
            ) {
                Ok(binding) => RestoreOneOutcome::Revived(binding),
                Err(error) => {
                    warn!(agent_id = %agent.id.0, error = %error, "could not register existing session");
                    RestoreOneOutcome::Dead
                }
            }
        }
        StartupClassification::Running => {
            match revive_agent_session(agent, &signature, runtime, runtime_warning) {
                ReviveOutcome::Revived => {
                    let Ok(launch_signature) =
                        jefe::runtime::launch_compose::launch_signature_from_request(&signature)
                    else {
                        return RestoreOneOutcome::Dead;
                    };
                    runtime
                        .runtime_binding(&agent.id, launch_signature)
                        .map_or(RestoreOneOutcome::Dead, RestoreOneOutcome::Revived)
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

    let mut revived_running = Vec::new();
    let mut newly_dead = Vec::new();
    let runtime_warning: Option<String> = None;

    for agent in agents {
        match restore_one_agent(
            &agent,
            &repositories,
            &mut ctx_guard.runtime,
            runtime_warning.as_ref(),
        ) {
            RestoreOneOutcome::Revived(binding) => {
                revived_running.push(RevivedAgent {
                    agent_id: agent.id.clone(),
                    binding,
                });
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
/// in-memory map; `AlreadyRunning` means the session is already tracked. The
/// launch authority is built through the same authorized-preparation contract
/// (`observe_launch_state` + `prepare_launch`) used by the relaunch and
/// fresh-send routes — no bypass helper forges a plan.
fn revive_agent_session(
    agent: &jefe::domain::Agent,
    signature: &AgentLaunchRequest,
    runtime: &mut TmuxRuntimeManager,
    runtime_warning: Option<&String>,
) -> ReviveOutcome {
    // Derive the launch proof through the authorized-preparation contract.
    // `observe_launch_state` is the state-root entry for routes that do not own
    // AppState (the startup restore path). If preparation cannot supply a
    // proof (e.g. remote restore, whose evidence must be state-owned, or an
    // uninstalled agent), the session is left to be marked Dead by the caller.
    let prepared = match jefe::runtime::launch_compose::observe_launch_state(signature)
        .and_then(|evidence| jefe::runtime::launch_compose::prepare_launch(signature, &evidence))
    {
        Ok(prepared) => prepared,
        Err(error) => {
            warn!(agent_id = %agent.id.0, error = %error, "could not authorize restored session");
            return ReviveOutcome::Died;
        }
    };
    let remote = prepared.remote();
    match runtime.spawn_session(&agent.id, prepared.authorized(), remote) {
        Ok(()) | Err(RuntimeError::AlreadyRunning(_)) => {
            let _ = runtime_warning;
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
    revived_running: Vec<RevivedAgent>,
    newly_dead: Vec<AgentId>,
    runtime_warning: Option<String>,
) {
    for revived in revived_running {
        if let Some(agent) = state
            .agents
            .iter_mut()
            .find(|agent| agent.id == revived.agent_id)
        {
            agent.status = AgentStatus::Running;
            agent.persisted_launch_signature = Some(revived.binding.launch_signature.clone());
            agent.runtime_binding = Some(revived.binding);
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
    use jefe::domain::{Repository, RepositoryId, TypedValue};
    use jefe::runtime::RuntimeSession;

    fn code_puppy_agent_and_repository() -> (Agent, Repository) {
        let repository_id = RepositoryId("repo-model".to_owned());
        let repository = Repository::new(
            repository_id.clone(),
            jefe::domain::shipped_agent_type(1),
            jefe::domain::TypedMap::new(),
            "Model Repo".to_owned(),
            "model-repo".to_owned(),
            std::path::PathBuf::from("/tmp/model-repo"),
        );
        let agent = Agent::new(
            AgentId("agent-model".to_owned()),
            repository_id,
            jefe::domain::shipped_agent_type(1),
            jefe::domain::TypedMap::new(),
            "Model Agent".to_owned(),
            std::path::PathBuf::from("/tmp/model-agent"),
        );
        (agent, repository)
    }

    fn set_string(values: &mut jefe::domain::TypedMap, field: &str, value: &str) {
        jefe::domain::canonical_values::insert_json(
            values,
            field,
            serde_json::Value::String(value.to_owned()),
        )
        .unwrap_or_else(|error| panic!("valid {field} fixture: {error}"));
    }

    #[test]
    fn launch_request_uses_agent_type_values_and_repository_target() {
        let (mut agent, mut repository) = code_puppy_agent_and_repository();
        set_string(
            &mut repository.default_values,
            "model",
            "repo/default-model",
        );
        set_string(&mut agent.values, "model", "agent/model");
        repository.remote.host = "build.example.com".to_owned();

        let request = launch_signature_for_agent(&agent, &repository);

        assert_eq!(request.type_id, agent.type_id);
        assert_eq!(request.values, agent.values);
        assert_eq!(request.work_dir, agent.work_dir);
        assert_eq!(request.remote, repository.remote);
        assert_eq!(
            request.operation,
            jefe::domain::agent_definition::Operation::Resume
        );
        assert_eq!(
            jefe::domain::canonical_values::typed_field(&request.values, "model"),
            Some(&TypedValue::String("agent/model".to_owned()))
        );
    }

    #[test]
    fn launch_request_does_not_dynamically_inherit_repository_values() {
        let (agent, mut repository) = code_puppy_agent_and_repository();
        set_string(
            &mut repository.default_values,
            "model",
            "repo/default-model",
        );

        let request = launch_signature_for_agent(&agent, &repository);

        assert!(jefe::domain::canonical_values::typed_field(&request.values, "model").is_none());
    }

    #[test]
    fn durable_signature_distinguishes_definition_drift_from_value_and_target_changes() {
        let (mut agent, repository) = code_puppy_agent_and_repository();
        let current =
            jefe::state::durable_projection::current_launch_signature(&agent, &repository)
                .unwrap_or_else(|error| panic!("fixture signature must project: {error}"));
        agent.persisted_launch_signature = Some(current.clone());
        assert_eq!(
            durable_signature_evidence(&agent, &repository),
            DurableSignatureEvidence::Match
        );

        let mut previous_definition = current;
        previous_definition.definition_hash =
            jefe::domain::LaunchSignatureV1::default().definition_hash;
        agent.persisted_launch_signature = Some(previous_definition);
        assert_eq!(
            durable_signature_evidence(&agent, &repository),
            DurableSignatureEvidence::DefinitionDrift
        );

        set_string(&mut agent.values, "model", "changed-model");
        assert_eq!(
            durable_signature_evidence(&agent, &repository),
            DurableSignatureEvidence::Inconsistent
        );
        agent.values.clear();
        agent.work_dir = std::path::PathBuf::from("/tmp/changed-target");
        assert_eq!(
            durable_signature_evidence(&agent, &repository),
            DurableSignatureEvidence::Inconsistent
        );
    }

    #[test]
    fn binding_accepts_only_definition_drift_for_the_stable_session() {
        let (mut agent, repository) = code_puppy_agent_and_repository();
        let request = launch_signature_for_agent(&agent, &repository);
        let current = jefe::runtime::launch_compose::launch_signature_from_request(&request)
            .unwrap_or_else(|error| panic!("fixture signature must compose: {error}"));
        let mut previous_definition = current;
        previous_definition.definition_hash =
            jefe::domain::LaunchSignatureV1::default().definition_hash;
        agent.persisted_launch_signature = Some(previous_definition.clone());
        let mut binding = jefe::domain::RuntimeBinding {
            session_name: RuntimeSession::session_name_for(&agent.id),
            launch_signature: previous_definition,
            attached: false,
            last_seen: None,
            pid: None,
            process_identity: None,
            lifecycle_generation: 0,
            worker_identities: Vec::new(),
        };
        let durable = durable_signature_evidence(&agent, &repository);

        assert_eq!(durable, DurableSignatureEvidence::DefinitionDrift);
        assert_eq!(
            binding_evidence(
                Some(&binding),
                &agent.id,
                &request,
                agent.persisted_launch_signature.as_ref(),
                durable,
            ),
            BindingEvidence::DefinitionDrift
        );

        binding.session_name = "jefe-agent-other".to_owned();
        assert_eq!(
            binding_evidence(
                Some(&binding),
                &agent.id,
                &request,
                agent.persisted_launch_signature.as_ref(),
                durable,
            ),
            BindingEvidence::Inconsistent
        );
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
    fn live_session_survives_definition_hash_drift() {
        for liveness in [ProcessLiveness::Alive, ProcessLiveness::MalformedIdentity] {
            assert_eq!(
                classify_startup(
                    SessionEvidence::Alive,
                    BindingEvidence::DefinitionDrift,
                    false,
                    liveness,
                    jefe::runtime::OrphanClassification::NoOrphan,
                ),
                StartupClassification::DefinitionDrift,
                "live local session with definition drift must use reattach-only registration"
            );
        }
        assert_eq!(
            classify_startup(
                SessionEvidence::Alive,
                BindingEvidence::DefinitionDrift,
                false,
                ProcessLiveness::Dead,
                jefe::runtime::OrphanClassification::NoOrphan,
            ),
            StartupClassification::Inconsistent
        );
    }

    #[test]
    fn definition_drift_does_not_override_reused_pid_or_missing_session() {
        assert_eq!(
            classify_startup(
                SessionEvidence::Alive,
                BindingEvidence::DefinitionDrift,
                false,
                ProcessLiveness::ReusedPid,
                jefe::runtime::OrphanClassification::NoOrphan,
            ),
            StartupClassification::Stale
        );
        assert_eq!(
            classify_startup(
                SessionEvidence::Missing,
                BindingEvidence::DefinitionDrift,
                false,
                ProcessLiveness::Alive,
                jefe::runtime::OrphanClassification::NoOrphan,
            ),
            StartupClassification::Inconsistent
        );
    }

    #[test]
    fn remote_definition_drift_is_rejected() {
        assert_eq!(
            classify_startup(
                SessionEvidence::Alive,
                BindingEvidence::DefinitionDrift,
                true,
                ProcessLiveness::MalformedIdentity,
                jefe::runtime::OrphanClassification::NoOrphan,
            ),
            StartupClassification::Inconsistent
        );
    }

    #[test]
    fn live_session_with_inconsistent_binding_is_rejected() {
        assert_eq!(
            classify_startup(
                SessionEvidence::Alive,
                BindingEvidence::Inconsistent,
                false,
                ProcessLiveness::Alive,
                jefe::runtime::OrphanClassification::NoOrphan,
            ),
            StartupClassification::Inconsistent
        );
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
}
