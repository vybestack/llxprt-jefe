//! Local agent liveness observer (issue #493).
//!
//! Extracted from `app_shell.rs` (which had grown to 997 lines). Owns the
//! slow-poll LOCAL agent liveness loop. The non-Windows path preserves the
//! exact pre-extraction behavior: a two-subprocess batched check offloaded to
//! a background OS thread, with generation-checked `Dead` transitions and
//! durable projection.
//!
//! On Windows the same two-second cadence first probes the shared psmux
//! server identity via [`jefe::runtime::observe_server_liveness`]. A `Healthy`
//! same-server observation permits the existing per-agent `Dead` reconciliation
//! for `Running` targets only. `Gone` and `Replaced` instead transition every
//! affected `Running` agent to [`AgentStatus::ServerLost`] through the reducer
//! (binding and launch signature preserved), without capturing dead previews
//! or saving as stopped. `Unavailable` makes no state change. Genuine
//! per-agent death on a healthy server still follows the existing preview,
//! `Dead`, binding-clear, and durable-save path.

#[cfg(windows)]
use std::cell::RefCell;
use std::collections::HashMap;
#[cfg(windows)]
use std::collections::HashSet;

use tracing::debug;
#[cfg(windows)]
use tracing::info;
use tracing::warn;

use crate::app_input::{
    AppStateHandle, SharedContext, durable_save_request, schedule_durable_save,
};
use crate::app_shell_workers::capture_dead_previews;

use jefe::domain::liveness_observation::Observed;
#[cfg(windows)]
use jefe::domain::liveness_observation::Resolution;
use jefe::domain::{AgentId, AgentStatus};
use jefe::runtime::LivenessIdentity;
#[cfg(windows)]
use jefe::runtime::MultiplexerPlan;
#[cfg(windows)]
use jefe::runtime::ServerIdentity;
#[cfg(windows)]
use jefe::runtime::ServerLivenessObservation;
#[cfg(windows)]
use jefe::runtime::observe_server_liveness;
use jefe::state::AppEvent;
#[cfg(any(windows, test))]
use jefe::state::AppState;
use jefe::state::transition::commit_pure_site;

/// Liveness poll cadence reused from the original inline future.
const LIVENESS_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// Slow-poll LOCAL agent liveness.
///
/// See the module docs for the Windows/non-Windows split. Remote agents are
/// always excluded; their SSH round-trips would starve the executor and remote
/// death is detected lazily on select/attach.
pub async fn run_local_liveness(mut app_state: AppStateHandle, ctx: SharedContext) {
    // Pinned prior server identity (Windows psmux server watchdog).
    #[cfg(windows)]
    let mut pinned_prior: Option<ServerIdentity> = None;
    // Once-per-identity `exit-empty off` guard. Blocking probe work receives
    // owned snapshots and returns the updated value to this future.
    #[cfg(windows)]
    let mut applied_exit_empty: Option<ServerIdentity> = None;

    loop {
        smol::Timer::after(LIVENESS_POLL_INTERVAL).await;

        let Some(ctx_arc) = &ctx else {
            continue;
        };

        // Agents held at startup were never registered with the runtime, so
        // they produce no liveness target and would stay held forever. Ask
        // again before deciding there is nothing to do (issue #541).
        crate::app_init::reattempt_held_agents(&mut app_state, &ctx);

        // Collect local-only check targets for Running and ServerLost agents
        // under the lock, then release it before any subprocess work.
        let (running_targets, lost_targets) = collect_local_targets(&app_state, ctx_arc);
        if running_targets.is_empty() && lost_targets.is_empty() {
            continue;
        }

        #[cfg(windows)]
        {
            handle_windows_cycle(
                &mut app_state,
                &ctx,
                &running_targets,
                &lost_targets,
                &mut pinned_prior,
                &mut applied_exit_empty,
            )
            .await;
        }

        #[cfg(not(windows))]
        {
            handle_unix_cycle(&mut app_state, &ctx, &running_targets).await;
        }
    }
}

