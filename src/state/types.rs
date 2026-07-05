//! State types: structs, enums, and field definitions.

use crate::domain::{AgentId, AgentStatus, LaunchSignature, RepositoryId};
use crate::runtime::PreflightIssue;

// @plan PLAN-20260624-PR-MODE.P03
#[path = "pr_types.rs"]
mod pr_types;
pub use pr_types::*;

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
    /// @plan PLAN-20260624-PR-MODE.P03
    /// @requirement REQ-PR-001
    /// @pseudocode component-001 lines 66-76
    DashboardPullRequests,
}

/// Pane focus within a view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PaneFocus {
    #[default]
    Repositories,
    Agents,
    Terminal,
}

/// In-progress dashboard reorder ("grab") target — tracks the visible-index
/// position of the grabbed item so arrow-move stays within the filtered/visible set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DashboardGrabPane {
    /// Grabbing a repository at the given visible-index position.
    Repository { visible_index: usize },
    /// Grabbing an agent at the given local visible-index position within its repository.
    ///
    /// The `repository_id` is captured at grab time so the grab stays bound to
    /// the repository that was selected when Space was pressed — even if the
    /// selected repository changes (e.g. via a shortcut jump) while the grab
    /// is active.
    Agent {
        repository_id: RepositoryId,
        local_index: usize,
    },
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

    /// Agent IDs that were just killed and should remain visible in active-only
    /// mode until the user navigates away. Runtime-only — not persisted.
    pub sticky_dead_agent_ids: std::collections::HashSet<crate::domain::AgentId>,

    // Modal/form state
    pub modal: ModalState,

    // Split mode state
    pub split_filter: Option<RepositoryId>,
    pub split_grab_index: Option<usize>,

    /// Active dashboard reorder grab (Space to grab, arrows to move, Space/Enter to drop).
    /// Transient interaction state — not persisted (like split_grab_index).
    pub dashboard_grab: Option<DashboardGrabPane>,

    // Errors/warnings
    pub error_message: Option<String>,
    pub warning_message: Option<String>,

    // Issues mode state
    /// @plan PLAN-20260329-ISSUES-MODE.P03
    /// @requirement REQ-ISS-001
    pub issues_state: IssuesState,

    // PR mode state (runtime-only — omitted from persisted DTO, same as issues_state)
    /// @plan PLAN-20260624-PR-MODE.P03
    /// @requirement REQ-PR-001
    pub prs_state: PullRequestsState,
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
    NewIssue,
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
pub struct IssuesState {
    pub active: bool,
    pub issues: Vec<crate::domain::Issue>,
    pub selected_issue_index: Option<usize>,
    pub issue_detail: Option<crate::domain::IssueDetail>,
    pub committed_filter: crate::domain::IssueFilter,
    pub draft_filter: crate::domain::IssueFilter,
    pub search_query: String,
    pub loading: IssueLoadingState,
    pub list_cursor: Option<String>,
    pub has_more_issues: bool,
    pub error: Option<String>,
    pub issue_focus: IssueFocus,
    pub detail_subfocus: DetailSubfocus,
    /// Scroll offset (in lines) for the detail pane viewport.
    pub detail_scroll_offset: usize,
    /// Last rendered detail viewport height in rows.
    pub detail_viewport_rows: usize,
    pub inline_state: InlineState,
    pub agent_chooser: Option<AgentChooserState>,
    pub filter_ui: IssueFilterUiState,
    pub search_input_focused: bool,
    pub prior_agent_focus: Option<PriorAgentFocus>,
    pub draft_notice: Option<String>,
    pub mutation_pending: Option<IssueMutationPending>,
    pub next_mutation_id: u64,
    pub list_reload_pending: Option<IssueListReloadPending>,
    pub next_issue_list_request_id: u64,
    pub list_page_pending: Option<IssueListPagePending>,
    pub detail_pending: Option<IssueDetailPending>,
    pub next_issue_detail_request_id: u64,
    pub comments_page_pending: Option<IssueCommentsPagePending>,
    pub next_comments_page_request_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueListReloadPending {
    pub scope_repo_id: RepositoryId,
    pub filter: crate::domain::IssueFilter,
    pub request_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueListPagePending {
    pub scope_repo_id: RepositoryId,
    pub filter: crate::domain::IssueFilter,
    pub cursor: Option<String>,
    pub request_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueDetailPending {
    pub scope_repo_id: RepositoryId,
    pub issue_number: u64,
    pub request_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueCommentsPagePending {
    pub scope_repo_id: RepositoryId,
    pub issue_number: u64,
    pub cursor: Option<String>,
    pub request_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueMutationPending {
    pub scope_repo_id: RepositoryId,
    pub id: u64,
    pub target: InlineState,
}

#[derive(Debug, Clone, Default)]
pub struct IssueLoadingState {
    pub list: bool,
    pub detail: bool,
    pub comments: bool,
}

pub const ISSUE_FILTER_FIELD_COUNT: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct IssueFilterUiState {
    pub controls_open: bool,
    /// Index of the currently focused filter field (0=state, 1=author, 2=assignee, 3=labels, 4=type, 5=milestone, 6=module, 7=query_text).
    pub field_index: usize,
    /// Raw labels text while editing (preserves trailing commas). Parsed into Vec on apply.
    pub draft_labels_text: String,
}

impl IssuesState {
    /// Count the number of rendered content lines for the current detail view.
    #[must_use]
    pub fn detail_content_line_count(&self) -> usize {
        let Some(detail) = &self.issue_detail else {
            return 0;
        };

        crate::issue_detail_content::detail_content_line_count(
            detail,
            &self.inline_state,
            self.loading.comments,
        )
    }

    /// Maximum scroll offset so the last line of content sits at the bottom of the viewport.
    /// Returns 0 when content fits entirely within the viewport (no scrolling needed).
    #[must_use]
    pub fn max_detail_scroll_offset(&self) -> usize {
        let viewport_rows = if self.detail_viewport_rows == 0 {
            crate::layout::detail_viewport_rows(40)
        } else {
            self.detail_viewport_rows
        };
        self.max_detail_scroll_offset_for_viewport(viewport_rows)
    }

    /// Maximum detail scroll offset for a caller-provided viewport row count.
    #[must_use]
    pub fn max_detail_scroll_offset_for_viewport(&self, viewport_rows: usize) -> usize {
        if self.issue_detail.is_none() {
            return 0;
        }
        let composer_active = matches!(
            self.inline_state,
            InlineState::Composer {
                target: ComposerTarget::NewComment | ComposerTarget::Reply { .. },
                ..
            }
        );
        self.detail_content_line_count().saturating_sub(
            crate::layout::issue_detail_document_viewport_rows(viewport_rows, composer_active),
        )
    }

    /// Maximum detail scroll offset for the Issues-mode layout bands currently
    /// visible in the UI.
    #[must_use]
    pub fn max_detail_scroll_offset_for_layout(
        &self,
        term_rows: usize,
        error_visible: bool,
        filter_controls_open: bool,
    ) -> usize {
        self.max_detail_scroll_offset_for_viewport(crate::layout::issues_detail_viewport_rows(
            term_rows,
            error_visible,
            filter_controls_open,
        ))
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

    // Dashboard reorder grab (Space to grab, arrows to move, Space/Enter to drop)
    EnterDashboardGrab,
    ExitDashboardGrab,
    DashboardGrabMoveUp,
    DashboardGrabMoveDown,

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
    /// Kill and relaunch an agent in one action (Ctrl-r). Surfaces an error
    /// if any step fails rather than silently dropping the agent (issue #117).
    RestartAgent(AgentId),
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
        filter: Box<crate::domain::IssueFilter>,
        request_id: u64,
        issues: Vec<crate::domain::Issue>,
        cursor: Option<String>,
        has_more: bool,
    },
    IssueListLoadFailed {
        scope_repo_id: RepositoryId,
        filter: Box<crate::domain::IssueFilter>,
        request_id: u64,
        request_cursor: Option<String>,
        error: String,
    },
    IssueListPageLoaded {
        scope_repo_id: RepositoryId,
        filter: Box<crate::domain::IssueFilter>,
        request_id: u64,
        request_cursor: Option<String>,
        issues: Vec<crate::domain::Issue>,
        cursor: Option<String>,
        has_more: bool,
    },
    IssueDetailLoaded {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        request_id: u64,
        detail: Box<crate::domain::IssueDetail>,
    },
    IssueDetailLoadFailed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        request_id: u64,
        error: String,
    },
    IssueCommentsPageLoaded {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        request_id: u64,
        request_cursor: Option<String>,
        comments: Vec<crate::domain::IssueComment>,
        cursor: Option<String>,
        has_more: bool,
    },
    IssueCommentsPageFailed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        request_id: u64,
        request_cursor: Option<String>,
        error: String,
    },

    // Filter/Search
    OpenFilterControls,
    CloseFilterControls,
    ApplyFilter,
    ClearFilter,
    ClearDraftFilter,
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
    OpenNewIssueComposer,
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
    MutationSubmitted {
        scope_repo_id: RepositoryId,
        mutation_id: u64,
        target: InlineState,
    },
    IssueCreated {
        scope_repo_id: RepositoryId,
        mutation_id: u64,
        issue_number: u64,
    },
    CommentCreated {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        mutation_id: u64,
        comment: crate::domain::IssueComment,
    },
    CommentCreateFailed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        mutation_id: u64,
        error: String,
    },
    IssueBodyUpdated {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        mutation_id: u64,
        body: String,
    },
    CommentUpdated {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        mutation_id: u64,
        comment_id: u64,
        comment_index: usize,
        body: String,
    },
    MutationFailed {
        scope_repo_id: RepositoryId,
        issue_number: Option<u64>,
        mutation_id: Option<u64>,
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

    // ---- Pull Requests Mode events (additive) ----
    // @plan PLAN-20260624-PR-MODE.P03
    // @requirement REQ-PR-001

    // PR Lifecycle
    /// @pseudocode component-001 lines 66-87
    EnterPrsMode,
    ExitPrsMode,
    RefocusPrList,

    // PR Navigation / Focus
    /// @pseudocode component-001 lines 99-162
    PrNavigateUp,
    PrNavigateDown,
    PrNavigatePageUp,
    PrNavigatePageDown,
    PrNavigateHome,
    PrNavigateEnd,
    PrListEnter,
    PrCycleFocus,
    PrCycleFocusReverse,
    PrScrollDetailUp,
    PrScrollDetailDown,
    PrScrollDetailPageUp,
    PrScrollDetailPageDown,
    PrDetailSubfocusNext,
    PrDetailSubfocusPrev,

    // PR Data Loading
    /// @pseudocode component-001 lines 209-247
    PrListLoaded {
        scope_repo_id: RepositoryId,
        filter: Box<crate::domain::PrFilter>,
        request_id: u64,
        pull_requests: Vec<crate::domain::PullRequest>,
        cursor: Option<String>,
        has_more: bool,
    },
    PrListLoadFailed {
        scope_repo_id: RepositoryId,
        request_id: u64,
        error: String,
    },
    PrListPageLoaded {
        scope_repo_id: RepositoryId,
        request_id: u64,
        pull_requests: Vec<crate::domain::PullRequest>,
        cursor: Option<String>,
        has_more: bool,
    },
    PrDetailLoaded {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        detail: Box<crate::domain::PullRequestDetail>,
    },
    PrDetailLoadFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        error: String,
    },
    PrCommentsPageLoaded {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        comments: Vec<crate::domain::IssueComment>,
        cursor: Option<String>,
        has_more: bool,
    },
    PrCommentsPageFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        error: String,
    },

    // PR Filter / Search
    /// @pseudocode component-001 lines 249-291
    PrOpenFilterControls,
    PrCloseFilterControls,
    PrApplyFilter,
    PrClearFilter,
    PrFilterNavigateNext,
    PrFilterNavigatePrev,
    PrCycleFilterState,
    PrCycleDraftFilter,
    PrCycleReviewFilter,
    PrCycleChecksFilter,
    PrUpdateDraftFilter {
        field: String,
        value: String,
    },
    PrFocusSearchInput,
    PrBlurSearchInput,
    PrSetSearchQuery {
        query: String,
    },
    PrApplySearch,
    PrClearSearch,

    // PR Inline Mutation
    /// @pseudocode component-001 lines 292-330
    PrOpenNewCommentComposer,
    PrOpenReplyComposer {
        comment_index: usize,
    },
    PrInlineChar(char),
    PrInlineNewline,
    PrInlineBackspace,
    PrInlineDelete,
    PrInlineCursorLeft,
    PrInlineCursorRight,
    PrInlineCursorUp,
    PrInlineCursorDown,
    PrInlineSubmit,
    PrInlineCancelOrEsc,
    PrCommentCreated {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        comment: crate::domain::IssueComment,
    },
    PrCommentCreateFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        error: String,
    },
    PrMutationFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        error: String,
    },

    // PR Read-Only Notice (REQ-PR-010/012/013)
    /// @pseudocode component-003 lines 83-89
    PrShowNotice(ReadOnlyHintKind),

    // PR Open-in-Browser (REQ-PR-012)
    /// @pseudocode component-001 lines 349-365
    PrOpenInBrowser,
    PrOpenedInBrowser {
        scope_repo_id: RepositoryId,
        pr_number: u64,
    },
    PrOpenInBrowserFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        error: String,
    },

    // PR In-App Merge (issue #92)
    /// @plan PLAN-20260624-PR-MODE.P03
    /// @requirement REQ-PR-009
    PrOpenMergeChooser,
    PrMergeNavigateUp,
    PrMergeNavigateDown,
    PrMergeConfirm,
    PrMergeCancel,
    PrMerged {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        method: crate::domain::MergeMethod,
    },
    PrMergeFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        error: String,
    },
    PrMergeMethodsLoaded {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        allowed_methods: Vec<crate::domain::MergeMethod>,
    },

    // PR Send-to-Agent
    /// @pseudocode component-001 lines 331-343
    PrOpenAgentChooser,
    PrAgentChooserNavigateUp,
    PrAgentChooserNavigateDown,
    PrAgentChooserConfirm,
    PrAgentChooserCancel,
    PrSendToAgentCompleted,
    PrSendToAgentFailed {
        error: String,
    },
}
