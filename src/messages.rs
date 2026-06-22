//! Domain-scoped internal message bus.
//!
//! The UI can keep producing the historical [`crate::state::AppEvent`] facade,
//! while reducers and dispatch code route through typed domain messages. New
//! behavior should be added to the smallest domain message enum rather than to
//! app-shell-specific branching.

use crate::domain::{AgentId, AgentStatus, Issue, IssueComment, IssueDetail, RepositoryId};
use crate::state::AppEvent;
use crate::state::EditorTarget;

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
        issues: Vec<Issue>,
        cursor: Option<String>,
        has_more: bool,
    },
    ListLoadFailed {
        scope_repo_id: RepositoryId,
        error: String,
    },
    ListPageLoaded {
        scope_repo_id: RepositoryId,
        issues: Vec<Issue>,
        cursor: Option<String>,
        has_more: bool,
    },
    DetailLoaded {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        detail: Box<IssueDetail>,
    },
    DetailLoadFailed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        error: String,
    },
    CommentsPageLoaded {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        comments: Vec<IssueComment>,
        cursor: Option<String>,
        has_more: bool,
    },
    CommentsPageFailed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
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
    CommentCreated {
        comment: IssueComment,
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

impl From<IssuesMessage> for AppEvent {
    fn from(message: IssuesMessage) -> Self {
        message.into_app_event()
    }
}

impl IssuesMessage {
    /// Convert an issues-domain [`AppEvent`] into the typed message.
    ///
    /// `from_issues_event` only routes issues variants here; the exhaustive
    /// fallback is split across focused helpers to stay within the clippy line
    /// budget.
    fn from_app_event(event: AppEvent) -> Self {
        match event {
            AppEvent::EnterIssuesMode
            | AppEvent::ExitIssuesMode
            | AppEvent::RefocusIssueList
            | AppEvent::IssuesNavigateUp
            | AppEvent::IssuesNavigateDown
            | AppEvent::IssuesNavigatePageUp
            | AppEvent::IssuesNavigatePageDown
            | AppEvent::IssuesNavigateHome
            | AppEvent::IssuesNavigateEnd
            | AppEvent::IssuesEnter
            | AppEvent::IssuesCycleFocus
            | AppEvent::IssuesCycleFocusReverse
            | AppEvent::IssuesScrollDetailUp
            | AppEvent::IssuesScrollDetailDown
            | AppEvent::IssuesScrollDetailPageUp
            | AppEvent::IssuesScrollDetailPageDown
            | AppEvent::IssueDetailSubfocusNext
            | AppEvent::IssueDetailSubfocusPrev => Self::from_app_event_navigation(event),
            other => Self::from_app_event_payload(other),
        }
    }

    /// Navigation and scroll events that carry no payload.
    fn from_app_event_navigation(event: AppEvent) -> Self {
        match event {
            AppEvent::EnterIssuesMode => Self::EnterMode,
            AppEvent::ExitIssuesMode => Self::ExitMode,
            AppEvent::RefocusIssueList => Self::RefocusList,
            AppEvent::IssuesNavigateUp => Self::NavigateUp,
            AppEvent::IssuesNavigateDown => Self::NavigateDown,
            AppEvent::IssuesNavigatePageUp => Self::NavigatePageUp,
            AppEvent::IssuesNavigatePageDown => Self::NavigatePageDown,
            AppEvent::IssuesNavigateHome => Self::NavigateHome,
            AppEvent::IssuesNavigateEnd => Self::NavigateEnd,
            AppEvent::IssuesEnter => Self::Enter,
            AppEvent::IssuesCycleFocus => Self::CycleFocus,
            AppEvent::IssuesCycleFocusReverse => Self::CycleFocusReverse,
            AppEvent::IssuesScrollDetailUp => Self::ScrollDetailUp,
            AppEvent::IssuesScrollDetailDown => Self::ScrollDetailDown,
            AppEvent::IssuesScrollDetailPageUp => Self::ScrollDetailPageUp,
            AppEvent::IssuesScrollDetailPageDown => Self::ScrollDetailPageDown,
            AppEvent::IssueDetailSubfocusNext => Self::DetailSubfocusNext,
            AppEvent::IssueDetailSubfocusPrev => Self::DetailSubfocusPrev,
            _ => unreachable!("non-navigation AppEvent routed to navigation converter"),
        }
    }

    /// Loaded/error payload events and the remaining issues mutations.
    fn from_app_event_payload(event: AppEvent) -> Self {
        match event {
            AppEvent::IssueListLoaded {
                scope_repo_id,
                issues,
                cursor,
                has_more,
            } => Self::ListLoaded {
                scope_repo_id,
                issues,
                cursor,
                has_more,
            },
            AppEvent::IssueListLoadFailed {
                scope_repo_id,
                error,
            } => Self::ListLoadFailed {
                scope_repo_id,
                error,
            },
            AppEvent::IssueListPageLoaded {
                scope_repo_id,
                issues,
                cursor,
                has_more,
            } => Self::ListPageLoaded {
                scope_repo_id,
                issues,
                cursor,
                has_more,
            },
            AppEvent::IssueDetailLoaded {
                scope_repo_id,
                issue_number,
                detail,
            } => Self::DetailLoaded {
                scope_repo_id,
                issue_number,
                detail,
            },
            AppEvent::IssueDetailLoadFailed {
                scope_repo_id,
                issue_number,
                error,
            } => Self::DetailLoadFailed {
                scope_repo_id,
                issue_number,
                error,
            },
            other => Self::from_app_event_comments_and_controls(other),
        }
    }

    /// Comments payloads, then controls.
    fn from_app_event_comments_and_controls(event: AppEvent) -> Self {
        match event {
            AppEvent::IssueCommentsPageLoaded {
                scope_repo_id,
                issue_number,
                comments,
                cursor,
                has_more,
            } => Self::CommentsPageLoaded {
                scope_repo_id,
                issue_number,
                comments,
                cursor,
                has_more,
            },
            AppEvent::IssueCommentsPageFailed {
                scope_repo_id,
                issue_number,
                error,
            } => Self::CommentsPageFailed {
                scope_repo_id,
                issue_number,
                error,
            },
            other => Self::from_app_event_controls(other),
        }
    }

    /// Filter controls, search, composer, inline editor, and chooser events.
    fn from_app_event_controls(event: AppEvent) -> Self {
        match event {
            AppEvent::OpenFilterControls => Self::OpenFilterControls,
            AppEvent::CloseFilterControls => Self::CloseFilterControls,
            AppEvent::ApplyFilter => Self::ApplyFilter,
            AppEvent::ClearFilter => Self::ClearFilter,
            AppEvent::FilterNavigateNext => Self::FilterNavigateNext,
            AppEvent::FilterNavigatePrev => Self::FilterNavigatePrev,
            AppEvent::CycleFilterState => Self::CycleFilterState,
            AppEvent::FocusSearchInput => Self::FocusSearchInput,
            AppEvent::BlurSearchInput => Self::BlurSearchInput,
            AppEvent::SetSearchQuery { query } => Self::SetSearchQuery { query },
            AppEvent::ApplySearch => Self::ApplySearch,
            AppEvent::ClearSearch => Self::ClearSearch,
            AppEvent::UpdateDraftFilter { field, value } => {
                Self::UpdateDraftFilter { field, value }
            }
            AppEvent::OpenNewIssueComposer => Self::OpenNewIssueComposer,
            AppEvent::OpenNewCommentComposer => Self::OpenNewCommentComposer,
            AppEvent::OpenReplyComposer { comment_index } => {
                Self::OpenReplyComposer { comment_index }
            }
            AppEvent::OpenInlineEditor { target } => Self::OpenInlineEditor { target },
            AppEvent::InlineChar(c) => Self::InlineChar(c),
            AppEvent::InlineNewline => Self::InlineNewline,
            AppEvent::InlineBackspace => Self::InlineBackspace,
            AppEvent::InlineDelete => Self::InlineDelete,
            AppEvent::InlineCursorLeft => Self::InlineCursorLeft,
            AppEvent::InlineCursorRight => Self::InlineCursorRight,
            AppEvent::InlineCursorUp => Self::InlineCursorUp,
            AppEvent::InlineCursorDown => Self::InlineCursorDown,
            AppEvent::InlineSubmit => Self::InlineSubmit,
            AppEvent::InlineCancelOrEsc => Self::InlineCancelOrEsc,
            AppEvent::CommentCreated { comment } => Self::CommentCreated { comment },
            AppEvent::CommentCreateFailed { error } => Self::CommentCreateFailed { error },
            AppEvent::IssueBodyUpdated { body } => Self::IssueBodyUpdated { body },
            AppEvent::CommentUpdated {
                comment_index,
                body,
            } => Self::CommentUpdated {
                comment_index,
                body,
            },
            AppEvent::MutationFailed { error } => Self::MutationFailed { error },
            AppEvent::OpenAgentChooser => Self::OpenAgentChooser,
            AppEvent::AgentChooserNavigateUp => Self::AgentChooserNavigateUp,
            AppEvent::AgentChooserNavigateDown => Self::AgentChooserNavigateDown,
            AppEvent::AgentChooserConfirm => Self::AgentChooserConfirm,
            AppEvent::AgentChooserCancel => Self::AgentChooserCancel,
            AppEvent::SendToAgentCompleted => Self::SendToAgentCompleted,
            AppEvent::SendToAgentFailed { error } => Self::SendToAgentFailed { error },
            // All issues variants are covered above; other domains never reach here.
            _ => unreachable!("non-issues AppEvent routed to issues converter"),
        }
    }

    /// Convert this issues-domain message back into the historical [`AppEvent`].
    ///
    /// Delegates to focused helpers so each converter stays within the clippy
    /// line budget without a complexity suppression.
    fn into_app_event(self) -> AppEvent {
        match self {
            Self::EnterMode
            | Self::ExitMode
            | Self::RefocusList
            | Self::NavigateUp
            | Self::NavigateDown
            | Self::NavigatePageUp
            | Self::NavigatePageDown
            | Self::NavigateHome
            | Self::NavigateEnd
            | Self::Enter
            | Self::CycleFocus
            | Self::CycleFocusReverse
            | Self::ScrollDetailUp
            | Self::ScrollDetailDown
            | Self::ScrollDetailPageUp
            | Self::ScrollDetailPageDown
            | Self::DetailSubfocusNext
            | Self::DetailSubfocusPrev => self.into_app_event_navigation(),
            other => other.into_app_event_data(),
        }
    }

    /// Navigation and scroll messages that carry no payload.
    fn into_app_event_navigation(self) -> AppEvent {
        match self {
            Self::EnterMode => AppEvent::EnterIssuesMode,
            Self::ExitMode => AppEvent::ExitIssuesMode,
            Self::RefocusList => AppEvent::RefocusIssueList,
            Self::NavigateUp => AppEvent::IssuesNavigateUp,
            Self::NavigateDown => AppEvent::IssuesNavigateDown,
            Self::NavigatePageUp => AppEvent::IssuesNavigatePageUp,
            Self::NavigatePageDown => AppEvent::IssuesNavigatePageDown,
            Self::NavigateHome => AppEvent::IssuesNavigateHome,
            Self::NavigateEnd => AppEvent::IssuesNavigateEnd,
            Self::Enter => AppEvent::IssuesEnter,
            Self::CycleFocus => AppEvent::IssuesCycleFocus,
            Self::CycleFocusReverse => AppEvent::IssuesCycleFocusReverse,
            Self::ScrollDetailUp => AppEvent::IssuesScrollDetailUp,
            Self::ScrollDetailDown => AppEvent::IssuesScrollDetailDown,
            Self::ScrollDetailPageUp => AppEvent::IssuesScrollDetailPageUp,
            Self::ScrollDetailPageDown => AppEvent::IssuesScrollDetailPageDown,
            Self::DetailSubfocusNext => AppEvent::IssueDetailSubfocusNext,
            Self::DetailSubfocusPrev => AppEvent::IssueDetailSubfocusPrev,
            // The caller guarantees only navigation variants reach this helper.
            _ => unreachable!("non-navigation IssuesMessage routed to navigation converter"),
        }
    }

    /// Loaded/error payloads and composer/filter/inline/chooser mutations.
    fn into_app_event_data(self) -> AppEvent {
        match self {
            Self::ListLoaded { .. }
            | Self::ListLoadFailed { .. }
            | Self::ListPageLoaded { .. }
            | Self::DetailLoaded { .. }
            | Self::DetailLoadFailed { .. } => self.into_app_event_list_detail(),
            other => other.into_app_event_comments_and_controls(),
        }
    }

    /// List and detail loaded/error payload messages.
    fn into_app_event_list_detail(self) -> AppEvent {
        match self {
            Self::ListLoaded {
                scope_repo_id,
                issues,
                cursor,
                has_more,
            } => AppEvent::IssueListLoaded {
                scope_repo_id,
                issues,
                cursor,
                has_more,
            },
            Self::ListLoadFailed {
                scope_repo_id,
                error,
            } => AppEvent::IssueListLoadFailed {
                scope_repo_id,
                error,
            },
            Self::ListPageLoaded {
                scope_repo_id,
                issues,
                cursor,
                has_more,
            } => AppEvent::IssueListPageLoaded {
                scope_repo_id,
                issues,
                cursor,
                has_more,
            },
            Self::DetailLoaded {
                scope_repo_id,
                issue_number,
                detail,
            } => AppEvent::IssueDetailLoaded {
                scope_repo_id,
                issue_number,
                detail,
            },
            Self::DetailLoadFailed {
                scope_repo_id,
                issue_number,
                error,
            } => AppEvent::IssueDetailLoadFailed {
                scope_repo_id,
                issue_number,
                error,
            },
            // Only list/detail variants are routed here by into_app_event_data.
            _ => unreachable!("non-list/detail IssuesMessage routed to list/detail converter"),
        }
    }

    /// Comments payloads, then controls; further delegates to controls helper.
    fn into_app_event_comments_and_controls(self) -> AppEvent {
        match self {
            Self::CommentsPageLoaded {
                scope_repo_id,
                issue_number,
                comments,
                cursor,
                has_more,
            } => AppEvent::IssueCommentsPageLoaded {
                scope_repo_id,
                issue_number,
                comments,
                cursor,
                has_more,
            },
            Self::CommentsPageFailed {
                scope_repo_id,
                issue_number,
                error,
            } => AppEvent::IssueCommentsPageFailed {
                scope_repo_id,
                issue_number,
                error,
            },
            other => other.into_app_event_controls(),
        }
    }

    /// Filter controls, search, composer, inline editor, and chooser messages.
    fn into_app_event_controls(self) -> AppEvent {
        match self {
            Self::OpenFilterControls => AppEvent::OpenFilterControls,
            Self::CloseFilterControls => AppEvent::CloseFilterControls,
            Self::ApplyFilter => AppEvent::ApplyFilter,
            Self::ClearFilter => AppEvent::ClearFilter,
            Self::FilterNavigateNext => AppEvent::FilterNavigateNext,
            Self::FilterNavigatePrev => AppEvent::FilterNavigatePrev,
            Self::CycleFilterState => AppEvent::CycleFilterState,
            Self::FocusSearchInput => AppEvent::FocusSearchInput,
            Self::BlurSearchInput => AppEvent::BlurSearchInput,
            Self::SetSearchQuery { query } => AppEvent::SetSearchQuery { query },
            Self::ApplySearch => AppEvent::ApplySearch,
            Self::ClearSearch => AppEvent::ClearSearch,
            Self::UpdateDraftFilter { field, value } => {
                AppEvent::UpdateDraftFilter { field, value }
            }
            Self::OpenNewIssueComposer => AppEvent::OpenNewIssueComposer,
            Self::OpenNewCommentComposer => AppEvent::OpenNewCommentComposer,
            Self::OpenReplyComposer { comment_index } => {
                AppEvent::OpenReplyComposer { comment_index }
            }
            Self::OpenInlineEditor { target } => AppEvent::OpenInlineEditor { target },
            Self::InlineChar(c) => AppEvent::InlineChar(c),
            Self::InlineNewline => AppEvent::InlineNewline,
            Self::InlineBackspace => AppEvent::InlineBackspace,
            Self::InlineDelete => AppEvent::InlineDelete,
            Self::InlineCursorLeft => AppEvent::InlineCursorLeft,
            Self::InlineCursorRight => AppEvent::InlineCursorRight,
            Self::InlineCursorUp => AppEvent::InlineCursorUp,
            Self::InlineCursorDown => AppEvent::InlineCursorDown,
            Self::InlineSubmit => AppEvent::InlineSubmit,
            Self::InlineCancelOrEsc => AppEvent::InlineCancelOrEsc,
            Self::CommentCreated { comment } => AppEvent::CommentCreated { comment },
            Self::CommentCreateFailed { error } => AppEvent::CommentCreateFailed { error },
            Self::IssueBodyUpdated { body } => AppEvent::IssueBodyUpdated { body },
            Self::CommentUpdated {
                comment_index,
                body,
            } => AppEvent::CommentUpdated {
                comment_index,
                body,
            },
            Self::MutationFailed { error } => AppEvent::MutationFailed { error },
            Self::OpenAgentChooser => AppEvent::OpenAgentChooser,
            Self::AgentChooserNavigateUp => AppEvent::AgentChooserNavigateUp,
            Self::AgentChooserNavigateDown => AppEvent::AgentChooserNavigateDown,
            Self::AgentChooserConfirm => AppEvent::AgentChooserConfirm,
            Self::AgentChooserCancel => AppEvent::AgentChooserCancel,
            Self::SendToAgentCompleted => AppEvent::SendToAgentCompleted,
            Self::SendToAgentFailed { error } => AppEvent::SendToAgentFailed { error },
            // Loaded/error payloads are routed by into_app_event_data first.
            _ => unreachable!("routed IssuesMessage variant reached controls converter"),
        }
    }
}
