use crate::state::AppEvent;

use super::IssuesMessage;

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
    pub(super) fn from_app_event(event: AppEvent) -> Self {
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
            _ => unreachable!("routed IssuesMessage variant reached controls converter"),
        }
    }
}
