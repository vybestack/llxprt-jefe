//! Runtime-driving half of the startup reclaim pass (issue #585).
//!
//! [`super::reclaim`] decides what a set of observed sessions means; this
//! module is what talks to the runtime about those decisions. Keeping the two
//! apart lets the classification stay pure and testable without a live
//! multiplexer, which is the same split `server_health` and `server_health_io`
//! already use.

use iocraft::hooks::State as HookState;
use tracing::warn;

use jefe::domain::AgentId;
use jefe::state::AppState;

use super::{RevivedAgent, apply_restored_state, launch_signature_for_agent, reclaim};

/// Re-bind live sessions whose records startup left unbound (issue #585).
///
/// Runs after the restore pass has settled, so it sees the bindings restore
/// actually established rather than the ones the document claimed. A record is
/// eligible only when it holds no binding at that point, which excludes both a
/// healthy agent and one deliberately left alone as `Recoverable`.
///
/// Adoption goes through `register_existing_local_session`, so session
/// liveness, pane and worker identity, and PID-reuse rejection all still apply:
/// this pass widens *what may be reconsidered*, never what counts as proof.
/// Nothing here launches, sends, kills, or mutates configuration.
pub(super) fn reclaim_unbound_sessions(
    app_state: &mut HookState<AppState>,
    ctx_arc: &std::sync::Arc<std::sync::Mutex<crate::AppContext>>,
) -> bool {
    let candidates: Vec<(AgentId, String)> = {
        let state = app_state.read();
        state
            .agents
            .iter()
            .filter(|agent| agent.runtime_binding.is_none())
            .filter(|agent| {
                state
                    .repository_for_agent(&agent.id)
                    .is_some_and(|repository| !repository.remote.enabled)
            })
            .map(|agent| (agent.id.clone(), reclaim::expected_session(&agent.id)))
            .collect()
    };

    let observed = jefe::runtime::list_jefe_sessions();
    if observed.is_empty() {
        return false;
    }

    let decisions = reclaim::classify_reclaimable(&observed, &candidates);
    let Some(outcome) = adopt_reclaimable(&*app_state, ctx_arc, decisions) else {
        return false;
    };
    let ReclaimOutcome {
        adopted,
        ambiguous,
        unowned,
        revived,
    } = outcome;

    let report = reclaim::reclaim_report(&adopted, &ambiguous, &unowned);
    if revived.is_empty() && report.is_none() {
        return false;
    }
    let mut state = app_state.write();
    apply_restored_state(&mut state, revived, Vec::new(), report);
    true
}

/// What one reclaim pass established.
struct ReclaimOutcome {
    adopted: Vec<AgentId>,
    ambiguous: Vec<String>,
    unowned: Vec<String>,
    revived: Vec<RevivedAgent>,
}

/// Drive the runtime for each classified session.
///
/// Returns `None` when the runtime lock is unusable, which is the one case
/// where the pass declines entirely rather than reporting a partial result.
fn adopt_reclaimable(
    app_state: &HookState<AppState>,
    ctx_arc: &std::sync::Arc<std::sync::Mutex<crate::AppContext>>,
    decisions: Vec<reclaim::ReclaimDecision>,
) -> Option<ReclaimOutcome> {
    // A poisoned runtime lock means an earlier panic left the runtime in an
    // unknown state, so reclaim declines rather than adopting against it. Say
    // so: skipping silently would make a live agent look abandoned for a reason
    // the user could never see, which is the very failure this pass exists to
    // end.
    let mut ctx_guard = match ctx_arc.lock() {
        Ok(guard) => guard,
        Err(error) => {
            warn!(error = %error, "runtime lock poisoned; skipping session reclaim this startup");
            return None;
        }
    };
    let mut outcome = ReclaimOutcome {
        adopted: Vec::new(),
        ambiguous: Vec::new(),
        unowned: Vec::new(),
        revived: Vec::new(),
    };
    for decision in decisions {
        match decision {
            reclaim::ReclaimDecision::Adopt(agent_id) => {
                let Some((work_dir, signature)) = reclaim_inputs(app_state, &agent_id) else {
                    continue;
                };
                match ctx_guard
                    .runtime
                    .register_existing_local_session(&agent_id, &work_dir, signature)
                {
                    Ok(binding) => {
                        outcome.adopted.push(agent_id.clone());
                        outcome.revived.push(RevivedAgent {
                            agent_id,
                            binding: Box::new(binding),
                        });
                    }
                    Err(error) => {
                        warn!(agent_id = %agent_id.0, error = %error, "could not reclaim live session");
                    }
                }
            }
            reclaim::ReclaimDecision::Ambiguous(session) => outcome.ambiguous.push(session),
            reclaim::ReclaimDecision::Unowned(session) => outcome.unowned.push(session),
        }
    }
    Some(outcome)
}

/// Work directory and launched-with signature for one reclaim candidate.
///
/// The binding carries the signature the process was *launched* with, taken
/// from the persisted record; the current configuration is deliberately not
/// consulted, because it describes the next launch rather than this process
/// (issue #583).
fn reclaim_inputs(
    app_state: &HookState<AppState>,
    agent_id: &AgentId,
) -> Option<(std::path::PathBuf, jefe::domain::LaunchSignatureV1)> {
    let state = app_state.read();
    let inputs = {
        let agent = state.agents.iter().find(|agent| agent.id == *agent_id)?;
        let repository = state.repository_for_agent(agent_id)?;
        let signature = agent.persisted_launch_signature.clone().or_else(|| {
            jefe::runtime::launch_compose::launch_signature_from_request(
                &launch_signature_for_agent(agent, repository),
            )
            .ok()
        })?;
        (agent.work_dir.clone(), signature)
    };
    drop(state);
    Some(inputs)
}
