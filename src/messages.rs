//! Domain-scoped internal message bus.
//!
//! The UI can keep producing the historical [`crate::state::AppEvent`] facade,
//! while reducers and dispatch code route through typed domain messages. New
//! behavior should be added to the smallest domain message enum rather than to
//! app-shell-specific branching.

use crate::domain::{
    AgentId, AgentStatus, Issue, IssueComment, IssueDetail, IssueFilter, RepositoryId,
};
use crate::state::AppEvent;
use crate::state::{EditorTarget, InlineState};

mod issues_conversion;

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
}

/// Modal and form-editing messages.
#[derive(Debug, Clone)]
pub enum ModalMessage {
    OpenHelp,
    OpenSearch,
    CloseModal,
    SubmitForm,
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
});

message_names!(ModalMessage {
    Self::OpenHelp => "OpenHelp",
    Self::OpenSearch => "OpenSearch",
    Self::CloseModal => "CloseModal",
    Self::SubmitForm => "SubmitForm",
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
    Self::OpenAgentChooser => "OpenAgentChooser",
    Self::AgentChooserNavigateUp => "AgentChooserNavigateUp",
    Self::AgentChooserNavigateDown => "AgentChooserNavigateDown",
    Self::AgentChooserConfirm => "AgentChooserConfirm",
    Self::AgentChooserCancel => "AgentChooserCancel",
    Self::SendToAgentCompleted => "SendToAgentCompleted",
    Self::SendToAgentFailed { .. } => "SendToAgentFailed",
});

impl From<AppEvent> for AppMessage {
    fn from(event: AppEvent) -> Self {
        match event {
            AppEvent::NavigateUp => Self::UiNavigation(UiNavigationMessage::NavigateUp),
            AppEvent::NavigateDown => Self::UiNavigation(UiNavigationMessage::NavigateDown),
            AppEvent::NavigateLeft => Self::UiNavigation(UiNavigationMessage::NavigateLeft),
            AppEvent::NavigateRight => Self::UiNavigation(UiNavigationMessage::NavigateRight),
            AppEvent::SelectRepository(index) => {
                Self::UiNavigation(UiNavigationMessage::SelectRepository(index))
            }
            AppEvent::SelectAgent(index) => {
                Self::UiNavigation(UiNavigationMessage::SelectAgent(index))
            }
            AppEvent::JumpToAgentByShortcut(slot) => {
                Self::UiNavigation(UiNavigationMessage::JumpToAgentByShortcut(slot))
            }
            AppEvent::CyclePaneFocus => Self::UiNavigation(UiNavigationMessage::CyclePaneFocus),
            AppEvent::ToggleTerminalFocus => {
                Self::UiNavigation(UiNavigationMessage::ToggleTerminalFocus)
            }
            AppEvent::ToggleHideIdleRepositories => {
                Self::UiNavigation(UiNavigationMessage::ToggleHideIdleRepositories)
            }
            AppEvent::EnterSplitMode => Self::UiNavigation(UiNavigationMessage::EnterSplitMode),
            AppEvent::ExitSplitMode => Self::UiNavigation(UiNavigationMessage::ExitSplitMode),
            AppEvent::EnterGrabMode => Self::UiNavigation(UiNavigationMessage::EnterGrabMode),
            AppEvent::ExitGrabMode => Self::UiNavigation(UiNavigationMessage::ExitGrabMode),
            AppEvent::GrabMoveUp => Self::UiNavigation(UiNavigationMessage::GrabMoveUp),
            AppEvent::GrabMoveDown => Self::UiNavigation(UiNavigationMessage::GrabMoveDown),
            AppEvent::SetSplitFilter(filter) => {
                Self::UiNavigation(UiNavigationMessage::SetSplitFilter(filter))
            }
            AppEvent::OpenHelp => Self::Modal(ModalMessage::OpenHelp),
            AppEvent::OpenSearch => Self::Modal(ModalMessage::OpenSearch),
            AppEvent::CloseModal => Self::Modal(ModalMessage::CloseModal),
            AppEvent::SubmitForm => Self::Modal(ModalMessage::SubmitForm),
            AppEvent::FormChar(c) => Self::Modal(ModalMessage::FormChar(c)),
            AppEvent::FormBackspace => Self::Modal(ModalMessage::FormBackspace),
            AppEvent::FormDelete => Self::Modal(ModalMessage::FormDelete),
            AppEvent::FormMoveCursorLeft => Self::Modal(ModalMessage::FormMoveCursorLeft),
            AppEvent::FormMoveCursorRight => Self::Modal(ModalMessage::FormMoveCursorRight),
            AppEvent::FormNextField => Self::Modal(ModalMessage::FormNextField),
            AppEvent::FormPrevField => Self::Modal(ModalMessage::FormPrevField),
            AppEvent::FormToggleCheckbox => Self::Modal(ModalMessage::FormToggleCheckbox),
            other => Self::from_non_ui_nav_event(other),
        }
    }
}

