//! Application state and event layer.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @requirement REQ-TECH-001
//! @requirement REQ-TECH-003
//!
//! Pseudocode reference: component-001 lines 01-12

use crate::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};

/// Form fields for creating/editing an agent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentFormFields {
    pub name: String,
    pub description: String,
    pub work_dir: String,
    pub profile: String,
    pub mode: String,
    pub pass_continue: bool,
}

/// Which field is focused in the agent form.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AgentFormFocus {
    #[default]
    Name,
    Description,
    WorkDir,
    Profile,
    Mode,
    PassContinue,
}

impl AgentFormFocus {
    /// Move to next field.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Name => Self::Description,
            Self::Description => Self::WorkDir,
            Self::WorkDir => Self::Profile,
            Self::Profile => Self::Mode,
            Self::Mode => Self::PassContinue,
            Self::PassContinue => Self::Name,
        }
    }

    /// Move to previous field.
    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Self::Name => Self::PassContinue,
            Self::Description => Self::Name,
            Self::WorkDir => Self::Description,
            Self::Profile => Self::WorkDir,
            Self::Mode => Self::Profile,
            Self::PassContinue => Self::Mode,
        }
    }
}

/// Form fields for creating/editing a repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepositoryFormFields {
    pub name: String,
    pub base_dir: String,
    pub default_profile: String,
}

/// Which field is focused in the repository form.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RepositoryFormFocus {
    #[default]
    Name,
    BaseDir,
    DefaultProfile,
}

impl RepositoryFormFocus {
    /// Move to next field.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Name => Self::BaseDir,
            Self::BaseDir => Self::DefaultProfile,
            Self::DefaultProfile => Self::Name,
        }
    }

    /// Move to previous field.
    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Self::Name => Self::DefaultProfile,
            Self::BaseDir => Self::Name,
            Self::DefaultProfile => Self::BaseDir,
        }
    }
}

/// Modal/form state variants.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ModalState {
    #[default]
    None,
    Help,
    Search {
        query: String,
    },
    NewRepository {
        fields: RepositoryFormFields,
        focus: RepositoryFormFocus,
    },
    EditRepository {
        id: RepositoryId,
        fields: RepositoryFormFields,
        focus: RepositoryFormFocus,
    },
    ConfirmDeleteRepository {
        id: RepositoryId,
    },
    NewAgent {
        repository_id: RepositoryId,
        fields: AgentFormFields,
        focus: AgentFormFocus,
        /// Track if work_dir was manually edited (stop auto-deriving from name).
        work_dir_manual: bool,
    },
    EditAgent {
        id: AgentId,
        fields: AgentFormFields,
        focus: AgentFormFocus,
    },
    ConfirmDeleteAgent {
        id: AgentId,
        delete_work_dir: bool,
    },
    ConfirmKillAgent {
        id: AgentId,
    },
}

/// Screen mode variants.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScreenMode {
    #[default]
    Dashboard,
    Split,
}

/// Pane focus within a view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PaneFocus {
    #[default]
    Repositories,
    Agents,
    Terminal,
}

/// Application state - single source of truth.
#[derive(Debug, Default, Clone)]
pub struct AppState {
    // Data
    pub repositories: Vec<Repository>,
    pub agents: Vec<Agent>,

    // Selection
    pub selected_repository_index: Option<usize>,
    pub selected_agent_index: Option<usize>,

    // View state
    pub screen_mode: ScreenMode,
    pub pane_focus: PaneFocus,
    pub terminal_focused: bool,

    // Modal/form state
    pub modal: ModalState,

    // Split mode state
    pub split_filter: Option<RepositoryId>,
    pub split_grab_index: Option<usize>,

    // Errors/warnings
    pub error_message: Option<String>,
    pub warning_message: Option<String>,
}

/// Application events for deterministic state transitions.
#[derive(Debug, Clone)]
pub enum AppEvent {
    // Navigation
    NavigateUp,
    NavigateDown,
    NavigateLeft,
    NavigateRight,
    SelectRepository(usize),
    SelectAgent(usize),

