//! Pane-aware vertical navigation handlers for [`AppState`].
//!
//! Split out of `mod.rs` to keep that file within the source-size gate. Both
//! handlers move the selection within the currently focused pane, skipping
//! filtered-out rows and resetting terminal scrollback on a real move.
//!
//! Routing is by `pane_focus` alone, matching `handle_navigate_page`. The
//! startup agent-type availability list is the zero-agent form of the agents
//! pane, so `PaneFocus::Agents` moves its cursor exactly while that pane is
//! the one on screen — the same condition `activate_execution` already uses to
//! send Enter there. A pane that is not showing still moves nothing, which is
//! what issue #722 asked for; a pane that is showing moves, which is what the
//! pre-cutover dashboard did (issue #734).

use super::AppState;
use super::types::PaneFocus;

impl AppState {
    /// Whether vertical keys address the startup availability list.
    ///
    /// True only when the pane has replaced the agent list *and* the agent
    /// side of the dashboard holds the focus; with the repositories pane
    /// focused the repository cursor moves, as issue #722 requires.
    fn agent_types_pane_focused(&self) -> bool {
        self.pane_focus == PaneFocus::Agents && self.agent_types_pane_active()
    }

    pub(super) fn handle_navigate_up(&mut self) {
        if self.agent_types_pane_focused() {
            self.selected_agent_type_index = self.selected_agent_type_index.saturating_sub(1);
            return;
        }
        match self.pane_focus {
            PaneFocus::Repositories => {
                let visible_repo_indices = self.visible_repository_indices();
                let selected_visible_idx = self.selected_repository_visible_index();
                if let Some(visible_idx) = selected_visible_idx.filter(|&idx| idx > 0) {
                    self.remember_selected_agent_for_current_repo();
                    self.selected_repository_index = Some(visible_repo_indices[visible_idx - 1]);
                    self.restore_selected_agent_for_current_repo();
                    self.reset_terminal_scrollback();
                }
            }
            PaneFocus::Agents => {
                let Some(repository_id) = self.selected_repository_id().cloned() else {
                    self.selected_agent_index = None;
                    return;
                };
                let visible_indices = self.agent_indices_for_repository(&repository_id);
                if visible_indices.is_empty() {
                    self.selected_agent_index = None;
                    return;
                }
                let selected_local = self.selected_agent_index.and_then(|selected_idx| {
                    visible_indices
                        .iter()
                        .position(|global_idx| *global_idx == selected_idx)
                });

                match selected_local {
                    Some(local_idx) if local_idx > 0 => {
                        self.selected_agent_index = Some(visible_indices[local_idx - 1]);
                        self.remember_selected_agent_for_current_repo();
                        self.reset_terminal_scrollback();
                    }
                    Some(_) => {}
                    None => {
                        self.selected_agent_index = visible_indices.first().copied();
                        self.remember_selected_agent_for_current_repo();
                        self.reset_terminal_scrollback();
                    }
                }
            }
            PaneFocus::Terminal => {}
        }
    }

    pub(super) fn handle_navigate_down(&mut self) {
        if self.agent_types_pane_focused() {
            let last = self.agent_type_availability.len().saturating_sub(1);
            self.selected_agent_type_index =
                self.selected_agent_type_index.saturating_add(1).min(last);
            return;
        }
        match self.pane_focus {
            PaneFocus::Repositories => {
                let visible_repo_indices = self.visible_repository_indices();
                let selected_visible_idx = self.selected_repository_visible_index();
                if let Some(visible_idx) = selected_visible_idx
                    && visible_idx + 1 < visible_repo_indices.len()
                {
                    self.remember_selected_agent_for_current_repo();
                    self.selected_repository_index = Some(visible_repo_indices[visible_idx + 1]);
                    self.restore_selected_agent_for_current_repo();
                    self.reset_terminal_scrollback();
                }
            }
            PaneFocus::Agents => {
                let Some(repository_id) = self.selected_repository_id().cloned() else {
                    self.selected_agent_index = None;
                    return;
                };
                let visible_indices = self.agent_indices_for_repository(&repository_id);
                if visible_indices.is_empty() {
                    self.selected_agent_index = None;
                    return;
                }
                let selected_local = self.selected_agent_index.and_then(|selected_idx| {
                    visible_indices
                        .iter()
                        .position(|global_idx| *global_idx == selected_idx)
                });

                match selected_local {
                    Some(local_idx) if local_idx + 1 < visible_indices.len() => {
                        self.selected_agent_index = Some(visible_indices[local_idx + 1]);
                        self.remember_selected_agent_for_current_repo();
                        self.reset_terminal_scrollback();
                    }
                    Some(_) => {}
                    None => {
                        self.selected_agent_index = visible_indices.first().copied();
                        self.remember_selected_agent_for_current_repo();
                        self.reset_terminal_scrollback();
                    }
                }
            }
            PaneFocus::Terminal => {}
        }
    }
}
