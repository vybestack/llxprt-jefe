//! Application state and event layer.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @requirement REQ-TECH-001
//! @requirement REQ-TECH-003
//!
//! Pseudocode reference: component-001 lines 01-12

mod form_cursor;
mod form_ops;
mod issues_inline_ops;
mod issues_load_ops;
mod issues_mutation_ops;
mod issues_ops;
// @plan PLAN-20260624-PR-MODE.P03
// @requirement REQ-PR-001
mod prs_inline_ops;
mod prs_load_ops;
mod prs_mutation_ops;
mod prs_nav_ops;
mod prs_ops;
mod selectors;
pub mod state_ops;
mod types;
mod util;

pub use state_ops::{delete_selected_agent, delete_selected_repository};
pub use types::*;

use tracing::{debug, trace};

use crate::domain::{Agent, AgentId, AgentStatus, DEFAULT_SANDBOX_FLAGS, SandboxEngine};
use crate::domain::{Repository, RepositoryId};
use crate::messages::{
    AppMessage, MessageRoute, ModalMessage, PersistenceMessage, RepositoryAgentMessage,
    RuntimeMessage, SystemMessage, ThemeMessage, UiNavigationMessage,
};

/// Move the inline editor cursor up or down by `direction` lines (-1 = up, 1 = down).
/// Attempts to land on the same column in the target line, clamping to line length.
///
/// Column positions are computed in **characters** (Unicode scalar values), not
/// bytes, so multi-byte text does not cause cursor jumps to invalid positions.
fn inline_cursor_vertical(text: &str, cursor: &mut usize, direction: i32) {
    // Split into lines (as &str slices) preserving byte offsets.
    let mut line_byte_starts: Vec<usize> = vec![0];
    for (byte_idx, ch) in text.char_indices() {
        if ch == char::from(0x0Au8) {
            line_byte_starts.push(byte_idx + ch.len_utf8());
        }
    }

    // Clamp the cursor to a valid char boundary within the text. As the shared
    // single source of truth for vertical movement in both Issues and PR modes,
    // this defensively walks a mid-codepoint offset DOWN to the nearest UTF-8
    // boundary so slicing cannot panic on malformed input.
    let mut clamped_cursor = (*cursor).min(text.len());
    while clamped_cursor > 0 && !text.is_char_boundary(clamped_cursor) {
        clamped_cursor -= 1;
    }
    let before_cursor = &text[..clamped_cursor];

    // Find which line the cursor is on (by byte offset).
    let mut current_line = 0;
    for (i, &byte_start) in line_byte_starts.iter().enumerate() {
        if clamped_cursor >= byte_start {
            current_line = i;
        }
    }

    // Compute the current column in CHARACTERS, not bytes.
    let line_byte_start = line_byte_starts[current_line];
    let col_chars = before_cursor[line_byte_start..].chars().count();

    let target_line = if direction < 0 {
        current_line.saturating_sub(1)
    } else {
        (current_line + 1).min(line_byte_starts.len() - 1)
    };

    if target_line == current_line {
        return; // already at first/last line
    }

    // Slice the target line (excluding its trailing newline) and convert the
    // desired character column back to a byte offset within the target line.
    let target_byte_start = line_byte_starts[target_line];
    let target_line_end_byte = if target_line + 1 < line_byte_starts.len() {
        line_byte_starts[target_line + 1] - 1
    } else {
        text.len()
    };
    let target_slice = &text[target_byte_start..target_line_end_byte];
    let target_byte_offset = target_slice
        .char_indices()
        .nth(col_chars)
        .map_or(target_slice.len(), |(byte_idx, _)| byte_idx);

    *cursor = target_byte_start + target_byte_offset;
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
            // @plan PLAN-20260624-PR-MODE.P05
            // @requirement REQ-PR-001
            // @pseudocode component-004 lines 86-94
            AppMessage::PullRequests(message) => {
                let msg_debug = format!("{message:?}");
                let handled = self.apply_prs_message(message);
                debug_assert!(handled, "unhandled PullRequestsMessage: {msg_debug}");
            }
        }

        self.finalize_message(route);
        self
    }

    fn terminal_blocks(message: &AppMessage) -> bool {
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

    fn apply_ui_navigation(&mut self, message: UiNavigationMessage) {
        match message {
            UiNavigationMessage::NavigateUp => self.handle_navigate_up(),
            UiNavigationMessage::NavigateDown => self.handle_navigate_down(),
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
                self.normalize_selection_indices();
            }
            UiNavigationMessage::EnterSplitMode => self.screen_mode = ScreenMode::Split,
            UiNavigationMessage::ExitSplitMode => self.exit_split_mode(),
            UiNavigationMessage::EnterGrabMode => {
                self.split_grab_index = self.selected_repository_visible_index();
            }
            UiNavigationMessage::ExitGrabMode => self.split_grab_index = None,
            UiNavigationMessage::GrabMoveUp => self.move_split_grab_up(),
            UiNavigationMessage::GrabMoveDown => self.move_split_grab_down(),
            UiNavigationMessage::SetSplitFilter(filter) => self.split_filter = filter,
        }
    }

    fn cycle_pane_focus(&mut self) {
        let old = self.pane_focus;
        self.pane_focus = match self.pane_focus {
            PaneFocus::Repositories => PaneFocus::Agents,
            PaneFocus::Agents => PaneFocus::Terminal,
            PaneFocus::Terminal => PaneFocus::Repositories,
        };
        debug!(old = ?old, new = ?self.pane_focus, "pane focus changed (tab)");
    }

    fn move_pane_focus_right(&mut self) {
        let old = self.pane_focus;
        self.pane_focus = match self.pane_focus {
            PaneFocus::Repositories => PaneFocus::Agents,
            PaneFocus::Agents | PaneFocus::Terminal => PaneFocus::Terminal,
        };
        debug!(old = ?old, new = ?self.pane_focus, "pane focus changed (right)");
    }

    fn move_pane_focus_left(&mut self) {
        let old = self.pane_focus;
        self.pane_focus = match self.pane_focus {
            PaneFocus::Repositories | PaneFocus::Agents => PaneFocus::Repositories,
            PaneFocus::Terminal => PaneFocus::Agents,
        };
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
            self.remember_selected_agent_for_current_repo();
            self.selected_repository_index = Some(idx);
            self.restore_selected_agent_for_current_repo();

            if self.issues_state.active {
                self.reset_issues_for_repo_change();
            }
            // @plan PLAN-20260624-PR-MODE.P05
            // @requirement REQ-PR-003
            if self.prs_state.active {
                self.reset_prs_for_repo_change();
            }
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
        true
    }

    fn select_agent_by_local_index(&mut self, idx: usize) {
        if let Some(repository_id) = self.selected_repository_id().cloned() {
            let visible_indices = self.agent_indices_for_repository(&repository_id);
            if idx < visible_indices.len() {
                self.selected_agent_index = Some(visible_indices[idx]);
                self.remember_selected_agent_for_current_repo();
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
            self.remember_selected_agent_for_current_repo();
            self.selected_repository_index = Some(target_repo_idx);
            self.selected_agent_index = Some(agent_idx);
            self.pane_focus = PaneFocus::Agents;
            self.terminal_focused = false;
            self.remember_selected_agent_for_current_repo();
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

    fn apply_modal_message(&mut self, message: ModalMessage) {
        match message {
            ModalMessage::OpenHelp => self.modal = ModalState::Help,
            ModalMessage::OpenSearch => {
                self.modal = ModalState::Search {
                    query: String::new(),
                };
            }
            ModalMessage::CloseModal => self.modal = ModalState::None,
            ModalMessage::SubmitForm => self.handle_submit_form(),
            ModalMessage::FormChar(c) => self.handle_form_char(c),
            ModalMessage::FormBackspace => self.handle_form_backspace(),
            ModalMessage::FormDelete => self.handle_form_delete(),
            ModalMessage::FormMoveCursorLeft => self.handle_form_move_cursor_left(),
            ModalMessage::FormMoveCursorRight => self.handle_form_move_cursor_right(),
            ModalMessage::FormNextField => self.handle_form_next_field(),
            ModalMessage::FormPrevField => self.handle_form_prev_field(),
            ModalMessage::FormToggleCheckbox => self.handle_form_toggle_checkbox(),
        }
    }

    fn apply_repository_agent_message(&mut self, message: RepositoryAgentMessage) {
        match message {
            RepositoryAgentMessage::OpenNewRepository => self.open_new_repository_modal(),
            RepositoryAgentMessage::OpenEditRepository(id) => self.open_edit_repository_modal(id),
            RepositoryAgentMessage::OpenDeleteRepository(id) => {
                self.modal = ModalState::ConfirmDeleteRepository { id };
            }
            RepositoryAgentMessage::OpenNewAgent(repository_id) => {
                self.open_new_agent_modal(repository_id);
            }
            RepositoryAgentMessage::OpenEditAgent(id) => self.open_edit_agent_modal(id),
            RepositoryAgentMessage::OpenDeleteAgent(id) => {
                self.modal = ModalState::ConfirmDeleteAgent {
                    id,
                    delete_work_dir: false,
                };
            }
            RepositoryAgentMessage::ToggleDeleteWorkDir => self.toggle_delete_work_dir(),
        }
    }

    fn open_new_repository_modal(&mut self) {
        self.modal = ModalState::NewRepository {
            fields: RepositoryFormFields::default(),
            focus: RepositoryFormFocus::default(),
            cursor: RepositoryFormCursor::default(),
        };
    }

    fn open_edit_repository_modal(&mut self, id: RepositoryId) {
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

    fn open_new_agent_modal(&mut self, repository_id: RepositoryId) {
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

    fn open_edit_agent_modal(&mut self, id: AgentId) {
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

    fn toggle_delete_work_dir(&mut self) {
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

    fn apply_runtime_message(&mut self, message: RuntimeMessage) {
        match message {
            RuntimeMessage::KillAgent(agent_id) => {
                if let Some(agent) = self.agents.iter_mut().find(|a| a.id == agent_id) {
                    agent.status = AgentStatus::Dead;
                    agent.runtime_binding = None;
                }
            }
            RuntimeMessage::AgentStatusChanged(agent_id, status) => {
                if let Some(agent) = self.agents.iter_mut().find(|a| a.id == agent_id) {
                    agent.status = status;
                }
            }
            RuntimeMessage::RelaunchAgent(agent_id) => {
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
        }
    }

    fn apply_system_message(&mut self, message: SystemMessage) {
        match message {
            SystemMessage::ClearError => self.error_message = None,
            SystemMessage::ClearWarning => self.warning_message = None,
            SystemMessage::Quit => {}
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
}

#[cfg(test)]
#[path = "issues_tests.rs"]
mod issues_tests;

#[cfg(test)]
#[path = "issues_tests_components.rs"]
mod issues_tests_components;

#[cfg(test)]
#[path = "issues_tests_detail.rs"]
mod issues_tests_detail;

#[cfg(test)]
#[path = "issues_tests_detail_flow.rs"]
mod issues_tests_detail_flow;

#[cfg(test)]
#[path = "issues_tests_repo_nav.rs"]
mod issues_tests_repo_nav;

#[cfg(test)]
#[path = "issues_tests_filter.rs"]
mod issues_tests_filter;

#[cfg(test)]
#[path = "issues_tests_composer_focus.rs"]
mod issues_tests_composer_focus;

#[cfg(test)]
#[path = "prs_tests.rs"]
mod prs_tests;

#[cfg(test)]
#[path = "prs_tests_detail.rs"]
mod prs_tests_detail;

#[cfg(test)]
#[path = "prs_tests_filter.rs"]
mod prs_tests_filter;

#[cfg(test)]
#[path = "prs_tests_repo_nav.rs"]
mod prs_tests_repo_nav;

/// Shared `#[cfg(test)]` fixtures used by the PR-mode reducer test modules.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 44-50
#[cfg(test)]
#[path = "prs_test_fixtures.rs"]
mod prs_test_fixtures;

#[cfg(test)]
#[path = "prs_tests_composer_focus.rs"]
mod prs_tests_composer_focus;

/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 44-50
#[cfg(test)]
#[path = "prs_tests_cursor_arrows.rs"]
mod prs_tests_cursor_arrows;

#[cfg(test)]
#[path = "prs_tests_detail_flow.rs"]
mod prs_tests_detail_flow;

#[cfg(test)]
#[path = "prs_tests_components.rs"]
mod prs_tests_components;

// @plan PLAN-20260624-PR-MODE.P15
// @requirement REQ-PR-001
// @pseudocode component-001 lines 66-291
#[cfg(test)]
#[path = "prs_integration_tests.rs"]
mod prs_integration_tests;
