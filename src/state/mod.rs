//! Application state and event layer.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @requirement REQ-TECH-001
//! @requirement REQ-TECH-003
//!
//! Pseudocode reference: component-001 lines 01-12

mod actions_job_ops;
mod actions_load_ops;
#[cfg(test)]
mod actions_load_tests;
mod actions_ops;
#[cfg(test)]
mod actions_tests;
mod auth_ops;
#[cfg(test)]
mod comment_pagination_tests;
mod dashboard_grab_ops;
mod errors_ops;
mod errors_types;
mod events;
mod form_build;
mod form_cursor;
mod form_delete_helpers;
mod form_ops;
mod form_projection;
mod form_runtime;
mod form_workflow_dispatch;
mod issues_close_delete_ops;
mod issues_close_reason_ops;
mod issues_inline_ops;
mod issues_load_ops;
mod issues_mutation_ops;
mod issues_ops;
mod issues_property_ops;
mod list_navigation_ops;
mod modal_ops;
/// Generic deterministic pagination state container (`PaginatedList<T, I>`).
pub mod pagination;
/// Coalesced post-mutation refresh scheduling state.
pub mod post_mutation_refresh;
#[cfg(test)]
mod post_mutation_refresh_tests;
mod preferences_ops;
mod property_edit;
mod prs_inline_ops;
mod prs_load_ops;
mod prs_merge_ops;
mod prs_mutation_ops;
mod prs_nav_ops;
mod prs_ops;
mod prs_property_ops;
mod prs_thread_ops;
pub mod scrollback_ops;
mod selectors;
pub use selectors::ChooserAgentInfo;
pub(crate) use selectors::build_chooser_entries_from_state;
pub mod state_ops;
pub mod theme_picker_view;
pub mod transient_ops;
mod types;
mod util;

pub use errors_types::{ErrorsFocus, ErrorsState};
pub use events::*;
pub use issues_close_reason_ops::filter_duplicate_candidates;
pub use property_edit::PROPERTY_CLEAR_LABEL;
pub use scrollback_ops::{FollowIndicator, terminal_follow_indicator};
pub use state_ops::{delete_selected_agent, delete_selected_repository};
pub use types::*;
/// Default row jump for list and detail page navigation without a measured viewport.
pub(super) const VIEWPORT_PAGE_JUMP: usize = 10;
pub use form_projection::{
    AgentFormFieldVisibility, agent_form_visibility, effective_agent_kinds, effective_kinds_hint,
    is_field_visible, is_repository_field_visible, kind_from_form_value, next_visible_focus,
    next_visible_repository_focus, prev_visible_focus, prev_visible_repository_focus,
};

use tracing::{debug, trace};

use crate::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
use crate::list_viewport::ListMove;
use crate::messages::{
    AppMessage, MessageRoute, PersistenceMessage, RuntimeMessage, SystemMessage, ThemeMessage,
    UiNavigationMessage,
};

// Re-exported so sibling inline-cursor modules and tests can keep using
// `super::inline_cursor_vertical` after the helper moved into `util`.
pub use util::inline_cursor_vertical;

impl AppState {
    /// Reset terminal scrollback state to defaults (fix #4). Called from
    /// every path that changes the selected agent or repository.
    fn reset_terminal_scrollback(&mut self) {
        self.terminal_history_offset = None;
        self.terminal_viewport_rows = 0;
        self.terminal_total_lines = 0;
    }

    #[must_use]
    pub fn selected_repository_id(&self) -> Option<&RepositoryId> {
        self.selected_repository_index
            .and_then(|idx| self.repositories.get(idx).map(|repo| &repo.id))
    }

    #[must_use]
    pub fn repository_by_id(&self, repository_id: &RepositoryId) -> Option<&Repository> {
        self.repositories
            .iter()
            .find(|repo| &repo.id == repository_id)
    }

    #[must_use]
    pub fn repository_for_agent(&self, agent_id: &AgentId) -> Option<&Repository> {
        let agent = self.agents.iter().find(|agent| &agent.id == agent_id)?;
        self.repository_by_id(&agent.repository_id)
    }

