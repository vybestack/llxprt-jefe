//! Central application state management.
//!
//! This module provides the main `AppState` struct that manages
//! the entire application state, including projects, tasks, UI state,
//! and event handling.

use chrono::Utc;

use crate::data::{Agent, AgentStatus, OutputKind, OutputLine, Repository};
use crate::events::AppEvent;

/// Expand a leading `~` or `~/` to the user's home directory.
fn expand_tilde(path: &str) -> String {
    if path == "~" || path.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            let home = home.to_string_lossy();
            return if path == "~" {
                home.into_owned()
            } else {
                format!("{}{}", home, &path[1..])
            };
        }
    }
    path.to_owned()
}

/// Normalize profile input from forms.
///
/// `[]` and empty values both mean "use llxprt defaults" and are persisted as
/// an empty string.
fn normalize_profile_input(value: String) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "[]" {
        String::new()
    } else {
        value
    }
}

/// Extract mode flags as whitespace-separated tokens.
fn mode_tokens(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect()
}

/// Returns true if mode contains `--continue`.
fn mode_has_continue(value: &str) -> bool {
    value.split_whitespace().any(|flag| flag == "--continue")
}

/// Remove all `--continue` flags from mode string.
fn mode_without_continue(value: &str) -> String {
    mode_tokens(value)
        .into_iter()
        .filter(|flag| flag != "--continue")
        .collect::<Vec<_>>()
        .join(" ")
}

/// Compose persisted mode string from mode text input + continue checkbox.
fn compose_mode(mode_input: String, pass_continue: bool) -> String {
    let base = mode_without_continue(&mode_input);
    let mut flags = if base.trim().is_empty() {
        vec!["--yolo".to_owned()]
    } else {
        mode_tokens(&base)
    };

    if pass_continue && !flags.iter().any(|flag| flag == "--continue") {
        flags.push("--continue".to_owned());
    }

    flags.join(" ")
}

/// The currently active pane in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivePane {
    /// Repository/agent sidebar.
    #[default]
    Sidebar,
    /// Agent list view.
    AgentList,
    /// Agent detail preview pane.
    Preview,
}

/// The current screen being displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    /// Main dashboard view.
    #[default]
    Dashboard,
    /// Command palette/search.
    CommandPalette,
    /// New agent form.
    NewAgent,
    /// New repository form.
    NewRepository,
    /// Split mode view for all running agents.
    Split,
    /// Edit agent config form.
    EditAgent,
    /// Edit repository config form.
    EditRepository,
}

/// Modal dialog state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ModalState {
    /// No modal is shown.
    #[default]
    None,
    /// Confirmation dialog for deleting a repository.
    ConfirmDeleteRepo(usize),
    /// Confirmation dialog for deleting an agent.
    ConfirmDeleteAgent {
        /// Repository index.
        repo_idx: usize,
        /// Agent index within the repo.
        agent_idx: usize,
        /// Whether to also delete the working directory.
        delete_work_dir: bool,
    },
    /// Help/keyboard shortcuts dialog.
    Help,
}

/// Which pane is focused inside split mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SplitFocus {
    /// Repository sidebar (filter selector).
    #[default]
    Repos,
    /// Agent list.
    Agents,
}

/// State for split-mode reordering workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SplitState {
    /// Which pane is focused in split mode.
    pub focus: SplitFocus,
    /// Whether the highlighted agent is "grabbed" for reordering.
    pub grabbed: bool,
    /// Cursor position in the (filtered) running-agent list.
    pub selected_row: usize,
    /// Repository filter: `None` = show all, `Some(idx)` = filter to that repo.
    pub repo_filter: Option<usize>,
    /// Cursor position in the repo filter sidebar (0 = "All").
    pub repo_cursor: usize,
}

/// Central application state.
#[derive(Debug, Clone, Default)]
pub struct AppState {
    /// All repositories with their agents.
    pub repositories: Vec<Repository>,
    /// Index of the currently selected repository.
    pub selected_repo: usize,
    /// Index of the currently selected agent within the selected repository.
    pub selected_agent: usize,
    /// Which pane is currently active/focused.
    pub active_pane: ActivePane,
    /// Current screen being displayed.
    pub screen: Screen,
    /// Current modal state.
    pub modal: ModalState,
    /// Search query text.
    pub search_query: String,
    /// Whether search mode is active.
    pub is_searching: bool,
    /// Whether keyboard input is forwarded to the embedded terminal PTY.
    pub terminal_focused: bool,
    /// Split mode state.
    pub split: SplitState,
    /// Form fields for new agent dialog (name, description, work_dir, profile, mode).
    pub new_agent_fields: Vec<String>,
    /// Whether new-agent launch includes `--continue`.
    pub new_agent_pass_continue: bool,
    /// Which field is focused in the new agent form (0-based).
    ///
    /// `0..=4` are text fields, `5` is the "pass --continue" checkbox.
    pub new_agent_focus: usize,
    /// Whether the work_dir field has been manually edited by the user.
    pub new_agent_workdir_manual: bool,
    /// Form fields for new repository dialog (name, base_dir, default_profile, default_model).
    pub new_repository_fields: Vec<String>,
    /// Which field is focused in the new repository form (0-based).
    pub new_repository_focus: usize,
}

