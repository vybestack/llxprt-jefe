//! Stub implementation of [`RuntimeManager`] for tests and the lifecycle
//! integration suites.
//!
//! Extracted from `manager.rs` so that file stays under the source-file size
//! hard limit. The stub owns no PTY/tmux resources: every snapshot is blank,
//! history capture returns `None`, and `is_dirty` is always `false`.

use std::collections::HashSet;

use iocraft::Color;

use super::errors::RuntimeError;
use super::manager::RuntimeManager;
use super::session::{RuntimeSession, TerminalCellStyle, TerminalSnapshot};
use crate::domain::{AgentId, RemoteRepositorySettings};
use crate::runtime::agent_preflight::{AuthorizedLaunchPlan, ProcessSandboxInspector};

/// Stub implementation of RuntimeManager for testing.
#[derive(Debug, Default)]
pub struct StubRuntimeManager {
    sessions: Vec<RuntimeSession>,
    attached_index: Option<usize>,
    spawn_failure: Option<RuntimeError>,
    attach_failure: Option<RuntimeError>,
    /// Agent IDs whose embedded shell window is currently open (issue #222).
    open_shell_windows: HashSet<AgentId>,
    /// Agent IDs killed via `kill`, mirroring the real manager's dead-marker
    /// set: `relaunch` only accepts an agent that was previously running and
    /// then killed, and a successful spawn/relaunch clears the marker.
    dead_agents: HashSet<AgentId>,
}

impl StubRuntimeManager {
    /// Construct a deterministic manager whose spawn boundary returns `error`.
    #[must_use]
    pub fn with_spawn_failure(error: RuntimeError) -> Self {
        Self {
            spawn_failure: Some(error),
            ..Self::default()
        }
    }

    /// Whether this stub is still holding a session record for `agent_id`.
    ///
    /// Named for what it actually answers. The trait used to carry
    /// `is_alive`/`session_exists` returning `bool`, which read as liveness
    /// predicates while really reporting the stub's own bookkeeping -- and a
    /// real implementation cannot answer liveness in two values, because
    /// "the session is gone" and "the multiplexer could not be asked" are
    /// different facts (issue #597).
    #[must_use]
    pub fn has_session_record(&self, agent_id: &AgentId) -> bool {
        self.sessions.iter().any(|s| &s.agent_id == agent_id)
    }

    /// Construct a deterministic manager whose attach boundary returns `error`.
    #[must_use]
    pub fn with_attach_failure(error: RuntimeError) -> Self {
        Self {
            attach_failure: Some(error),
            ..Self::default()
        }
    }
}