    pub(super) fn first_unused_shortcut_slot(&self, ignore_agent: Option<&AgentId>) -> Option<u8> {
        (1u8..=9u8).find(|slot| {
            self.agents.iter().all(|agent| {
                if ignore_agent.is_some_and(|id| &agent.id == id) {
                    true
                } else {
                    agent.shortcut_slot != Some(*slot)
                }
            })
        })
    }

    fn enforce_shortcut_uniqueness(&mut self, owner_id: &AgentId, slot: Option<u8>) {
        let Some(slot) = slot else {
            return;
        };
        for agent in &mut self.agents {
            if agent.id != *owner_id && agent.shortcut_slot == Some(slot) {
                agent.shortcut_slot = None;
            }
        }
    }

    fn remember_selected_agent_for_current_repo(&mut self) {
        let selected_repo_id = self.selected_repository_id().cloned();
        let selected_agent_id = self.selected_agent().map(|agent| agent.id.clone());

        if let Some(repo_id) = selected_repo_id {
            if let Some(agent_id) = selected_agent_id {
                if let Some(entry) = self
                    .last_selected_agent_by_repo
                    .iter_mut()
                    .find(|(rid, _)| *rid == repo_id)
                {
                    entry.1 = agent_id;
                } else {
                    self.last_selected_agent_by_repo.push((repo_id, agent_id));
                }
            } else {
                self.last_selected_agent_by_repo
                    .retain(|(rid, _)| *rid != repo_id);
            }
        }
    }

    fn restore_selected_agent_for_current_repo(&mut self) {
        let Some(repo_id) = self.selected_repository_id().cloned() else {
            return;
        };

        let remembered_agent_id = self
            .last_selected_agent_by_repo
            .iter()
            .find(|(rid, _)| *rid == repo_id)
            .map(|(_, aid)| aid.clone());

        let visible_indices = self.agent_indices_for_repository(&repo_id);
        if visible_indices.is_empty() {
            self.selected_agent_index = None;
            return;
        }

        if let Some(agent_id) = remembered_agent_id
            && let Some(global_idx) = self
                .agents
                .iter()
                .position(|agent| agent.id == agent_id && agent.repository_id == repo_id)
            && visible_indices.contains(&global_idx)
        {
            self.selected_agent_index = Some(global_idx);
            return;
        }

        self.selected_agent_index = visible_indices.first().copied();
    }

    fn has_visible_agent_in_repository(&self, repository_id: &RepositoryId) -> bool {
        self.agents.iter().any(|agent| {
            &agent.repository_id == repository_id
                && (agent.is_running() || self.sticky_dead_agent_ids.contains(&agent.id))
        })
    }

    fn is_agent_visible_with_idle_filter(&self, agent: &Agent) -> bool {
        !self.hide_idle_repositories
            || agent.is_running()
            || self.sticky_dead_agent_ids.contains(&agent.id)
    }

    pub fn rebuild_repository_agent_ids(&mut self) {
        for repository in &mut self.repositories {
            repository.agent_ids.clear();
        }

        for agent in &self.agents {
            if let Some(repository) = self
                .repositories
                .iter_mut()
                .find(|repository| repository.id == agent.repository_id)
            {
                repository.agent_ids.push(agent.id.clone());
            }
        }
    }

    pub fn normalize_selection_indices(&mut self) {
        if self.repositories.is_empty() {
            self.selected_repository_index = None;
            self.selected_agent_index = None;
            return;
        }

        if self
            .selected_repository_index
            .is_some_and(|idx| idx >= self.repositories.len())
        {
            self.selected_repository_index = Some(self.repositories.len().saturating_sub(1));
        }

        let visible_repo_indices = self.visible_repository_indices();
        if visible_repo_indices.is_empty() {
            self.selected_repository_index = None;
            self.selected_agent_index = None;
            return;
        }

        let selected_repo_hidden = match self.selected_repository_index {
            Some(idx) => !visible_repo_indices.contains(&idx),
            None => true,
        };
        if selected_repo_hidden {
            self.selected_repository_index = visible_repo_indices.first().copied();
        }

        let Some(repository_id) = self.selected_repository_id().cloned() else {
            self.selected_agent_index = None;
            return;
        };

        let visible_indices = self.agent_indices_for_repository(&repository_id);
        if visible_indices.is_empty() {
            self.selected_agent_index = None;
            return;
        }

        if let Some(selected_idx) = self.selected_agent_index
            && visible_indices.contains(&selected_idx)
        {
            return;
        }

        self.selected_agent_index = visible_indices.first().copied();
    }

