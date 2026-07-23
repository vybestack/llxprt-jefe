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
            agent_id: Some(agent_id.clone()),
            generation: self.shell_overlay.generation.wrapping_add(1),
            previous_pane_focus: Some(self.pane_focus),
            inventory: self.shell_overlay.inventory.clone(),
        };
        // Record the shell window in the runtime inventory (issue #361). The
        // caller (runtime boundary) only invokes this after a successful open,
        // so recording here keeps the inventory consistent with visibility.
        self.shell_overlay.inventory.record(agent_id);
        // Focus the terminal so keyboard input is forwarded to the shell.
        self.terminal_focused = true;
        self.pane_focus = crate::state::PaneFocus::Terminal;
        self.dashboard_grab = None;
        self.reset_shell_terminal_view();
    }

    /// Deactivate the shell overlay, restoring its launch surface.
    pub fn close_shell_overlay(&mut self) {
        if let Some(agent_id) = self.shell_overlay.agent_id.take() {
            self.shell_overlay.inventory.remove(&agent_id);
            self.restore_after_shell_overlay();
        }
    }

    /// Hide the visible shell overlay while keeping its `jefe-shell` window
    /// alive (issue #361 PR A).
    ///
    /// Clears the visible `agent_id` and restores dashboard focus/layout, but
    /// keeps the inventory entry so F10 can resume the exact shell. Bumps the
    /// generation so the background observer recognizes the new state. The
    /// runtime boundary is responsible for selecting agent window 0 before
    /// invoking this; this reducer performs no I/O.
    pub fn hide_shell_overlay(&mut self) {
        if self.shell_overlay.agent_id.is_none() {
            return;
        }
        // Inventory entry persists: the shell window is alive, just hidden.
        self.shell_overlay.generation = self.shell_overlay.generation.wrapping_add(1);
        self.restore_after_shell_overlay();
    }

    fn restore_after_shell_overlay(&mut self) {
        self.shell_overlay.agent_id = None;
        self.terminal_focused = false;
        self.dashboard_grab = None;
        let previous_pane_focus = self.shell_overlay.previous_pane_focus.take();
        if self.shell_return_target == crate::state::ShellReturnTarget::TerminalManager {
            self.screen_mode = crate::state::ScreenMode::DashboardTerminals;
            self.terminal_manager.active = true;
            self.pane_focus = crate::state::PaneFocus::Agents;
        } else {
            self.pane_focus = previous_pane_focus.unwrap_or(crate::state::PaneFocus::Agents);
        }
        self.shell_return_target = crate::state::ShellReturnTarget::Dashboard;
        self.reset_shell_terminal_view();
    }

    /// Resume a hidden shell for `agent_id`, making the overlay visible again
    /// (issue #361 PR A).
    ///
    /// The runtime boundary invokes this after successfully re-selecting the
    /// existing `jefe-shell` window. This reducer records the inventory entry
    /// (idempotent) and restores the visible overlay state. It does not
    /// duplicate the window because the runtime only re-selects an existing
    /// window.
    pub fn resume_shell_overlay(&mut self, agent_id: AgentId) {
        self.shell_overlay.inventory.record(agent_id.clone());
        self.shell_overlay = ShellOverlayState {
            agent_id: Some(agent_id),
            generation: self.shell_overlay.generation.wrapping_add(1),
            previous_pane_focus: Some(self.pane_focus),
            inventory: self.shell_overlay.inventory.clone(),
        };
        self.terminal_focused = true;
        self.pane_focus = crate::state::PaneFocus::Terminal;
        self.dashboard_grab = None;
        self.reset_shell_terminal_view();
    }

    /// Whether `agent_id` owns a tracked shell window (visible or hidden).
    #[must_use]
    pub fn has_shell_window(&self, agent_id: &AgentId) -> bool {
        self.shell_overlay.inventory.contains(agent_id)
    }

    /// Record a shell window in the runtime inventory (issue #361). Called by
    /// the runtime boundary after a successful open/resume/adopt.
    pub fn record_shell_window(&mut self, agent_id: AgentId) {
        self.shell_overlay.inventory.record(agent_id);
    }

    /// Remove `agent_id` from the runtime inventory (issue #361). Called by
    /// the runtime boundary after a successful close, disappearance, kill, or
    /// startup orphan cleanup. Returns whether an entry was removed.
    pub fn remove_shell_window(&mut self, agent_id: &AgentId) -> bool {
        self.shell_overlay.inventory.remove(agent_id)
    }

    /// Snapshot the agent IDs owning tracked shell windows (issue #361).
    /// Used by graceful shutdown to close every Jefe-created shell.
    #[must_use]
    pub fn shell_window_owners(&self) -> Vec<AgentId> {
        self.shell_overlay.inventory.to_vec()
    }

    #[must_use]
    pub fn shell_focus_ordinal(&self, agent_id: &AgentId) -> u64 {
        self.shell_overlay.inventory.focus_ordinal(agent_id)
    }

    pub fn record_shell_focus(&mut self, agent_id: &AgentId) {
        self.shell_overlay.inventory.record_focus(agent_id);
    }

    /// Replace the entire shell inventory from runtime ground truth
    /// (issue #361). Used by startup adoption and batched reconciliation.
    pub fn replace_shell_inventory(&mut self, agents: Vec<AgentId>) {
        self.shell_overlay.inventory.replace(agents);
    }

    /// Clear the entire shell inventory (issue #361). Used by graceful
    /// shutdown after best-effort close attempts.
    pub fn clear_shell_inventory(&mut self) {
        self.shell_overlay.inventory.clear();
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
            UiNavigationMessage::HideShellOverlay => self.hide_shell_overlay(),
            UiNavigationMessage::ResumeShellOverlay(agent_id) => {
                self.resume_shell_overlay(agent_id);
            }
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
        let mut state = AppState {
            pane_focus: PaneFocus::Agents,
            ..AppState::default()
        };
        state.open_shell_overlay(AgentId("agent-1".into()));
        state.close_shell_overlay();
        assert_eq!(state.shell_overlay.agent_id, None);
        assert!(!state.terminal_focused);
        assert_eq!(state.pane_focus, PaneFocus::Agents);
        assert!(!state.shell_overlay_active());
    }

    #[test]
    fn close_shell_overlay_restores_repository_focus() {
        let mut state = AppState::default();
        state.open_shell_overlay(AgentId("agent-1".into()));
        state.close_shell_overlay();
        assert_eq!(state.pane_focus, PaneFocus::Repositories);
    }

    #[test]
    fn manager_restore_consumes_previous_focus_and_return_target() {
        let mut state = AppState::default();
        state.shell_return_target = crate::state::ShellReturnTarget::TerminalManager;
        state.open_shell_overlay(AgentId("agent-1".into()));

        state.hide_shell_overlay();

        assert_eq!(
            state.screen_mode,
            crate::state::ScreenMode::DashboardTerminals
        );
        assert!(state.terminal_manager.active);
        assert_eq!(state.shell_overlay.previous_pane_focus, None);
        assert_eq!(
            state.shell_return_target,
            crate::state::ShellReturnTarget::Dashboard
        );
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

    #[test]
    fn hide_shell_overlay_clears_visible_overlay_but_keeps_inventory() {
        let mut state = AppState {
            pane_focus: PaneFocus::Agents,
            ..AppState::default()
        };
        state.open_shell_overlay(AgentId("agent-1".into()));
        state.hide_shell_overlay();
        assert_eq!(state.shell_overlay.agent_id, None);
        assert!(!state.terminal_focused);
        assert_eq!(state.pane_focus, PaneFocus::Agents);
        assert!(!state.shell_overlay_active());
        // Inventory still tracks the hidden shell.
        assert!(state.has_shell_window(&AgentId("agent-1".into())));
    }

    #[test]
    fn hide_shell_overlay_restores_repository_focus() {
        let mut state = AppState::default();
        state.open_shell_overlay(AgentId("agent-1".into()));
        state.hide_shell_overlay();
        assert_eq!(state.pane_focus, PaneFocus::Repositories);
    }

    #[test]
    fn hide_shell_overlay_is_noop_when_not_visible() {
        let mut state = AppState::default();
        state.hide_shell_overlay();
        assert_eq!(state.shell_overlay.agent_id, None);
        assert!(state.shell_overlay.inventory.is_empty());
    }

    #[test]
    fn hide_shell_overlay_bumps_generation() {
        let mut state = AppState::default();
        state.open_shell_overlay(AgentId("agent-1".into()));
        let gen_before = state.shell_overlay.generation;
        state.hide_shell_overlay();
        assert_eq!(
            state.shell_overlay.generation,
            gen_before.wrapping_add(1),
            "hide must bump generation so observers recognize the new state"
        );
    }

    #[test]
    fn manager_shell_hide_returns_to_manager_and_clears_return_target() {
        let mut state = AppState::default();
        state.screen_mode = crate::state::ScreenMode::DashboardTerminals;
        state.terminal_manager.active = true;
        state.shell_return_target = crate::state::ShellReturnTarget::TerminalManager;
        state.resume_shell_overlay(AgentId("agent-1".into()));

        state.hide_shell_overlay();

        assert_eq!(
            state.screen_mode,
            crate::state::ScreenMode::DashboardTerminals
        );
        assert!(state.terminal_manager.active);
        assert!(!state.shell_overlay_active());
        assert_eq!(
            state.shell_return_target,
            crate::state::ShellReturnTarget::Dashboard
        );
    }

    #[test]
    fn resume_shell_overlay_makes_overlay_visible_and_keeps_inventory() {
        let mut state = AppState::default();
        let agent_id = AgentId("agent-1".into());
        state.open_shell_overlay(agent_id.clone());
        state.hide_shell_overlay();
        state.resume_shell_overlay(agent_id.clone());
        assert_eq!(state.shell_overlay.agent_id, Some(agent_id.clone()));
        assert!(state.terminal_focused);
        assert_eq!(state.pane_focus, PaneFocus::Terminal);
        assert!(state.shell_overlay_active());
        assert!(state.has_shell_window(&agent_id));
        // Resume does not duplicate the inventory entry.
        assert_eq!(state.shell_overlay.inventory.len(), 1);
    }

    #[test]
    fn resume_shell_overlay_bumps_generation() {
        let mut state = AppState::default();
        let agent_id = AgentId("agent-1".into());
        state.open_shell_overlay(agent_id.clone());
        state.hide_shell_overlay();
        let gen_before = state.shell_overlay.generation;
        state.resume_shell_overlay(agent_id);
        assert_eq!(
            state.shell_overlay.generation,
            gen_before.wrapping_add(1),
            "resume must bump generation so observers recognize the new state"
        );
    }

    #[test]
    fn close_shell_overlay_removes_inventory_entry() {
        let mut state = AppState::default();
        let agent_id = AgentId("agent-1".into());
        state.open_shell_overlay(agent_id.clone());
        assert!(state.has_shell_window(&agent_id));
        state.close_shell_overlay();
        assert!(!state.has_shell_window(&agent_id));
    }

    #[test]
    fn close_after_hide_via_remove_window_clears_inventory_entry() {
        let mut state = AppState::default();
        let agent_id = AgentId("agent-1".into());
        state.open_shell_overlay(agent_id.clone());
        state.hide_shell_overlay();
        assert!(state.has_shell_window(&agent_id));
        // When the shell was hidden, the natural-exit observer/reconciler
        // removes the inventory entry directly by agent_id (it cannot use
        // close_shell_overlay because the visible overlay is already gone).
        assert!(state.remove_shell_window(&agent_id));
        assert!(!state.has_shell_window(&agent_id));
    }

    #[test]
    fn record_shell_window_is_idempotent() {
        let mut state = AppState::default();
        let agent_id = AgentId("agent-1".into());
        state.record_shell_window(agent_id.clone());
        state.record_shell_window(agent_id.clone());
        assert_eq!(state.shell_overlay.inventory.len(), 1);
    }

    #[test]
    fn remove_shell_window_returns_whether_entry_existed() {
        let mut state = AppState::default();
        let agent_id = AgentId("agent-1".into());
        state.record_shell_window(agent_id.clone());
        assert!(state.remove_shell_window(&agent_id));
        assert!(!state.remove_shell_window(&agent_id));
    }

    #[test]
    fn replace_shell_inventory_overwrites_membership() {
        let mut state = AppState::default();
        state.record_shell_window(AgentId("old".into()));
        state.replace_shell_inventory(vec![AgentId("a".into()), AgentId("b".into())]);
        assert!(!state.has_shell_window(&AgentId("old".into())));
        assert!(state.has_shell_window(&AgentId("a".into())));
        assert!(state.has_shell_window(&AgentId("b".into())));
    }

    #[test]
    fn clear_shell_inventory_empties_all_entries() {
        let mut state = AppState::default();
        state.record_shell_window(AgentId("a".into()));
        state.record_shell_window(AgentId("b".into()));
        state.clear_shell_inventory();
        assert!(state.shell_overlay.inventory.is_empty());
    }

    #[test]
    fn shell_window_owners_snapshots_inventory() {
        let mut state = AppState::default();
        state.record_shell_window(AgentId("b".into()));
        state.record_shell_window(AgentId("a".into()));
        let owners = state.shell_window_owners();
        assert_eq!(
            owners,
            vec![AgentId("a".into()), AgentId("b".into())],
            "owners snapshot must be deterministic lexicographic order"
        );
    }

    #[test]
    fn kill_agent_message_removes_shell_inventory_entry() {
        use crate::domain::{Agent, AgentStatus, RepositoryId};
        use crate::messages::RuntimeMessage;
        use crate::state::AppMessage;

        let agent_id = AgentId("agent-kill".into());
        let mut state = AppState::default();
        let mut agent = Agent::new(
            agent_id.clone(),
            RepositoryId("repo".into()),
            "Agent".into(),
            std::path::PathBuf::from("/tmp/agent"),
        );
        agent.status = AgentStatus::Running;
        state.agents.push(agent);
        state.record_shell_window(agent_id.clone());
        assert!(state.has_shell_window(&agent_id));

        state = state.apply_message(AppMessage::Runtime(RuntimeMessage::KillAgent(
            agent_id.clone(),
        )));

        assert!(
            !state.has_shell_window(&agent_id),
            "KillAgent must remove the shell inventory entry (issue #361)"
        );
    }

    #[test]
    fn natural_agent_status_changed_to_dead_does_not_remove_shell_inventory() {
        // Natural death (AgentStatusChanged->Dead) must NOT remove inventory
        // on its own: the shell window is closed close-only and the inventory
        // is cleaned by the close/runtime disappearance path, not by the
        // natural-death reducer (issue #361 invariant #4).
        use crate::domain::{Agent, AgentStatus, RepositoryId};
        use crate::messages::RuntimeMessage;
        use crate::state::AppMessage;

        let agent_id = AgentId("agent-natural".into());
        let mut state = AppState::default();
        let mut agent = Agent::new(
            agent_id.clone(),
            RepositoryId("repo".into()),
            "Agent".into(),
            std::path::PathBuf::from("/tmp/agent"),
        );
        agent.status = AgentStatus::Running;
        state.agents.push(agent);
        state.record_shell_window(agent_id.clone());
        assert!(state.has_shell_window(&agent_id));

        state = state.apply_message(AppMessage::Runtime(RuntimeMessage::AgentStatusChanged(
            agent_id.clone(),
            AgentStatus::Dead,
        )));

        assert!(
            state.has_shell_window(&agent_id),
            "natural death must retain shell inventory (close-only cleanup, issue #361)"
        );
    }
}