impl AppState {
    /// Creates a new `AppState` with the given repositories.
    #[must_use]
    pub fn new(repositories: Vec<Repository>) -> Self {
        Self {
            repositories,
            selected_repo: 0,
            selected_agent: 0,
            active_pane: ActivePane::Sidebar,
            screen: Screen::Dashboard,
            modal: ModalState::None,
            search_query: String::new(),
            is_searching: false,
            terminal_focused: false,
            split: SplitState {
                focus: SplitFocus::Repos,
                grabbed: false,
                selected_row: 0,
                repo_filter: None,
                repo_cursor: 0,
            },
            new_agent_fields: vec![String::new(); 5],
            new_agent_pass_continue: true,
            new_agent_focus: 0,
            new_agent_workdir_manual: false,
            new_repository_fields: vec![String::new(); 3],
            new_repository_focus: 0,
        }
    }

    /// Returns a reference to the currently selected repository, if any.
    #[must_use]
    pub fn current_repo(&self) -> Option<&Repository> {
        self.repositories.get(self.selected_repo)
    }

    /// Returns a mutable reference to the currently selected repository, if any.
    #[must_use]
    pub fn current_repo_mut(&mut self) -> Option<&mut Repository> {
        self.repositories.get_mut(self.selected_repo)
    }

    /// Returns a reference to the currently selected agent, if any.
    #[must_use]
    pub fn current_agent(&self) -> Option<&Agent> {
        self.current_repo()
            .and_then(|r| r.agents.get(self.selected_agent))
    }

    /// Returns a mutable reference to the currently selected agent, if any.
    #[must_use]
    pub fn current_agent_mut(&mut self) -> Option<&mut Agent> {
        let selected_agent = self.selected_agent;
        self.current_repo_mut()
            .and_then(move |r| r.agents.get_mut(selected_agent))
    }

    /// Returns the total number of agents across all repositories.
    #[must_use]
    pub fn agent_count(&self) -> usize {
        self.repositories.iter().map(|r| r.agents.len()).sum()
    }

    /// Returns flat (repo_idx, agent_idx) tuples for all running agents.
    #[must_use]
    pub fn running_agent_positions(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (ri, repo) in self.repositories.iter().enumerate() {
            for (ai, agent) in repo.agents.iter().enumerate() {
                if agent.status == AgentStatus::Running {
                    out.push((ri, ai));
                }
            }
        }
        out
    }

    /// Returns running agent positions filtered by the split-mode repo filter.
    #[must_use]
    pub fn filtered_running_positions(&self) -> Vec<(usize, usize)> {
        let all = self.running_agent_positions();
        match self.split.repo_filter {
            None => all,
            Some(ri) => all.into_iter().filter(|(r, _)| *r == ri).collect(),
        }
    }

    /// Returns the number of currently running agents.
    #[must_use]
    pub fn running_count(&self) -> usize {
        self.repositories
            .iter()
            .flat_map(|r| &r.agents)
            .filter(|a| a.status == AgentStatus::Running)
            .count()
    }

