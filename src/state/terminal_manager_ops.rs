//! Terminal-manager reducer operations (issue #361 PR B).
//!
//! Pure, deterministic transitions for the Terminal Manager screen. The
//! manager lists every runtime-inventory shell and shows a throttled
//! read-only preview. Inventory/manager/return state are runtime-only (never
//! persisted). Side effects (capture, attach, close) happen at the runtime
//! boundary BEFORE these reducers run.

use super::{
    AppState, ManagedShellRow, PaneFocus, PriorAgentFocus, ScreenMode, ShellReturnTarget,
    status_label_for,
};
use crate::domain::{AgentId, AgentStatus};
use crate::messages::{NavDir, TerminalManagerMessage};

/// Build the deterministic list of managed-shell rows from current state.
///
/// Pure projection: no I/O. The order follows the inventory's stable
/// lexicographic order (mirrors [`super::ShellInventory::iter`]). Dead or
/// non-Running owners are marked `close_only` so the UI annotates them and
/// the input layer disables Enter.
#[must_use]
pub fn project_managed_shell_rows(state: &AppState) -> Vec<ManagedShellRow> {
    state
        .shell_overlay
        .inventory
        .iter()
        .filter_map(|agent_id| {
            let agent = state.agents.iter().find(|a| &a.id == agent_id)?;
            let repository = state.repository_by_id(&agent.repository_id)?;
            Some(ManagedShellRow {
                agent_id: agent.id.clone(),
                agent_name: agent.name.clone(),
                repository_name: repository.name.clone(),
                repository_id: repository.id.clone(),
                work_dir: agent.work_dir.to_string_lossy().into_owned(),
                status_label: status_label_for(agent.status).to_string(),
                running: agent.status == AgentStatus::Running,
                close_only: agent.status != AgentStatus::Running,
            })
        })
        .collect()
}

impl AppState {
    /// Enter terminal-manager mode, saving prior focus state (issue #361 PR B).
    fn enter_terminal_manager_mode(&mut self) -> bool {
        self.terminal_manager.prior_agent_focus = Some(PriorAgentFocus {
            pane_focus: self.pane_focus,
            selected_repository_index: self.selected_repository_index,
            selected_agent_index: self.selected_agent_index,
        });
        self.screen_mode = ScreenMode::DashboardTerminals;
        self.terminal_manager.active = true;
        self.terminal_manager.bump_generation();
        let rows = project_managed_shell_rows(self);
        self.terminal_manager.selected_index = if rows.is_empty() { None } else { Some(0) };
        // Clear stale preview when entering.
        self.terminal_manager.preview = super::ShellPreview::default();
        true
    }

    /// Exit terminal-manager mode, restoring prior focus state.
    fn exit_terminal_manager_mode(&mut self) {
        self.screen_mode = ScreenMode::Dashboard;
        self.terminal_manager.active = false;
        self.terminal_manager.clear_pending_focus();
        self.terminal_manager.preview = super::ShellPreview::default();
        if let Some(prior) = self.terminal_manager.prior_agent_focus.take() {
            self.pane_focus = prior.pane_focus;
            if let Some(idx) = prior.selected_agent_index
                && idx < self.agents.len()
            {
                self.selected_agent_index = Some(idx);
            }
            if let Some(idx) = prior.selected_repository_index
                && idx < self.repositories.len()
            {
                self.selected_repository_index = Some(idx);
            }
        } else {
            self.pane_focus = PaneFocus::Agents;
        }
    }

    fn handle_terminal_manager_navigation(&mut self, dir: NavDir) -> bool {
        let rows = project_managed_shell_rows(self);
        let count = rows.len();
        if count == 0 {
            return true;
        }
        let current = self.terminal_manager.selected_index.unwrap_or(0);
        let new_index = match dir {
            NavDir::Up => current.saturating_sub(1),
            NavDir::Down => (current + 1).min(count - 1),
            NavDir::Home => 0,
            NavDir::End => count - 1,
            NavDir::PageUp(_) | NavDir::PageDown(_) | NavDir::Next | NavDir::Prev => current,
        };
        if new_index != current {
            self.terminal_manager.selected_index = Some(new_index);
            // Selection changed: clear stale preview so the next capture
            // correlates cleanly.
            self.terminal_manager.preview = super::ShellPreview::default();
        }
        true
    }

