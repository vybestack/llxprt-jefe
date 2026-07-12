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
use crate::state::{EditorTarget, InlineState, ReadOnlyHintKind};

mod issues_conversion;
// @plan PLAN-20260624-PR-MODE.P03
// @requirement REQ-PR-002
mod actions;
mod actions_conversion;
mod prs_conversion;
pub use actions::ActionsMessage;

// @plan PLAN-20260624-PR-MODE.P03
// @requirement REQ-PR-002
// @pseudocode component-004 lines 46-50
mod event_conversion;

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
    NavigatePageUp,
    NavigatePageDown,
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
        issue_number: u64,
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
    IssueClosed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        mutation_id: u64,
    },
    IssueDeleted {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        mutation_id: u64,
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
    /// Non-blocking self-assignment failure warning (issue #186).
    IssueSelfAssignmentFailed {
        owner_repo: String,
        issue_number: u64,
        error: String,
    },
}

/// Pull Requests mode messages — mirrors `IssuesMessage` shape.
///
/// @plan PLAN-20260624-PR-MODE.P03
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
    OpenAgentChooser,
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
}

/// Navigation direction for PR list and filter controls.
///
/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-003
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavDir {
    Up,
    Down,
    PageUp,
    PageDown,
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
            Self::System(message) => message.name(),
        }
    }
}

macro_rules! message_names {
    ($enum_name:ident { $($variant:pat => $name:literal),+ $(,)? }) => {
        impl $enum_name {
            #[must_use]
            pub const fn name(&self) -> &'static str {
                match self {
                    $($variant => $name,)+
                }
            }
        }
    };
}

message_names!(UiNavigationMessage {
    Self::NavigateUp => "NavigateUp",
    Self::NavigateDown => "NavigateDown",
    Self::NavigateLeft => "NavigateLeft",
    Self::NavigateRight => "NavigateRight",
    Self::SelectRepository(_) => "SelectRepository",
    Self::SelectAgent(_) => "SelectAgent",
    Self::JumpToAgentByShortcut(_) => "JumpToAgentByShortcut",
    Self::CyclePaneFocus => "CyclePaneFocus",
    Self::ToggleTerminalFocus => "ToggleTerminalFocus",
    Self::ToggleHideIdleRepositories => "ToggleHideIdleRepositories",
    Self::EnterSplitMode => "EnterSplitMode",
    Self::ExitSplitMode => "ExitSplitMode",
    Self::EnterGrabMode => "EnterGrabMode",
    Self::ExitGrabMode => "ExitGrabMode",
    Self::GrabMoveUp => "GrabMoveUp",
    Self::GrabMoveDown => "GrabMoveDown",
    Self::SetSplitFilter(_) => "SetSplitFilter",
    Self::EnterDashboardGrab => "EnterDashboardGrab",
    Self::ExitDashboardGrab => "ExitDashboardGrab",
    Self::DashboardGrabMoveUp => "DashboardGrabMoveUp",
    Self::DashboardGrabMoveDown => "DashboardGrabMoveDown",
    Self::TerminalScrollUp => "TerminalScrollUp",
    Self::TerminalScrollDown => "TerminalScrollDown",
    Self::TerminalScrollPageUp => "TerminalScrollPageUp",
    Self::TerminalScrollPageDown => "TerminalScrollPageDown",
    Self::TerminalFollowTail => "TerminalFollowTail",
    Self::TerminalScrollToTop => "TerminalScrollToTop",
});

message_names!(ModalMessage {
    Self::OpenHelp => "OpenHelp",
    Self::OpenSearch => "OpenSearch",
    Self::CloseModal => "CloseModal",
    Self::SubmitForm => "SubmitForm",
    Self::ConfirmCycleFocus => "ConfirmCycleFocus",
    Self::FormChar(_) => "FormChar",
    Self::FormBackspace => "FormBackspace",
    Self::FormDelete => "FormDelete",
    Self::FormMoveCursorLeft => "FormMoveCursorLeft",
    Self::FormMoveCursorRight => "FormMoveCursorRight",
    Self::FormNextField => "FormNextField",
    Self::FormPrevField => "FormPrevField",
    Self::FormToggleCheckbox => "FormToggleCheckbox",
});

