use crate::domain::RepositoryId;

use super::{ActionsFilterField, InlineState, ReadOnlyHintKind};

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
    /// Cycle confirm-dialog button focus (Left/Right/Tab in a confirm modal, issue #228).
    ConfirmCycleFocus,

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
    OpenEditAgent(crate::domain::AgentId),
    OpenDeleteAgent(crate::domain::AgentId),
    ToggleDeleteWorkDir,

    // Lifecycle
    KillAgent(crate::domain::AgentId),
    RelaunchAgent(crate::domain::AgentId),
    /// Kill and relaunch an agent in one action (Ctrl-r). Surfaces an error
    /// if any step fails rather than silently dropping the agent (issue #117).
    RestartAgent(crate::domain::AgentId),
    AgentStatusChanged(crate::domain::AgentId, crate::domain::AgentStatus),

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
    /// Toggle the "Apply jefe theme to agent" theme-picker checkbox (issue #179).
    ThemePickerToggleOverride,
    CloseThemePicker,

    // System
    Quit,
    ClearError,
    ClearWarning,

    // Terminal scrollback (issue #198)
    /// Scroll the terminal viewport up (back in history) by one line.
    TerminalScrollUp,
    /// Scroll the terminal viewport down (toward live) by one line.
    TerminalScrollDown,
    /// Scroll the terminal viewport up by a full page.
    TerminalScrollPageUp,
    /// Scroll the terminal viewport down by a full page.
    TerminalScrollPageDown,
    /// Resume follow-tail (clear the scrollback offset).
    TerminalFollowTail,
    /// Scroll the terminal viewport to the top of history (issue #198 review
    /// fix #8: Home key).
    TerminalScrollToTop,

    // Issues Mode events
    EnterIssuesMode,
    ExitIssuesMode,
    RefocusIssueList,
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
    /// Silent background list refresh succeeded (issue #175). Mirrors
    /// `PrListSilentRefreshed`: preserves selection/scroll/filter and does NOT
    /// flash the loading spinner.
    IssueListSilentRefreshed {
        scope_repo_id: RepositoryId,
        filter: Box<crate::domain::IssueFilter>,
        request_id: u64,
        issues: Vec<crate::domain::Issue>,
        cursor: Option<String>,
        has_more: bool,
    },
    /// Silent background list refresh failed (issue #175). Clears the pending
    /// marker WITHOUT surfacing a visible error.
    IssueListSilentRefreshFailed {
        scope_repo_id: RepositoryId,
        request_id: u64,
    },
    /// Silent background detail refresh succeeded (issue #175). Mirrors
    /// `PrDetailSilentRefreshed`: updates detail in place WITHOUT setting
    /// `loading.detail` and preserves `detail_scroll_offset`.
    IssueDetailSilentRefreshed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        request_id: u64,
        detail: Box<crate::domain::IssueDetail>,
    },
    /// Silent background detail refresh failed (issue #175). Clears
    /// `detail_pending` silently WITHOUT setting an error.
    IssueDetailSilentRefreshFailed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        request_id: u64,
    },
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
    OpenNewIssueComposer,
    OpenNewCommentComposer,
    OpenReplyComposer {
        comment_index: usize,
    },
    OpenInlineEditor {
        target: super::EditorTarget,
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
    OpenAgentChooser,
    AgentChooserNavigateUp,
    AgentChooserNavigateDown,
    AgentChooserConfirm,
    AgentChooserCancel,
    SendToAgentCompleted,
    SendToAgentFailed {
        error: String,
    },
    /// Non-blocking warning: an issue send-to-agent succeeded, but the
    /// follow-up self-assignment to the authenticated viewer failed (issue
    /// #186). Sets `warning_message` without affecting the launch.
    IssueSelfAssignmentFailed {
        owner_repo: String,
        issue_number: u64,
        error: String,
    },

    // Pull Requests Mode events
    EnterPrsMode,
    ExitPrsMode,
    RefocusPrList,
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
    PrListSilentRefreshed {
        scope_repo_id: RepositoryId,
        filter: Box<crate::domain::PrFilter>,
        request_id: u64,
        pull_requests: Vec<crate::domain::PullRequest>,
        cursor: Option<String>,
        has_more: bool,
    },
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
    PrDetailSilentRefreshed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        detail: Box<crate::domain::PullRequestDetail>,
    },
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
    PrShowNotice(ReadOnlyHintKind),
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
    PrOpenAgentChooser,
    PrAgentChooserNavigateUp,
    PrAgentChooserNavigateDown,
    PrAgentChooserConfirm,
    PrAgentChooserCancel,
    PrSendToAgentCompleted,
    PrSendToAgentFailed {
        error: String,
    },

    // Actions Mode events
    EnterActionsMode,
    ExitActionsMode,
    RefocusActionsList,
    ActionsReload,
    ActionsNavigateUp,
    ActionsNavigateDown,
    ActionsNavigatePageUp,
    ActionsNavigatePageDown,
    ActionsNavigateHome,
    ActionsNavigateEnd,
    ActionsEnter,
    ActionsCycleFocus,
    ActionsCycleFocusReverse,
    ActionsScrollDetailUp,
    ActionsScrollDetailDown,
    ActionsToggleJobExpand,
    ActionsCollapseJob,
    ActionsNavigateJobUp,
    ActionsNavigateJobDown,
    ActionsRunsLoaded {
        scope_repo_id: RepositoryId,
        filter: Box<crate::domain::ActionsFilter>,
        page: u32,
        request_id: u64,
        runs: Vec<crate::domain::WorkflowRun>,
        has_more: bool,
    },
    ActionsRunsLoadFailed {
        scope_repo_id: RepositoryId,
        filter: Box<crate::domain::ActionsFilter>,
        page: u32,
        request_id: u64,
        error: String,
    },
    ActionsDetailLoaded {
        scope_repo_id: RepositoryId,
        run_id: u64,
        request_id: u64,
        detail: Box<crate::domain::WorkflowRunDetail>,
    },
    ActionsDetailLoadFailed {
        scope_repo_id: RepositoryId,
        run_id: u64,
        request_id: u64,
        error: String,
    },
    WorkflowsLoaded {
        scope_repo_id: RepositoryId,
        request_id: u64,
        workflows: Vec<crate::domain::Workflow>,
    },
    WorkflowsLoadFailed {
        scope_repo_id: RepositoryId,
        request_id: u64,
        error: String,
    },
    ActionsOpenFilterControls,
    ActionsCloseFilterControls,
    ActionsApplyFilter,
    ActionsClearFilter,
    ActionsClearDraftFilter,
    ActionsFilterNavigateNext,
    ActionsFilterNavigatePrev,
    ActionsCycleFilterStatus,
    ActionsFocusSearchInput,
    ActionsBlurSearchInput,
    ActionsSetSearchQuery {
        query: String,
    },
    ActionsApplySearch,
    ActionsClearSearch,
    ActionsUpdateDraftFilter {
        field: ActionsFilterField,
        value: String,
    },
    OpenWorkflowDispatch(crate::domain::Workflow),
    CloseWorkflowDispatch,
    WorkflowDispatchSubmitted {
        scope_repo_id: RepositoryId,
        workflow_id: String,
        ref_name: String,
        inputs: Vec<(String, String)>,
    },
    WorkflowDispatchSuccess {
        scope_repo_id: RepositoryId,
        request_id: u64,
    },
    WorkflowDispatchFailed {
        scope_repo_id: RepositoryId,
        request_id: u64,
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

    // Property editing (issue #175) — Issues
    IssueOpenPropertyEditor {
        kind: super::IssuePropertyKind,
    },
    IssuePropertyEditorNavigateUp,
    IssuePropertyEditorNavigateDown,
    IssuePropertyEditorToggle,
    IssuePropertyEditorConfirm,
    IssuePropertyEditorCancel,
    IssuePropertyEditorTitleChar(char),
    IssuePropertyEditorTitleBackspace,
    IssuePropertyEditorTitleDelete,
    IssuePropertyEditorTitleCursorLeft,
    IssuePropertyEditorTitleCursorRight,
    IssuePropertyEditorOptionsLoaded {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        kind: super::IssuePropertyKind,
        request_id: u64,
        options: Vec<(Option<String>, String, bool)>,
    },
    IssuePropertyEditorOptionsFailed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        kind: super::IssuePropertyKind,
        request_id: u64,
        error: String,
    },
    IssuePropertyEditSucceeded {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        kind: super::IssuePropertyKind,
        request_id: u64,
    },
    IssuePropertyEditFailed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        kind: super::IssuePropertyKind,
        request_id: u64,
        error: String,
    },
    /// Synchronous validation error (e.g. empty title, missing repo) that
    /// should set the open editor's error WITHOUT mutation correlation
    /// (issue #175). Applied directly to the active editor if its kind matches.
    IssuePropertyEditorValidationError {
        kind: super::IssuePropertyKind,
        error: String,
    },

    // Property editing (issue #175) — PRs
    PrOpenPropertyEditor {
        kind: super::PrPropertyKind,
    },
    PrPropertyEditorNavigateUp,
    PrPropertyEditorNavigateDown,
    PrPropertyEditorToggle,
    PrPropertyEditorConfirm,
    PrPropertyEditorCancel,
    PrPropertyEditorTitleChar(char),
    PrPropertyEditorTitleBackspace,
    PrPropertyEditorTitleDelete,
    PrPropertyEditorTitleCursorLeft,
    PrPropertyEditorTitleCursorRight,
    PrPropertyEditorOptionsLoaded {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        kind: super::PrPropertyKind,
        request_id: u64,
        options: Vec<(Option<String>, String, bool)>,
    },
    PrPropertyEditorOptionsFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        kind: super::PrPropertyKind,
        request_id: u64,
        error: String,
    },
    PrPropertyEditSucceeded {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        kind: super::PrPropertyKind,
        request_id: u64,
    },
    PrPropertyEditFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        kind: super::PrPropertyKind,
        request_id: u64,
        error: String,
    },
    /// Synchronous validation error (e.g. empty title, missing repo) that
    /// should set the open PR editor's error WITHOUT mutation correlation
    /// (issue #175). Applied directly to the active editor if its kind matches.
    PrPropertyEditorValidationError {
        kind: super::PrPropertyKind,
        error: String,
    },
}
