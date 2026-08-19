//! Domain-scoped internal message bus.
//! Reducers and dispatch code route through typed domain messages; new behavior
//! goes into the smallest domain enum rather than app-shell branching.
use crate::domain::observation::AgentObservation;
use crate::domain::{
    AgentId, AgentStatus, Issue, IssueComment, IssueDetail, IssueFilter, MergeMethod, PrFilter,
    PullRequest, PullRequestDetail, RepositoryId,
};
use crate::list_viewport::PageItemCount;
use crate::state::{EditorTarget, InlineState, ReadOnlyHintKind};
mod issues_conversion;
mod issues_conversion_close;
mod issues_mutation_conversion;
mod issues_property_conversion;
mod issues_silent_refresh_conversion;
// @plan PLAN-20260624-PR-MODE.P03
// @requirement REQ-PR-002
mod actions;
mod actions_conversion;
mod prs;
mod prs_changes_conversion;
mod prs_conversion;
mod prs_conversion_agent;
mod prs_lifecycle_conversion;
mod prs_property_conversion;
pub use actions::ActionsMessage;
pub mod provider;
pub use provider::ProviderMessage;
mod errors;
mod errors_conversion;
pub use errors::ErrorsMessage;
/// Settings-shell messages (issue #387 CW-07).
pub mod settings;
pub use prs::PullRequestsMessage;
pub use settings::SettingsMessage;
mod event_conversion;
mod names;
pub use names::is_new_issue_form_msg;
mod terminal_manager;
mod terminal_manager_conversion;
pub use terminal_manager::TerminalManagerMessage;
/// Stable domain channel names used for routing, tracing, and policy tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageDomain {
    UiNavigation,
    Modal,
    RepositoryAgent,
    Runtime,
    Persistence,
    Theme,
    Issues,
    /// @plan PLAN-20260624-PR-MODE.P03
    /// @requirement REQ-PR-001
    PullRequests,
    Actions,
    Provider,
    Errors,
    /// Settings-shell domain (issue #387 CW-07).
    Settings,
    /// Terminal-manager domain (issue #361 PR B).
    TerminalManager,
    System,
    /// Typed post-commit effect completions (issue #381 CW01-11).
    Effects,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageRoute {
    pub domain: MessageDomain,
    pub name: &'static str,
}
#[derive(Debug, Clone)]
pub enum UiNavigationMessage {
    /// Unwind exactly one active navigation layer using the shared Back reducer.
    Back,
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
    /// Dashboard "search lite" events for repositories and agents (issue #405).
    FocusDashboardSearch,
    BlurDashboardSearch,
    SetDashboardSearchQuery {
        query: String,
    },
    ClearDashboardSearch,
    EnterSplitMode,
    ExitSplitMode,
    EnterGrabMode,
    ExitGrabMode,
    GrabMoveUp,
    GrabMoveDown,
    SetSplitFilter(Option<RepositoryId>),
    EnterDashboardGrab,
    ExitDashboardGrab,
    DashboardGrabMoveUp,
    DashboardGrabMoveDown,
    /// Terminal scrollback viewport events (issue #198).
    TerminalScrollUp,
    TerminalScrollDown,
    TerminalScrollPageUp,
    TerminalScrollPageDown,
    TerminalFollowTail,
    /// Scroll to the top of terminal history (Home key, issue #198 review #8).
    TerminalScrollToTop,
    /// Open the embedded agent-shell overlay (F10, issue #222).
    OpenShellOverlay,
    /// Close the embedded agent-shell overlay (F10 toggle, issue #355).
    CloseShellOverlay,
    /// Hide the visible shell overlay while keeping the `jefe-shell` window
    /// alive (F12, issue #361).
    HideShellOverlay,
    /// Resume a hidden shell for `agent_id` (F10 from dashboard, issue #361).
    ResumeShellOverlay(crate::domain::AgentId),
    /// Toggle one status bucket in the workbench filter mask (issue #626).
    ToggleWorkbenchStatusBucket(crate::workbench_view::StatusBucket),
    /// Advance to the next workbench page (clamped, issue #626).
    WorkbenchNextPage,
    /// Return to the previous workbench page (clamped, issue #626).
    WorkbenchPrevPage,
    WorkbenchFilterCursorPrev,
    WorkbenchFilterCursorNext,
    WorkbenchSelectPrev,
    WorkbenchSelectNext,
    WorkbenchAttach,
}
#[derive(Debug, Clone)]
pub enum ModalMessage {
    OpenHelp,
    OpenSearch,
    CloseModal,
    SubmitForm,
    /// Cycle confirm-dialog button focus (issue #228).
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
}
#[derive(Debug, Clone)]
pub enum RepositoryAgentMessage {
    OpenNewRepository,
    OpenEditRepository(RepositoryId),
    OpenDeleteRepository(RepositoryId),
    OpenNewAgent(RepositoryId),
    OpenAgentTypeForm(crate::domain::agent_definition::AgentTypeId),
    OpenEditAgent(AgentId),
    OpenDeleteAgent(AgentId),
    ToggleDeleteWorkDir,
    ProbeAgentAvailability(Vec<crate::domain::effects::AgentAvailabilityProbe>),
    ProjectActionAvailability,
}
#[derive(Debug, Clone)]
pub enum RuntimeMessage {
    KillAgent(AgentId),
    RelaunchAgent(AgentId),
    RestartAgent(AgentId),
    AgentStatusChanged(AgentId, AgentStatus),
    ObservationUpdated(AgentId, u64, Box<AgentObservation>),
    ObservationCleared(AgentId, u64),
}
#[derive(Debug, Clone)]
pub enum PersistenceMessage {
    LoadSuccess,
    LoadFailed(String),
    SaveSuccess,
    SaveFailed(String),
    /// Stage a durable save of the committed state (issue #381).
    ///
    /// The reducer projects the schema-2 candidate, assigns it the next
    /// revision, and stages one bounded `PersistState` effect; the bytes are
    /// written by the root shell after every state guard is released.
    StageSave,
}
#[derive(Debug, Clone)]
pub enum ThemeMessage {
    ResolveFailed(String),
}
/// Issues-mode messages.
#[derive(Debug, Clone)]
pub enum IssuesMessage {
    EnterMode,
    ExitMode,
    RefocusList,
    NavigateUp,
    NavigateDown,
    NavigatePageUp(PageItemCount),
    NavigatePageDown(PageItemCount),
    NavigateHome,
    NavigateEnd,
    Enter,
    CycleFocus,
    CycleFocusReverse,
    ScrollDetailUp,
    ScrollDetailDown,
    ScrollDetailPageUp,
    ScrollDetailPageDown,
    DetailSubfocusNext,
    DetailSubfocusPrev,
    ListLoaded {
        scope_repo_id: RepositoryId,
        filter: Box<IssueFilter>,
        request_id: u64,
        issues: Vec<Issue>,
        cursor: Option<String>,
        has_more: bool,
    },
    ListLoadFailed {
        scope_repo_id: RepositoryId,
        filter: Box<IssueFilter>,
        request_id: u64,
        request_cursor: Option<String>,
        error: String,
    },
    ListPageLoaded {
        scope_repo_id: RepositoryId,
        filter: Box<IssueFilter>,
        request_id: u64,
        request_cursor: Option<String>,
        issues: Vec<Issue>,
        cursor: Option<String>,
        has_more: bool,
    },
    DetailLoaded {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        request_id: u64,
        detail: Box<IssueDetail>,
    },
    DetailLoadFailed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        request_id: u64,
        error: String,
    },
    DetailAuthRequired {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        request_id: u64,
    },
    CommentsPageLoaded {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        request_id: u64,
        request_cursor: Option<String>,
        comments: Vec<IssueComment>,
        cursor: Option<String>,
        has_more: bool,
    },
    CommentsPageFailed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        request_id: u64,
        request_cursor: Option<String>,
        error: String,
    },
    /// Silent background list refresh succeeded (issue #175).
    ListSilentRefreshed {
        scope_repo_id: RepositoryId,
        filter: Box<IssueFilter>,
        request_id: u64,
        issues: Vec<Issue>,
        cursor: Option<String>,
        has_more: bool,
    },
    /// Silent background list refresh failed (issue #175).
    ListSilentRefreshFailed {
        scope_repo_id: RepositoryId,
        request_id: u64,
    },
    /// Silent background detail refresh succeeded (issue #175).
    DetailSilentRefreshed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        request_id: u64,
        detail: Box<IssueDetail>,
    },
    /// Silent background detail refresh failed (issue #175).
    DetailSilentRefreshFailed {
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
        target: EditorTarget,
    },
    // ── New Issue form (issue #407) ──────────────────────────────────
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
    NewIssueCreated {
        scope_repo_id: RepositoryId,
        mutation_id: u64,
        issue: Box<Issue>,
    },
    NewIssueCreateFailed {
        scope_repo_id: RepositoryId,
        mutation_id: u64,
        issue_number: Option<u64>,
        error: String,
    },
    InlineChar(char),
    InlineNewline,
    InlineBackspace,
    InlineDelete,
    InlineCursorLeft,
    InlineCursorRight,
    InlineCursorUp,
    InlineCursorDown,
    /// Move the caret to the start of the current line (Home, issue #406).
    InlineCursorHome,
    /// Move the caret to the end of the current line (End, issue #406).
    InlineCursorEnd,
    InlineSubmit,
    InlineCancelOrEsc,
    /// Ask the configured default agent to rewrite the new-issue draft
    /// non-interactively (issue #214).
    RequestIssueRewrite,
    /// The non-interactive rewrite completed (issue #214).
    IssueRewriteSucceeded {
        text: String,
    },
    /// The non-interactive rewrite failed (issue #214).
    IssueRewriteFailed {
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
        issue: Box<Issue>,
    },
    CommentCreated {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        mutation_id: u64,
        comment: IssueComment,
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
    // Issue Close / Delete lifecycle (issue #182)
    CloseIssue,
    OpenDeleteIssueConfirm,
    IssueDeleteConfirm,
    IssueDeleteCancel,
    // Issue Close-with-reason chooser (issue #188)
    OpenCloseReasonChooser,
    CloseReasonNavigateUp,
    CloseReasonNavigateDown,
    CloseReasonSelect,
    CloseReasonDuplicateSearchChar(char),
    CloseReasonDuplicateSearchBackspace,
    CloseReasonDuplicateSearchNavigateUp,
    CloseReasonDuplicateSearchNavigateDown,
    CloseReasonConfirm,
    CloseReasonCancel,
    IssueClosed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        mutation_id: u64,
        close_reason: Option<crate::domain::CloseReason>,
        duplicate_of: Option<u64>,
    },
    IssueDeleted {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        mutation_id: u64,
    },
    OpenAgentChooser {
        metadata: Vec<crate::domain::AgentChooserGitMetadata>,
    },
    BeginListSendDetail {
        metadata: Vec<crate::domain::AgentChooserGitMetadata>,
    },
    CancelListSendDetail,
    ListSendDetailReady {
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
    /// Non-blocking self-assignment failure warning (issue #186).
    IssueSelfAssignmentFailed {
        owner_repo: String,
        issue_number: u64,
        error: String,
    },
    // Property editing (issue #175)
    OpenPropertyEditor {
        kind: crate::state::IssuePropertyKind,
    },
    PropertyEditorNavigateUp,
    PropertyEditorNavigateDown,
    PropertyEditorToggle,
    PropertyEditorConfirm,
    PropertyEditorCancel,
    PropertyEditorTitleChar(char),
    PropertyEditorTitleBackspace,
    PropertyEditorTitleDelete,
    PropertyEditorTitleCursorLeft,
    PropertyEditorTitleCursorRight,
    PropertyEditorTitleCursorHome,
    PropertyEditorTitleCursorEnd,
    PropertyEditorOptionsLoaded {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        kind: crate::state::IssuePropertyKind,
        request_id: u64,
        options: Vec<(Option<String>, String, bool)>,
    },
    PropertyEditorOptionsFailed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        kind: crate::state::IssuePropertyKind,
        request_id: u64,
        error: String,
    },
    PropertyEditSucceeded {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        kind: crate::state::IssuePropertyKind,
        request_id: u64,
    },
    /// Consume a queued issue refresh immediately before orchestration starts it.
    PostMutationRefreshStarted,
    PropertyEditFailed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        kind: crate::state::IssuePropertyKind,
        request_id: u64,
        error: String,
    },
    /// Synchronous validation error set directly on the open editor (issue #175).
    PropertyEditorValidationError {
        kind: crate::state::IssuePropertyKind,
        error: String,
    },
}
/// Navigation direction for PR list and filter controls.
///
/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-003
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavDir {
    Up,
    Down,
    PageUp(PageItemCount),
    PageDown(PageItemCount),
    Home,
    End,
    /// Forward navigation for filter/chooser field stepping (Next/Prev semantics).
    Next,
    /// Reverse navigation for filter/chooser field stepping.
    Prev,
}
/// Scroll direction for the PR detail pane.
///
/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-009
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDir {
    Up,
    Down,
    PageUp,
    PageDown,
}
/// Filter field identifier for `UpdateDraftFilter`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrFilterField {
    Query,
    Author,
    Assignee,
    Reviewer,
    Labels,
}
impl PrFilterField {
    /// Parse a filter field name string into the enum.
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-002
    /// @pseudocode component-004 lines 45-85
    #[must_use]
    pub fn from_string(s: &str) -> Self {
        match s {
            "author" => Self::Author,
            "assignee" => Self::Assignee,
            "reviewer" => Self::Reviewer,
            "labels" => Self::Labels,
            _ => Self::Query,
        }
    }

