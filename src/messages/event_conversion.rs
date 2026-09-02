//! `AppEvent` <-> `AppMessage` conversion impls (extracted from messages.rs).
//!
//! @plan PLAN-20260624-PR-MODE.P03
//! @requirement REQ-PR-002
//! @pseudocode component-004 lines 46-50
//!
//! Every domain converter returns [`ControlFlow`]; events no domain claims
//! flow to [`AppMessage::from_unrouted_event`], which reports the drift as a
//! captured error on the errors screen instead of panicking. This keeps the
//! `AppEvent` -> `AppMessage` conversion total without `unreachable!` tails
//! and without duplicating variant lists in routing classifiers.

use std::ops::ControlFlow;

use crate::domain::ErrorSource;
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
            AppEvent::Back
            | AppEvent::NavigateUp
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
            | AppEvent::ToggleHideIdleRepositories => Self::claim_nav_event(event),
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
            | AppEvent::ResumeShellOverlay(_) => Self::claim_split_grab_or_scroll_event(event),
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
            | AppEvent::FormToggleCheckbox => Self::claim_modal_event(event),
            other => Self::from_non_ui_nav_event(other),
        }
    }
}

impl AppMessage {
    /// Claim a navigation-grouped event or report it as unroutable drift.
    fn claim_nav_event(event: AppEvent) -> Self {
        match Self::from_nav_event(event) {
            ControlFlow::Break(message) => message,
            ControlFlow::Continue(unclaimed) => Self::from_unrouted_event(unclaimed),
        }
    }

    /// Claim a split/grab/scroll-grouped event or report it as unroutable drift.
    fn claim_split_grab_or_scroll_event(event: AppEvent) -> Self {
        match Self::from_split_grab_or_scroll_event(event) {
            ControlFlow::Break(message) => message,
            ControlFlow::Continue(unclaimed) => Self::from_unrouted_event(unclaimed),
        }
    }
    /// Claim a modal-grouped event or report it as unroutable drift.
    fn claim_modal_event(event: AppEvent) -> Self {
        match Self::from_modal_event(event) {
            ControlFlow::Break(message) => message,
            ControlFlow::Continue(unclaimed) => Self::from_unrouted_event(unclaimed),
        }
    }

    /// Convert navigation [`AppEvent`] variants into UI-navigation messages.
    /// Split out so the top-level converter stays within the clippy line budget.
    fn from_nav_event(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        use UiNavigationMessage as U;
        match event {
            AppEvent::Back => ControlFlow::Break(Self::UiNavigation(U::Back)),
            AppEvent::NavigateUp => ControlFlow::Break(Self::UiNavigation(U::NavigateUp)),
            AppEvent::NavigateDown => ControlFlow::Break(Self::UiNavigation(U::NavigateDown)),
            AppEvent::NavigatePageUp(page) => {
                ControlFlow::Break(Self::UiNavigation(U::NavigatePageUp(page)))
            }
            AppEvent::NavigatePageDown(page) => {
                ControlFlow::Break(Self::UiNavigation(U::NavigatePageDown(page)))
            }
            AppEvent::NavigateHome => ControlFlow::Break(Self::UiNavigation(U::NavigateHome)),
            AppEvent::NavigateEnd => ControlFlow::Break(Self::UiNavigation(U::NavigateEnd)),
            AppEvent::NavigateLeft => ControlFlow::Break(Self::UiNavigation(U::NavigateLeft)),
            AppEvent::NavigateRight => ControlFlow::Break(Self::UiNavigation(U::NavigateRight)),
            AppEvent::SelectRepository(index) => {
                ControlFlow::Break(Self::UiNavigation(U::SelectRepository(index)))
            }
            AppEvent::SelectAgent(index) => {
                ControlFlow::Break(Self::UiNavigation(U::SelectAgent(index)))
            }
            AppEvent::JumpToAgentByShortcut(slot) => {
                ControlFlow::Break(Self::UiNavigation(U::JumpToAgentByShortcut(slot)))
            }
            AppEvent::CyclePaneFocus => ControlFlow::Break(Self::UiNavigation(U::CyclePaneFocus)),
            AppEvent::ToggleTerminalFocus => {
                ControlFlow::Break(Self::UiNavigation(U::ToggleTerminalFocus))
            }
            AppEvent::ToggleHideIdleRepositories => {
                ControlFlow::Break(Self::UiNavigation(U::ToggleHideIdleRepositories))
            }
            other => ControlFlow::Continue(other),
        }
    }