/// Collect local liveness targets for `Running` agents and for agents already
/// `ServerLost`.
///
/// Lost agents are returned as full probe targets rather than bare ids because
/// recovery has to *ask* whether their sessions came back. Returning only ids
/// is what left `ServerLost` a one-way trapdoor: there was nothing to probe
/// with, so the recovery path could never be written (issue #541).
fn collect_local_targets(
    app_state: &AppStateHandle,
    ctx_arc: &std::sync::Arc<std::sync::Mutex<crate::AppContext>>,
) -> (
    Vec<jefe::runtime::LivenessCheck>,
    Vec<jefe::runtime::LivenessCheck>,
) {
    let Ok(ctx_guard) = ctx_arc.lock() else {
        return (Vec::new(), Vec::new());
    };
    let state = app_state.read();
    let running_ids: Vec<AgentId> = state
        .agents
        .iter()
        .filter(|agent| agent.status == AgentStatus::Running)
        .map(|agent| agent.id.clone())
        .collect();
    let lost_ids: Vec<AgentId> = state
        .agents
        .iter()
        .filter(|agent| agent.status == AgentStatus::ServerLost)
        .map(|agent| agent.id.clone())
        .collect();
    drop(state);
    let all_targets = ctx_guard.runtime.liveness_targets();
    drop(ctx_guard);

    let mut running_targets = Vec::new();
    let mut lost_targets = Vec::new();
    for target in all_targets {
        if target.remote.is_some() {
            continue;
        }
        if running_ids.contains(&target.agent_id) {
            running_targets.push(target);
        } else if lost_ids.contains(&target.agent_id) {
            lost_targets.push(target);
        }
    }
    (running_targets, lost_targets)
}

/// Windows liveness cycle: probe the shared psmux server identity first, then
/// reconcile per-agent health only on a healthy same-server observation.
#[cfg(windows)]
async fn handle_windows_cycle(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    running_targets: &[jefe::runtime::LivenessCheck],
    lost_targets: &[jefe::runtime::LivenessCheck],
    pinned_prior: &mut Option<ServerIdentity>,
    applied_exit_empty: &mut Option<ServerIdentity>,
) {
    let observation = match MultiplexerPlan::current() {
        Ok(plan) => {
            let prior = pinned_prior.clone();
            let applied = applied_exit_empty.clone();
            let (observation, next_applied) = smol::unblock(move || {
                let cell = RefCell::new(applied);
                let observation = observe_server_liveness(&plan, prior.as_ref(), &cell);
                (observation, cell.into_inner())
            })
            .await;
            *applied_exit_empty = next_applied;
            observation
        }
        Err(error) => {
            warn!(error = %error, "windows liveness: multiplexer plan unavailable");
            return;
        }
    };

    match plan_server_cycle(observation) {
        ServerCycleAction::Reconcile(current) => {
            *pinned_prior = current.or_else(|| pinned_prior.clone());
            reconcile_healthy_agents(app_state, ctx, running_targets).await;
            recover_server_lost_agents(app_state, ctx, lost_targets).await;
        }
        ServerCycleAction::DeclareLost(next) => {
            if let Some(next) = next {
                *pinned_prior = Some(next);
            }
            let state = app_state.read();
            let affected = eligible_for_server_lost(&state, running_targets);
            drop(state);
            if affected.is_empty() {
                return;
            }
            transition_to_server_lost(app_state, ctx, &affected);
        }
        ServerCycleAction::Hold => {}
    }
}

/// What one server observation authorises this liveness cycle to do.
#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq)]
enum ServerCycleAction {
    /// Pin the carried identity, then reconcile healthy agents and recover
    /// previously lost ones.
    Reconcile(Option<ServerIdentity>),
    /// Pin the carried identity, then transition eligible running agents to
    /// [`AgentStatus::ServerLost`].
    DeclareLost(Option<ServerIdentity>),
    /// Make no state change at all, in either direction.
    Hold,
}

/// Decide the cycle's action from the probe's verdict.
///
/// `Hold` covers every observation that is not evidence about agent liveness:
/// an unanswered probe (issue #493), and a conflicting server identity where
/// two servers under one namespace answered inconsistently (issue #664). An
/// unanswered probe is no more evidence that a lost agent recovered than that
/// a live one died, and a process that cannot have replaced the pinned server
/// is no more trustworthy as the current server than silence.
#[cfg(windows)]
fn plan_server_cycle(observation: ServerLivenessObservation) -> ServerCycleAction {
    match observation {
        ServerLivenessObservation::Healthy(current) => ServerCycleAction::Reconcile(current),
        ServerLivenessObservation::Gone => ServerCycleAction::DeclareLost(None),
        ServerLivenessObservation::Replaced(next) => ServerCycleAction::DeclareLost(Some(next)),
        ServerLivenessObservation::Unavailable
        | ServerLivenessObservation::ConflictingIdentity(_) => ServerCycleAction::Hold,
    }
}

