//! Runtime manager trait and liveness-check metadata.
//!
//! Extracted from `manager.rs` to keep that file under the source-file size
//! hard limit. Defines the boundary between the application layer and the
//! runtime orchestration layer (tmux/PTY). Implementations handle actual
//! process management, PTY I/O, and session lifecycle.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P06
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @requirement REQ-FUNC-007
//! @pseudocode component-002 lines 01-35

use std::path::Path;

use super::errors::RuntimeError;
use super::session::{RuntimeSession, TerminalSnapshot};
use crate::domain::{AgentId, LaunchSignature, RemoteRepositorySettings};

/// Lightweight metadata for checking session liveness without holding the runtime lock.
///
/// Callers collect these under the lock, drop it, then run the (potentially slow)
/// liveness checks externally — avoiding mutex contention with input/render paths.
#[derive(Clone)]
pub struct LivenessCheck {
    pub agent_id: AgentId,
    pub session_name: String,
    pub remote: Option<RemoteRepositorySettings>,
    pub pid: Option<u32>,
    pub process_identity: Option<crate::domain::ProcessIdentity>,
}

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

    /// Spawn a new runtime session and force a fresh tmux process.
    ///
    /// This bypasses reattach behavior and is used for explicit user relaunch
    /// after kill, so latest config/env values are guaranteed to apply.
    fn spawn_session_fresh(
        &mut self,
        agent_id: &AgentId,
        work_dir: &Path,
        signature: &LaunchSignature,
    ) -> Result<(), RuntimeError> {
        self.spawn_session(agent_id, work_dir, signature)
    }

    /// Spawn a fresh session from an already-prepared launch, killing the
    /// existing session exactly once and reusing the prepared data for the
    /// post-kill spawn (issue #269).
    ///
    /// The caller prepares the [`PreparedLaunch`] once (resolving all
    /// non-destructive prerequisites before the kill). This method kills the
    /// existing session (if any), waits for teardown if required, executes
    /// the SAME prepared data, and stores the runtime session mapping. No
    /// re-resolution or re-prepare occurs — eliminating the double-kill /
    /// double-probe hazard of the old prepare → drop → kill → reprepare path.
    ///
    /// Unlike [`spawn_session_fresh`](Self::spawn_session_fresh), this method
    /// is the ONE path that may REPLACE an existing running mapping for the
    /// agent (the restart/replacement case): it stashes the old mapping,
    /// performs the single kill → delay → spawn transaction, and restores the
    /// old mapping on spawn failure so the caller observes the pre-restart
    /// state.
    fn spawn_prepared_session_fresh(
        &mut self,
        agent_id: &AgentId,
        work_dir: &Path,
        signature: &LaunchSignature,
        prepared: &super::prepared_launch::PreparedLaunch,
    ) -> Result<(), RuntimeError> {
        // Default: ignore the prepared launch and fall back to the standard
        // force-fresh path. Real implementations override this to reuse the
        // prepared data.
        let _ = prepared;
        self.spawn_session_fresh(agent_id, work_dir, signature)
    }

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

    /// Check whether a tmux session exists for the given agent.
    fn session_exists(&self, agent_id: &AgentId) -> bool;

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

    /// Whether the attached application currently has bracketed paste enabled.
    fn bracketed_paste_active(&self) -> bool;

    /// Atomically read and clear the dirty flag on the attached viewer.
    ///
    /// Returns `true` when new PTY data has arrived since the last call,
    /// `false` otherwise. This enables event-driven rendering: the render loop
    /// only triggers a re-render when the terminal content has actually changed,
    /// avoiding wasteful ~30fps renders that block keyboard input processing.
    #[must_use]
    fn take_dirty(&self) -> bool;

    /// Non-consuming check of the dirty flag on the attached viewer (issue #198).
    ///
    /// Returns `true` when new PTY data has arrived since the last
    /// [`take_dirty`](Self::take_dirty), without clearing the flag. Used by the
    /// scrollback history cache to decide whether to re-capture without
    /// stealing the dirty flag out from under the render-decision path.
    #[must_use]
    fn is_dirty(&self) -> bool;

    /// Monotonically increasing generation counter for attached PTY output
    /// (issue #198 review fix).
    ///
    /// Increments when new output arrives on the attached viewer. The
    /// scrollback history cache stores the generation it captured at and
    /// compares it to the *current* generation to decide whether a re-capture
    /// is necessary. This decouples history-cache invalidation from the
    /// render-decision dirty flag (`take_dirty`), which is consumed during the
    /// render decision and therefore always reads `false` later in the same
    /// render frame — causing stale caches when `is_dirty()` was used.
    #[must_use]
    fn output_generation(&self) -> u64;

    /// Get a reference to a session by agent ID.
    fn get_session(&self, agent_id: &AgentId) -> Option<&RuntimeSession>;

    /// Capture pane output for a known session (used for dead-pane crash text).
    fn capture_session_output(&self, agent_id: &AgentId) -> Option<TerminalSnapshot>;

    /// Retrieve retained scrollback history lines for the currently attached
    /// session (issue #198).
    ///
    /// Returns `Option<Vec<String>>` — plain-text rows (no styles) from the
    /// tmux pane's scrollback buffer. Implementations SHOULD cache so they do
    /// not shell out on every render frame: re-capture only when `take_dirty()`
    /// returns true (new PTY data) or the attached session changes.
    ///
    /// - **`TmuxRuntimeManager`**: cached `capture-pane -S` bounded to
    ///   `HISTORY_LINE_CAP` lines.
    /// - **`StubRuntimeManager`**: always returns `None` (no PTY).
    fn capture_history(&mut self) -> Option<Vec<String>>;
}
