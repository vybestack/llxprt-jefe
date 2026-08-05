//! `AppEvent` <-> `AppMessage` conversion impls (extracted from messages.rs).
//!
//! @plan PLAN-20260624-PR-MODE.P03
//! @requirement REQ-PR-002
//! @pseudocode component-004 lines 46-50

use crate::state::AppEvent;
use crate::state::observation_events::ObservationEvent;

use super::{
    ActionsMessage, AppMessage, ErrorsMessage, IssuesMessage, ModalMessage, PersistenceMessage,
    PullRequestsMessage, RepositoryAgentMessage, RuntimeMessage, SystemMessage,
    TerminalManagerMessage, ThemeMessage, UiNavigationMessage,
};

impl From<AppEvent> for AppMessage {
    fn from(event: AppEvent) -> Self {
        match event {
            AppEvent::EffectCompletion(completion) => Self::EffectCompletion(completion),
            AppEvent::NavigateUp
            | AppEvent::NavigateDown
            | AppEvent::NavigatePageUp(_)
            | AppEvent::NavigatePageDown(_)
            | AppEvent::NavigateHome
            | AppEvent::NavigateEnd
            | AppEvent::NavigateLeft
            | AppEvent::NavigateRight
            | AppEvent::SelectRepository(_)
            | AppEvent::SelectAgent(_)
            | AppEvent::JumpToAgentByShortcut(_)
            | AppEvent::CyclePaneFocus
            | AppEvent::ToggleTerminalFocus
            | AppEvent::ToggleHideIdleRepositories => Self::from_nav_event(event),
            AppEvent::FocusDashboardSearch
            | AppEvent::BlurDashboardSearch
            | AppEvent::SetDashboardSearchQuery { .. }
            | AppEvent::ClearDashboardSearch => Self::from_dashboard_search_event(event),
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
            | AppEvent::CloseShellOverlay
            | AppEvent::HideShellOverlay
            | AppEvent::ResumeShellOverlay(_) => Self::from_split_grab_or_scroll_event(event),
            AppEvent::OpenHelp
            | AppEvent::OpenSearch
            | AppEvent::CloseModal
            | AppEvent::SubmitForm
            | AppEvent::ConfirmCycleFocus
            | AppEvent::FormChar(_)
            | AppEvent::FormBackspace
            | AppEvent::FormDelete
            | AppEvent::FormMoveCursorLeft
            | AppEvent::FormMoveCursorRight
            | AppEvent::FormMoveCursorStart
            | AppEvent::FormMoveCursorEnd
            | AppEvent::FormNextField
            | AppEvent::FormPrevField
            | AppEvent::FormToggleCheckbox => Self::from_modal_event(event),
            other => Self::from_non_ui_nav_event(other),
        }
    }
}

impl AppMessage {
    /// Convert navigation [`AppEvent`] variants into UI-navigation messages.
    /// Split out so the top-level converter stays within the clippy line budget.
    fn from_nav_event(event: AppEvent) -> Self {
        use UiNavigationMessage as U;
        match event {
            AppEvent::NavigateUp => Self::UiNavigation(U::NavigateUp),
            AppEvent::NavigateDown => Self::UiNavigation(U::NavigateDown),
            AppEvent::NavigatePageUp(page) => Self::UiNavigation(U::NavigatePageUp(page)),
            AppEvent::NavigatePageDown(page) => Self::UiNavigation(U::NavigatePageDown(page)),
            AppEvent::NavigateHome => Self::UiNavigation(U::NavigateHome),
            AppEvent::NavigateEnd => Self::UiNavigation(U::NavigateEnd),
            AppEvent::NavigateLeft => Self::UiNavigation(U::NavigateLeft),
            AppEvent::NavigateRight => Self::UiNavigation(U::NavigateRight),
            AppEvent::SelectRepository(index) => Self::UiNavigation(U::SelectRepository(index)),
            AppEvent::SelectAgent(index) => Self::UiNavigation(U::SelectAgent(index)),
            AppEvent::JumpToAgentByShortcut(slot) => {
                Self::UiNavigation(U::JumpToAgentByShortcut(slot))
            }
            AppEvent::CyclePaneFocus => Self::UiNavigation(U::CyclePaneFocus),
            AppEvent::ToggleTerminalFocus => Self::UiNavigation(U::ToggleTerminalFocus),
            AppEvent::ToggleHideIdleRepositories => {
                Self::UiNavigation(U::ToggleHideIdleRepositories)
            }
            _ => unreachable!("non-navigation AppEvent routed to from_nav_event"),
        }
    }