#[cfg(windows)]
/// Decide which lost agents the probe says have returned.
///
/// Pure, so the held case is testable rather than only reachable through a
/// real subprocess failure. An agent is a recovery candidate when the probe
/// answered *and* did not name it among the dead: absence from an answered
/// result is evidence, absence from an unanswered one is nothing at all.
fn classify_recovery(
    lost_targets: &[jefe::runtime::LivenessCheck],
    observed: Observed<Vec<LivenessIdentity>>,
) -> Resolution<Vec<AgentId>> {
    observed.resolve(|dead| {
        let dead_ids: HashSet<&AgentId> = dead.iter().map(|identity| &identity.agent_id).collect();
        lost_targets
            .iter()
            .filter(|target| !dead_ids.contains(&target.agent_id))
            .map(|target| target.agent_id.clone())
            .collect()
    })
}

/// Restore agents whose sessions are alive again now the server has returned.
///
/// `ServerLost` was previously a one-way trapdoor: nothing re-examined those
/// agents, so a transient server replacement stranded them until the operator
/// intervened by hand. The live repro on issue #541 hit exactly this -- agents
/// showing `!` while every process was alive.
///
/// Recovery is fail-closed in the same direction as everything else here. Only
/// a probe that *answered* can recover an agent, and only for agents whose
/// binding still matches the one that was probed; anything else leaves the
/// agent lost, because "we could not tell" must not promote a status any more
/// than it may demote one.
#[cfg(windows)]
async fn recover_server_lost_agents(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    lost_targets: &[jefe::runtime::LivenessCheck],
) {
    if lost_targets.is_empty() {
        return;
    }
    let observed = batch_dead_identities(lost_targets).await;
    let recovered = match classify_recovery(lost_targets, observed) {
        Resolution::Transition(ids) => ids,
        Resolution::Hold(reason) => {
            warn!(%reason, "server-lost recovery held: agents left as they were");
            return;
        }
    };
    if recovered.is_empty() {
        return;
    }

    let mut state = app_state.write();
    let mut changed = 0usize;
    for target in lost_targets
        .iter()
        .filter(|target| recovered.contains(&target.agent_id))
    {
        let still_lost_and_bound = state.agents.iter().any(|agent| {
            agent.id == target.agent_id
                && agent.status == AgentStatus::ServerLost
                && agent.runtime_binding.as_ref().is_some_and(|binding| {
                    Some(binding.session_name.as_str()) == target.binding_session_name.as_deref()
                        && binding.lifecycle_generation == target.lifecycle_generation
                })
        });
        if !still_lost_and_bound {
            continue;
        }
        commit_pure_site(
            &mut state,
            AppEvent::AgentStatusChanged(target.agent_id.clone(), AgentStatus::Running).into(),
        );
        changed = changed.saturating_add(1);
    }
    if changed > 0 {
        info!(count = changed, "server returned: agents recovered");
        let persisted = durable_save_request(&mut state);
        drop(state);
        schedule_durable_save(ctx, persisted);
    } else {
        drop(state);
    }
}

/// Non-Windows liveness cycle: preserve the exact pre-extraction batched
/// behavior. No server probe and no `ServerLost` transition.
#[cfg(not(windows))]
async fn handle_unix_cycle(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    running_targets: &[jefe::runtime::LivenessCheck],
) {
    if running_targets.is_empty() {
        return;
    }
    let dead_identities = answered_dead_identities(batch_dead_identities(running_targets).await);
    if dead_identities.is_empty() {
        return;
    }
    debug!(
        count = dead_identities.len(),
        "liveness poll found dead agents"
    );
    apply_dead_identities(app_state, ctx, dead_identities).await;
}

/// Return the dead identity triples for the given targets via a background
/// OS thread so the smol executor stays free for input events (issue #287).
async fn batch_dead_identities(
    targets: &[jefe::runtime::LivenessCheck],
) -> Observed<Vec<LivenessIdentity>> {
    let targets_owned = targets.to_vec();
    smol::unblock(move || jefe::runtime::batch_liveness_check_with_identity(&targets_owned)).await
}