    /// Convert multi-agent workbench [`AppEvent`] variants into UI-navigation
    /// messages (issue #626). Split out so the top-level converter stays within
    /// the clippy line budget.
    fn from_workbench_event(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        use UiNavigationMessage as U;
        match event {
            AppEvent::ToggleWorkbenchStatusBucket(bucket) => {
                ControlFlow::Break(Self::UiNavigation(U::ToggleWorkbenchStatusBucket(bucket)))
            }
            AppEvent::WorkbenchFilterCursorPrev => {
                ControlFlow::Break(Self::UiNavigation(U::WorkbenchFilterCursorPrev))
            }
            AppEvent::WorkbenchFilterCursorNext => {
                ControlFlow::Break(Self::UiNavigation(U::WorkbenchFilterCursorNext))
            }
            AppEvent::WorkbenchSelectPrev => {
                ControlFlow::Break(Self::UiNavigation(U::WorkbenchSelectPrev))
            }
            AppEvent::WorkbenchSelectNext => {
                ControlFlow::Break(Self::UiNavigation(U::WorkbenchSelectNext))
            }
            AppEvent::WorkbenchAttach => ControlFlow::Break(Self::UiNavigation(U::WorkbenchAttach)),
            other => ControlFlow::Continue(other),
        }
    }

