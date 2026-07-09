//! Application state and event layer.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @requirement REQ-TECH-001
//! @requirement REQ-TECH-003
//!
//! Pseudocode reference: component-001 lines 01-12

mod form_ops;
mod issues_ops;
pub mod state_ops;
pub mod theme_picker_view;
mod types;
mod util;

pub use state_ops::{delete_selected_agent, delete_selected_repository};
pub use types::*;

use tracing::{debug, trace};

use crate::domain::{Agent, AgentId, AgentStatus, DEFAULT_SANDBOX_FLAGS, SandboxEngine};
use crate::domain::{Repository, RepositoryId};

/// Move the inline editor cursor up or down by `direction` lines (-1 = up, 1 = down).
/// Attempts to land on the same column in the target line, clamping to line length.
fn inline_cursor_vertical(text: &str, cursor: &mut usize, direction: i32) {
    // Find line boundaries and current line/column
    let mut line_starts: Vec<usize> = vec![0];
    for (i, ch) in text.char_indices() {
        if ch == char::from(0x0Au8) {
            line_starts.push(i + ch.len_utf8());
        }
    }

    // Find which line the cursor is on
    let mut current_line = 0;
    for (i, &start) in line_starts.iter().enumerate() {
        if *cursor >= start {
            current_line = i;
        }
    }

    let col = *cursor - line_starts[current_line];
    let target_line = if direction < 0 {
        current_line.saturating_sub(1)
    } else {
        (current_line + 1).min(line_starts.len() - 1)
    };

    if target_line == current_line {
        return; // already at first/last line
    }

    // Find end of target line
    let target_start = line_starts[target_line];
    let target_end = if target_line + 1 < line_starts.len() {
        // end is just before the newline
        line_starts[target_line + 1] - 1
    } else {
        text.len()
    };

    let target_len = target_end - target_start;
    let raw_pos = target_start + col.min(target_len);
    // Snap to nearest char boundary at or before raw_pos.
    // Use char end positions (start + len) since cursor can sit after the last char.
    let target_slice = &text[target_start..target_end];
    let snapped = target_slice
        .char_indices()
        .map(|(i, c)| target_start + i + c.len_utf8())
        .take_while(|end_pos| *end_pos <= raw_pos)
        .last()
        .unwrap_or(target_start);
    *cursor = snapped.min(target_end);
}