/// The dead identities from an answered poll, or none when the multiplexer
/// could not answer.
///
/// A held poll leaves every agent exactly as it was. The reason is logged
/// rather than dropped, because an invisible hold is indistinguishable to the
/// operator from the fail-open bug this replaces (issue #541).
fn answered_dead_identities(observed: Observed<Vec<LivenessIdentity>>) -> Vec<LivenessIdentity> {
    match observed {
        Observed::Known(dead) => dead,
        Observed::Unknown(reason) => {
            warn!(%reason, "liveness poll held: no agent state was changed");
            Vec::new()
        }
    }
}

/// Capture previews, then commit generation-checked `Dead` transitions,
/// clear bindings, store previews, and coalesce one durable save.
async fn apply_dead_identities(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    dead_identities: Vec<LivenessIdentity>,
) {
    let mut dead_previews: HashMap<_, _> = capture_dead_previews(dead_identities.clone())
        .await
        .into_iter()
        .map(|(identity, lines)| (identity.agent_id, lines))
        .collect();

    let mut state = app_state.write();
    let binding_matches = compute_binding_matches(&state, &dead_identities);
    let mut changed = false;
    let mut revoked = Vec::new();
    for (identity, matches) in dead_identities.iter().zip(binding_matches) {
        if !matches {
            debug!(agent_id = %identity.agent_id.0, "liveness: stale result after preview capture; skipping");
            continue;
        }
        // Issue #543: a dead pane is not a dead agent. Where the pane leader is
        // the session host, a validated worker anchor can outlive its pane, and
        // reporting that as death both lies to the user and drops the binding
        // that still names a live process. Deciding what an unowned live worker
        // should become is the ownership model's call (issue #542); until then
        // this pass refuses to call it death and leaves the agent as it was.
        if identity.worker == jefe::runtime::WorkerDisposition::SurvivedPane {
            tracing::warn!(
                agent_id = %identity.agent_id.0,
                "liveness: pane died but a validated worker anchor is still alive; \
                 not reporting the agent dead"
            );
            continue;
        }
        let preview = dead_previews.remove(&identity.agent_id);
        commit_pure_site(
            &mut state,
            AppEvent::AgentStatusChanged(identity.agent_id.clone(), AgentStatus::Dead).into(),
        );
        if let Some(agent) = state
            .agents
            .iter_mut()
            .find(|agent| agent.id == identity.agent_id)
        {
            // Existing binding clear for the genuine per-agent Dead path is
            // the sole direct mutation allowed outside the reducer.
            agent.runtime_binding = None;
        }
        if let Some(lines) = preview {
            state.store_dead_preview(identity.agent_id.clone(), lines);
        }
        revoked.push(identity.agent_id.clone());
        changed = true;
    }
    if changed {
        let persisted = durable_save_request(&mut state);
        drop(state);
        if let Some(ctx_arc) = ctx
            && let Ok(mut context) = ctx_arc.lock()
        {
            for agent_id in revoked {
                let _ = context.runtime.mark_session_dead(&agent_id);
            }
        }
        schedule_durable_save(ctx, persisted);
    }
}

/// Run the existing per-agent `Dead` reconciliation for healthy-server
/// `Running` targets (issue #493: same-server individual death remains `Dead`).
#[cfg(windows)]
async fn reconcile_healthy_agents(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    running_targets: &[jefe::runtime::LivenessCheck],
) {
    if running_targets.is_empty() {
        return;
    }
    let dead_identities = answered_dead_identities(batch_dead_identities(running_targets).await);
    if dead_identities.is_empty() {
        return;
    }
    debug!(
        count = dead_identities.len(),
        "liveness poll found dead agents"
    );
    apply_dead_identities(app_state, ctx, dead_identities).await;
}

/// Pure helper: which target agent ids are currently `Running` and therefore
/// eligible to transition to `ServerLost` when the shared server is gone or
/// replaced.
///
/// `ServerLost` agents are excluded: they are already in the recoverable
/// state and the reducer must not re-transition them. The runtime target list
/// is the source of truth for which agents still have a tracked session on the
/// lost server.
#[must_use]
#[cfg(any(windows, test))]
fn eligible_for_server_lost(
    state: &AppState,
    targets: &[jefe::runtime::LivenessCheck],
) -> Vec<LivenessIdentity> {
    targets
        .iter()
        .filter(|target| target.remote.is_none())
        .filter(|target| {
            state
                .agents
                .iter()
                .any(|agent| agent.id == target.agent_id && agent.status == AgentStatus::Running)
        })
        .map(|target| LivenessIdentity {
            agent_id: target.agent_id.clone(),
            binding_session_name: target.binding_session_name.clone(),
            lifecycle_generation: target.lifecycle_generation,
            // A lost server says nothing about the workers below it, so each
            // agent's own anchors are asked rather than inheriting the
            // server's fate. Where an agent recorded no anchors the answer is
            // `Unknown`, never "died with the server" (issues #541, #543).
            worker: jefe::runtime::observe_worker_disposition(&target.worker_identities),
        })
        .collect()
}

