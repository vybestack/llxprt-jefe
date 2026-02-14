//! Central application state management.
//!
//! This module provides the main `AppState` struct that manages
//! the entire application state, including projects, tasks, UI state,
//! and event handling.

use chrono::Utc;
use uuid::Uuid;

use crate::data::{Agent, AgentStatus, OutputKind, OutputLine, Repository, TodoItem, TodoStatus, ToolStatus};
use crate::events::AppEvent;

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
    /// Detailed agent view.
    AgentDetail,
    /// Command palette/search.
    CommandPalette,
    /// Terminal view for a running agent.
    Terminal,
    /// New agent form.
    NewAgent,
    /// New repository form.
    NewRepository,
    /// Split mode view for all running agents.
    Split,
}

/// Modal dialog state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ModalState {
    /// No modal is shown.
    #[default]
    None,
    /// Confirmation dialog for killing an agent.
    ConfirmKill(usize),
    /// Help/keyboard shortcuts dialog.
    Help,
}

/// State for split-mode reordering workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SplitState {
    /// Whether split-mode reorder is armed.
    pub reorder_armed: bool,
    /// Selected row in split mode (global running index order).
    pub selected_row: usize,
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
}

impl AppState {
    /// Creates a new `AppState` with the given repositories.
    #[must_use]
    pub const fn new(repositories: Vec<Repository>) -> Self {
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
                reorder_armed: false,
                selected_row: 0,
            },
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

    /// Returns the flat (repo_idx, agent_idx) tuples for all non-dead agents.
    #[must_use]
    pub fn active_agent_positions(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (ri, repo) in self.repositories.iter().enumerate() {
            for (ai, agent) in repo.agents.iter().enumerate() {
                if agent.status != AgentStatus::Dead {
                    out.push((ri, ai));
                }
            }
        }
        out
    }

