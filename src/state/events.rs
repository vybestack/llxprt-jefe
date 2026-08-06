use super::{ActionsFilterField, InlineState, ReadOnlyHintKind};
use crate::domain::RepositoryId;
use crate::list_viewport::PageItemCount;
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Typed completion for a staged post-commit effect (issue #381 CW01-11).
    EffectCompletion(Box<crate::domain::effects::EffectCompletion>),
    NavigateUp,
    NavigateDown,
    NavigatePageUp(PageItemCount),
    NavigatePageDown(PageItemCount),
    NavigateHome,
    NavigateEnd,
    NavigateLeft,
    NavigateRight,
    SelectRepository(usize),
    SelectAgent(usize),
    JumpToAgentByShortcut(u8),
    CyclePaneFocus,
    ToggleTerminalFocus,
    ToggleHideIdleRepositories,
    // Dashboard "search lite" for repositories and agents (issue #405).
    /// Focus the dashboard search input.
    FocusDashboardSearch,
    /// Blur the dashboard search input, retaining the query so the filter
    /// persists (mirrors Issues/PRs Enter-to-apply semantics).
    BlurDashboardSearch,
    /// Replace the dashboard search query (live-filtering the repo sidebar
    /// and agent pane).
    SetDashboardSearchQuery {
        query: String,
    },
    /// Clear the dashboard search query and blur the input.
    ClearDashboardSearch,
    /// Open the embedded shell overlay for the selected local running agent.
    OpenShellOverlay,
    /// Close/restore the embedded shell overlay (F10 toggle or natural exit detected).
    CloseShellOverlay,
    /// Hide the visible shell overlay while keeping the `jefe-shell` window
    /// alive (issue #361). Selects the agent window 0 so the multiplexer
    /// current window invariant holds, then restores dashboard focus/layout.
    HideShellOverlay,
    /// Resume a hidden shell for the selected agent (issue #361). Re-selects
    /// the existing `jefe-shell` window without duplicating it.
    ResumeShellOverlay(crate::domain::AgentId),
    EnterSplitMode,
    ExitSplitMode,
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
    OpenHelp,
    OpenSearch,
    CloseModal,
    SubmitForm,
    /// Cycle confirm-dialog button focus (Left/Right/Tab in a confirm modal, issue #228).
    ConfirmCycleFocus,
    FormChar(char),
    FormBackspace,
    FormDelete,
    FormMoveCursorLeft,
    FormMoveCursorRight,
    FormMoveCursorStart,
    FormMoveCursorEnd,
    FormNextField,
    FormPrevField,
    FormToggleCheckbox,
    OpenNewRepository,
    OpenEditRepository(RepositoryId),
    OpenDeleteRepository(RepositoryId),
    OpenNewAgent(RepositoryId),
    OpenAgentTypeForm(crate::domain::agent_definition::AgentTypeId),
    OpenEditAgent(crate::domain::AgentId),
    OpenDeleteAgent(crate::domain::AgentId),
    ToggleDeleteWorkDir,
    ProbeAgentAvailability(Vec<crate::domain::effects::AgentAvailabilityProbe>),
    ProjectActionAvailability,
    KillAgent(crate::domain::AgentId),
    RelaunchAgent(crate::domain::AgentId),
    /// Kill and relaunch an agent in one action (Ctrl-r). Surfaces an error
    /// if any step fails rather than silently dropping the agent (issue #117).
    RestartAgent(crate::domain::AgentId),
    AgentStatusChanged(crate::domain::AgentId, crate::domain::AgentStatus),
    Observation(super::observation_events::ObservationEvent),
    PersistenceLoadSuccess,
    PersistenceLoadFailed(String),
    PersistenceSaveSuccess,
    /// Stage a durable save of the committed state (issue #381).
    StageDurableSave,
    PersistenceSaveFailed(String),

    ThemeResolveFailed(String),

    /// One Settings-shell intent or completion (issue #387).
    ///
    /// Boxed because one variant carries the whole loaded settings source and
    /// would otherwise set the size of every event in this enum.
    Settings(Box<crate::messages::SettingsMessage>),

    /// One provider request lifecycle message (issue #390 CW-10, Slice B).
    /// Boxed because several variants carry multiple `TypedMap` fields and
    /// would otherwise set the size of every `AppEvent` variant.
    Provider(Box<crate::messages::ProviderMessage>),

    Quit,
    ClearError,
    ClearWarning,
    /// Open the auth dialog and start the device-code flow.
    OpenAuthDialog,
    /// The one-time code + verification URL were parsed from `gh` stderr.
    AuthCodeReceived {
        code: String,
        url: String,
    },
    /// The device-code flow completed successfully (token stored by `gh`).
    AuthSucceeded,
    /// The device-code flow failed (network, code expiry, denied).
    AuthFailed {
        error: String,
    },
    /// The user cancelled the auth dialog (Esc).
    AuthCancelled,
    /// The user requested a retry from the Failed phase.
    AuthRetry,
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
    EnterIssuesMode,
    ExitIssuesMode,
    RefocusIssueList,
    IssuesNavigateUp,
    IssuesNavigateDown,
    IssuesNavigatePageUp(PageItemCount),
    IssuesNavigatePageDown(PageItemCount),
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
    IssueDetailAuthRequired(RepositoryId, u64, u64),
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
    CycleIssueSortByNext,
    CycleIssueSortByPrev,
    ToggleIssueSortOrder,
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
    /// Move the inline composer/editor caret to the start of the current
    /// logical line (Home key, issue #406).
    InlineCursorHome,
    /// Move the inline composer/editor caret to the end of the current
    /// logical line (End key, issue #406).
    InlineCursorEnd,
    InlineSubmit,
    InlineCancelOrEsc,
    /// Ask the configured default agent to rewrite the current new-issue
    /// composer draft non-interactively (issue #214). Applied via the app_input
    /// orchestration layer, which spawns the agent run and applies the result.
    RequestIssueRewrite,
    /// The non-interactive rewrite completed; the composer text is replaced
    /// with `text` (issue #214).
    IssueRewriteSucceeded {
        text: String,
    },
    /// The non-interactive rewrite failed (issue #214). `error` is surfaced as
    /// a non-fatal draft notice so the composer draft is preserved.
    IssueRewriteFailed {
        error: String,
    },
    // ── New Issue dialog events (issue #407) ─────────────────────────────
    NewIssueTemplateNext,
    NewIssueTypeNext,
    NewIssueTitleChar(char),
    NewIssueTitleBackspace,
    NewIssueTitleDelete,
    NewIssueTitleCursorLeft,
    NewIssueTitleCursorRight,
    NewIssueTitleCursorHome,
    NewIssueTitleCursorEnd,
    NewIssueBodyChar(char),
    NewIssueBodyNewline,
    NewIssueBodyBackspace,
    NewIssueBodyDelete,
    NewIssueBodyCursorLeft,
    NewIssueBodyCursorRight,
    NewIssueBodyCursorUp,
    NewIssueBodyCursorDown,
    NewIssueBodyCursorHome,
    NewIssueBodyCursorEnd,
    NewIssueFocusNext,
    NewIssueFocusPrev,
    NewIssueSubmit,
    NewIssueCreated {
        scope_repo_id: RepositoryId,
        mutation_id: u64,
        issue: Box<crate::domain::Issue>,
    },
    NewIssueCreateFailed {
        scope_repo_id: RepositoryId,
        mutation_id: u64,
        /// When the issue was created but a property apply failed, the created
        /// issue number so the UI can tell the user the issue exists (issue #407).
        issue_number: Option<u64>,
        error: String,
    },
    NewIssueCancel,
    NewIssueOptionsLoaded {
        labels: Vec<String>,
        milestones: Vec<String>,
        types: Vec<crate::state::IssueType>,
        assignees: Vec<String>,
    },
    NewIssueOptionsFailed {
        error: String,
    },
    MutationSubmitted {
        scope_repo_id: RepositoryId,
        mutation_id: u64,
        target: InlineState,
    },
    IssueCreated {
        scope_repo_id: RepositoryId,
        mutation_id: u64,
        /// Newly created issue row used for optimistic list insert (issue #215).
        issue: Box<crate::domain::Issue>,
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
        title: String,
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
        /// Close reason carried from the chooser (issue #188). `None` for the
        /// legacy plain-close path.
        close_reason: Option<crate::domain::CloseReason>,
        /// For a Duplicate close, the canonical issue number (issue #188).
        duplicate_of: Option<u64>,
    },
    /// Delete mutation succeeded.
    IssueDeleted {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        mutation_id: u64,
    },
    /// Open the close-reason chooser overlay.
    OpenCloseReasonChooser,
    CloseReasonNavigateUp,
    CloseReasonNavigateDown,
    /// Enter on a reason: for Duplicate enters duplicate-search; otherwise arms
    /// confirmation.
    CloseReasonSelect,
    CloseReasonDuplicateSearchChar(char),
    CloseReasonDuplicateSearchBackspace,
    CloseReasonDuplicateSearchNavigateUp,
    CloseReasonDuplicateSearchNavigateDown,
    /// Second Enter: dispatches the actual close with reason.
    CloseReasonConfirm,
    /// Esc: close the chooser without closing the issue.
    CloseReasonCancel,
    OpenAgentChooser {
        metadata: Vec<crate::domain::AgentChooserGitMetadata>,
    },
    BeginIssueListSendDetail(Vec<crate::domain::AgentChooserGitMetadata>),
    CancelIssueListSendDetail,
    IssueListSendDetailReady {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        request_id: u64,
    },
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
    EnterPrsMode,
    ExitPrsMode,
    RefocusPrList,
    PrNavigateUp,
    PrNavigateDown,
    PrNavigatePageUp(PageItemCount),
    PrNavigatePageDown(PageItemCount),
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
    PrOpenChanges,
    PrChangesFocusContent,
    PrChangesFocusFiles,
    PrChangesToggleView,
    /// Open a line-review composer for the selected Changes row.
    PrOpenChangesComment,
    PrChangesBack,
    /// Retry the changed-files read after a terminal failure. Restages a
    /// fresh head-correlated files load (issue #376).
    PrChangesRetryFiles,
    /// Retry the selected full-file blob read after a terminal failure
    /// (issue #376).
    PrChangesRetryBlob,
    PrChangesLoaded(crate::state::PrChangesLoadedPayload),
    PrChangesLoadFailed(crate::state::PrChangesLoadFailedPayload),
    PrChangesBlobLoaded(crate::state::PrChangesBlobLoadedPayload),
    PrChangesBlobLoadFailed(crate::state::PrChangesBlobLoadFailedPayload),
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
    PrDetailAuthRequired(RepositoryId, u64, u64),
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
    PrCommentsPageDispatchFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
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
    PrCycleSortByNext,
    PrCycleSortByPrev,
    PrToggleSortOrder,
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
    /// Move the PR composer/editor caret to the start of the current logical
    /// line (Home key, issue #406).
    PrInlineCursorHome,
    /// Move the PR composer/editor caret to the end of the current logical
    /// line (End key, issue #406).
    PrInlineCursorEnd,
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
    /// Pull-request lifecycle mutations: merge, close, delete, create.
    PrLifecycle(Box<super::PrLifecycleEvent>),
    PrOpenAgentChooser {
        metadata: Vec<crate::domain::AgentChooserGitMetadata>,
    },
    BeginPrListSendDetail(Vec<crate::domain::AgentChooserGitMetadata>),
    CancelPrListSendDetail,
    PrListSendDetailReady {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
    },
    PrAgentChooserNavigateUp,
    PrAgentChooserNavigateDown,
    PrAgentChooserConfirm,
    PrAgentChooserCancel,
    PrSendToAgentCompleted,
    PrSendToAgentFailed {
        error: String,
    },
    /// A transient agent send was queued (max_concurrent reached).
    TransientAgentQueued {
        queue_position: usize,
    },
    /// A transient agent was dequeued and is being launched.
    TransientAgentDequeued,
    EnterActionsMode,
    /// Enter Actions mode with a PR filter pre-set (cross-mode action from PR mode).
    EnterActionsModeWithPrFilter {
        pr_number: u64,
        head_sha: String,
    },
    ExitActionsMode,
    RefocusActionsList,
    ActionsReload,
    ActionsNavigateUp,
    ActionsNavigateDown,
    ActionsNavigatePageUp(PageItemCount),
    ActionsNavigatePageDown(PageItemCount),
    ActionsNavigateHome,
    ActionsNavigateEnd,
    ActionsEnter,
    ActionsCycleFocus,
    ActionsCycleFocusReverse,
    ActionsSetDetailGeometry {
        viewport_rows: usize,
        content_width: usize,
    },
    ActionsScrollDetailUp,
    ActionsScrollDetailDown,
    ActionsExpandJob,
    ActionsCollapseJob,
    ActionsDetailEscape,
    ActionsNavigateJobUp,
    ActionsNavigateJobDown,
    ActionsBeginDetailReload {
        scope_repo_id: RepositoryId,
        run_id: u64,
        request_id: u64,
    },
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
    /// Page append result (load-more path).
    ActionsRunsPageLoaded {
        scope_repo_id: RepositoryId,
        filter: Box<crate::domain::ActionsFilter>,
        page: u32,
        request_id: u64,
        runs: Vec<crate::domain::WorkflowRun>,
        has_more: bool,
    },
    /// Page append failure.
    ActionsRunsPageLoadFailed {
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
    CycleActionsSortByNext,
    CycleActionsSortByPrev,
    ToggleActionsSortOrder,
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
    IssuePropertyEditorTitleCursorHome,
    IssuePropertyEditorTitleCursorEnd,
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
    /// Consume a queued issue refresh immediately before orchestration starts it.
    IssuePostMutationRefreshStarted,
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
    PrPropertyEditorTitleCursorHome,
    PrPropertyEditorTitleCursorEnd,
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
    /// Consume a queued PR refresh immediately before orchestration starts it.
    PrPostMutationRefreshStarted,
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
    EnterErrorsMode,
    ExitErrorsMode,
    RefocusErrorList,
    ErrorsNavigateUp,
    ErrorsNavigateDown,
    ErrorsNavigateHome,
    ErrorsNavigateEnd,
    ErrorsEnter,
    ErrorsCycleFocus,
    ErrorsCycleFocusReverse,
    ErrorsScrollDetailUp,
    ErrorsScrollDetailDown,
    ErrorsScrollDetailPageUp,
    ErrorsScrollDetailPageDown,
    CaptureSilentError(String, String, crate::domain::ErrorSource, String),
    ErrorsClearAll,
    /// F7 opens Terminal Manager; Esc/F12 returns to Dashboard.
    EnterTerminalManagerMode,
    ExitTerminalManagerMode,
    TerminalManagerNavigateUp,
    TerminalManagerNavigateDown,
    TerminalManagerNavigateHome,
    TerminalManagerNavigateEnd,
    /// Request cross-agent focus on the selected Running owner (reducer only
    /// records generation-guarded pending state; attach happens first).
    RequestShellFocus {
        agent_id: crate::domain::AgentId,
        origin: super::ShellFocusOrigin,
    },
    /// Confirm a pending focus after the expected owner attached.
    ConfirmShellFocus(crate::domain::AgentId),
    /// Fail a pending focus (attach failed or owner no longer Running).
    FailShellFocus,
    /// Selected-shell preview, correlated so stale captures are discarded.
    ShellPreviewResult {
        agent_id: crate::domain::AgentId,
        generation: u64,
        ok: bool,
        lines: Vec<String>,
    },
    /// A shell closed after runtime removed its inventory entry.
    ShellClosed(crate::domain::AgentId),

    // ── Multi-agent workbench (issue #626) ──────────────────────────────
    /// Toggle one status bucket in the workbench filter mask.
    ToggleWorkbenchStatusBucket(crate::workbench_view::StatusBucket),
    /// Advance to the next workbench page (clamped at the last page).
    WorkbenchNextPage,
    /// Return to the previous workbench page (clamped at page 0).
    WorkbenchPrevPage,
    /// Move the workbench status-filter cursor to the previous bucket.
    WorkbenchFilterCursorPrev,
    /// Move the workbench status-filter cursor to the next bucket.
    WorkbenchFilterCursorNext,
    /// Move the agent selection one card back along the workbench order.
    WorkbenchSelectPrev,
    /// Move the agent selection one card forward along the workbench order.
    WorkbenchSelectNext,
    /// Leave the workbench for the selected agent's terminal.
    WorkbenchAttach,
}
