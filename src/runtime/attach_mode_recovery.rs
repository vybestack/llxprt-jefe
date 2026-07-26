//! Post-attach mouse-reporting mode recovery for `TmuxRuntimeManager`
//! (issue #296).
//!
//! Extracted into its own module so `manager.rs` stays under the project's
//! 1000-line source-file limit. Follows the same inherent-impl extension
//! pattern as `async_attach.rs`.

use super::manager::TmuxRuntimeManager;
use crate::domain::AgentId;
use tracing::debug;

// Bring the `RuntimeManager` trait into scope so `mouse_reporting_active()`
// is callable on `&TmuxRuntimeManager` from this module.
use super::manager::RuntimeManager;

impl TmuxRuntimeManager {
    /// Issue #296: nudge the attached child to re-advertise its DEC private
    /// mouse-reporting modes, then trace the observed post-attach state.
    ///
    /// A freshly spawned `AttachedViewer` builds a blank `Term` with cleared
    /// mouse bits; reporting is only recovered if the child re-emits DEC
    /// private mouse modes through the PTY stream after attach. On Windows
    /// ConPTY those mode sequences can be consumed before Jefe observes them.
    /// The same-size resize nudge delivers a window-change event that prompts
    /// a well-behaved TUI to repaint and re-emit its modes. Best-effort:
    /// failures are logged inside the nudge and never block attach completion.
    pub(super) fn post_attach_mode_recovery(&self, agent_id: &AgentId) {
        if let Some(viewer) = self.viewer.as_ref() {
            viewer.nudge_for_mode_recovery();
        }
        debug!(
            agent_id = %agent_id.0,
            mouse_reporting = self.mouse_reporting_active(),
            "attach: viewer installed"
        );
    }
}
