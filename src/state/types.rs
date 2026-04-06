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
            Self::DefaultProfile => Self::GitHubRepo,
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
            Self::GitHubRepo => Self::DefaultProfile,
            Self::RemoteEnabled => Self::GitHubRepo,
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
    /// @plan PLAN-20260329-ISSUES-MODE.P03
    /// @requirement REQ-ISS-001
    DashboardIssues,
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

    // Issues mode state
    /// @plan PLAN-20260329-ISSUES-MODE.P03
    /// @requirement REQ-ISS-001
    pub issues_state: IssuesState,
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-001
/// @pseudocode component-001 lines 01-05
/// Focus domain within Issues Mode — separate from PaneFocus.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IssueFocus {
    RepoList,
    #[default]
    IssueList,
    IssueDetail,
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-003
/// Subfocus within issue detail view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DetailSubfocus {
    #[default]
    Body,
    Comment(usize),
    NewComment,
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-010
/// Inline mutable control state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum InlineState {
    #[default]
    None,
    Composer {
        target: ComposerTarget,
        text: String,
        cursor: usize,
    },
    Editor {
        target: EditorTarget,
        text: String,
        cursor: usize,
    },
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-010
/// Target for inline composer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposerTarget {
    NewComment,
    Reply {
        comment_index: usize,
        author: String,
    },
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-010
/// Target for inline editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTarget {
    IssueBody,
    Comment { comment_index: usize },
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-011
/// State for send-to-agent chooser overlay.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentChooserState {
    pub selected_index: usize,
    pub agents: Vec<(crate::domain::AgentId, String)>,
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-005
/// Saved agent-mode focus for restoration on exit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriorAgentFocus {
    pub pane_focus: PaneFocus,
    pub selected_repository_index: Option<usize>,
    pub selected_agent_index: Option<usize>,
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-001
/// @pseudocode component-001 lines 33-40
/// Aggregate state for Issues Mode.
#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct IssuesState {
    pub active: bool,
    pub issues: Vec<crate::domain::Issue>,
    pub selected_issue_index: Option<usize>,
    pub issue_detail: Option<crate::domain::IssueDetail>,
    pub committed_filter: crate::domain::IssueFilter,
    pub draft_filter: crate::domain::IssueFilter,
    pub search_query: String,
    pub list_loading: bool,
    pub detail_loading: bool,
    pub comments_loading: bool,
    pub list_cursor: Option<String>,
    pub has_more_issues: bool,
    pub error: Option<String>,
    pub issue_focus: IssueFocus,
    pub detail_subfocus: DetailSubfocus,
    /// Scroll offset (in lines) for the detail pane viewport.
    pub detail_scroll_offset: usize,
    pub inline_state: InlineState,
    pub agent_chooser: Option<AgentChooserState>,
    pub filter_controls_open: bool,
    /// Index of the currently focused filter field (0=state, 1=author, 2=assignee, 3=labels, 4=query_text).
    pub filter_field_index: usize,
    /// Raw labels text while editing (preserves trailing commas). Parsed into Vec on apply.
    pub draft_labels_text: String,
    pub search_input_focused: bool,
    pub prior_agent_focus: Option<PriorAgentFocus>,
    pub draft_notice: Option<String>,
}

/// Layout constants matching issue_detail.rs and issues.rs.
const DETAIL_HEADER_ROWS: usize = 5;
const DETAIL_CHROME_ROWS: usize = 4;
const ISSUE_LIST_TITLE_ROWS: usize = 1;
const ISSUE_LIST_MIN_VIEWPORT_ROWS: usize = 3;

impl IssuesState {
    /// Compute the scroll viewport rows dynamically from terminal height,
    /// matching the same formula used by `IssueDetailView`.
    fn detail_viewport_rows() -> usize {
        let term_rows = crossterm::terminal::size().map_or(40, |(_, h)| h as usize);
        let workspace_rows = term_rows.saturating_sub(DETAIL_CHROME_ROWS);
        let list_rows = workspace_rows * 3 / 10;
        let detail_pane_rows = workspace_rows.saturating_sub(list_rows);
        detail_pane_rows
            .saturating_sub(DETAIL_HEADER_ROWS + 2)
            .max(5)
    }

    /// Compute the visible rows available for the compact issue list pane.
    fn issue_list_viewport_rows() -> usize {
        let term_rows = crossterm::terminal::size().map_or(40, |(_, h)| h as usize);
        let workspace_rows = term_rows.saturating_sub(DETAIL_CHROME_ROWS);
        let list_rows = workspace_rows * 3 / 10;
        list_rows
            .saturating_sub(ISSUE_LIST_TITLE_ROWS + 2)
            .max(ISSUE_LIST_MIN_VIEWPORT_ROWS)
    }