    /// Convert dashboard "search lite" [`AppEvent`] variants into UI-navigation
    /// messages (issue #405). Split out so the top-level converter stays within
    /// the clippy line budget.
    fn from_dashboard_search_event(event: AppEvent) -> Self {
        use UiNavigationMessage as U;
        match event {
            AppEvent::FocusDashboardSearch => Self::UiNavigation(U::FocusDashboardSearch),
            AppEvent::BlurDashboardSearch => Self::UiNavigation(U::BlurDashboardSearch),
            AppEvent::SetDashboardSearchQuery { query } => {
                Self::UiNavigation(U::SetDashboardSearchQuery { query })
            }
            AppEvent::ClearDashboardSearch => Self::UiNavigation(U::ClearDashboardSearch),
            _ => {
                unreachable!("non-dashboard-search AppEvent routed to from_dashboard_search_event")
            }
        }
    }

    /// Convert multi-agent workbench [`AppEvent`] variants into UI-navigation
    /// messages (issue #626). Split out so the top-level converter stays within
    /// the clippy line budget.
    fn from_workbench_event(event: AppEvent) -> Self {
        use UiNavigationMessage as U;
        match event {
            AppEvent::ToggleWorkbenchStatusBucket(bucket) => {
                Self::UiNavigation(U::ToggleWorkbenchStatusBucket(bucket))
            }
            AppEvent::WorkbenchNextPage => Self::UiNavigation(U::WorkbenchNextPage),
            AppEvent::WorkbenchPrevPage => Self::UiNavigation(U::WorkbenchPrevPage),
            AppEvent::WorkbenchFilterCursorPrev => Self::UiNavigation(U::WorkbenchFilterCursorPrev),
            AppEvent::WorkbenchFilterCursorNext => Self::UiNavigation(U::WorkbenchFilterCursorNext),
            AppEvent::WorkbenchSelectPrev => Self::UiNavigation(U::WorkbenchSelectPrev),
            AppEvent::WorkbenchSelectNext => Self::UiNavigation(U::WorkbenchSelectNext),
            AppEvent::WorkbenchAttach => Self::UiNavigation(U::WorkbenchAttach),
            _ => unreachable!("non-workbench AppEvent routed to from_workbench_event"),
        }
    }