    #[must_use]
    pub fn selected_agent_local_index(&self) -> Option<usize> {
        let repository_id = self.selected_repository_id()?;
        let selected_global = self.selected_agent_index?;

        self.agent_indices_for_repository(repository_id)
            .iter()
            .position(|global_idx| *global_idx == selected_global)
    }

    /// Apply an event to produce the next state.
    #[must_use]
    pub fn apply(self, event: AppEvent) -> Self {
        self.apply_message(event.into())
    }

    /// Apply a routed domain message to produce the next state.
    ///
    /// State transitions are deterministic per REQ-TECH-003.
    /// @plan PLAN-20260216-FIRSTVERSION-V1.P05
    /// @requirement REQ-TECH-003
    /// @pseudocode component-001 lines 13-33
    #[must_use]
    pub fn apply_message(mut self, message: AppMessage) -> Self {
        let route = message.route();
        trace!(
            message_domain = ?route.domain,
            message = route.name,
            terminal_focused = self.terminal_focused,
            pane_focus = ?self.pane_focus,
            modal = ?std::mem::discriminant(&self.modal),
            "state.apply_message"
        );

        if self.terminal_focused && Self::terminal_blocks(&message) {
            debug!(
                message_domain = ?route.domain,
                message = route.name,
                "blocked navigation message (terminal_focused=true)"
            );
            return self;
        }

        match message {
            AppMessage::UiNavigation(message) => self.apply_ui_navigation(message),
            AppMessage::Modal(message) => self.apply_modal_message(message),
            AppMessage::RepositoryAgent(message) => self.apply_repository_agent_message(message),
            AppMessage::Runtime(message) => self.apply_runtime_message(message),
            AppMessage::Persistence(message) => self.apply_persistence_message(message),
            AppMessage::Theme(message) => self.apply_theme_message(message),
            AppMessage::System(message) => self.apply_system_message(message),
            AppMessage::Issues(message) => {
                let handled = self.apply_issues_message(message);
                debug_assert!(handled, "unhandled issues message in apply_message()");
            }
            AppMessage::PullRequests(message) => {
                let msg_debug = format!("{message:?}");
                let handled = self.apply_prs_message(message);
                debug_assert!(handled, "unhandled PullRequestsMessage: {msg_debug}");
            }
            AppMessage::Actions(message) => {
                let handled = self.apply_actions_message(message);
                debug_assert!(handled, "unhandled actions message in apply_message()");
            }
            AppMessage::Errors(message) => {
                let handled = self.apply_errors_message(message);
                debug_assert!(handled, "unhandled errors message in apply_message()");
            }
        }

        self.finalize_message(route);
        self
    }

    fn terminal_blocks(message: &AppMessage) -> bool {
        // Scrollback events and focus toggles are never blocked (issue #198).
        if let AppMessage::UiNavigation(msg) = message
            && matches!(
                msg,
                UiNavigationMessage::TerminalScrollUp
                    | UiNavigationMessage::TerminalScrollDown
                    | UiNavigationMessage::TerminalScrollPageUp
                    | UiNavigationMessage::TerminalScrollPageDown
                    | UiNavigationMessage::TerminalFollowTail
                    | UiNavigationMessage::TerminalScrollToTop
                    | UiNavigationMessage::ToggleTerminalFocus
                    | UiNavigationMessage::CyclePaneFocus
            )
        {
            return false;
        }
        matches!(
            message,
            AppMessage::UiNavigation(
                UiNavigationMessage::NavigateUp
                    | UiNavigationMessage::NavigateDown
                    | UiNavigationMessage::NavigateLeft
                    | UiNavigationMessage::NavigateRight
                    | UiNavigationMessage::SelectRepository(_)
                    | UiNavigationMessage::SelectAgent(_)
                    | UiNavigationMessage::JumpToAgentByShortcut(_)
            )
        )
    }

