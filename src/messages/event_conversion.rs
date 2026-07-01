//! `AppEvent` <-> `AppMessage` conversion impls (extracted from messages.rs).
//!
//! @plan PLAN-20260624-PR-MODE.P03
//! @requirement REQ-PR-002
//! @pseudocode component-004 lines 46-50

use crate::state::AppEvent;

use super::{
    AppMessage, IssuesMessage, ModalMessage, PersistenceMessage, PullRequestsMessage,
    RepositoryAgentMessage, RuntimeMessage, SystemMessage, ThemeMessage, UiNavigationMessage,
};

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
        if Self::is_issues_event(&event) {
            Self::Issues(IssuesMessage::from_app_event(event))
        } else {
            // @plan PLAN-20260624-PR-MODE.P03
            // @requirement REQ-PR-002
            Self::from_prs_event(event)
        }
    }

    /// Whether the event belongs to the issues domain.
    fn is_issues_event(event: &AppEvent) -> bool {
        Self::is_issues_nav_event(event) || Self::is_issues_data_event(event)
    }

    /// Whether the event is an issues navigation/lifecycle event.
    fn is_issues_nav_event(event: &AppEvent) -> bool {
        matches!(
            event,
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
                | AppEvent::IssueDetailSubfocusPrev
                | AppEvent::OpenFilterControls
                | AppEvent::CloseFilterControls
                | AppEvent::ApplyFilter
                | AppEvent::ClearFilter
                | AppEvent::ClearDraftFilter
                | AppEvent::FilterNavigateNext
                | AppEvent::FilterNavigatePrev
                | AppEvent::CycleFilterState
                | AppEvent::FocusSearchInput
                | AppEvent::BlurSearchInput
                | AppEvent::SetSearchQuery { .. }
                | AppEvent::ApplySearch
                | AppEvent::ClearSearch
                | AppEvent::UpdateDraftFilter { .. }
        )
    }

    /// Whether the event is an issues data/mutation/agent event.
    fn is_issues_data_event(event: &AppEvent) -> bool {
        matches!(
            event,
            AppEvent::IssueListLoaded { .. }
                | AppEvent::IssueListLoadFailed { .. }
                | AppEvent::IssueListPageLoaded { .. }
                | AppEvent::IssueDetailLoaded { .. }
                | AppEvent::IssueDetailLoadFailed { .. }
                | AppEvent::IssueCommentsPageLoaded { .. }
                | AppEvent::IssueCommentsPageFailed { .. }
                | AppEvent::OpenNewIssueComposer
                | AppEvent::OpenNewCommentComposer
                | AppEvent::OpenReplyComposer { .. }
                | AppEvent::OpenInlineEditor { .. }
                | AppEvent::InlineChar(_)
                | AppEvent::InlineNewline
                | AppEvent::InlineBackspace
                | AppEvent::InlineDelete
                | AppEvent::InlineCursorLeft
                | AppEvent::InlineCursorRight
                | AppEvent::InlineCursorUp
                | AppEvent::InlineCursorDown
                | AppEvent::InlineSubmit
                | AppEvent::InlineCancelOrEsc
                | AppEvent::MutationSubmitted { .. }
                | AppEvent::IssueCreated { .. }
                | AppEvent::CommentCreated { .. }
                | AppEvent::CommentCreateFailed { .. }
                | AppEvent::IssueBodyUpdated { .. }
                | AppEvent::CommentUpdated { .. }
                | AppEvent::MutationFailed { .. }
                | AppEvent::OpenAgentChooser
                | AppEvent::AgentChooserNavigateUp
                | AppEvent::AgentChooserNavigateDown
                | AppEvent::AgentChooserConfirm
                | AppEvent::AgentChooserCancel
                | AppEvent::SendToAgentCompleted
                | AppEvent::SendToAgentFailed { .. }
        )
    }

    /// Convert PR-domain [`AppEvent`] variants into the typed message bus.
    ///
    /// @pseudocode component-004 lines 46-50
    fn from_prs_event(event: AppEvent) -> Self {
        Self::PullRequests(PullRequestsMessage::from_app_event(event))
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
            // @plan PLAN-20260624-PR-MODE.P03
            // @requirement REQ-PR-002
            AppMessage::PullRequests(message) => message.into(),
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
