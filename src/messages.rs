//! Domain-scoped internal message bus.
//!
//! The UI can keep producing the historical [`crate::state::AppEvent`] facade,
//! while reducers and dispatch code route through typed domain messages. New
//! behavior should be added to the smallest domain message enum rather than to
//! app-shell-specific branching.

use crate::domain::{
    AgentId, AgentStatus, Issue, IssueComment, IssueDetail, IssueFilter, MergeMethod, PrFilter,
    PullRequest, PullRequestDetail, RepositoryId,
};
use crate::list_viewport::PageItemCount;
use crate::state::{EditorTarget, InlineState, ReadOnlyHintKind};

mod issues_conversion;
mod issues_mutation_conversion;
mod issues_property_conversion;
mod issues_silent_refresh_conversion;
// @plan PLAN-20260624-PR-MODE.P03
// @requirement REQ-PR-002
mod actions;
mod actions_conversion;
mod prs_conversion;
mod prs_property_conversion;
pub use actions::ActionsMessage;
mod errors;
mod errors_conversion;
pub use errors::ErrorsMessage;

// @plan PLAN-20260624-PR-MODE.P03
// @requirement REQ-PR-002
// @pseudocode component-004 lines 46-50
mod event_conversion;
mod names;

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
    Errors,
    System,
}

/// A resolved message route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageRoute {
    pub domain: MessageDomain,
    pub name: &'static str,
}

/// Navigation, focus, and screen-layout messages.
#[derive(Debug, Clone)]
pub enum UiNavigationMessage {
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
    /// Close the embedded agent-shell overlay (F11, issue #222).
    CloseShellOverlay,
}

/// Modal and form-editing messages.
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
    FormNextField,
    FormPrevField,
    FormToggleCheckbox,
}

/// Repository and agent configuration messages.
#[derive(Debug, Clone)]
pub enum RepositoryAgentMessage {
    OpenNewRepository,
    OpenEditRepository(RepositoryId),
    OpenDeleteRepository(RepositoryId),
    OpenNewAgent(RepositoryId),
    OpenEditAgent(AgentId),
    OpenDeleteAgent(AgentId),
    ToggleDeleteWorkDir,
}

/// Runtime lifecycle messages.
#[derive(Debug, Clone)]
pub enum RuntimeMessage {
    KillAgent(AgentId),
    RelaunchAgent(AgentId),
    /// Kill then relaunch an agent in one action (issue #117).
    RestartAgent(AgentId),
    AgentStatusChanged(AgentId, AgentStatus),
}

/// Persistence result messages.
#[derive(Debug, Clone)]
pub enum PersistenceMessage {
    LoadSuccess,
    LoadFailed(String),
    SaveSuccess,
    SaveFailed(String),
}