    /// Return the canonical string name for this filter field.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-002
    /// @pseudocode component-004 lines 45-85
    #[must_use]
    pub fn as_string(&self) -> String {
        match self {
            Self::Query => "query".to_string(),
            Self::Author => "author".to_string(),
            Self::Assignee => "assignee".to_string(),
            Self::Reviewer => "reviewer".to_string(),
            Self::Labels => "labels".to_string(),
        }
    }
}

/// Inline composer message for PR mode.
///
/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-010
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrInlineMsg {
    Char(char),
    Newline,
    Backspace,
    Delete,
    CursorLeft,
    CursorRight,
    CursorUp,
    CursorDown,
    /// Move the caret to the start of the current line (Home, issue #406).
    CursorHome,
    /// Move the caret to the end of the current line (End, issue #406).
    CursorEnd,
    Submit,
    CancelOrEsc,
}

/// System-level messages that do not mutate a domain reducer directly.
#[derive(Debug, Clone)]
pub enum SystemMessage {
    Quit,
    ClearError,
    ClearWarning,
    /// Open the in-app device-code auth dialog (issue #244).
    OpenAuthDialog,
    /// One-time code + verification URL parsed from `gh auth login` stderr.
    AuthCodeReceived {
        code: String,
        url: String,
    },
    /// Device-code flow succeeded.
    AuthSucceeded,
    /// Device-code flow failed (transient — retry offered).
    AuthFailed {
        error: String,
    },
    /// User cancelled the auth dialog.
    AuthCancelled,
    /// User requested a retry of the auth flow.
    AuthRetry,
    /// A transient agent send was queued (issue #213).
    TransientAgentQueued {
        queue_position: usize,
    },
    /// A transient agent was dequeued and is being launched (issue #213).
    TransientAgentDequeued,
}