    /// Convert modal/form [`AppEvent`] variants into modal messages. Split out
    /// so the top-level converter stays within the clippy line budget.
    fn from_modal_event(event: AppEvent) -> Self {
        match event {
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
            AppEvent::FormMoveCursorStart => Self::Modal(ModalMessage::FormMoveCursorStart),
            AppEvent::FormMoveCursorEnd => Self::Modal(ModalMessage::FormMoveCursorEnd),
            AppEvent::FormNextField => Self::Modal(ModalMessage::FormNextField),
            AppEvent::FormPrevField => Self::Modal(ModalMessage::FormPrevField),
            AppEvent::FormToggleCheckbox => Self::Modal(ModalMessage::FormToggleCheckbox),
            _ => unreachable!("non-modal AppEvent routed to from_modal_event"),
        }
    }

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
            AppEvent::HideShellOverlay => Self::UiNavigation(U::HideShellOverlay),
            AppEvent::ResumeShellOverlay(agent_id) => {
                Self::UiNavigation(U::ResumeShellOverlay(agent_id))
            }
            _ => unreachable!(
                "non-split/grab/scroll AppEvent routed to from_split_grab_or_scroll_event"
            ),
        }
    }

    /// Convert non-UI-navigation [`AppEvent`] variants into the typed message bus.
    ///
    /// Split out from [`AppMessage::from`] so the top-level converter stays
    /// within the clippy line budget without a complexity suppression.
    fn from_non_ui_nav_event(event: AppEvent) -> Self {
        match event {
            AppEvent::ToggleWorkbenchStatusBucket(_)
            | AppEvent::WorkbenchNextPage
            | AppEvent::WorkbenchPrevPage
            | AppEvent::WorkbenchFilterCursorPrev
            | AppEvent::WorkbenchFilterCursorNext
            | AppEvent::WorkbenchSelectPrev
            | AppEvent::WorkbenchSelectNext
            | AppEvent::WorkbenchAttach => Self::from_workbench_event(event),
            AppEvent::KillAgent(id) => Self::Runtime(RuntimeMessage::KillAgent(id)),
            AppEvent::RelaunchAgent(id) => Self::Runtime(RuntimeMessage::RelaunchAgent(id)),
            AppEvent::RestartAgent(id) => Self::Runtime(RuntimeMessage::RestartAgent(id)),
            AppEvent::AgentStatusChanged(id, status) => {
                Self::Runtime(RuntimeMessage::AgentStatusChanged(id, status))
            }
            AppEvent::Observation(ObservationEvent::Updated(id, generation, observation)) => {
                Self::Runtime(RuntimeMessage::ObservationUpdated(
                    id,
                    generation,
                    observation,
                ))
            }
            AppEvent::Observation(ObservationEvent::Cleared(id, generation)) => {
                Self::Runtime(RuntimeMessage::ObservationCleared(id, generation))
            }
            AppEvent::PersistenceLoadSuccess => Self::Persistence(PersistenceMessage::LoadSuccess),
            AppEvent::PersistenceLoadFailed(error) => {
                Self::Persistence(PersistenceMessage::LoadFailed(error))
            }
            AppEvent::PersistenceSaveSuccess => Self::Persistence(PersistenceMessage::SaveSuccess),
            AppEvent::PersistenceSaveFailed(error) => {
                Self::Persistence(PersistenceMessage::SaveFailed(error))
            }
            AppEvent::ThemeResolveFailed(error) => Self::Theme(ThemeMessage::ResolveFailed(error)),
            AppEvent::Settings(message) => Self::Settings(message),
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
            AppEvent::OpenAgentTypeForm(id) => {
                Self::RepositoryAgent(RepositoryAgentMessage::OpenAgentTypeForm(id))
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
            AppEvent::ProbeAgentAvailability(probes) => {
                Self::RepositoryAgent(RepositoryAgentMessage::ProbeAgentAvailability(probes))
            }
            AppEvent::ProjectActionAvailability => {
                Self::RepositoryAgent(RepositoryAgentMessage::ProjectActionAvailability)
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
        } else if Self::is_terminal_manager_event(&event) {
            Self::TerminalManager(TerminalManagerMessage::from_app_event(event))
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
                | AppEvent::CaptureSilentError(..)
                | AppEvent::ErrorsClearAll
        )
    }

    /// Whether the event belongs to the terminal-manager domain (issue #361
    /// PR B).
    fn is_terminal_manager_event(event: &AppEvent) -> bool {
        matches!(
            event,
            AppEvent::EnterTerminalManagerMode
                | AppEvent::ExitTerminalManagerMode
                | AppEvent::TerminalManagerNavigateUp
                | AppEvent::TerminalManagerNavigateDown
                | AppEvent::TerminalManagerNavigateHome
                | AppEvent::TerminalManagerNavigateEnd
                | AppEvent::RequestShellFocus { .. }
                | AppEvent::ConfirmShellFocus(_)
                | AppEvent::FailShellFocus
                | AppEvent::ShellPreviewResult { .. }
                | AppEvent::ShellClosed(_)
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
                | AppEvent::CycleActionsSortByNext
                | AppEvent::CycleActionsSortByPrev
                | AppEvent::ToggleActionsSortOrder
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
                | AppEvent::CycleIssueSortByNext
                | AppEvent::CycleIssueSortByPrev
                | AppEvent::ToggleIssueSortOrder
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
        Self::is_issues_core_data_event(event)
            || matches!(
                event,
                AppEvent::BeginIssueListSendDetail(..)
                    | AppEvent::CancelIssueListSendDetail
                    | AppEvent::IssueListSendDetailReady { .. }
                    | AppEvent::IssueDetailAuthRequired(..)
            )
            || Self::is_new_issue_form_data_event(event)
            || Self::is_issue_property_data_event(event)
    }

    /// Core issues data/mutation/lifecycle/agent events (issue inline composer,
    /// close/delete, agent chooser). Split from `is_issues_data_event` to stay
    /// under the clippy too-many-lines limit (issue #407).
    fn is_issues_core_data_event(event: &AppEvent) -> bool {
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
                | AppEvent::InlineCursorHome
                | AppEvent::InlineCursorEnd
                | AppEvent::InlineSubmit
                | AppEvent::InlineCancelOrEsc
                | AppEvent::RequestIssueRewrite
                | AppEvent::IssueRewriteSucceeded { .. }
                | AppEvent::IssueRewriteFailed { .. }
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
        )
    }

    /// Whether the event is a New Issue dialog data/agent event.
    fn is_new_issue_form_data_event(event: &AppEvent) -> bool {
        matches!(
            event,
            AppEvent::OpenNewIssueComposer
                | AppEvent::NewIssueTemplateNext
                | AppEvent::NewIssueTypeNext
                | AppEvent::NewIssueTitleChar(_)
                | AppEvent::NewIssueTitleBackspace
                | AppEvent::NewIssueTitleDelete
                | AppEvent::NewIssueTitleCursorLeft
                | AppEvent::NewIssueTitleCursorRight
                | AppEvent::NewIssueTitleCursorHome
                | AppEvent::NewIssueTitleCursorEnd
                | AppEvent::NewIssueBodyChar(_)
                | AppEvent::NewIssueBodyNewline
                | AppEvent::NewIssueBodyBackspace
                | AppEvent::NewIssueBodyDelete
                | AppEvent::NewIssueBodyCursorLeft
                | AppEvent::NewIssueBodyCursorRight
                | AppEvent::NewIssueBodyCursorUp
                | AppEvent::NewIssueBodyCursorDown
                | AppEvent::NewIssueBodyCursorHome
                | AppEvent::NewIssueBodyCursorEnd
                | AppEvent::NewIssueFocusNext
                | AppEvent::NewIssueFocusPrev
                | AppEvent::NewIssueSubmit
                | AppEvent::NewIssueCancel
                | AppEvent::NewIssueCreated { .. }
                | AppEvent::NewIssueCreateFailed { .. }
                | AppEvent::NewIssueOptionsLoaded { .. }
                | AppEvent::NewIssueOptionsFailed { .. }
        )
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
                | AppEvent::IssuePropertyEditorTitleCursorHome
                | AppEvent::IssuePropertyEditorTitleCursorEnd
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
            AppMessage::Settings(message) => Self::Settings(message),
            AppMessage::TerminalManager(message) => message.into(),
            AppMessage::System(message) => message.into(),
            AppMessage::EffectCompletion(completion) => Self::EffectCompletion(completion),
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
            UiNavigationMessage::FocusDashboardSearch => Self::FocusDashboardSearch,
            UiNavigationMessage::BlurDashboardSearch => Self::BlurDashboardSearch,
            UiNavigationMessage::SetDashboardSearchQuery { query } => {
                Self::SetDashboardSearchQuery { query }
            }
            UiNavigationMessage::ClearDashboardSearch => Self::ClearDashboardSearch,
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
            UiNavigationMessage::HideShellOverlay => Self::HideShellOverlay,
            UiNavigationMessage::ResumeShellOverlay(agent_id) => Self::ResumeShellOverlay(agent_id),
            UiNavigationMessage::ToggleWorkbenchStatusBucket(bucket) => {
                Self::ToggleWorkbenchStatusBucket(bucket)
            }
            UiNavigationMessage::WorkbenchNextPage => Self::WorkbenchNextPage,
            UiNavigationMessage::WorkbenchPrevPage => Self::WorkbenchPrevPage,
            UiNavigationMessage::WorkbenchFilterCursorPrev => Self::WorkbenchFilterCursorPrev,
            UiNavigationMessage::WorkbenchFilterCursorNext => Self::WorkbenchFilterCursorNext,
            UiNavigationMessage::WorkbenchSelectPrev => Self::WorkbenchSelectPrev,
            UiNavigationMessage::WorkbenchSelectNext => Self::WorkbenchSelectNext,
            UiNavigationMessage::WorkbenchAttach => Self::WorkbenchAttach,
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
            ModalMessage::FormMoveCursorStart => Self::FormMoveCursorStart,
            ModalMessage::FormMoveCursorEnd => Self::FormMoveCursorEnd,
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
            RepositoryAgentMessage::OpenAgentTypeForm(id) => Self::OpenAgentTypeForm(id),
            RepositoryAgentMessage::OpenEditAgent(id) => Self::OpenEditAgent(id),
            RepositoryAgentMessage::OpenDeleteAgent(id) => Self::OpenDeleteAgent(id),
            RepositoryAgentMessage::ToggleDeleteWorkDir => Self::ToggleDeleteWorkDir,
            RepositoryAgentMessage::ProbeAgentAvailability(probes) => {
                Self::ProbeAgentAvailability(probes)
            }
            RepositoryAgentMessage::ProjectActionAvailability => Self::ProjectActionAvailability,
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
            RuntimeMessage::ObservationUpdated(id, generation, observation) => {
                Self::Observation(ObservationEvent::Updated(id, generation, observation))
            }
            RuntimeMessage::ObservationCleared(id, generation) => {
                Self::Observation(ObservationEvent::Cleared(id, generation))
            }
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
            PersistenceMessage::StageSave => Self::StageDurableSave,
        }
    }
}

impl From<ThemeMessage> for AppEvent {
    fn from(message: ThemeMessage) -> Self {
        match message {
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