    /// Convert modal/form [`AppEvent`] variants into modal messages. Split out
    /// so the top-level converter stays within the clippy line budget.
    fn from_modal_event(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::OpenHelp => ControlFlow::Break(Self::Modal(ModalMessage::OpenHelp)),
            AppEvent::OpenSearch => ControlFlow::Break(Self::Modal(ModalMessage::OpenSearch)),
            AppEvent::CloseModal => ControlFlow::Break(Self::Modal(ModalMessage::CloseModal)),
            AppEvent::SubmitForm => ControlFlow::Break(Self::Modal(ModalMessage::SubmitForm)),
            AppEvent::ConfirmCycleFocus => {
                ControlFlow::Break(Self::Modal(ModalMessage::ConfirmCycleFocus))
            }
            AppEvent::FormChar(c) => ControlFlow::Break(Self::Modal(ModalMessage::FormChar(c))),
            AppEvent::FormBackspace => ControlFlow::Break(Self::Modal(ModalMessage::FormBackspace)),
            AppEvent::FormDelete => ControlFlow::Break(Self::Modal(ModalMessage::FormDelete)),
            AppEvent::FormMoveCursorLeft => {
                ControlFlow::Break(Self::Modal(ModalMessage::FormMoveCursorLeft))
            }
            AppEvent::FormMoveCursorRight => {
                ControlFlow::Break(Self::Modal(ModalMessage::FormMoveCursorRight))
            }
            AppEvent::FormMoveCursorStart => {
                ControlFlow::Break(Self::Modal(ModalMessage::FormMoveCursorStart))
            }
            AppEvent::FormMoveCursorEnd => {
                ControlFlow::Break(Self::Modal(ModalMessage::FormMoveCursorEnd))
            }
            AppEvent::FormNextField => ControlFlow::Break(Self::Modal(ModalMessage::FormNextField)),
            AppEvent::FormPrevField => ControlFlow::Break(Self::Modal(ModalMessage::FormPrevField)),
            AppEvent::FormToggleCheckbox => {
                ControlFlow::Break(Self::Modal(ModalMessage::FormToggleCheckbox))
            }
            other => ControlFlow::Continue(other),
        }
    }

    /// Convert split-mode, dashboard-grab, and terminal-scrollback
    /// [`AppEvent`] variants into UI-navigation messages. Split out so the
    /// top-level converter stays within the clippy line budget.
    fn from_split_grab_or_scroll_event(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        use UiNavigationMessage as U;
        match event {
            AppEvent::EnterSplitMode => ControlFlow::Break(Self::UiNavigation(U::EnterSplitMode)),
            AppEvent::ExitSplitMode => ControlFlow::Break(Self::UiNavigation(U::ExitSplitMode)),
            AppEvent::EnterGrabMode => ControlFlow::Break(Self::UiNavigation(U::EnterGrabMode)),
            AppEvent::ExitGrabMode => ControlFlow::Break(Self::UiNavigation(U::ExitGrabMode)),
            AppEvent::GrabMoveUp => ControlFlow::Break(Self::UiNavigation(U::GrabMoveUp)),
            AppEvent::GrabMoveDown => ControlFlow::Break(Self::UiNavigation(U::GrabMoveDown)),
            AppEvent::SetSplitFilter(filter) => {
                ControlFlow::Break(Self::UiNavigation(U::SetSplitFilter(filter)))
            }
            AppEvent::EnterDashboardGrab => {
                ControlFlow::Break(Self::UiNavigation(U::EnterDashboardGrab))
            }
            AppEvent::ExitDashboardGrab => {
                ControlFlow::Break(Self::UiNavigation(U::ExitDashboardGrab))
            }
            AppEvent::DashboardGrabMoveUp => {
                ControlFlow::Break(Self::UiNavigation(U::DashboardGrabMoveUp))
            }
            AppEvent::DashboardGrabMoveDown => {
                ControlFlow::Break(Self::UiNavigation(U::DashboardGrabMoveDown))
            }
            // Terminal scrollback viewport events (issue #198).
            AppEvent::TerminalScrollUp => {
                ControlFlow::Break(Self::UiNavigation(U::TerminalScrollUp))
            }
            AppEvent::TerminalScrollDown => {
                ControlFlow::Break(Self::UiNavigation(U::TerminalScrollDown))
            }
            AppEvent::TerminalScrollPageUp => {
                ControlFlow::Break(Self::UiNavigation(U::TerminalScrollPageUp))
            }
            AppEvent::TerminalScrollPageDown => {
                ControlFlow::Break(Self::UiNavigation(U::TerminalScrollPageDown))
            }
            AppEvent::TerminalFollowTail => {
                ControlFlow::Break(Self::UiNavigation(U::TerminalFollowTail))
            }
            AppEvent::TerminalScrollToTop => {
                ControlFlow::Break(Self::UiNavigation(U::TerminalScrollToTop))
            }
            // Shell-overlay events (issue #222).
            AppEvent::OpenShellOverlay => {
                ControlFlow::Break(Self::UiNavigation(U::OpenShellOverlay))
            }
            AppEvent::CloseShellOverlay => {
                ControlFlow::Break(Self::UiNavigation(U::CloseShellOverlay))
            }
            AppEvent::HideShellOverlay => {
                ControlFlow::Break(Self::UiNavigation(U::HideShellOverlay))
            }
            AppEvent::ResumeShellOverlay(agent_id) => {
                ControlFlow::Break(Self::UiNavigation(U::ResumeShellOverlay(agent_id)))
            }
            other => ControlFlow::Continue(other),
        }
    }

    /// Convert non-UI-navigation [`AppEvent`] variants into the typed message bus.
    ///
    /// Split out from [`AppMessage::from`] so the top-level converter stays
    /// within the clippy line budget without a complexity suppression.
    fn from_non_ui_nav_event(event: AppEvent) -> Self {
        match event {
            AppEvent::ToggleWorkbenchStatusBucket(_)
            | AppEvent::WorkbenchFilterCursorPrev
            | AppEvent::WorkbenchFilterCursorNext
            | AppEvent::WorkbenchSelectPrev
            | AppEvent::WorkbenchSelectNext
            | AppEvent::WorkbenchAttach => match Self::from_workbench_event(event) {
                ControlFlow::Break(message) => message,
                ControlFlow::Continue(unclaimed) => Self::from_unrouted_event(unclaimed),
            },
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
            AppEvent::StageDurableSave => Self::Persistence(PersistenceMessage::StageSave),
            AppEvent::ThemeResolveFailed(error) => Self::Theme(ThemeMessage::ResolveFailed(error)),
            AppEvent::Settings(message) => Self::Settings(message),
            AppEvent::Provider(message) => Self::Provider(message),
            other => Self::from_system_event(other),
        }
    }

    /// System-channel events (quit, error/warning clearing, auth remediation
    /// from issue #244, transient-agent queueing).
    fn from_system_event(event: AppEvent) -> Self {
        match event {
            AppEvent::Quit => Self::System(SystemMessage::Quit),
            AppEvent::ClearError => Self::System(SystemMessage::ClearError),
            AppEvent::ClearWarning => Self::System(SystemMessage::ClearWarning),
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

    /// Convert issues-domain [`AppEvent`] variants into the typed message bus,
    /// or hand the event to the next domain try-converter.
    fn from_issues_event(event: AppEvent) -> Self {
        match IssuesMessage::try_from_app_event(event) {
            ControlFlow::Break(message) => Self::Issues(message),
            ControlFlow::Continue(event) => Self::from_actions_event(event),
        }
    }

    /// Convert actions-domain [`AppEvent`] variants into the typed message bus,
    /// or hand the event to the next domain try-converter.
    fn from_actions_event(event: AppEvent) -> Self {
        match ActionsMessage::try_from_app_event(event) {
            ControlFlow::Break(message) => Self::Actions(message),
            ControlFlow::Continue(event) => Self::from_errors_event(event),
        }
    }

    /// Convert errors-domain [`AppEvent`] variants into the typed message bus,
    /// or hand the event to the next domain try-converter.
    fn from_errors_event(event: AppEvent) -> Self {
        match ErrorsMessage::try_from_app_event(event) {
            ControlFlow::Break(message) => Self::Errors(message),
            ControlFlow::Continue(event) => Self::from_terminal_manager_event(event),
        }
    }

    /// Convert terminal-manager-domain [`AppEvent`] variants into the typed
    /// message bus, or hand the event to the PR try-converter.
    fn from_terminal_manager_event(event: AppEvent) -> Self {
        match TerminalManagerMessage::try_from_app_event(event) {
            ControlFlow::Break(message) => Self::TerminalManager(message),
            ControlFlow::Continue(event) => Self::from_prs_event(event),
        }
    }

    /// Convert PR-domain [`AppEvent`] variants into the typed message bus.
    ///
    /// @pseudocode component-004 lines 46-50
    fn from_prs_event(event: AppEvent) -> Self {
        match PullRequestsMessage::try_from_app_event(event) {
            ControlFlow::Break(message) => Self::PullRequests(message),
            ControlFlow::Continue(unclaimed) => Self::from_unrouted_event(unclaimed),
        }
    }

    /// Report an event that no message domain claimed.
    ///
    /// The try-converter chain above is exhaustive over `AppEvent`, so this is
    /// only reachable when a new variant is added without a converter arm.
    /// Drift is surfaced on the errors screen (mirroring the panic-capture
    /// route) instead of crashing the TUI.
    fn from_unrouted_event(event: AppEvent) -> Self {
        Self::Errors(ErrorsMessage::CaptureSilent {
            title: "Unroutable AppEvent".to_owned(),
            detail: format!("{event:?} matched no message domain"),
            source: ErrorSource::Panic,
            timestamp: unix_timestamp(),
        })
    }
}

/// Unix epoch seconds used to stamp a captured converter-drift error, matching
/// the panic-capture timestamp convention.
fn unix_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(
            |_| "0".to_owned(),
            |duration| duration.as_secs().to_string(),
        )
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
            AppMessage::Provider(message) => Self::Provider(message),
            AppMessage::TerminalManager(message) => message.into(),
            AppMessage::System(message) => message.into(),
            AppMessage::EffectCompletion(completion) => Self::EffectCompletion(completion),
        }
    }
}