/// Transition every affected agent to `ServerLost` via the reducer, preserving
/// `runtime_binding` and launch signatures, then coalesce one durable save.
#[cfg(windows)]
fn transition_to_server_lost(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    affected: &[LivenessIdentity],
) {
    let mut state = app_state.write();
    let binding_matches = compute_binding_matches(&state, affected);
    let mut changed = 0usize;
    for (identity, matches) in affected.iter().zip(binding_matches) {
        let still_running = state
            .agents
            .iter()
            .any(|agent| agent.id == identity.agent_id && agent.status == AgentStatus::Running);
        if !matches || !still_running {
            continue;
        }
        // This agent's own worker answered, and it is alive. A server event is
        // not evidence about a process that outlived it, and marking such an
        // agent lost is what let launching agent N+1 strand agents 1..N.
        if identity.worker == jefe::runtime::WorkerDisposition::SurvivedPane {
            warn!(
                agent_id = %identity.agent_id.0,
                "server changed but this agent's worker is alive: leaving it running"
            );
            continue;
        }
        commit_pure_site(
            &mut state,
            AppEvent::AgentStatusChanged(identity.agent_id.clone(), AgentStatus::ServerLost).into(),
        );
        changed = changed.saturating_add(1);
    }
    if changed > 0 {
        warn!(count = changed, "server lost: agents require recovery");
        let persisted = durable_save_request(&mut state);
        drop(state);
        schedule_durable_save(ctx, persisted);
    } else {
        drop(state);
    }
}

