//! `AppEvent` <-> `PullRequestsMessage` conversion.
//!
//! @plan PLAN-20260624-PR-MODE.P05
//! @requirement REQ-PR-002
//! @pseudocode component-004 lines 45-85

use std::ops::ControlFlow;

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
    /// Each layer claims its own variants and hands the residual to the next
    /// layer; an event no PR layer claims returns
    /// [`ControlFlow::Continue`] so the dispatcher can route it elsewhere.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 51-67
    pub(super) fn try_from_app_event(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::EnterPrsMode => ControlFlow::Break(Self::EnterMode),
            AppEvent::ExitPrsMode => ControlFlow::Break(Self::ExitMode),
            AppEvent::RefocusPrList => ControlFlow::Break(Self::RefocusList),
            AppEvent::PrNavigateUp => ControlFlow::Break(Self::Navigate(NavDir::Up)),
            AppEvent::PrNavigateDown => ControlFlow::Break(Self::Navigate(NavDir::Down)),
            AppEvent::PrNavigatePageUp(page) => {
                ControlFlow::Break(Self::Navigate(NavDir::PageUp(page)))
            }
            AppEvent::PrNavigatePageDown(page) => {
                ControlFlow::Break(Self::Navigate(NavDir::PageDown(page)))
            }
            AppEvent::PrNavigateHome => ControlFlow::Break(Self::Navigate(NavDir::Home)),
            AppEvent::PrNavigateEnd => ControlFlow::Break(Self::Navigate(NavDir::End)),
            AppEvent::PrListEnter => ControlFlow::Break(Self::Enter),
            AppEvent::PrCycleFocus => ControlFlow::Break(Self::CycleFocus),
            AppEvent::PrCycleFocusReverse => ControlFlow::Break(Self::CycleFocusReverse),
            AppEvent::PrScrollDetailUp => ControlFlow::Break(Self::ScrollDetail(ScrollDir::Up)),
            AppEvent::PrScrollDetailDown => ControlFlow::Break(Self::ScrollDetail(ScrollDir::Down)),
            AppEvent::PrScrollDetailPageUp => {
                ControlFlow::Break(Self::ScrollDetail(ScrollDir::PageUp))
            }
            AppEvent::PrScrollDetailPageDown => {
                ControlFlow::Break(Self::ScrollDetail(ScrollDir::PageDown))
            }
            AppEvent::PrDetailSubfocusNext => ControlFlow::Break(Self::DetailSubfocusNext),
            AppEvent::PrDetailSubfocusPrev => ControlFlow::Break(Self::DetailSubfocusPrev),
            other => Self::from_app_event_list(other),
        }
    }

    /// List loaded/error payload events.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 51-67
    fn from_app_event_list(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        if let Some(msg) = Self::from_app_event_silent_refresh(&event) {
            return ControlFlow::Break(msg);
        }
        match event {
            AppEvent::PrListLoaded {
                scope_repo_id,
                filter,
                request_id,
                pull_requests,
                cursor,
                has_more,
            } => ControlFlow::Break(Self::ListLoaded {
                scope_repo_id,
                filter,
                request_id,
                pull_requests,
                cursor,
                has_more,
            }),
            AppEvent::PrListLoadFailed {
                scope_repo_id,
                request_id,
                error,
            } => ControlFlow::Break(Self::ListLoadFailed {
                scope_repo_id,
                request_id,
                error,
            }),
            AppEvent::PrListPageLoaded {
                scope_repo_id,
                request_id,
                pull_requests,
                cursor,
                has_more,
            } => ControlFlow::Break(Self::ListPageLoaded {
                scope_repo_id,
                request_id,
                pull_requests,
                cursor,
                has_more,
            }),
            other => Self::from_app_event_detail(other),
        }
    }

    /// Silent-refresh list events (issue #128).
    fn from_app_event_silent_refresh(event: &AppEvent) -> Option<Self> {
        match event {
            AppEvent::PrListSilentRefreshed {
                scope_repo_id,
                filter,
                request_id,
                pull_requests,
                cursor,
                has_more,
            } => Some(Self::ListSilentRefreshed {
                scope_repo_id: scope_repo_id.clone(),
                filter: filter.clone(),
                request_id: *request_id,
                pull_requests: pull_requests.clone(),
                cursor: cursor.clone(),
                has_more: *has_more,
            }),
            AppEvent::PrListSilentRefreshFailed {
                scope_repo_id,
                request_id,
            } => Some(Self::ListSilentRefreshFailed {
                scope_repo_id: scope_repo_id.clone(),
                request_id: *request_id,
            }),
            _ => None,
        }
    }

    /// Detail loaded/error payload events.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 51-67
    fn from_app_event_detail(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::PrDetailLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                detail,
            } => ControlFlow::Break(Self::DetailLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                detail,
            }),
            AppEvent::PrDetailLoadFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            } => ControlFlow::Break(Self::DetailLoadFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            }),
            AppEvent::PrDetailAuthRequired(scope_repo_id, pr_number, request_id) => {
                ControlFlow::Break(Self::DetailAuthRequired {
                    scope_repo_id,
                    pr_number,
                    request_id,
                })
            }
            AppEvent::PrDetailSilentRefreshed {
                scope_repo_id,
                pr_number,
                request_id,
                detail,
            } => ControlFlow::Break(Self::DetailSilentRefreshed {
                scope_repo_id,
                pr_number,
                request_id,
                detail,
            }),
            AppEvent::PrDetailSilentRefreshFailed {
                scope_repo_id,
                pr_number,
                request_id,
            } => ControlFlow::Break(Self::DetailSilentRefreshFailed {
                scope_repo_id,
                pr_number,
                request_id,
            }),
            other => Self::from_app_event_comments(other),
        }
    }

    /// Comments page loaded/failed payload events.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 51-67
    fn from_app_event_comments(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::PrCommentsPageLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                comments,
                cursor,
                has_more,
            } => ControlFlow::Break(Self::CommentsPageLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                comments,
                cursor,
                has_more,
            }),
            AppEvent::PrCommentsPageFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            } => ControlFlow::Break(Self::CommentsPageFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            }),
            AppEvent::PrCommentsPageDispatchFailed {
                scope_repo_id,
                pr_number,
                error,
            } => ControlFlow::Break(Self::CommentsPageDispatchFailed {
                scope_repo_id,
                pr_number,
                error,
            }),
            other => Self::from_app_event_controls(other),
        }
    }

    /// Filter controls, search, composer, inline, mutation, agent, notice, browser.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 51-67
    fn from_app_event_controls(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        let event = match Self::changes_from_app_event(event) {
            ControlFlow::Break(message) => return ControlFlow::Break(message),
            ControlFlow::Continue(event) => event,
        };
        match event {
            AppEvent::PrOpenFilterControls => ControlFlow::Break(Self::OpenFilterControls),
            AppEvent::PrCloseFilterControls => ControlFlow::Break(Self::CloseFilterControls),
            AppEvent::PrApplyFilter => ControlFlow::Break(Self::ApplyFilter),
            AppEvent::PrClearFilter => ControlFlow::Break(Self::ClearFilter),
            AppEvent::PrFilterNavigateNext => {
                ControlFlow::Break(Self::FilterNavigate(NavDir::Next))
            }
            AppEvent::PrFilterNavigatePrev => {
                ControlFlow::Break(Self::FilterNavigate(NavDir::Prev))
            }
            AppEvent::PrCycleFilterState => ControlFlow::Break(Self::CycleFilterState),
            AppEvent::PrCycleDraftFilter => ControlFlow::Break(Self::CycleDraftFilter),
            AppEvent::PrCycleReviewFilter => ControlFlow::Break(Self::CycleReviewFilter),
            AppEvent::PrCycleChecksFilter => ControlFlow::Break(Self::CycleChecksFilter),
            AppEvent::PrCycleSortByNext => ControlFlow::Break(Self::PrCycleSortByNext),
            AppEvent::PrCycleSortByPrev => ControlFlow::Break(Self::PrCycleSortByPrev),
            AppEvent::PrToggleSortOrder => ControlFlow::Break(Self::PrToggleSortOrder),
            AppEvent::PrFocusSearchInput => ControlFlow::Break(Self::FocusSearchInput),
            AppEvent::PrBlurSearchInput => ControlFlow::Break(Self::BlurSearchInput),
            AppEvent::PrSetSearchQuery { query } => {
                ControlFlow::Break(Self::SetSearchQuery { query })
            }
            AppEvent::PrApplySearch => ControlFlow::Break(Self::ApplySearch),
            AppEvent::PrClearSearch => ControlFlow::Break(Self::ClearSearch),
            other => Self::from_app_event_composer_and_inline(other),
        }
    }

    /// Composer open, inline editor, mutation, agent, notice, browser.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 51-67
    fn from_app_event_composer_and_inline(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::PrUpdateDraftFilter { field, value } => {
                ControlFlow::Break(Self::UpdateDraftFilter {
                    field: PrFilterField::from_string(&field),
                    value,
                })
            }
            AppEvent::PrOpenNewCommentComposer => ControlFlow::Break(Self::OpenNewCommentComposer),
            AppEvent::PrOpenReplyComposer { comment_index } => {
                ControlFlow::Break(Self::OpenReplyComposer { comment_index })
            }
            AppEvent::PrInlineChar(c) => ControlFlow::Break(Self::Inline(PrInlineMsg::Char(c))),
            AppEvent::PrInlineNewline => ControlFlow::Break(Self::Inline(PrInlineMsg::Newline)),
            AppEvent::PrInlineBackspace => ControlFlow::Break(Self::Inline(PrInlineMsg::Backspace)),
            AppEvent::PrInlineDelete => ControlFlow::Break(Self::Inline(PrInlineMsg::Delete)),
            AppEvent::PrInlineCursorLeft => {
                ControlFlow::Break(Self::Inline(PrInlineMsg::CursorLeft))
            }
            AppEvent::PrInlineCursorRight => {
                ControlFlow::Break(Self::Inline(PrInlineMsg::CursorRight))
            }
            AppEvent::PrInlineCursorUp => ControlFlow::Break(Self::Inline(PrInlineMsg::CursorUp)),
            AppEvent::PrInlineCursorDown => {
                ControlFlow::Break(Self::Inline(PrInlineMsg::CursorDown))
            }
            AppEvent::PrInlineCursorHome => {
                ControlFlow::Break(Self::Inline(PrInlineMsg::CursorHome))
            }
            AppEvent::PrInlineCursorEnd => ControlFlow::Break(Self::Inline(PrInlineMsg::CursorEnd)),
            AppEvent::PrInlineSubmit => ControlFlow::Break(Self::Inline(PrInlineMsg::Submit)),
            AppEvent::PrInlineCancelOrEsc => {
                ControlFlow::Break(Self::Inline(PrInlineMsg::CancelOrEsc))
            }
            other => Self::from_app_event_mutation_and_agent(other),
        }
    }

    /// Mutation lifecycle, agent chooser, notice, browser variants.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-002
    /// @pseudocode component-004 lines 51-67
    fn from_app_event_mutation_and_agent(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::PrCommentCreated {
                scope_repo_id,
                pr_number,
                mutation_id,
                comment,
            } => ControlFlow::Break(Self::CommentCreated {
                scope_repo_id,
                pr_number,
                mutation_id,
                comment,
            }),
            AppEvent::PrCommentCreateFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            } => ControlFlow::Break(Self::CommentCreateFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            }),
            AppEvent::PrMutationFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            } => ControlFlow::Break(Self::MutationFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            }),
            AppEvent::PrShowNotice(kind) => ControlFlow::Break(Self::ShowNotice(kind)),
            AppEvent::PrOpenInBrowser => ControlFlow::Break(Self::OpenInBrowser),
            AppEvent::PrOpenedInBrowser {
                scope_repo_id,
                pr_number,
            } => ControlFlow::Break(Self::OpenedInBrowser {
                scope_repo_id,
                pr_number,
            }),
            AppEvent::PrOpenInBrowserFailed {
                scope_repo_id,
                pr_number,
                error,
            } => ControlFlow::Break(Self::OpenInBrowserFailed {
                scope_repo_id,
                pr_number,
                error,
            }),
            other => Self::from_app_event_agent(other),
        }
    }

    /// Agent chooser variants.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 51-67
    fn from_app_event_agent(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::PrOpenAgentChooser { metadata } => {
                ControlFlow::Break(Self::OpenAgentChooser { metadata })
            }
            AppEvent::BeginPrListSendDetail(metadata) => {
                ControlFlow::Break(Self::BeginListSendDetail { metadata })
            }
            AppEvent::CancelPrListSendDetail => ControlFlow::Break(Self::CancelListSendDetail),
            AppEvent::PrListSendDetailReady {
                scope_repo_id,
                pr_number,
                request_id,
            } => ControlFlow::Break(Self::ListSendDetailReady {
                scope_repo_id,
                pr_number,
                request_id,
            }),
            AppEvent::PrAgentChooserNavigateUp => {
                ControlFlow::Break(Self::AgentChooserNavigate(NavDir::Up))
            }
            AppEvent::PrAgentChooserNavigateDown => {
                ControlFlow::Break(Self::AgentChooserNavigate(NavDir::Down))
            }
            AppEvent::PrAgentChooserConfirm => ControlFlow::Break(Self::AgentChooserConfirm),
            AppEvent::PrAgentChooserCancel => ControlFlow::Break(Self::AgentChooserCancel),
            AppEvent::PrSendToAgentCompleted => ControlFlow::Break(Self::SendToAgentCompleted),
            AppEvent::PrSendToAgentFailed { error } => {
                ControlFlow::Break(Self::SendToAgentFailed { error })
            }
            other => Self::from_app_event_lifecycle(other),
        }
    }

    /// Whether an `AppEvent` is a PR property-editor event.
    pub(super) fn is_pr_property_app_event(event: &AppEvent) -> bool {
        matches!(
            event,
            AppEvent::PrOpenPropertyEditor { .. }
                | AppEvent::PrPropertyEditorNavigateUp
                | AppEvent::PrPropertyEditorNavigateDown
                | AppEvent::PrPropertyEditorToggle
                | AppEvent::PrPropertyEditorConfirm
                | AppEvent::PrPropertyEditorCancel
                | AppEvent::PrPropertyEditorTitleChar(_)
                | AppEvent::PrPropertyEditorTitleBackspace
                | AppEvent::PrPropertyEditorTitleDelete
                | AppEvent::PrPropertyEditorTitleCursorLeft
                | AppEvent::PrPropertyEditorTitleCursorRight
                | AppEvent::PrPropertyEditorTitleCursorHome
                | AppEvent::PrPropertyEditorTitleCursorEnd
                | AppEvent::PrPropertyEditorOptionsLoaded { .. }
                | AppEvent::PrPropertyEditorOptionsFailed { .. }
                | AppEvent::PrPropertyEditSucceeded { .. }
                | AppEvent::PrPostMutationRefreshStarted
                | AppEvent::PrPropertyEditFailed { .. }
                | AppEvent::PrPropertyEditorValidationError { .. }
        )
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
            other => other.into_app_event_list(),
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
            Self::Navigate(NavDir::PageUp(page)) => AppEvent::PrNavigatePageUp(page),
            Self::Navigate(NavDir::PageDown(page)) => AppEvent::PrNavigatePageDown(page),
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
            other => other.into_app_event_list(),
        }
    }

    /// List loaded/error payload messages.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 68-85
    fn into_app_event_list(self) -> AppEvent {
        if let Some(event) = self.silent_refresh_to_app_event() {
            return event;
        }
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
            other => other.into_app_event_detail(),
        }
    }

    /// Convert silent-refresh PR messages back into `AppEvent` (issue #128).
    fn silent_refresh_to_app_event(&self) -> Option<AppEvent> {
        match self {
            Self::ListSilentRefreshed {
                scope_repo_id,
                filter,
                request_id,
                pull_requests,
                cursor,
                has_more,
            } => Some(AppEvent::PrListSilentRefreshed {
                scope_repo_id: scope_repo_id.clone(),
                filter: filter.clone(),
                request_id: *request_id,
                pull_requests: pull_requests.clone(),
                cursor: cursor.clone(),
                has_more: *has_more,
            }),
            Self::ListSilentRefreshFailed {
                scope_repo_id,
                request_id,
            } => Some(AppEvent::PrListSilentRefreshFailed {
                scope_repo_id: scope_repo_id.clone(),
                request_id: *request_id,
            }),
            _ => None,
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
            Self::DetailAuthRequired {
                scope_repo_id,
                pr_number,
                request_id,
            } => AppEvent::PrDetailAuthRequired(scope_repo_id, pr_number, request_id),
            Self::DetailSilentRefreshed {
                scope_repo_id,
                pr_number,
                request_id,
                detail,
            } => AppEvent::PrDetailSilentRefreshed {
                scope_repo_id,
                pr_number,
                request_id,
                detail,
            },
            Self::DetailSilentRefreshFailed {
                scope_repo_id,
                pr_number,
                request_id,
            } => AppEvent::PrDetailSilentRefreshFailed {
                scope_repo_id,
                pr_number,
                request_id,
            },
            other => other.into_app_event_comments(),
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
            Self::CommentsPageDispatchFailed {
                scope_repo_id,
                pr_number,
                error,
            } => AppEvent::PrCommentsPageDispatchFailed {
                scope_repo_id,
                pr_number,
                error,
            },
            other => other.into_app_event_controls(),
        }
    }

    /// Filter controls, search, composer, inline, mutation, agent, notice, browser.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-001
    /// @pseudocode component-004 lines 68-85
    fn into_app_event_controls(self) -> AppEvent {
        let message = match self.changes_into_app_event() {
            ControlFlow::Break(event) => return event,
            ControlFlow::Continue(message) => message,
        };
        match message {
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
            Self::PrCycleSortByNext => AppEvent::PrCycleSortByNext,
            Self::PrCycleSortByPrev => AppEvent::PrCycleSortByPrev,
            Self::PrToggleSortOrder => AppEvent::PrToggleSortOrder,
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
            Self::Inline(PrInlineMsg::CursorHome) => AppEvent::PrInlineCursorHome,
            Self::Inline(PrInlineMsg::CursorEnd) => AppEvent::PrInlineCursorEnd,
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
}