impl From<UiNavigationMessage> for AppEvent {
    fn from(message: UiNavigationMessage) -> Self {
        match message {
            UiNavigationMessage::Back => Self::Back,
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
            UiNavigationMessage::HideShellOverlay => Self::HideShellOverlay,
            UiNavigationMessage::ResumeShellOverlay(agent_id) => Self::ResumeShellOverlay(agent_id),
            UiNavigationMessage::ToggleWorkbenchStatusBucket(bucket) => {
                Self::ToggleWorkbenchStatusBucket(bucket)
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_durable_save_routes_to_persistence() {
        assert!(matches!(
            AppMessage::from(AppEvent::StageDurableSave),
            AppMessage::Persistence(PersistenceMessage::StageSave)
        ));
    }

    #[test]
    fn domain_events_route_through_try_converters() {
        assert!(matches!(
            AppMessage::from(AppEvent::Quit),
            AppMessage::System(SystemMessage::Quit)
        ));
        assert!(matches!(
            AppMessage::from(AppEvent::ErrorsNavigateDown),
            AppMessage::Errors(ErrorsMessage::Navigate(crate::messages::NavDir::Down))
        ));
        assert!(matches!(
            AppMessage::from(AppEvent::TerminalManagerNavigateUp),
            AppMessage::TerminalManager(TerminalManagerMessage::Navigate(
                crate::messages::NavDir::Up
            ))
        ));
    }
}
