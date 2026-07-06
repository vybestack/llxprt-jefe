//! Dashboard reorder ("grab") state transition logic.
//!
//! Implements the select-then-move interaction: Space grabs the highlighted
//! repository or agent, arrows move it within its visible set, and Space/Enter
//! drops it. Grab state is transient (not persisted).

use super::{AppState, DashboardGrabPane, PaneFocus, ScreenMode};

impl AppState {
    /// Validate that an active dashboard grab still points to a valid visible
    /// item. Clears the grab if the screen is no longer the Dashboard, the
    /// grabbed repository was deleted, or the visible-index/local-index is out
    /// of bounds after a visibility or data change.
    pub(super) fn validate_dashboard_grab(&mut self) {
        if self.screen_mode != ScreenMode::Dashboard {
            self.dashboard_grab = None;
            return;
        }
        match &self.dashboard_grab {
            Some(DashboardGrabPane::Repository { visible_index }) => {
                if self
                    .visible_repository_indices()
                    .get(*visible_index)
                    .is_none()
                {
                    self.dashboard_grab = None;
                }
            }
            Some(DashboardGrabPane::Agent {
                repository_id,
                local_index,
            }) => {
                let repo_exists = self
                    .repositories
                    .iter()
                    .any(|repo| repo.id == *repository_id);
                if !repo_exists
                    || self
                        .agent_indices_for_repository(repository_id)
                        .get(*local_index)
                        .is_none()
                {
                    self.dashboard_grab = None;
                }
            }
            None => {}
        }
    }

    /// Begin a dashboard reorder grab on the currently focused pane item.
    ///
    /// Repositories grab at their visible-index; agents grab at their local
    /// visible-index within the selected repository. The terminal pane is a no-op.
    pub(super) fn enter_dashboard_grab(&mut self) {
        match self.pane_focus {
            PaneFocus::Repositories => {
                self.dashboard_grab = self
                    .selected_repository_visible_index()
                    .map(|visible_index| DashboardGrabPane::Repository { visible_index });
            }
            PaneFocus::Agents => {
                // Capture the selected repository so the grab stays bound to it
                // even if the selection changes while the grab is active.
                if let Some(repository_id) = self.selected_repository_id().cloned() {
                    self.dashboard_grab = self.selected_agent_local_index().map(|local_index| {
                        DashboardGrabPane::Agent {
                            repository_id,
                            local_index,
                        }
                    });
                }
            }
            PaneFocus::Terminal => {}
        }
    }

    /// Move the grabbed dashboard item up within its visible set.
    pub(super) fn move_dashboard_grab_up(&mut self) {
        match &self.dashboard_grab {
            Some(DashboardGrabPane::Repository { visible_index }) => {
                if *visible_index == 0 {
                    return;
                }
                let visible_repo_indices = self.visible_repository_indices();
                let Some((current_global_idx, target_global_idx)) = visible_repo_indices
                    .get(*visible_index)
                    .zip(visible_repo_indices.get(*visible_index - 1))
                    .map(|(a, b)| (*a, *b))
                else {
                    return;
                };
                self.repositories
                    .swap(current_global_idx, target_global_idx);
                self.dashboard_grab = Some(DashboardGrabPane::Repository {
                    visible_index: *visible_index - 1,
                });
                self.selected_repository_index = Some(target_global_idx);
            }
            Some(DashboardGrabPane::Agent {
                repository_id,
                local_index,
            }) => {
                if *local_index == 0 {
                    return;
                }
                let agent_indices = self.agent_indices_for_repository(repository_id);
                let Some((current_global_idx, target_global_idx)) = agent_indices
                    .get(*local_index)
                    .zip(agent_indices.get(*local_index - 1))
                    .map(|(a, b)| (*a, *b))
                else {
                    return;
                };
                self.agents.swap(current_global_idx, target_global_idx);
                self.dashboard_grab = Some(DashboardGrabPane::Agent {
                    repository_id: repository_id.clone(),
                    local_index: *local_index - 1,
                });
                self.selected_agent_index = Some(target_global_idx);
            }
            None => {}
        }
    }

    /// Move the grabbed dashboard item down within its visible set.
    pub(super) fn move_dashboard_grab_down(&mut self) {
        match &self.dashboard_grab {
            Some(DashboardGrabPane::Repository { visible_index }) => {
                let visible_repo_indices = self.visible_repository_indices();
                let Some((current_global_idx, target_global_idx)) = visible_repo_indices
                    .get(*visible_index)
                    .zip(visible_repo_indices.get(*visible_index + 1))
                    .map(|(a, b)| (*a, *b))
                else {
                    return;
                };
                self.repositories
                    .swap(current_global_idx, target_global_idx);
                self.dashboard_grab = Some(DashboardGrabPane::Repository {
                    visible_index: *visible_index + 1,
                });
                self.selected_repository_index = Some(target_global_idx);
            }
            Some(DashboardGrabPane::Agent {
                repository_id,
                local_index,
            }) => {
                let agent_indices = self.agent_indices_for_repository(repository_id);
                let Some((current_global_idx, target_global_idx)) = agent_indices
                    .get(*local_index)
                    .zip(agent_indices.get(*local_index + 1))
                    .map(|(a, b)| (*a, *b))
                else {
                    return;
                };
                self.agents.swap(current_global_idx, target_global_idx);
                self.dashboard_grab = Some(DashboardGrabPane::Agent {
                    repository_id: repository_id.clone(),
                    local_index: *local_index + 1,
                });
                self.selected_agent_index = Some(target_global_idx);
            }
            None => {}
        }
    }
}