    fn finalize_message(&mut self, route: MessageRoute) {
        self.rebuild_repository_agent_ids();
        self.normalize_selection_indices();
        self.validate_dashboard_grab();
        errors_ops::capture_runtime_errors(self);
        self.last_selected_agent_by_repo
            .retain(|(repo_id, agent_id)| {
                self.repositories.iter().any(|repo| repo.id == *repo_id)
                    && self
                        .agents
                        .iter()
                        .any(|agent| agent.id == *agent_id && agent.repository_id == *repo_id)
            });

        trace!(
            message_domain = ?route.domain,
            message = route.name,
            terminal_focused = self.terminal_focused,
            pane_focus = ?self.pane_focus,
            modal = ?std::mem::discriminant(&self.modal),
            "state.apply_message complete"
        );
    }

    fn prepare_ui_navigation(&mut self, message: &UiNavigationMessage) {
        let changes_selection = matches!(
            message,
            UiNavigationMessage::NavigateUp
                | UiNavigationMessage::NavigateDown
                | UiNavigationMessage::NavigatePageUp(_)
                | UiNavigationMessage::NavigatePageDown(_)
                | UiNavigationMessage::NavigateHome
                | UiNavigationMessage::NavigateEnd
                | UiNavigationMessage::NavigateLeft
                | UiNavigationMessage::NavigateRight
                | UiNavigationMessage::SelectRepository(_)
                | UiNavigationMessage::SelectAgent(_)
                | UiNavigationMessage::JumpToAgentByShortcut(_)
        );
        if changes_selection {
            self.sticky_dead_agent_ids.clear();
            self.dashboard_grab = None;
        }
    }

    fn apply_ui_navigation(&mut self, message: UiNavigationMessage) {
        self.prepare_ui_navigation(&message);
        match message {
            UiNavigationMessage::NavigateUp => self.handle_navigate_up(),
            UiNavigationMessage::NavigateDown => self.handle_navigate_down(),
            UiNavigationMessage::NavigatePageUp(page) => {
                self.handle_navigate_page(ListMove::PageUp(page));
            }
            UiNavigationMessage::NavigatePageDown(page) => {
                self.handle_navigate_page(ListMove::PageDown(page));
            }
            UiNavigationMessage::NavigateHome => self.handle_navigate_page(ListMove::Home),
            UiNavigationMessage::NavigateEnd => self.handle_navigate_page(ListMove::End),
            UiNavigationMessage::NavigateLeft => self.move_pane_focus_left(),
            UiNavigationMessage::NavigateRight => self.move_pane_focus_right(),
            UiNavigationMessage::SelectRepository(idx) => self.select_repository_by_index(idx),
            UiNavigationMessage::SelectAgent(idx) => self.select_agent_by_local_index(idx),
            UiNavigationMessage::JumpToAgentByShortcut(slot) => {
                self.jump_to_agent_by_shortcut(slot);
            }
            UiNavigationMessage::CyclePaneFocus => self.cycle_pane_focus(),
            UiNavigationMessage::ToggleTerminalFocus => self.toggle_terminal_focus(),
            UiNavigationMessage::ToggleHideIdleRepositories => {
                self.hide_idle_repositories = !self.hide_idle_repositories;
                self.dashboard_grab = None;
                self.normalize_selection_indices();
            }
            UiNavigationMessage::EnterSplitMode => {
                self.screen_mode = ScreenMode::Split;
                self.pane_focus = PaneFocus::Repositories;
                self.dashboard_grab = None;
            }
            UiNavigationMessage::ExitSplitMode => self.exit_split_mode(),
            UiNavigationMessage::EnterGrabMode => {
                self.split_grab_index = self.selected_repository_visible_index();
            }
            UiNavigationMessage::ExitGrabMode => self.split_grab_index = None,
            UiNavigationMessage::GrabMoveUp => self.move_split_grab_up(),
            UiNavigationMessage::GrabMoveDown => self.move_split_grab_down(),
            UiNavigationMessage::SetSplitFilter(filter) => self.split_filter = filter,
            UiNavigationMessage::EnterDashboardGrab => self.enter_dashboard_grab(),
            UiNavigationMessage::ExitDashboardGrab => self.dashboard_grab = None,
            UiNavigationMessage::DashboardGrabMoveUp => self.move_dashboard_grab_up(),
            UiNavigationMessage::DashboardGrabMoveDown => self.move_dashboard_grab_down(),
            UiNavigationMessage::TerminalScrollUp
            | UiNavigationMessage::TerminalScrollDown
            | UiNavigationMessage::TerminalScrollPageUp
            | UiNavigationMessage::TerminalScrollPageDown
            | UiNavigationMessage::TerminalFollowTail
            | UiNavigationMessage::TerminalScrollToTop => {
                scrollback_ops::apply_terminal_scroll_message(
                    &mut self.terminal_history_offset,
                    self.terminal_total_lines,
                    self.terminal_viewport_rows,
                    message,
                );
            }
        }
    }