message_names!(RepositoryAgentMessage {
    Self::OpenNewRepository => "OpenNewRepository",
    Self::OpenEditRepository(_) => "OpenEditRepository",
    Self::OpenDeleteRepository(_) => "OpenDeleteRepository",
    Self::OpenNewAgent(_) => "OpenNewAgent",
    Self::OpenEditAgent(_) => "OpenEditAgent",
    Self::OpenDeleteAgent(_) => "OpenDeleteAgent",
    Self::ToggleDeleteWorkDir => "ToggleDeleteWorkDir",
});

message_names!(RuntimeMessage {
    Self::KillAgent(_) => "KillAgent",
    Self::RelaunchAgent(_) => "RelaunchAgent",
    Self::RestartAgent(_) => "RestartAgent",
    Self::AgentStatusChanged(_, _) => "AgentStatusChanged",
});

message_names!(PersistenceMessage {
    Self::LoadSuccess => "PersistenceLoadSuccess",
    Self::LoadFailed(_) => "PersistenceLoadFailed",
    Self::SaveSuccess => "PersistenceSaveSuccess",
    Self::SaveFailed(_) => "PersistenceSaveFailed",
});

message_names!(ThemeMessage {
    Self::SetTheme(_) => "SetTheme",
    Self::ResolveFailed(_) => "ThemeResolveFailed",
    Self::OpenThemePicker { .. } => "OpenThemePicker",
    Self::PickerNavigateUp => "ThemePickerNavigateUp",
    Self::PickerNavigateDown => "ThemePickerNavigateDown",
    Self::PickerConfirm => "ThemePickerConfirm",
    Self::PickerCancel => "CloseThemePicker",
    Self::ToggleAgentThemeOverride => "ThemePickerToggleOverride",
});

message_names!(SystemMessage {
    Self::Quit => "Quit",
    Self::ClearError => "ClearError",
    Self::ClearWarning => "ClearWarning",
});

