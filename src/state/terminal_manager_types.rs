//! Terminal-manager aggregate state types (issue #361 PR B).
//!
//! Pure projection/selection types for the Terminal Manager screen. Runtime
//! only — never persisted. The manager lists every runtime-inventory shell
//! with its owner agent name, repository name, workdir, status, and a
//! close-only annotation for dead/non-Running owners; the lower pane shows a
//! throttled, read-only preview of the selected shell captured from the
//! multiplexer.

use crate::domain::{AgentId, AgentStatus, RepositoryId};

/// Source that initiated a generation-guarded shell focus request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellFocusOrigin {
    DashboardF10,
    ManagerEnter,
}

/// Read-only projection of a single managed shell for the manager list.
///
/// Built deterministically from `AppState` at the input/render boundary; the
/// reducer never performs I/O. `close_only` is true when the owner agent is
/// not Running so the UI annotates the row and Enter is disabled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedShellRow {
    /// Owner agent id (also the inventory key).
    pub agent_id: AgentId,
    /// Owner agent display name.
    pub agent_name: String,
    /// Repository display name.
    pub repository_name: String,
    /// Repository id (for attach routing).
    pub repository_id: RepositoryId,
    /// Agent workdir absolute path.
    pub work_dir: String,
    /// Owner status string ("Running", "Dead", ...).
    pub status_label: String,
    /// Whether the owner is Running (Enter enabled) or close-only.
    pub running: bool,
    /// Whether the owner is dead/missing (close-only + stale preview).
    pub close_only: bool,
}

/// Pending cross-agent shell focus request (issue #361 PR B).
///
/// Generation-guarded so stale attach confirmations cannot complete a focus
/// for a different owner. Cleared on success, failure, navigation away, or
/// manager exit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingShellFocus {
    /// The agent whose shell we are focusing.
    pub agent_id: AgentId,
    /// Monotonic identity used to reject stale attach results.
    pub generation: u64,
    /// Surface that should present the shell after focus completes.
    pub origin: ShellFocusOrigin,
}

/// Read-only preview payload for the selected shell (issue #361 PR B).
///
/// Captured off the input/render path via a targeted `capture-pane` of
/// `<session>:jefe-shell`. `None` lines mean an empty row; the renderer
/// reduces the capture to fit the preview viewport. Stored as plain text so
/// the reducer stays free of styling side effects.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellPreview {
    /// Captured plain-text lines (may be empty; never creates a viewer).
    pub lines: Vec<String>,
    /// Whether the last capture attempt failed (clears the preview).
    pub failed: bool,
    /// The owner agent id this preview belongs to (for stale rejection).
    pub agent_id: Option<AgentId>,
}

/// Where a focused shell should return when hidden/closed/exited.
///
/// A shell entered from the Terminal Manager returns to the manager; a shell
/// entered from the Dashboard keeps its Dashboard return target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShellReturnTarget {
    #[default]
    Dashboard,
    TerminalManager,
}

/// Aggregate state for the Terminal Manager screen (issue #361 PR B).
///
/// Runtime only — never persisted. Inventory/manager/return state live here;
/// they are populated only after runtime operations succeed and cleared on
/// exit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerminalManagerState {
    /// Whether the manager screen is currently active.
    pub active: bool,
    /// Selected row index in the manager list (clamped to row count).
    pub selected_index: Option<usize>,
    /// Pending cross-agent focus request, if any.
    pub pending_focus: Option<PendingShellFocus>,
    /// Last captured preview for the selected shell.
    pub preview: ShellPreview,
    /// Monotonic generation for the manager session; bumped on enter/exit and
    /// selection-driven capture requests so stale results are rejected.
    pub generation: u64,
    /// Saved agent-mode focus for restoration on exit (mirrors errors/actions).
    pub prior_agent_focus: Option<super::PriorAgentFocus>,
}

impl TerminalManagerState {
    /// Bump the manager generation and return the new value.
    pub fn bump_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }

    /// Clear any pending focus request.
    pub fn clear_pending_focus(&mut self) {
        self.pending_focus = None;
    }
}

/// Format an [`AgentStatus`] as a stable display label.
#[must_use]
pub fn status_label_for(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Running => "Running",
        AgentStatus::Queued => "Queued",
        AgentStatus::Completed => "Completed",
        AgentStatus::Errored => "Errored",
        AgentStatus::Waiting => "Waiting",
        AgentStatus::Paused => "Paused",
        AgentStatus::Dead => "Dead",
    }
}
