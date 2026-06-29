//! `AppEvent` <-> `PullRequestsMessage` conversion.
//!
//! @plan PLAN-20260624-PR-MODE.P05
//! @requirement REQ-PR-002
//! @pseudocode component-004 lines 45-85

use crate::state::AppEvent;

use super::{NavDir, PrFilterField, PrInlineMsg, PullRequestsMessage, ScrollDir};

impl From<PullRequestsMessage> for AppEvent {
    /// Delegate to [`PullRequestsMessage::into_app_event`].
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-002
    /// @pseudocode component-004 lines 68-85
    fn from(message: PullRequestsMessage) -> Self {
        message.into_app_event()
    }
}

impl PullRequestsMessage {
    /// Convert a PR-domain [`AppEvent`] into the typed message.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 51-67
    pub(super) fn from_app_event(event: AppEvent) -> Self {
        match event {
            AppEvent::EnterPrsMode
            | AppEvent::ExitPrsMode
            | AppEvent::RefocusPrList
            | AppEvent::PrNavigateUp
            | AppEvent::PrNavigateDown
            | AppEvent::PrNavigatePageUp
            | AppEvent::PrNavigatePageDown
            | AppEvent::PrNavigateHome
            | AppEvent::PrNavigateEnd
            | AppEvent::PrListEnter
            | AppEvent::PrCycleFocus
            | AppEvent::PrCycleFocusReverse
            | AppEvent::PrScrollDetailUp
            | AppEvent::PrScrollDetailDown
            | AppEvent::PrScrollDetailPageUp
            | AppEvent::PrScrollDetailPageDown
            | AppEvent::PrDetailSubfocusNext
            | AppEvent::PrDetailSubfocusPrev => Self::from_app_event_navigation(event),
            other => Self::from_app_event_payload(other),
        }
    }

    /// Navigation and scroll events that carry no payload.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 51-67
    fn from_app_event_navigation(event: AppEvent) -> Self {
        match event {
            AppEvent::EnterPrsMode => Self::EnterMode,
            AppEvent::ExitPrsMode => Self::ExitMode,
            AppEvent::RefocusPrList => Self::RefocusList,
            AppEvent::PrNavigateUp => Self::Navigate(NavDir::Up),
            AppEvent::PrNavigateDown => Self::Navigate(NavDir::Down),
            AppEvent::PrNavigatePageUp => Self::Navigate(NavDir::PageUp),
            AppEvent::PrNavigatePageDown => Self::Navigate(NavDir::PageDown),
            AppEvent::PrNavigateHome => Self::Navigate(NavDir::Home),
            AppEvent::PrNavigateEnd => Self::Navigate(NavDir::End),
            AppEvent::PrListEnter => Self::Enter,
            AppEvent::PrCycleFocus => Self::CycleFocus,
            AppEvent::PrCycleFocusReverse => Self::CycleFocusReverse,
            AppEvent::PrScrollDetailUp => Self::ScrollDetail(ScrollDir::Up),
            AppEvent::PrScrollDetailDown => Self::ScrollDetail(ScrollDir::Down),
            AppEvent::PrScrollDetailPageUp => Self::ScrollDetail(ScrollDir::PageUp),
            AppEvent::PrScrollDetailPageDown => Self::ScrollDetail(ScrollDir::PageDown),
            AppEvent::PrDetailSubfocusNext => Self::DetailSubfocusNext,
            AppEvent::PrDetailSubfocusPrev => Self::DetailSubfocusPrev,
            _ => unreachable!("non-navigation AppEvent routed to navigation converter"),
        }
    }

    /// Loaded/error payload events and data/filter/mutation/agent variants.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 51-67
    fn from_app_event_payload(event: AppEvent) -> Self {
        match event {
            AppEvent::PrListLoaded { .. }
            | AppEvent::PrListLoadFailed { .. }
            | AppEvent::PrListPageLoaded { .. } => Self::from_app_event_list(event),
            AppEvent::PrDetailLoaded { .. } | AppEvent::PrDetailLoadFailed { .. } => {
                Self::from_app_event_detail(event)
            }
            AppEvent::PrCommentsPageLoaded { .. } | AppEvent::PrCommentsPageFailed { .. } => {
                Self::from_app_event_comments(event)
            }
            other => Self::from_app_event_controls(other),
        }
    }