message_names!(IssuesMessage {
    Self::EnterMode => "EnterIssuesMode",
    Self::ExitMode => "ExitIssuesMode",
    Self::RefocusList => "RefocusIssueList",
    Self::NavigateUp => "IssuesNavigateUp",
    Self::NavigateDown => "IssuesNavigateDown",
    Self::NavigatePageUp => "IssuesNavigatePageUp",
    Self::NavigatePageDown => "IssuesNavigatePageDown",
    Self::NavigateHome => "IssuesNavigateHome",
    Self::NavigateEnd => "IssuesNavigateEnd",
    Self::Enter => "IssuesEnter",
    Self::CycleFocus => "IssuesCycleFocus",
    Self::CycleFocusReverse => "IssuesCycleFocusReverse",
    Self::ScrollDetailUp => "IssuesScrollDetailUp",
    Self::ScrollDetailDown => "IssuesScrollDetailDown",
    Self::ScrollDetailPageUp => "IssuesScrollDetailPageUp",
    Self::ScrollDetailPageDown => "IssuesScrollDetailPageDown",
    Self::DetailSubfocusNext => "IssueDetailSubfocusNext",
    Self::DetailSubfocusPrev => "IssueDetailSubfocusPrev",
    Self::ListLoaded { .. } => "IssueListLoaded",
    Self::ListLoadFailed { .. } => "IssueListLoadFailed",
    Self::ListPageLoaded { .. } => "IssueListPageLoaded",
    Self::DetailLoaded { .. } => "IssueDetailLoaded",
    Self::DetailLoadFailed { .. } => "IssueDetailLoadFailed",
    Self::CommentsPageLoaded { .. } => "IssueCommentsPageLoaded",
    Self::CommentsPageFailed { .. } => "IssueCommentsPageFailed",
    Self::OpenFilterControls => "OpenFilterControls",
    Self::CloseFilterControls => "CloseFilterControls",
    Self::ApplyFilter => "ApplyFilter",
    Self::ClearFilter => "ClearFilter",
    Self::ClearDraftFilter => "ClearDraftFilter",
    Self::FilterNavigateNext => "FilterNavigateNext",
    Self::FilterNavigatePrev => "FilterNavigatePrev",
    Self::CycleFilterState => "CycleFilterState",
    Self::FocusSearchInput => "FocusSearchInput",
    Self::BlurSearchInput => "BlurSearchInput",
    Self::SetSearchQuery { .. } => "SetSearchQuery",
    Self::ApplySearch => "ApplySearch",
    Self::ClearSearch => "ClearSearch",
    Self::UpdateDraftFilter { .. } => "UpdateDraftFilter",
    Self::OpenNewIssueComposer => "OpenNewIssueComposer",
    Self::OpenNewCommentComposer => "OpenNewCommentComposer",
    Self::OpenReplyComposer { .. } => "OpenReplyComposer",
    Self::OpenInlineEditor { .. } => "OpenInlineEditor",
    Self::InlineChar(_) => "InlineChar",
    Self::InlineNewline => "InlineNewline",
    Self::InlineBackspace => "InlineBackspace",
    Self::InlineDelete => "InlineDelete",
    Self::InlineCursorLeft => "InlineCursorLeft",
    Self::InlineCursorRight => "InlineCursorRight",
    Self::InlineCursorUp => "InlineCursorUp",
    Self::InlineCursorDown => "InlineCursorDown",
    Self::InlineSubmit => "InlineSubmit",
    Self::InlineCancelOrEsc => "InlineCancelOrEsc",
            Self::MutationSubmitted { .. } => "MutationSubmitted",
    Self::IssueCreated { .. } => "IssueCreated",
            Self::CommentCreated { .. } => "CommentCreated",
            Self::CommentCreateFailed { .. } => "CommentCreateFailed",
    Self::IssueBodyUpdated { .. } => "IssueBodyUpdated",
    Self::CommentUpdated { .. } => "CommentUpdated",
    Self::MutationFailed { .. } => "MutationFailed",
    Self::CloseIssue => "CloseIssue",
    Self::OpenDeleteIssueConfirm => "OpenDeleteIssueConfirm",
    Self::IssueDeleteConfirm => "IssueDeleteConfirm",
    Self::IssueDeleteCancel => "IssueDeleteCancel",
    Self::IssueClosed { .. } => "IssueClosed",
    Self::IssueDeleted { .. } => "IssueDeleted",
    Self::OpenAgentChooser => "OpenAgentChooser",
    Self::AgentChooserNavigateUp => "AgentChooserNavigateUp",
    Self::AgentChooserNavigateDown => "AgentChooserNavigateDown",
    Self::AgentChooserConfirm => "AgentChooserConfirm",
    Self::AgentChooserCancel => "AgentChooserCancel",
    Self::SendToAgentCompleted => "SendToAgentCompleted",
    Self::SendToAgentFailed { .. } => "SendToAgentFailed",
    Self::IssueSelfAssignmentFailed { .. } => "IssueSelfAssignmentFailed",
});