    fn cycle_pane_focus(&mut self) {
        let old = self.pane_focus;
        self.pane_focus = match self.pane_focus {
            PaneFocus::Repositories => PaneFocus::Agents,
            PaneFocus::Agents => PaneFocus::Terminal,
            PaneFocus::Terminal => PaneFocus::Repositories,
        };
        self.dashboard_grab = None;
        debug!(old = ?old, new = ?self.pane_focus, "pane focus changed (tab)");
    }

    fn move_pane_focus_right(&mut self) {
        let old = self.pane_focus;
        self.pane_focus = match self.pane_focus {
            PaneFocus::Repositories => PaneFocus::Agents,
            PaneFocus::Agents | PaneFocus::Terminal => PaneFocus::Terminal,
        };
        self.dashboard_grab = None;
        debug!(old = ?old, new = ?self.pane_focus, "pane focus changed (right)");
    }

    fn move_pane_focus_left(&mut self) {
        let old = self.pane_focus;
        self.pane_focus = match self.pane_focus {
            PaneFocus::Repositories | PaneFocus::Agents => PaneFocus::Repositories,
            PaneFocus::Terminal => PaneFocus::Agents,
        };
        self.dashboard_grab = None;
        debug!(old = ?old, new = ?self.pane_focus, "pane focus changed (left)");
    }

    fn toggle_terminal_focus(&mut self) {
        self.terminal_focused = !self.terminal_focused;
        debug!(
            terminal_focused = self.terminal_focused,
            "toggled terminal focus"
        );
    }

    fn exit_split_mode(&mut self) {
        self.screen_mode = ScreenMode::Dashboard;
        self.split_filter = None;
        self.split_grab_index = None;
    }

    fn select_repository_by_index(&mut self, idx: usize) {
        if idx < self.repositories.len()
            && (!self.hide_idle_repositories || self.visible_repository_indices().contains(&idx))
        {
            let prev_repo_id = self.current_repo_id();
            self.remember_selected_agent_for_current_repo();
            self.selected_repository_index = Some(idx);
            self.restore_selected_agent_for_current_repo();
            self.sync_preferences_for_repo_change(prev_repo_id);
            self.reset_terminal_scrollback();
        }
    }