    /// Returns the flat (global) index of the currently selected agent
    /// across all repositories. Used to index into the PTY session list.
    #[must_use]
    pub fn global_agent_index(&self) -> usize {
        let mut idx = 0;
        for (i, repo) in self.repositories.iter().enumerate() {
            if i == self.selected_repo {
                return idx + self.selected_agent;
            }
            idx += repo.agents.len();
        }
        idx
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
        }
    }

    fn navigate_up(&mut self) {
        if self.screen == Screen::Split && self.split.reorder_armed {
            let running = self.running_agent_positions();
            if running.is_empty() || self.split.selected_row >= running.len() {
                return;
            }
            if self.split.selected_row > 0 {
                self.swap_running_agents(self.split.selected_row, self.split.selected_row - 1);
                self.split.selected_row -= 1;
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
        if self.screen == Screen::Split && self.split.reorder_armed {
            let running = self.running_agent_positions();
            if running.is_empty() || self.split.selected_row >= running.len() {
                return;
            }
            if self.split.selected_row + 1 < running.len() {
                self.swap_running_agents(self.split.selected_row, self.split.selected_row + 1);
                self.split.selected_row += 1;
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
        if self.screen == Screen::Split {
            self.split.reorder_armed = !self.split.reorder_armed;
            return;
        }

        match self.screen {
            Screen::Dashboard => {
                if self.current_agent().is_some() {
                    self.screen = Screen::AgentDetail;
                }
            }
            Screen::AgentDetail => {}
            Screen::CommandPalette => {}
            Screen::Terminal => {}
            Screen::NewAgent => {
                self.screen = Screen::Dashboard;
            }
            Screen::NewRepository => {
                self.screen = Screen::Dashboard;
            }
            Screen::Split => {}
        }
    }

    fn handle_back(&mut self) {
        match self.screen {
            Screen::AgentDetail => {
                self.screen = Screen::Dashboard;
            }
            Screen::CommandPalette => {
                self.screen = Screen::Dashboard;
                self.is_searching = false;
            }
            Screen::Terminal => {
                self.screen = Screen::Dashboard;
                self.terminal_focused = false;
            }
            Screen::NewAgent => {
                self.screen = Screen::Dashboard;
            }
            Screen::NewRepository => {
                self.screen = Screen::Dashboard;
            }
            Screen::Split => {
                if self.split.reorder_armed {
                    // Esc with one selected in split mode: go main with selected agent, no terminal focus.
                    self.sync_selection_from_split();
                }
                self.screen = Screen::Dashboard;
                self.terminal_focused = false;
                self.split.reorder_armed = false;
            }
            Screen::Dashboard => {
                if self.modal != ModalState::None {
                    self.modal = ModalState::None;
                }
            }
        }
    }

    fn open_new_agent(&mut self) {
        self.screen = Screen::NewAgent;
    }

    fn open_new_repository(&mut self) {
        self.screen = Screen::NewRepository;
    }

    fn delete_current_agent(&mut self) {
        if let Some(repo) = self.repositories.get_mut(self.selected_repo) {
            if self.selected_agent < repo.agents.len() {
                repo.agents.remove(self.selected_agent);
                if self.selected_agent > 0 {
                    self.selected_agent -= 1;
                }
                if self.selected_agent >= repo.agents.len() && !repo.agents.is_empty() {
                    self.selected_agent = repo.agents.len() - 1;
                }
            }
        }
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
        self.modal = ModalState::Help;
    }

    fn focus_repository(&mut self) {
        self.active_pane = ActivePane::Sidebar;
        self.terminal_focused = false;
    }

    fn focus_agent_list(&mut self) {
        if self.screen == Screen::Split {
            self.split.reorder_armed = true;
            let running_len = self.running_agent_positions().len();
            if running_len == 0 {
                self.split.selected_row = 0;
            } else if self.split.selected_row >= running_len {
                self.split.selected_row = running_len - 1;
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
            self.split.reorder_armed = false;
            return;
        }

        self.screen = Screen::Split;
        self.terminal_focused = false;
        self.split.reorder_armed = false;
        let running = self.running_agent_positions();
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

        self.add_mock_repository_on_relaunch();
    }

    fn return_to_main_focused(&mut self) {
        if self.screen == Screen::Split {
            self.sync_selection_from_split();
            self.screen = Screen::Dashboard;
            self.terminal_focused = true;
            self.split.reorder_armed = false;
        }
    }

    fn toggle_terminal_focus(&mut self) {
        self.terminal_focused = !self.terminal_focused;
        if self.terminal_focused {
            self.screen = Screen::Dashboard;
        }
    }

    fn handle_char(&mut self, c: char) {
        if self.is_searching {
            self.search_query.push(c);
        }
    }

    fn swap_running_agents(&mut self, row_a: usize, row_b: usize) {
        let running = self.running_agent_positions();
        if row_a >= running.len() || row_b >= running.len() {
            return;
        }

        let (repo_a, agent_a) = running[row_a];
        let (repo_b, agent_b) = running[row_b];

        if repo_a == repo_b {
            if let Some(repo) = self.repositories.get_mut(repo_a) {
                repo.agents.swap(agent_a, agent_b);
            }
            return;
        }

        if repo_a < repo_b {
            let (left, right) = self.repositories.split_at_mut(repo_b);
            let repo_left = &mut left[repo_a];
            let repo_right = &mut right[0];
            std::mem::swap(&mut repo_left.agents[agent_a], &mut repo_right.agents[agent_b]);
        } else {
            let (left, right) = self.repositories.split_at_mut(repo_a);
            let repo_left = &mut left[repo_b];
            let repo_right = &mut right[0];
            std::mem::swap(&mut repo_left.agents[agent_b], &mut repo_right.agents[agent_a]);
        }
    }

    fn sync_selection_from_split(&mut self) {
        let running = self.running_agent_positions();
        if running.is_empty() {
            return;
        }
        let idx = self.split.selected_row.min(running.len() - 1);
        let (repo_idx, agent_idx) = running[idx];
        self.selected_repo = repo_idx;
        self.selected_agent = agent_idx;
        self.active_pane = ActivePane::AgentList;
    }

    fn add_mock_repository_on_relaunch(&mut self) {
        let idx = self.repositories.len() + 1;
        let name = format!("relaunched-repo-{idx}");
        let slug = format!("relaunched-repo-{idx}");
        self.repositories.push(Repository {
            name,
            slug,
            base_dir: format!("/tmp/relaunched-repo-{idx}"),
            agents: vec![Agent {
                id: Uuid::new_v4(),
                display_id: format!("#R{idx}"),
                purpose: "Relaunch smoke task".to_owned(),
                work_dir: format!("/tmp/relaunched-repo-{idx}/work"),
                model: "claude-opus-4-6".to_owned(),
                profile: "default".to_owned(),
                mode: "--yolo".to_owned(),
                status: AgentStatus::Queued,
                started_at: Utc::now(),
                token_in: 0,
                token_out: 0,
                cost_usd: 0.0,
                todos: vec![TodoItem {
                    content: "Warm boot relaunch task".to_owned(),
                    status: TodoStatus::Pending,
                }],
                recent_output: vec![OutputLine {
                    kind: OutputKind::Text,
                    content: "Created by relaunch action".to_owned(),
                    tool_status: Some(ToolStatus::Completed),
                }],
                elapsed_secs: 0,
            }],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn test_split_mode_enter_escape_flow() {
        let repositories = generate_mock_data();
        let mut state = AppState::new(repositories);

        state.handle_event(AppEvent::ToggleSplitMode);
        assert_eq!(state.screen, Screen::Split);

        state.handle_event(AppEvent::FocusAgentList);
        assert!(state.split.reorder_armed);

        state.handle_event(AppEvent::Select);
        assert!(!state.split.reorder_armed);

        state.handle_event(AppEvent::FocusAgentList);
        state.handle_event(AppEvent::Back);
        assert_eq!(state.screen, Screen::Dashboard);
        assert!(!state.terminal_focused);
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
    }

    #[test]
    fn test_kill_and_relaunch() {
        let repositories = generate_mock_data();
        let mut state = AppState::new(repositories);

        let initial_repo_count = state.repositories.len();

        state.handle_event(AppEvent::KillAgent);
        assert_eq!(state.current_agent().map(|a| a.status), Some(AgentStatus::Dead));

        state.handle_event(AppEvent::RelaunchAgent);
        assert_eq!(state.current_agent().map(|a| a.status), Some(AgentStatus::Running));
        assert!(state.repositories.len() > initial_repo_count);
    }

    #[test]
    fn test_delete_agent() {
        let repositories = generate_mock_data();
        let mut state = AppState::new(repositories);

        let initial_count = state.current_repo().map(|r| r.agents.len()).unwrap_or(0);
        state.handle_event(AppEvent::DeleteAgent);
        let final_count = state.current_repo().map(|r| r.agents.len()).unwrap_or(0);

        assert_eq!(final_count, initial_count - 1);
    }

    #[test]
    fn test_running_count() {
        let repositories = generate_mock_data();
        let state = AppState::new(repositories);
        assert!(state.running_count() > 0);
    }
}
