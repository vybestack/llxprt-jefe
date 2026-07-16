//! `AppEvent` <-> `AppMessage` conversion impls (extracted from messages.rs).
//!
//! @plan PLAN-20260624-PR-MODE.P03
//! @requirement REQ-PR-002
//! @pseudocode component-004 lines 46-50

use crate::state::AppEvent;

use super::{
    ActionsMessage, AppMessage, ErrorsMessage, IssuesMessage, ModalMessage, PersistenceMessage,
    PullRequestsMessage, RepositoryAgentMessage, RuntimeMessage, SystemMessage, ThemeMessage,
    UiNavigationMessage,
};

fn ui(message: UiNavigationMessage) -> AppMessage {
    AppMessage::UiNavigation(message)
}

impl From<AppEvent> for AppMessage {
    fn from(event: AppEvent) -> Self {
        match event {
            AppEvent::NavigateUp => Self::UiNavigation(UiNavigationMessage::NavigateUp),
            AppEvent::NavigateDown => Self::UiNavigation(UiNavigationMessage::NavigateDown),
            AppEvent::NavigatePageUp(page) => ui(UiNavigationMessage::NavigatePageUp(page)),
            AppEvent::NavigatePageDown(page) => ui(UiNavigationMessage::NavigatePageDown(page)),
            AppEvent::NavigateHome => Self::UiNavigation(UiNavigationMessage::NavigateHome),
            AppEvent::NavigateEnd => Self::UiNavigation(UiNavigationMessage::NavigateEnd),
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
            AppEvent::EnterSplitMode
            | AppEvent::ExitSplitMode
            | AppEvent::EnterGrabMode
            | AppEvent::ExitGrabMode
            | AppEvent::GrabMoveUp
            | AppEvent::GrabMoveDown
            | AppEvent::SetSplitFilter(_)
            | AppEvent::EnterDashboardGrab
            | AppEvent::ExitDashboardGrab
            | AppEvent::DashboardGrabMoveUp
            | AppEvent::DashboardGrabMoveDown
            | AppEvent::TerminalScrollUp
            | AppEvent::TerminalScrollDown
            | AppEvent::TerminalScrollPageUp
            | AppEvent::TerminalScrollPageDown
            | AppEvent::TerminalFollowTail
            | AppEvent::TerminalScrollToTop
            | AppEvent::OpenShellOverlay
            | AppEvent::CloseShellOverlay => Self::from_split_grab_or_scroll_event(event),
            AppEvent::OpenHelp => Self::Modal(ModalMessage::OpenHelp),
            AppEvent::OpenSearch => Self::Modal(ModalMessage::OpenSearch),
            AppEvent::CloseModal => Self::Modal(ModalMessage::CloseModal),
            AppEvent::SubmitForm => Self::Modal(ModalMessage::SubmitForm),
            AppEvent::ConfirmCycleFocus => Self::Modal(ModalMessage::ConfirmCycleFocus),
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
    /// Convert split-mode, dashboard-grab, and terminal-scrollback
    /// [`AppEvent`] variants into UI-navigation messages. Split out so the
    /// top-level converter stays within the clippy line budget.
    fn from_split_grab_or_scroll_event(event: AppEvent) -> Self {
        use UiNavigationMessage as U;
        match event {
            AppEvent::EnterSplitMode => Self::UiNavigation(U::EnterSplitMode),
            AppEvent::ExitSplitMode => Self::UiNavigation(U::ExitSplitMode),
            AppEvent::EnterGrabMode => Self::UiNavigation(U::EnterGrabMode),
            AppEvent::ExitGrabMode => Self::UiNavigation(U::ExitGrabMode),
            AppEvent::GrabMoveUp => Self::UiNavigation(U::GrabMoveUp),
            AppEvent::GrabMoveDown => Self::UiNavigation(U::GrabMoveDown),
            AppEvent::SetSplitFilter(filter) => Self::UiNavigation(U::SetSplitFilter(filter)),
            AppEvent::EnterDashboardGrab => Self::UiNavigation(U::EnterDashboardGrab),
            AppEvent::ExitDashboardGrab => Self::UiNavigation(U::ExitDashboardGrab),
            AppEvent::DashboardGrabMoveUp => Self::UiNavigation(U::DashboardGrabMoveUp),
            AppEvent::DashboardGrabMoveDown => Self::UiNavigation(U::DashboardGrabMoveDown),
            // Terminal scrollback viewport events (issue #198).
            AppEvent::TerminalScrollUp => Self::UiNavigation(U::TerminalScrollUp),
            AppEvent::TerminalScrollDown => Self::UiNavigation(U::TerminalScrollDown),
            AppEvent::TerminalScrollPageUp => Self::UiNavigation(U::TerminalScrollPageUp),
            AppEvent::TerminalScrollPageDown => Self::UiNavigation(U::TerminalScrollPageDown),
            AppEvent::TerminalFollowTail => Self::UiNavigation(U::TerminalFollowTail),
            AppEvent::TerminalScrollToTop => Self::UiNavigation(U::TerminalScrollToTop),
            // Shell-overlay events (issue #222).
            AppEvent::OpenShellOverlay => Self::UiNavigation(U::OpenShellOverlay),
            AppEvent::CloseShellOverlay => Self::UiNavigation(U::CloseShellOverlay),
            // Catch-all is required: the caller passes an `AppEvent` value that
            // is known at the call site to be split/grab/scroll, but the type
            // system cannot enforce that constraint. This arm delegates to the
            // full converter so an unexpected variant still routes correctly.
            other => Self::from_non_ui_nav_event(other),
        }
    }

    /// Convert non-UI-navigation [`AppEvent`] variants into the typed message bus.
    ///
    /// Split out from [`AppMessage::from`] so the top-level converter stays
    /// within the clippy line budget without a complexity suppression.
    fn from_non_ui_nav_event(event: AppEvent) -> Self {
        match event {
            AppEvent::KillAgent(id) => Self::Runtime(RuntimeMessage::KillAgent(id)),
            AppEvent::RelaunchAgent(id) => Self::Runtime(RuntimeMessage::RelaunchAgent(id)),
            AppEvent::RestartAgent(id) => Self::Runtime(RuntimeMessage::RestartAgent(id)),
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
            AppEvent::OpenThemePicker {
                available_themes,
                active_slug,
            } => Self::Theme(ThemeMessage::OpenThemePicker {
                available_themes,
                active_slug,
            }),
            AppEvent::ThemePickerNavigateUp => Self::Theme(ThemeMessage::PickerNavigateUp),
            AppEvent::ThemePickerNavigateDown => Self::Theme(ThemeMessage::PickerNavigateDown),
            AppEvent::ThemePickerConfirm => Self::Theme(ThemeMessage::PickerConfirm),
            AppEvent::ThemePickerToggleOverride => {
                Self::Theme(ThemeMessage::ToggleAgentThemeOverride)
            }
            AppEvent::CloseThemePicker => Self::Theme(ThemeMessage::PickerCancel),
            AppEvent::Quit => Self::System(SystemMessage::Quit),
            AppEvent::ClearError => Self::System(SystemMessage::ClearError),
            AppEvent::ClearWarning => Self::System(SystemMessage::ClearWarning),
            // Auth remediation events route to the System channel (issue #244).
            AppEvent::OpenAuthDialog => Self::System(SystemMessage::OpenAuthDialog),
            AppEvent::AuthCodeReceived { code, url } => {
                Self::System(SystemMessage::AuthCodeReceived { code, url })
            }
            AppEvent::AuthSucceeded => Self::System(SystemMessage::AuthSucceeded),
            AppEvent::AuthFailed { error } => Self::System(SystemMessage::AuthFailed { error }),
            AppEvent::AuthCancelled => Self::System(SystemMessage::AuthCancelled),
            AppEvent::AuthRetry => Self::System(SystemMessage::AuthRetry),
            AppEvent::TransientAgentQueued { queue_position } => {
                Self::System(SystemMessage::TransientAgentQueued { queue_position })
            }
            AppEvent::TransientAgentDequeued => Self::System(SystemMessage::TransientAgentDequeued),
            // Catch-all: repository/agent events, then issues/PRs/actions.
            other => Self::from_repository_agent_event(other),
        }
    }

    /// Convert repository/agent [`AppEvent`] variants into the typed message bus.
    fn from_repository_agent_event(event: AppEvent) -> Self {
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
            other => Self::from_issues_event(other),
        }
    }

    /// Convert issues-domain [`AppEvent`] variants into the typed message bus.
    fn from_issues_event(event: AppEvent) -> Self {
        if Self::is_issues_event(&event) {
            Self::Issues(IssuesMessage::from_app_event(event))
        } else if Self::is_actions_event(&event) {
            Self::Actions(ActionsMessage::from_app_event(event))
        } else if Self::is_errors_event(&event) {
            Self::Errors(ErrorsMessage::from_app_event(event))
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

    /// Whether the event belongs to the errors domain (issue #292).
    fn is_errors_event(event: &AppEvent) -> bool {
        matches!(
            event,
            AppEvent::EnterErrorsMode
                | AppEvent::ExitErrorsMode
                | AppEvent::RefocusErrorList
                | AppEvent::ErrorsNavigateUp
                | AppEvent::ErrorsNavigateDown
                | AppEvent::ErrorsNavigateHome
                | AppEvent::ErrorsNavigateEnd
                | AppEvent::ErrorsEnter
                | AppEvent::ErrorsCycleFocus
                | AppEvent::ErrorsCycleFocusReverse
                | AppEvent::ErrorsScrollDetailUp
                | AppEvent::ErrorsScrollDetailDown
                | AppEvent::ErrorsScrollDetailPageUp
                | AppEvent::ErrorsScrollDetailPageDown
                | AppEvent::ErrorsClearAll
        )
    }

    /// Whether the event belongs to the actions domain.
    fn is_actions_event(event: &AppEvent) -> bool {
        matches!(
            event,
            AppEvent::EnterActionsMode
                | AppEvent::EnterActionsModeWithPrFilter { .. }
                | AppEvent::ExitActionsMode
                | AppEvent::RefocusActionsList
                | AppEvent::ActionsReload
                | AppEvent::ActionsNavigateUp
                | AppEvent::ActionsNavigateDown
                | AppEvent::ActionsNavigatePageUp(_)
                | AppEvent::ActionsNavigatePageDown(_)
                | AppEvent::ActionsNavigateHome
                | AppEvent::ActionsNavigateEnd
                | AppEvent::ActionsEnter
                | AppEvent::ActionsCycleFocus
                | AppEvent::ActionsCycleFocusReverse
                | AppEvent::ActionsSetDetailGeometry { .. }
                | AppEvent::ActionsScrollDetailUp
                | AppEvent::ActionsScrollDetailDown
                | AppEvent::ActionsExpandJob
                | AppEvent::ActionsCollapseJob
                | AppEvent::ActionsDetailEscape
                | AppEvent::ActionsNavigateJobUp
                | AppEvent::ActionsNavigateJobDown
                | AppEvent::ActionsBeginDetailReload { .. }
                | AppEvent::ActionsRunsLoaded { .. }
                | AppEvent::ActionsRunsLoadFailed { .. }
                | AppEvent::ActionsRunsPageLoaded { .. }
                | AppEvent::ActionsRunsPageLoadFailed { .. }
                | AppEvent::ActionsDetailLoaded { .. }
                | AppEvent::ActionsDetailLoadFailed { .. }
                | AppEvent::WorkflowsLoaded { .. }
                | AppEvent::WorkflowsLoadFailed { .. }
                | AppEvent::ActionsOpenFilterControls
                | AppEvent::ActionsCloseFilterControls
                | AppEvent::ActionsApplyFilter
                | AppEvent::ActionsClearFilter
                | AppEvent::ActionsClearDraftFilter
                | AppEvent::ActionsFilterNavigateNext
                | AppEvent::ActionsFilterNavigatePrev
                | AppEvent::ActionsCycleFilterStatus
                | AppEvent::ActionsFocusSearchInput
                | AppEvent::ActionsBlurSearchInput
                | AppEvent::ActionsSetSearchQuery { .. }
                | AppEvent::ActionsApplySearch
                | AppEvent::ActionsClearSearch
                | AppEvent::ActionsUpdateDraftFilter { .. }
                | AppEvent::OpenWorkflowDispatch(_)
                | AppEvent::CloseWorkflowDispatch
                | AppEvent::WorkflowDispatchSubmitted { .. }
                | AppEvent::WorkflowDispatchSuccess { .. }
                | AppEvent::WorkflowDispatchFailed { .. }
        )
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
                | AppEvent::IssuesNavigatePageUp(_)
                | AppEvent::IssuesNavigatePageDown(_)
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
                | AppEvent::CloseIssue
                | AppEvent::OpenDeleteIssueConfirm
                | AppEvent::IssueDeleteConfirm
                | AppEvent::IssueDeleteCancel
                | AppEvent::IssueClosed { .. }
                | AppEvent::IssueDeleted { .. }
                | AppEvent::OpenCloseReasonChooser
                | AppEvent::CloseReasonNavigateUp
                | AppEvent::CloseReasonNavigateDown
                | AppEvent::CloseReasonSelect
                | AppEvent::CloseReasonDuplicateSearchChar(_)
                | AppEvent::CloseReasonDuplicateSearchBackspace
                | AppEvent::CloseReasonDuplicateSearchNavigateUp
                | AppEvent::CloseReasonDuplicateSearchNavigateDown
                | AppEvent::CloseReasonConfirm
                | AppEvent::CloseReasonCancel
                | AppEvent::OpenAgentChooser { .. }
                | AppEvent::AgentChooserNavigateUp
                | AppEvent::AgentChooserNavigateDown
                | AppEvent::AgentChooserConfirm
                | AppEvent::AgentChooserCancel
                | AppEvent::SendToAgentCompleted
                | AppEvent::SendToAgentFailed { .. }
                | AppEvent::IssueSelfAssignmentFailed { .. }
        ) || Self::is_issue_property_data_event(event)
    }

    /// Property-editor and silent-refresh issues events (issue #175).
    fn is_issue_property_data_event(event: &AppEvent) -> bool {
        matches!(
            event,
            AppEvent::IssueOpenPropertyEditor { .. }
                | AppEvent::IssuePropertyEditorNavigateUp
                | AppEvent::IssuePropertyEditorNavigateDown
                | AppEvent::IssuePropertyEditorToggle
                | AppEvent::IssuePropertyEditorConfirm
                | AppEvent::IssuePropertyEditorCancel
                | AppEvent::IssuePropertyEditorTitleChar(_)
                | AppEvent::IssuePropertyEditorTitleBackspace
                | AppEvent::IssuePropertyEditorTitleDelete
                | AppEvent::IssuePropertyEditorTitleCursorLeft
                | AppEvent::IssuePropertyEditorTitleCursorRight
                | AppEvent::IssuePropertyEditorOptionsLoaded { .. }
                | AppEvent::IssuePropertyEditorOptionsFailed { .. }
                | AppEvent::IssuePropertyEditSucceeded { .. }
                | AppEvent::IssuePostMutationRefreshStarted
                | AppEvent::IssuePropertyEditFailed { .. }
                | AppEvent::IssuePropertyEditorValidationError { .. }
                | AppEvent::IssueListSilentRefreshed { .. }
                | AppEvent::IssueListSilentRefreshFailed { .. }
                | AppEvent::IssueDetailSilentRefreshed { .. }
                | AppEvent::IssueDetailSilentRefreshFailed { .. }
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
            AppMessage::Actions(message) => message.into(),
            AppMessage::Errors(message) => message.into(),
            AppMessage::System(message) => message.into(),
        }
    }
}

impl From<UiNavigationMessage> for AppEvent {
    fn from(message: UiNavigationMessage) -> Self {
        match message {
            UiNavigationMessage::NavigateUp => Self::NavigateUp,
            UiNavigationMessage::NavigateDown => Self::NavigateDown,
            UiNavigationMessage::NavigatePageUp(page) => Self::NavigatePageUp(page),
            UiNavigationMessage::NavigatePageDown(page) => Self::NavigatePageDown(page),
            UiNavigationMessage::NavigateHome => Self::NavigateHome,
            UiNavigationMessage::NavigateEnd => Self::NavigateEnd,
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
            UiNavigationMessage::EnterDashboardGrab => Self::EnterDashboardGrab,
            UiNavigationMessage::ExitDashboardGrab => Self::ExitDashboardGrab,
            UiNavigationMessage::DashboardGrabMoveUp => Self::DashboardGrabMoveUp,
            UiNavigationMessage::DashboardGrabMoveDown => Self::DashboardGrabMoveDown,
            UiNavigationMessage::TerminalScrollUp => Self::TerminalScrollUp,
            UiNavigationMessage::TerminalScrollDown => Self::TerminalScrollDown,
            UiNavigationMessage::TerminalScrollPageUp => Self::TerminalScrollPageUp,
            UiNavigationMessage::TerminalScrollPageDown => Self::TerminalScrollPageDown,
            UiNavigationMessage::TerminalFollowTail => Self::TerminalFollowTail,
            UiNavigationMessage::TerminalScrollToTop => Self::TerminalScrollToTop,
            UiNavigationMessage::OpenShellOverlay => Self::OpenShellOverlay,
            UiNavigationMessage::CloseShellOverlay => Self::CloseShellOverlay,
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
            ModalMessage::ConfirmCycleFocus => Self::ConfirmCycleFocus,
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
            RuntimeMessage::RestartAgent(id) => Self::RestartAgent(id),
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
            ThemeMessage::OpenThemePicker {
                available_themes,
                active_slug,
            } => Self::OpenThemePicker {
                available_themes,
                active_slug,
            },
            ThemeMessage::PickerNavigateUp => Self::ThemePickerNavigateUp,
            ThemeMessage::PickerNavigateDown => Self::ThemePickerNavigateDown,
            ThemeMessage::PickerConfirm => Self::ThemePickerConfirm,
            ThemeMessage::ToggleAgentThemeOverride => Self::ThemePickerToggleOverride,
            ThemeMessage::PickerCancel => Self::CloseThemePicker,
        }
    }
}

impl From<SystemMessage> for AppEvent {
    fn from(message: SystemMessage) -> Self {
        match message {
            SystemMessage::Quit => Self::Quit,
            SystemMessage::ClearError => Self::ClearError,
            SystemMessage::ClearWarning => Self::ClearWarning,
            SystemMessage::OpenAuthDialog => Self::OpenAuthDialog,
            SystemMessage::AuthCodeReceived { code, url } => Self::AuthCodeReceived { code, url },
            SystemMessage::AuthSucceeded => Self::AuthSucceeded,
            SystemMessage::AuthFailed { error } => Self::AuthFailed { error },
            SystemMessage::AuthCancelled => Self::AuthCancelled,
            SystemMessage::AuthRetry => Self::AuthRetry,
            SystemMessage::TransientAgentQueued { queue_position } => {
                Self::TransientAgentQueued { queue_position }
            }
            SystemMessage::TransientAgentDequeued => Self::TransientAgentDequeued,
        }
    }
}