    /// Move the repository selection up or down within the visible set.
    ///
    /// Shared by Issues and PR mode repo navigation (independent of pane_focus, #47).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @pseudocode component-001 lines 134-145
    fn move_repo_selection(&mut self, direction: crate::messages::NavDir) -> bool {
        let indices = self.visible_repository_indices();
        if indices.is_empty() {
            return false;
        }
        // Handle "no selection" explicitly: Down selects the FIRST visible repo
        // (indices[0]); Up is a no-op. This avoids the old `unwrap_or(0)` which
        // treated None as visible-index 0 and then Down computed target=1,
        // skipping the first repo entirely.
        let Some(current) = self.selected_repository_visible_index() else {
            if direction == crate::messages::NavDir::Down {
                self.remember_selected_agent_for_current_repo();
                self.selected_repository_index = Some(indices[0]);
                self.restore_selected_agent_for_current_repo();
                self.reset_terminal_scrollback();
                return true;
            }
            return false;
        };
        let target = match direction {
            crate::messages::NavDir::Up => {
                if current > 0 {
                    current - 1
                } else {
                    current
                }
            }
            crate::messages::NavDir::Down => {
                if current + 1 < indices.len() {
                    current + 1
                } else {
                    current
                }
            }
            _ => return false,
        };
        if target == current {
            return false;
        }
        self.remember_selected_agent_for_current_repo();
        self.selected_repository_index = Some(indices[target]);
        self.restore_selected_agent_for_current_repo();
        self.reset_terminal_scrollback();
        true
    }

    fn select_agent_by_local_index(&mut self, idx: usize) {
        if let Some(repository_id) = self.selected_repository_id().cloned() {
            let visible_indices = self.agent_indices_for_repository(&repository_id);
            if idx < visible_indices.len() {
                self.selected_agent_index = Some(visible_indices[idx]);
                self.remember_selected_agent_for_current_repo();
                self.reset_terminal_scrollback();
            }
        }
    }

    fn jump_to_agent_by_shortcut(&mut self, slot: u8) {
        if let Some((agent_idx, target_repo_id)) =
            self.agents.iter().enumerate().find_map(|(idx, agent)| {
                (agent.shortcut_slot == Some(slot) && self.is_agent_visible_with_idle_filter(agent))
                    .then_some((idx, agent.repository_id.clone()))
            })
            && let Some(target_repo_idx) = self
                .repositories
                .iter()
                .position(|repo| repo.id == target_repo_id)
            && (!self.hide_idle_repositories
                || self.visible_repository_indices().contains(&target_repo_idx))
        {
            let prev_repo_id = self.current_repo_id();
            self.remember_selected_agent_for_current_repo();
            self.selected_repository_index = Some(target_repo_idx);
            self.selected_agent_index = Some(agent_idx);
            self.pane_focus = PaneFocus::Agents;
            self.terminal_focused = false;
            self.reset_terminal_scrollback();
            self.remember_selected_agent_for_current_repo();
            self.sync_preferences_for_repo_change(prev_repo_id);
        }
    }

    fn move_split_grab_up(&mut self) {
        if let Some(grab_visible_idx) = self.split_grab_index
            && grab_visible_idx > 0
        {
            let visible_repo_indices = self.visible_repository_indices();
            if let (Some(&current_global_idx), Some(&target_global_idx)) = (
                visible_repo_indices.get(grab_visible_idx),
                visible_repo_indices.get(grab_visible_idx - 1),
            ) {
                self.repositories
                    .swap(current_global_idx, target_global_idx);
                self.split_grab_index = Some(grab_visible_idx - 1);
                self.selected_repository_index = Some(target_global_idx);
            }
        }
    }