    /// List loaded/error payload events.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 51-67
    fn from_app_event_list(event: AppEvent) -> Self {
        match event {
            AppEvent::PrListLoaded {
                scope_repo_id,
                filter,
                request_id,
                pull_requests,
                cursor,
                has_more,
            } => Self::ListLoaded {
                scope_repo_id,
                filter,
                request_id,
                pull_requests,
                cursor,
                has_more,
            },
            AppEvent::PrListLoadFailed {
                scope_repo_id,
                request_id,
                error,
            } => Self::ListLoadFailed {
                scope_repo_id,
                request_id,
                error,
            },
            AppEvent::PrListPageLoaded {
                scope_repo_id,
                request_id,
                pull_requests,
                cursor,
                has_more,
            } => Self::ListPageLoaded {
                scope_repo_id,
                request_id,
                pull_requests,
                cursor,
                has_more,
            },
            _ => unreachable!("non-list AppEvent routed to list converter"),
        }
    }

    /// Detail loaded/error payload events.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 51-67
    fn from_app_event_detail(event: AppEvent) -> Self {
        match event {
            AppEvent::PrDetailLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                detail,
            } => Self::DetailLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                detail,
            },
            AppEvent::PrDetailLoadFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            } => Self::DetailLoadFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            },
            _ => unreachable!("non-detail AppEvent routed to detail converter"),
        }
    }

    /// Comments page loaded/failed payload events.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 51-67
    fn from_app_event_comments(event: AppEvent) -> Self {
        match event {
            AppEvent::PrCommentsPageLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                comments,
                cursor,
                has_more,
            } => Self::CommentsPageLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                comments,
                cursor,
                has_more,
            },
            AppEvent::PrCommentsPageFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            } => Self::CommentsPageFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            },
            _ => unreachable!("non-comments AppEvent routed to comments converter"),
        }
    }

    /// Filter controls, search, composer, inline, mutation, agent, notice, browser.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 51-67
    fn from_app_event_controls(event: AppEvent) -> Self {
        match event {
            AppEvent::PrOpenFilterControls => Self::OpenFilterControls,
            AppEvent::PrCloseFilterControls => Self::CloseFilterControls,
            AppEvent::PrApplyFilter => Self::ApplyFilter,
            AppEvent::PrClearFilter => Self::ClearFilter,
            AppEvent::PrFilterNavigateNext => Self::FilterNavigate(NavDir::Next),
            AppEvent::PrFilterNavigatePrev => Self::FilterNavigate(NavDir::Prev),
            AppEvent::PrCycleFilterState => Self::CycleFilterState,
            AppEvent::PrCycleDraftFilter => Self::CycleDraftFilter,
            AppEvent::PrCycleReviewFilter => Self::CycleReviewFilter,
            AppEvent::PrCycleChecksFilter => Self::CycleChecksFilter,
            AppEvent::PrFocusSearchInput => Self::FocusSearchInput,
            AppEvent::PrBlurSearchInput => Self::BlurSearchInput,
            AppEvent::PrSetSearchQuery { query } => Self::SetSearchQuery { query },
            AppEvent::PrApplySearch => Self::ApplySearch,
            AppEvent::PrClearSearch => Self::ClearSearch,
            other => Self::from_app_event_composer_and_inline(other),
        }
    }

    /// Composer open, inline editor, mutation, agent, notice, browser.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 51-67
    fn from_app_event_composer_and_inline(event: AppEvent) -> Self {
        match event {
            AppEvent::PrUpdateDraftFilter { field, value } => Self::UpdateDraftFilter {
                field: PrFilterField::from_string(&field),
                value,
            },
            AppEvent::PrOpenNewCommentComposer => Self::OpenNewCommentComposer,
            AppEvent::PrOpenReplyComposer { comment_index } => {
                Self::OpenReplyComposer { comment_index }
            }
            AppEvent::PrInlineChar(c) => Self::Inline(PrInlineMsg::Char(c)),
            AppEvent::PrInlineNewline => Self::Inline(PrInlineMsg::Newline),
            AppEvent::PrInlineBackspace => Self::Inline(PrInlineMsg::Backspace),
            AppEvent::PrInlineDelete => Self::Inline(PrInlineMsg::Delete),
            AppEvent::PrInlineCursorLeft => Self::Inline(PrInlineMsg::CursorLeft),
            AppEvent::PrInlineCursorRight => Self::Inline(PrInlineMsg::CursorRight),
            AppEvent::PrInlineCursorUp => Self::Inline(PrInlineMsg::CursorUp),
            AppEvent::PrInlineCursorDown => Self::Inline(PrInlineMsg::CursorDown),
            AppEvent::PrInlineSubmit => Self::Inline(PrInlineMsg::Submit),
            AppEvent::PrInlineCancelOrEsc => Self::Inline(PrInlineMsg::CancelOrEsc),
            other => Self::from_app_event_mutation_and_agent(other),
        }
    }

    /// Mutation lifecycle, agent chooser, notice, browser variants.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-002
    /// @pseudocode component-004 lines 51-67
    fn from_app_event_mutation_and_agent(event: AppEvent) -> Self {
        match event {
            AppEvent::PrCommentCreated {
                scope_repo_id,
                pr_number,
                mutation_id,
                comment,
            } => Self::CommentCreated {
                scope_repo_id,
                pr_number,
                mutation_id,
                comment,
            },
            AppEvent::PrCommentCreateFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            } => Self::CommentCreateFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            },
            AppEvent::PrMutationFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            } => Self::MutationFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            },
            AppEvent::PrShowNotice(kind) => Self::ShowNotice(kind),
            AppEvent::PrOpenInBrowser => Self::OpenInBrowser,
            AppEvent::PrOpenedInBrowser {
                scope_repo_id,
                pr_number,
            } => Self::OpenedInBrowser {
                scope_repo_id,
                pr_number,
            },
            AppEvent::PrOpenInBrowserFailed {
                scope_repo_id,
                pr_number,
                error,
            } => Self::OpenInBrowserFailed {
                scope_repo_id,
                pr_number,
                error,
            },
            other => Self::from_app_event_agent(other),
        }
    }

    /// Agent chooser variants.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 51-67
    fn from_app_event_agent(event: AppEvent) -> Self {
        match event {
            AppEvent::PrOpenAgentChooser => Self::OpenAgentChooser,
            AppEvent::PrAgentChooserNavigateUp => Self::AgentChooserNavigate(NavDir::Up),
            AppEvent::PrAgentChooserNavigateDown => Self::AgentChooserNavigate(NavDir::Down),
            AppEvent::PrAgentChooserConfirm => Self::AgentChooserConfirm,
            AppEvent::PrAgentChooserCancel => Self::AgentChooserCancel,
            AppEvent::PrSendToAgentCompleted => Self::SendToAgentCompleted,
            AppEvent::PrSendToAgentFailed { error } => Self::SendToAgentFailed { error },
            _ => unreachable!("non-PR AppEvent routed to PR converter"),
        }
    }

    /// Convert this PR-domain message back into the [`AppEvent`].
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 68-85
    fn into_app_event(self) -> AppEvent {
        match self {
            Self::EnterMode
            | Self::ExitMode
            | Self::RefocusList
            | Self::Navigate(_)
            | Self::Enter
            | Self::CycleFocus
            | Self::CycleFocusReverse
            | Self::ScrollDetail(_)
            | Self::DetailSubfocusNext
            | Self::DetailSubfocusPrev => self.into_app_event_navigation(),
            other => other.into_app_event_data(),
        }
    }

    /// Navigation and scroll messages that carry no payload.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 68-85
    fn into_app_event_navigation(self) -> AppEvent {
        match self {
            Self::EnterMode => AppEvent::EnterPrsMode,
            Self::ExitMode => AppEvent::ExitPrsMode,
            Self::RefocusList => AppEvent::RefocusPrList,
            // `Navigate` carries LIST-navigation semantics and is only ever
            // constructed with Up/Down/PageUp/PageDown/Home/End (see the
            // forward map). Next/Prev are filter/chooser field-stepping
            // directions that never reach a list `Navigate`; fold them onto
            // the closest list-nav equivalent (Next=forward=Down, Prev=back=Up)
            // so this stays within the list-nav domain rather than leaking into
            // unrelated filter events.
            Self::Navigate(NavDir::Up | NavDir::Prev) => AppEvent::PrNavigateUp,
            Self::Navigate(NavDir::Down | NavDir::Next) => AppEvent::PrNavigateDown,
            Self::Navigate(NavDir::PageUp) => AppEvent::PrNavigatePageUp,
            Self::Navigate(NavDir::PageDown) => AppEvent::PrNavigatePageDown,
            Self::Navigate(NavDir::Home) => AppEvent::PrNavigateHome,
            Self::Navigate(NavDir::End) => AppEvent::PrNavigateEnd,
            Self::Enter => AppEvent::PrListEnter,
            Self::CycleFocus => AppEvent::PrCycleFocus,
            Self::CycleFocusReverse => AppEvent::PrCycleFocusReverse,
            Self::ScrollDetail(ScrollDir::Up) => AppEvent::PrScrollDetailUp,
            Self::ScrollDetail(ScrollDir::Down) => AppEvent::PrScrollDetailDown,
            Self::ScrollDetail(ScrollDir::PageUp) => AppEvent::PrScrollDetailPageUp,
            Self::ScrollDetail(ScrollDir::PageDown) => AppEvent::PrScrollDetailPageDown,
            Self::DetailSubfocusNext => AppEvent::PrDetailSubfocusNext,
            Self::DetailSubfocusPrev => AppEvent::PrDetailSubfocusPrev,
            _ => unreachable!("non-navigation PullRequestsMessage routed to navigation"),
        }
    }

    /// Loaded/error payloads and control/filter/inline/mutation/agent messages.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 68-85
    fn into_app_event_data(self) -> AppEvent {
        match self {
            Self::ListLoaded { .. } | Self::ListLoadFailed { .. } | Self::ListPageLoaded { .. } => {
                self.into_app_event_list()
            }
            Self::DetailLoaded { .. } | Self::DetailLoadFailed { .. } => {
                self.into_app_event_detail()
            }
            Self::CommentsPageLoaded { .. } | Self::CommentsPageFailed { .. } => {
                self.into_app_event_comments()
            }
            other => other.into_app_event_controls(),
        }
    }

    /// List loaded/error payload messages.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 68-85
    fn into_app_event_list(self) -> AppEvent {
        match self {
            Self::ListLoaded {
                scope_repo_id,
                filter,
                request_id,
                pull_requests,
                cursor,
                has_more,
            } => AppEvent::PrListLoaded {
                scope_repo_id,
                filter,
                request_id,
                pull_requests,
                cursor,
                has_more,
            },
            Self::ListLoadFailed {
                scope_repo_id,
                request_id,
                error,
            } => AppEvent::PrListLoadFailed {
                scope_repo_id,
                request_id,
                error,
            },
            Self::ListPageLoaded {
                scope_repo_id,
                request_id,
                pull_requests,
                cursor,
                has_more,
            } => AppEvent::PrListPageLoaded {
                scope_repo_id,
                request_id,
                pull_requests,
                cursor,
                has_more,
            },
            _ => unreachable!("non-list PullRequestsMessage routed to list converter"),
        }
    }

    /// Detail loaded/error payload messages.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 68-85
    fn into_app_event_detail(self) -> AppEvent {
        match self {
            Self::DetailLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                detail,
            } => AppEvent::PrDetailLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                detail,
            },
            Self::DetailLoadFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            } => AppEvent::PrDetailLoadFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            },
            _ => unreachable!("non-detail PullRequestsMessage routed to detail"),
        }
    }

    /// Comments page loaded/failed payload messages.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 68-85
    fn into_app_event_comments(self) -> AppEvent {
        match self {
            Self::CommentsPageLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                comments,
                cursor,
                has_more,
            } => AppEvent::PrCommentsPageLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                comments,
                cursor,
                has_more,
            },
            Self::CommentsPageFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            } => AppEvent::PrCommentsPageFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            },
            _ => unreachable!("non-comments PullRequestsMessage routed to comments"),
        }
    }

    /// Filter controls, search, composer, inline, mutation, agent, notice, browser.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 68-85
    fn into_app_event_controls(self) -> AppEvent {
        match self {
            Self::OpenFilterControls => AppEvent::PrOpenFilterControls,
            Self::CloseFilterControls => AppEvent::PrCloseFilterControls,
            Self::ApplyFilter => AppEvent::PrApplyFilter,
            Self::ClearFilter => AppEvent::PrClearFilter,
            Self::FilterNavigate(NavDir::Next) => AppEvent::PrFilterNavigateNext,
            Self::FilterNavigate(NavDir::Prev) => AppEvent::PrFilterNavigatePrev,
            Self::CycleFilterState => AppEvent::PrCycleFilterState,
            Self::CycleDraftFilter => AppEvent::PrCycleDraftFilter,
            Self::CycleReviewFilter => AppEvent::PrCycleReviewFilter,
            Self::CycleChecksFilter => AppEvent::PrCycleChecksFilter,
            Self::FocusSearchInput => AppEvent::PrFocusSearchInput,
            Self::BlurSearchInput => AppEvent::PrBlurSearchInput,
            Self::SetSearchQuery { query } => AppEvent::PrSetSearchQuery { query },
            Self::ApplySearch => AppEvent::PrApplySearch,
            Self::ClearSearch => AppEvent::PrClearSearch,
            Self::UpdateDraftFilter { field, value } => AppEvent::PrUpdateDraftFilter {
                field: field.as_string(),
                value,
            },
            other => other.into_app_event_composer_and_inline(),
        }
    }

    /// Composer open, inline editor, mutation, agent, notice, browser.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 68-85
    fn into_app_event_composer_and_inline(self) -> AppEvent {
        match self {
            Self::OpenNewCommentComposer => AppEvent::PrOpenNewCommentComposer,
            Self::OpenReplyComposer { comment_index } => {
                AppEvent::PrOpenReplyComposer { comment_index }
            }
            Self::Inline(PrInlineMsg::Char(c)) => AppEvent::PrInlineChar(c),
            Self::Inline(PrInlineMsg::Newline) => AppEvent::PrInlineNewline,
            Self::Inline(PrInlineMsg::Backspace) => AppEvent::PrInlineBackspace,
            Self::Inline(PrInlineMsg::Delete) => AppEvent::PrInlineDelete,
            Self::Inline(PrInlineMsg::CursorLeft) => AppEvent::PrInlineCursorLeft,
            Self::Inline(PrInlineMsg::CursorRight) => AppEvent::PrInlineCursorRight,
            Self::Inline(PrInlineMsg::CursorUp) => AppEvent::PrInlineCursorUp,
            Self::Inline(PrInlineMsg::CursorDown) => AppEvent::PrInlineCursorDown,
            Self::Inline(PrInlineMsg::Submit) => AppEvent::PrInlineSubmit,
            Self::Inline(PrInlineMsg::CancelOrEsc) => AppEvent::PrInlineCancelOrEsc,
            other => other.into_app_event_mutation_and_agent(),
        }
    }

    /// Mutation lifecycle, notice, browser, agent variants.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 68-85
    fn into_app_event_mutation_and_agent(self) -> AppEvent {
        match self {
            Self::CommentCreated {
                scope_repo_id,
                pr_number,
                mutation_id,
                comment,
            } => AppEvent::PrCommentCreated {
                scope_repo_id,
                pr_number,
                mutation_id,
                comment,
            },
            Self::CommentCreateFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            } => AppEvent::PrCommentCreateFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            },
            Self::MutationFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            } => AppEvent::PrMutationFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            },
            Self::ShowNotice(kind) => AppEvent::PrShowNotice(kind),
            Self::OpenInBrowser => AppEvent::PrOpenInBrowser,
            Self::OpenedInBrowser {
                scope_repo_id,
                pr_number,
            } => AppEvent::PrOpenedInBrowser {
                scope_repo_id,
                pr_number,
            },
            Self::OpenInBrowserFailed {
                scope_repo_id,
                pr_number,
                error,
            } => AppEvent::PrOpenInBrowserFailed {
                scope_repo_id,
                pr_number,
                error,
            },
            other => other.into_app_event_agent(),
        }
    }

    /// Agent chooser variants.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-002
    /// @pseudocode component-004 lines 68-85
    fn into_app_event_agent(self) -> AppEvent {
        match self {
            Self::OpenAgentChooser => AppEvent::PrOpenAgentChooser,
            Self::AgentChooserNavigate(NavDir::Up) => AppEvent::PrAgentChooserNavigateUp,
            Self::AgentChooserNavigate(NavDir::Down) => AppEvent::PrAgentChooserNavigateDown,
            Self::AgentChooserConfirm => AppEvent::PrAgentChooserConfirm,
            Self::AgentChooserCancel => AppEvent::PrAgentChooserCancel,
            Self::SendToAgentCompleted => AppEvent::PrSendToAgentCompleted,
            Self::SendToAgentFailed { error } => AppEvent::PrSendToAgentFailed { error },
            _ => unreachable!("unrouted PullRequestsMessage variant reached agent converter"),
        }
    }
}
