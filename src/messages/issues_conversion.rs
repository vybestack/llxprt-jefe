use std::ops::ControlFlow;

use crate::state::AppEvent;

use super::IssuesMessage;
use super::names::is_issue_property_app_event;

impl From<IssuesMessage> for AppEvent {
    fn from(message: IssuesMessage) -> Self {
        message.into_app_event()
    }
}

impl IssuesMessage {
    /// Convert an issues-domain [`AppEvent`] into the typed message.
    ///
    /// Layers peel through focused converters; any event no issues layer
    /// claims returns to the dispatcher via [`ControlFlow::Continue`] instead
    /// of panicking, so classifier drift surfaces as a captured error.
    pub(super) fn try_from_app_event(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::EnterIssuesMode => ControlFlow::Break(Self::EnterMode),
            AppEvent::ExitIssuesMode => ControlFlow::Break(Self::ExitMode),
            AppEvent::RefocusIssueList => ControlFlow::Break(Self::RefocusList),
            AppEvent::IssuesNavigateUp => ControlFlow::Break(Self::NavigateUp),
            AppEvent::IssuesNavigateDown => ControlFlow::Break(Self::NavigateDown),
            AppEvent::IssuesNavigatePageUp(page) => ControlFlow::Break(Self::NavigatePageUp(page)),
            AppEvent::IssuesNavigatePageDown(page) => {
                ControlFlow::Break(Self::NavigatePageDown(page))
            }
            AppEvent::IssuesNavigateHome => ControlFlow::Break(Self::NavigateHome),
            AppEvent::IssuesNavigateEnd => ControlFlow::Break(Self::NavigateEnd),
            AppEvent::IssuesEnter => ControlFlow::Break(Self::Enter),
            AppEvent::IssuesCycleFocus => ControlFlow::Break(Self::CycleFocus),
            AppEvent::IssuesCycleFocusReverse => ControlFlow::Break(Self::CycleFocusReverse),
            AppEvent::IssuesScrollDetailUp => ControlFlow::Break(Self::ScrollDetailUp),
            AppEvent::IssuesScrollDetailDown => ControlFlow::Break(Self::ScrollDetailDown),
            AppEvent::IssuesScrollDetailPageUp => ControlFlow::Break(Self::ScrollDetailPageUp),
            AppEvent::IssuesScrollDetailPageDown => ControlFlow::Break(Self::ScrollDetailPageDown),
            AppEvent::IssueDetailSubfocusNext => ControlFlow::Break(Self::DetailSubfocusNext),
            AppEvent::IssueDetailSubfocusPrev => ControlFlow::Break(Self::DetailSubfocusPrev),
            other => Self::from_app_event_list(other),
        }
    }

    /// List loaded/error payload events (silent refresh claimed first).
    fn from_app_event_list(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        if let Some(msg) = Self::from_app_event_silent_refresh(&event) {
            return ControlFlow::Break(msg);
        }
        match event {
            AppEvent::IssueListLoaded {
                scope_repo_id,
                filter,
                request_id,
                issues,
                cursor,
                has_more,
            } => ControlFlow::Break(Self::ListLoaded {
                scope_repo_id,
                filter,
                request_id,
                issues,
                cursor,
                has_more,
            }),
            AppEvent::IssueListLoadFailed {
                scope_repo_id,
                filter,
                request_id,
                request_cursor,
                error,
            } => ControlFlow::Break(Self::ListLoadFailed {
                scope_repo_id,
                filter,
                request_id,
                request_cursor,
                error,
            }),
            AppEvent::IssueListPageLoaded {
                scope_repo_id,
                filter,
                request_id,
                request_cursor,
                issues,
                cursor,
                has_more,
            } => ControlFlow::Break(Self::ListPageLoaded {
                scope_repo_id,
                filter,
                request_id,
                request_cursor,
                issues,
                cursor,
                has_more,
            }),
            other => Self::from_app_event_detail(other),
        }
    }

    /// Detail loaded/error payload events (including silent refresh, issue #175).
    fn from_app_event_detail(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::IssueDetailLoaded {
                scope_repo_id,
                issue_number,
                request_id,
                detail,
            } => ControlFlow::Break(Self::DetailLoaded {
                scope_repo_id,
                issue_number,
                request_id,
                detail,
            }),
            AppEvent::IssueDetailLoadFailed {
                scope_repo_id,
                issue_number,
                request_id,
                error,
            } => ControlFlow::Break(Self::DetailLoadFailed {
                scope_repo_id,
                issue_number,
                request_id,
                error,
            }),
            AppEvent::IssueDetailAuthRequired(scope_repo_id, issue_number, request_id) => {
                ControlFlow::Break(Self::DetailAuthRequired {
                    scope_repo_id,
                    issue_number,
                    request_id,
                })
            }
            AppEvent::IssueDetailSilentRefreshed {
                scope_repo_id,
                issue_number,
                request_id,
                detail,
            } => ControlFlow::Break(Self::DetailSilentRefreshed {
                scope_repo_id,
                issue_number,
                request_id,
                detail,
            }),
            AppEvent::IssueDetailSilentRefreshFailed {
                scope_repo_id,
                issue_number,
                request_id,
            } => ControlFlow::Break(Self::DetailSilentRefreshFailed {
                scope_repo_id,
                issue_number,
                request_id,
            }),
            other => Self::from_app_event_comments_and_controls(other),
        }
    }

    /// Comments payloads, then the control layers.
    fn from_app_event_comments_and_controls(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::IssueCommentsPageLoaded {
                scope_repo_id,
                issue_number,
                request_id,
                request_cursor,
                comments,
                cursor,
                has_more,
            } => ControlFlow::Break(Self::CommentsPageLoaded {
                scope_repo_id,
                issue_number,
                request_id,
                request_cursor,
                comments,
                cursor,
                has_more,
            }),
            AppEvent::IssueCommentsPageFailed {
                scope_repo_id,
                issue_number,
                request_id,
                request_cursor,
                error,
            } => ControlFlow::Break(Self::CommentsPageFailed {
                scope_repo_id,
                issue_number,
                request_id,
                request_cursor,
                error,
            }),
            other => Self::from_app_event_simple_controls(other),
        }
    }

    /// Filter and search controls that carry no cross-domain routing concerns.
    fn from_app_event_simple_controls(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::OpenFilterControls => ControlFlow::Break(Self::OpenFilterControls),
            AppEvent::CloseFilterControls => ControlFlow::Break(Self::CloseFilterControls),
            AppEvent::ApplyFilter => ControlFlow::Break(Self::ApplyFilter),
            AppEvent::ClearFilter => ControlFlow::Break(Self::ClearFilter),
            AppEvent::ClearDraftFilter => ControlFlow::Break(Self::ClearDraftFilter),
            AppEvent::FilterNavigateNext => ControlFlow::Break(Self::FilterNavigateNext),
            AppEvent::FilterNavigatePrev => ControlFlow::Break(Self::FilterNavigatePrev),
            AppEvent::CycleFilterState => ControlFlow::Break(Self::CycleFilterState),
            AppEvent::CycleIssueSortByNext => ControlFlow::Break(Self::CycleIssueSortByNext),
            AppEvent::CycleIssueSortByPrev => ControlFlow::Break(Self::CycleIssueSortByPrev),
            AppEvent::ToggleIssueSortOrder => ControlFlow::Break(Self::ToggleIssueSortOrder),
            AppEvent::FocusSearchInput => ControlFlow::Break(Self::FocusSearchInput),
            AppEvent::BlurSearchInput => ControlFlow::Break(Self::BlurSearchInput),
            AppEvent::SetSearchQuery { query } => {
                ControlFlow::Break(Self::SetSearchQuery { query })
            }
            AppEvent::ApplySearch => ControlFlow::Break(Self::ApplySearch),
            AppEvent::ClearSearch => ControlFlow::Break(Self::ClearSearch),
            AppEvent::UpdateDraftFilter { field, value } => {
                ControlFlow::Break(Self::UpdateDraftFilter { field, value })
            }
            other => Self::from_app_event_composer_and_inline(other),
        }
    }

    /// Composer-open and inline-editor events.
    fn from_app_event_composer_and_inline(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::OpenNewIssueComposer => ControlFlow::Break(Self::OpenNewIssueComposer),
            AppEvent::OpenNewCommentComposer => ControlFlow::Break(Self::OpenNewCommentComposer),
            AppEvent::OpenReplyComposer { comment_index } => {
                ControlFlow::Break(Self::OpenReplyComposer { comment_index })
            }
            AppEvent::OpenInlineEditor { target } => {
                ControlFlow::Break(Self::OpenInlineEditor { target })
            }
            AppEvent::InlineChar(c) => ControlFlow::Break(Self::InlineChar(c)),
            AppEvent::InlineNewline => ControlFlow::Break(Self::InlineNewline),
            AppEvent::InlineBackspace => ControlFlow::Break(Self::InlineBackspace),
            AppEvent::InlineDelete => ControlFlow::Break(Self::InlineDelete),
            AppEvent::InlineCursorLeft => ControlFlow::Break(Self::InlineCursorLeft),
            AppEvent::InlineCursorRight => ControlFlow::Break(Self::InlineCursorRight),
            AppEvent::InlineCursorUp => ControlFlow::Break(Self::InlineCursorUp),
            AppEvent::InlineCursorDown => ControlFlow::Break(Self::InlineCursorDown),
            AppEvent::InlineCursorHome => ControlFlow::Break(Self::InlineCursorHome),
            AppEvent::InlineCursorEnd => ControlFlow::Break(Self::InlineCursorEnd),
            AppEvent::InlineSubmit => ControlFlow::Break(Self::InlineSubmit),
            AppEvent::InlineCancelOrEsc => ControlFlow::Break(Self::InlineCancelOrEsc),
            AppEvent::RequestIssueRewrite => ControlFlow::Break(Self::RequestIssueRewrite),
            AppEvent::IssueRewriteSucceeded { text } => {
                ControlFlow::Break(Self::IssueRewriteSucceeded { text })
            }
            AppEvent::IssueRewriteFailed { error } => {
                ControlFlow::Break(Self::IssueRewriteFailed { error })
            }
            other => Self::from_app_event_new_issue_form(other),
        }
    }

    /// New Issue form events (issue dialogs) — composer open is claimed
    /// upstream, so only the form variants land here.
    fn from_app_event_new_issue_form(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::NewIssueTemplateNext => ControlFlow::Break(Self::NewIssueTemplateNext),
            AppEvent::NewIssueTypeNext => ControlFlow::Break(Self::NewIssueTypeNext),
            AppEvent::NewIssueTitleChar(c) => ControlFlow::Break(Self::NewIssueTitleChar(c)),
            AppEvent::NewIssueTitleBackspace => ControlFlow::Break(Self::NewIssueTitleBackspace),
            AppEvent::NewIssueTitleDelete => ControlFlow::Break(Self::NewIssueTitleDelete),
            AppEvent::NewIssueTitleCursorLeft => ControlFlow::Break(Self::NewIssueTitleCursorLeft),
            AppEvent::NewIssueTitleCursorRight => {
                ControlFlow::Break(Self::NewIssueTitleCursorRight)
            }
            AppEvent::NewIssueTitleCursorHome => ControlFlow::Break(Self::NewIssueTitleCursorHome),
            AppEvent::NewIssueTitleCursorEnd => ControlFlow::Break(Self::NewIssueTitleCursorEnd),
            AppEvent::NewIssueBodyChar(c) => ControlFlow::Break(Self::NewIssueBodyChar(c)),
            AppEvent::NewIssueBodyNewline => ControlFlow::Break(Self::NewIssueBodyNewline),
            AppEvent::NewIssueBodyBackspace => ControlFlow::Break(Self::NewIssueBodyBackspace),
            AppEvent::NewIssueBodyDelete => ControlFlow::Break(Self::NewIssueBodyDelete),
            AppEvent::NewIssueBodyCursorLeft => ControlFlow::Break(Self::NewIssueBodyCursorLeft),
            AppEvent::NewIssueBodyCursorRight => ControlFlow::Break(Self::NewIssueBodyCursorRight),
            AppEvent::NewIssueBodyCursorUp => ControlFlow::Break(Self::NewIssueBodyCursorUp),
            AppEvent::NewIssueBodyCursorDown => ControlFlow::Break(Self::NewIssueBodyCursorDown),
            AppEvent::NewIssueBodyCursorHome => ControlFlow::Break(Self::NewIssueBodyCursorHome),
            AppEvent::NewIssueBodyCursorEnd => ControlFlow::Break(Self::NewIssueBodyCursorEnd),
            AppEvent::NewIssueFocusNext => ControlFlow::Break(Self::NewIssueFocusNext),
            AppEvent::NewIssueFocusPrev => ControlFlow::Break(Self::NewIssueFocusPrev),
            AppEvent::NewIssueSubmit => ControlFlow::Break(Self::NewIssueSubmit),
            AppEvent::NewIssueCancel => ControlFlow::Break(Self::NewIssueCancel),
            other => Self::from_app_event_new_issue_results(other),
        }
    }

    fn from_app_event_new_issue_results(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::NewIssueOptionsLoaded {
                labels,
                milestones,
                types,
                assignees,
            } => ControlFlow::Break(Self::NewIssueOptionsLoaded {
                labels,
                milestones,
                types,
                assignees,
            }),
            AppEvent::NewIssueOptionsFailed { error } => {
                ControlFlow::Break(Self::NewIssueOptionsFailed { error })
            }
            AppEvent::NewIssueCreated {
                scope_repo_id,
                mutation_id,
                issue,
            } => ControlFlow::Break(Self::NewIssueCreated {
                scope_repo_id,
                mutation_id,
                issue,
            }),
            AppEvent::NewIssueCreateFailed {
                scope_repo_id,
                mutation_id,
                issue_number,
                error,
            } => ControlFlow::Break(Self::NewIssueCreateFailed {
                scope_repo_id,
                mutation_id,
                issue_number,
                error,
            }),
            other => Self::from_app_event_property_guard(other),
        }
    }

    /// Property-editor events; the guard keeps non-property events away from
    /// the property converter.
    fn from_app_event_property_guard(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            property if is_issue_property_app_event(&property) => {
                ControlFlow::Break(Self::from_app_event_property(property))
            }
            other => Self::from_app_event_mutation_and_agent(other),
        }
    }

    /// Mutation-lifecycle and agent-chooser events.
    fn from_app_event_mutation_and_agent(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::OpenAgentChooser { metadata } => {
                ControlFlow::Break(Self::OpenAgentChooser { metadata })
            }
            AppEvent::BeginIssueListSendDetail(metadata) => {
                ControlFlow::Break(Self::BeginListSendDetail { metadata })
            }
            AppEvent::CancelIssueListSendDetail => ControlFlow::Break(Self::CancelListSendDetail),
            AppEvent::IssueListSendDetailReady {
                scope_repo_id,
                issue_number,
                request_id,
            } => ControlFlow::Break(Self::ListSendDetailReady {
                scope_repo_id,
                issue_number,
                request_id,
            }),
            AppEvent::AgentChooserNavigateUp => ControlFlow::Break(Self::AgentChooserNavigateUp),
            AppEvent::AgentChooserNavigateDown => {
                ControlFlow::Break(Self::AgentChooserNavigateDown)
            }
            AppEvent::AgentChooserConfirm => ControlFlow::Break(Self::AgentChooserConfirm),
            AppEvent::AgentChooserCancel => ControlFlow::Break(Self::AgentChooserCancel),
            AppEvent::SendToAgentCompleted => ControlFlow::Break(Self::SendToAgentCompleted),
            AppEvent::SendToAgentFailed { error } => {
                ControlFlow::Break(Self::SendToAgentFailed { error })
            }
            other => Self::from_app_event_mutation_or_close(other),
        }
    }

    /// Convert this issues-domain message back into the historical [`AppEvent`].
    ///
    /// Mirrors the from-direction: layers peel through focused converters and
    /// the terminal reports any residual as a captured converter-drift error.
    fn into_app_event(self) -> AppEvent {
        self.into_app_event_navigation()
    }

    /// Navigation and scroll messages that carry no payload.
    fn into_app_event_navigation(self) -> AppEvent {
        match self {
            Self::EnterMode => AppEvent::EnterIssuesMode,
            Self::ExitMode => AppEvent::ExitIssuesMode,
            Self::RefocusList => AppEvent::RefocusIssueList,
            Self::NavigateUp => AppEvent::IssuesNavigateUp,
            Self::NavigateDown => AppEvent::IssuesNavigateDown,
            Self::NavigatePageUp(page) => AppEvent::IssuesNavigatePageUp(page),
            Self::NavigatePageDown(page) => AppEvent::IssuesNavigatePageDown(page),
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
            other => other.into_app_event_list(),
        }
    }

    /// List loaded/error payload messages (silent refresh claimed first).
    fn into_app_event_list(self) -> AppEvent {
        if let Some(event) = self.silent_refresh_to_app_event() {
            return event;
        }
        match self {
            Self::ListLoaded {
                scope_repo_id,
                filter,
                request_id,
                issues,
                cursor,
                has_more,
            } => AppEvent::IssueListLoaded {
                scope_repo_id,
                filter,
                request_id,
                issues,
                cursor,
                has_more,
            },
            Self::ListLoadFailed {
                scope_repo_id,
                filter,
                request_id,
                request_cursor,
                error,
            } => AppEvent::IssueListLoadFailed {
                scope_repo_id,
                filter,
                request_id,
                request_cursor,
                error,
            },
            Self::ListPageLoaded {
                scope_repo_id,
                filter,
                request_id,
                request_cursor,
                issues,
                cursor,
                has_more,
            } => AppEvent::IssueListPageLoaded {
                scope_repo_id,
                filter,
                request_id,
                request_cursor,
                issues,
                cursor,
                has_more,
            },
            other => other.into_app_event_detail(),
        }
    }

    /// Detail loaded/error payload messages (including silent refresh).
    fn into_app_event_detail(self) -> AppEvent {
        match self {
            Self::DetailLoaded {
                scope_repo_id,
                issue_number,
                request_id,
                detail,
            } => AppEvent::IssueDetailLoaded {
                scope_repo_id,
                issue_number,
                request_id,
                detail,
            },
            Self::DetailLoadFailed {
                scope_repo_id,
                issue_number,
                request_id,
                error,
            } => AppEvent::IssueDetailLoadFailed {
                scope_repo_id,
                issue_number,
                request_id,
                error,
            },
            Self::DetailAuthRequired {
                scope_repo_id,
                issue_number,
                request_id,
            } => AppEvent::IssueDetailAuthRequired(scope_repo_id, issue_number, request_id),
            Self::DetailSilentRefreshed {
                scope_repo_id,
                issue_number,
                request_id,
                detail,
            } => AppEvent::IssueDetailSilentRefreshed {
                scope_repo_id,
                issue_number,
                request_id,
                detail,
            },
            Self::DetailSilentRefreshFailed {
                scope_repo_id,
                issue_number,
                request_id,
            } => AppEvent::IssueDetailSilentRefreshFailed {
                scope_repo_id,
                issue_number,
                request_id,
            },
            other => other.into_app_event_comments_and_controls(),
        }
    }

    /// Comments payloads, then the control layers.
    fn into_app_event_comments_and_controls(self) -> AppEvent {
        match self {
            Self::CommentsPageLoaded {
                scope_repo_id,
                issue_number,
                request_id,
                request_cursor,
                comments,
                cursor,
                has_more,
            } => AppEvent::IssueCommentsPageLoaded {
                scope_repo_id,
                issue_number,
                request_id,
                request_cursor,
                comments,
                cursor,
                has_more,
            },
            Self::CommentsPageFailed {
                scope_repo_id,
                issue_number,
                request_id,
                request_cursor,
                error,
            } => AppEvent::IssueCommentsPageFailed {
                scope_repo_id,
                issue_number,
                request_id,
                request_cursor,
                error,
            },
            other => other.into_app_event_simple_controls(),
        }
    }

    /// Filter and search control messages.
    fn into_app_event_simple_controls(self) -> AppEvent {
        match self {
            Self::OpenFilterControls => AppEvent::OpenFilterControls,
            Self::CloseFilterControls => AppEvent::CloseFilterControls,
            Self::ApplyFilter => AppEvent::ApplyFilter,
            Self::ClearFilter => AppEvent::ClearFilter,
            Self::ClearDraftFilter => AppEvent::ClearDraftFilter,
            Self::FilterNavigateNext => AppEvent::FilterNavigateNext,
            Self::FilterNavigatePrev => AppEvent::FilterNavigatePrev,
            Self::CycleFilterState => AppEvent::CycleFilterState,
            Self::CycleIssueSortByNext => AppEvent::CycleIssueSortByNext,
            Self::CycleIssueSortByPrev => AppEvent::CycleIssueSortByPrev,
            Self::ToggleIssueSortOrder => AppEvent::ToggleIssueSortOrder,
            Self::FocusSearchInput => AppEvent::FocusSearchInput,
            Self::BlurSearchInput => AppEvent::BlurSearchInput,
            Self::SetSearchQuery { query } => AppEvent::SetSearchQuery { query },
            Self::ApplySearch => AppEvent::ApplySearch,
            Self::ClearSearch => AppEvent::ClearSearch,
            Self::UpdateDraftFilter { field, value } => {
                AppEvent::UpdateDraftFilter { field, value }
            }
            other => other.into_app_event_composer_and_inline(),
        }
    }

    /// Composer-open and inline-editor messages.
    fn into_app_event_composer_and_inline(self) -> AppEvent {
        match self {
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
            Self::InlineCursorHome => AppEvent::InlineCursorHome,
            Self::InlineCursorEnd => AppEvent::InlineCursorEnd,
            Self::InlineSubmit => AppEvent::InlineSubmit,
            Self::InlineCancelOrEsc => AppEvent::InlineCancelOrEsc,
            Self::RequestIssueRewrite => AppEvent::RequestIssueRewrite,
            Self::IssueRewriteSucceeded { text } => AppEvent::IssueRewriteSucceeded { text },
            Self::IssueRewriteFailed { error } => AppEvent::IssueRewriteFailed { error },
            other => other.into_app_event_new_issue_form(),
        }
    }

    /// New Issue form messages.
    fn into_app_event_new_issue_form(self) -> AppEvent {
        match self {
            Self::NewIssueTemplateNext => AppEvent::NewIssueTemplateNext,
            Self::NewIssueTypeNext => AppEvent::NewIssueTypeNext,
            Self::NewIssueTitleChar(c) => AppEvent::NewIssueTitleChar(c),
            Self::NewIssueTitleBackspace => AppEvent::NewIssueTitleBackspace,
            Self::NewIssueTitleDelete => AppEvent::NewIssueTitleDelete,
            Self::NewIssueTitleCursorLeft => AppEvent::NewIssueTitleCursorLeft,
            Self::NewIssueTitleCursorRight => AppEvent::NewIssueTitleCursorRight,
            Self::NewIssueTitleCursorHome => AppEvent::NewIssueTitleCursorHome,
            Self::NewIssueTitleCursorEnd => AppEvent::NewIssueTitleCursorEnd,
            Self::NewIssueBodyChar(c) => AppEvent::NewIssueBodyChar(c),
            Self::NewIssueBodyNewline => AppEvent::NewIssueBodyNewline,
            Self::NewIssueBodyBackspace => AppEvent::NewIssueBodyBackspace,
            Self::NewIssueBodyDelete => AppEvent::NewIssueBodyDelete,
            Self::NewIssueBodyCursorLeft => AppEvent::NewIssueBodyCursorLeft,
            Self::NewIssueBodyCursorRight => AppEvent::NewIssueBodyCursorRight,
            Self::NewIssueBodyCursorUp => AppEvent::NewIssueBodyCursorUp,
            Self::NewIssueBodyCursorDown => AppEvent::NewIssueBodyCursorDown,
            Self::NewIssueBodyCursorHome => AppEvent::NewIssueBodyCursorHome,
            Self::NewIssueBodyCursorEnd => AppEvent::NewIssueBodyCursorEnd,
            Self::NewIssueFocusNext => AppEvent::NewIssueFocusNext,
            Self::NewIssueFocusPrev => AppEvent::NewIssueFocusPrev,
            Self::NewIssueSubmit => AppEvent::NewIssueSubmit,
            Self::NewIssueCancel => AppEvent::NewIssueCancel,
            Self::NewIssueOptionsLoaded {
                labels,
                milestones,
                types,
                assignees,
            } => AppEvent::NewIssueOptionsLoaded {
                labels,
                milestones,
                types,
                assignees,
            },
            Self::NewIssueOptionsFailed { error } => AppEvent::NewIssueOptionsFailed { error },
            Self::NewIssueCreated {
                scope_repo_id,
                mutation_id,
                issue,
            } => AppEvent::NewIssueCreated {
                scope_repo_id,
                mutation_id,
                issue,
            },
            Self::NewIssueCreateFailed {
                scope_repo_id,
                mutation_id,
                issue_number,
                error,
            } => AppEvent::NewIssueCreateFailed {
                scope_repo_id,
                mutation_id,
                issue_number,
                error,
            },
            other => other.into_app_event_property_guard(),
        }
    }

    /// Property-editor messages; the guard keeps non-property messages away
    /// from the property converter.
    fn into_app_event_property_guard(self) -> AppEvent {
        match self {
            property if super::names::is_issue_property_msg(&property) => {
                property.into_app_event_property()
            }
            other => other.into_app_event_mutation_and_agent(),
        }
    }

    /// Mutation-lifecycle and agent-chooser messages.
    fn into_app_event_mutation_and_agent(self) -> AppEvent {
        match self {
            Self::OpenAgentChooser { metadata } => AppEvent::OpenAgentChooser { metadata },
            Self::BeginListSendDetail { metadata } => AppEvent::BeginIssueListSendDetail(metadata),
            Self::CancelListSendDetail => AppEvent::CancelIssueListSendDetail,
            Self::ListSendDetailReady {
                scope_repo_id,
                issue_number,
                request_id,
            } => AppEvent::IssueListSendDetailReady {
                scope_repo_id,
                issue_number,
                request_id,
            },
            Self::AgentChooserNavigateUp => AppEvent::AgentChooserNavigateUp,
            Self::AgentChooserNavigateDown => AppEvent::AgentChooserNavigateDown,
            Self::AgentChooserConfirm => AppEvent::AgentChooserConfirm,
            Self::AgentChooserCancel => AppEvent::AgentChooserCancel,
            Self::SendToAgentCompleted => AppEvent::SendToAgentCompleted,
            Self::SendToAgentFailed { error } => AppEvent::SendToAgentFailed { error },
            other => other.into_app_event_mutation_or_close(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::ControlFlow;

    use super::*;

    #[test]
    fn non_issues_events_continue_to_next_domain() {
        assert!(matches!(
            IssuesMessage::try_from_app_event(AppEvent::Quit),
            ControlFlow::Continue(AppEvent::Quit)
        ));
        assert!(matches!(
            IssuesMessage::try_from_app_event(AppEvent::IssueCommentsPageLoaded {
                scope_repo_id: crate::domain::RepositoryId::default(),
                issue_number: 1,
                request_id: 1,
                request_cursor: None,
                comments: Vec::new(),
                cursor: None,
                has_more: false,
            }),
            ControlFlow::Break(IssuesMessage::CommentsPageLoaded { .. })
        ));
    }

    #[test]
    fn close_family_events_round_trip() {
        let event = AppEvent::CloseReasonDuplicateSearchChar('x');
        let ControlFlow::Break(message) = IssuesMessage::try_from_app_event(event) else {
            panic!("close-reason event should be claimed by the issues chain");
        };
        assert!(matches!(
            message,
            IssuesMessage::CloseReasonDuplicateSearchChar('x')
        ));
        assert!(matches!(
            AppEvent::from(message),
            AppEvent::CloseReasonDuplicateSearchChar('x')
        ));
    }
}