    /// Scroll offset needed to keep the selected issue visible in the issue list pane.
    #[must_use]
    pub fn issue_list_scroll_offset(&self) -> usize {
        let Some(selected) = self.selected_issue_index else {
            return 0;
        };
        let viewport = Self::issue_list_viewport_rows();
        let max_offset = self.issues.len().saturating_sub(viewport);

        if selected < viewport {
            0
        } else {
            (selected + 1).saturating_sub(viewport).min(max_offset)
        }
    }

    /// Maximum scroll offset so the last line of content sits at the bottom of the viewport.
    /// Returns 0 when content fits entirely within the viewport (no scrolling needed).
    #[must_use]
    pub fn max_detail_scroll_offset(&self) -> usize {
        let Some(detail) = &self.issue_detail else {
            return 0;
        };

        let viewport = Self::detail_viewport_rows();

        // Estimate content lines to match build_detail_content() in issue_detail.rs:
        //   Body section: 1 (label) + body lines + 1 (separator)
        //   Comments section: 1 (header) + per-comment (1 author + body lines + 1 blank)
        //   New Comment section: 1 (label) + 1 (hint)
        let body_lines = if detail.body.is_empty() {
            1
        } else {
            detail.body.lines().count()
        };
        let mut total = 1 + body_lines + 1; // body label + body + separator

        total += 1; // "Comments" header
        if detail.comments.is_empty() {
            total += 1; // "No comments yet."
        } else {
            for c in &detail.comments {
                let c_lines = if c.body.is_empty() {
                    1
                } else {
                    c.body.lines().count()
                };
                total += 1 + c_lines + 1; // author + body + blank
            }
        }

        total += 2; // new comment label + hint

        total.saturating_sub(viewport)
    }
}

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

    // Issues Mode events
    // @plan PLAN-20260329-ISSUES-MODE.P03
    // @requirement REQ-ISS-001
    EnterIssuesMode,
    ExitIssuesMode,
    RefocusIssueList,

    // Issues Navigation
    IssuesNavigateUp,
    IssuesNavigateDown,
    IssuesNavigatePageUp,
    IssuesNavigatePageDown,
    IssuesNavigateHome,
    IssuesNavigateEnd,
    IssuesEnter,
    IssuesCycleFocus,
    IssuesCycleFocusReverse,
    IssuesScrollDetailUp,
    IssuesScrollDetailDown,
    IssuesScrollDetailPageUp,
    IssuesScrollDetailPageDown,
    IssueDetailSubfocusNext,
    IssueDetailSubfocusPrev,

    // Issue Data Loading
    IssueListLoaded {
        scope_repo_id: RepositoryId,
        issues: Vec<crate::domain::Issue>,
        cursor: Option<String>,
        has_more: bool,
    },
    IssueListLoadFailed {
        scope_repo_id: RepositoryId,
        error: String,
    },
    IssueListPageLoaded {
        scope_repo_id: RepositoryId,
        issues: Vec<crate::domain::Issue>,
        cursor: Option<String>,
        has_more: bool,
    },
    IssueDetailLoaded {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        detail: Box<crate::domain::IssueDetail>,
    },
    IssueDetailLoadFailed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        error: String,
    },
    IssueCommentsPageLoaded {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        comments: Vec<crate::domain::IssueComment>,
        cursor: Option<String>,
        has_more: bool,
    },
    IssueCommentsPageFailed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        error: String,
    },

    // Filter/Search
    OpenFilterControls,
    CloseFilterControls,
    ApplyFilter,
    ClearFilter,
    FilterNavigateNext,
    FilterNavigatePrev,
    CycleFilterState,
    FocusSearchInput,
    BlurSearchInput,
    SetSearchQuery {
        query: String,
    },
    ApplySearch,
    ClearSearch,
    UpdateDraftFilter {
        field: String,
        value: String,
    },

    // Inline Mutation
    OpenNewCommentComposer,
    OpenReplyComposer {
        comment_index: usize,
    },
    OpenInlineEditor {
        target: EditorTarget,
    },
    InlineChar(char),
    InlineNewline,
    InlineBackspace,
    InlineDelete,
    InlineCursorLeft,
    InlineCursorRight,
    InlineCursorUp,
    InlineCursorDown,
    InlineSubmit,
    InlineCancelOrEsc,
    CommentCreated {
        comment: crate::domain::IssueComment,
    },
    CommentCreateFailed {
        error: String,
    },
    IssueBodyUpdated {
        body: String,
    },
    CommentUpdated {
        comment_index: usize,
        body: String,
    },
    MutationFailed {
        error: String,
    },

    // Send-to-Agent
    OpenAgentChooser,
    AgentChooserNavigateUp,
    AgentChooserNavigateDown,
    AgentChooserConfirm,
    AgentChooserCancel,
    SendToAgentCompleted,
    SendToAgentFailed {
        error: String,
    },
}