impl AppMessage {
    /// Convert non-UI-navigation [`AppEvent`] variants into the typed message bus.
    ///
    /// Split out from [`AppMessage::from`] so the top-level converter stays
    /// within the clippy line budget without a complexity suppression.
    fn from_non_ui_nav_event(event: AppEvent) -> Self {
        match event {
            AppEvent::OpenNewRepository => {
                Self::RepositoryAgent(RepositoryAgentMessage::OpenNewRepository)
            }
            AppEvent::OpenEditRepository(id) => {
                Self::RepositoryAgent(RepositoryAgentMessage::OpenEditRepository(id))
            }
            AppEvent::OpenDeleteRepository(id) => {
                Self::RepositoryAgent(RepositoryAgentMessage::OpenDeleteRepository(id))
            }
            AppEvent::OpenNewAgent(id) => {
                Self::RepositoryAgent(RepositoryAgentMessage::OpenNewAgent(id))
            }
            AppEvent::OpenEditAgent(id) => {
                Self::RepositoryAgent(RepositoryAgentMessage::OpenEditAgent(id))
            }
            AppEvent::OpenDeleteAgent(id) => {
                Self::RepositoryAgent(RepositoryAgentMessage::OpenDeleteAgent(id))
            }
            AppEvent::ToggleDeleteWorkDir => {
                Self::RepositoryAgent(RepositoryAgentMessage::ToggleDeleteWorkDir)
            }
            AppEvent::KillAgent(id) => Self::Runtime(RuntimeMessage::KillAgent(id)),
            AppEvent::RelaunchAgent(id) => Self::Runtime(RuntimeMessage::RelaunchAgent(id)),
            AppEvent::AgentStatusChanged(id, status) => {
                Self::Runtime(RuntimeMessage::AgentStatusChanged(id, status))
            }
            AppEvent::PersistenceLoadSuccess => Self::Persistence(PersistenceMessage::LoadSuccess),
            AppEvent::PersistenceLoadFailed(error) => {
                Self::Persistence(PersistenceMessage::LoadFailed(error))
            }
            AppEvent::PersistenceSaveSuccess => Self::Persistence(PersistenceMessage::SaveSuccess),
            AppEvent::PersistenceSaveFailed(error) => {
                Self::Persistence(PersistenceMessage::SaveFailed(error))
            }
            AppEvent::SetTheme(theme) => Self::Theme(ThemeMessage::SetTheme(theme)),
            AppEvent::ThemeResolveFailed(error) => Self::Theme(ThemeMessage::ResolveFailed(error)),
            AppEvent::Quit => Self::System(SystemMessage::Quit),
            AppEvent::ClearError => Self::System(SystemMessage::ClearError),
            AppEvent::ClearWarning => Self::System(SystemMessage::ClearWarning),
            other => Self::from_issues_event(other),
        }
    }

    /// Convert issues-domain [`AppEvent`] variants into the typed message bus.
    fn from_issues_event(event: AppEvent) -> Self {
        Self::Issues(IssuesMessage::from_app_event(event))
    }
}

impl From<AppMessage> for AppEvent {
    fn from(message: AppMessage) -> Self {
        match message {
            AppMessage::UiNavigation(message) => message.into(),
            AppMessage::Modal(message) => message.into(),
            AppMessage::RepositoryAgent(message) => message.into(),
            AppMessage::Runtime(message) => message.into(),
            AppMessage::Persistence(message) => message.into(),
            AppMessage::Theme(message) => message.into(),
            AppMessage::Issues(message) => message.into(),
            AppMessage::System(message) => message.into(),
        }
    }
}

