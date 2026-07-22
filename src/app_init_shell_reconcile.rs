//! Startup shell-window reconciliation (issue #361 PR A).
//!
//! After runtime sessions restore, Jefe reconciles the multiplexer's
//! ground-truth shell windows against AppState's runtime-only inventory:
//!
//! - **Adopt**: known-agent sessions that host a `jefe-shell` window are
//!   recorded in the shell inventory so F10 can resume them.
//! - **Normalize**: for every adopted known session, select window 0 so the
//!   multiplexer current-window invariant holds (a hidden shell leaves window
//!   0 current).
//! - **Orphan cleanup**: `jefe-shell` windows belonging to *unknown* sessions
//!   (left over from a crashed/killed prior Jefe run) are killed best-effort.
//! - **Warnings only**: probe errors surface as a warning and never mark an
//!   agent Dead (issue #361 invariant: shells never mask agent death).
//!
//! All I/O happens here, at the startup boundary; AppState transitions are
//! deterministic. The pure decision seam (`classify_shell_reconciliation`) is
//! unit-tested without a multiplexer.

use jefe::domain::AgentId;
use jefe::runtime::{RuntimeError, RuntimeManager, RuntimeSession};
use jefe::state::AppState;
use tracing::{debug, warn};

use crate::app_input::SharedContext;

use iocraft::hooks::State as HookState;

/// Outcome of classifying one observed shell-window owner session against the
/// set of known agent sessions (issue #361). Pure: no I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ShellReconciliation {
    /// The session belongs to a known agent → adopt into inventory + select
    /// window 0.
    Adopt(AgentId),
    /// The session is unknown (orphan from a prior run) → kill its
    /// `jefe-shell` window.
    Orphan,
}

/// Classify an observed `jefe-shell`-owning session against the known
/// agent→session map (issue #361). Pure decision seam, unit-tested.
///
/// Returns `Adopt(agent_id)` when `observed_session` matches a known agent's
/// session name, otherwise `Orphan`.
#[must_use]
pub(super) fn classify_shell_reconciliation(
    observed_session: &str,
    known_sessions: &[(AgentId, String)],
) -> ShellReconciliation {
    for (agent_id, session_name) in known_sessions {
        if session_name == observed_session {
            return ShellReconciliation::Adopt(agent_id.clone());
        }
    }
    ShellReconciliation::Orphan
}

/// Reconcile runtime shell windows against AppState after sessions restore
/// (issue #361 PR A).
///
/// Drives the runtime boundary (observe + select-window-0 + kill-window) and
/// applies deterministic AppState transitions (record inventory). Probe
/// failures surface as a warning via the returned string and never mark an
/// agent Dead. Returns the warning (if any) so the caller can append it to
/// AppState and persist.
fn classify_observed_sessions(
    observed_sessions: &[String],
    known_sessions: &[(AgentId, String)],
) -> (Vec<AgentId>, Vec<String>) {
    let mut adopted = Vec::new();
    let mut orphans = Vec::new();
    for session in observed_sessions {
        match classify_shell_reconciliation(session, known_sessions) {
            ShellReconciliation::Adopt(agent_id) => adopted.push(agent_id),
            ShellReconciliation::Orphan => orphans.push(session.clone()),
        }
    }
    (adopted, orphans)
}