/// Top-level typed message routed by the bus.
#[derive(Debug, Clone)]
pub enum AppMessage {
    UiNavigation(UiNavigationMessage),
    Modal(ModalMessage),
    RepositoryAgent(RepositoryAgentMessage),
    Runtime(RuntimeMessage),
    Persistence(PersistenceMessage),
    Theme(ThemeMessage),
    Issues(IssuesMessage),
    /// @plan PLAN-20260624-PR-MODE.P03
    /// @requirement REQ-PR-001
    PullRequests(PullRequestsMessage),
    Actions(ActionsMessage),
    Provider(Box<ProviderMessage>),
    Errors(ErrorsMessage),
    /// Settings-shell domain (issue #387 CW-07). Boxed because one variant
    /// carries the whole loaded source and would otherwise set the size of
    /// every message on the bus.
    Settings(Box<SettingsMessage>),
    /// Terminal-manager domain (issue #361 PR B).
    TerminalManager(TerminalManagerMessage),
    System(SystemMessage),
    /// Typed completion for a staged post-commit effect (issue #381); stale
    /// completions are byte-equivalent no-ops.
    EffectCompletion(Box<crate::domain::effects::EffectCompletion>),
}

impl AppMessage {
    #[must_use]
    pub const fn domain(&self) -> MessageDomain {
        match self {
            Self::UiNavigation(_) => MessageDomain::UiNavigation,
            Self::Modal(_) => MessageDomain::Modal,
            Self::RepositoryAgent(_) => MessageDomain::RepositoryAgent,
            Self::Runtime(_) => MessageDomain::Runtime,
            Self::Persistence(_) => MessageDomain::Persistence,
            Self::Theme(_) => MessageDomain::Theme,
            Self::Issues(_) => MessageDomain::Issues,
            // @plan PLAN-20260624-PR-MODE.P03
            // @requirement REQ-PR-001
            Self::PullRequests(_) => MessageDomain::PullRequests,
            Self::Actions(_) => MessageDomain::Actions,
            Self::Provider(_) => MessageDomain::Provider,
            Self::Errors(_) => MessageDomain::Errors,
            Self::Settings(_) => MessageDomain::Settings,
            Self::TerminalManager(_) => MessageDomain::TerminalManager,
            Self::System(_) => MessageDomain::System,
            Self::EffectCompletion(_) => MessageDomain::Effects,
        }
    }

    #[must_use]
    pub fn route(&self) -> MessageRoute {
        MessageRoute {
            domain: self.domain(),
            name: self.name(),
        }
    }

    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::UiNavigation(message) => message.name(),
            Self::Modal(message) => message.name(),
            Self::RepositoryAgent(message) => message.name(),
            Self::Runtime(message) => message.name(),
            Self::Persistence(message) => message.name(),
            Self::Theme(message) => message.name(),
            Self::Issues(message) => message.name(),
            // @plan PLAN-20260624-PR-MODE.P03
            // @requirement REQ-PR-002
            Self::PullRequests(message) => message.name(),
            Self::Actions(message) => message.name(),
            Self::Provider(message) => message.name(),
            Self::Errors(message) => message.name(),
            Self::Settings(message) => message.name(),
            Self::TerminalManager(message) => message.name(),
            Self::System(message) => message.name(),
            Self::EffectCompletion(_) => "EffectCompletion",
        }
    }
}
