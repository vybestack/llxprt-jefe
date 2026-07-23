//! Stub implementation of [`RuntimeManager`] for tests and the lifecycle
//! integration suites.
//!
//! Extracted from `manager.rs` so that file stays under the source-file size
//! hard limit. The stub owns no PTY/tmux resources: every snapshot is blank,
//! history capture returns `None`, and `is_dirty` is always `false`.

use std::collections::HashSet;
use std::path::Path;

use iocraft::Color;

use super::errors::RuntimeError;
use super::manager::RuntimeManager;
use super::session::{RuntimeSession, TerminalCellStyle, TerminalSnapshot};
use crate::domain::{AgentId, LaunchSignature};

/// Stub implementation of RuntimeManager for testing.
#[derive(Debug, Default)]
pub struct StubRuntimeManager {
    sessions: Vec<RuntimeSession>,
    attached_index: Option<usize>,
    spawn_failure: Option<RuntimeError>,
    attach_failure: Option<RuntimeError>,
    /// Agent IDs whose embedded shell window is currently open (issue #222).
    open_shell_windows: HashSet<AgentId>,
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
        _work_dir: &Path,
        signature: &LaunchSignature,
    ) -> Result<(), RuntimeError> {
        if let Some(error) = &self.spawn_failure {
            return Err(error.clone());
        }
        // Check for duplicate
        if self.sessions.iter().any(|s| &s.agent_id == agent_id) {
            return Err(RuntimeError::AlreadyRunning(agent_id.clone()));
        }

        let session = RuntimeSession::new(
            agent_id.clone(),
            RuntimeSession::session_name_for(agent_id),
            signature.clone(),
        );
        self.sessions.push(session);
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
            Ok(())
        } else {
            Err(RuntimeError::SessionNotFound(agent_id.0.clone()))
        }
    }

    fn relaunch(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError> {
        // Stub: verify agent existed but is dead (removed)
        // In real impl, would respawn using stored LaunchSignature
        if self.sessions.iter().any(|s| &s.agent_id == agent_id) {
            Err(RuntimeError::AlreadyRunning(agent_id.clone()))
        } else {
            // Would need stored signature to relaunch
            Err(RuntimeError::NotRunning(agent_id.clone()))
        }
    }

    fn is_alive(&self, agent_id: &AgentId) -> bool {
        self.sessions.iter().any(|s| &s.agent_id == agent_id)
    }

    fn session_exists(&self, agent_id: &AgentId) -> bool {
        self.sessions.iter().any(|s| &s.agent_id == agent_id)
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
        if session.launch_signature.remote.enabled {
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
    use crate::domain::{LaunchSignature, RemoteRepositorySettings};

    fn local_signature() -> LaunchSignature {
        LaunchSignature {
            work_dir: std::path::PathBuf::from("/tmp/work"),
            profile: String::new(),
            code_puppy_model: String::new(),
            code_puppy_version: String::new(),
            code_puppy_yolo: None,
            code_puppy_quick_resume: false,
            mode_flags: Vec::new(),
            llxprt_debug: String::new(),
            pass_continue: false,
            sandbox_enabled: false,
            sandbox_engine: crate::domain::SandboxEngine::default(),
            sandbox_flags: String::new(),
            remote: RemoteRepositorySettings::default(),
            agent_kind: crate::domain::AgentKind::default(),
            llxprt_version: None,
        }
    }

    fn stub_with_session(agent_id: &AgentId) -> StubRuntimeManager {
        let mut stub = StubRuntimeManager::default();
        stub.spawn_session(
            agent_id,
            std::path::Path::new("/tmp/work"),
            &local_signature(),
        )
        .unwrap_or_else(|e| panic!("spawn: {e}"));
        stub
    }

    #[test]
    fn stub_close_all_shell_windows_clears_tracked_set() {
        let a = AgentId("a".into());
        let b = AgentId("b".into());
        let mut stub = stub_with_session(&a);
        stub.spawn_session(&b, std::path::Path::new("/tmp/work"), &local_signature())
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