pub fn reconcile_shell_inventory(
    app_state: &mut HookState<AppState>,
    ctx: &SharedContext,
) -> Option<String> {
    let Some(ctx_arc) = ctx else {
        return None;
    };

    // Every known local agent has a deterministic session name, including a
    // naturally dead owner whose still-live shell must remain adoptable.
    let known_sessions: Vec<(AgentId, String)> = {
        let state = app_state.read();
        state
            .agents
            .iter()
            .filter(|agent| {
                state
                    .repository_for_agent(&agent.id)
                    .is_some_and(|repository| !repository.remote.enabled)
            })
            .map(|agent| {
                (
                    agent.id.clone(),
                    RuntimeSession::session_name_for(&agent.id),
                )
            })
            .collect()
    };

    // Observe every session hosting a jefe-shell window (batched). Returns
    // raw session names so orphans are discovered.
    let observed_sessions: Vec<String> = {
        let Ok(ctx_guard) = ctx_arc.lock() else {
            warn!("startup shell reconcile: runtime context mutex poisoned");
            return Some(
                "could not reconcile shell windows: runtime context unavailable".to_owned(),
            );
        };
        match ctx_guard.runtime.observe_shell_window_sessions() {
            Ok(sessions) => sessions,
            Err(error) => {
                // Probe failure: warn and do NOT touch inventory or agent
                // status (issue #361 invariant).
                warn!(error = %error, "startup shell reconcile: observation failed");
                return Some(format!("could not reconcile shell windows: {error}"));
            }
        }
    };

    let (adopted, orphans) = classify_observed_sessions(&observed_sessions, &known_sessions);

    let mut warnings: Vec<String> = Vec::new();

    // Drive runtime side effects for adoption (select window 0) and orphan
    // cleanup (kill jefe-shell). Failures are best-effort and surface as
    // warnings, never as agent death.
    if !adopted.is_empty() || !orphans.is_empty() {
        for agent_id in &adopted {
            let session_name = RuntimeSession::session_name_for(agent_id);
            if let Err(error) = jefe::runtime::hide_shell_window(&session_name) {
                warn!(agent_id = %agent_id.0, error = %error, "startup shell reconcile: select window 0 failed");
                warnings.push(format!(
                    "could not normalize shell window for {}: {error}",
                    agent_id.0
                ));
            }
        }
        reconcile_orphans(&orphans, &mut warnings);
    }

    // Inventory is runtime-only and must never trigger persisted-state writes.
    app_state.write().replace_shell_inventory(adopted);

    (!warnings.is_empty()).then(|| warnings.join("; "))
}
fn reconcile_orphans(orphans: &[String], warnings: &mut Vec<String>) {
    for orphan in orphans {
        match jefe::runtime::close_shell_window(orphan) {
            Ok(()) | Err(RuntimeError::SessionNotFound(_)) => {}
            Err(error @ RuntimeError::KillFailed(_)) => {
                debug!(session = %orphan, error = %error, "startup shell reconcile: orphan already absent or could not be killed");
            }
            Err(error) => {
                warn!(session = %orphan, error = %error, "startup shell reconcile: orphan kill failed");
                warnings.push(format!(
                    "could not remove orphan shell in {orphan}: {error}"
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(pairs: &[(&str, &str)]) -> Vec<(AgentId, String)> {
        pairs
            .iter()
            .map(|(id, session)| (AgentId((*id).to_owned()), (*session).to_owned()))
            .collect()
    }

    #[test]
    fn classify_adopts_known_session() {
        let known = known(&[("agent-1", "jefe-agent-1")]);
        let decision = classify_shell_reconciliation("jefe-agent-1", &known);
        assert_eq!(
            decision,
            ShellReconciliation::Adopt(AgentId("agent-1".into()))
        );
    }

    #[test]
    fn classify_marks_unknown_session_as_orphan() {
        let known = known(&[("agent-1", "jefe-agent-1")]);
        let decision = classify_shell_reconciliation("jefe-agent-ghost", &known);
        assert_eq!(decision, ShellReconciliation::Orphan);
    }

    #[test]
    fn classify_does_not_match_partial_session_name() {
        let known = known(&[("agent-1", "jefe-agent-1")]);
        // Prefix must not match.
        let decision = classify_shell_reconciliation("jefe-agent", &known);
        assert_eq!(decision, ShellReconciliation::Orphan);
    }

    #[test]
    fn classify_adopts_when_multiple_known_sessions() {
        let known = known(&[("agent-1", "jefe-agent-1"), ("agent-2", "jefe-agent-2")]);
        assert_eq!(
            classify_shell_reconciliation("jefe-agent-2", &known),
            ShellReconciliation::Adopt(AgentId("agent-2".into()))
        );
        assert_eq!(
            classify_shell_reconciliation("jefe-agent-1", &known),
            ShellReconciliation::Adopt(AgentId("agent-1".into()))
        );
    }
}
