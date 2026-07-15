//! Stub implementation of [`RuntimeManager`] for tests and the lifecycle
//! integration suites.
//!
//! Extracted from `manager.rs` so that file stays under the source-file size
//! hard limit. The stub owns no PTY/tmux resources: every snapshot is blank,
//! history capture returns `None`, and `is_dirty` is always `false`.

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
}