impl RuntimeManager for StubRuntimeManager {
    fn spawn_session(
        &mut self,
        agent_id: &AgentId,
        launch: &AuthorizedLaunchPlan,
        remote: Option<&RemoteRepositorySettings>,
    ) -> Result<(), RuntimeError> {
        let cleared = launch
            .prepare_current(&ProcessSandboxInspector::new())
            .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))?;
        if let Some(error) = &self.spawn_failure {
            return Err(error.clone());
        }
        if self.sessions.iter().any(|s| &s.agent_id == agent_id) {
            return Err(RuntimeError::AlreadyRunning(agent_id.clone()));
        }

        let session = RuntimeSession::new(
            agent_id.clone(),
            RuntimeSession::session_name_for(agent_id),
            cleared.plan().clone(),
            remote.cloned(),
        );
        self.sessions.push(session);
        // A successful spawn clears any prior dead marker, mirroring the real
        // manager's `dead_plans` pop-on-spawn behavior.
        self.dead_agents.remove(agent_id);
        Ok(())
    }

    fn attach(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError> {
        if let Some(error) = &self.attach_failure {
            return Err(error.clone());
        }
        if let Some(idx) = self.sessions.iter().position(|s| &s.agent_id == agent_id) {
            // Detach from current if any
            if let Some(prev_idx) = self.attached_index {
                self.sessions[prev_idx].attached = false;
            }
            self.attached_index = Some(idx);
            self.sessions[idx].attached = true;
            Ok(())
        } else {
            Err(RuntimeError::SessionNotFound(agent_id.0.clone()))
        }
    }

    fn detach(&mut self) -> Result<(), RuntimeError> {
        if let Some(idx) = self.attached_index {
            self.sessions[idx].attached = false;
        }
        self.attached_index = None;
        Ok(())
    }

    fn kill(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError> {
        if let Some(idx) = self.sessions.iter().position(|s| &s.agent_id == agent_id) {
            self.sessions.remove(idx);
            self.open_shell_windows.remove(agent_id);
            // Adjust attached_index
            match self.attached_index {
                Some(i) if i == idx => self.attached_index = None,
                Some(i) if i > idx => self.attached_index = Some(i - 1),
                _ => {}
            }
            // Record the killed agent as eligible for relaunch, mirroring the
            // real manager's dead-marker retention.
            self.dead_agents.insert(agent_id.clone());
            Ok(())
        } else {
            Err(RuntimeError::SessionNotFound(agent_id.0.clone()))
        }
    }

    fn relaunch(
        &mut self,
        agent_id: &AgentId,
        launch: &AuthorizedLaunchPlan,
        remote: Option<&RemoteRepositorySettings>,
    ) -> Result<(), RuntimeError> {
        if self.sessions.iter().any(|s| &s.agent_id == agent_id) {
            return Err(RuntimeError::AlreadyRunning(agent_id.clone()));
        }
        // Relaunch requires a prior kill (dead marker), mirroring the real
        // manager's `dead_plans` eligibility check. An agent that was never
        // running cannot be relaunched.
        if !self.dead_agents.contains(agent_id) {
            return Err(RuntimeError::NotRunning(agent_id.clone()));
        }
        self.spawn_session(agent_id, launch, remote)
    }

    fn snapshot(&self) -> Option<TerminalSnapshot> {
        self.attached_index.map(|_| {
            let style = TerminalCellStyle {
                fg: Color::Rgb {
                    r: 0x6a,
                    g: 0x99,
                    b: 0x55,
                },
                bg: Color::Rgb { r: 0, g: 0, b: 0 },
                bold: false,
                dim: false,
                underline: false,
            };
            TerminalSnapshot::blank(1, 1, style)
        })
    }

    fn write_input(&mut self, _bytes: &[u8]) -> Result<(), RuntimeError> {
        if self.attached_index.is_some() {
            Ok(())
        } else {
            Err(RuntimeError::NoAttachedViewer)
        }
    }

    fn resize(&mut self, _rows: u16, _cols: u16) -> Result<(), RuntimeError> {
        if self.attached_index.is_some() {
            Ok(())
        } else {
            Err(RuntimeError::NoAttachedViewer)
        }
    }

    fn attached_agent(&self) -> Option<&AgentId> {
        self.attached_index
            .and_then(|idx| self.sessions.get(idx).map(|s| &s.agent_id))
    }

    fn mouse_reporting_active(&self) -> bool {
        false
    }

    fn bracketed_paste_active(&self) -> bool {
        false
    }

    fn take_dirty(&self) -> bool {
        false
    }

    fn is_dirty(&self) -> bool {
        false
    }

    fn output_generation(&self) -> u64 {
        0
    }

    fn get_session(&self, agent_id: &AgentId) -> Option<&RuntimeSession> {
        self.sessions.iter().find(|s| &s.agent_id == agent_id)
    }

    fn capture_session_output(&self, _agent_id: &AgentId) -> Option<TerminalSnapshot> {
        None
    }

    fn capture_history(&mut self) -> Option<Vec<String>> {
        None
    }

    fn open_shell_window(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError> {
        let session = self
            .sessions
            .iter()
            .find(|s| &s.agent_id == agent_id)
            .ok_or_else(|| RuntimeError::SessionNotFound(agent_id.0.clone()))?;
        if session.remote.is_some() {
            return Err(RuntimeError::SpawnFailed(
                "embedded shell is local-only for remote repositories".to_owned(),
            ));
        }
        self.open_shell_windows.insert(agent_id.clone());
        Ok(())
    }

    fn select_shell_window(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError> {
        if !self.sessions.iter().any(|s| &s.agent_id == agent_id)
            || !self.open_shell_windows.contains(agent_id)
        {
            return Err(RuntimeError::SessionNotFound(agent_id.0.clone()));
        }
        Ok(())
    }

    fn close_shell_window(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError> {
        if !self.sessions.iter().any(|s| &s.agent_id == agent_id) {
            return Err(RuntimeError::SessionNotFound(agent_id.0.clone()));
        }
        self.open_shell_windows.remove(agent_id);
        Ok(())
    }

    fn shell_window_exists(&self, agent_id: &AgentId) -> Result<bool, RuntimeError> {
        if !self.sessions.iter().any(|s| &s.agent_id == agent_id) {
            return Err(RuntimeError::SessionNotFound(agent_id.0.clone()));
        }
        Ok(self.open_shell_windows.contains(agent_id))
    }

    fn hide_shell_window(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError> {
        // The stub models shell-window visibility only as set membership; the
        // real implementation selects window 0. Hiding is a no-op for the
        // stub because the window stays tracked.
        if !self.sessions.iter().any(|s| &s.agent_id == agent_id) {
            return Err(RuntimeError::SessionNotFound(agent_id.0.clone()));
        }
        Ok(())
    }

    fn observe_shell_window_sessions(&self) -> Result<Vec<String>, RuntimeError> {
        Ok(self
            .sessions
            .iter()
            .filter(|session| self.open_shell_windows.contains(&session.agent_id))
            .map(|session| session.session_name.clone())
            .collect())
    }

    fn close_all_shell_windows(&mut self) -> Vec<RuntimeError> {
        self.open_shell_windows.clear();
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::agent_execution_guard::{
        AuthorizationResult, ExecutionEvidence, authorize_execution,
    };
    use crate::runtime::agent_preflight::{
        PreparationOutcome, ProcessSandboxInspector, prepare_execution,
    };

    fn fixture_plan() -> crate::domain::agent_definition::AgentLaunchPlan {
        crate::domain::agent_definition::AgentLaunchPlan {
            target: crate::domain::agent_definition::Target::Remote(
                crate::domain::agent_definition::RemoteTarget {
                    user: "fixture".to_owned(),
                    host: "example.invalid".to_owned(),
                    port: None,
                    run_as_user: String::new(),
                    canonical_cwd: std::path::PathBuf::from("/tmp/work"),
                },
            ),
            cwd: std::path::PathBuf::from("/tmp/work"),
            ..crate::domain::agent_definition::AgentLaunchPlan::default()
        }
    }

    /// Seal a fixture plan into an [`AuthorizedLaunchPlan`] through the real
    /// authorize + preflight proof chain.
    fn authorized(plan: &crate::domain::agent_definition::AgentLaunchPlan) -> AuthorizedLaunchPlan {
        let evidence = ExecutionEvidence::new(
            plan.definition_sha256,
            plan.executable_fingerprint.clone(),
            plan.probe_generation,
            plan.target_generation,
            plan.activation_generation,
        );
        let authorized = match authorize_execution(plan, &evidence) {
            AuthorizationResult::Authorized(authorized) => authorized,
            AuthorizationResult::Rejected(error) => panic!("fixture must authorize: {error}"),
        };
        let cleared = match prepare_execution(authorized, None, &ProcessSandboxInspector::new()) {
            PreparationOutcome::Cleared(cleared) => cleared,
            PreparationOutcome::Unavailable(reason) => {
                panic!("fixture must clear preflight: {reason}")
            }
        };
        AuthorizedLaunchPlan::from_cleared(cleared, plan.clone(), evidence)
            .unwrap_or_else(|error| panic!("fixture must seal: {error}"))
    }

    fn stub_with_session(agent_id: &AgentId) -> StubRuntimeManager {
        let mut stub = StubRuntimeManager::default();
        let plan = authorized(&fixture_plan());
        stub.spawn_session(agent_id, &plan, None)
            .unwrap_or_else(|e| panic!("spawn: {e}"));
        stub
    }

    #[test]
    fn stub_close_all_shell_windows_clears_tracked_set() {
        let a = AgentId("a".into());
        let b = AgentId("b".into());
        let mut stub = stub_with_session(&a);
        let plan = authorized(&fixture_plan());
        stub.spawn_session(&b, &plan, None)
            .unwrap_or_else(|e| panic!("spawn: {e}"));
        stub.open_shell_window(&a)
            .unwrap_or_else(|e| panic!("open shell: {e}"));
        stub.open_shell_window(&b)
            .unwrap_or_else(|e| panic!("open shell: {e}"));
        assert!(
            stub.shell_window_exists(&a)
                .unwrap_or_else(|e| panic!("observe shell: {e}"))
        );

        let failures = stub.close_all_shell_windows();
        assert!(
            failures.is_empty(),
            "best-effort stub cleanup reports no failures"
        );
        assert!(
            !stub
                .shell_window_exists(&a)
                .unwrap_or_else(|e| panic!("observe shell: {e}")),
            "close_all must actually clear the tracked shell set (issue #361)"
        );
    }

    #[test]
    fn stub_hide_shell_window_succeeds_for_known_session() {
        let a = AgentId("a".into());
        let mut stub = stub_with_session(&a);
        stub.open_shell_window(&a)
            .unwrap_or_else(|e| panic!("open shell: {e}"));
        stub.hide_shell_window(&a)
            .unwrap_or_else(|e| panic!("hide: {e}"));
        // Hide keeps the window tracked in the stub model.
        assert!(
            stub.shell_window_exists(&a)
                .unwrap_or_else(|e| panic!("observe shell: {e}"))
        );
    }

    #[test]
    fn stub_select_shell_window_never_creates_a_missing_shell() {
        let agent = AgentId("a".into());
        let mut stub = stub_with_session(&agent);

        assert!(stub.select_shell_window(&agent).is_err());
        assert!(
            !stub
                .shell_window_exists(&agent)
                .unwrap_or_else(|error| panic!("observe shell: {error}"))
        );
        stub.open_shell_window(&agent)
            .unwrap_or_else(|error| panic!("open shell: {error}"));
        stub.select_shell_window(&agent)
            .unwrap_or_else(|error| panic!("select shell: {error}"));
    }

    #[test]
    fn stub_observe_all_shell_window_sessions_returns_session_names() {
        let a = AgentId("a".into());
        let mut stub = stub_with_session(&a);
        stub.open_shell_window(&a)
            .unwrap_or_else(|e| panic!("open shell: {e}"));
        let sessions = stub
            .observe_shell_window_sessions()
            .unwrap_or_else(|e| panic!("observe all: {e}"));
        assert_eq!(
            sessions,
            vec![RuntimeSession::session_name_for(&a)],
            "stub must map open shells to session names for startup reconcile"
        );
    }
}
