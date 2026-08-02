//! One-time application startup: state hydration and runtime session restore.

#[path = "app_init_orphan_reconcile.rs"]
mod orphan_reconcile;
#[path = "app_init_shell_reconcile.rs"]
mod shell_reconcile;
#[path = "app_init_signature_reconcile.rs"]
mod signature_reconcile;

#[cfg(test)]
use self::signature_reconcile::BindingEvidence;
use self::signature_reconcile::{
    SessionEvidence, StartupClassification, binding_evidence, classify_startup,
};
use iocraft::hooks::State as HookState;
use tracing::warn;

use jefe::domain::liveness_observation::{
    Observed, ProbeBoundary, RetryPolicy, Uncertainty, retry_observation,
};
use jefe::domain::{Agent, AgentId, AgentLaunchRequest, AgentStatus, WorkerProcessIdentity};
use jefe::persistence::{PersistenceManager, Settings};
#[cfg(windows)]
use jefe::runtime::MultiplexerPlan;
use jefe::runtime::{
    ProcessLiveness, RuntimeError, RuntimeManager, SessionLiveness, TmuxRuntimeManager,
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

/// Compose the startup diagnostic from what qualification and provenance found.
///
/// Kept free of I/O so both verdicts can be exercised directly. Both are
/// reported when both fail: showing only the first would send the operator
/// round the loop twice (issue #540).
///
/// Windows-only because the psmux contract is: other platforms run tmux and
/// have nothing to qualify against it.
#[cfg(windows)]
fn startup_multiplexer_warning(
    qualification: &jefe::runtime::MultiplexerQualification,
    provenance: Option<&jefe::runtime::ProvenanceVerdict>,
) -> Option<String> {
    let mut problems: Vec<&str> = Vec::new();

    if let jefe::runtime::MultiplexerQualification::Refused { message } = qualification {
        problems.push(message);
    }
    if let Some(
        jefe::runtime::ProvenanceVerdict::Unqualified { diagnostic }
        | jefe::runtime::ProvenanceVerdict::Changed { diagnostic },
    ) = provenance
    {
        problems.push(diagnostic);
    }

    (!problems.is_empty()).then(|| {
        problems.join(
            "

",
        )
    })
}

#[cfg(windows)]
fn windows_multiplexer_startup_warning() -> Option<String> {
    // The version gate used to run at first `new-session`, so an unusable
    // multiplexer was discovered only when starting an agent. Version,
    // conformance and provenance are all settled here instead (issue #540).
    let plan = match MultiplexerPlan::current() {
        Ok(plan) => plan,
        Err(error) => {
            warn!(error = %error, "native Windows multiplexer could not be resolved");
            return Some(format!("psmux preflight warning: {error}"));
        }
    };

    let qualification = jefe::runtime::qualify_multiplexer_for_startup(&plan);
    let provenance = jefe::runtime::fingerprint_multiplexer(&plan).map(|fingerprint| {
        // No manifest ships yet, so every binary is unrecognised. Verifying
        // against an empty manifest would refuse every install, so provenance
        // reports only once digests are recorded; the fingerprint is taken
        // regardless so a mid-session replacement can still be detected.
        let manifest = jefe::runtime::ProvenanceManifest::default();
        if manifest.is_empty() {
            jefe::runtime::ProvenanceVerdict::Qualified
        } else {
            manifest.verify(&fingerprint)
        }
    });

    match startup_multiplexer_warning(&qualification, provenance.as_ref()) {
        None => {
            tracing::info!("native Windows multiplexer qualified at startup");
            None
        }
        Some(warning) => {
            warn!(warning = %warning, "native Windows multiplexer qualification failed");
            Some(warning)
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

/// Adopt an already-running local session under its launch signature.
///
/// Every failure here is reached *after* the session was observed alive, so
/// none of them is evidence of death.
fn adopt_running_local_agent(
    agent: &Agent,
    signature: &AgentLaunchRequest,
    runtime: &mut TmuxRuntimeManager,
) -> RestoreOneOutcome {
    let launched_with = match signature_for_running_agent(
        agent.persisted_launch_signature.clone(),
        jefe::runtime::launch_compose::launch_signature_from_request(signature).ok(),
    ) {
        Ok(value) => value,
        Err(reason) => return RestoreOneOutcome::Held(reason),
    };
    match runtime.register_existing_local_session(&agent.id, &agent.work_dir, launched_with) {
        Ok(binding) => RestoreOneOutcome::Revived(Box::new(binding)),
        // Failing to record a live session is our bookkeeping failing, not the
        // agent dying.
        Err(error) => {
            warn!(agent_id = %agent.id.0, error = %error, "could not register existing session");
            RestoreOneOutcome::Held(Uncertainty::new(
                ProbeBoundary::SessionExists,
                format!("could not register the live session: {error}"),
            ))
        }
    }
}

/// Revive a running agent's session, holding rather than burying it when the
/// revival succeeds but the bookkeeping around it does not.
fn revive_running_agent(
    agent: &Agent,
    signature: &AgentLaunchRequest,
    runtime: &mut TmuxRuntimeManager,
    runtime_warning: Option<&String>,
) -> RestoreOneOutcome {
    match revive_agent_session(agent, signature, runtime, runtime_warning) {
        ReviveOutcome::Revived => {
            let launch_signature = match signature_for_running_agent(
                None,
                jefe::runtime::launch_compose::launch_signature_from_request(signature).ok(),
            ) {
                Ok(value) => value,
                Err(reason) => return RestoreOneOutcome::Held(reason),
            };
            match runtime.runtime_binding(&agent.id, &launch_signature) {
                Some(binding) => RestoreOneOutcome::Revived(Box::new(binding)),
                None => RestoreOneOutcome::Held(Uncertainty::new(
                    ProbeBoundary::SessionExists,
                    "revived session produced no binding".to_owned(),
                )),
            }
        }
        ReviveOutcome::Died => RestoreOneOutcome::Dead,
    }
}

/// Choose the signature to adopt a *running* agent under.
///
/// Reached only once the session probe has said the agent is alive, which is
/// what makes the failing case interesting: a signature that cannot be
/// composed is a statement about the current configuration, and configuration
/// says nothing about whether a process is running. #527 marked twenty live
/// panes stopped because a hash changed; treating an uncomposable signature as
/// death is the same move, so it yields a hold instead.
///
/// The persisted signature wins when present: it is the one the process was
/// actually launched with, and what the configuration would produce *now* is a
/// statement about the next launch (issue #583).
fn signature_for_running_agent(
    persisted: Option<jefe::domain::LaunchSignatureV1>,
    composed: Option<jefe::domain::LaunchSignatureV1>,
) -> Result<jefe::domain::LaunchSignatureV1, Uncertainty> {
    persisted.or(composed).ok_or_else(|| {
        Uncertainty::new(
            ProbeBoundary::LaunchSignature,
            "no persisted signature and the current configuration composes none".to_owned(),
        )
    })
}

/// Ask the session probe again before accepting that it cannot answer.
///
/// #537 was a single transient subprocess failure at cold start that stranded
/// a live agent. Holding instead of guessing stops that becoming a false
/// death, but a hold is still the wrong answer when the probe would have
/// succeeded a moment later, so `Unavailable` is retried before it is believed.
///
/// Only `Unavailable` is retried. `Missing` is an answer -- the session really
/// is gone -- and re-asking a question that was already answered would delay
/// every genuinely dead agent at startup for nothing.
fn retry_session_evidence(
    policy: RetryPolicy,
    mut probe: impl FnMut() -> SessionLiveness,
) -> SessionEvidence {
    let observed = retry_observation(
        policy,
        || match probe() {
            SessionLiveness::Unavailable => Observed::unknown(
                ProbeBoundary::SessionExists,
                "session probe did not answer".to_owned(),
            ),
            answered => Observed::Known(answered),
        },
        std::thread::sleep,
    );
    observed
        .known()
        .copied()
        .map_or(SessionEvidence::Unavailable, SessionEvidence::from)
}

fn classify_agent_startup(
    agent: &Agent,
    signature: &AgentLaunchRequest,
    runtime: &TmuxRuntimeManager,
) -> StartupClassification {
    let session = retry_session_evidence(RetryPolicy::default(), || {
        runtime.session_liveness_for_signature(&agent.id, signature)
    });
    let binding = binding_evidence(agent.runtime_binding.as_ref(), &agent.id);
    let process = if signature.remote.enabled {
        ProcessLiveness::MalformedIdentity
    } else {
        // Startup classification asks whether the *agent* is still running, so
        // it is anchored on the worker identity. Where the worker cannot be
        // identified the answer is "unknown", never the pane's answer (#543).
        process_liveness_for_binding(
            agent
                .runtime_binding
                .as_ref()
                .and_then(|value| value.worker_identity),
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

fn process_liveness_for_binding(worker: Option<WorkerProcessIdentity>) -> ProcessLiveness {
    let Some(worker) = worker else {
        return ProcessLiveness::MalformedIdentity;
    };
    // A creation token lets the probe reject PID reuse. Without one the same
    // probe still answers, it just cannot rule reuse out -- so there is no
    // reason to ask a narrower question. Routing the token-less case through a
    // `bool` used to launder `Inaccessible` and `ProbeFailure` into a positive
    // `Alive`, which meant identical uncertainty was held when a token was
    // present and asserted as liveness when it was not (issue #541).
    process_liveness(Some(worker.identity()))
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
    state.screen = persisted.screen;
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
    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        return Vec::new();
    };

    let settings = ctx_guard.persistence.load_settings().unwrap_or_else(|e| {
        warn!(error = %e, "could not load settings, using defaults");
        Settings::default_with_version()
    });

    // A read that fails is not a read that found nothing. Defaulting here is
    // what let #445 turn an unreadable document into an empty one, because the
    // empty result was then projected straight back over the file.
    let (persisted, durable_read_held) = match ctx_guard.persistence.load_durable_state() {
        Ok(value) => (value, None),
        Err(error) => {
            warn!(error = %error, "could not read durable state; holding writes");
            (
                jefe::state::durable_projection::RestoredState::default(),
                Some(format!(
                    "Durable state could not be read ({error}). Agents shown may be incomplete and saving is paused so the existing file is not overwritten."
                )),
            )
        }
    };

    let mut state = app_state.write();
    state.durable_read_held = durable_read_held;
    restore_persisted_state(&mut state, persisted);
    apply_startup_warning(&mut state, multiplexer_warning);
    state.override_agent_theme = settings.override_agent_theme;
    state.rebuild_repository_agent_ids();
    state.normalize_selection_indices();
    let agent_probe_effects = observe_agent_types(&mut state, &ctx_guard.published_settings);
    state.action_registry_snapshot = ctx_guard.keymap_snapshot.take();

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
    crate::app_input::refresh_action_availability(app_state);
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
        match classify_agent_startup(agent, &signature, runtime) {
            StartupClassification::Orphaned => {
                // Dead pane with surviving validated worker descendants
                // (issue #332): reap the orphan tree and remove the stale
                // session before marking Dead. Best-effort, agent-scoped,
                // warn-don't-fail — probe/kill failures never abort startup.
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
    ///
    /// Boxed because the binding now carries a distinct identity per process
    /// role, which makes it far larger than the unit variants (issue #543).
    Revived(Box<jefe::domain::RuntimeBinding>),
    /// Agent should be marked Dead (binding cleared).
    Dead,
    /// Agent should be left as-is (non-running, or local orphan kept Running).
    Skip,
    /// A startup probe did not answer, so the agent keeps everything it was
    /// persisted with and is re-probed later.
    ///
    /// Deliberately not `Skip`: both leave state untouched now, but only this
    /// one records that the question is still open, which is what a deferred
    /// re-probe and the operator-facing state need (issue #541).
    Held(Uncertainty),
}
struct RevivedAgent {
    agent_id: AgentId,
    binding: Box<jefe::domain::RuntimeBinding>,
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

    match classify_agent_startup(agent, &signature, runtime) {
        StartupClassification::Stopped
        | StartupClassification::Stale
        | StartupClassification::Inconsistent
        | StartupClassification::Orphaned => RestoreOneOutcome::Dead,
        StartupClassification::Recoverable => RestoreOneOutcome::Skip,
        StartupClassification::Held => RestoreOneOutcome::Held(Uncertainty::new(
            ProbeBoundary::SessionExists,
            format!(
                "startup could not determine whether session {} is alive",
                agent.id.0
            ),
        )),
        // A live local session means jefe is re-adopting a process it already
        // started. Adoption is proved by session name, liveness and process
        // identity, so it must not re-resolve a version selector or probe an
        // executable: the running process is unaffected by what the
        // configuration says now, and making adoption depend on the current
        // executable is what stranded live agents (issue #583).
        //
        // The binding carries the signature the process was *launched* with,
        // not the one the current configuration would produce.
        StartupClassification::Running if !signature.remote.enabled => {
            adopt_running_local_agent(agent, &signature, runtime)
        }
        StartupClassification::Running => {
            revive_running_agent(agent, &signature, runtime, runtime_warning)
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
            // Left exactly as persisted, including its binding. The deferred
            // re-probe that turns this into an eventual answer is issue #541's
            // next slice; until then the hold is at least visible in the log
            // rather than silently indistinguishable from a healthy skip.
            RestoreOneOutcome::Held(reason) => {
                warn!(
                    agent_id = %agent.id.0,
                    %reason,
                    "startup held this agent: its state was not determined and was left untouched"
                );
            }
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
            agent.runtime_binding = Some(*revived.binding);
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
#[path = "app_init_tests.rs"]
mod tests;
