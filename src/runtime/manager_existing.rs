//! Registration of already-running local sessions without launch authority.

use std::path::Path;

use super::{TmuxRuntimeManager, commands, liveness};
use crate::domain::{AgentId, LaunchSignatureV1, ProcessIdentity, RuntimeBinding};
use crate::runtime::{RuntimeError, RuntimeSession};

pub(super) struct ExistingLocalSessionObservation {
    pub(super) pid: u32,
    pub(super) process_identity: ProcessIdentity,
    pub(super) worker_identities: Vec<ProcessIdentity>,
}

impl TmuxRuntimeManager {
    /// Register an existing local session without constructing launch
    /// authority or permitting a fresh spawn.
    pub fn register_existing_local_session(
        &mut self,
        agent_id: &AgentId,
        work_dir: &Path,
        launch_signature: LaunchSignatureV1,
    ) -> Result<RuntimeBinding, RuntimeError> {
        if self.sessions.contains_key(agent_id) {
            return Err(RuntimeError::AlreadyRunning(agent_id.clone()));
        }
        let session_name = RuntimeSession::session_name_for(agent_id);
        match liveness::session_liveness(&session_name) {
            liveness::SessionLiveness::Alive => {}
            liveness::SessionLiveness::Missing => {
                return Err(RuntimeError::SessionNotFound(session_name));
            }
            liveness::SessionLiveness::Unavailable => {
                return Err(RuntimeError::CapabilityProbeFailed(format!(
                    "could not verify existing session {session_name}"
                )));
            }
        }
        let pid = commands::pane_pid(&session_name).ok_or_else(|| {
            RuntimeError::CapabilityProbeFailed(format!(
                "could not capture worker pid for existing session {session_name}"
            ))
        })?;
        let process_identity =
            super::super::process::capture_process_identity(pid).map_err(|error| {
                RuntimeError::CapabilityProbeFailed(format!(
                    "could not capture worker identity for existing session {session_name}: {error}"
                ))
            })?;
        let observation = ExistingLocalSessionObservation {
            pid,
            process_identity,
            worker_identities: super::super::orphan::capture_worker_identities(Some(pid)),
        };
        self.ensure_prefix_passthrough(&session_name);
        Ok(self.register_observed_local_session(
            agent_id,
            work_dir,
            launch_signature,
            session_name,
            observation,
        ))
    }

    pub(super) fn register_observed_local_session(
        &mut self,
        agent_id: &AgentId,
        work_dir: &Path,
        launch_signature: LaunchSignatureV1,
        session_name: String,
        observation: ExistingLocalSessionObservation,
    ) -> RuntimeBinding {
        let mut session = RuntimeSession::existing_local(
            agent_id.clone(),
            session_name.clone(),
            work_dir.to_path_buf(),
        );
        session.pid = Some(observation.pid);
        session.process_identity = Some(observation.process_identity);
        session
            .worker_identities
            .clone_from(&observation.worker_identities);
        session.lifecycle_generation = self.next_lifecycle_generation();
        let lifecycle_generation = session.lifecycle_generation;
        self.sessions.insert(agent_id.clone(), session);
        let _ = self.dead_plans.pop(agent_id);

        RuntimeBinding {
            session_name,
            launch_signature,
            attached: false,
            last_seen: None,
            pid: Some(observation.pid),
            process_identity: Some(observation.process_identity),
            lifecycle_generation,
            worker_identities: observation.worker_identities,
        }
    }

    /// Snapshot one tracked session as an authoritative runtime binding.
    #[must_use]
    pub fn runtime_binding(
        &self,
        agent_id: &AgentId,
        launch_signature: LaunchSignatureV1,
    ) -> Option<RuntimeBinding> {
        let session = self.sessions.get(agent_id)?;
        Some(RuntimeBinding {
            session_name: session.session_name.clone(),
            launch_signature,
            attached: session.attached,
            last_seen: None,
            pid: session.pid,
            process_identity: session.process_identity,
            lifecycle_generation: session.lifecycle_generation,
            worker_identities: session.worker_identities.clone(),
        })
    }
}