impl AppState {
    fn selected_repository_id(&self) -> Option<&RepositoryId> {
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

    fn first_unused_shortcut_slot(&self, ignore_agent: Option<&AgentId>) -> Option<u8> {
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

    fn has_running_agent_in_repository(&self, repository_id: &RepositoryId) -> bool {
        self.agents
            .iter()
            .any(|agent| &agent.repository_id == repository_id && agent.is_running())
    }

    fn is_agent_visible_with_idle_filter(&self, agent: &Agent) -> bool {
        !self.hide_idle_repositories || agent.is_running()
    }

    #[must_use]
    pub fn visible_repository_indices(&self) -> Vec<usize> {
        self.repositories
            .iter()
            .enumerate()
            .filter_map(|(idx, repository)| {
                (!self.hide_idle_repositories
                    || self.has_running_agent_in_repository(&repository.id))
                .then_some(idx)
            })
            .collect()
    }

    #[must_use]
    pub fn selected_repository_visible_index(&self) -> Option<usize> {
        let selected = self.selected_repository_index?;
        self.visible_repository_indices()
            .iter()
            .position(|idx| *idx == selected)
    }

    #[must_use]
    pub fn agent_indices_for_repository(&self, repository_id: &RepositoryId) -> Vec<usize> {
        self.agents
            .iter()
            .enumerate()
            .filter_map(|(idx, agent)| {
                (&agent.repository_id == repository_id
                    && self.is_agent_visible_with_idle_filter(agent))
                .then_some(idx)
            })
            .collect()
    }

    /// Return the visible agents for a repository, respecting the idle filter.
    ///
    /// This uses `agent_indices_for_repository` internally so the returned
    /// list is always consistent with `selected_agent_local_index`.
    #[must_use]
    pub fn visible_agents_for_repository(&self, repository_id: &RepositoryId) -> Vec<Agent> {
        self.agent_indices_for_repository(repository_id)
            .iter()
            .filter_map(|idx| self.agents.get(*idx).cloned())
            .collect()
    }

    /// Count of visible agents for a repository, respecting the idle filter.
    #[must_use]
    pub fn visible_agent_count_for_repository(&self, repository_id: &RepositoryId) -> usize {
        self.agent_indices_for_repository(repository_id).len()
    }

    /// Total count of visible agents across all repositories.
    #[must_use]
    pub fn visible_agent_count(&self) -> usize {
        self.agents
            .iter()
            .filter(|agent| self.is_agent_visible_with_idle_filter(agent))
            .count()
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

        #[allow(clippy::unnecessary_map_or)]
        if self
            .selected_repository_index
            .map_or(true, |idx| !visible_repo_indices.contains(&idx))
        {
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
    ///
    /// State transitions are deterministic per REQ-TECH-003.
    /// @plan PLAN-20260216-FIRSTVERSION-V1.P05
    /// @requirement REQ-TECH-003
    /// @pseudocode component-001 lines 13-33
    #[must_use]
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub fn apply(mut self, event: AppEvent) -> Self {
        trace!(
            event = ?event,
            terminal_focused = self.terminal_focused,
            pane_focus = ?self.pane_focus,
            modal = ?std::mem::discriminant(&self.modal),
            "state.apply"
        );

        // When terminal is focused, navigation events are forwarded to PTY
        // and should NOT change UI selection state.
        // However, CyclePaneFocus is allowed (user can switch panes even while F12 active).
        if self.terminal_focused {
            match &event {
                AppEvent::NavigateUp
                | AppEvent::NavigateDown
                | AppEvent::NavigateLeft
                | AppEvent::NavigateRight
                | AppEvent::SelectRepository(_)
                | AppEvent::SelectAgent(_)
                | AppEvent::JumpToAgentByShortcut(_) => {
                    debug!(event = ?event, "blocked navigation event (terminal_focused=true)");
                    return self;
                }
                _ => {}
            }
        }

        match event {
            // Navigation
            AppEvent::NavigateUp => self.handle_navigate_up(),
            AppEvent::NavigateDown => self.handle_navigate_down(),
            AppEvent::SelectRepository(idx) => {
                if idx < self.repositories.len()
                    && (!self.hide_idle_repositories
                        || self.visible_repository_indices().contains(&idx))
                {
                    self.remember_selected_agent_for_current_repo();
                    self.selected_repository_index = Some(idx);
                    self.restore_selected_agent_for_current_repo();

                    // Scope change while in issues mode invalidates loaded data.
                    // @plan PLAN-20260329-ISSUES-MODE.P15
                    // @requirement REQ-ISS-001, REQ-ISS-013
                    if self.issues_state.active {
                        self.reset_issues_for_repo_change();
                    }
                }
            }
            AppEvent::SelectAgent(idx) => {
                if let Some(repository_id) = self.selected_repository_id().cloned() {
                    let visible_indices = self.agent_indices_for_repository(&repository_id);
                    if idx < visible_indices.len() {
                        self.selected_agent_index = Some(visible_indices[idx]);
                        self.remember_selected_agent_for_current_repo();
                    }
                }
            }
            AppEvent::JumpToAgentByShortcut(slot) => {
                if let Some((agent_idx, target_repo_id)) =
                    self.agents.iter().enumerate().find_map(|(idx, agent)| {
                        (agent.shortcut_slot == Some(slot)
                            && self.is_agent_visible_with_idle_filter(agent))
                        .then_some((idx, agent.repository_id.clone()))
                    })
                    && let Some(target_repo_idx) = self
                        .repositories
                        .iter()
                        .position(|repo| repo.id == target_repo_id)
                    && (!self.hide_idle_repositories
                        || self.visible_repository_indices().contains(&target_repo_idx))
                {
                    self.remember_selected_agent_for_current_repo();
                    self.selected_repository_index = Some(target_repo_idx);
                    self.selected_agent_index = Some(agent_idx);
                    self.pane_focus = PaneFocus::Agents;
                    self.terminal_focused = false;
                    self.remember_selected_agent_for_current_repo();
                }
            }

            // Focus — Tab wraps around, Right arrow clamps at Terminal.
            AppEvent::CyclePaneFocus => {
                let old = self.pane_focus;
                self.pane_focus = match self.pane_focus {
                    PaneFocus::Repositories => PaneFocus::Agents,
                    PaneFocus::Agents => PaneFocus::Terminal,
                    PaneFocus::Terminal => PaneFocus::Repositories,
                };
                debug!(old = ?old, new = ?self.pane_focus, "pane focus changed (tab)");
            }
            AppEvent::NavigateRight => {
                let old = self.pane_focus;
                self.pane_focus = match self.pane_focus {
                    PaneFocus::Repositories => PaneFocus::Agents,
                    PaneFocus::Agents | PaneFocus::Terminal => PaneFocus::Terminal,
                };
                debug!(old = ?old, new = ?self.pane_focus, "pane focus changed (right)");
            }
            AppEvent::ToggleTerminalFocus => {
                self.terminal_focused = !self.terminal_focused;
                debug!(
                    terminal_focused = self.terminal_focused,
                    "toggled terminal focus"
                );
            }
            AppEvent::ToggleHideIdleRepositories => {
                self.hide_idle_repositories = !self.hide_idle_repositories;
                self.normalize_selection_indices();
            }

            // Screen mode
            AppEvent::EnterSplitMode => {
                self.screen_mode = ScreenMode::Split;
            }
            AppEvent::ExitSplitMode => {
                self.screen_mode = ScreenMode::Dashboard;
                self.split_filter = None;
                self.split_grab_index = None;
            }

            // Grab mode for split view reordering
            AppEvent::EnterGrabMode => {
                self.split_grab_index = self.selected_repository_visible_index();
            }
            AppEvent::ExitGrabMode => {
                self.split_grab_index = None;
            }
            AppEvent::GrabMoveUp => {
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
            AppEvent::GrabMoveDown => {
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
            AppEvent::SetSplitFilter(filter) => {
                self.split_filter = filter;
            }

            // Modal/form actions
            AppEvent::OpenHelp => {
                self.modal = ModalState::Help;
            }
            AppEvent::OpenSearch => {
                self.modal = ModalState::Search {
                    query: String::new(),
                };
            }
            AppEvent::CloseModal | AppEvent::ThemePickerConfirm(_) | AppEvent::CloseThemePicker => {
                self.modal = ModalState::None;
            }
            AppEvent::SubmitForm => {
                self.handle_submit_form();
            }

            // Form input events
            AppEvent::FormChar(c) => {
                self.handle_form_char(c);
            }
            AppEvent::FormBackspace => {
                self.handle_form_backspace();
            }
            AppEvent::FormDelete => {
                self.handle_form_delete();
            }
            AppEvent::FormMoveCursorLeft => {
                self.handle_form_move_cursor_left();
            }
            AppEvent::FormMoveCursorRight => {
                self.handle_form_move_cursor_right();
            }
            AppEvent::FormNextField => {
                self.handle_form_next_field();
            }
            AppEvent::FormPrevField => {
                self.handle_form_prev_field();
            }
            AppEvent::FormToggleCheckbox => {
                self.handle_form_toggle_checkbox();
            }

            // CRUD
            AppEvent::OpenNewRepository => {
                self.modal = ModalState::NewRepository {
                    fields: RepositoryFormFields::default(),
                    focus: RepositoryFormFocus::default(),
                    cursor: RepositoryFormCursor::default(),
                };
            }
            AppEvent::OpenEditRepository(id) => {
                let fields = self
                    .repositories
                    .iter()
                    .find(|r| r.id == id)
                    .map(|r| RepositoryFormFields {
                        name: r.name.clone(),
                        base_dir: r.base_dir.to_string_lossy().into_owned(),
                        default_profile: r.default_profile.clone(),
                        github_repo: r.github_repo.clone(),
                        remote_enabled: r.remote.enabled,
                        login_user: r.remote.login_user.clone(),
                        host: r.remote.host.clone(),
                        run_as_user: r.remote.run_as_user.clone(),
                        setup_env_default: r.remote.setup_env_default,
                    })
                    .unwrap_or_default();
                self.modal = ModalState::EditRepository {
                    id,
                    cursor: RepositoryFormCursor {
                        name: fields.name.chars().count(),
                        base_dir: fields.base_dir.chars().count(),
                        default_profile: fields.default_profile.chars().count(),
                        github_repo: fields.github_repo.chars().count(),
                        login_user: fields.login_user.chars().count(),
                        host: fields.host.chars().count(),
                        run_as_user: fields.run_as_user.chars().count(),
                    },
                    fields,
                    focus: RepositoryFormFocus::default(),
                };
            }
            AppEvent::OpenDeleteRepository(id) => {
                self.modal = ModalState::ConfirmDeleteRepository { id };
            }
            AppEvent::OpenNewAgent(repository_id) => {
                let (base_dir, default_profile) = self
                    .repositories
                    .iter()
                    .find(|r| r.id == repository_id)
                    .map(|r| {
                        (
                            r.base_dir.to_string_lossy().into_owned(),
                            r.default_profile.clone(),
                        )
                    })
                    .unwrap_or_default();

                let work_dir_len = base_dir.chars().count();
                let profile_len = default_profile.chars().count();

                self.modal = ModalState::NewAgent {
                    repository_id,
                    fields: AgentFormFields {
                        shortcut_slot: self.first_unused_shortcut_slot(None),
                        name: String::new(),
                        description: String::new(),
                        work_dir: base_dir,
                        profile: default_profile,
                        mode: "--yolo".to_owned(),
                        llxprt_debug: String::new(),
                        pass_continue: true,
                        sandbox_enabled: false,
                        sandbox_engine: SandboxEngine::Podman.label().to_owned(),
                        sandbox_flags: DEFAULT_SANDBOX_FLAGS.to_owned(),
                    },
                    cursor: AgentFormCursor {
                        work_dir: work_dir_len,
                        profile: profile_len,
                        mode: "--yolo".chars().count(),
                        sandbox_flags: DEFAULT_SANDBOX_FLAGS.chars().count(),
                        ..AgentFormCursor::default()
                    },
                    focus: AgentFormFocus::default(),
                    work_dir_manual: false,
                };
            }
            AppEvent::OpenEditAgent(id) => {
                let fields = self
                    .agents
                    .iter()
                    .find(|a| a.id == id)
                    .map(|a| AgentFormFields {
                        shortcut_slot: a.shortcut_slot,
                        name: a.name.clone(),
                        description: a.description.clone(),
                        work_dir: a.work_dir.to_string_lossy().into_owned(),
                        profile: a.profile.clone(),
                        mode: a.mode_flags.join(" "),
                        llxprt_debug: a.llxprt_debug.clone(),
                        pass_continue: a.pass_continue,
                        sandbox_enabled: a.sandbox_enabled,
                        sandbox_engine: a.sandbox_engine.label().to_owned(),
                        sandbox_flags: a.sandbox_flags.clone(),
                    })
                    .unwrap_or_default();
                self.modal = ModalState::EditAgent {
                    id,
                    cursor: AgentFormCursor {
                        name: fields.name.chars().count(),
                        description: fields.description.chars().count(),
                        work_dir: fields.work_dir.chars().count(),
                        profile: fields.profile.chars().count(),
                        mode: fields.mode.chars().count(),
                        llxprt_debug: fields.llxprt_debug.chars().count(),
                        sandbox_flags: fields.sandbox_flags.chars().count(),
                    },
                    fields,
                    focus: AgentFormFocus::default(),
                };
            }
            AppEvent::OpenDeleteAgent(id) => {
                self.modal = ModalState::ConfirmDeleteAgent {
                    id,
                    delete_work_dir: false,
                };
            }
            AppEvent::ToggleDeleteWorkDir => {
                if let ModalState::ConfirmDeleteAgent {
                    id,
                    delete_work_dir,
                } = self.modal.clone()
                {
                    self.modal = ModalState::ConfirmDeleteAgent {
                        id,
                        delete_work_dir: !delete_work_dir,
                    };
                }
            }

            // Lifecycle
            AppEvent::KillAgent(ref agent_id) => {
                if let Some(agent) = self.agents.iter_mut().find(|a| &a.id == agent_id) {
                    agent.status = AgentStatus::Dead;
                }
            }
            AppEvent::AgentStatusChanged(ref agent_id, status) => {
                if let Some(agent) = self.agents.iter_mut().find(|a| &a.id == agent_id) {
                    agent.status = status;
                }
            }
            AppEvent::RelaunchAgent(ref agent_id) => {
                if let Some(agent) = self.agents.iter_mut().find(|a| &a.id == agent_id)
                    && agent.runtime_binding.is_some()
                {
                    agent.status = AgentStatus::Running;
                }
            }

            // Persistence results - clear or set error
            AppEvent::PersistenceLoadSuccess | AppEvent::ClearError => {
                self.error_message = None;
            }
            AppEvent::PersistenceLoadFailed(msg) | AppEvent::PersistenceSaveFailed(msg) => {
                self.error_message = Some(msg);
            }

            // Theme
            AppEvent::ThemeResolveFailed(msg) => {
                self.warning_message = Some(msg);
            }
            AppEvent::OpenThemePicker {
                available_themes,
                active_slug,
            } => {
                // Default selection to the currently active theme.
                let selected_index = available_themes
                    .iter()
                    .position(|(slug, _)| *slug == active_slug)
                    .unwrap_or(0);
                self.modal = ModalState::ThemePicker {
                    available_themes,
                    selected_index,
                };
            }
            AppEvent::ThemePickerNavigateUp => {
                if let ModalState::ThemePicker { selected_index, .. } = &mut self.modal {
                    if *selected_index > 0 {
                        *selected_index -= 1;
                    }
                }
            }
            AppEvent::ThemePickerNavigateDown => {
                if let ModalState::ThemePicker {
                    available_themes,
                    selected_index,
                } = &mut self.modal
                {
                    if *selected_index + 1 < available_themes.len() {
                        *selected_index += 1;
                    }
                }
            }

            // Clear warning
            AppEvent::ClearWarning => {
                self.warning_message = None;
            }

            // Pane focus navigation — Left/Right clamp at boundaries (no wrapping).
            AppEvent::NavigateLeft => {
                let old = self.pane_focus;
                self.pane_focus = match self.pane_focus {
                    PaneFocus::Repositories | PaneFocus::Agents => PaneFocus::Repositories,
                    PaneFocus::Terminal => PaneFocus::Agents,
                };
                debug!(old = ?old, new = ?self.pane_focus, "pane focus changed (left)");
            }

            // No-op events (handled elsewhere or reserved)
            AppEvent::PersistenceSaveSuccess
            | AppEvent::SetTheme(_)
            | AppEvent::Quit
            | AppEvent::ApplySearch
            | AppEvent::InlineSubmit => {}

            // Issues mode events — delegated to issues_ops.rs
            event => {
                let handled = self.apply_issues_event(event);
                debug_assert!(handled, "unhandled AppEvent variant in apply()");
            }
        }

        self.rebuild_repository_agent_ids();
        self.normalize_selection_indices();
        self.last_selected_agent_by_repo
            .retain(|(repo_id, agent_id)| {
                self.repositories.iter().any(|repo| repo.id == *repo_id)
                    && self
                        .agents
                        .iter()
                        .any(|agent| agent.id == *agent_id && agent.repository_id == *repo_id)
            });
        self
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
                    }
                    Some(_) => {}
                    None => {
                        self.selected_agent_index = visible_indices.first().copied();
                        self.remember_selected_agent_for_current_repo();
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
                    }
                    Some(_) => {}
                    None => {
                        self.selected_agent_index = visible_indices.first().copied();
                        self.remember_selected_agent_for_current_repo();
                    }
                }
            }
            PaneFocus::Terminal => {}
        }
    }

    /// Get the currently selected repository, if any.
    #[must_use]
    pub fn selected_repository(&self) -> Option<&Repository> {
        self.selected_repository_index
            .and_then(|i| self.repositories.get(i))
    }

    /// Get the currently selected agent, if any.
    #[must_use]
    pub fn selected_agent(&self) -> Option<&Agent> {
        let repository_id = self.selected_repository_id()?;
        let selected_idx = self.selected_agent_index?;
        let agent = self.agents.get(selected_idx)?;
        (&agent.repository_id == repository_id && self.is_agent_visible_with_idle_filter(agent))
            .then_some(agent)
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::field_reassign_with_default,
    clippy::manual_string_new,
    clippy::uninlined_format_args
)]
#[path = "issues_tests.rs"]
mod issues_tests;

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::field_reassign_with_default,
    clippy::manual_string_new,
    clippy::uninlined_format_args
)]
#[path = "issues_tests_detail.rs"]
mod issues_tests_detail;

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::field_reassign_with_default,
    clippy::manual_string_new,
    clippy::uninlined_format_args
)]
#[path = "issues_tests_repo_nav.rs"]
mod issues_tests_repo_nav;

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::field_reassign_with_default,
    clippy::manual_string_new,
    clippy::uninlined_format_args
)]
#[path = "issues_tests_filter.rs"]
mod issues_tests_filter;
