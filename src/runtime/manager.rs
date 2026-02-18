//! Runtime manager trait and implementations.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P06
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @requirement REQ-FUNC-007
//! @pseudocode component-002 lines 01-35

use std::collections::HashMap;
use std::path::Path;

use super::attach::AttachedViewer;
use super::commands;
use super::errors::RuntimeError;
use super::liveness;
use super::session::{RuntimeSession, TerminalCellStyle, TerminalSnapshot};
use crate::domain::{AgentId, LaunchSignature};

/// Runtime manager trait - owns attach/reattach, input forwarding, kill/relaunch.
///
/// This trait defines the boundary between the application layer and the
/// runtime orchestration layer (tmux/PTY). Implementations handle actual
/// process management, PTY I/O, and session lifecycle.
pub trait RuntimeManager: Send {
    /// Spawn a new runtime session for an agent.
    ///
    /// @pseudocode component-002 lines 01-06
    fn spawn_session(
        &mut self,
        agent_id: &AgentId,
        work_dir: &Path,
        signature: &LaunchSignature,
    ) -> Result<(), RuntimeError>;

    /// Attach to an existing session.
    ///
    /// @pseudocode component-002 lines 07-14
    fn attach(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError>;

    /// Detach from the currently attached session.
    fn detach(&mut self) -> Result<(), RuntimeError>;

    /// Kill a running session.
    ///
    /// @pseudocode component-002 lines 21-26
    fn kill(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError>;

    /// Relaunch a dead session using its stored launch signature.
    ///
    /// @pseudocode component-002 lines 27-32
    fn relaunch(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError>;

    /// Check if a session is alive.
    ///
    /// @pseudocode component-002 lines 33-35
    fn is_alive(&self, agent_id: &AgentId) -> bool;

    /// Get terminal snapshot for the currently attached session.
    fn snapshot(&self) -> Option<TerminalSnapshot>;

    /// Forward input bytes to the attached session.
    ///
    /// @pseudocode component-002 lines 15-20
    fn write_input(&mut self, bytes: &[u8]) -> Result<(), RuntimeError>;

    /// Resize the attached terminal.
    fn resize(&mut self, rows: u16, cols: u16) -> Result<(), RuntimeError>;

    /// Get the currently attached agent ID.
    fn attached_agent(&self) -> Option<&AgentId>;

    /// Whether the attached application currently has terminal mouse reporting enabled.
    fn mouse_reporting_active(&self) -> bool;

    /// Get a reference to a session by agent ID.
    fn get_session(&self, agent_id: &AgentId) -> Option<&RuntimeSession>;
}

/// Stub implementation of RuntimeManager for testing.
#[derive(Debug, Default)]
pub struct StubRuntimeManager {
    sessions: Vec<RuntimeSession>,
    attached_index: Option<usize>,
}

impl RuntimeManager for StubRuntimeManager {
    fn spawn_session(
        &mut self,
        agent_id: &AgentId,
        _work_dir: &Path,
        signature: &LaunchSignature,
    ) -> Result<(), RuntimeError> {
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

    fn snapshot(&self) -> Option<TerminalSnapshot> {
        self.attached_index.map(|_| {
            let style = TerminalCellStyle {
                fg: iocraft::Color::Rgb {
                    r: 0x6a,
                    g: 0x99,
                    b: 0x55,
                },
                bg: iocraft::Color::Rgb { r: 0, g: 0, b: 0 },
                bold: false,
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

    fn get_session(&self, agent_id: &AgentId) -> Option<&RuntimeSession> {
        self.sessions.iter().find(|s| &s.agent_id == agent_id)
    }
}

/// Real tmux-based runtime manager.
///
/// @plan PLAN-20260216-FIRSTVERSION-V1.P08
/// @requirement REQ-TECH-004
/// @requirement REQ-FUNC-007
pub struct TmuxRuntimeManager {
    /// Active sessions by agent ID.
    sessions: HashMap<AgentId, RuntimeSession>,
    /// Currently attached viewer (single viewer model).
    viewer: Option<AttachedViewer>,
    /// Agent ID of the currently attached session.
    attached_agent_id: Option<AgentId>,
    /// Dead sessions that can be relaunched (stores signatures).
    dead_signatures: HashMap<AgentId, LaunchSignature>,
    /// Terminal dimensions.
    rows: u16,
    cols: u16,
}

impl TmuxRuntimeManager {
    /// Create a new tmux runtime manager.
    #[must_use]
    pub fn new(rows: u16, cols: u16) -> Self {
        Self {
            sessions: HashMap::new(),
            viewer: None,
            attached_agent_id: None,
            dead_signatures: HashMap::new(),
            rows,
            cols,
        }
    }

    /// Update terminal dimensions.
    pub fn set_size(&mut self, rows: u16, cols: u16) {
        self.rows = rows;
        self.cols = cols;
    }
}

impl RuntimeManager for TmuxRuntimeManager {
    fn spawn_session(
        &mut self,
        agent_id: &AgentId,
        work_dir: &Path,
        signature: &LaunchSignature,
    ) -> Result<(), RuntimeError> {
        // Check for duplicate
        if self.sessions.contains_key(agent_id) {
            return Err(RuntimeError::AlreadyRunning(agent_id.clone()));
        }

        let session_name = RuntimeSession::session_name_for(agent_id);

        // Create tmux session running llxprt
        commands::create_session(&session_name, work_dir, signature)?;

        // Store session
        let session = RuntimeSession::new(agent_id.clone(), session_name, signature.clone());
        self.sessions.insert(agent_id.clone(), session);

        // Remove from dead signatures if present
        self.dead_signatures.remove(agent_id);

        Ok(())
    }

    fn attach(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError> {
        // Check session exists
        if !self.sessions.contains_key(agent_id) {
            return Err(RuntimeError::SessionNotFound(agent_id.0.clone()));
        }

        // Detach current viewer if different
        if self.attached_agent_id.as_ref() != Some(agent_id) {
            // Mark old session as detached
            if let Some(old_id) = self.attached_agent_id.take()
                && let Some(old_session) = self.sessions.get_mut(&old_id)
            {
                old_session.attached = false;
            }

            // Kill old viewer
            if let Some(old_viewer) = self.viewer.take() {
                old_viewer.mark_dead();
            }

            // Get session name for spawning
            let session_name = self.sessions[agent_id].session_name.clone();

            // Spawn new viewer
            let viewer = AttachedViewer::spawn(&session_name, self.rows, self.cols)?;
            self.viewer = Some(viewer);
            self.attached_agent_id = Some(agent_id.clone());
        }

        // Mark new session as attached
        if let Some(session) = self.sessions.get_mut(agent_id) {
            session.attached = true;
        }
        Ok(())
    }

    fn detach(&mut self) -> Result<(), RuntimeError> {
        if let Some(agent_id) = self.attached_agent_id.take()
            && let Some(session) = self.sessions.get_mut(&agent_id)
        {
            session.attached = false;
        }

        if let Some(viewer) = self.viewer.take() {
            viewer.mark_dead();
        }

        Ok(())
    }

    fn kill(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError> {
        let session = self
            .sessions
            .remove(agent_id)
            .ok_or_else(|| RuntimeError::SessionNotFound(agent_id.0.clone()))?;

        // Store signature for relaunch
        self.dead_signatures
            .insert(agent_id.clone(), session.launch_signature.clone());

        // If attached, clear attachment
        if self.attached_agent_id.as_ref() == Some(agent_id) {
            self.attached_agent_id = None;
            if let Some(viewer) = self.viewer.take() {
                viewer.mark_dead();
            }
        }

        // Kill tmux session
        commands::kill_session(&session.session_name)?;

        Ok(())
    }

    fn relaunch(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError> {
        // Check not already running
        if self.sessions.contains_key(agent_id) {
            return Err(RuntimeError::AlreadyRunning(agent_id.clone()));
        }

        // Get stored signature
        let signature = self
            .dead_signatures
            .remove(agent_id)
            .ok_or_else(|| RuntimeError::NotRunning(agent_id.clone()))?;

        // Spawn with stored signature
        self.spawn_session(agent_id, &signature.work_dir.clone(), &signature)?;

        Ok(())
    }

    fn is_alive(&self, agent_id: &AgentId) -> bool {
        if let Some(session) = self.sessions.get(agent_id) {
            liveness::check_session_alive(&session.session_name)
        } else {
            false
        }
    }

    fn snapshot(&self) -> Option<TerminalSnapshot> {
        self.viewer.as_ref().and_then(AttachedViewer::snapshot)
    }

    fn write_input(&mut self, bytes: &[u8]) -> Result<(), RuntimeError> {
        let viewer = self.viewer.as_ref().ok_or(RuntimeError::NoAttachedViewer)?;
        viewer.write_input(bytes)
    }

    fn resize(&mut self, rows: u16, cols: u16) -> Result<(), RuntimeError> {
        self.rows = rows;
        self.cols = cols;

        if let Some(viewer) = &self.viewer {
            viewer.resize(rows, cols)?;
        }

        Ok(())
    }

    fn attached_agent(&self) -> Option<&AgentId> {
        self.attached_agent_id.as_ref()
    }

    fn mouse_reporting_active(&self) -> bool {
        self.viewer
            .as_ref()
            .is_some_and(AttachedViewer::mouse_reporting_active)
    }

    fn get_session(&self, agent_id: &AgentId) -> Option<&RuntimeSession> {
        self.sessions.get(agent_id)
    }
}