    fn move_split_grab_down(&mut self) {
        if let Some(grab_visible_idx) = self.split_grab_index {
            let visible_repo_indices = self.visible_repository_indices();
            if grab_visible_idx + 1 < visible_repo_indices.len()
                && let (Some(&current_global_idx), Some(&target_global_idx)) = (
                    visible_repo_indices.get(grab_visible_idx),
                    visible_repo_indices.get(grab_visible_idx + 1),
                )
            {
                self.repositories
                    .swap(current_global_idx, target_global_idx);
                self.split_grab_index = Some(grab_visible_idx + 1);
                self.selected_repository_index = Some(target_global_idx);
            }
        }
    }
    fn apply_runtime_message(&mut self, message: RuntimeMessage) {
        match message {
            RuntimeMessage::KillAgent(agent_id) => {
                if let Some(agent) = self.agents.iter_mut().find(|a| a.id == agent_id) {
                    agent.status = AgentStatus::Dead;
                    agent.runtime_binding = None;
                    self.sticky_dead_agent_ids.insert(agent_id);
                }
            }
            RuntimeMessage::AgentStatusChanged(agent_id, status) => {
                if let Some(agent) = self.agents.iter_mut().find(|a| a.id == agent_id) {
                    agent.status = status;
                    if status == AgentStatus::Running {
                        self.sticky_dead_agent_ids.remove(&agent_id);
                    }
                    // Reset scroll state when selected agent's status changes
                    // (fix #6).
                    if self.selected_agent().is_some_and(|a| a.id == agent_id) {
                        self.reset_terminal_scrollback();
                    }
                }
            }
            RuntimeMessage::RelaunchAgent(agent_id) => {
                if let Some(agent) = self.agents.iter_mut().find(|a| a.id == agent_id)
                    && agent.runtime_binding.is_some()
                {
                    agent.status = AgentStatus::Running;
                    self.sticky_dead_agent_ids.remove(&agent_id);
                }
            }
            // RestartAgent handles the edge case where apply_and_persist is
            // called with RestartAgent directly (not via dispatch). The normal
            // path goes through dispatch_restart_agent which applies Kill then
            // Relaunch separately. Here we clear sticky and set Running.
            RuntimeMessage::RestartAgent(agent_id) => {
                self.sticky_dead_agent_ids.remove(&agent_id);
                if let Some(agent) = self.agents.iter_mut().find(|a| a.id == agent_id)
                    && agent.runtime_binding.is_some()
                {
                    agent.status = AgentStatus::Running;
                }
            }
        }
    }

    fn apply_persistence_message(&mut self, message: PersistenceMessage) {
        match message {
            PersistenceMessage::LoadSuccess | PersistenceMessage::SaveSuccess => {
                self.error_message = None;
            }
            PersistenceMessage::LoadFailed(msg) | PersistenceMessage::SaveFailed(msg) => {
                self.error_message = Some(msg);
            }
        }
    }

    fn apply_theme_message(&mut self, message: ThemeMessage) {
        match message {
            ThemeMessage::ResolveFailed(msg) => self.warning_message = Some(msg),
            ThemeMessage::SetTheme(_) => {}
            ThemeMessage::OpenThemePicker {
                available_themes,
                active_slug,
            } => {
                let selected_index = available_themes
                    .iter()
                    .position(|(slug, _)| *slug == active_slug)
                    .unwrap_or(0);
                self.modal = ModalState::ThemePicker {
                    available_themes,
                    selected_index,
                    active_slug,
                    override_theme: self.override_agent_theme,
                };
            }
            ThemeMessage::PickerNavigateUp => {
                if let ModalState::ThemePicker { selected_index, .. } = &mut self.modal
                    && *selected_index > 0
                {
                    *selected_index -= 1;
                }
            }
            ThemeMessage::PickerNavigateDown => {
                if let ModalState::ThemePicker {
                    available_themes,
                    selected_index,
                    ..
                } = &mut self.modal
                    && *selected_index + 1 < available_themes.len()
                {
                    *selected_index += 1;
                }
            }
            ThemeMessage::ToggleAgentThemeOverride => {
                if let ModalState::ThemePicker { override_theme, .. } = &mut self.modal {
                    *override_theme = !*override_theme;
                }
            }
            ThemeMessage::PickerConfirm => {
                // Commit the in-dialog override toggle to the runtime mirror
                // before closing (issue #179). Persistence is applied by the
                // input layer; this keeps the state transition deterministic.
                if let ModalState::ThemePicker { override_theme, .. } = &self.modal {
                    self.override_agent_theme = *override_theme;
                }
                self.modal = ModalState::None;
            }
            ThemeMessage::PickerCancel => {
                self.modal = ModalState::None;
            }
        }
    }

    fn apply_system_message(&mut self, message: SystemMessage) {
        match message {
            SystemMessage::ClearError => self.error_message = None,
            SystemMessage::ClearWarning => self.warning_message = None,
            SystemMessage::Quit => {}
            SystemMessage::TransientAgentQueued { queue_position } => {
                self.apply_transient_queued(queue_position);
            }
            SystemMessage::TransientAgentDequeued => self.clear_transient_notice(),
            auth => self.apply_auth_message(auth),
        }
    }

