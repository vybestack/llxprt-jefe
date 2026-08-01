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

use tracing::debug;
#[cfg(windows)]
use tracing::warn;

use crate::app_input::{
    AppStateHandle, SharedContext, durable_save_request, schedule_durable_save,
};
use crate::app_shell_workers::capture_dead_previews;

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

        // Collect local-only check targets for Running and ServerLost agents
        // under the lock, then release it before any subprocess work.
        let (running_targets, lost_ids) = collect_local_targets(&app_state, ctx_arc);
        if running_targets.is_empty() && lost_ids.is_empty() {
            continue;
        }

        #[cfg(windows)]
        {
            handle_windows_cycle(
                &mut app_state,
                &ctx,
                &running_targets,
                &lost_ids,
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

/// Collect local liveness targets plus the ids of agents already `ServerLost`.
///
/// Returns the runtime-supplied targets filtered to local agents whose status
/// is `Running`, and the ids of local agents currently `ServerLost` (so the
/// Windows path can recover them when the server returns).
fn collect_local_targets(
    app_state: &AppStateHandle,
    ctx_arc: &std::sync::Arc<std::sync::Mutex<crate::AppContext>>,
) -> (Vec<jefe::runtime::LivenessCheck>, Vec<AgentId>) {
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

    let running_targets = all_targets
        .into_iter()
        .filter(|target| target.remote.is_none() && running_ids.contains(&target.agent_id))
        .collect::<Vec<_>>();
    (running_targets, lost_ids)
}

/// Windows liveness cycle: probe the shared psmux server identity first, then
/// reconcile per-agent health only on a healthy same-server observation.
#[cfg(windows)]
async fn handle_windows_cycle(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    running_targets: &[jefe::runtime::LivenessCheck],
    lost_ids: &[AgentId],
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

    match observation {
        ServerLivenessObservation::Healthy(current) => {
            *pinned_prior = current.or_else(|| pinned_prior.clone());
            reconcile_healthy_agents(app_state, ctx, running_targets).await;
        }
        ServerLivenessObservation::Gone | ServerLivenessObservation::Replaced(_) => {
            if let ServerLivenessObservation::Replaced(next) = observation {
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
        ServerLivenessObservation::Unavailable => {
            // Probe failure fails open: no agent status change this cycle.
        }
    }
    let _ = lost_ids;
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
    let dead_identities = batch_dead_identities(running_targets).await;
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
async fn batch_dead_identities(targets: &[jefe::runtime::LivenessCheck]) -> Vec<LivenessIdentity> {
    let targets_owned = targets.to_vec();
    smol::unblock(move || jefe::runtime::batch_liveness_check_with_identity(&targets_owned)).await
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
        changed = true;
    }
    if changed {
        let persisted = durable_save_request(&mut state);
        drop(state);
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
    let dead_identities = batch_dead_identities(running_targets).await;
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
            // A lost multiplexer server says nothing about the workers below
            // it, so their fate is explicitly unknown here (issue #543).
            worker: jefe::runtime::WorkerDisposition::Unknown,
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
        let mut state = AppState::default();
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
        let mut state = AppState::default();
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
        let mut state = AppState::default();
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
        let mut state = AppState::default();
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
}