    /// Record a generation-guarded pending focus request (issue #361 PR B).
    /// The input boundary calls this BEFORE driving the attach scheduler; the
    /// actual focus completes only after `confirm_shell_focus` observes the
    /// expected owner attached.
    fn request_shell_focus(&mut self, agent_id: AgentId) -> bool {
        let rows = project_managed_shell_rows(self);
        let Some(selected_index) = self.terminal_manager.selected_index else {
            return true;
        };
        // Verify the requested agent is the selected row's owner and Running.
        let Some(row) = rows.get(selected_index) else {
            return true;
        };
        if row.agent_id != agent_id || !row.running {
            // Stale or non-Running: do not request.
            return true;
        }
        let generation = self.terminal_manager.bump_generation();
        self.terminal_manager.pending_focus = Some(super::PendingShellFocus {
            agent_id,
            selected_index,
            generation,
        });
        true
    }

    /// Confirm a pending focus after the expected owner attached (issue #361
    /// PR B). Generation-guarded: a mismatched owner or stale generation is
    /// rejected. On success the pending focus is cleared and the caller opens
    /// the shell overlay with a TerminalManager return target.
    fn confirm_shell_focus(&mut self, attached_agent_id: &AgentId) -> bool {
        let Some(pending) = self.terminal_manager.pending_focus.clone() else {
            return false;
        };
        if pending.generation != self.terminal_manager.generation
            || &pending.agent_id != attached_agent_id
        {
            return false;
        }
        self.terminal_manager.clear_pending_focus();
        self.terminal_manager.active = false;
        self.screen_mode = ScreenMode::Dashboard;
        self.shell_return_target = ShellReturnTarget::TerminalManager;
        self.resume_shell_overlay(attached_agent_id.clone());
        true
    }

    /// Fail a pending focus (e.g. attach failed or owner no longer Running).
    fn fail_shell_focus(&mut self) {
        self.terminal_manager.clear_pending_focus();
    }

    /// Apply a preview capture result for the selected shell (issue #361 PR B).
    /// Correlates by owner agent id and current generation so stale captures
    /// are discarded. A failure clears the preview.
    fn apply_shell_preview_result(
        &mut self,
        agent_id: &AgentId,
        generation: u64,
        result: Result<Vec<String>, ()>,
    ) -> bool {
        // Reject stale results: generation must match the current manager
        // session AND the owner must still be the selected row.
        if generation != self.terminal_manager.generation {
            return true;
        }
        let rows = project_managed_shell_rows(self);
        let selected = self
            .terminal_manager
            .selected_index
            .and_then(|idx| rows.get(idx));
        let selected_owner = selected.map(|row| &row.agent_id);
        if selected_owner != Some(agent_id) {
            // Selection moved on: discard.
            return true;
        }
        match result {
            Ok(lines) => {
                self.terminal_manager.preview = super::ShellPreview {
                    lines,
                    failed: false,
                    agent_id: Some(agent_id.clone()),
                };
            }
            Err(()) => {
                self.terminal_manager.preview = super::ShellPreview {
                    lines: Vec::new(),
                    failed: true,
                    agent_id: Some(agent_id.clone()),
                };
            }
        }
        true
    }

    /// Handle all Terminal Manager events (issue #361 PR B).
    pub(super) fn apply_terminal_manager_message(
        &mut self,
        message: TerminalManagerMessage,
    ) -> bool {
        match message {
            TerminalManagerMessage::EnterMode => self.enter_terminal_manager_mode(),
            TerminalManagerMessage::ExitMode => {
                self.exit_terminal_manager_mode();
                true
            }
            TerminalManagerMessage::Navigate(dir) => self.handle_terminal_manager_navigation(dir),
            TerminalManagerMessage::RequestFocus(agent_id) => self.request_shell_focus(agent_id),
            TerminalManagerMessage::ConfirmFocus(agent_id) => self.confirm_shell_focus(&agent_id),
            TerminalManagerMessage::FailFocus => {
                self.fail_shell_focus();
                true
            }
            TerminalManagerMessage::PreviewResult {
                agent_id,
                generation,
                result,
            } => self.apply_shell_preview_result(&agent_id, generation, result),
            TerminalManagerMessage::ShellClosed(agent_id) => {
                self.remove_shell_window(&agent_id);
                if self.terminal_manager.preview.agent_id.as_ref() == Some(&agent_id) {
                    self.terminal_manager.preview = super::ShellPreview::default();
                }
                let rows = project_managed_shell_rows(self);
                self.terminal_manager.selected_index =
                    Self::clamp_selection(rows.len(), self.terminal_manager.selected_index);
                true
            }
        }
    }

