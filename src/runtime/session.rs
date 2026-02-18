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

/// Style information for one terminal cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCellStyle {
    /// Foreground color.
    pub fg: iocraft::Color,
    /// Background color.
    pub bg: iocraft::Color,
    /// Bold weight.
    pub bold: bool,
    /// Underline decoration.
    pub underline: bool,
}

/// One renderable terminal cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCell {
    /// Display character.
    pub ch: char,
    /// Cell style.
    pub style: TerminalCellStyle,
}

/// Terminal snapshot data for rendering.
///
/// Represents a frozen, styled view of terminal state at a point in time.
#[derive(Debug, Clone, Default)]
pub struct TerminalSnapshot {
    /// Number of visible rows.
    pub rows: usize,
    /// Number of visible columns.
    pub cols: usize,
    /// Row-major terminal cells.
    pub cells: Vec<Vec<TerminalCell>>,
}

impl TerminalSnapshot {
    /// Build an empty snapshot pre-filled with a base style.
    #[must_use]
    pub fn blank(rows: usize, cols: usize, style: TerminalCellStyle) -> Self {
        let cell = TerminalCell { ch: ' ', style };
        Self {
            rows,
            cols,
            cells: vec![vec![cell; cols]; rows],
        }
    }

    /// Check if the snapshot is empty (no content).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty() || self.cells.iter().all(Vec::is_empty)
    }
}
