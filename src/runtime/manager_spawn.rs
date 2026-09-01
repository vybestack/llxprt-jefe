//! Session creation, reattachment probing, and session-binding insertion for
//! the production runtime manager, split out of `manager.rs`.

use std::path::Path;

use super::{TmuxRuntimeManager, commands, liveness};
use tracing::debug;

use crate::domain::agent_definition::AgentLaunchPlan;
use crate::domain::{
    AgentId, PaneProcessIdentity, PaneWorkerTopology, RemoteRepositorySettings,
    worker_identity_from_pane,
};
use crate::runtime::{RuntimeError, RuntimeSession};

impl TmuxRuntimeManager {
    fn kill_before_fresh_spawn(
        allow_reattach: bool,
        remote: Option<&RemoteRepositorySettings>,
        session_name: &str,
    ) {
        if allow_reattach {
            return;
        }
        let result = if let Some(remote) = remote {
            commands::kill_remote_session(remote, session_name)
        } else {
            commands::kill_session(session_name)
        };
        if let Err(error) = result {
            debug!(
                session_name,
                error = %error,
                "force-fresh spawn pre-kill was not clean"
            );
        }
    }

    /// Probe whether a session is alive using the optional remote transport.
    fn create_or_reattach_after_probe(
        agent_id: &AgentId,
        plan: &AgentLaunchPlan,
        remote: Option<&RemoteRepositorySettings>,
        allow_reattach: bool,
        session_name: &str,
        session_host_root: Option<&Path>,
    ) -> Result<bool, RuntimeError> {
        if allow_reattach
            && Self::session_alive_for_remote(agent_id, remote) == liveness::SessionLiveness::Alive
        {
            return Ok(true);
        }
        Self::kill_before_fresh_spawn(allow_reattach, remote, session_name);
        debug!(session_name, "creating new tmux session");
        // Reattach is handled above. Only the create path receives the
        // session-host root, so reattach never stages or replaces a host.
        match commands::create_session(session_name, plan, remote, session_host_root) {
            Ok(()) => Ok(false),
            Err(_error)
                if allow_reattach
                    && Self::session_alive_for_remote(agent_id, remote)
                        == liveness::SessionLiveness::Alive =>
            {
                Ok(true)
            }
            Err(error) => Err(error),
        }
    }

    pub(super) fn spawn_session_internal(
        &mut self,
        agent_id: &AgentId,
        plan: &AgentLaunchPlan,
        remote: Option<&RemoteRepositorySettings>,
        allow_reattach: bool,
        lifecycle_generation: u64,
    ) -> Result<bool, RuntimeError> {
        self.ensure_initial_geometry()?;
        // Last line of defense: reject a duplicate mapping just before the
        // session is inserted. The public entry points check earlier to fail
        // fast before expensive preflight and credential material is created.
        self.ensure_not_running(agent_id)?;

        // Fresh spawn (not reattach): invalidate stale cache (fix #8).
        if !allow_reattach {
            self.history_cache.clear(agent_id);
        }

        let session_name = RuntimeSession::session_name_for(agent_id);

        // Reattach-first behavior is only allowed for restore/startup paths.
        let can_reattach = allow_reattach
            && Self::session_alive_for_remote(agent_id, remote) == liveness::SessionLiveness::Alive;
        let reattached = if can_reattach {
            true
        } else {
            Self::create_or_reattach_after_probe(
                agent_id,
                plan,
                remote,
                allow_reattach,
                &session_name,
                self.session_host_root.as_deref(),
            )?
        };
        if reattached {
            debug!(session_name = %session_name, "reattaching to existing tmux session");
            if let Some(remote) = remote {
                self.ensure_remote_prefix_passthrough(remote, &session_name);
            } else {
                self.ensure_prefix_passthrough(&session_name);
            }
        } else if remote.is_none() {
            self.ensure_clipboard_passthrough(&session_name);
            self.ensure_prefix_passthrough(&session_name);
        }

        // Capture the *pane leader* PID. `#{pane_pid}` reports whatever process
        // the multiplexer put at the head of the pane, which is not the agent on
        // every platform:
        //
        //   * Unix  — jefe launches the agent as the pane's direct command, so
        //             the pane leader and the worker are the same process.
        //   * Windows — the pane runs `pwsh`, which runs the session host, which
        //             spawns the agent (issue #467). The pane leader is then two
        //             hops above the worker.
        //
        // `PaneWorkerTopology` decides which of those holds; the worker identity
        // is only derived from the pane where the platform actually guarantees
        // it. Pane PID is local-only, so it is not queried for remote sessions.
        // Captured on both the reattach and create branches so creation and
        // revival stay symmetric (issue #543).
        let captured_pane_pid = if remote.is_some() {
            None
        } else {
            commands::pane_pid(&session_name)
        };

        // Store/refresh session binding. Bump the lifecycle generation so
        // stale liveness results from a prior binding are rejected (issue
        // #301 Phase 4).
        let mut session = RuntimeSession::new(
            agent_id.clone(),
            session_name,
            plan.clone(),
            remote.cloned(),
        );
        let pane_identity = captured_pane_pid
            .and_then(|pid| super::super::process::capture_process_identity(pid).ok())
            .map(PaneProcessIdentity::from_identity);
        session.pane_identity = pane_identity;
        // Only platforms where the pane leader *is* the agent may promote the
        // pane identity to the worker role. Elsewhere the worker identity stays
        // unknown until the session host reports it (issue #543).
        session.worker_identity = pane_identity
            .and_then(|pane| worker_identity_from_pane(PaneWorkerTopology::current(), pane));
        // Best-effort launch-tree enumeration so a dead-launcher orphan can be
        // reaped PID-reuse-safely later (issue #332).
        session.worker_identities =
            super::super::orphan::capture_worker_identities(captured_pane_pid);
        session.lifecycle_generation = lifecycle_generation;
        self.sessions.insert(agent_id.clone(), session);

        // Remove from dead plans if present.
        let _ = self.dead_plans.pop(agent_id);

        Ok(reattached)
    }
}