    /// Clamp a selection index to the row count, preserving None when empty.
    fn clamp_selection(count: usize, current: Option<usize>) -> Option<usize> {
        if count == 0 {
            None
        } else {
            Some(current.unwrap_or(0).min(count - 1))
        }
    }

    /// Whether the terminal-manager screen is currently active.
    #[must_use]
    pub fn terminal_manager_active(&self) -> bool {
        self.terminal_manager.active
    }

    /// Whether a cross-agent shell focus is pending confirmation.
    #[must_use]
    pub fn terminal_manager_focus_pending(&self) -> bool {
        self.terminal_manager.pending_focus.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Agent, AgentStatus, Repository, RepositoryId};
    use std::path::PathBuf;

    fn make_agent(id: &str, name: &str, repo_id: &str, status: AgentStatus) -> Agent {
        let mut agent = Agent::new(
            AgentId(id.into()),
            RepositoryId(repo_id.into()),
            name.into(),
            PathBuf::from(format!("/tmp/{id}")),
        );
        agent.status = status;
        agent
    }

    fn make_repo(id: &str, name: &str) -> Repository {
        Repository::new(
            RepositoryId(id.into()),
            name.into(),
            id.into(),
            PathBuf::from("/tmp"),
        )
    }

    fn state_with_two_shells() -> AppState {
        let mut state = AppState::default();
        let repo = make_repo("repo-1", "Fixture Repo");
        state.repositories.push(repo);
        let alpha = make_agent("agent-1", "Alpha Agent", "repo-1", AgentStatus::Running);
        let beta = make_agent("agent-2", "Beta Agent", "repo-1", AgentStatus::Running);
        state.agents.push(alpha);
        state.agents.push(beta);
        state.record_shell_window(AgentId("agent-1".into()));
        state.record_shell_window(AgentId("agent-2".into()));
        state
    }

    #[test]
    fn project_rows_lists_inventory_in_stable_order_with_fields() {
        let state = state_with_two_shells();
        let rows = project_managed_shell_rows(&state);
        assert_eq!(rows.len(), 2);
        // Inventory is sorted lexicographically by agent id.
        assert_eq!(rows[0].agent_name, "Alpha Agent");
        assert_eq!(rows[0].repository_name, "Fixture Repo");
        assert_eq!(rows[0].status_label, "Running");
        assert!(rows[0].running);
        assert!(!rows[0].close_only);
        assert_eq!(rows[1].agent_name, "Beta Agent");
    }

    #[test]
    fn project_rows_marks_dead_owner_close_only() {
        let mut state = state_with_two_shells();
        // Mark beta dead.
        for agent in &mut state.agents {
            if agent.id == AgentId("agent-2".into()) {
                agent.status = AgentStatus::Dead;
            }
        }
        let rows = project_managed_shell_rows(&state);
        let Some(beta) = rows.iter().find(|row| row.agent_name == "Beta Agent") else {
            panic!("beta present");
        };
        assert!(!beta.running);
        assert!(beta.close_only);
        assert_eq!(beta.status_label, "Dead");
    }

    #[test]
    fn request_focus_records_generation_guarded_pending() {
        let mut state = state_with_two_shells();
        state.apply_terminal_manager_message(TerminalManagerMessage::EnterMode);
        let gen_before = state.terminal_manager.generation;
        state.apply_terminal_manager_message(TerminalManagerMessage::RequestFocus(AgentId(
            "agent-1".into(),
        )));
        let Some(pending) = state.terminal_manager.pending_focus.as_ref() else {
            panic!("pending focus recorded");
        };
        assert_eq!(pending.agent_id, AgentId("agent-1".into()));
        assert_eq!(pending.generation, gen_before.wrapping_add(1));
    }

    #[test]
    fn confirm_focus_succeeds_for_matching_owner_and_generation() {
        let mut state = state_with_two_shells();
        state.apply_terminal_manager_message(TerminalManagerMessage::EnterMode);
        state.apply_terminal_manager_message(TerminalManagerMessage::RequestFocus(AgentId(
            "agent-1".into(),
        )));
        let ok = state.apply_terminal_manager_message(TerminalManagerMessage::ConfirmFocus(
            AgentId("agent-1".into()),
        ));
        assert!(ok);
    }

