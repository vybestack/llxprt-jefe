//! Runtime session identity model.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P06
//! @requirement REQ-TECH-004
//! @pseudocode component-002 lines 01-06

use crate::domain::{AgentId, LaunchSignature};

/// Runtime session binding for an agent.
///
/// Represents the stable identity of a tmux session bound to an agent.
/// The session may be attached (viewer connected) or detached.
#[derive(Debug, Clone)]
pub struct RuntimeSession {
    /// The agent this session belongs to.
    pub agent_id: AgentId,
    /// The tmux session name (e.g., "jefe-{agent_id}").
    pub session_name: String,
    /// Launch configuration for spawn/relaunch.
    pub launch_signature: LaunchSignature,
    /// Whether a viewer is currently attached to this session.
    pub attached: bool,
}

impl RuntimeSession {
    /// Create a new runtime session binding.
    #[must_use]
    pub fn new(agent_id: AgentId, session_name: String, launch_signature: LaunchSignature) -> Self {
        Self {
            agent_id,
            session_name,
            launch_signature,
            attached: false,
        }
    }

    /// Generate a session name from an agent ID.
    #[must_use]
    pub fn session_name_for(agent_id: &AgentId) -> String {
        format!("jefe-{}", agent_id.0)
    }
}

/// Terminal snapshot data for rendering.
///
/// Represents a frozen view of the terminal state at a point in time.
#[derive(Debug, Clone, Default)]
pub struct TerminalSnapshot {
    /// Lines of text, each as a vector of characters.
    pub lines: Vec<Vec<char>>,
    /// Cursor row position (0-indexed).
    pub cursor_row: usize,
    /// Cursor column position (0-indexed).
    pub cursor_col: usize,
    /// Terminal height in rows.
    pub rows: u16,
    /// Terminal width in columns.
    pub cols: u16,
}

impl TerminalSnapshot {
    /// Check if the snapshot is empty (no content).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() || self.lines.iter().all(Vec::is_empty)
    }
}