    /// Handles an application event and updates state accordingly.
    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Quit => {}
            AppEvent::NavigateUp => self.navigate_up(),
            AppEvent::NavigateDown => self.navigate_down(),
            AppEvent::NavigateLeft => self.navigate_left(),
            AppEvent::NavigateRight => self.navigate_right(),
            AppEvent::Select => self.handle_select(),
            AppEvent::Back => self.handle_back(),
            AppEvent::NewAgent => self.open_new_agent(),
            AppEvent::NewRepository => self.open_new_repository(),
            AppEvent::DeleteAgent => self.delete_current_agent(),
            AppEvent::OpenSearch => self.toggle_search(),
            AppEvent::OpenHelp => self.show_help(),
            AppEvent::FocusRepository => self.focus_repository(),
            AppEvent::FocusAgentList => self.focus_agent_list(),
            AppEvent::FocusTerminal => self.focus_terminal(),
            AppEvent::ToggleSplitMode => self.toggle_split_mode(),
            AppEvent::KillAgent => self.kill_current_agent(),
            AppEvent::RelaunchAgent => self.relaunch_current_agent(),
            AppEvent::ReturnToMainFocused => self.return_to_main_focused(),
            AppEvent::ToggleTerminalFocus => self.toggle_terminal_focus(),
            AppEvent::Char(c) => self.handle_char(c),
            AppEvent::SubmitForm => self.submit_form(),
            AppEvent::NextField => self.next_field(),
            AppEvent::PrevField => self.prev_field(),
            AppEvent::Backspace => self.handle_backspace(),
        }
    }

    fn toggle_delete_work_dir(&mut self) {
        if let ModalState::ConfirmDeleteAgent { delete_work_dir, .. } = &mut self.modal {
            *delete_work_dir = !*delete_work_dir;
        }
    }

    fn navigate_up(&mut self) {
        // Toggle delete_work_dir in ConfirmDeleteAgent modal
        if matches!(self.modal, ModalState::ConfirmDeleteAgent { .. }) {
            self.toggle_delete_work_dir();
            return;
        }

        if self.screen == Screen::Split {
            match self.split.focus {
                SplitFocus::Repos => {
                    if self.split.repo_cursor > 0 {
                        self.split.repo_cursor -= 1;
                    }
                }
                SplitFocus::Agents => {
                    let filtered = self.filtered_running_positions();
                    if filtered.is_empty() {
                        return;
                    }
                    if self.split.selected_row > 0 {
                        if self.split.grabbed {
                            self.swap_filtered_agents(
                                self.split.selected_row,
                                self.split.selected_row - 1,
                            );
                        }
                        self.split.selected_row -= 1;
                    }
                }
            }
            return;
        }

        match self.active_pane {
            ActivePane::Sidebar => {
                if self.selected_repo > 0 {
                    self.selected_repo -= 1;
                    self.selected_agent = 0;
                }
            }
            ActivePane::AgentList => {
                if self.selected_agent > 0 {
                    self.selected_agent -= 1;
                }
            }
            ActivePane::Preview => {}
        }
    }

    fn navigate_down(&mut self) {
        // Toggle delete_work_dir in ConfirmDeleteAgent modal
        if matches!(self.modal, ModalState::ConfirmDeleteAgent { .. }) {
            self.toggle_delete_work_dir();
            return;
        }

        if self.screen == Screen::Split {
            match self.split.focus {
                SplitFocus::Repos => {
                    // 0 = All, 1..=len = individual repos.
                    let max = self.repositories.len(); // max cursor == len (0-based "All" + repos)
                    if self.split.repo_cursor < max {
                        self.split.repo_cursor += 1;
                    }
                }
                SplitFocus::Agents => {
                    let filtered = self.filtered_running_positions();
                    if filtered.is_empty() {
                        return;
                    }
                    if self.split.selected_row + 1 < filtered.len() {
                        if self.split.grabbed {
                            self.swap_filtered_agents(
                                self.split.selected_row,
                                self.split.selected_row + 1,
                            );
                        }
                        self.split.selected_row += 1;
                    }
                }
            }
            return;
        }

        match self.active_pane {
            ActivePane::Sidebar => {
                if self.selected_repo + 1 < self.repositories.len() {
                    self.selected_repo += 1;
                    self.selected_agent = 0;
                }
            }
            ActivePane::AgentList => {
                if let Some(repo) = self.current_repo() {
                    if self.selected_agent + 1 < repo.agents.len() {
                        self.selected_agent += 1;
                    }
                }
            }
            ActivePane::Preview => {}
        }
    }

    fn navigate_left(&mut self) {
        self.active_pane = match self.active_pane {
            ActivePane::Preview => ActivePane::AgentList,
            ActivePane::AgentList => ActivePane::Sidebar,
            ActivePane::Sidebar => ActivePane::Sidebar,
        };
    }

    fn navigate_right(&mut self) {
        self.active_pane = match self.active_pane {
            ActivePane::Sidebar => ActivePane::AgentList,
            ActivePane::AgentList => ActivePane::Preview,
            ActivePane::Preview => ActivePane::Preview,
        };
    }

    fn handle_select(&mut self) {
        // Handle modal confirmations first
        if let ModalState::ConfirmDeleteRepo(idx) = self.modal {
            self.confirm_delete_repository(idx);
            return;
        }

        if let ModalState::ConfirmDeleteAgent { repo_idx, agent_idx, delete_work_dir } = self.modal {
            self.confirm_delete_agent(repo_idx, agent_idx, delete_work_dir);
            return;
        }
        
        if self.screen == Screen::Split {
            match self.split.focus {
                SplitFocus::Repos => {
                    // Apply repo filter from cursor position.
                    if self.split.repo_cursor == 0 {
                        self.split.repo_filter = None; // "All"
                    } else {
                        self.split.repo_filter = Some(self.split.repo_cursor - 1);
                    }
                    // Reset agent cursor and ungrab when filter changes.
                    self.split.selected_row = 0;
                    self.split.grabbed = false;
                }
                SplitFocus::Agents => {
                    // Toggle grab on the highlighted agent.
                    self.split.grabbed = !self.split.grabbed;
                }
            }
            return;
        }

        match self.screen {
            Screen::Dashboard => {
                match self.active_pane {
                    ActivePane::Sidebar => {
                        if self.current_repo().is_some() {
                            self.open_edit_repository();
                        }
                    }
                    ActivePane::AgentList | ActivePane::Preview => {
                        if self.current_agent().is_some() {
                            self.open_edit_agent();
                        }
                    }
                }
            }
            Screen::CommandPalette => {}
            Screen::NewAgent => {
                self.screen = Screen::Dashboard;
            }
            Screen::NewRepository => {
                self.screen = Screen::Dashboard;
            }
            Screen::EditAgent => {
                self.submit_form();
            }
            Screen::EditRepository => {
                self.submit_form();
            }
            Screen::Split => {}
        }
    }

    fn handle_back(&mut self) {
        match self.screen {
            Screen::CommandPalette => {
                self.screen = Screen::Dashboard;
                self.is_searching = false;
            }
            Screen::NewAgent => {
                self.screen = Screen::Dashboard;
            }
            Screen::NewRepository => {
                self.screen = Screen::Dashboard;
            }
            Screen::EditAgent => {
                self.screen = Screen::Dashboard;
            }
            Screen::EditRepository => {
                self.screen = Screen::Dashboard;
            }
            Screen::Split => {
                if self.split.grabbed {
                    // Esc while grabbed: ungrab but stay in split.
                    self.split.grabbed = false;
                } else if self.split.focus == SplitFocus::Agents {
                    // Esc while in agent pane: go back to repo pane.
                    self.split.focus = SplitFocus::Repos;
                } else {
                    // Esc from repo pane: exit split, sync selection, no terminal focus.
                    self.sync_selection_from_split();
                    self.screen = Screen::Dashboard;
                    self.terminal_focused = false;
                    self.split.grabbed = false;
                }
            }
            Screen::Dashboard => {
                if matches!(self.modal, ModalState::ConfirmDeleteRepo(_) | ModalState::ConfirmDeleteAgent { .. } | ModalState::Help) {
                    self.modal = ModalState::None;
                }
            }
        }
    }

    fn open_new_agent(&mut self) {
        let repo = self.current_repo();
        // Repositories are allowed to keep an empty default profile (meaning
        // "use llxprt defaults"). Agents inherit that value as-is.
        let default_profile = repo.map_or_else(String::new, |r| r.default_profile.clone());
        let repo_base = repo.map_or_else(|| "/tmp".to_owned(), |r| r.base_dir.clone());
        self.new_agent_fields = vec![
            String::new(),   // 0: name
            String::new(),   // 1: description
            repo_base,       // 2: work_dir (starts as repo base, updates as you type name)
            default_profile, // 3: profile (inherited from repo, may be empty)
            "--yolo".into(), // 4: mode
        ];
        self.new_agent_pass_continue = true;
        self.new_agent_focus = 0;
        self.new_agent_workdir_manual = false;
        self.screen = Screen::NewAgent;
    }

    fn open_new_repository(&mut self) {
        self.new_repository_fields = vec![
            String::new(),           // 0: name
            String::new(),           // 1: base_dir
            String::new(),           // 2: default_profile (empty means use llxprt defaults)
        ];
        self.new_repository_focus = 0;
        self.screen = Screen::NewRepository;
    }

    fn open_edit_agent(&mut self) {
        let Some(agent) = self.current_agent() else { return };
        let name = agent.name.clone();
        let description = agent.description.clone();
        let work_dir = agent.work_dir.clone();
        let profile = agent.profile.clone();
        let mode_no_continue = mode_without_continue(&agent.mode);
        let pass_continue = mode_has_continue(&agent.mode);

        self.new_agent_fields = vec![
            name,
            description,
            work_dir,
            profile,
            mode_no_continue,
        ];
        self.new_agent_pass_continue = pass_continue;
        self.new_agent_focus = 0;
        self.new_agent_workdir_manual = true;
        self.screen = Screen::EditAgent;
    }

    fn open_edit_repository(&mut self) {
        let Some(repo) = self.current_repo() else { return };
        self.new_repository_fields = vec![
            repo.name.clone(),
            repo.base_dir.clone(),
            repo.default_profile.clone(),
        ];
        self.new_repository_focus = 0;
        self.screen = Screen::EditRepository;
    }

    fn delete_current_agent(&mut self) {
        match self.active_pane {
            ActivePane::Sidebar => {
                if !self.repositories.is_empty() {
                    self.modal = ModalState::ConfirmDeleteRepo(self.selected_repo);
                }
            }
            ActivePane::AgentList | ActivePane::Preview => {
                if let Some(repo) = self.repositories.get(self.selected_repo) {
                    if self.selected_agent < repo.agents.len() {
                        self.modal = ModalState::ConfirmDeleteAgent {
                            repo_idx: self.selected_repo,
                            agent_idx: self.selected_agent,
                            delete_work_dir: true,
                        };
                    }
                }
            }
        }
    }

    fn confirm_delete_agent(&mut self, repo_idx: usize, agent_idx: usize, delete_work_dir: bool) {
        if let Some(repo) = self.repositories.get_mut(repo_idx) {
            if agent_idx < repo.agents.len() {
                let agent = repo.agents.remove(agent_idx);
                if delete_work_dir {
                    if let Err(e) = std::fs::remove_dir_all(&agent.work_dir) {
                        eprintln!("Warning: failed to remove work dir {}: {}", agent.work_dir, e);
                    }
                }
                if self.selected_agent > 0 && self.selected_agent >= repo.agents.len() {
                    self.selected_agent = repo.agents.len().saturating_sub(1);
                }
            }
        }
        self.modal = ModalState::None;
    }

    /// Return PTY slot for an agent at explicit repo/agent indices.
    pub fn agent_pty_slot(&self, repo_idx: usize, agent_idx: usize) -> Option<usize> {
        self.repositories
            .get(repo_idx)
            .and_then(|repo| repo.agents.get(agent_idx))
            .and_then(|agent| agent.pty_slot)
    }

    fn toggle_search(&mut self) {
        self.is_searching = !self.is_searching;
        if self.is_searching {
            self.screen = Screen::CommandPalette;
        } else {
            self.screen = Screen::Dashboard;
            self.search_query.clear();
        }
    }

    fn show_help(&mut self) {
        if self.modal == ModalState::Help {
            self.modal = ModalState::None;
        } else {
            self.modal = ModalState::Help;
        }
    }

    fn focus_repository(&mut self) {
        if self.screen == Screen::Split {
            self.split.focus = SplitFocus::Repos;
            self.split.grabbed = false;
        } else {
            self.active_pane = ActivePane::Sidebar;
            self.terminal_focused = false;
        }
    }

    fn focus_agent_list(&mut self) {
        if self.screen == Screen::Split {
            self.split.focus = SplitFocus::Agents;
            let filtered_len = self.filtered_running_positions().len();
            if filtered_len == 0 {
                self.split.selected_row = 0;
            } else if self.split.selected_row >= filtered_len {
                self.split.selected_row = filtered_len - 1;
            }
        } else {
            self.active_pane = ActivePane::AgentList;
            self.terminal_focused = false;
        }
    }

    fn focus_terminal(&mut self) {
        self.screen = Screen::Dashboard;
        self.terminal_focused = true;
    }

    fn toggle_split_mode(&mut self) {
        if self.screen == Screen::Split {
            self.screen = Screen::Dashboard;
            self.split.grabbed = false;
            return;
        }

        self.screen = Screen::Split;
        self.terminal_focused = false;
        self.split.focus = SplitFocus::Repos;
        self.split.grabbed = false;
        self.split.repo_filter = None;
        self.split.repo_cursor = 0;
        let running = self.filtered_running_positions();
        if running.is_empty() {
            self.split.selected_row = 0;
        } else {
            self.split.selected_row = self.split.selected_row.min(running.len() - 1);
        }
    }

    fn kill_current_agent(&mut self) {
        if let Some(agent) = self.current_agent_mut() {
            agent.status = AgentStatus::Dead;
            agent.recent_output.push(OutputLine {
                kind: OutputKind::Text,
                content: "[jefe] PTY terminated".to_owned(),
                tool_status: None,
            });
        }
    }

    fn relaunch_current_agent(&mut self) {
        let is_dead = self
            .current_agent()
            .is_some_and(|agent| agent.status == AgentStatus::Dead);
        if !is_dead {
            return;
        }

        if let Some(agent) = self.current_agent_mut() {
            agent.status = AgentStatus::Running;
            agent.started_at = Utc::now();
            agent.elapsed_secs = 0;
            agent.recent_output.push(OutputLine {
                kind: OutputKind::Text,
                content: "[jefe] PTY relaunched".to_owned(),
                tool_status: None,
            });
        }
    }

    fn return_to_main_focused(&mut self) {
        if self.screen == Screen::Split {
            self.sync_selection_from_split();
            self.screen = Screen::Dashboard;
            self.terminal_focused = true;
            self.split.grabbed = false;
        }
    }

    fn toggle_terminal_focus(&mut self) {
        self.terminal_focused = !self.terminal_focused;
        if self.terminal_focused {
            self.screen = Screen::Dashboard;
        }
    }

    fn update_agent_workdir_from_name(&mut self) {
        let repo_base = self.current_repo()
            .map_or_else(|| "/tmp".to_owned(), |r| r.base_dir.clone());
        let repo_base = expand_tilde(&repo_base);
        let name = self.new_agent_fields.get(0).map_or("", String::as_str);
        let slug = name
            .to_lowercase()
            .replace(' ', "-")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '/')
            .collect::<String>();
        let work_dir = if slug.is_empty() {
            repo_base
        } else {
            format!("{}/{}", repo_base.trim_end_matches('/'), slug)
        };
        if let Some(field) = self.new_agent_fields.get_mut(2) {
            *field = work_dir;
        }
    }


    fn handle_char(&mut self, c: char) {
        // Toggle delete_work_dir in ConfirmDeleteAgent modal with space or 'd'
        if matches!(self.modal, ModalState::ConfirmDeleteAgent { .. }) && (c == ' ' || c == 'd') {
            self.toggle_delete_work_dir();
            return;
        }

        if self.is_searching {
            self.search_query.push(c);
        } else if self.screen == Screen::NewAgent || self.screen == Screen::EditAgent {
            if self.new_agent_focus == 5 {
                if c == ' ' {
                    self.new_agent_pass_continue = !self.new_agent_pass_continue;
                }
                return;
            }

            if let Some(field) = self.new_agent_fields.get_mut(self.new_agent_focus) {
                field.push(c);
            }
            // Auto-update work_dir from name if not manually edited
            if self.new_agent_focus == 0 && !self.new_agent_workdir_manual {
                self.update_agent_workdir_from_name();
            } else if self.new_agent_focus == 2 {
                self.new_agent_workdir_manual = true;
            }
        } else if self.screen == Screen::NewRepository || self.screen == Screen::EditRepository {
            if let Some(field) = self.new_repository_fields.get_mut(self.new_repository_focus) {
                field.push(c);
            }
        }
    }


    /// Swap two rows in the filtered running agent list.
    fn swap_filtered_agents(&mut self, row_a: usize, row_b: usize) {
        let filtered = self.filtered_running_positions();
        if row_a >= filtered.len() || row_b >= filtered.len() {
            return;
        }
        let (repo_a, agent_a) = filtered[row_a];
        let (repo_b, agent_b) = filtered[row_b];

        if repo_a == repo_b {
            if let Some(repo) = self.repositories.get_mut(repo_a) {
                repo.agents.swap(agent_a, agent_b);
            }
            return;
        }

        if repo_a < repo_b {
            let (left, right) = self.repositories.split_at_mut(repo_b);
            std::mem::swap(
                &mut left[repo_a].agents[agent_a],
                &mut right[0].agents[agent_b],
            );
        } else {
            let (left, right) = self.repositories.split_at_mut(repo_a);
            std::mem::swap(
                &mut left[repo_b].agents[agent_b],
                &mut right[0].agents[agent_a],
            );
        }
    }

    fn sync_selection_from_split(&mut self) {
        let filtered = self.filtered_running_positions();
        if filtered.is_empty() {
            return;
        }
        let idx = self.split.selected_row.min(filtered.len() - 1);
        let (repo_idx, agent_idx) = filtered[idx];
        self.selected_repo = repo_idx;
        self.selected_agent = agent_idx;
        self.active_pane = ActivePane::AgentList;
    }

    fn next_field(&mut self) {
        match self.screen {
            Screen::NewAgent | Screen::EditAgent => {
                // 5 text fields + 1 checkbox at index 5.
                const NEW_AGENT_FIELD_COUNT: usize = 6;
                self.new_agent_focus = (self.new_agent_focus + 1) % NEW_AGENT_FIELD_COUNT;
            }
            Screen::NewRepository | Screen::EditRepository => {
                self.new_repository_focus = (self.new_repository_focus + 1) % self.new_repository_fields.len();
            }
            _ => {}
        }
    }

    fn prev_field(&mut self) {
        match self.screen {
            Screen::NewAgent | Screen::EditAgent => {
                // 5 text fields + 1 checkbox at index 5.
                const NEW_AGENT_FIELD_COUNT: usize = 6;
                if self.new_agent_focus == 0 {
                    self.new_agent_focus = NEW_AGENT_FIELD_COUNT - 1;
                } else {
                    self.new_agent_focus -= 1;
                }
            }
            Screen::NewRepository | Screen::EditRepository => {
                if self.new_repository_focus == 0 {
                    self.new_repository_focus = self.new_repository_fields.len() - 1;
                } else {
                    self.new_repository_focus -= 1;
                }
            }
            _ => {}
        }
    }

    fn handle_backspace(&mut self) {
        match self.screen {
            Screen::NewAgent | Screen::EditAgent => {
                if self.new_agent_focus == 5 {
                    return;
                }

                if let Some(field) = self.new_agent_fields.get_mut(self.new_agent_focus) {
                    field.pop();
                }
                // Auto-update work_dir from name if not manually edited
                if self.new_agent_focus == 0 && !self.new_agent_workdir_manual {
                    self.update_agent_workdir_from_name();
                } else if self.new_agent_focus == 2 {
                    self.new_agent_workdir_manual = true;
                }
            }
            Screen::NewRepository | Screen::EditRepository => {
                if let Some(field) = self.new_repository_fields.get_mut(self.new_repository_focus) {
                    field.pop();
                }
            }
            _ => {
                if self.is_searching {
                    self.search_query.pop();
                }
            }
        }
    }

    fn submit_form(&mut self) {
        use crate::data::{Agent, AgentStatus};
        
        match self.screen {
            Screen::NewAgent => {
                let name = self.new_agent_fields.get(0).cloned().unwrap_or_default();
                let description = self.new_agent_fields.get(1).cloned().unwrap_or_default();
                let work_dir_input = self.new_agent_fields.get(2).cloned().unwrap_or_default();
                let profile = normalize_profile_input(
                    self.new_agent_fields.get(3).cloned().unwrap_or_default(),
                );
                let mode = compose_mode(
                    self.new_agent_fields
                        .get(4)
                        .cloned()
                        .unwrap_or_else(|| "--yolo".into()),
                    self.new_agent_pass_continue,
                );

                if name.is_empty() {
                    return; // Don't submit empty
                }

                let repo_base = self.current_repo().map_or_else(
                    || "/tmp".to_owned(),
                    |r| r.base_dir.clone(),
                );

                let work_dir = expand_tilde(&if work_dir_input.is_empty() {
                    crate::data::models::agent_work_dir(&repo_base, &name)
                } else {
                    work_dir_input
                });

                // Create directory on disk
                if let Err(e) = std::fs::create_dir_all(&work_dir) {
                    eprintln!("Warning: failed to create work dir {}: {}", work_dir, e);
                }

                let agent = Agent {
                    id: uuid::Uuid::new_v4(),
                    display_id: format!("#{}", self.agent_count() + 1),
                    name,
                    description,
                    work_dir,
                    profile,
                    mode,
                    pty_slot: None,
                    status: AgentStatus::Running,
                    started_at: Utc::now(),
                    token_in: 0,
                    token_out: 0,
                    cost_usd: 0.0,
                    todos: vec![],
                    recent_output: vec![],
                    elapsed_secs: 0,
                };

                if let Some(repo) = self.repositories.get_mut(self.selected_repo) {
                    repo.agents.push(agent);
                    self.selected_agent = repo.agents.len() - 1;
                }
                self.screen = Screen::Dashboard;
            }
            Screen::EditAgent => {
                let name = self.new_agent_fields.get(0).cloned().unwrap_or_default();
                let description = self.new_agent_fields.get(1).cloned().unwrap_or_default();
                let work_dir = expand_tilde(&self.new_agent_fields.get(2).cloned().unwrap_or_default());
                let profile = normalize_profile_input(
                    self.new_agent_fields.get(3).cloned().unwrap_or_default(),
                );
                let mode = compose_mode(
                    self.new_agent_fields
                        .get(4)
                        .cloned()
                        .unwrap_or_else(|| "--yolo".into()),
                    self.new_agent_pass_continue,
                );

                if name.is_empty() {
                    return;
                }

                if let Some(repo) = self.repositories.get_mut(self.selected_repo) {
                    if let Some(agent) = repo.agents.get_mut(self.selected_agent) {
                        agent.name = name;
                        agent.description = description;
                        if !work_dir.is_empty() {
                            // Create new dir if it changed
                            if work_dir != agent.work_dir {
                                if let Err(e) = std::fs::create_dir_all(&work_dir) {
                                    eprintln!("Warning: failed to create work dir {}: {}", work_dir, e);
                                }
                            }
                            agent.work_dir = work_dir;
                        }
                        agent.profile = profile;
                        agent.mode = mode;
                    }
                }
                self.screen = Screen::Dashboard;
            }
            Screen::NewRepository => {
                let name = self.new_repository_fields.get(0).cloned().unwrap_or_default();
                let base_dir = self.new_repository_fields.get(1).cloned().unwrap_or_default();
                let default_profile = normalize_profile_input(
                    self.new_repository_fields.get(2).cloned().unwrap_or_default(),
                );

                if name.is_empty() {
                    return; // Don't submit empty
                }

                let slug = name.to_lowercase().replace(' ', "-");
                let actual_base = expand_tilde(&if base_dir.is_empty() {
                    format!("/tmp/{}", slug)
                } else {
                    base_dir
                });

                // Create base dir on disk
                if let Err(e) = std::fs::create_dir_all(&actual_base) {
                    eprintln!("Warning: failed to create base dir {}: {}", actual_base, e);
                }

                let repo = Repository {
                    name: name.clone(),
                    slug,
                    base_dir: actual_base,
                    default_profile,
                    agents: vec![],
                };

                self.repositories.push(repo);
                self.selected_repo = self.repositories.len() - 1;
                self.selected_agent = 0;
                self.screen = Screen::Dashboard;
            }
            Screen::EditRepository => {
                let name = self.new_repository_fields.get(0).cloned().unwrap_or_default();
                let base_dir = self.new_repository_fields.get(1).cloned().unwrap_or_default();
                let default_profile = normalize_profile_input(
                    self.new_repository_fields.get(2).cloned().unwrap_or_default(),
                );

                if name.is_empty() {
                    return;
                }

                if let Some(repo) = self.repositories.get_mut(self.selected_repo) {
                    repo.name = name.clone();
                    repo.slug = name.to_lowercase().replace(' ', "-");
                    repo.default_profile = default_profile;
                    if !base_dir.is_empty() {
                        repo.base_dir = base_dir;
                    }
                }
                self.screen = Screen::Dashboard;
            }
            _ => {}
        }
    }


    fn confirm_delete_repository(&mut self, idx: usize) {
        if idx < self.repositories.len() {
            self.repositories.remove(idx);
            if self.selected_repo >= self.repositories.len() && !self.repositories.is_empty() {
                self.selected_repo = self.repositories.len() - 1;
            }
            if self.repositories.is_empty() {
                self.selected_repo = 0;
            }
            self.selected_agent = 0;
        }
        self.modal = ModalState::None;
    }

}