    #[test]
    fn confirm_focus_rejects_mismatched_owner() {
        let mut state = state_with_two_shells();
        state.apply_terminal_manager_message(TerminalManagerMessage::EnterMode);
        state.apply_terminal_manager_message(TerminalManagerMessage::RequestFocus(AgentId(
            "agent-1".into(),
        )));
        let ok = state.apply_terminal_manager_message(TerminalManagerMessage::ConfirmFocus(
            AgentId("agent-2".into()),
        ));
        assert!(!ok);
        assert!(state.terminal_manager.pending_focus.is_some());
    }

    #[test]
    fn confirm_focus_rejects_stale_generation() {
        let mut state = state_with_two_shells();
        state.apply_terminal_manager_message(TerminalManagerMessage::EnterMode);
        state.apply_terminal_manager_message(TerminalManagerMessage::RequestFocus(AgentId(
            "agent-1".into(),
        )));
        // Bump generation (e.g. user navigated/entered again).
        state.terminal_manager.bump_generation();
        let ok = state.apply_terminal_manager_message(TerminalManagerMessage::ConfirmFocus(
            AgentId("agent-1".into()),
        ));
        assert!(!ok);
        assert!(state.terminal_manager.pending_focus.is_some());
    }

    #[test]
    fn fail_focus_clears_pending() {
        let mut state = state_with_two_shells();
        state.apply_terminal_manager_message(TerminalManagerMessage::EnterMode);
        state.apply_terminal_manager_message(TerminalManagerMessage::RequestFocus(AgentId(
            "agent-1".into(),
        )));
        state.apply_terminal_manager_message(TerminalManagerMessage::FailFocus);
        assert!(state.terminal_manager.pending_focus.is_none());
    }

    #[test]
    fn preview_result_correlates_by_owner_and_generation() {
        let mut state = state_with_two_shells();
        state.apply_terminal_manager_message(TerminalManagerMessage::EnterMode);
        let generation = state.terminal_manager.generation;
        state.apply_terminal_manager_message(TerminalManagerMessage::PreviewResult {
            agent_id: AgentId("agent-1".into()),
            generation,
            result: Ok(vec!["line-1".into(), "line-2".into()]),
        });
        assert_eq!(
            state.terminal_manager.preview.lines,
            vec!["line-1".to_string(), "line-2".to_string()]
        );
        assert!(!state.terminal_manager.preview.failed);
    }

    #[test]
    fn preview_result_rejects_stale_generation() {
        let mut state = state_with_two_shells();
        state.apply_terminal_manager_message(TerminalManagerMessage::EnterMode);
        state.terminal_manager.preview = super::super::ShellPreview::default();
        state.apply_terminal_manager_message(TerminalManagerMessage::PreviewResult {
            agent_id: AgentId("agent-1".into()),
            generation: state.terminal_manager.generation.wrapping_add(99),
            result: Ok(vec!["stale".into()]),
        });
        assert!(state.terminal_manager.preview.lines.is_empty());
    }

    #[test]
    fn preview_result_failure_clears_preview() {
        let mut state = state_with_two_shells();
        state.apply_terminal_manager_message(TerminalManagerMessage::EnterMode);
        let generation = state.terminal_manager.generation;
        state.terminal_manager.preview = super::super::ShellPreview {
            lines: vec!["old".into()],
            failed: false,
            agent_id: Some(AgentId("agent-1".into())),
        };
        state.apply_terminal_manager_message(TerminalManagerMessage::PreviewResult {
            agent_id: AgentId("agent-1".into()),
            generation,
            result: Err(()),
        });
        assert!(state.terminal_manager.preview.lines.is_empty());
        assert!(state.terminal_manager.preview.failed);
    }

    #[test]
    fn shell_closed_clears_preview_and_clamps_selection() {
        let mut state = state_with_two_shells();
        state.apply_terminal_manager_message(TerminalManagerMessage::EnterMode);
        // Move selection to beta (row 1).
        state.apply_terminal_manager_message(TerminalManagerMessage::Navigate(NavDir::End));
        assert_eq!(state.terminal_manager.selected_index, Some(1));
        // Runtime closes beta: inventory entry already removed.
        state.remove_shell_window(&AgentId("agent-2".into()));
        state.terminal_manager.preview = super::super::ShellPreview {
            lines: vec!["beta".into()],
            failed: false,
            agent_id: Some(AgentId("agent-2".into())),
        };
        state.apply_terminal_manager_message(TerminalManagerMessage::ShellClosed(AgentId(
            "agent-2".into(),
        )));
        assert!(state.terminal_manager.preview.lines.is_empty());
        // Selection clamped to remaining single row.
        assert_eq!(state.terminal_manager.selected_index, Some(0));
    }
}