impl From<UiNavigationMessage> for AppEvent {
    fn from(message: UiNavigationMessage) -> Self {
        match message {
            UiNavigationMessage::NavigateUp => Self::NavigateUp,
            UiNavigationMessage::NavigateDown => Self::NavigateDown,
            UiNavigationMessage::NavigateLeft => Self::NavigateLeft,
            UiNavigationMessage::NavigateRight => Self::NavigateRight,
            UiNavigationMessage::SelectRepository(index) => Self::SelectRepository(index),
            UiNavigationMessage::SelectAgent(index) => Self::SelectAgent(index),
            UiNavigationMessage::JumpToAgentByShortcut(slot) => Self::JumpToAgentByShortcut(slot),
            UiNavigationMessage::CyclePaneFocus => Self::CyclePaneFocus,
            UiNavigationMessage::ToggleTerminalFocus => Self::ToggleTerminalFocus,
            UiNavigationMessage::ToggleHideIdleRepositories => Self::ToggleHideIdleRepositories,
            UiNavigationMessage::EnterSplitMode => Self::EnterSplitMode,
            UiNavigationMessage::ExitSplitMode => Self::ExitSplitMode,
            UiNavigationMessage::EnterGrabMode => Self::EnterGrabMode,
            UiNavigationMessage::ExitGrabMode => Self::ExitGrabMode,
            UiNavigationMessage::GrabMoveUp => Self::GrabMoveUp,
            UiNavigationMessage::GrabMoveDown => Self::GrabMoveDown,
            UiNavigationMessage::SetSplitFilter(filter) => Self::SetSplitFilter(filter),
        }
    }
}

impl From<ModalMessage> for AppEvent {
    fn from(message: ModalMessage) -> Self {
        match message {
            ModalMessage::OpenHelp => Self::OpenHelp,
            ModalMessage::OpenSearch => Self::OpenSearch,
            ModalMessage::CloseModal => Self::CloseModal,
            ModalMessage::SubmitForm => Self::SubmitForm,
            ModalMessage::FormChar(c) => Self::FormChar(c),
            ModalMessage::FormBackspace => Self::FormBackspace,
            ModalMessage::FormDelete => Self::FormDelete,
            ModalMessage::FormMoveCursorLeft => Self::FormMoveCursorLeft,
            ModalMessage::FormMoveCursorRight => Self::FormMoveCursorRight,
            ModalMessage::FormNextField => Self::FormNextField,
            ModalMessage::FormPrevField => Self::FormPrevField,
            ModalMessage::FormToggleCheckbox => Self::FormToggleCheckbox,
        }
    }
}

impl From<RepositoryAgentMessage> for AppEvent {
    fn from(message: RepositoryAgentMessage) -> Self {
        match message {
            RepositoryAgentMessage::OpenNewRepository => Self::OpenNewRepository,
            RepositoryAgentMessage::OpenEditRepository(id) => Self::OpenEditRepository(id),
            RepositoryAgentMessage::OpenDeleteRepository(id) => Self::OpenDeleteRepository(id),
            RepositoryAgentMessage::OpenNewAgent(id) => Self::OpenNewAgent(id),
            RepositoryAgentMessage::OpenEditAgent(id) => Self::OpenEditAgent(id),
            RepositoryAgentMessage::OpenDeleteAgent(id) => Self::OpenDeleteAgent(id),
            RepositoryAgentMessage::ToggleDeleteWorkDir => Self::ToggleDeleteWorkDir,
        }
    }
}

impl From<RuntimeMessage> for AppEvent {
    fn from(message: RuntimeMessage) -> Self {
        match message {
            RuntimeMessage::KillAgent(id) => Self::KillAgent(id),
            RuntimeMessage::RelaunchAgent(id) => Self::RelaunchAgent(id),
            RuntimeMessage::AgentStatusChanged(id, status) => Self::AgentStatusChanged(id, status),
        }
    }
}

impl From<PersistenceMessage> for AppEvent {
    fn from(message: PersistenceMessage) -> Self {
        match message {
            PersistenceMessage::LoadSuccess => Self::PersistenceLoadSuccess,
            PersistenceMessage::LoadFailed(error) => Self::PersistenceLoadFailed(error),
            PersistenceMessage::SaveSuccess => Self::PersistenceSaveSuccess,
            PersistenceMessage::SaveFailed(error) => Self::PersistenceSaveFailed(error),
        }
    }
}

impl From<ThemeMessage> for AppEvent {
    fn from(message: ThemeMessage) -> Self {
        match message {
            ThemeMessage::SetTheme(theme) => Self::SetTheme(theme),
            ThemeMessage::ResolveFailed(error) => Self::ThemeResolveFailed(error),
        }
    }
}

impl From<SystemMessage> for AppEvent {
    fn from(message: SystemMessage) -> Self {
        match message {
            SystemMessage::Quit => Self::Quit,
            SystemMessage::ClearError => Self::ClearError,
            SystemMessage::ClearWarning => Self::ClearWarning,
        }
    }
}
