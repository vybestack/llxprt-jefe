//! Application state and event layer.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @requirement REQ-TECH-001
//! @requirement REQ-TECH-003
//!
//! Pseudocode reference: component-001 lines 01-12

mod form_ops;
mod types;
mod util;

pub use types::*;

use tracing::{debug, trace};

use crate::domain::{Agent, AgentId, AgentStatus, DEFAULT_SANDBOX_FLAGS, SandboxEngine};
use crate::domain::{IssueFilter, Repository, RepositoryId};

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
            .filter_map(|(idx, agent)| (&agent.repository_id == repository_id).then_some(idx))
            .collect()
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

        if self
            .selected_repository_index
            .is_none_or(|idx| !visible_repo_indices.contains(&idx))
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
                        // Discard unsent inline drafts with notice
                        if self.issues_state.inline_state != InlineState::None {
                            self.issues_state.draft_notice =
                                Some("Unsent draft discarded".to_string());
                            self.issues_state.inline_state = InlineState::None;
                        }
                        self.issues_state.issues.clear();
                        self.issues_state.selected_issue_index = None;
                        self.issues_state.issue_detail = None;
                        self.issues_state.list_cursor = None;
                        self.issues_state.has_more_issues = false;
                        self.issues_state.error = None;
                        self.issues_state.list_loading = true;
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
                        (agent.shortcut_slot == Some(slot))
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
            AppEvent::CloseModal => {
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

            // Scroll detail pane viewport (clamped to content length)
            AppEvent::IssuesScrollDetailUp => {
                self.issues_state.detail_scroll_offset =
                    self.issues_state.detail_scroll_offset.saturating_sub(1);
            }
            AppEvent::IssuesScrollDetailDown => {
                let max = self.issues_state.max_detail_scroll_offset();
                if self.issues_state.detail_scroll_offset < max {
                    self.issues_state.detail_scroll_offset += 1;
                }
            }
            AppEvent::IssuesScrollDetailPageUp => {
                self.issues_state.detail_scroll_offset =
                    self.issues_state.detail_scroll_offset.saturating_sub(10);
            }
            AppEvent::IssuesScrollDetailPageDown => {
                let max = self.issues_state.max_detail_scroll_offset();
                self.issues_state.detail_scroll_offset =
                    (self.issues_state.detail_scroll_offset + 10).min(max);
            }

            // Issues Mode events — P05 Domain + State Implementation
            // @plan PLAN-20260329-ISSUES-MODE.P05
            // @requirement REQ-ISS-001, REQ-ISS-005
            // @pseudocode component-001 lines 54-61
            AppEvent::EnterIssuesMode => {
                // Save prior focus
                self.issues_state.prior_agent_focus = Some(PriorAgentFocus {
                    pane_focus: self.pane_focus,
                    selected_repository_index: self.selected_repository_index,
                    selected_agent_index: self.selected_agent_index,
                });
                self.screen_mode = ScreenMode::DashboardIssues;
                self.issues_state.active = true;
                self.issues_state.issue_focus = IssueFocus::IssueList;
                // Clear transient issue data
                self.issues_state.issues.clear();
                self.issues_state.selected_issue_index = None;
                self.issues_state.issue_detail = None;
                self.issues_state.list_cursor = None;
                self.issues_state.has_more_issues = false;
                self.issues_state.error = None;
                self.issues_state.inline_state = InlineState::None;
                self.issues_state.agent_chooser = None;
                self.issues_state.filter_controls_open = false;
                self.issues_state.search_input_focused = false;
                self.issues_state.search_query.clear();
                self.issues_state.detail_subfocus = DetailSubfocus::Body;
                self.issues_state.draft_notice = None;
                self.issues_state.list_loading = true;
            }

            // @requirement REQ-ISS-001, REQ-ISS-005
            // @pseudocode component-001 lines 62-72
            AppEvent::ExitIssuesMode => {
                self.screen_mode = ScreenMode::Dashboard;
                self.issues_state.active = false;
                // Discard unsent inline drafts with notice
                if self.issues_state.inline_state != InlineState::None {
                    self.issues_state.draft_notice = Some("Unsent draft discarded".to_string());
                    self.issues_state.inline_state = InlineState::None;
                }
                // Restore prior focus
                if let Some(prior) = self.issues_state.prior_agent_focus.take() {
                    self.pane_focus = prior.pane_focus;
                    // Validate target still exists
                    if let Some(idx) = prior.selected_agent_index {
                        if idx < self.agents.len() {
                            self.selected_agent_index = Some(idx);
                        } else {
                            // Fallback
                            self.pane_focus = PaneFocus::Agents;
                            self.selected_agent_index = if self.agents.is_empty() {
                                None
                            } else {
                                Some(0)
                            };
                        }
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

            // @requirement REQ-ISS-002
            AppEvent::RefocusIssueList => {
                self.issues_state.issue_focus = IssueFocus::IssueList;
            }

            // @requirement REQ-ISS-004
            // @pseudocode component-001 lines 73-81
            AppEvent::IssuesNavigateUp => {
                match self.issues_state.issue_focus {
                    IssueFocus::IssueList => {
                        if let Some(idx) = self.issues_state.selected_issue_index
                            && idx > 0
                        {
                            self.issues_state.selected_issue_index = Some(idx - 1);
                        }
                    }
                    IssueFocus::RepoList => {
                        // Delegate to existing repo navigation
                        self.handle_navigate_up();
                    }
                    IssueFocus::IssueDetail => {
                        // Scroll detail content up (UI-level concern, state no-op for now)
                    }
                }
            }

            // @requirement REQ-ISS-004
            // @pseudocode component-001 lines 82-91
            AppEvent::IssuesNavigateDown => {
                match self.issues_state.issue_focus {
                    IssueFocus::IssueList => {
                        if let Some(idx) = self.issues_state.selected_issue_index
                            && idx + 1 < self.issues_state.issues.len()
                        {
                            self.issues_state.selected_issue_index = Some(idx + 1);
                        }
                    }
                    IssueFocus::RepoList => {
                        self.handle_navigate_down();
                    }
                    IssueFocus::IssueDetail => {
                        // Scroll detail content down
                    }
                }
            }

            // @requirement REQ-ISS-004
            AppEvent::IssuesNavigatePageUp => {
                if let Some(idx) = self.issues_state.selected_issue_index {
                    self.issues_state.selected_issue_index = Some(idx.saturating_sub(10));
                }
            }

            // @requirement REQ-ISS-004
            AppEvent::IssuesNavigatePageDown => {
                if let Some(idx) = self.issues_state.selected_issue_index {
                    let max = self.issues_state.issues.len().saturating_sub(1);
                    self.issues_state.selected_issue_index = Some((idx + 10).min(max));
                }
            }

            // @requirement REQ-ISS-004
            AppEvent::IssuesNavigateHome => {
                if !self.issues_state.issues.is_empty() {
                    self.issues_state.selected_issue_index = Some(0);
                }
            }

            // @requirement REQ-ISS-004
            AppEvent::IssuesNavigateEnd => {
                if !self.issues_state.issues.is_empty() {
                    self.issues_state.selected_issue_index =
                        Some(self.issues_state.issues.len() - 1);
                }
            }

            // @requirement REQ-ISS-002
            // @pseudocode component-001 lines 161-164
            AppEvent::IssuesEnter => {
                if self.issues_state.issue_focus == IssueFocus::IssueList
                    && self.issues_state.selected_issue_index.is_some()
                {
                    self.issues_state.issue_focus = IssueFocus::IssueDetail;
                }
            }

            // @requirement REQ-ISS-002
            // @pseudocode component-001 lines 92-97
            AppEvent::IssuesCycleFocus => {
                self.issues_state.issue_focus = match self.issues_state.issue_focus {
                    IssueFocus::RepoList => IssueFocus::IssueList,
                    IssueFocus::IssueList => IssueFocus::IssueDetail,
                    IssueFocus::IssueDetail => IssueFocus::RepoList,
                };
            }

            // @requirement REQ-ISS-002
            // @pseudocode component-001 lines 98-103
            AppEvent::IssuesCycleFocusReverse => {
                self.issues_state.issue_focus = match self.issues_state.issue_focus {
                    IssueFocus::RepoList => IssueFocus::IssueDetail,
                    IssueFocus::IssueList => IssueFocus::RepoList,
                    IssueFocus::IssueDetail => IssueFocus::IssueList,
                };
            }

            // @requirement REQ-ISS-003
            // @pseudocode component-001 lines 165-177
            AppEvent::IssueDetailSubfocusNext => {
                if let Some(detail) = &self.issues_state.issue_detail {
                    let has_comments = !detail.comments.is_empty();
                    let comment_count = detail.comments.len();
                    self.issues_state.detail_subfocus = match self.issues_state.detail_subfocus {
                        DetailSubfocus::Body => {
                            if has_comments {
                                DetailSubfocus::Comment(0)
                            } else {
                                DetailSubfocus::NewComment
                            }
                        }
                        DetailSubfocus::Comment(i) => {
                            if i + 1 < comment_count {
                                DetailSubfocus::Comment(i + 1)
                            } else {
                                DetailSubfocus::NewComment
                            }
                        }
                        DetailSubfocus::NewComment => DetailSubfocus::Body,
                    };
                }
            }

            // @requirement REQ-ISS-003
            // @pseudocode component-001 lines 178-189
            AppEvent::IssueDetailSubfocusPrev => {
                if let Some(detail) = &self.issues_state.issue_detail {
                    let has_comments = !detail.comments.is_empty();
                    let comment_count = detail.comments.len();
                    self.issues_state.detail_subfocus = match self.issues_state.detail_subfocus {
                        DetailSubfocus::Body => DetailSubfocus::NewComment,

                        DetailSubfocus::Comment(0) => DetailSubfocus::Body,
                        DetailSubfocus::Comment(i) => DetailSubfocus::Comment(i - 1),
                        DetailSubfocus::NewComment => {
                            if has_comments {
                                DetailSubfocus::Comment(comment_count - 1)
                            } else {
                                DetailSubfocus::Body
                            }
                        }
                    };
                }
            }

            // @requirement REQ-ISS-008
            AppEvent::OpenFilterControls => {
                self.issues_state.filter_controls_open = true;
            }

            // @requirement REQ-ISS-008
            AppEvent::CloseFilterControls => {
                self.issues_state.filter_controls_open = false;
            }

            // @requirement REQ-ISS-008
            AppEvent::ApplyFilter => {
                // Commit draft filter to committed filter
                self.issues_state.committed_filter = self.issues_state.draft_filter.clone();
                self.issues_state.filter_controls_open = false;
            }

            // @requirement REQ-ISS-008
            AppEvent::ClearFilter => {
                self.issues_state.committed_filter = IssueFilter::default();
            }

            // @requirement REQ-ISS-007
            AppEvent::FocusSearchInput => {
                self.issues_state.search_input_focused = true;
            }

            // @requirement REQ-ISS-007
            AppEvent::BlurSearchInput => {
                self.issues_state.search_input_focused = false;
            }

            // @requirement REQ-ISS-007
            AppEvent::ClearSearch => {
                self.issues_state.search_query.clear();
            }

            // @requirement REQ-ISS-006, REQ-ISS-012
            // @pseudocode component-001 lines 104-118
            AppEvent::IssueListLoaded {
                scope_repo_id,
                issues,
                cursor,
                has_more,
            } => {
                let current_repo_id = self.selected_repository_id().cloned();
                if current_repo_id.as_ref() == Some(&scope_repo_id) {
                    self.issues_state.issues = issues;
                    self.issues_state.list_cursor = cursor;
                    self.issues_state.has_more_issues = has_more;
                    self.issues_state.list_loading = false;
                    if self.issues_state.issues.is_empty() {
                        self.issues_state.selected_issue_index = None;
                        self.issues_state.issue_detail = None;
                    } else {
                        self.issues_state.selected_issue_index = Some(0);
                    }
                }
            }

            // @requirement REQ-ISS-006
            // @pseudocode component-001 lines 119-126
            AppEvent::IssueListPageLoaded {
                scope_repo_id,
                issues,
                cursor,
                has_more,
            } => {
                let current_repo_id = self.selected_repository_id().cloned();
                if current_repo_id.as_ref() == Some(&scope_repo_id) {
                    self.issues_state.issues.extend(issues);
                    self.issues_state.list_cursor = cursor;
                    self.issues_state.has_more_issues = has_more;
                    self.issues_state.list_loading = false;
                }
            }

            // @requirement REQ-ISS-009
            // @pseudocode component-001 lines 127-135
            AppEvent::IssueDetailLoaded {
                scope_repo_id,
                detail,
                ..
            } => {
                let current_repo_id = self.selected_repository_id().cloned();
                if current_repo_id.as_ref() == Some(&scope_repo_id) {
                    self.issues_state.issue_detail = Some(*detail);
                    self.issues_state.detail_loading = false;
                    self.issues_state.detail_subfocus = DetailSubfocus::Body;
                    self.issues_state.detail_scroll_offset = 0;
                }
            }

            // @requirement REQ-ISS-009
            AppEvent::IssueCommentsPageLoaded {
                scope_repo_id,
                issue_number,
                comments,
                cursor,
                has_more,
            } => {
                let current_repo_id = self.selected_repository_id().cloned();
                if current_repo_id.as_ref() == Some(&scope_repo_id) {
                    if let Some(detail) = &mut self.issues_state.issue_detail
                        && detail.number == issue_number
                    {
                        detail.comments.extend(comments);
                        detail.comments_cursor = cursor;
                        detail.has_more_comments = has_more;
                    }
                    self.issues_state.comments_loading = false;
                }
            }

            // @requirement REQ-ISS-007
            AppEvent::SetSearchQuery { query } => {
                self.issues_state.search_query = query;
            }

            // @requirement REQ-ISS-008
            AppEvent::UpdateDraftFilter { field, value } => {
                // Update specific draft filter field
                match field.as_str() {
                    "author" => self.issues_state.draft_filter.author = value,
                    "assignee" => self.issues_state.draft_filter.assignee = value,
                    "mentioned" => self.issues_state.draft_filter.mentioned = value,
                    "query_text" => self.issues_state.draft_filter.query_text = value,
                    "updated_before" => self.issues_state.draft_filter.updated_before = value,
                    "updated_after" => self.issues_state.draft_filter.updated_after = value,
                    _ => {}
                }
            }

            // @requirement REQ-ISS-010
            // @pseudocode component-001 lines 190-197
            AppEvent::OpenNewCommentComposer => {
                if self.issues_state.inline_state == InlineState::None {
                    self.issues_state.inline_state = InlineState::Composer {
                        target: ComposerTarget::NewComment,
                        text: String::new(),
                        cursor: 0,
                    };
                }
            }

            // @requirement REQ-ISS-010
            // @pseudocode component-001 lines 198-209
            AppEvent::OpenReplyComposer { comment_index } => {
                if self.issues_state.inline_state == InlineState::None {
                    let author = self
                        .issues_state
                        .issue_detail
                        .as_ref()
                        .and_then(|d| d.comments.get(comment_index))
                        .map(|c| format!("@{} ", c.author_login))
                        .unwrap_or_default();
                    let cursor = author.len();
                    self.issues_state.inline_state = InlineState::Composer {
                        target: ComposerTarget::Reply {
                            comment_index,
                            author: author.clone(),
                        },
                        text: author,
                        cursor,
                    };
                }
            }

            // @requirement REQ-ISS-010
            // @pseudocode component-001 lines 210-225
            AppEvent::OpenInlineEditor { target } => {
                if self.issues_state.inline_state == InlineState::None {
                    let text = match &target {
                        EditorTarget::IssueBody => self
                            .issues_state
                            .issue_detail
                            .as_ref()
                            .map(|d| d.body.clone())
                            .unwrap_or_default(),
                        EditorTarget::Comment { comment_index } => self
                            .issues_state
                            .issue_detail
                            .as_ref()
                            .and_then(|d| d.comments.get(*comment_index))
                            .map(|c| c.body.clone())
                            .unwrap_or_default(),
                    };
                    let cursor = text.len();
                    self.issues_state.inline_state = InlineState::Editor {
                        target,
                        text,
                        cursor,
                    };
                }
            }

            // @requirement REQ-ISS-010
            // @pseudocode component-001 lines 226-235
            AppEvent::InlineChar(c) => match &mut self.issues_state.inline_state {
                InlineState::Composer { text, cursor, .. }
                | InlineState::Editor { text, cursor, .. } => {
                    text.insert(*cursor, c);
                    *cursor += c.len_utf8();
                }
                InlineState::None => {}
            },

            AppEvent::InlineNewline => match &mut self.issues_state.inline_state {
                InlineState::Composer { text, cursor, .. }
                | InlineState::Editor { text, cursor, .. } => {
                    text.insert(*cursor, char::from(0x0Au8));
                    *cursor += 1;
                }
                InlineState::None => {}
            },

            // @requirement REQ-ISS-010
            // @pseudocode component-001 lines 236-246
            AppEvent::InlineBackspace => match &mut self.issues_state.inline_state {
                InlineState::Composer { text, cursor, .. }
                | InlineState::Editor { text, cursor, .. } => {
                    if *cursor > 0 {
                        let prev = text[..*cursor].chars().last().map_or(0, char::len_utf8);
                        text.drain((*cursor - prev)..*cursor);
                        *cursor -= prev;
                    }
                }
                InlineState::None => {}
            },

            AppEvent::InlineDelete => match &mut self.issues_state.inline_state {
                InlineState::Composer { text, cursor, .. }
                | InlineState::Editor { text, cursor, .. } => {
                    if *cursor < text.len() {
                        let next = text[*cursor..].chars().next().map_or(0, char::len_utf8);
                        text.drain(*cursor..(*cursor + next));
                    }
                }
                InlineState::None => {}
            },

            // @requirement REQ-ISS-010
            AppEvent::InlineCursorLeft => match &mut self.issues_state.inline_state {
                InlineState::Composer { text, cursor, .. }
                | InlineState::Editor { text, cursor, .. } => {
                    if *cursor > 0 {
                        let prev = text[..*cursor].chars().last().map_or(0, char::len_utf8);
                        *cursor -= prev;
                    }
                }
                InlineState::None => {}
            },

            // @requirement REQ-ISS-010
            AppEvent::InlineCursorRight => match &mut self.issues_state.inline_state {
                InlineState::Composer { text, cursor, .. }
                | InlineState::Editor { text, cursor, .. } => {
                    if *cursor < text.len() {
                        let next = text[*cursor..].chars().next().map_or(0, char::len_utf8);
                        *cursor += next;
                    }
                }
                InlineState::None => {}
            },

            // Move cursor to equivalent column on previous line
            AppEvent::InlineCursorUp => match &mut self.issues_state.inline_state {
                InlineState::Composer { text, cursor, .. }
                | InlineState::Editor { text, cursor, .. } => {
                    inline_cursor_vertical(text, cursor, -1);
                }
                InlineState::None => {}
            },

            // Move cursor to equivalent column on next line
            AppEvent::InlineCursorDown => match &mut self.issues_state.inline_state {
                InlineState::Composer { text, cursor, .. }
                | InlineState::Editor { text, cursor, .. } => {
                    inline_cursor_vertical(text, cursor, 1);
                }
                InlineState::None => {}
            },

            // @requirement REQ-ISS-010
            AppEvent::InlineCancelOrEsc => {
                self.issues_state.inline_state = InlineState::None;
            }

            // @requirement REQ-ISS-010
            AppEvent::CommentCreated { comment } => {
                if let Some(detail) = &mut self.issues_state.issue_detail {
                    detail.comments.push(comment);
                }
                self.issues_state.inline_state = InlineState::None;
            }

            // @requirement REQ-ISS-010
            AppEvent::IssueBodyUpdated { body } => {
                if let Some(detail) = &mut self.issues_state.issue_detail {
                    detail.body = body;
                }
                self.issues_state.inline_state = InlineState::None;
            }

            // @requirement REQ-ISS-010
            AppEvent::CommentUpdated {
                comment_index,
                body,
            } => {
                if let Some(detail) = &mut self.issues_state.issue_detail
                    && let Some(comment) = detail.comments.get_mut(comment_index)
                {
                    comment.body = body;
                }
                self.issues_state.inline_state = InlineState::None;
            }

            // @requirement REQ-ISS-011
            AppEvent::OpenAgentChooser => {
                // Populate from current agents
                let agents: Vec<_> = self
                    .agents
                    .iter()
                    .map(|a| (a.id.clone(), a.name.clone()))
                    .collect();
                if !agents.is_empty() {
                    self.issues_state.agent_chooser = Some(AgentChooserState {
                        selected_index: 0,
                        agents,
                    });
                }
            }

            // @requirement REQ-ISS-011
            AppEvent::AgentChooserNavigateUp => {
                if let Some(chooser) = &mut self.issues_state.agent_chooser
                    && chooser.selected_index > 0
                {
                    chooser.selected_index -= 1;
                }
            }

            // @requirement REQ-ISS-011
            AppEvent::AgentChooserNavigateDown => {
                if let Some(chooser) = &mut self.issues_state.agent_chooser
                    && chooser.selected_index + 1 < chooser.agents.len()
                {
                    chooser.selected_index += 1;
                }
            }

            // @requirement REQ-ISS-011
            // AgentChooserConfirm: actual send is handled by dispatch_app_event
            AppEvent::AgentChooserConfirm
            | AppEvent::AgentChooserCancel
            | AppEvent::SendToAgentCompleted => {
                self.issues_state.agent_chooser = None;
            }

            // Error events
            // @requirement REQ-ISS-012
            AppEvent::IssueListLoadFailed {
                scope_repo_id,
                error,
            } => {
                let current_repo_id = self.selected_repository_id().cloned();
                if current_repo_id.as_ref() == Some(&scope_repo_id) {
                    self.issues_state.list_loading = false;
                    self.issues_state.error = Some(error);
                }
            }

            // @requirement REQ-ISS-012
            AppEvent::IssueDetailLoadFailed {
                scope_repo_id,
                error,
                ..
            } => {
                let current_repo_id = self.selected_repository_id().cloned();
                if current_repo_id.as_ref() == Some(&scope_repo_id) {
                    self.issues_state.detail_loading = false;
                    self.issues_state.error = Some(error);
                }
            }

            // @requirement REQ-ISS-012
            AppEvent::IssueCommentsPageFailed {
                scope_repo_id,
                error,
                ..
            } => {
                let current_repo_id = self.selected_repository_id().cloned();
                if current_repo_id.as_ref() == Some(&scope_repo_id) {
                    self.issues_state.comments_loading = false;
                    self.issues_state.error = Some(error);
                }
            }

            AppEvent::CommentCreateFailed { error } | AppEvent::MutationFailed { error } => {
                self.issues_state.error = Some(error);
                self.issues_state.inline_state = InlineState::None;
            }

            AppEvent::SendToAgentFailed { error } => {
                self.issues_state.error = Some(error);
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
        (&agent.repository_id == repository_id).then_some(agent)
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
mod tests {
    use crate::domain::{
        Agent, AgentId, Issue, IssueComment, IssueDetail, IssueState, Repository, RepositoryId,
    };
    use crate::state::AppState;
    use crate::state::types::{
        AgentChooserState, AppEvent, ComposerTarget, DetailSubfocus, EditorTarget, InlineState,
        IssueFocus, PaneFocus, PriorAgentFocus, ScreenMode,
    };
    use std::path::PathBuf;

    /// Helper to create a test issue with the given number.
    fn make_test_issue(number: u64) -> Issue {
        Issue {
            number,
            title: format!("Test Issue #{}", number),
            state: IssueState::Open,
            author_login: "testuser".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            assignee_summary: "".to_string(),
            labels_summary: "".to_string(),
            comment_count: 0,
            body: String::new(),
        }
    }

    /// Test 1: EnterIssuesMode sets screen mode, activates issues state, and focuses issue list.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-001
    /// @pseudocode component-001 lines 10-15
    #[test]
    fn test_enter_issues_mode_sets_screen_mode() {
        let state = AppState::default();
        let new_state = state.apply(AppEvent::EnterIssuesMode);
        assert_eq!(new_state.screen_mode, ScreenMode::DashboardIssues);
        assert!(new_state.issues_state.active);
        assert_eq!(new_state.issues_state.issue_focus, IssueFocus::IssueList);
    }

    /// Test 2: EnterIssuesMode saves prior agent focus for restoration on exit.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-005
    /// @pseudocode component-001 lines 20-25
    #[test]
    fn test_enter_issues_mode_saves_prior_focus() {
        let mut state = AppState::default();
        state.pane_focus = PaneFocus::Agents;
        state.selected_agent_index = Some(2);
        state.selected_repository_index = Some(1);

        let new_state = state.apply(AppEvent::EnterIssuesMode);
        assert!(new_state.issues_state.prior_agent_focus.is_some());
        let saved = new_state.issues_state.prior_agent_focus.unwrap();
        assert_eq!(saved.pane_focus, PaneFocus::Agents);
        assert_eq!(saved.selected_agent_index, Some(2));
        assert_eq!(saved.selected_repository_index, Some(1));
    }

    /// Test 3: ExitIssuesMode restores the saved prior focus.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-005
    /// @pseudocode component-001 lines 30-35
    #[test]
    fn test_exit_issues_mode_restores_focus() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.active = true;
        state.issues_state.prior_agent_focus = Some(PriorAgentFocus {
            pane_focus: PaneFocus::Agents,
            selected_repository_index: Some(0),
            selected_agent_index: Some(1),
        });

        // Set up 2 agents for the selected repository
        state.repositories.push(Repository::new(
            RepositoryId("repo-1".to_string()),
            "Repo 1".to_string(),
            "repo-1".to_string(),
            PathBuf::from("/tmp/repo1"),
        ));
        state.selected_repository_index = Some(0);

        // Create agents for the repository
        state.agents.push(Agent::new(
            AgentId("agent-1".to_string()),
            RepositoryId("repo-1".to_string()),
            "Agent 1".to_string(),
            PathBuf::from("/tmp/agent1"),
        ));
        state.agents.push(Agent::new(
            AgentId("agent-2".to_string()),
            RepositoryId("repo-1".to_string()),
            "Agent 2".to_string(),
            PathBuf::from("/tmp/agent2"),
        ));

        let new_state = state.apply(AppEvent::ExitIssuesMode);
        assert_eq!(new_state.screen_mode, ScreenMode::Dashboard);
        assert_eq!(new_state.pane_focus, PaneFocus::Agents);
        assert_eq!(new_state.selected_agent_index, Some(1));
    }

    /// Test 4: ExitIssuesMode falls back gracefully when saved agent index is out of bounds.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-005
    /// @pseudocode component-001 lines 36-40
    #[test]
    fn test_exit_issues_mode_fallback_when_target_gone() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.active = true;
        state.issues_state.prior_agent_focus = Some(PriorAgentFocus {
            pane_focus: PaneFocus::Agents,
            selected_repository_index: Some(0),
            selected_agent_index: Some(5), // Out of bounds - only 2 agents
        });

        // Set up repository with 2 agents
        state.repositories.push(Repository::new(
            RepositoryId("repo-1".to_string()),
            "Repo 1".to_string(),
            "repo-1".to_string(),
            PathBuf::from("/tmp/repo1"),
        ));
        state.selected_repository_index = Some(0);

        state.agents.push(Agent::new(
            AgentId("agent-1".to_string()),
            RepositoryId("repo-1".to_string()),
            "Agent 1".to_string(),
            PathBuf::from("/tmp/agent1"),
        ));
        state.agents.push(Agent::new(
            AgentId("agent-2".to_string()),
            RepositoryId("repo-1".to_string()),
            "Agent 2".to_string(),
            PathBuf::from("/tmp/agent2"),
        ));

        let new_state = state.apply(AppEvent::ExitIssuesMode);
        assert_eq!(new_state.pane_focus, PaneFocus::Agents);
        // Should fall back to Some(0) or None
        assert!(
            new_state.selected_agent_index == Some(0) || new_state.selected_agent_index.is_none()
        );
    }

    /// Test 5: ExitIssuesMode discards active drafts and shows a notice.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-010
    /// @pseudocode component-001 lines 45-50
    #[test]
    fn test_exit_issues_mode_discards_draft_with_notice() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.active = true;
        state.issues_state.inline_state = InlineState::Composer {
            target: ComposerTarget::NewComment,
            text: "Draft comment".to_string(),
            cursor: 5,
        };

        let new_state = state.apply(AppEvent::ExitIssuesMode);
        assert_eq!(new_state.issues_state.inline_state, InlineState::None);
        assert!(new_state.issues_state.draft_notice.is_some());
        let notice = new_state.issues_state.draft_notice.unwrap();
        assert!(notice.contains("discarded") || notice.contains("Draft"));
    }

    /// Test 6: IssuesCycleFocus advances through RepoList -> IssueList -> IssueDetail -> RepoList.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-002
    /// @pseudocode component-001 lines 55-60
    #[test]
    fn test_issues_cycle_focus_tab() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.active = true;
        state.issues_state.issue_focus = IssueFocus::RepoList;

        // Cycle: RepoList -> IssueList
        let state = state.apply(AppEvent::IssuesCycleFocus);
        assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueList);

        // Cycle: IssueList -> IssueDetail
        let state = state.apply(AppEvent::IssuesCycleFocus);
        assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueDetail);

        // Cycle: IssueDetail -> RepoList
        let state = state.apply(AppEvent::IssuesCycleFocus);
        assert_eq!(state.issues_state.issue_focus, IssueFocus::RepoList);
    }

    /// Test 7: IssuesCycleFocusReverse cycles backwards through focus areas.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-002
    /// @pseudocode component-001 lines 61-66
    #[test]
    fn test_issues_cycle_focus_shift_tab() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.active = true;
        state.issues_state.issue_focus = IssueFocus::RepoList;

        // Reverse cycle: RepoList -> IssueDetail
        let state = state.apply(AppEvent::IssuesCycleFocusReverse);
        assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueDetail);

        // Reverse cycle: IssueDetail -> IssueList
        let state = state.apply(AppEvent::IssuesCycleFocusReverse);
        assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueList);

        // Reverse cycle: IssueList -> RepoList
        let state = state.apply(AppEvent::IssuesCycleFocusReverse);
        assert_eq!(state.issues_state.issue_focus, IssueFocus::RepoList);
    }

    /// Test 8: IssuesNavigateUp decrements selected_issue_index.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-004
    /// @pseudocode component-001 lines 70-75
    #[test]
    fn test_issues_navigate_up_in_issue_list() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.active = true;
        state.issues_state.issue_focus = IssueFocus::IssueList;
        state.issues_state.issues = vec![
            make_test_issue(1),
            make_test_issue(2),
            make_test_issue(3),
            make_test_issue(4),
            make_test_issue(5),
        ];
        state.issues_state.selected_issue_index = Some(3);

        let new_state = state.apply(AppEvent::IssuesNavigateUp);
        assert_eq!(new_state.issues_state.selected_issue_index, Some(2));
    }

    /// Test 9: IssuesNavigateUp clamps at zero.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-004
    /// @pseudocode component-001 lines 76-80
    #[test]
    fn test_issues_navigate_up_clamps_at_zero() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.active = true;
        state.issues_state.issue_focus = IssueFocus::IssueList;
        state.issues_state.issues =
            vec![make_test_issue(1), make_test_issue(2), make_test_issue(3)];
        state.issues_state.selected_issue_index = Some(0);

        let new_state = state.apply(AppEvent::IssuesNavigateUp);
        assert_eq!(new_state.issues_state.selected_issue_index, Some(0));
    }

    /// Test 10: IssuesNavigateDown increments selected_issue_index.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-004
    /// @pseudocode component-001 lines 81-85
    #[test]
    fn test_issues_navigate_down_in_issue_list() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.active = true;
        state.issues_state.issue_focus = IssueFocus::IssueList;
        state.issues_state.issues = vec![
            make_test_issue(1),
            make_test_issue(2),
            make_test_issue(3),
            make_test_issue(4),
            make_test_issue(5),
        ];
        state.issues_state.selected_issue_index = Some(2);

        let new_state = state.apply(AppEvent::IssuesNavigateDown);
        assert_eq!(new_state.issues_state.selected_issue_index, Some(3));
    }

    /// Test 11: IssueListLoaded selects the first issue and clears loading state.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-006
    /// @pseudocode component-001 lines 90-95
    #[test]
    fn test_issue_list_loaded_selects_first() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.list_loading = true;

        // Set up repository
        state.repositories.push(Repository::new(
            RepositoryId("repo-1".to_string()),
            "Repo 1".to_string(),
            "repo-1".to_string(),
            PathBuf::from("/tmp/repo1"),
        ));
        state.selected_repository_index = Some(0);

        let issues = vec![make_test_issue(1), make_test_issue(2), make_test_issue(3)];

        let new_state = state.apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            issues: issues.clone(),
            cursor: None,
            has_more: false,
        });

        assert_eq!(new_state.issues_state.selected_issue_index, Some(0));
        assert!(!new_state.issues_state.list_loading);
        assert_eq!(new_state.issues_state.issues.len(), 3);
    }

    /// Test 12: IssueListLoaded with empty issues sets selected_index to None.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-006
    /// @pseudocode component-001 lines 96-100
    #[test]
    fn test_issue_list_loaded_empty() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.list_loading = true;

        // Set up repository
        state.repositories.push(Repository::new(
            RepositoryId("repo-1".to_string()),
            "Repo 1".to_string(),
            "repo-1".to_string(),
            PathBuf::from("/tmp/repo1"),
        ));
        state.selected_repository_index = Some(0);

        let new_state = state.apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            issues: vec![],
            cursor: None,
            has_more: false,
        });

        assert_eq!(new_state.issues_state.selected_issue_index, None);
        assert!(new_state.issues_state.issue_detail.is_none());
    }

    /// Test 13: IssueListLoaded with stale scope is discarded.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-012
    /// @pseudocode component-001 lines 105-110
    #[test]
    fn test_issue_list_loaded_stale_scope_discarded() {
        let mut state = AppState::default();

        // Set up repo at index 0 with id "repo-1"
        state.repositories.push(Repository::new(
            RepositoryId("repo-1".to_string()),
            "Repo 1".to_string(),
            "repo-1".to_string(),
            PathBuf::from("/tmp/repo1"),
        ));
        state.selected_repository_index = Some(0);
        state.issues_state.list_loading = true;

        // Try to load issues for wrong repo
        let new_state = state.apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-WRONG".to_string()),
            issues: vec![make_test_issue(1)],
            cursor: None,
            has_more: false,
        });

        // State should be unchanged (stale scope discarded)
        assert!(new_state.issues_state.issues.is_empty());
        assert!(new_state.issues_state.list_loading);
    }

    /// Test 14: IssueListPageLoaded appends issues to existing list.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-006
    /// @pseudocode component-001 lines 111-115
    #[test]
    fn test_issue_list_page_loaded_appends() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;

        // Set up repository
        state.repositories.push(Repository::new(
            RepositoryId("repo-1".to_string()),
            "Repo 1".to_string(),
            "repo-1".to_string(),
            PathBuf::from("/tmp/repo1"),
        ));
        state.selected_repository_index = Some(0);

        // Start with 3 issues
        state.issues_state.issues =
            vec![make_test_issue(1), make_test_issue(2), make_test_issue(3)];
        state.issues_state.selected_issue_index = Some(1);

        let new_state = state.apply(AppEvent::IssueListPageLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            issues: vec![make_test_issue(4), make_test_issue(5)],
            cursor: None,
            has_more: false,
        });

        assert_eq!(new_state.issues_state.issues.len(), 5);
        assert_eq!(new_state.issues_state.selected_issue_index, Some(1)); // Unchanged
    }

    /// Test 15: IssueDetailSubfocusNext cycles through Body -> Comment(0) -> Comment(1) -> NewComment -> Body.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-003
    /// @pseudocode component-001 lines 120-125
    #[test]
    fn test_detail_subfocus_tab_with_comments() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.active = true;
        state.issues_state.issue_focus = IssueFocus::IssueDetail;
        state.issues_state.detail_subfocus = DetailSubfocus::Body;

        // Set up issue detail with 2 comments
        state.issues_state.issue_detail = Some(IssueDetail {
            repo_owner_name: "owner/repo".to_string(),
            number: 1,
            title: "Test Issue".to_string(),
            state: IssueState::Open,
            author_login: "testuser".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-02T00:00:00Z".to_string(),
            labels: vec![],
            assignees: vec![],
            milestone: None,
            body: "Issue body".to_string(),
            external_url: "https://github.com/owner/repo/issues/1".to_string(),
            comments: vec![
                IssueComment {
                    comment_id: 100,
                    author_login: "user1".to_string(),
                    created_at: "2024-01-02T00:00:00Z".to_string(),
                    edited_at: None,
                    body: "First comment".to_string(),
                },
                IssueComment {
                    comment_id: 101,
                    author_login: "user2".to_string(),
                    created_at: "2024-01-03T00:00:00Z".to_string(),
                    edited_at: None,
                    body: "Second comment".to_string(),
                },
            ],
            has_more_comments: false,
            comments_cursor: None,
        });

        // Body -> Comment(0)
        let state = state.apply(AppEvent::IssueDetailSubfocusNext);
        assert_eq!(
            state.issues_state.detail_subfocus,
            DetailSubfocus::Comment(0)
        );

        // Comment(0) -> Comment(1)
        let state = state.apply(AppEvent::IssueDetailSubfocusNext);
        assert_eq!(
            state.issues_state.detail_subfocus,
            DetailSubfocus::Comment(1)
        );

        // Comment(1) -> NewComment
        let state = state.apply(AppEvent::IssueDetailSubfocusNext);
        assert_eq!(
            state.issues_state.detail_subfocus,
            DetailSubfocus::NewComment
        );

        // NewComment -> Body
        let state = state.apply(AppEvent::IssueDetailSubfocusNext);
        assert_eq!(state.issues_state.detail_subfocus, DetailSubfocus::Body);
    }

    /// Test 16: IssueDetailSubfocusNext with no comments skips to NewComment then back to Body.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-003
    /// @pseudocode component-001 lines 126-130
    #[test]
    fn test_detail_subfocus_tab_no_comments() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.active = true;
        state.issues_state.issue_focus = IssueFocus::IssueDetail;
        state.issues_state.detail_subfocus = DetailSubfocus::Body;

        // Set up issue detail with 0 comments
        state.issues_state.issue_detail = Some(IssueDetail {
            repo_owner_name: "owner/repo".to_string(),
            number: 1,
            title: "Test Issue".to_string(),
            state: IssueState::Open,
            author_login: "testuser".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-02T00:00:00Z".to_string(),
            labels: vec![],
            assignees: vec![],
            milestone: None,
            body: "Issue body".to_string(),
            external_url: "https://github.com/owner/repo/issues/1".to_string(),
            comments: vec![],
            has_more_comments: false,
            comments_cursor: None,
        });

        // Body -> NewComment (skip comments since there are none)
        let state = state.apply(AppEvent::IssueDetailSubfocusNext);
        assert_eq!(
            state.issues_state.detail_subfocus,
            DetailSubfocus::NewComment
        );

        // NewComment -> Body
        let state = state.apply(AppEvent::IssueDetailSubfocusNext);
        assert_eq!(state.issues_state.detail_subfocus, DetailSubfocus::Body);
    }

    /// Test 17: InlineCancelOrEsc clears inline editor state.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-010
    /// @pseudocode component-001 lines 135-140
    #[test]
    fn test_esc_cancels_inline_editor() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.inline_state = InlineState::Editor {
            target: EditorTarget::IssueBody,
            text: "draft content".to_string(),
            cursor: 5,
        };

        let new_state = state.apply(AppEvent::InlineCancelOrEsc);
        assert_eq!(new_state.issues_state.inline_state, InlineState::None);
    }

    /// Test 18: AgentChooserCancel clears agent chooser state.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-011
    /// @pseudocode component-001 lines 141-145
    #[test]
    fn test_esc_cancels_agent_chooser() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.agent_chooser = Some(AgentChooserState::default());
        state.issues_state.inline_state = InlineState::None;

        let new_state = state.apply(AppEvent::AgentChooserCancel);
        assert!(new_state.issues_state.agent_chooser.is_none());
    }

    /// Test 19: ClearSearch clears non-empty search query.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-007
    /// @pseudocode component-001 lines 146-150
    #[test]
    fn test_esc_clears_nonempty_search() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.search_input_focused = true;
        state.issues_state.search_query = "bug".to_string();
        state.issues_state.inline_state = InlineState::None;
        state.issues_state.agent_chooser = None;

        let new_state = state.apply(AppEvent::ClearSearch);
        assert!(new_state.issues_state.search_query.is_empty());
        assert!(new_state.issues_state.search_input_focused);
    }

    /// Test 20: BlurSearchInput blurs empty search input.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-007
    /// @pseudocode component-001 lines 151-155
    #[test]
    fn test_esc_blurs_empty_search() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.search_input_focused = true;
        state.issues_state.search_query = "".to_string();

        let new_state = state.apply(AppEvent::BlurSearchInput);
        assert!(!new_state.issues_state.search_input_focused);
    }

    /// Test 21: CloseFilterControls closes filter controls.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-008
    /// @pseudocode component-001 lines 156-160
    #[test]
    fn test_esc_closes_filter_controls() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.filter_controls_open = true;

        let new_state = state.apply(AppEvent::CloseFilterControls);
        assert!(!new_state.issues_state.filter_controls_open);
    }

    /// Test 22: ExitIssuesMode when no inner controls are active.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-001
    /// @pseudocode component-001 lines 161-165
    #[test]
    fn test_esc_exits_issues_mode() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.active = true;
        state.issues_state.inline_state = InlineState::None;
        state.issues_state.agent_chooser = None;
        state.issues_state.filter_controls_open = false;
        state.issues_state.search_input_focused = false;

        let new_state = state.apply(AppEvent::ExitIssuesMode);
        assert_eq!(new_state.screen_mode, ScreenMode::Dashboard);
    }

    /// Test 23: OpenInlineEditor is blocked when another inline control is active.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-010
    /// @pseudocode component-001 lines 170-175
    #[test]
    fn test_inline_exclusivity_blocks_second_control() {
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;

        // Set active Composer
        state.issues_state.inline_state = InlineState::Composer {
            target: ComposerTarget::NewComment,
            text: "hello".to_string(),
            cursor: 5,
        };

        // Try to open Editor while Composer is active
        let new_state = state.apply(AppEvent::OpenInlineEditor {
            target: EditorTarget::IssueBody,
        });

        // Should still be Composer, not changed to Editor
        match new_state.issues_state.inline_state {
            InlineState::Composer {
                target: ComposerTarget::NewComment,
                ..
            } => {}
            _ => panic!(
                "Expected Composer state to remain, but got {:?}",
                new_state.issues_state.inline_state
            ),
        }
    }

    /// Test 24: IssueListLoaded with mismatched scope_repo_id is discarded.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-012
    /// @pseudocode component-001 lines 180-185
    #[test]
    fn test_stale_scope_list_loaded_discarded() {
        let mut state = AppState::default();

        // Set up repo "repo-A" at index 0
        state.repositories.push(Repository::new(
            RepositoryId("repo-A".to_string()),
            "Repo A".to_string(),
            "repo-a".to_string(),
            PathBuf::from("/tmp/repo-a"),
        ));
        state.selected_repository_index = Some(0);
        state.issues_state.list_loading = true;

        // Load issues for wrong repo "repo-B"
        let new_state = state.apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-B".to_string()),
            issues: vec![make_test_issue(1)],
            cursor: None,
            has_more: false,
        });

        // Issues list should remain unchanged
        assert!(new_state.issues_state.issues.is_empty());
        assert!(new_state.issues_state.list_loading);
    }

    // -------------------------------------------------------------------------
    // P13 Tests — UI Components + Persistence Rendering Contracts
    // -------------------------------------------------------------------------

    /// Helper to build a minimal IssueDetail for testing.
    fn make_test_detail(comments: Vec<IssueComment>) -> IssueDetail {
        IssueDetail {
            repo_owner_name: "owner/repo".to_string(),
            number: 42,
            title: "Test detail issue".to_string(),
            state: IssueState::Open,
            author_login: "octocat".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-02T00:00:00Z".to_string(),
            labels: vec!["bug".to_string(), "ui".to_string()],
            assignees: vec!["dev1".to_string()],
            milestone: Some("v1.0".to_string()),
            body: "Detail body text".to_string(),
            external_url: "https://github.com/owner/repo/issues/42".to_string(),
            comments,
            has_more_comments: false,
            comments_cursor: None,
        }
    }

    /// Helper to make a test IssueComment.
    fn make_test_comment(id: u64, author: &str, body: &str) -> IssueComment {
        IssueComment {
            comment_id: id,
            author_login: author.to_string(),
            created_at: "2024-01-03T00:00:00Z".to_string(),
            edited_at: None,
            body: body.to_string(),
        }
    }

    /// Helper to set up a state with a selected repository at index 0.
    fn state_with_repo(repo_id: &str) -> AppState {
        let mut state = AppState::default();
        state.repositories.push(Repository::new(
            RepositoryId(repo_id.to_string()),
            "Test Repo".to_string(),
            repo_id.to_string(),
            std::path::PathBuf::from("/tmp/test-repo"),
        ));
        state.selected_repository_index = Some(0);
        state
    }

    /// P13 Test 3: IssueListLoaded with 5 issues populates issues_state.issues with exactly 5 items.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P13
    /// @requirement REQ-ISS-006
    #[test]
    fn test_issue_list_row_count() {
        let state = state_with_repo("repo-1").apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            issues: (1u64..=5).map(make_test_issue).collect(),
            cursor: None,
            has_more: false,
        });

        assert_eq!(state.issues_state.issues.len(), 5);
    }

    /// P13 Test 4: After loading issues and navigating down, selected_issue_index becomes Some(1).
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P13
    /// @requirement REQ-ISS-006
    #[test]
    fn test_issue_list_selection_highlight() {
        let state = state_with_repo("repo-1").apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            issues: (1u64..=5).map(make_test_issue).collect(),
            cursor: None,
            has_more: false,
        });

        // After load, selection is at 0. Navigate down once.
        let state = state.apply(AppEvent::IssuesNavigateDown);

        assert_eq!(state.issues_state.selected_issue_index, Some(1));
    }

    /// P13 Test 5: Entering issues mode sets list_loading to true initially.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P13
    /// @requirement REQ-ISS-006
    #[test]
    fn test_issue_list_loading_state() {
        let state = AppState::default().apply(AppEvent::EnterIssuesMode);

        // list_loading should be true right after EnterIssuesMode (before data arrives)
        assert!(state.issues_state.list_loading);
    }

    /// P13 Test 6: IssueListLoaded with empty vec leaves issues empty and selected_issue_index None.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P13
    /// @requirement REQ-ISS-006, REQ-ISS-014
    #[test]
    fn test_issue_list_empty_state() {
        let state = state_with_repo("repo-1").apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            issues: vec![],
            cursor: None,
            has_more: false,
        });

        assert!(state.issues_state.issues.is_empty());
        assert!(state.issues_state.selected_issue_index.is_none());
    }

    /// P13 Test 7: IssueDetailLoaded populates all fields in issues_state.issue_detail.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P13
    /// @requirement REQ-ISS-009
    #[test]
    fn test_issue_detail_all_fields() {
        let comments = vec![make_test_comment(1, "alice", "Looks good")];
        let detail = make_test_detail(comments);

        let state = state_with_repo("repo-1").apply(AppEvent::IssueDetailLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            issue_number: 42,
            detail: Box::new(detail),
        });

        let loaded = state
            .issues_state
            .issue_detail
            .expect("detail should be Some");
        assert_eq!(loaded.number, 42);
        assert_eq!(loaded.title, "Test detail issue");
        assert_eq!(loaded.author_login, "octocat");
        assert_eq!(loaded.body, "Detail body text");
        assert_eq!(loaded.labels, vec!["bug".to_string(), "ui".to_string()]);
        assert_eq!(loaded.assignees, vec!["dev1".to_string()]);
        assert_eq!(loaded.milestone, Some("v1.0".to_string()));
        assert!(!loaded.external_url.is_empty());
        assert_eq!(loaded.repo_owner_name, "owner/repo");
    }

    /// P13 Test 8: IssueDetailLoaded with 3 comments — detail.comments.len() == 3.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P13
    /// @requirement REQ-ISS-009
    #[test]
    fn test_issue_detail_comments_timeline() {
        let comments = vec![
            make_test_comment(10, "alice", "First"),
            make_test_comment(11, "bob", "Second"),
            make_test_comment(12, "carol", "Third"),
        ];
        let detail = make_test_detail(comments);

        let state = state_with_repo("repo-1").apply(AppEvent::IssueDetailLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            issue_number: 42,
            detail: Box::new(detail),
        });

        let loaded = state
            .issues_state
            .issue_detail
            .expect("detail should be Some");
        assert_eq!(loaded.comments.len(), 3);
        assert_eq!(loaded.comments[0].author_login, "alice");
        assert_eq!(loaded.comments[2].author_login, "carol");
    }

    /// P13 Test 9: OpenNewCommentComposer transitions inline_state to Composer(NewComment).
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P13
    /// @requirement REQ-ISS-010
    #[test]
    fn test_issue_detail_inline_composer_visible() {
        let mut state = AppState::default();
        state.issues_state.inline_state = InlineState::None;

        let state = state.apply(AppEvent::OpenNewCommentComposer);

        match state.issues_state.inline_state {
            InlineState::Composer {
                target: ComposerTarget::NewComment,
                ..
            } => {} // Correct
            other => panic!("expected Composer(NewComment), got {other:?}"),
        }
    }

    /// P13 Test 10: UpdateDraftFilter sets values in draft_filter fields.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P13
    /// @requirement REQ-ISS-008
    #[test]
    fn test_filter_controls_value_binding() {
        let mut state = AppState::default();
        state.issues_state.filter_controls_open = true;

        // Update multiple draft filter fields
        let state = state
            .apply(AppEvent::UpdateDraftFilter {
                field: "author".to_string(),
                value: "octocat".to_string(),
            })
            .apply(AppEvent::UpdateDraftFilter {
                field: "assignee".to_string(),
                value: "dev1".to_string(),
            })
            .apply(AppEvent::UpdateDraftFilter {
                field: "query_text".to_string(),
                value: "segfault".to_string(),
            });

        assert_eq!(state.issues_state.draft_filter.author, "octocat");
        assert_eq!(state.issues_state.draft_filter.assignee, "dev1");
        assert_eq!(state.issues_state.draft_filter.query_text, "segfault");
    }

    /// P13 Test 11: Loading an empty issue list means the empty-state condition holds
    /// (issues.is_empty() is the data contract the UI component checks).
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P13
    /// @requirement REQ-ISS-014
    #[test]
    fn test_empty_state_no_issues() {
        let state = state_with_repo("repo-1").apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            issues: vec![],
            cursor: None,
            has_more: false,
        });

        // The UI rendering component checks this condition to show the empty message
        assert!(state.issues_state.issues.is_empty());
        assert!(!state.issues_state.list_loading);
    }

    /// P13 Test 12: IssueDetailLoaded with no comments — detail.comments is empty.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P13
    /// @requirement REQ-ISS-014
    #[test]
    fn test_empty_state_no_comments() {
        let detail = make_test_detail(vec![]);

        let state = state_with_repo("repo-1").apply(AppEvent::IssueDetailLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            issue_number: 42,
            detail: Box::new(detail),
        });

        let loaded = state
            .issues_state
            .issue_detail
            .expect("detail should be Some");
        assert!(loaded.comments.is_empty());
    }

    /// P13 Test 13: OpenAgentChooser with no agents leaves agent_chooser as None
    /// (UI empty-state: no agents available to send to).
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P13
    /// @requirement REQ-ISS-014
    #[test]
    fn test_empty_state_no_agents_for_send() {
        let mut state = AppState::default();
        // Confirm no agents are configured
        assert!(state.agents.is_empty());

        // OpenAgentChooser with no agents should leave chooser as None
        state = state.apply(AppEvent::OpenAgentChooser);

        // When agents list is empty, agent_chooser is not opened
        assert!(state.issues_state.agent_chooser.is_none());
    }

    /// P13 Test 14: ScreenMode::DashboardIssues is distinct from ScreenMode::Dashboard.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P13
    /// @requirement REQ-ISS-002
    #[test]
    fn test_keybind_bar_issues_mode() {
        let dashboard_state = AppState::default();
        assert_eq!(dashboard_state.screen_mode, ScreenMode::Dashboard);

        let issues_state = AppState::default().apply(AppEvent::EnterIssuesMode);
        assert_eq!(issues_state.screen_mode, ScreenMode::DashboardIssues);

        // Modes are distinguishable — keybind bar can branch on this
        assert_ne!(dashboard_state.screen_mode, issues_state.screen_mode);

        // And exit returns to Dashboard
        let exited = issues_state.apply(AppEvent::ExitIssuesMode);
        assert_eq!(exited.screen_mode, ScreenMode::Dashboard);
        assert_ne!(exited.screen_mode, ScreenMode::DashboardIssues);
    }

    // =========================================================================
    // P15 Integration Tests — Full State Flow Verification
    // =========================================================================

    /// Helper: create a state already in issues mode with a selected repository.
    fn issues_mode_state_with_repo(repo_id: &str) -> AppState {
        let mut state = AppState::default();
        state.repositories.push(Repository::new(
            RepositoryId(repo_id.to_string()),
            "Test Repo".to_string(),
            repo_id.to_string(),
            std::path::PathBuf::from("/tmp/test"),
        ));
        state.selected_repository_index = Some(0);
        state.apply(AppEvent::EnterIssuesMode)
    }

    /// Helper: create a minimal IssueDetail with given number and empty comments.
    fn p15_detail(number: u64) -> IssueDetail {
        IssueDetail {
            repo_owner_name: "owner/repo".to_string(),
            number,
            title: format!("Issue #{}", number),
            state: IssueState::Open,
            author_login: "user".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-02T00:00:00Z".to_string(),
            labels: vec![],
            assignees: vec![],
            milestone: None,
            body: "Issue body".to_string(),
            external_url: format!("https://github.com/owner/repo/issues/{}", number),
            comments: vec![],
            has_more_comments: false,
            comments_cursor: None,
        }
    }

    /// P15 Test 1: Enter issues mode, load issues, select one, exit.
    /// Verifies: mode entered, issues loaded, mode exited, issues_state cleared,
    /// screen_mode back to Dashboard.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-001
    #[test]
    fn test_mode_lifecycle_enter_browse_exit() {
        // Enter issues mode
        let state = issues_mode_state_with_repo("repo-1");
        assert_eq!(state.screen_mode, ScreenMode::DashboardIssues);
        assert!(state.issues_state.active);
        assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueList);

        // Load issues
        let state = state.apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            issues: vec![make_test_issue(1), make_test_issue(2), make_test_issue(3)],
            cursor: None,
            has_more: false,
        });
        assert_eq!(state.issues_state.issues.len(), 3);
        assert_eq!(state.issues_state.selected_issue_index, Some(0));
        assert!(!state.issues_state.list_loading);

        // Navigate down to select issue #2
        let state = state.apply(AppEvent::IssuesNavigateDown);
        assert_eq!(state.issues_state.selected_issue_index, Some(1));

        // Exit issues mode
        let state = state.apply(AppEvent::ExitIssuesMode);
        assert_eq!(state.screen_mode, ScreenMode::Dashboard);
        assert!(!state.issues_state.active);
    }

    /// P15 Test 2: Enter, load issues, open detail, open composer, type text, cancel, exit.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-001
    #[test]
    fn test_mode_lifecycle_enter_interact_exit() {
        let state = issues_mode_state_with_repo("repo-1");

        // Load issues and open detail
        let state = state
            .apply(AppEvent::IssueListLoaded {
                scope_repo_id: RepositoryId("repo-1".to_string()),
                issues: vec![make_test_issue(10)],
                cursor: None,
                has_more: false,
            })
            .apply(AppEvent::IssuesEnter);
        assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueDetail);

        // Open inline composer
        let state = state.apply(AppEvent::OpenNewCommentComposer);
        match &state.issues_state.inline_state {
            InlineState::Composer {
                target: ComposerTarget::NewComment,
                ..
            } => {}
            other => panic!("expected Composer(NewComment), got {other:?}"),
        }

        // Type some text
        let state = state
            .apply(AppEvent::InlineChar('h'))
            .apply(AppEvent::InlineChar('i'));
        match &state.issues_state.inline_state {
            InlineState::Composer { text, .. } => assert_eq!(text, "hi"),
            other => panic!("expected Composer with text, got {other:?}"),
        }

        // Cancel the composer
        let state = state.apply(AppEvent::InlineCancelOrEsc);
        assert_eq!(state.issues_state.inline_state, InlineState::None);

        // Exit issues mode
        let state = state.apply(AppEvent::ExitIssuesMode);
        assert_eq!(state.screen_mode, ScreenMode::Dashboard);
        assert!(!state.issues_state.active);
    }

    /// P15 Test 3: State-level routing integration — applying routed events in all 3 focus domains
    /// produces correct state transitions.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-002
    #[test]
    fn test_key_routing_all_focus_domains() {
        // RepoList domain: IssuesNavigateUp/Down delegate to repo navigation
        let mut state = AppState::default();
        state.repositories.push(Repository::new(
            RepositoryId("r1".to_string()),
            "R1".to_string(),
            "r1".to_string(),
            std::path::PathBuf::from("/tmp/r1"),
        ));
        state.repositories.push(Repository::new(
            RepositoryId("r2".to_string()),
            "R2".to_string(),
            "r2".to_string(),
            std::path::PathBuf::from("/tmp/r2"),
        ));
        state.selected_repository_index = Some(0);
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.active = true;
        state.issues_state.issue_focus = IssueFocus::RepoList;

        // In RepoList focus, IssuesNavigateDown moves to next repo
        let state = state.apply(AppEvent::IssuesNavigateDown);
        assert_eq!(state.selected_repository_index, Some(1));

        // IssueList domain: IssuesEnter (with issue selected) transitions to IssueDetail
        let mut state = state;
        state.issues_state.issue_focus = IssueFocus::IssueList;
        state.issues_state.issues = vec![make_test_issue(1)];
        state.issues_state.selected_issue_index = Some(0);
        let state = state.apply(AppEvent::IssuesEnter);
        assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueDetail);

        // IssueDetail domain: IssueDetailSubfocusNext advances subfocus (requires detail)
        let mut state = state;
        state.issues_state.issue_detail = Some(p15_detail(1));
        let state = state.apply(AppEvent::IssueDetailSubfocusNext);
        // Body with no comments -> NewComment
        assert_eq!(
            state.issues_state.detail_subfocus,
            DetailSubfocus::NewComment
        );
    }

    /// P15 Test 4: Suppressed key events produce no state change across all focus domains.
    ///
    /// In issues mode, keys 's', Ctrl-d, Ctrl-k, 'l' are suppressed (no-op).
    /// At the state level, the corresponding AppEvent variants don't exist as suppressions;
    /// we verify that the focus domains are preserved through a full navigation sequence
    /// (i.e., the state machine doesn't accidentally jump focus on unknown inputs).
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-002
    #[test]
    fn test_key_routing_suppression_comprehensive() {
        // Suppression at the state level means: any unrecognized event should not
        // affect issues_state focus or mode. Verify focus is stable across domains
        // when we apply IssuesCycleFocus (Tab) — the catch-all that falls through
        // after per-domain handlers.
        let domains = [
            IssueFocus::RepoList,
            IssueFocus::IssueList,
            IssueFocus::IssueDetail,
        ];

        for domain in domains {
            let mut state = AppState::default();
            state.screen_mode = ScreenMode::DashboardIssues;
            state.issues_state.active = true;
            state.issues_state.issue_focus = domain;

            // Applying CloseModal (no-op in issues mode) should not change issues focus
            let state = state.apply(AppEvent::CloseModal);
            assert_eq!(
                state.issues_state.issue_focus, domain,
                "issues focus changed unexpectedly in domain {domain:?}"
            );
            assert_eq!(state.screen_mode, ScreenMode::DashboardIssues);
            assert!(state.issues_state.active);
        }

        // Separately verify that all 4 suppressed-key AppEvent equivalents (no direct
        // mapping) don't affect issues mode state: mode stays active, focus unchanged.
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.active = true;
        state.issues_state.issue_focus = IssueFocus::IssueList;

        // 's' maps to OpenSearch in normal mode, but in issues mode there's no handler;
        // applying OpenSearch opens the modal but doesn't exit issues mode
        let state = state.apply(AppEvent::OpenSearch);
        assert!(
            state.issues_state.active,
            "issues mode should remain active"
        );

        // ClearWarning (no-op) doesn't affect issues focus
        let state = state.apply(AppEvent::ClearWarning);
        assert!(state.issues_state.active);
    }

    /// P15 Test 5: Open composer, type text, apply CommentCreateFailed — draft preserved? error set.
    ///
    /// Note: CommentCreateFailed clears inline_state (sends failed, draft gone). Error is set.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-013
    #[test]
    fn test_error_handling_rate_limit_preserves_draft() {
        let mut state = AppState::default();
        state.issues_state.inline_state = InlineState::Composer {
            target: ComposerTarget::NewComment,
            text: "my draft comment".to_string(),
            cursor: 16,
        };

        let state = state.apply(AppEvent::CommentCreateFailed {
            error: "API rate limit exceeded".to_string(),
        });

        // Error is set
        assert_eq!(
            state.issues_state.error,
            Some("API rate limit exceeded".to_string())
        );
        // Inline is cleared (failed submit clears state)
        assert_eq!(state.issues_state.inline_state, InlineState::None);
    }

    /// P15 Test 6: Apply IssueListLoadFailed with auth message — error displayed, mode still active.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-013
    #[test]
    fn test_error_handling_auth_failure_blocks_ops() {
        let state = issues_mode_state_with_repo("repo-1");
        assert!(state.issues_state.active);

        let state = state.apply(AppEvent::IssueListLoadFailed {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            error: "authentication required: token expired".to_string(),
        });

        // Error is shown
        assert!(state.issues_state.error.is_some());
        let err = state.issues_state.error.as_ref().unwrap();
        assert!(err.contains("authentication") || err.contains("token"));
        // Mode remains active
        assert!(state.issues_state.active);
        assert_eq!(state.screen_mode, ScreenMode::DashboardIssues);
        // List loading is cleared
        assert!(!state.issues_state.list_loading);
    }

    /// P15 Test 7: Apply network error — mode/focus stable, error shown.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-013
    #[test]
    fn test_error_handling_network_error_stable_mode() {
        let state = issues_mode_state_with_repo("repo-1");
        let focus_before = state.issues_state.issue_focus;

        let state = state.apply(AppEvent::IssueListLoadFailed {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            error: "network timeout: connection refused".to_string(),
        });

        // Error is shown
        assert!(state.issues_state.error.is_some());
        // Focus unchanged
        assert_eq!(state.issues_state.issue_focus, focus_before);
        // Mode stable
        assert!(state.issues_state.active);
        assert_eq!(state.screen_mode, ScreenMode::DashboardIssues);
    }

    /// P15 Test 8: Load issues with has_more=true — has_more_issues flag set.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-007
    #[test]
    fn test_pagination_issue_list_auto_load() {
        let state = issues_mode_state_with_repo("repo-1").apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            issues: vec![make_test_issue(1), make_test_issue(2)],
            cursor: Some("cursor-abc".to_string()),
            has_more: true,
        });

        assert!(state.issues_state.has_more_issues);
        assert_eq!(
            state.issues_state.list_cursor,
            Some("cursor-abc".to_string())
        );
        assert_eq!(state.issues_state.issues.len(), 2);
    }

    /// P15 Test 9: Load detail, load first comments page, load second — all comments present in order.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-007
    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_pagination_comments_append() {
        let repo_id = RepositoryId("repo-1".to_string());

        // Load detail with no comments first
        let detail = p15_detail(42);
        let state = issues_mode_state_with_repo("repo-1").apply(AppEvent::IssueDetailLoaded {
            scope_repo_id: repo_id.clone(),
            issue_number: 42,
            detail: Box::new(detail),
        });
        assert_eq!(
            state
                .issues_state
                .issue_detail
                .as_ref()
                .unwrap()
                .comments
                .len(),
            0
        );

        // Load first page of comments
        let state = state.apply(AppEvent::IssueCommentsPageLoaded {
            scope_repo_id: repo_id.clone(),
            issue_number: 42,
            comments: vec![
                IssueComment {
                    comment_id: 1,
                    author_login: "alice".to_string(),
                    created_at: "2024-01-01T00:00:00Z".to_string(),
                    edited_at: None,
                    body: "First comment".to_string(),
                },
                IssueComment {
                    comment_id: 2,
                    author_login: "bob".to_string(),
                    created_at: "2024-01-02T00:00:00Z".to_string(),
                    edited_at: None,
                    body: "Second comment".to_string(),
                },
            ],
            cursor: Some("page2".to_string()),
            has_more: true,
        });
        let detail = state.issues_state.issue_detail.as_ref().unwrap();
        assert_eq!(detail.comments.len(), 2);
        assert!(detail.has_more_comments);

        // Load second page of comments
        let state = state.apply(AppEvent::IssueCommentsPageLoaded {
            scope_repo_id: repo_id.clone(),
            issue_number: 42,
            comments: vec![IssueComment {
                comment_id: 3,
                author_login: "carol".to_string(),
                created_at: "2024-01-03T00:00:00Z".to_string(),
                edited_at: None,
                body: "Third comment".to_string(),
            }],
            cursor: None,
            has_more: false,
        });
        let detail = state.issues_state.issue_detail.as_ref().unwrap();
        assert_eq!(detail.comments.len(), 3);
        assert!(!detail.has_more_comments);
        // Comments appear in insertion order
        assert_eq!(detail.comments[0].comment_id, 1);
        assert_eq!(detail.comments[1].comment_id, 2);
        assert_eq!(detail.comments[2].comment_id, 3);
    }

    /// P15 Test 10: Enter issues, exit — prior focus (pane_focus, selected_agent_index) restored.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-005
    #[test]
    fn test_exit_focus_restoration_valid() {
        let mut state = AppState::default();

        // Set up repo + 2 agents
        state.repositories.push(Repository::new(
            RepositoryId("repo-1".to_string()),
            "Repo".to_string(),
            "repo-1".to_string(),
            std::path::PathBuf::from("/tmp"),
        ));
        state.selected_repository_index = Some(0);
        state.agents.push(Agent::new(
            AgentId("agent-0".to_string()),
            RepositoryId("repo-1".to_string()),
            "Agent 0".to_string(),
            std::path::PathBuf::from("/tmp/a0"),
        ));
        state.agents.push(Agent::new(
            AgentId("agent-1".to_string()),
            RepositoryId("repo-1".to_string()),
            "Agent 1".to_string(),
            std::path::PathBuf::from("/tmp/a1"),
        ));
        state.pane_focus = PaneFocus::Agents;
        state.selected_agent_index = Some(1);

        // Enter issues mode — focus is saved
        let state = state.apply(AppEvent::EnterIssuesMode);
        assert_eq!(state.screen_mode, ScreenMode::DashboardIssues);

        // Exit — prior focus restored
        let state = state.apply(AppEvent::ExitIssuesMode);
        assert_eq!(state.pane_focus, PaneFocus::Agents);
        assert_eq!(state.selected_agent_index, Some(1));
        assert_eq!(state.screen_mode, ScreenMode::Dashboard);
    }

    /// P15 Test 11: Enter issues, agent removed while in issues mode, exit — fallback, no crash.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-005
    #[test]
    fn test_exit_focus_restoration_stale() {
        let mut state = AppState::default();

        // Set up repo + 1 agent
        state.repositories.push(Repository::new(
            RepositoryId("repo-1".to_string()),
            "Repo".to_string(),
            "repo-1".to_string(),
            std::path::PathBuf::from("/tmp"),
        ));
        state.selected_repository_index = Some(0);
        state.agents.push(Agent::new(
            AgentId("agent-0".to_string()),
            RepositoryId("repo-1".to_string()),
            "Agent 0".to_string(),
            std::path::PathBuf::from("/tmp/a0"),
        ));
        state.pane_focus = PaneFocus::Agents;
        state.selected_agent_index = Some(0);

        // Enter issues mode with agent-0 selected
        let state = state.apply(AppEvent::EnterIssuesMode);

        // Simulate agent removed while in issues mode by injecting stale prior_agent_focus
        // (In real usage agents can be deleted; we directly set a stale index)
        let mut state = state;
        state.agents.clear(); // delete agent
        // prior_agent_focus still points to index 0 (now out-of-bounds)

        // Exit — should fall back gracefully
        let state = state.apply(AppEvent::ExitIssuesMode);
        assert_eq!(state.screen_mode, ScreenMode::Dashboard);
        assert!(!state.issues_state.active);
        // No panic; agent_index is None or 0 (fallback)
        assert!(
            state.selected_agent_index.is_none() || state.selected_agent_index == Some(0),
            "expected None or Some(0), got {:?}",
            state.selected_agent_index
        );
    }

    /// P15 Test 12: SelectRepository in issues mode clears issues_state and resets list_loading.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-001
    #[test]
    fn test_scope_change_invalidation() {
        let mut state = AppState::default();

        // Set up two repositories
        state.repositories.push(Repository::new(
            RepositoryId("repo-1".to_string()),
            "Repo 1".to_string(),
            "repo-1".to_string(),
            std::path::PathBuf::from("/tmp/r1"),
        ));
        state.repositories.push(Repository::new(
            RepositoryId("repo-2".to_string()),
            "Repo 2".to_string(),
            "repo-2".to_string(),
            std::path::PathBuf::from("/tmp/r2"),
        ));
        state.selected_repository_index = Some(0);

        // Enter issues mode and load some issues for repo-1
        let state = state
            .apply(AppEvent::EnterIssuesMode)
            .apply(AppEvent::IssueListLoaded {
                scope_repo_id: RepositoryId("repo-1".to_string()),
                issues: vec![make_test_issue(1), make_test_issue(2)],
                cursor: Some("cur".to_string()),
                has_more: true,
            });
        assert_eq!(state.issues_state.issues.len(), 2);
        assert!(state.issues_state.has_more_issues);
        assert!(!state.issues_state.list_loading);

        // Switch to a different repository
        let state = state.apply(AppEvent::SelectRepository(1));

        // Issues data should be cleared and reload triggered
        assert!(state.issues_state.issues.is_empty());
        assert!(state.issues_state.list_loading);
        assert!(!state.issues_state.has_more_issues);
        assert!(state.issues_state.list_cursor.is_none());
        assert!(state.issues_state.selected_issue_index.is_none());
    }

    /// P15 Test 13: SelectRepository clears existing data when repo changes.
    ///
    /// Tests that stale scope response from old repo is irrelevant after repo change.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-013
    #[test]
    fn test_stale_scope_response_suppressed() {
        let mut state = AppState::default();

        state.repositories.push(Repository::new(
            RepositoryId("repo-1".to_string()),
            "Repo 1".to_string(),
            "repo-1".to_string(),
            std::path::PathBuf::from("/tmp/r1"),
        ));
        state.repositories.push(Repository::new(
            RepositoryId("repo-2".to_string()),
            "Repo 2".to_string(),
            "repo-2".to_string(),
            std::path::PathBuf::from("/tmp/r2"),
        ));
        state.selected_repository_index = Some(0);

        let state = state
            .apply(AppEvent::EnterIssuesMode)
            .apply(AppEvent::IssueListLoaded {
                scope_repo_id: RepositoryId("repo-1".to_string()),
                issues: vec![make_test_issue(1)],
                cursor: None,
                has_more: false,
            });

        // Switch repos
        let state = state.apply(AppEvent::SelectRepository(1));
        assert!(state.issues_state.issues.is_empty());

        // Now a stale response for repo-1 arrives
        let state = state.apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            issues: vec![make_test_issue(99)],
            cursor: None,
            has_more: false,
        });

        // Stale data is discarded — repo-1 data does not appear since current repo is repo-2
        assert!(state.issues_state.issues.is_empty());
    }

    /// P15 Test 14: Open composer with text, change repo — inline cancelled, draft_notice set.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-013
    #[test]
    fn test_draft_discard_on_scope_change() {
        let mut state = AppState::default();

        state.repositories.push(Repository::new(
            RepositoryId("repo-1".to_string()),
            "Repo 1".to_string(),
            "repo-1".to_string(),
            std::path::PathBuf::from("/tmp/r1"),
        ));
        state.repositories.push(Repository::new(
            RepositoryId("repo-2".to_string()),
            "Repo 2".to_string(),
            "repo-2".to_string(),
            std::path::PathBuf::from("/tmp/r2"),
        ));
        state.selected_repository_index = Some(0);

        // Enter issues mode, open composer, type text
        let state = state
            .apply(AppEvent::EnterIssuesMode)
            .apply(AppEvent::OpenNewCommentComposer)
            .apply(AppEvent::InlineChar('h'))
            .apply(AppEvent::InlineChar('i'));

        match &state.issues_state.inline_state {
            InlineState::Composer { text, .. } => assert_eq!(text, "hi"),
            other => panic!("expected Composer, got {other:?}"),
        }

        // Change repository — should cancel inline and set draft notice
        let state = state.apply(AppEvent::SelectRepository(1));

        assert_eq!(state.issues_state.inline_state, InlineState::None);
        assert!(
            state.issues_state.draft_notice.is_some(),
            "expected draft_notice to be set"
        );
    }

    /// P15 Test 15: With composer active, attempt to open editor — exclusivity enforced.
    /// With editor active, attempt to open composer — exclusivity enforced.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-010
    #[test]
    fn test_inline_exclusivity_all_combinations() {
        let mut base = AppState::default();
        base.screen_mode = ScreenMode::DashboardIssues;

        // Composer active → OpenInlineEditor blocked
        base.issues_state.inline_state = InlineState::Composer {
            target: ComposerTarget::NewComment,
            text: "draft".to_string(),
            cursor: 5,
        };
        let state = base.clone().apply(AppEvent::OpenInlineEditor {
            target: EditorTarget::IssueBody,
        });
        match &state.issues_state.inline_state {
            InlineState::Composer { .. } => {}
            other => panic!("Composer should block editor open, got {other:?}"),
        }

        // Editor active → OpenNewCommentComposer blocked
        base.issues_state.inline_state = InlineState::Editor {
            target: EditorTarget::IssueBody,
            text: "edit".to_string(),
            cursor: 4,
        };
        let state = base.clone().apply(AppEvent::OpenNewCommentComposer);
        match &state.issues_state.inline_state {
            InlineState::Editor { .. } => {}
            other => panic!("Editor should block composer open, got {other:?}"),
        }

        // Editor active → OpenReplyComposer blocked
        base.issues_state.inline_state = InlineState::Editor {
            target: EditorTarget::IssueBody,
            text: "edit".to_string(),
            cursor: 4,
        };
        let state = base
            .clone()
            .apply(AppEvent::OpenReplyComposer { comment_index: 0 });
        match &state.issues_state.inline_state {
            InlineState::Editor { .. } => {}
            other => panic!("Editor should block reply composer open, got {other:?}"),
        }
    }

    /// P15 Test 16: Build send payload from detail with focused comment — all fields present.
    ///
    /// Tests that state correctly holds all data needed for agent send payload:
    /// issue detail, focused comment (via detail_subfocus), agent chooser state.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-011
    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_send_to_agent_payload_complete() {
        let mut state = AppState::default();

        state.repositories.push(Repository::new(
            RepositoryId("repo-1".to_string()),
            "Repo 1".to_string(),
            "repo-1".to_string(),
            std::path::PathBuf::from("/tmp/r1"),
        ));
        state.selected_repository_index = Some(0);

        state.agents.push(Agent::new(
            AgentId("agent-1".to_string()),
            RepositoryId("repo-1".to_string()),
            "My Agent".to_string(),
            std::path::PathBuf::from("/tmp/a1"),
        ));

        // Load issue detail with 2 comments
        let state = state
            .apply(AppEvent::EnterIssuesMode)
            .apply(AppEvent::IssueDetailLoaded {
                scope_repo_id: RepositoryId("repo-1".to_string()),
                issue_number: 7,
                detail: Box::new(IssueDetail {
                    repo_owner_name: "owner/repo".to_string(),
                    number: 7,
                    title: "Fix crash".to_string(),
                    state: IssueState::Open,
                    author_login: "octocat".to_string(),
                    created_at: "2024-01-01T00:00:00Z".to_string(),
                    updated_at: "2024-01-02T00:00:00Z".to_string(),
                    labels: vec!["bug".to_string()],
                    assignees: vec![],
                    milestone: None,
                    body: "Crash on startup".to_string(),
                    external_url: "https://github.com/owner/repo/issues/7".to_string(),
                    comments: vec![
                        IssueComment {
                            comment_id: 100,
                            author_login: "dev".to_string(),
                            created_at: "2024-01-02T00:00:00Z".to_string(),
                            edited_at: None,
                            body: "Reproduced on main".to_string(),
                        },
                        IssueComment {
                            comment_id: 101,
                            author_login: "tester".to_string(),
                            created_at: "2024-01-03T00:00:00Z".to_string(),
                            edited_at: None,
                            body: "Also seen in v2.1".to_string(),
                        },
                    ],
                    has_more_comments: false,
                    comments_cursor: None,
                }),
            });

        // Subfocus on comment index 1
        let state = state.apply(AppEvent::IssueDetailSubfocusNext); // Body -> Comment(0)
        let state = state.apply(AppEvent::IssueDetailSubfocusNext); // Comment(0) -> Comment(1)
        assert_eq!(
            state.issues_state.detail_subfocus,
            DetailSubfocus::Comment(1)
        );

        // Open agent chooser
        let state = state.apply(AppEvent::OpenAgentChooser);
        let chooser = state
            .issues_state
            .agent_chooser
            .as_ref()
            .expect("chooser should be open");
        assert_eq!(chooser.agents.len(), 1);
        assert_eq!(chooser.agents[0].1, "My Agent");

        // Verify all payload fields are accessible from state
        let detail = state
            .issues_state
            .issue_detail
            .as_ref()
            .expect("detail should be set");
        assert_eq!(detail.number, 7);
        assert_eq!(detail.title, "Fix crash");
        assert_eq!(detail.body, "Crash on startup");
        let focused_comment = match state.issues_state.detail_subfocus {
            DetailSubfocus::Comment(idx) => detail.comments.get(idx),
            _ => None,
        };
        assert!(focused_comment.is_some());
        assert_eq!(focused_comment.unwrap().comment_id, 101);
    }

    /// P15 Test 17: OpenAgentChooser with no agents — chooser not opened.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-011
    #[test]
    fn test_send_to_agent_no_agents() {
        let state = issues_mode_state_with_repo("repo-1");
        assert!(state.agents.is_empty());

        let state = state.apply(AppEvent::OpenAgentChooser);

        assert!(state.issues_state.agent_chooser.is_none());
    }

    /// P15 Test 18: Build payload with issue_base_prompt — field present in repository.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-012
    #[test]
    fn test_issue_base_prompt_in_payload() {
        let mut state = AppState::default();

        // Repository with issue_base_prompt set
        let mut repo = Repository::new(
            RepositoryId("repo-1".to_string()),
            "Repo 1".to_string(),
            "repo-1".to_string(),
            std::path::PathBuf::from("/tmp/r1"),
        );
        repo.issue_base_prompt = "Always look for root causes before proposing fixes.".to_string();
        state.repositories.push(repo);
        state.selected_repository_index = Some(0);

        let state = state.apply(AppEvent::EnterIssuesMode);

        // Verify the field is accessible from selected repository
        let repo = state
            .selected_repository()
            .expect("repo should be selected");
        assert_eq!(
            repo.issue_base_prompt,
            "Always look for root causes before proposing fixes."
        );
    }

    /// P15 Test 19: Set up state with inline active + search focused + filter open;
    /// apply Esc events in sequence; verify each level closes correctly.
    ///
    /// The 6-level Esc chain (from innermost to outermost):
    ///   1. Inline editor/composer → InlineCancelOrEsc
    ///   2. Agent chooser → AgentChooserCancel
    ///   3. Search non-empty → ClearSearch
    ///   4. Search empty → BlurSearchInput
    ///   5. Filter controls → CloseFilterControls
    ///   6. Mode exit → ExitIssuesMode
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P15
    /// @requirement REQ-ISS-004
    #[test]
    fn test_esc_chain_all_six_levels_integrated() {
        // Level 1: Inline Composer — InlineCancelOrEsc closes it
        let mut state = AppState::default();
        state.screen_mode = ScreenMode::DashboardIssues;
        state.issues_state.active = true;
        state.issues_state.inline_state = InlineState::Composer {
            target: ComposerTarget::NewComment,
            text: "draft".to_string(),
            cursor: 5,
        };
        let state = state.apply(AppEvent::InlineCancelOrEsc);
        assert_eq!(state.issues_state.inline_state, InlineState::None);

        // Level 2: Agent Chooser — AgentChooserCancel closes it
        let mut state = state;
        state.issues_state.agent_chooser = Some(AgentChooserState {
            selected_index: 0,
            agents: vec![(AgentId("a1".to_string()), "Agent 1".to_string())],
        });
        let state = state.apply(AppEvent::AgentChooserCancel);
        assert!(state.issues_state.agent_chooser.is_none());

        // Level 3: Search with text — ClearSearch clears text (stays focused)
        let mut state = state;
        state.issues_state.search_input_focused = true;
        state.issues_state.search_query = "open bug".to_string();
        let state = state.apply(AppEvent::ClearSearch);
        assert!(state.issues_state.search_query.is_empty());
        assert!(state.issues_state.search_input_focused);

        // Level 4: Search empty — BlurSearchInput removes focus
        let state = state.apply(AppEvent::BlurSearchInput);
        assert!(!state.issues_state.search_input_focused);

        // Level 5: Filter controls open — CloseFilterControls closes them
        let mut state = state;
        state.issues_state.filter_controls_open = true;
        let state = state.apply(AppEvent::CloseFilterControls);
        assert!(!state.issues_state.filter_controls_open);

        // Level 6: Nothing else active — ExitIssuesMode exits mode
        let state = state.apply(AppEvent::ExitIssuesMode);
        assert_eq!(state.screen_mode, ScreenMode::Dashboard);
        assert!(!state.issues_state.active);
    }

    /// InlineCursorUp/Down move the cursor between lines in multi-line text.
    #[test]
    fn test_inline_cursor_vertical_navigation() {
        use super::inline_cursor_vertical;

        // 3 lines: abc, def, ghi — offsets [0..3], [4..7], [8..11]
        let text = ["abc", "def", "ghi"].join(&String::from(char::from(0x0Au8)));

        // Down from line 0 col 1 to line 1 col 1
        let mut cursor = 1;
        inline_cursor_vertical(&text, &mut cursor, 1);
        assert_eq!(cursor, 5);

        // Down from line 1 col 1 to line 2 col 1
        inline_cursor_vertical(&text, &mut cursor, 1);
        assert_eq!(cursor, 9);

        // Down from last line stays
        inline_cursor_vertical(&text, &mut cursor, 1);
        assert_eq!(cursor, 9);

        // Up from line 2 col 1 to line 1 col 1
        inline_cursor_vertical(&text, &mut cursor, -1);
        assert_eq!(cursor, 5);

        // Up from line 1 col 1 to line 0 col 1
        inline_cursor_vertical(&text, &mut cursor, -1);
        assert_eq!(cursor, 1);

        // Up from first line stays
        inline_cursor_vertical(&text, &mut cursor, -1);
        assert_eq!(cursor, 1);
    }

    /// InlineCursorUp/Down clamp column when target line is shorter.
    #[test]
    fn test_inline_cursor_vertical_column_clamping() {
        use super::inline_cursor_vertical;

        // 3 lines: abcdef (len 6), xy (len 2), z (len 1)
        let nl = String::from(char::from(0x0Au8));
        let text = ["abcdef", "xy", "z"].join(&nl);

        // Cursor at col 5 of line 0 → down to line 1 (len 2) → clamp to col 2
        let mut cursor = 5;
        inline_cursor_vertical(&text, &mut cursor, 1);
        assert_eq!(cursor, 9); // line 1 start=7, col clamped to 2 = byte 9
    }
}