    fn handle_navigate_up(&mut self) {
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

    fn handle_navigate_down(&mut self) {
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
#[cfg(test)]
#[path = "auth_ops_tests.rs"]
mod auth_ops_tests;
#[cfg(test)]
mod confirm_focus_tests;
#[cfg(test)]
mod errors_tests;
#[cfg(test)]
mod issues_test_fixtures;
#[cfg(test)]
#[path = "issues_tests.rs"]
mod issues_tests;
#[cfg(test)]
#[path = "issues_tests_close_delete.rs"]
mod issues_tests_close_delete;
#[cfg(test)]
#[path = "issues_tests_close_reason.rs"]
mod issues_tests_close_reason;
#[cfg(test)]
#[path = "issues_tests_components.rs"]
mod issues_tests_components;
#[cfg(test)]
#[path = "issues_tests_composer_focus.rs"]
mod issues_tests_composer_focus;
#[cfg(test)]
mod issues_tests_create;
#[cfg(test)]
#[path = "issues_tests_detail.rs"]
mod issues_tests_detail;
#[cfg(test)]
mod issues_tests_detail_content;
#[cfg(test)]
#[path = "issues_tests_detail_flow.rs"]
mod issues_tests_detail_flow;
#[cfg(test)]
#[path = "issues_tests_filter.rs"]
mod issues_tests_filter;
#[cfg(test)]
#[path = "issues_tests_mutations.rs"]
mod issues_tests_mutations;
#[cfg(test)]
#[path = "issues_tests_repo_nav.rs"]
mod issues_tests_repo_nav;
#[cfg(test)]
#[path = "issues_tests_self_assignment.rs"]
mod issues_tests_self_assignment;
#[cfg(test)]
#[path = "issues_tests_send_to_agent.rs"]
mod issues_tests_send_to_agent;
#[cfg(test)]
#[path = "issues_tests_subfocus.rs"]
mod issues_tests_subfocus;
#[cfg(test)]
#[path = "preferences_tests.rs"]
mod preferences_tests;
#[cfg(test)]
#[path = "prs_integration_tests.rs"]
mod prs_integration_tests;
#[cfg(test)]
#[path = "prs_test_fixtures.rs"]
mod prs_test_fixtures;
#[cfg(test)]
#[path = "prs_tests.rs"]
mod prs_tests;
#[cfg(test)]
#[path = "prs_tests_bodyless_review_nav.rs"]
mod prs_tests_bodyless_review_nav;
#[cfg(test)]
#[path = "prs_tests_chooser_security.rs"]
mod prs_tests_chooser_security;
#[cfg(test)]
#[path = "prs_tests_components.rs"]
mod prs_tests_components;
#[cfg(test)]
#[path = "prs_tests_composer_focus.rs"]
mod prs_tests_composer_focus;
#[cfg(test)]
#[path = "prs_tests_cursor_arrows.rs"]
mod prs_tests_cursor_arrows;
#[cfg(test)]
#[path = "prs_tests_detail.rs"]
mod prs_tests_detail;
#[cfg(test)]
#[path = "prs_tests_detail_flow.rs"]
mod prs_tests_detail_flow;
#[cfg(test)]
#[path = "prs_tests_filter.rs"]
mod prs_tests_filter;
#[cfg(test)]
#[path = "prs_tests_merge.rs"]
mod prs_tests_merge;
#[cfg(test)]
#[path = "prs_tests_pagination.rs"]
mod prs_tests_pagination;
#[cfg(test)]
#[path = "prs_tests_repo_nav.rs"]
mod prs_tests_repo_nav;
#[cfg(test)]
#[path = "prs_tests_review_order.rs"]
mod prs_tests_review_order;
#[cfg(test)]
#[path = "prs_tests_review_threads.rs"]
mod prs_tests_review_threads;
#[cfg(test)]
#[path = "prs_tests_silent_refresh.rs"]
mod prs_tests_silent_refresh;
#[cfg(test)]
#[path = "transient_agent_tests.rs"]
mod transient_agent_tests;
#[cfg(test)]
#[path = "transient_system_message_tests.rs"]
mod transient_system_message_tests;