/// Theme messages.
#[derive(Debug, Clone)]
pub enum ThemeMessage {
    SetTheme(String),
    ResolveFailed(String),
    /// Open the theme picker modal.
    ///
    /// `available_themes` is a list of `(slug, display_name)` pairs from
    /// `ThemeManager::themes_with_names()`. `active_slug` is the currently
    /// applied theme's slug (used to mark the active row).
    OpenThemePicker {
        available_themes: Vec<(String, String)>,
        active_slug: String,
    },
    PickerNavigateUp,
    PickerNavigateDown,
    PickerConfirm,
    PickerCancel,
    /// Toggle the in-dialog "Apply jefe theme to agent" override checkbox
    /// (issue #179). Flips `ModalState::ThemePicker.override_theme`.
    ToggleAgentThemeOverride,
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
///
/// @plan PLAN-20260624-PR-MODE.P03
///
/// @requirement REQ-PR-001
/// @requirement REQ-PR-002
/// @requirement REQ-PR-006
/// @requirement REQ-PR-008
/// @requirement REQ-PR-010
/// @requirement REQ-PR-012
/// @pseudocode component-004 lines 02-35
#[derive(Debug, Clone)]
pub enum PullRequestsMessage {
    EnterMode,
    ExitMode,
    RefocusList,
    Navigate(NavDir),
    Enter,
    CycleFocus,
    CycleFocusReverse,
    ScrollDetail(ScrollDir),
    DetailSubfocusNext,
    DetailSubfocusPrev,
    ListLoaded {
        scope_repo_id: RepositoryId,
        filter: Box<PrFilter>,
        request_id: u64,
        pull_requests: Vec<PullRequest>,
        cursor: Option<String>,
        has_more: bool,
    },
    ListLoadFailed {
        scope_repo_id: RepositoryId,
        request_id: u64,
        error: String,
    },
    ListPageLoaded {
        scope_repo_id: RepositoryId,
        request_id: u64,
        pull_requests: Vec<PullRequest>,
        cursor: Option<String>,
        has_more: bool,
    },
    /// Silent background refresh succeeded (issue #128).
    ListSilentRefreshed {
        scope_repo_id: RepositoryId,
        filter: Box<PrFilter>,
        request_id: u64,
        pull_requests: Vec<PullRequest>,
        cursor: Option<String>,
        has_more: bool,
    },
    /// Silent background refresh failed (issue #128).
    ListSilentRefreshFailed {
        scope_repo_id: RepositoryId,
        request_id: u64,
    },
    DetailLoaded {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        detail: Box<PullRequestDetail>,
    },
    DetailLoadFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        error: String,
    },
    /// Silent background detail refresh succeeded (issue #128).
    DetailSilentRefreshed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        detail: Box<PullRequestDetail>,
    },
    /// Silent background detail refresh failed (issue #128).
    DetailSilentRefreshFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
    },
    CommentsPageLoaded {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        comments: Vec<IssueComment>,
        cursor: Option<String>,
        has_more: bool,
    },
    CommentsPageFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        error: String,
    },
    CommentsPageDispatchFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        error: String,
    },
    OpenFilterControls,
    CloseFilterControls,
    ApplyFilter,
    ClearFilter,
    FilterNavigate(NavDir),
    CycleFilterState,
    CycleDraftFilter,
    CycleReviewFilter,
    CycleChecksFilter,
    UpdateDraftFilter {
        field: PrFilterField,
        value: String,
    },
    FocusSearchInput,
    BlurSearchInput,
    SetSearchQuery {
        query: String,
    },
    ApplySearch,
    ClearSearch,
    OpenNewCommentComposer,
    OpenReplyComposer {
        comment_index: usize,
    },
    Inline(PrInlineMsg),
    CommentCreated {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        comment: IssueComment,
    },
    CommentCreateFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        error: String,
    },
    MutationFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        error: String,
    },
    ShowNotice(ReadOnlyHintKind),
    OpenAgentChooser {
        metadata: Vec<crate::domain::AgentChooserGitMetadata>,
    },
    AgentChooserNavigate(NavDir),
    AgentChooserConfirm,
    AgentChooserCancel,
    SendToAgentCompleted,
    SendToAgentFailed {
        error: String,
    },
    OpenInBrowser,
    OpenedInBrowser {
        scope_repo_id: RepositoryId,
        pr_number: u64,
    },
    OpenInBrowserFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        error: String,
    },
    // PR In-App Merge (issue #92)
    /// @plan PLAN-20260624-PR-MODE.P03
    /// @requirement REQ-PR-009
    OpenMergeChooser,
    MergeNavigate(NavDir),
    MergeConfirm,
    MergeCancel,
    Merged {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        method: MergeMethod,
    },
    MergeFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        error: String,
    },
    MergeMethodsLoaded {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        allowed_methods: Vec<MergeMethod>,
    },
    // PR Review Threads (issue #119)
    /// Open the inline reply composer for a review thread.
    OpenThreadReply {
        thread_index: usize,
    },
    /// Toggle resolve/unresolve on a focused review thread.
    ToggleThreadResolve {
        thread_index: usize,
    },
    /// A review-thread resolve/unresolve mutation succeeded.
    ThreadResolveSucceeded {
        scope_repo_id: RepositoryId,
        thread_index: usize,
        is_resolved: bool,
        request_id: u64,
    },
    /// A review-thread resolve/unresolve mutation failed.
    ThreadResolveFailed {
        scope_repo_id: RepositoryId,
        thread_index: usize,
        request_id: u64,
        error: String,
    },
    // Property editing (issue #175)
    OpenPropertyEditor {
        kind: crate::state::PrPropertyKind,
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
    PropertyEditorOptionsLoaded {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        kind: crate::state::PrPropertyKind,
        request_id: u64,
        options: Vec<(Option<String>, String, bool)>,
    },
    PropertyEditorOptionsFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        kind: crate::state::PrPropertyKind,
        request_id: u64,
        error: String,
    },
    PropertyEditSucceeded {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        kind: crate::state::PrPropertyKind,
        request_id: u64,
    },
    /// Consume a queued PR refresh immediately before orchestration starts it.
    PostMutationRefreshStarted,
    PropertyEditFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        kind: crate::state::PrPropertyKind,
        request_id: u64,
        error: String,
    },
    /// Synchronous validation error set directly on the open editor (issue #175).
    PropertyEditorValidationError {
        kind: crate::state::PrPropertyKind,
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
///
/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-008
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
    ///
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
    Errors(ErrorsMessage),
    System(SystemMessage),
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
            Self::Errors(_) => MessageDomain::Errors,
            Self::System(_) => MessageDomain::System,
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
            Self::Errors(message) => message.name(),
            Self::System(message) => message.name(),
        }
    }
}
