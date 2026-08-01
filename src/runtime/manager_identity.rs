//! Process-identity accessors for tracked runtime sessions.
//!
//! Split out of `manager.rs` to keep that file within the source-size policy.
//! These accessors are the runtime layer's answer to "which process?", and the
//! whole point of issue #543 is that the question has more than one answer: the
//! pane leader, the agent worker, and the descendant anchors are three distinct
//! things and each has its own accessor here. None of them falls back to
//! another when its own answer is unknown.

use super::{TmuxRuntimeManager, worker_report};
use crate::domain::{AgentId, PaneProcessIdentity, WorkerProcessIdentity};

impl TmuxRuntimeManager {
    /// Return the stored worker PID (the agent OS process) for an agent, if known.
    ///
    /// Bridges the runtime layer to the app/domain layer for the PID-based
    /// liveness fallback. Returns `None` for untracked agents or sessions whose
    /// worker PID is not knowable from the pane leader alone — on Windows the
    /// pane runs the session host and the worker sits below it, so the pane PID
    /// is deliberately *not* substituted here (issue #543).
    #[must_use]
    pub fn worker_pid(&self, agent_id: &AgentId) -> Option<u32> {
        self.worker_process_identity(agent_id)
            .map(WorkerProcessIdentity::pid)
    }

    /// Return the stable worker process identity for restart reconciliation.
    #[must_use]
    pub fn worker_process_identity(&self, agent_id: &AgentId) -> Option<WorkerProcessIdentity> {
        self.sessions
            .get(agent_id)
            .and_then(|session| session.worker_identity)
    }

    /// Adopt the session host's report of the worker it spawned, for sessions
    /// whose worker could not be derived from the pane leader (issue #543).
    ///
    /// This is deliberately pull-based and unbounded in time rather than a
    /// post-spawn wait: the report appears when the host has spawned the
    /// worker, and polling for it against a deadline would turn a scheduling
    /// delay into a false verdict (issue #562). Until the report lands the
    /// worker identity stays *unknown*, which is the honest answer.
    ///
    /// Returns the identity now known for the agent, if any.
    pub fn adopt_reported_worker_identity(
        &mut self,
        agent_id: &AgentId,
    ) -> Option<WorkerProcessIdentity> {
        let session = self.sessions.get(agent_id)?;
        if let Some(known) = session.worker_identity {
            return Some(known);
        }
        let pane = session.pane_identity?;
        let reported = worker_report::worker_identity_from_report(&session.session_name, pane)?;
        let session = self.sessions.get_mut(agent_id)?;
        session.worker_identity = Some(reported);
        Some(reported)
    }

    /// Return the recorded worker descendant anchors for an agent.
    ///
    /// These are the PID-reuse-safe anchors the orphan reaper validates against,
    /// so callers persisting a runtime binding must carry them across rather
    /// than reset them (issue #332, issue #543).
    #[must_use]
    pub fn worker_identities(&self, agent_id: &AgentId) -> Vec<WorkerProcessIdentity> {
        self.sessions
            .get(agent_id)
            .map(|session| session.worker_identities.clone())
            .unwrap_or_default()
    }

    /// Return the pane leader PID for an agent, if known.
    ///
    /// This is the multiplexer's `#{pane_pid}`: the shell or session host that
    /// owns the pane. It is the correct anchor for pane-scoped questions and
    /// the wrong anchor for anything about the agent itself (issue #543).
    #[must_use]
    pub fn pane_pid(&self, agent_id: &AgentId) -> Option<u32> {
        self.pane_process_identity(agent_id)
            .map(PaneProcessIdentity::pid)
    }

    /// Return the stable pane-leader process identity.
    #[must_use]
    pub fn pane_process_identity(&self, agent_id: &AgentId) -> Option<PaneProcessIdentity> {
        self.sessions
            .get(agent_id)
            .and_then(|session| session.pane_identity)
    }
}