    // Focus
    CyclePaneFocus,
    ToggleTerminalFocus,

    // Screen mode
    EnterSplitMode,
    ExitSplitMode,

    // Grab mode (split view reordering)
    EnterGrabMode,
    ExitGrabMode,
    GrabMoveUp,
    GrabMoveDown,
    SetSplitFilter(Option<RepositoryId>),

    // Modal/form actions
    OpenHelp,
    OpenSearch,
    CloseModal,
    SubmitForm,

    // Form input events
    FormChar(char),
    FormBackspace,
    FormNextField,
    FormPrevField,
    FormToggleCheckbox,

    // CRUD
    OpenNewRepository,
    OpenEditRepository(RepositoryId),
    OpenDeleteRepository(RepositoryId),
    OpenNewAgent(RepositoryId),
    OpenEditAgent(AgentId),
    OpenDeleteAgent(AgentId),
    ToggleDeleteWorkDir,

    // Lifecycle
    KillAgent(AgentId),
    RelaunchAgent(AgentId),
    AgentStatusChanged(AgentId, AgentStatus),

    // Persistence results
    PersistenceLoadSuccess,
    PersistenceLoadFailed(String),
    PersistenceSaveSuccess,
    PersistenceSaveFailed(String),

    // Theme
    SetTheme(String),
    ThemeResolveFailed(String),

    // System
    Quit,
    ClearError,
    ClearWarning,
}

impl AppState {
    fn selected_repository_id(&self) -> Option<&RepositoryId> {
        self.selected_repository_index
            .and_then(|idx| self.repositories.get(idx).map(|repo| &repo.id))
    }

