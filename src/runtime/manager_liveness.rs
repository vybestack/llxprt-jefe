//! Liveness and session-signature queries for [`TmuxRuntimeManager`].
//!
//! Split out of `manager.rs` to keep that file within the source-size gate.
//! These are read-mostly queries over tracked sessions; they perform no
//! spawn, attach, or teardown work.

use super::commands;
use super::liveness;
use super::manager::{LivenessCheck, TmuxRuntimeManager};
use super::session::RuntimeSession;
use crate::domain::{AgentId, AgentLaunchRequest, RemoteRepositorySettings};

impl TmuxRuntimeManager {
    /// Collect liveness check metadata for all tracked sessions.
    ///
    /// The caller can drop the runtime lock before performing the actual
    /// (potentially blocking) liveness checks, preventing SSH round-trips
    /// from stalling the input/render loop.
    #[must_use]
    pub fn liveness_targets(&self) -> Vec<LivenessCheck> {
        self.sessions
            .iter()
            .map(|(agent_id, session)| LivenessCheck {
                agent_id: agent_id.clone(),
                session_name: session.session_name.clone(),
                remote: session.remote.clone(),
                binding_session_name: Some(session.session_name.clone()),
                lifecycle_generation: session.lifecycle_generation,
            })
            .collect()
    }

    /// Check whether a session exists using explicit launch-signature context.
    #[must_use]
    pub fn session_exists_for_signature(
        &self,
        agent_id: &AgentId,
        signature: &AgentLaunchRequest,
    ) -> bool {
        self.session_liveness_for_signature(agent_id, signature) == liveness::SessionLiveness::Alive
    }

    /// Probe a persisted session without collapsing infrastructure failures into absence.
    #[must_use]
    pub fn session_liveness_for_signature(
        &self,
        agent_id: &AgentId,
        signature: &AgentLaunchRequest,
    ) -> liveness::SessionLiveness {
        let session_name = RuntimeSession::session_name_for(agent_id);
        if signature.remote.enabled {
            match commands::remote_session_exists(&signature.remote, &session_name) {
                Ok(true) => liveness::SessionLiveness::Alive,
                Ok(false) => liveness::SessionLiveness::Missing,
                Err(_) => liveness::SessionLiveness::Unavailable,
            }
        } else {
            liveness::session_liveness(&session_name)
        }
    }

    pub(super) fn session_alive_for_remote(
        agent_id: &AgentId,
        remote: Option<&RemoteRepositorySettings>,
    ) -> liveness::SessionLiveness {
        let session_name = RuntimeSession::session_name_for(agent_id);
        if let Some(remote) = remote {
            match commands::remote_session_exists(remote, &session_name) {
                Ok(true) => liveness::SessionLiveness::Alive,
                Ok(false) => liveness::SessionLiveness::Missing,
                Err(_) => liveness::SessionLiveness::Unavailable,
            }
        } else {
            liveness::session_liveness(&session_name)
        }
    }
}
