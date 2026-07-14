//! Async attach helpers for `TmuxRuntimeManager` (issue #301 Phase 3).
//!
//! These methods snapshot the minimal inputs needed to build an
//! `AttachedViewer`, build the viewer on a background thread, and install
//! the result under a short lock — never holding the runtime mutex across
//! the external spawn.

use super::attach::AttachedViewer;
use super::commands;
use super::errors::RuntimeError;
use super::manager::{AttachInputs, TmuxRuntimeManager};
use crate::domain::AgentId;

impl TmuxRuntimeManager {
    /// Read cached history for `(agent_id, generation)`.
    #[must_use]
    pub fn history_cache_get(&self, agent_id: &AgentId, generation: u64) -> Option<&Vec<String>> {
        self.history_cache.get(agent_id, generation)
    }

    /// Store a capture result into the history cache (background worker).
    pub fn history_cache_store(
        &mut self,
        agent_id: &AgentId,
        generation: u64,
        lines: Option<Vec<String>>,
    ) {
        self.history_cache.store(agent_id, generation, lines);
    }

    /// Get the fallback (any-generation) cached history for `agent_id`.
    #[must_use]
    pub fn history_cache_fallback(&self, agent_id: &AgentId) -> Option<&Vec<String>> {
        self.history_cache.get_fallback(agent_id)
    }

    /// Snapshot the minimal inputs needed to build an `AttachedViewer`.
    ///
    /// Returns `None` if the agent has no tracked session.
    #[must_use]
    pub fn attach_inputs(&self, agent_id: &AgentId) -> Option<AttachInputs> {
        let session = self.sessions.get(agent_id)?;
        Some(AttachInputs {
            session_name: session.session_name.clone(),
            remote: if session.launch_signature.remote.enabled {
                Some(session.launch_signature.remote.clone())
            } else {
                None
            },
            rows: self.rows,
            cols: self.cols,
        })
    }

    /// Install a pre-built `AttachedViewer` for `agent_id`.
    ///
    /// The caller must have validated that `agent_id` is still the desired
    /// target before calling this. Drops any existing viewer on a background
    /// thread and marks the old session as detached.
    pub fn apply_attach_result(
        &mut self,
        agent_id: &AgentId,
        viewer: AttachedViewer,
    ) -> Result<(), RuntimeError> {
        if !self.sessions.contains_key(agent_id) {
            super::manager::drop_viewer_in_background_pub(&mut std::option::Option::Some(viewer));
            return Err(RuntimeError::SessionNotFound(agent_id.0.clone()));
        }

        if let Some(old_id) = self.attached_agent_id.take()
            && old_id != *agent_id
            && let Some(old_session) = self.sessions.get_mut(&old_id)
        {
            tracing::debug!(old_agent_id = %old_id.0, "apply_attach: detaching previous viewer");
            old_session.attached = false;
        }
        // Drop the old viewer (if any) on a background thread to avoid
        // blocking the caller during AttachedViewer::drop child teardown
        // (~300ms). This covers both the different-agent case (old viewer
        // from a prior attach) and the same-agent re-attach case (issue
        // #301 review: old viewer was previously dropped inline via
        // `self.viewer = Some(viewer)`).
        super::manager::drop_viewer_in_background_pub(&mut self.viewer);

        if !viewer.is_alive() {
            tracing::debug!(agent_id = %agent_id.0, "apply_attach: viewer exited immediately");
            if let Some(session) = self.sessions.get_mut(agent_id) {
                session.attached = false;
            }
            // Drop the dead viewer on a background thread for the same reason.
            let mut viewer_opt = Some(viewer);
            super::manager::drop_viewer_in_background_pub(&mut viewer_opt);
            return Err(RuntimeError::AttachFailed(
                "session viewer terminated before attach completed".to_owned(),
            ));
        }

        self.viewer = Some(viewer);
        self.attached_agent_id = Some(agent_id.clone());

        if let Some(session) = self.sessions.get_mut(agent_id) {
            session.attached = true;
        }
        Ok(())
    }

    /// Build an `AttachedViewer` from `AttachInputs`.
    ///
    /// This is the blocking work that runs on a background OS thread without
    /// holding the `AppContext` lock.
    pub fn build_viewer(inputs: &AttachInputs) -> Result<AttachedViewer, RuntimeError> {
        if let Some(remote) = &inputs.remote {
            let ssh_plan = commands::build_remote_attach_plan(remote, &inputs.session_name)?;
            AttachedViewer::spawn_remote(&inputs.session_name, inputs.rows, inputs.cols, &ssh_plan)
        } else {
            AttachedViewer::spawn(&inputs.session_name, inputs.rows, inputs.cols)
        }
    }
}