#[cfg(test)]
mod mode_tests {
    use super::{compose_mode, mode_has_continue, mode_without_continue};

    #[test]
    fn compose_mode_defaults_to_yolo_and_continue_for_empty_input() {
        assert_eq!(compose_mode(String::new(), true), "--yolo --continue");
    }

    #[test]
    fn compose_mode_preserves_mode_and_adds_continue_when_enabled() {
        assert_eq!(
            compose_mode("--yolo".to_owned(), true),
            "--yolo --continue",
        );
        assert_eq!(
            compose_mode("--auto-approve".to_owned(), true),
            "--auto-approve --continue",
        );
    }

    #[test]
    fn compose_mode_removes_continue_when_disabled() {
        assert_eq!(
            compose_mode("--yolo --continue".to_owned(), false),
            "--yolo",
        );
    }

    #[test]
    fn continue_helpers_detect_and_strip() {
        assert!(mode_has_continue("--yolo --continue"));
        assert!(!mode_has_continue("--yolo"));
        assert_eq!(mode_without_continue("--yolo --continue"), "--yolo");
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::SplitFocus;
    use crate::data::mock::generate_mock_data;

    #[test]
    fn test_app_state_creation() {
        let repositories = generate_mock_data();
        let state = AppState::new(repositories);

        assert_eq!(state.selected_repo, 0);
        assert_eq!(state.selected_agent, 0);
        assert_eq!(state.active_pane, ActivePane::Sidebar);
        assert_eq!(state.screen, Screen::Dashboard);
    }

    #[test]
    fn test_focus_shortcuts() {
        let repositories = generate_mock_data();
        let mut state = AppState::new(repositories);

        state.handle_event(AppEvent::FocusRepository);
        assert_eq!(state.active_pane, ActivePane::Sidebar);

        state.handle_event(AppEvent::FocusAgentList);
        assert_eq!(state.active_pane, ActivePane::AgentList);

        state.handle_event(AppEvent::FocusTerminal);
        assert!(state.terminal_focused);
    }

    #[test]
    fn test_split_mode_navigate_and_grab_flow() {
        let repositories = generate_mock_data();
        let mut state = AppState::new(repositories);

        state.handle_event(AppEvent::ToggleSplitMode);
        assert_eq!(state.screen, Screen::Split);
        assert_eq!(state.split.focus, SplitFocus::Repos);

        // Focus agent list.
        state.handle_event(AppEvent::FocusAgentList);
        assert_eq!(state.split.focus, SplitFocus::Agents);
        assert!(!state.split.grabbed);

        // Navigate down (cursor moves, no grab).
        let row_before = state.split.selected_row;
        state.handle_event(AppEvent::NavigateDown);
        // Should move if there are multiple filtered running agents.
        let filtered = state.filtered_running_positions();
        if filtered.len() > 1 {
            assert_eq!(state.split.selected_row, row_before + 1);
        }

        // Enter grabs.
        state.handle_event(AppEvent::Select);
        assert!(state.split.grabbed);

        // Enter again ungrabs.
        state.handle_event(AppEvent::Select);
        assert!(!state.split.grabbed);

        // Esc from agents goes to repos.
        state.handle_event(AppEvent::Back);
        assert_eq!(state.split.focus, SplitFocus::Repos);

        // Esc from repos exits split.
        state.handle_event(AppEvent::Back);
        assert_eq!(state.screen, Screen::Dashboard);
        assert!(!state.terminal_focused);
    }

    #[test]
    fn test_split_mode_repo_filter() {
        let repositories = generate_mock_data();
        let mut state = AppState::new(repositories);

        state.handle_event(AppEvent::ToggleSplitMode);
        assert!(state.split.repo_filter.is_none()); // "All" by default

        // Move cursor to first real repo (index 1) and select.
        state.handle_event(AppEvent::NavigateDown);
        assert_eq!(state.split.repo_cursor, 1);
        state.handle_event(AppEvent::Select);
        assert_eq!(state.split.repo_filter, Some(0));

        // Move back to "All" and select.
        state.handle_event(AppEvent::NavigateUp);
        state.handle_event(AppEvent::Select);
        assert!(state.split.repo_filter.is_none());
    }

    #[test]
    fn test_split_mode_m_to_main_with_terminal_focus() {
        let repositories = generate_mock_data();
        let mut state = AppState::new(repositories);

        state.handle_event(AppEvent::ToggleSplitMode);
        state.handle_event(AppEvent::FocusAgentList);
        state.handle_event(AppEvent::ReturnToMainFocused);

        assert_eq!(state.screen, Screen::Dashboard);
        assert!(state.terminal_focused);
        assert!(!state.split.grabbed);
    }

    #[test]
    fn test_kill_and_relaunch() {
        let repositories = generate_mock_data();
        let mut state = AppState::new(repositories);

        state.handle_event(AppEvent::KillAgent);
        assert_eq!(state.current_agent().map(|a| a.status), Some(AgentStatus::Dead));

        state.handle_event(AppEvent::RelaunchAgent);
        assert_eq!(state.current_agent().map(|a| a.status), Some(AgentStatus::Running));
    }

    #[test]
    fn test_delete_agent() {
        let repositories = generate_mock_data();
        let mut state = AppState::new(repositories);

        // Deleting from sidebar deletes repository; to delete agent, focus agent list.
        state.active_pane = ActivePane::AgentList;

        let initial_count = state.current_repo().map(|r| r.agents.len()).unwrap_or(0);
        state.handle_event(AppEvent::DeleteAgent);
        // Now the modal is shown, need to confirm
        assert!(matches!(state.modal, ModalState::ConfirmDeleteAgent { .. }));
        state.handle_event(AppEvent::Select);
        let final_count = state.current_repo().map(|r| r.agents.len()).unwrap_or(0);

        assert_eq!(final_count, initial_count - 1);
    }

    #[test]
    fn test_delete_repo_from_sidebar() {
        let repositories = generate_mock_data();
        let mut state = AppState::new(repositories);

        state.active_pane = ActivePane::Sidebar;
        let initial_repo_count = state.repositories.len();

        state.handle_event(AppEvent::DeleteAgent);
        assert!(matches!(state.modal, ModalState::ConfirmDeleteRepo(_)));
        state.handle_event(AppEvent::Select);

        assert_eq!(state.repositories.len(), initial_repo_count - 1);
    }

    #[test]
    fn test_running_count() {
        let repositories = generate_mock_data();
        let state = AppState::new(repositories);
        assert!(state.running_count() > 0);
    }
}
