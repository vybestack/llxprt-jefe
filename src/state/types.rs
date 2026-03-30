//! State types: structs, enums, and field definitions.

use crate::domain::{AgentId, AgentStatus, LaunchSignature, RepositoryId};
use crate::runtime::PreflightIssue;

/// Form fields for creating/editing an agent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentFormFields {
    pub shortcut_slot: Option<u8>,
    pub name: String,
    pub description: String,
    pub work_dir: String,
    pub profile: String,
    pub mode: String,
    pub llxprt_debug: String,
    pub pass_continue: bool,
    pub sandbox_enabled: bool,
    pub sandbox_engine: String,
    pub sandbox_flags: String,
}

/// Cursor positions for editable agent form text fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentFormCursor {
    pub name: usize,
    pub description: usize,
    pub work_dir: usize,
    pub profile: usize,
    pub mode: usize,
    pub llxprt_debug: usize,
    pub sandbox_flags: usize,
}

/// Which field is focused in the agent form.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AgentFormFocus {
    #[default]
    Shortcut,
    Name,
    Description,
    WorkDir,
    Profile,
    Mode,
    LlxprtDebug,
    PassContinue,
    Sandbox,
    SandboxEngine,
    SandboxFlags,
}

impl AgentFormFocus {
    /// Move to next field.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Shortcut => Self::Name,
            Self::Name => Self::Description,
            Self::Description => Self::WorkDir,
            Self::WorkDir => Self::Profile,
            Self::Profile => Self::Mode,
            Self::Mode => Self::LlxprtDebug,
            Self::LlxprtDebug => Self::PassContinue,
            Self::PassContinue => Self::Sandbox,
            Self::Sandbox => Self::SandboxEngine,
            Self::SandboxEngine => Self::SandboxFlags,
            Self::SandboxFlags => Self::Shortcut,
        }
    }

    /// Move to previous field.
    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Self::Shortcut => Self::SandboxFlags,
            Self::Name => Self::Shortcut,
            Self::Description => Self::Name,
            Self::WorkDir => Self::Description,
            Self::Profile => Self::WorkDir,
            Self::Mode => Self::Profile,
            Self::LlxprtDebug => Self::Mode,
            Self::PassContinue => Self::LlxprtDebug,
            Self::Sandbox => Self::PassContinue,
            Self::SandboxEngine => Self::Sandbox,
            Self::SandboxFlags => Self::SandboxEngine,
        }
    }
}

/// Form fields for creating/editing a repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepositoryFormFields {
    pub name: String,
    pub base_dir: String,
    pub default_profile: String,
    pub remote_enabled: bool,
    pub login_user: String,
    pub host: String,
    pub run_as_user: String,
    pub setup_env_default: bool,
}

/// Cursor positions for repository form text fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepositoryFormCursor {
    pub name: usize,
    pub base_dir: usize,
    pub default_profile: usize,
    pub login_user: usize,
    pub host: usize,
    pub run_as_user: usize,
}

/// Which field is focused in the repository form.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RepositoryFormFocus {
    #[default]
    Name,
    BaseDir,
    DefaultProfile,
    RemoteEnabled,
    LoginUser,
    Host,
    RunAsUser,
    SetupEnvDefault,
}

impl RepositoryFormFocus {
    /// Move to next field.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Name => Self::BaseDir,
            Self::BaseDir => Self::DefaultProfile,
            Self::DefaultProfile => Self::RemoteEnabled,
            Self::RemoteEnabled => Self::LoginUser,
            Self::LoginUser => Self::Host,
            Self::Host => Self::RunAsUser,
            Self::RunAsUser => Self::SetupEnvDefault,
            Self::SetupEnvDefault => Self::Name,
        }
    }

    /// Move to previous field.
    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Self::Name => Self::SetupEnvDefault,
            Self::BaseDir => Self::Name,
            Self::DefaultProfile => Self::BaseDir,
            Self::RemoteEnabled => Self::DefaultProfile,
            Self::LoginUser => Self::RemoteEnabled,
            Self::Host => Self::LoginUser,
            Self::RunAsUser => Self::Host,
            Self::SetupEnvDefault => Self::RunAsUser,
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
        cursor: RepositoryFormCursor,
    },
    EditRepository {
        id: RepositoryId,
        fields: RepositoryFormFields,
        focus: RepositoryFormFocus,
        cursor: RepositoryFormCursor,
    },
    ConfirmDeleteRepository {
        id: RepositoryId,
    },
    NewAgent {
        repository_id: RepositoryId,
        fields: AgentFormFields,
        focus: AgentFormFocus,
        cursor: AgentFormCursor,
        /// Track if work_dir was manually edited (stop auto-deriving from name).
        work_dir_manual: bool,
    },
    EditAgent {
        id: AgentId,
        fields: AgentFormFields,
        focus: AgentFormFocus,
        cursor: AgentFormCursor,
    },
    ConfirmDeleteAgent {
        id: AgentId,
        delete_work_dir: bool,
    },
    ConfirmKillAgent {
        id: AgentId,
    },
    /// Preflight check failed — prompt the user for remediation before launch.
    ///
    /// TODO(issue #24): Expand this to support a queue of issues if preflight
    /// transitions from single-issue checks to batched diagnostics.
    PreflightPrompt {
        /// The agent being launched (so we can resume after remediation).
        agent_id: AgentId,
        /// The launch signature (so we can resume the spawn).
        signature: LaunchSignature,
        /// The issue that was detected.
        issue: PreflightIssue,
        /// Placeholder for future multi-issue handling.
        remaining_issues: Vec<PreflightIssue>,
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
    pub repositories: Vec<crate::domain::Repository>,
    pub agents: Vec<crate::domain::Agent>,

    // Selection
    pub selected_repository_index: Option<usize>,
    pub selected_agent_index: Option<usize>,
    pub last_selected_agent_by_repo: Vec<(RepositoryId, AgentId)>,

    // View state
    pub screen_mode: ScreenMode,
    pub pane_focus: PaneFocus,
    pub terminal_focused: bool,
    pub hide_idle_repositories: bool,

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
    JumpToAgentByShortcut(u8),

    // Focus
    CyclePaneFocus,
    ToggleTerminalFocus,
    ToggleHideIdleRepositories,

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
    FormDelete,
    FormMoveCursorLeft,
    FormMoveCursorRight,
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
