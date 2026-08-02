//! Startup reclaim of live sessions that lost their runtime binding.
//!
//! Startup restore walks persisted records and asks whether to believe each
//! one. That treats the durable document as authority and the live system as
//! suspect, which is backwards: tmux is ground truth and the document is a
//! cache of it. A live session that no record claims is not an anomaly to
//! ignore, it is evidence the cache is wrong.
//!
//! Without a reclaim pass, any defect that clears a binding without killing the
//! session strands the agent permanently: `register_existing_local_session` is
//! reachable only for an agent already believed to be running, and nothing
//! observes sessions the document does not already know about. The stranded
//! agent keeps a worktree, an API budget and a tmux session, doing work nobody
//! can see or stop. This module closes that class rather than any one trigger.
//!
//! Matching is deliberately **forward**: an expected session name is computed
//! from each record and compared against what was observed. It is never parsed
//! backwards out of a session name, because
//! [`RuntimeSession::session_name_for`] is lossy — it collapses every character
//! outside `[A-Za-z0-9_-]` to `_`, so two distinct agent ids can produce one
//! session name. Where they do, the match is ambiguous and nothing is adopted.

use jefe::domain::AgentId;
use jefe::runtime::RuntimeSession;

/// What startup should do about one observed live session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ReclaimDecision {
    /// Exactly one record expects this session name; re-bind it.
    Adopt(AgentId),
    /// More than one record expects this session name, so ownership cannot be
    /// established. Adopt nothing and say so.
    Ambiguous(String),
    /// A live jefe session that no record claims. Report it; never kill it,
    /// because the process may be doing real work and this pass has no
    /// authority to decide otherwise.
    Unowned(String),
}

/// Classify every observed session against the records eligible for reclaim.
///
/// `candidates` are the records that startup did **not** already bind, paired
/// with the session name each one expects. Records that restore already revived
/// are excluded by the caller, so a healthy agent is never reconsidered here.
///
/// Deterministic and side-effect free: the caller owns observation and the
/// re-binding that follows.
#[must_use]
pub(super) fn classify_reclaimable(
    observed: &[String],
    candidates: &[(AgentId, String)],
) -> Vec<ReclaimDecision> {
    observed
        .iter()
        .map(|session| {
            let mut matches = candidates
                .iter()
                .filter(|(_, expected)| expected == session)
                .map(|(agent_id, _)| agent_id);
            match (matches.next(), matches.next()) {
                (Some(agent_id), None) => ReclaimDecision::Adopt(agent_id.clone()),
                (Some(_), Some(_)) => ReclaimDecision::Ambiguous(session.clone()),
                (None, _) => ReclaimDecision::Unowned(session.clone()),
            }
        })
        .collect()
}

/// Expected session name for one agent record.
#[must_use]
pub(super) fn expected_session(agent_id: &AgentId) -> String {
    RuntimeSession::session_name_for(agent_id)
}

/// Human-readable summary of a completed reclaim pass.
///
/// Returns `None` when there is nothing worth telling the user, so a normal
/// startup stays silent. Adoption is reported rather than performed quietly:
/// re-binding an agent changes what the dashboard shows and what the user can
/// act on, and that should never be a surprise.
#[must_use]
pub(super) fn reclaim_report(
    adopted: &[AgentId],
    ambiguous: &[String],
    unowned: &[String],
) -> Option<String> {
    let mut parts = Vec::new();
    if !adopted.is_empty() {
        let names = adopted
            .iter()
            .map(|agent_id| agent_id.0.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!(
            "reattached {} still-running agent(s) that had lost their binding: {names}",
            adopted.len()
        ));
    }
    if !ambiguous.is_empty() {
        parts.push(format!(
            "{} live session(s) matched more than one agent and were left alone: {}",
            ambiguous.len(),
            ambiguous.join(", ")
        ));
    }
    if !unowned.is_empty() {
        parts.push(format!(
            "{} live jefe session(s) match no agent and were left running: {}",
            unowned.len(),
            unowned.join(", ")
        ));
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

#[cfg(test)]
#[path = "app_init_reclaim_tests.rs"]
mod tests;
