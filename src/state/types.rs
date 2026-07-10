//! State types: structs, enums, and field definitions.

use std::time::Instant;

use crate::domain::{AgentId, AgentStatus, LaunchSignature, RepositoryId};
use crate::runtime::PreflightIssue;

// @plan PLAN-20260624-PR-MODE.P03
#[path = "pr_types.rs"]
mod pr_types;
pub use pr_types::*;

// Issues-mode aggregate state extracted to keep this file under the length limit.
#[path = "issues_types.rs"]
mod issues_types;
pub use issues_types::*;

// Form-field types extracted to keep this file under the length limit.
#[path = "form_types.rs"]
mod form_types;
pub use form_types::*;

/// Modal/form state variants.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ModalState {
    #[default]
    None,
    Help,
    Search {
        query: String,
    },
    /// Theme picker overlay.
    ///
    /// Lists available theme slugs/names for navigation + selection.
    /// `available_themes` is the snapshot of slugs+names captured when the
    /// picker was opened; the actual theme application happens via
    /// `AppEvent::SetTheme`, which the binary's dispatch layer applies to the
    /// `ThemeManager` and persists to `settings.toml`.
    ThemePicker {
        available_themes: Vec<(String, String)>,
        selected_index: usize,
        /// Slug of the currently-applied theme (for the active marker).
        active_slug: String,
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
    /// Issue send: the working copy has uncommitted changes (excluding
    /// jefe/llxprt-owned paths). Prompt the user to discard them before
    /// the issue-driven launch proceeds. The default is no/halt; the
    /// user must explicitly opt in (Enter) before destructive cleanup.
    /// Escape (or `n`) aborts and leaves the working copy untouched.
    ConfirmIssueDirtyCopy {
        agent_id: AgentId,
        work_dir: std::path::PathBuf,
        signature: LaunchSignature,
        payload: crate::github::SendPayload,
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

/// Bookkeeping for the rapid `qqq` quit sequence.
///
/// Held in [`AppState`] so the count survives across key events. It is reset
/// on the inter-press timeout, on any non-`q` key, and whenever a quit fires.
/// The decision logic lives in `crate::input::observe_quit_sequence`; this type
/// only stores the accumulated state. Runtime-only — never persisted.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct QuitSequenceState {
    /// Consecutive rapid `q` presses accumulated toward the quit threshold.
    pub presses: u8,
    /// Instant of the most recent `q`, used to enforce the inter-press window.
    pub last_press: Option<Instant>,
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

    /// Rapid `qqq` quit-sequence bookkeeping. Runtime-only — never persisted.
    pub quit_sequence: QuitSequenceState,

    /// Active mouse text-selection, if any. Runtime-only — never persisted.
    ///
    /// Set by the app-shell mouse router when the user drag-selects text in any
    /// pane (or in the terminal snapshot when unfocused). Cleared on Escape or
    /// when a new selection begins. Used by the renderers to paint an
    /// inverse-video highlight over the selected cells.
    pub selection: Option<crate::selection::TextSelection>,
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
    /// Reply to a PR review thread (issue #119). `thread_index` is the flat
    /// index across all reviews' threads, matching `PrDetailSubfocus::ReviewThread`.
    ReplyToReviewThread {
        thread_index: usize,
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

    /// Open the theme picker modal with a snapshot of available themes.
    /// Payload: `(slug, name)` pairs, plus the currently active slug.
    OpenThemePicker {
        available_themes: Vec<(String, String)>,
        active_slug: String,
    },
    ThemePickerNavigateUp,
    ThemePickerNavigateDown,
    /// Confirm the current theme-picker selection.
    /// The slug is derived from the modal's `selected_index` at dispatch time
    /// (see `modal_handlers::apply_theme_picker_selection`).
    ThemePickerConfirm,
    CloseThemePicker,

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

    // Issue Close / Delete lifecycle (issue #182)
    /// Key-layer request: close the focused issue (dispatch resolves context).
    CloseIssue,
    /// Key-layer request: open the delete confirm overlay.
    OpenDeleteIssueConfirm,
    /// Delete confirm overlay arm/confirm signal (two-step like merge chooser).
    IssueDeleteConfirm,
    /// Delete confirm overlay cancel.
    IssueDeleteCancel,
    /// Close mutation succeeded.
    IssueClosed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        mutation_id: u64,
    },
    /// Delete mutation succeeded.
    IssueDeleted {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        mutation_id: u64,
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
    /// Silent background refresh succeeded (issue #128). Like `PrListLoaded`
    /// but preserves selection/scroll and does NOT flash the loading spinner.
    PrListSilentRefreshed {
        scope_repo_id: RepositoryId,
        filter: Box<crate::domain::PrFilter>,
        request_id: u64,
        pull_requests: Vec<crate::domain::PullRequest>,
        cursor: Option<String>,
        has_more: bool,
    },
    /// Silent background refresh failed (issue #128). Clears the pending marker
    /// WITHOUT surfacing an error (background failures are non-disruptive).
    PrListSilentRefreshFailed {
        scope_repo_id: RepositoryId,
        request_id: u64,
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
    /// Silent background detail refresh succeeded (issue #128). Like
    /// `PrDetailLoaded` but does NOT set `loading.detail` and preserves
    /// `detail_subfocus` and `detail_scroll_offset`.
    PrDetailSilentRefreshed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        detail: Box<crate::domain::PullRequestDetail>,
    },
    /// Silent background detail refresh failed (issue #128). Clears
    /// `detail_pending` silently WITHOUT setting `loading.detail` or an error.
    PrDetailSilentRefreshFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
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

    // PR Review Threads (issue #119)
    /// Open the inline reply composer for a review thread.
    PrOpenThreadReplyComposer {
        thread_index: usize,
    },
    /// Toggle resolve/unresolve on a focused review thread.
    PrToggleThreadResolve {
        thread_index: usize,
    },
    /// A review-thread resolve/unresolve mutation succeeded.
    PrThreadResolveSucceeded {
        scope_repo_id: RepositoryId,
        thread_index: usize,
        is_resolved: bool,
        request_id: u64,
    },
    /// A review-thread resolve/unresolve mutation failed.
    PrThreadResolveFailed {
        scope_repo_id: RepositoryId,
        thread_index: usize,
        request_id: u64,
        error: String,
    },
}