    fn agent_indices_for_repository(&self, repository_id: &RepositoryId) -> Vec<usize> {
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
    #[allow(clippy::too_many_lines)]
    pub fn apply(mut self, event: AppEvent) -> Self {
        // When terminal is focused, navigation events are forwarded to PTY
        // and should NOT change UI selection state.
        // However, CyclePaneFocus is allowed (user can switch panes even while F12 active).
        // @plan PLAN-20260216-FIRSTVERSION-V1.P11
        // @requirement REQ-FUNC-003 (F12 focus consistency)
        if self.terminal_focused {
            match &event {
                AppEvent::NavigateUp
                | AppEvent::NavigateDown
                | AppEvent::NavigateLeft
                | AppEvent::NavigateRight
                | AppEvent::SelectRepository(_)
                | AppEvent::SelectAgent(_) => {
                    // Navigation keys go to PTY, not UI. No state change.
                    return self;
                }
                // CyclePaneFocus is NOT blocked - user can switch panes
                // Other events (ToggleTerminalFocus, Quit, etc.) are also processed
                _ => {}
            }
        }

        match event {
            // Navigation
            AppEvent::NavigateUp => self.handle_navigate_up(),
            AppEvent::NavigateDown => self.handle_navigate_down(),
            AppEvent::SelectRepository(idx) => {
                if idx < self.repositories.len() {
                    self.selected_repository_index = Some(idx);
                }
            }
            AppEvent::SelectAgent(idx) => {
                if let Some(repository_id) = self.selected_repository_id().cloned() {
                    let visible_indices = self.agent_indices_for_repository(&repository_id);
                    if idx < visible_indices.len() {
                        self.selected_agent_index = Some(visible_indices[idx]);
                    }
                }
            }

            // Focus
            AppEvent::CyclePaneFocus | AppEvent::NavigateRight => {
                self.pane_focus = match self.pane_focus {
                    PaneFocus::Repositories => PaneFocus::Agents,
                    PaneFocus::Agents => PaneFocus::Terminal,
                    PaneFocus::Terminal => PaneFocus::Repositories,
                };
            }
            AppEvent::ToggleTerminalFocus => {
                self.terminal_focused = !self.terminal_focused;
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
                if let Some(idx) = self.selected_repository_index {
                    self.split_grab_index = Some(idx);
                }
            }
            AppEvent::ExitGrabMode => {
                self.split_grab_index = None;
            }
            AppEvent::GrabMoveUp => {
                if let Some(grab_idx) = self.split_grab_index
                    && grab_idx > 0
                    && grab_idx < self.repositories.len()
                {
                    self.repositories.swap(grab_idx, grab_idx - 1);
                    self.split_grab_index = Some(grab_idx - 1);
                    self.selected_repository_index = Some(grab_idx - 1);
                }
            }
            AppEvent::GrabMoveDown => {
                if let Some(grab_idx) = self.split_grab_index
                    && grab_idx + 1 < self.repositories.len()
                {
                    self.repositories.swap(grab_idx, grab_idx + 1);
                    self.split_grab_index = Some(grab_idx + 1);
                    self.selected_repository_index = Some(grab_idx + 1);
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
                };
            }
            AppEvent::OpenEditRepository(id) => {
                // Find the repository and populate form fields
                let fields = self
                    .repositories
                    .iter()
                    .find(|r| r.id == id)
                    .map(|r| RepositoryFormFields {
                        name: r.name.clone(),
                        base_dir: r.base_dir.to_string_lossy().into_owned(),
                        default_profile: r.default_profile.clone(),
                    })
                    .unwrap_or_default();
                self.modal = ModalState::EditRepository {
                    id,
                    fields,
                    focus: RepositoryFormFocus::default(),
                };
            }
            AppEvent::OpenDeleteRepository(id) => {
                self.modal = ModalState::ConfirmDeleteRepository { id };
            }
            AppEvent::OpenNewAgent(repository_id) => {
                // Get defaults from the repository
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

                self.modal = ModalState::NewAgent {
                    repository_id,
                    fields: AgentFormFields {
                        name: String::new(),
                        description: String::new(),
                        work_dir: base_dir,
                        profile: default_profile,
                        mode: "--yolo".to_owned(),
                        pass_continue: true,
                    },
                    focus: AgentFormFocus::default(),
                    work_dir_manual: false,
                };
            }
            AppEvent::OpenEditAgent(id) => {
                // Find the agent and populate form fields
                let fields = self
                    .agents
                    .iter()
                    .find(|a| a.id == id)
                    .map(|a| AgentFormFields {
                        name: a.name.clone(),
                        description: a.description.clone(),
                        work_dir: a.work_dir.to_string_lossy().into_owned(),
                        profile: a.profile.clone(),
                        mode: a.mode_flags.join(" "),
                        pass_continue: a.pass_continue,
                    })
                    .unwrap_or_default();
                self.modal = ModalState::EditAgent {
                    id,
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

            // Pane focus navigation (Left cycles backward; Right handled with CyclePaneFocus)
            AppEvent::NavigateLeft => {
                self.pane_focus = match self.pane_focus {
                    PaneFocus::Repositories => PaneFocus::Terminal,
                    PaneFocus::Agents => PaneFocus::Repositories,
                    PaneFocus::Terminal => PaneFocus::Agents,
                };
            }

            // No-op events (handled elsewhere or reserved)
            AppEvent::RelaunchAgent(_)
            | AppEvent::PersistenceSaveSuccess
            | AppEvent::SetTheme(_)
            | AppEvent::Quit => {}
        }

        self.rebuild_repository_agent_ids();
        self.normalize_selection_indices();
        self
    }

    fn handle_navigate_up(&mut self) {
        match self.pane_focus {
            PaneFocus::Repositories => {
                if let Some(idx) = self.selected_repository_index.filter(|&i| i > 0) {
                    self.selected_repository_index = Some(idx - 1);
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
                    }
                    Some(_) => {}
                    None => {
                        self.selected_agent_index = visible_indices.first().copied();
                    }
                }
            }
            PaneFocus::Terminal => {}
        }
    }

    fn handle_navigate_down(&mut self) {
        match self.pane_focus {
            PaneFocus::Repositories => {
                if let Some(idx) = self.selected_repository_index {
                    let max = self.repositories.len().saturating_sub(1);
                    if idx < max {
                        self.selected_repository_index = Some(idx + 1);
                    }
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
                    }
                    Some(_) => {}
                    None => {
                        self.selected_agent_index = visible_indices.first().copied();
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

    // Form handling methods

    fn handle_form_char(&mut self, c: char) {
        match &mut self.modal {
            ModalState::Search { query } => {
                query.push(c);
            }
            ModalState::NewRepository { fields, focus, .. }
            | ModalState::EditRepository { fields, focus, .. } => {
                let field = match focus {
                    RepositoryFormFocus::Name => &mut fields.name,
                    RepositoryFormFocus::BaseDir => &mut fields.base_dir,
                    RepositoryFormFocus::DefaultProfile => &mut fields.default_profile,
                };
                field.push(c);
            }
            ModalState::NewAgent {
                fields,
                focus,
                work_dir_manual,
                ..
            } => {
                match focus {
                    AgentFormFocus::Name => {
                        fields.name.push(c);
                        // Auto-update work_dir from name if not manually edited
                        if !*work_dir_manual {
                            self.update_agent_work_dir_from_name();
                        }
                    }
                    AgentFormFocus::Description => fields.description.push(c),
                    AgentFormFocus::WorkDir => {
                        fields.work_dir.push(c);
                        *work_dir_manual = true;
                    }
                    AgentFormFocus::Profile => fields.profile.push(c),
                    AgentFormFocus::Mode => fields.mode.push(c),
                    AgentFormFocus::PassContinue => {
                        // Space or 'x' toggles checkbox, ignore other chars
                        if c == ' ' || c == 'x' || c == 'X' {
                            fields.pass_continue = !fields.pass_continue;
                        }
                    }
                }
            }
            ModalState::EditAgent { fields, focus, .. } => {
                match focus {
                    AgentFormFocus::Name => fields.name.push(c),
                    AgentFormFocus::Description => fields.description.push(c),
                    AgentFormFocus::WorkDir => fields.work_dir.push(c),
                    AgentFormFocus::Profile => fields.profile.push(c),
                    AgentFormFocus::Mode => fields.mode.push(c),
                    AgentFormFocus::PassContinue => {
                        // Space or 'x' toggles checkbox, ignore other chars
                        if c == ' ' || c == 'x' || c == 'X' {
                            fields.pass_continue = !fields.pass_continue;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn pop_repository_field(fields: &mut RepositoryFormFields, focus: RepositoryFormFocus) {
        match focus {
            RepositoryFormFocus::Name => {
                fields.name.pop();
            }
            RepositoryFormFocus::BaseDir => {
                fields.base_dir.pop();
            }
            RepositoryFormFocus::DefaultProfile => {
                fields.default_profile.pop();
            }
        }
    }

    fn pop_agent_field(fields: &mut AgentFormFields, focus: AgentFormFocus) {
        match focus {
            AgentFormFocus::Name => {
                fields.name.pop();
            }
            AgentFormFocus::Description => {
                fields.description.pop();
            }
            AgentFormFocus::WorkDir => {
                fields.work_dir.pop();
            }
            AgentFormFocus::Profile => {
                fields.profile.pop();
            }
            AgentFormFocus::Mode => {
                fields.mode.pop();
            }
            AgentFormFocus::PassContinue => {}
        }
    }

    fn handle_form_backspace(&mut self) {
        let mut refresh_work_dir = false;

        match &mut self.modal {
            ModalState::Search { query } => {
                query.pop();
            }
            ModalState::NewRepository { fields, focus, .. }
            | ModalState::EditRepository { fields, focus, .. } => {
                Self::pop_repository_field(fields, *focus);
            }
            ModalState::NewAgent {
                fields,
                focus,
                work_dir_manual,
                ..
            } => {
                let focused = *focus;
                Self::pop_agent_field(fields, focused);
                if focused == AgentFormFocus::WorkDir {
                    *work_dir_manual = true;
                } else if focused == AgentFormFocus::Name && !*work_dir_manual {
                    refresh_work_dir = true;
                }
            }
            ModalState::EditAgent { fields, focus, .. } => {
                Self::pop_agent_field(fields, *focus);
            }
            _ => {}
        }

        if refresh_work_dir {
            self.update_agent_work_dir_from_name();
        }
    }

    fn handle_form_next_field(&mut self) {
        match &mut self.modal {
            ModalState::NewRepository { focus, .. } | ModalState::EditRepository { focus, .. } => {
                *focus = focus.next();
            }
            ModalState::NewAgent { focus, .. } | ModalState::EditAgent { focus, .. } => {
                *focus = focus.next();
            }
            _ => {}
        }
    }

    fn handle_form_prev_field(&mut self) {
        match &mut self.modal {
            ModalState::NewRepository { focus, .. } | ModalState::EditRepository { focus, .. } => {
                *focus = focus.prev();
            }
            ModalState::NewAgent { focus, .. } | ModalState::EditAgent { focus, .. } => {
                *focus = focus.prev();
            }
            _ => {}
        }
    }

    fn handle_form_toggle_checkbox(&mut self) {
        match &mut self.modal {
            ModalState::NewAgent { fields, focus, .. }
            | ModalState::EditAgent { fields, focus, .. } => {
                if *focus == AgentFormFocus::PassContinue {
                    fields.pass_continue = !fields.pass_continue;
                }
            }
            ModalState::ConfirmDeleteAgent {
                delete_work_dir, ..
            } => {
                *delete_work_dir = !*delete_work_dir;
            }
            _ => {}
        }
    }

    fn update_agent_work_dir_from_name(&mut self) {
        if let ModalState::NewAgent {
            repository_id,
            fields,
            work_dir_manual,
            ..
        } = &mut self.modal
        {
            if *work_dir_manual {
                return;
            }
            let base_dir = self
                .repositories
                .iter()
                .find(|r| r.id == *repository_id)
                .map_or_else(
                    || "/tmp".to_owned(),
                    |r| r.base_dir.to_string_lossy().into_owned(),
                );

            let slug = fields
                .name
                .to_lowercase()
                .replace(' ', "-")
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '/')
                .collect::<String>();

            fields.work_dir = if slug.is_empty() {
                base_dir
            } else {
                let base_dir = base_dir.trim_end_matches('/');
                format!("{base_dir}/{slug}")
            };
        }
    }

    fn create_repository_from_fields(fields: &RepositoryFormFields) -> Option<Repository> {
        if fields.name.is_empty() {
            return None;
        }

        let slug = fields
            .name
            .to_lowercase()
            .replace(' ', "-")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect::<String>();

        let base_dir = if fields.base_dir.is_empty() {
            format!("/tmp/{slug}")
        } else {
            expand_tilde(&fields.base_dir)
        };

        let _ = std::fs::create_dir_all(&base_dir);

        Some(Repository {
            id: RepositoryId(generate_id("repo")),
            name: fields.name.clone(),
            slug,
            base_dir: std::path::PathBuf::from(&base_dir),
            default_profile: normalize_profile(&fields.default_profile),
            agent_ids: Vec::new(),
        })
    }

    fn update_repository_from_fields(repo: &mut Repository, fields: &RepositoryFormFields) {
        repo.name.clone_from(&fields.name);
        repo.slug = fields
            .name
            .to_lowercase()
            .replace(' ', "-")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect();

        if !fields.base_dir.is_empty() {
            repo.base_dir = std::path::PathBuf::from(expand_tilde(&fields.base_dir));
        }

        repo.default_profile = normalize_profile(&fields.default_profile);
    }

    fn create_agent_from_fields(
        repository_id: &RepositoryId,
        fields: &AgentFormFields,
        next_display_index: usize,
    ) -> Option<Agent> {
        if fields.name.is_empty() {
            return None;
        }

        let work_dir = expand_tilde(&fields.work_dir);
        let _ = std::fs::create_dir_all(&work_dir);

        let mode_flags: Vec<String> = if fields.mode.trim().is_empty() {
            vec!["--yolo".to_owned()]
        } else {
            fields.mode.split_whitespace().map(String::from).collect()
        };

        Some(Agent {
            id: AgentId(generate_id("agent")),
            display_id: format!("#{next_display_index}"),
            repository_id: repository_id.clone(),
            name: fields.name.clone(),
            description: fields.description.clone(),
            work_dir: std::path::PathBuf::from(&work_dir),
            profile: normalize_profile(&fields.profile),
            mode_flags,
            pass_continue: fields.pass_continue,
            status: AgentStatus::Running,
            runtime_binding: None,
        })
    }

    fn update_agent_from_fields(agent: &mut Agent, fields: &AgentFormFields) {
        agent.name.clone_from(&fields.name);
        agent.description.clone_from(&fields.description);

        if !fields.work_dir.is_empty() {
            let new_dir = expand_tilde(&fields.work_dir);
            if new_dir != agent.work_dir.to_string_lossy() {
                let _ = std::fs::create_dir_all(&new_dir);
            }
            agent.work_dir = std::path::PathBuf::from(&new_dir);
        }

        agent.profile = normalize_profile(&fields.profile);
        agent.mode_flags = if fields.mode.trim().is_empty() {
            vec!["--yolo".to_owned()]
        } else {
            fields.mode.split_whitespace().map(String::from).collect()
        };
        agent.pass_continue = fields.pass_continue;
    }

    fn handle_submit_form(&mut self) {
        match &self.modal {
            ModalState::NewRepository { fields, .. } => {
                if let Some(repo) = Self::create_repository_from_fields(fields) {
                    self.repositories.push(repo);
                    self.selected_repository_index = Some(self.repositories.len() - 1);
                    self.modal = ModalState::None;
                }
            }
            ModalState::EditRepository { id, fields, .. } => {
                if fields.name.is_empty() {
                    return;
                }

                if let Some(repo) = self.repositories.iter_mut().find(|r| r.id == *id) {
                    Self::update_repository_from_fields(repo, fields);
                }
                self.modal = ModalState::None;
            }
            ModalState::NewAgent {
                repository_id,
                fields,
                ..
            } => {
                let next_display_index = self.agents.len() + 1;
                if let Some(agent) =
                    Self::create_agent_from_fields(repository_id, fields, next_display_index)
                {
                    self.agents.push(agent);
                    self.selected_agent_index = Some(self.agents.len() - 1);
                    self.modal = ModalState::None;
                }
            }
            ModalState::EditAgent { id, fields, .. } => {
                if fields.name.is_empty() {
                    return;
                }

                if let Some(agent) = self.agents.iter_mut().find(|a| a.id == *id) {
                    Self::update_agent_from_fields(agent, fields);
                }
                self.modal = ModalState::None;
            }
            _ => {
                self.modal = ModalState::None;
            }
        }
    }
}

/// Generate a unique ID with a prefix.
fn generate_id(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{timestamp:x}")
}

/// Expand `~` to home directory.
fn expand_tilde(path: &str) -> String {
    if (path == "~" || path.starts_with("~/"))
        && let Some(home) = std::env::var_os("HOME")
    {
        let home = home.to_string_lossy();
        return if path == "~" {
            home.into_owned()
        } else {
            format!("{home}{}", &path[1..])
        };
    }
    path.to_owned()
}

/// Normalize profile input - empty or "[]" means use defaults.
fn normalize_profile(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "[]" {
        String::new()
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_has_no_selection() {
        let state = AppState::default();
        assert!(state.selected_repository_index.is_none());
        assert!(state.selected_agent_index.is_none());
    }

    #[test]
    fn default_state_is_dashboard_mode() {
        let state = AppState::default();
        assert_eq!(state.screen_mode, ScreenMode::Dashboard);
    }

    #[test]
    fn default_state_terminal_unfocused() {
        let state = AppState::default();
        assert!(!state.terminal_focused);
    }
}
