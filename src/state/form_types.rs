//! Agent and repository form-field types extracted from types.rs.
//!
//! These types are self-contained (no cross-dependencies on other state types)
//! and were extracted to keep types.rs under the source file length limit.

/// Form fields for creating/editing an agent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentFormFields {
    pub shortcut_slot: Option<u8>,
    pub name: String,
    pub description: String,
    pub work_dir: String,
    pub profile: String,
    pub code_puppy_model: String,
    /// LLxprt npm package selector. Blank preserves direct/resolved `llxprt`.
    pub llxprt_version: String,
    pub code_puppy_yolo: bool,
    pub code_puppy_quick_resume: crate::domain::QuickResume,
    pub agent_kind: String,
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
    pub code_puppy_model: usize,
    pub llxprt_version: usize,
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
    LlxprtVersion,
    CodePuppyModel,
    CodePuppyYolo,
    CodePuppyQuickResume,
    AgentKind,
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
            Self::Profile => Self::LlxprtVersion,
            Self::LlxprtVersion => Self::AgentKind,
            Self::AgentKind => Self::CodePuppyModel,
            Self::CodePuppyModel => Self::CodePuppyYolo,
            Self::CodePuppyYolo => Self::CodePuppyQuickResume,
            Self::CodePuppyQuickResume => Self::Mode,
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
            Self::LlxprtVersion => Self::Profile,
            Self::AgentKind => Self::LlxprtVersion,
            Self::CodePuppyModel => Self::AgentKind,
            Self::CodePuppyYolo => Self::CodePuppyModel,
            Self::CodePuppyQuickResume => Self::CodePuppyYolo,
            Self::Mode => Self::CodePuppyQuickResume,
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
    pub default_code_puppy_model: String,
    /// Default LLxprt npm package selector copied into new LLxprt agents.
    pub default_llxprt_version: String,
    pub default_agent_kind: String,
    /// GitHub repository slug in `"owner/repo"` format.
    pub github_repo: String,
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
    pub default_code_puppy_model: usize,
    pub default_llxprt_version: usize,
    pub github_repo: usize,
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
    DefaultCodePuppyModel,
    DefaultLlxprtVersion,
    DefaultAgentKind,
    GitHubRepo,
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
            Self::DefaultProfile => Self::DefaultCodePuppyModel,
            Self::DefaultCodePuppyModel => Self::DefaultLlxprtVersion,
            Self::DefaultLlxprtVersion => Self::DefaultAgentKind,
            Self::DefaultAgentKind => Self::GitHubRepo,
            Self::GitHubRepo => Self::RemoteEnabled,
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
            Self::DefaultCodePuppyModel => Self::DefaultProfile,
            Self::DefaultLlxprtVersion => Self::DefaultCodePuppyModel,
            Self::DefaultAgentKind => Self::DefaultLlxprtVersion,
            Self::GitHubRepo => Self::DefaultAgentKind,
            Self::RemoteEnabled => Self::GitHubRepo,
            Self::LoginUser => Self::RemoteEnabled,
            Self::Host => Self::LoginUser,
            Self::RunAsUser => Self::Host,
            Self::SetupEnvDefault => Self::RunAsUser,
        }
    }
}

/// Form fields for dispatching a GitHub Actions workflow manually.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkflowDispatchFormFields {
    pub ref_name: String,
    pub inputs: String,
}

/// Cursor positions for workflow dispatch form text fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkflowDispatchFormCursor {
    pub ref_name: usize,
    pub inputs: usize,
}

/// Focus states for workflow dispatch form fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkflowDispatchFormFocus {
    #[default]
    RefName,
    Inputs,
    Submit,
    Cancel,
}

impl WorkflowDispatchFormFocus {
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::RefName => Self::Inputs,
            Self::Inputs => Self::Submit,
            Self::Submit => Self::Cancel,
            Self::Cancel => Self::RefName,
        }
    }

    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Self::RefName => Self::Cancel,
            Self::Cancel => Self::Submit,
            Self::Submit => Self::Inputs,
            Self::Inputs => Self::RefName,
        }
    }
}