// @plan PLAN-20260624-PR-MODE.P03
// @requirement REQ-PR-002
// @pseudocode component-004 lines 43-44
message_names!(PullRequestsMessage {
    Self::EnterMode => "EnterPrsMode",
    Self::ExitMode => "ExitPrsMode",
    Self::RefocusList => "RefocusPrList",
    Self::Navigate(_) => "PrNavigate",
    Self::Enter => "PrListEnter",
    Self::CycleFocus => "PrCycleFocus",
    Self::CycleFocusReverse => "PrCycleFocusReverse",
    Self::ScrollDetail(_) => "PrScrollDetail",
    Self::DetailSubfocusNext => "PrDetailSubfocusNext",
    Self::DetailSubfocusPrev => "PrDetailSubfocusPrev",
    Self::ListLoaded { .. } => "PrListLoaded",
    Self::ListLoadFailed { .. } => "PrListLoadFailed",
    Self::ListPageLoaded { .. } => "PrListPageLoaded",
    Self::ListSilentRefreshed { .. } => "PrListSilentRefreshed",
    Self::ListSilentRefreshFailed { .. } => "PrListSilentRefreshFailed",
    Self::DetailLoaded { .. } => "PrDetailLoaded",
    Self::DetailLoadFailed { .. } => "PrDetailLoadFailed",
    Self::DetailSilentRefreshed { .. } => "PrDetailSilentRefreshed",
    Self::DetailSilentRefreshFailed { .. } => "PrDetailSilentRefreshFailed",
    Self::CommentsPageLoaded { .. } => "PrCommentsPageLoaded",
    Self::CommentsPageFailed { .. } => "PrCommentsPageFailed",
    Self::OpenFilterControls => "PrOpenFilterControls",
    Self::CloseFilterControls => "PrCloseFilterControls",
    Self::ApplyFilter => "PrApplyFilter",
    Self::ClearFilter => "PrClearFilter",
    Self::FilterNavigate(_) => "PrFilterNavigate",
    Self::CycleFilterState => "PrCycleFilterState",
    Self::CycleDraftFilter => "PrCycleDraftFilter",
    Self::CycleReviewFilter => "PrCycleReviewFilter",
    Self::CycleChecksFilter => "PrCycleChecksFilter",
    Self::UpdateDraftFilter { .. } => "PrUpdateDraftFilter",
    Self::FocusSearchInput => "PrFocusSearchInput",
    Self::BlurSearchInput => "PrBlurSearchInput",
    Self::SetSearchQuery { .. } => "PrSetSearchQuery",
    Self::ApplySearch => "PrApplySearch",
    Self::ClearSearch => "PrClearSearch",
    Self::OpenNewCommentComposer => "PrOpenNewCommentComposer",
    Self::OpenReplyComposer { .. } => "PrOpenReplyComposer",
    Self::Inline(_) => "PrInline",
    Self::CommentCreated { .. } => "PrCommentCreated",
    Self::CommentCreateFailed { .. } => "PrCommentCreateFailed",
    Self::MutationFailed { .. } => "PrMutationFailed",
    Self::ShowNotice(_) => "PrShowNotice",
    Self::OpenAgentChooser => "PrOpenAgentChooser",
    Self::AgentChooserNavigate(_) => "PrAgentChooserNavigate",
    Self::AgentChooserConfirm => "PrAgentChooserConfirm",
    Self::AgentChooserCancel => "PrAgentChooserCancel",
    Self::SendToAgentCompleted => "PrSendToAgentCompleted",
    Self::SendToAgentFailed { .. } => "PrSendToAgentFailed",
    Self::OpenInBrowser => "PrOpenInBrowser",
    Self::OpenedInBrowser { .. } => "PrOpenedInBrowser",
    Self::OpenInBrowserFailed { .. } => "PrOpenInBrowserFailed",
    Self::OpenMergeChooser => "PrOpenMergeChooser",
    Self::MergeNavigate(_) => "PrMergeNavigate",
    Self::MergeConfirm => "PrMergeConfirm",
    Self::MergeCancel => "PrMergeCancel",
    Self::Merged { .. } => "PrMerged",
    Self::MergeFailed { .. } => "PrMergeFailed",
    Self::MergeMethodsLoaded { .. } => "PrMergeMethodsLoaded",
    Self::OpenThreadReply { .. } => "PrOpenThreadReply",
    Self::ToggleThreadResolve { .. } => "PrToggleThreadResolve",
    Self::ThreadResolveSucceeded { .. } => "PrThreadResolveSucceeded",
    Self::ThreadResolveFailed { .. } => "PrThreadResolveFailed",
});
