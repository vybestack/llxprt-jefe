//! Shell-overlay reducer operations (issue #222).
//!
//! Pure state transitions for the embedded agent-shell overlay. The overlay
//! replaces the dashboard's agent list + preview with a shell terminal pane
//! while preserving the repository sidebar and outer bars.
//!
//! State is runtime-only (never persisted). Open/close events set
//! [`ShellOverlayState::agent_id`] which the layout and UI layers read to
//! decide whether to render the expanded shell geometry or the normal
//! dashboard.

use crate::domain::AgentId;
use crate::messages::UiNavigationMessage;
use crate::state::{AppState, ShellOverlayState};

impl AppState {
    /// Whether the shell overlay is currently active.
    #[must_use]
    pub fn shell_overlay_active(&self) -> bool {
        self.shell_overlay.agent_id.is_some()
    }

    /// The agent whose shell overlay is active, if any.
    #[must_use]
    pub fn shell_overlay_agent_id(&self) -> Option<&AgentId> {
        self.shell_overlay.agent_id.as_ref()
    }

    /// Activate the shell overlay for `agent_id`.
    pub fn open_shell_overlay(&mut self, agent_id: AgentId) {
        self.shell_overlay = ShellOverlayState {
            agent_id: Some(agent_id),
            generation: self.shell_overlay.generation.wrapping_add(1),
        };
        // Focus the terminal so keyboard input is forwarded to the shell.
        self.terminal_focused = true;
        self.pane_focus = crate::state::PaneFocus::Terminal;
        self.dashboard_grab = None;
        self.reset_shell_terminal_view();
    }

    /// Deactivate the shell overlay, restoring normal dashboard state.
    pub fn close_shell_overlay(&mut self) {
        if self.shell_overlay.agent_id.is_some() {
            self.shell_overlay.agent_id = None;
            self.terminal_focused = false;
            self.pane_focus = crate::state::PaneFocus::Agents;
            self.reset_shell_terminal_view();
        }
    }

    fn reset_shell_terminal_view(&mut self) {
        self.terminal_history_offset = None;
        self.terminal_viewport_rows = 0;
        self.terminal_total_lines = 0;
        self.selection = None;
        self.selection_snapshot = None;
        self.terminal_gesture_state = crate::selection::GestureState::default();
    }

    pub(super) fn apply_shell_overlay_message(&mut self, message: UiNavigationMessage) {
        match message {
            UiNavigationMessage::OpenShellOverlay => {
                if let Some(agent_id) = self.selected_agent().map(|agent| agent.id.clone()) {
                    self.open_shell_overlay(agent_id);
                }
            }
            UiNavigationMessage::CloseShellOverlay => self.close_shell_overlay(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::PaneFocus;

    #[test]
    fn open_shell_overlay_sets_agent_id_and_focuses_terminal() {
        let mut state = AppState::default();
        let agent_id = AgentId("agent-1".into());
        state.open_shell_overlay(agent_id.clone());
        assert_eq!(state.shell_overlay.agent_id, Some(agent_id));
        assert!(state.terminal_focused);
        assert_eq!(state.pane_focus, PaneFocus::Terminal);
        assert!(state.shell_overlay_active());
    }

    #[test]
    fn close_shell_overlay_clears_agent_id_and_restores_focus() {
        let mut state = AppState::default();
        state.open_shell_overlay(AgentId("agent-1".into()));
        state.close_shell_overlay();
        assert_eq!(state.shell_overlay.agent_id, None);
        assert!(!state.terminal_focused);
        assert_eq!(state.pane_focus, PaneFocus::Agents);
        assert!(!state.shell_overlay_active());
    }

    #[test]
    fn close_shell_overlay_is_idempotent_when_not_active() {
        let mut state = AppState::default();
        // Closing when not active should not panic.
        state.close_shell_overlay();
        assert_eq!(state.shell_overlay.agent_id, None);
    }

    #[test]
    fn open_shell_overlay_clears_dashboard_grab() {
        let mut state = AppState::default();
        state.dashboard_grab =
            Some(crate::state::DashboardGrabPane::Repository { visible_index: 0 });
        state.open_shell_overlay(AgentId("agent-1".into()));
        assert!(state.dashboard_grab.is_none());
    }
}