/// Pure helper: compute the per-identity binding-match vector used to guard
/// stale `Dead` results after preview capture (issue #301 Phase 4).
fn compute_binding_matches(
    state: &jefe::state::AppState,
    dead_identities: &[LivenessIdentity],
) -> Vec<bool> {
    let current_bindings: HashMap<&AgentId, (&str, u64)> = state
        .agents
        .iter()
        .filter_map(|agent| {
            agent.runtime_binding.as_ref().map(|binding| {
                (
                    &agent.id,
                    (binding.session_name.as_str(), binding.lifecycle_generation),
                )
            })
        })
        .collect();
    dead_identities
        .iter()
        .map(|identity| {
            current_bindings
                .get(&identity.agent_id)
                .is_some_and(|(session_name, generation)| {
                    Some(*session_name) == identity.binding_session_name.as_deref()
                        && *generation == identity.lifecycle_generation
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    use jefe::domain::liveness_observation::{ProbeBoundary, Uncertainty};
    use jefe::domain::{Agent, RemoteRepositorySettings, RepositoryId};
    use jefe::runtime::LivenessCheck;
    use std::path::PathBuf;

    fn fixture_running_agent(id: &str) -> Agent {
        let mut agent = Agent::new(
            AgentId(id.into()),
            RepositoryId("repo".into()),
            jefe::domain::shipped_agent_type(3),
            jefe::domain::TypedMap::new(),
            id.into(),
            PathBuf::from("/tmp"),
        );
        agent.status = AgentStatus::Running;
        agent
    }

    fn fixture_lost_agent(id: &str) -> Agent {
        let mut agent = Agent::new(
            AgentId(id.into()),
            RepositoryId("repo".into()),
            jefe::domain::shipped_agent_type(3),
            jefe::domain::TypedMap::new(),
            id.into(),
            PathBuf::from("/tmp"),
        );
        agent.status = AgentStatus::ServerLost;
        agent
    }

    fn liveness_target(id: &str) -> LivenessCheck {
        LivenessCheck {
            agent_id: AgentId(id.into()),
            session_name: format!("jefe-{id}"),
            remote: None,
            binding_session_name: Some(format!("jefe-{id}")),
            lifecycle_generation: 0,
            worker_identities: Vec::new(),
        }
    }

    /// `eligible_for_server_lost` returns only currently `Running` agents that
    /// appear in the runtime target list. `ServerLost` agents are excluded so
    /// the reducer does not re-transition them.
    #[test]
    fn eligible_excludes_already_lost_agents() {
        let mut state = crate::test_app_state();
        state.agents.push(fixture_running_agent("a"));
        state.agents.push(fixture_lost_agent("b"));
        let targets = vec![liveness_target("a"), liveness_target("b")];

        let eligible = eligible_for_server_lost(&state, &targets);

        assert_eq!(eligible.len(), 1);
        assert_eq!(eligible[0].agent_id, AgentId("a".into()));
    }

    /// Remote targets are never eligible for the local ServerLost path.
    #[test]
    fn eligible_excludes_remote_targets() {
        let mut state = crate::test_app_state();
        state.agents.push(fixture_running_agent("a"));
        let mut remote_target = liveness_target("a");
        remote_target.remote = Some(RemoteRepositorySettings::default());

        let eligible = eligible_for_server_lost(&state, &[remote_target]);

        assert!(eligible.is_empty(), "remote targets must be excluded");
    }

    /// Agents not present in the runtime target list (no tracked session) are
    /// not eligible, even if their status is `Running`.
    #[test]
    fn eligible_excludes_untracked_running_agents() {
        let mut state = crate::test_app_state();
        state.agents.push(fixture_running_agent("a"));
        state.agents.push(fixture_running_agent("b"));
        // Only agent "a" has a tracked target.
        let targets = vec![liveness_target("a")];

        let eligible = eligible_for_server_lost(&state, &targets);

        assert_eq!(eligible.len(), 1);
        assert_eq!(eligible[0].agent_id, AgentId("a".into()));
    }

    #[test]
    fn binding_match_rejects_stale_session_and_generation() {
        let mut state = crate::test_app_state();
        let mut agent = fixture_running_agent("a");
        agent.runtime_binding = Some(jefe::domain::RuntimeBinding {
            session_name: "jefe-current".into(),
            launch_signature: jefe::domain::LaunchSignatureV1::default(),
            attached: false,
            last_seen: None,
            pane_identity: None,
            worker_identity: None,
            lifecycle_generation: 2,
            worker_identities: vec![],
        });
        state.agents.push(agent);
        let stale_session = LivenessIdentity {
            agent_id: AgentId("a".into()),
            binding_session_name: Some("jefe-old".into()),
            lifecycle_generation: 2,
            worker: jefe::runtime::WorkerDisposition::GoneWithPane,
        };
        let stale_generation = LivenessIdentity {
            agent_id: AgentId("a".into()),
            binding_session_name: Some("jefe-current".into()),
            lifecycle_generation: 1,
            worker: jefe::runtime::WorkerDisposition::GoneWithPane,
        };

        assert_eq!(
            compute_binding_matches(&state, &[stale_session, stale_generation]),
            vec![false, false]
        );
    }

    #[cfg(windows)]
    fn server_identity(pid: u32, started_at: u64) -> ServerIdentity {
        ServerIdentity::new(
            jefe::domain::ServerProcessIdentity::new(pid, started_at),
            jefe::runtime::MultiplexerVersion::new(3, 3, 7),
        )
    }

    /// A healthy probe pins the observed server and reconciles agent health.
    #[cfg(windows)]
    #[test]
    fn a_healthy_observation_reconciles_and_pins() {
        let observed = server_identity(656, 100);

        assert_eq!(
            plan_server_cycle(ServerLivenessObservation::Healthy(Some(observed.clone()))),
            ServerCycleAction::Reconcile(Some(observed))
        );
    }

    /// A genuine replacement still declares the old agents lost and re-pins to
    /// the new server, so the #664 guard does not weaken real restart handling.
    #[cfg(windows)]
    #[test]
    fn a_replacement_declares_agents_lost_and_repins() {
        let replacement = server_identity(19948, 200);

        assert_eq!(
            plan_server_cycle(ServerLivenessObservation::Replaced(replacement.clone())),
            ServerCycleAction::DeclareLost(Some(replacement))
        );
    }

    /// A vanished server declares agents lost with nothing new to pin.
    #[cfg(windows)]
    #[test]
    fn a_vanished_server_declares_agents_lost() {
        assert_eq!(
            plan_server_cycle(ServerLivenessObservation::Gone),
            ServerCycleAction::DeclareLost(None)
        );
    }

    /// Issue #664: two servers under one namespace answered the identity probe
    /// inconsistently. That says nothing about whether any agent is alive, so
    /// the cycle holds — it must not declare agents lost and must not re-pin
    /// to the conflicting process, exactly as for an unanswered probe.
    #[cfg(windows)]
    #[test]
    fn a_conflicting_identity_holds_like_an_unanswered_probe() {
        assert_eq!(
            plan_server_cycle(ServerLivenessObservation::ConflictingIdentity(
                server_identity(19948, 100)
            )),
            ServerCycleAction::Hold
        );
        assert_eq!(
            plan_server_cycle(ServerLivenessObservation::Unavailable),
            ServerCycleAction::Hold
        );
    }

    #[cfg(windows)]
    fn dead_identity(id: &str) -> LivenessIdentity {
        LivenessIdentity {
            agent_id: AgentId(id.into()),
            binding_session_name: Some(format!("jefe-{id}")),
            lifecycle_generation: 0,
            worker: jefe::runtime::WorkerDisposition::GoneWithPane,
        }
    }

    /// The live repro on issue #541: agents sat at `ServerLost` while every
    /// process was alive, because nothing ever re-examined them.
    #[cfg(windows)]
    #[test]
    fn a_lost_agent_whose_session_returned_is_recovered() {
        let lost = vec![liveness_target("alpha"), liveness_target("beta")];

        let decision = classify_recovery(&lost, Observed::Known(vec![dead_identity("beta")]));

        let recovered = decision
            .transition()
            .unwrap_or_else(|| panic!("an answered probe must produce a decision"));
        assert_eq!(
            recovered,
            &vec![AgentId("alpha".into())],
            "only the agent absent from the dead list returns"
        );
    }

    /// Recovery is fail-closed in the same direction as everything else: an
    /// unanswered probe is no more evidence that a lost agent came back than
    /// that a live one died.
    #[cfg(windows)]
    #[test]
    fn an_unanswered_probe_recovers_nobody() {
        let lost = vec![liveness_target("alpha")];

        let decision = classify_recovery(
            &lost,
            Observed::unknown(ProbeBoundary::SessionList, "list-sessions failed"),
        );

        assert!(
            decision.transition().is_none(),
            "an unanswered probe must not promote an agent"
        );
        assert_eq!(
            decision.held().map(Uncertainty::boundary),
            Some(ProbeBoundary::SessionList)
        );
    }

    /// The mirror hazard for recovery: an agent the probe still reports dead
    /// must stay lost, or "never demote" would become "always promote".
    #[cfg(windows)]
    #[test]
    fn an_agent_still_reported_dead_is_not_recovered() {
        let lost = vec![liveness_target("alpha")];

        let decision = classify_recovery(&lost, Observed::Known(vec![dead_identity("alpha")]));

        assert_eq!(
            decision.transition(),
            Some(&Vec::new()),
            "a still-dead agent is not a recovery candidate"
        );
    }

    /// V6: a server-level event is judged per agent. An agent whose own worker
    /// is demonstrably alive did not die because the server changed, which is
    /// precisely what let launching agent N+1 strand agents 1..N.
    #[test]
    fn an_agent_whose_worker_is_alive_is_not_eligible_for_server_lost() {
        let mut state = crate::test_app_state();
        state.agents.push(fixture_running_agent("alpha"));
        let mut target = liveness_target("alpha");
        // This test process is, by construction, alive, and capturing its
        // identity gives the start token that proves the PID was not recycled.
        let me = jefe::runtime::capture_process_identity(std::process::id())
            .unwrap_or_else(|error| panic!("this process must be observable: {error}"));
        target.worker_identities = vec![jefe::domain::WorkerProcessIdentity::from_identity(me)];

        let affected = eligible_for_server_lost(&state, &[target]);

        assert_eq!(affected.len(), 1, "the agent is still examined");
        assert_eq!(
            affected[0].worker,
            jefe::runtime::WorkerDisposition::SurvivedPane,
            "a live worker must be observed as having survived the server"
        );
    }

    /// An agent that recorded no anchors cannot answer the question, and the
    /// answer must be `Unknown` rather than "died with the server".
    #[test]
    fn an_agent_without_anchors_reports_an_unknown_worker() {
        let mut state = crate::test_app_state();
        state.agents.push(fixture_running_agent("alpha"));

        let affected = eligible_for_server_lost(&state, &[liveness_target("alpha")]);

        assert_eq!(
            affected[0].worker,
            jefe::runtime::WorkerDisposition::Unknown,
            "no anchors means unknown, never dead"
        );
    }
}
