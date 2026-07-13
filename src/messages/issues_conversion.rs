use crate::state::AppEvent;

use super::IssuesMessage;
use super::names::{is_issue_property_app_event, is_issue_property_msg};

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
            AppEvent::IssueListLoaded { .. }
            | AppEvent::IssueListLoadFailed { .. }
            | AppEvent::IssueListPageLoaded { .. }
            | AppEvent::IssueListSilentRefreshed { .. }
            | AppEvent::IssueListSilentRefreshFailed { .. } => Self::from_app_event_list(event),
            AppEvent::IssueDetailLoaded { .. }
            | AppEvent::IssueDetailLoadFailed { .. }
            | AppEvent::IssueDetailSilentRefreshed { .. }
            | AppEvent::IssueDetailSilentRefreshFailed { .. } => Self::from_app_event_detail(event),
            other => Self::from_app_event_comments_and_controls(other),
        }
    }

    /// List loaded/error payload events.
    fn from_app_event_list(event: AppEvent) -> Self {
        if let Some(msg) = Self::from_app_event_silent_refresh(&event) {
            return msg;
        }
        match event {
            AppEvent::IssueListLoaded {
                scope_repo_id,
                filter,
                request_id,
                issues,
                cursor,
                has_more,
            } => Self::ListLoaded {
                scope_repo_id,
                filter,
                request_id,
                issues,
                cursor,
                has_more,
            },
            AppEvent::IssueListLoadFailed {
                scope_repo_id,
                filter,
                request_id,
                request_cursor,
                error,
            } => Self::ListLoadFailed {
                scope_repo_id,
                filter,
                request_id,
                request_cursor,
                error,
            },
            AppEvent::IssueListPageLoaded {
                scope_repo_id,
                filter,
                request_id,
                request_cursor,
                issues,
                cursor,
                has_more,
            } => Self::ListPageLoaded {
                scope_repo_id,
                filter,
                request_id,
                request_cursor,
                issues,
                cursor,
                has_more,
            },
            _ => unreachable!("non-list AppEvent routed to list converter"),
        }
    }

    /// Silent-refresh list events (issue #175).
    /// Detail loaded/error payload events (including silent refresh, issue #175).
    fn from_app_event_detail(event: AppEvent) -> Self {
        match event {
            AppEvent::IssueDetailLoaded {
                scope_repo_id,
                issue_number,
                request_id,
                detail,
            } => Self::DetailLoaded {
                scope_repo_id,
                issue_number,
                request_id,
                detail,
            },
            AppEvent::IssueDetailLoadFailed {
                scope_repo_id,
                issue_number,
                request_id,
                error,
            } => Self::DetailLoadFailed {
                scope_repo_id,
                issue_number,
                request_id,
                error,
            },
            AppEvent::IssueDetailSilentRefreshed {
                scope_repo_id,
                issue_number,
                request_id,
                detail,
            } => Self::DetailSilentRefreshed {
                scope_repo_id,
                issue_number,
                request_id,
                detail,
            },
            AppEvent::IssueDetailSilentRefreshFailed {
                scope_repo_id,
                issue_number,
                request_id,
            } => Self::DetailSilentRefreshFailed {
                scope_repo_id,
                issue_number,
                request_id,
            },
            _ => unreachable!("non-detail AppEvent routed to detail converter"),
        }
    }

    /// Comments payloads, then controls.
    fn from_app_event_comments_and_controls(event: AppEvent) -> Self {
        match event {
            AppEvent::IssueCommentsPageLoaded {
                scope_repo_id,
                issue_number,
                request_id,
                request_cursor,
                comments,
                cursor,
                has_more,
            } => Self::CommentsPageLoaded {
                scope_repo_id,
                issue_number,
                request_id,
                request_cursor,
                comments,
                cursor,
                has_more,
            },
            AppEvent::IssueCommentsPageFailed {
                scope_repo_id,
                issue_number,
                request_id,
                request_cursor,
                error,
            } => Self::CommentsPageFailed {
                scope_repo_id,
                issue_number,
                request_id,
                request_cursor,
                error,
            },
            other => Self::from_app_event_controls(other),
        }
    }

    fn from_app_event_controls(event: AppEvent) -> Self {
        match event {
            AppEvent::OpenFilterControls
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
            | AppEvent::InlineCancelOrEsc => Self::from_app_event_simple_controls(event),
            property if is_issue_property_app_event(&property) => {
                Self::from_app_event_property(property)
            }
            other => Self::from_app_event_mutation_and_agent(other),
        }
    }

    fn from_app_event_simple_controls(event: AppEvent) -> Self {
        match event {
            AppEvent::OpenFilterControls => Self::OpenFilterControls,
            AppEvent::CloseFilterControls => Self::CloseFilterControls,
            AppEvent::ApplyFilter => Self::ApplyFilter,
            AppEvent::ClearFilter => Self::ClearFilter,
            AppEvent::ClearDraftFilter => Self::ClearDraftFilter,
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
            other => Self::from_app_event_composer_and_inline(other),
        }
    }

    /// Composer-open and inline-editor events; delegates mutation/agent and
    /// further events to `from_app_event_mutation_and_agent`.
    fn from_app_event_composer_and_inline(event: AppEvent) -> Self {
        match event {
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
            other => Self::from_app_event_mutation_and_agent(other),
        }
    }

    /// Mutation-lifecycle and agent-chooser events; delegates close-family
    /// events to `from_app_event_close_family`.
    fn from_app_event_mutation_and_agent(event: AppEvent) -> Self {
        match event {
            AppEvent::MutationSubmitted { .. }
            | AppEvent::IssueCreated { .. }
            | AppEvent::CommentCreated { .. }
            | AppEvent::CommentCreateFailed { .. }
            | AppEvent::IssueBodyUpdated { .. }
            | AppEvent::CommentUpdated { .. }
            | AppEvent::MutationFailed { .. } => Self::from_app_event_mutation(event),
            AppEvent::OpenAgentChooser => Self::OpenAgentChooser,
            AppEvent::AgentChooserNavigateUp => Self::AgentChooserNavigateUp,
            AppEvent::AgentChooserNavigateDown => Self::AgentChooserNavigateDown,
            AppEvent::AgentChooserConfirm => Self::AgentChooserConfirm,
            AppEvent::AgentChooserCancel => Self::AgentChooserCancel,
            AppEvent::SendToAgentCompleted => Self::SendToAgentCompleted,
            AppEvent::SendToAgentFailed { error } => Self::SendToAgentFailed { error },
            other => Self::from_app_event_close_family(other),
        }
    }

    /// Close/delete lifecycle, close-reason chooser, and self-assignment events.
    fn from_app_event_close_family(event: AppEvent) -> Self {
        match event {
            AppEvent::CloseIssue
            | AppEvent::OpenDeleteIssueConfirm
            | AppEvent::IssueDeleteConfirm
            | AppEvent::IssueDeleteCancel
            | AppEvent::IssueClosed { .. }
            | AppEvent::IssueDeleted { .. } => Self::from_app_event_lifecycle(event),
            AppEvent::OpenCloseReasonChooser
            | AppEvent::CloseReasonNavigateUp
            | AppEvent::CloseReasonNavigateDown
            | AppEvent::CloseReasonSelect
            | AppEvent::CloseReasonDuplicateSearchChar(_)
            | AppEvent::CloseReasonDuplicateSearchBackspace
            | AppEvent::CloseReasonDuplicateSearchNavigateUp
            | AppEvent::CloseReasonDuplicateSearchNavigateDown
            | AppEvent::CloseReasonConfirm
            | AppEvent::CloseReasonCancel => Self::from_app_event_close_reason(event),
            AppEvent::IssueSelfAssignmentFailed { .. } => {
                Self::from_app_event_self_assignment(event)
            }
            _ => unreachable!("non-issues AppEvent routed to issues converter"),
        }
    }

    /// Close/delete lifecycle events (issue #182) — extracted from
    /// `from_app_event_controls` to stay within the per-function line budget.
    fn from_app_event_lifecycle(event: AppEvent) -> Self {
        match event {
            AppEvent::CloseIssue => Self::CloseIssue,
            AppEvent::OpenDeleteIssueConfirm => Self::OpenDeleteIssueConfirm,
            AppEvent::IssueDeleteConfirm => Self::IssueDeleteConfirm,
            AppEvent::IssueDeleteCancel => Self::IssueDeleteCancel,
            AppEvent::IssueClosed {
                scope_repo_id,
                issue_number,
                mutation_id,
                close_reason,
                duplicate_of,
            } => Self::IssueClosed {
                scope_repo_id,
                issue_number,
                mutation_id,
                close_reason,
                duplicate_of,
            },
            AppEvent::IssueDeleted {
                scope_repo_id,
                issue_number,
                mutation_id,
            } => Self::IssueDeleted {
                scope_repo_id,
                issue_number,
                mutation_id,
            },
            _ => unreachable!("non-lifecycle AppEvent routed to lifecycle converter"),
        }
    }

    /// Close-reason chooser events (issue #188) — extracted from
    /// `from_app_event_lifecycle` to stay within the per-function line budget.
    fn from_app_event_close_reason(event: AppEvent) -> Self {
        match event {
            AppEvent::OpenCloseReasonChooser => Self::OpenCloseReasonChooser,
            AppEvent::CloseReasonNavigateUp => Self::CloseReasonNavigateUp,
            AppEvent::CloseReasonNavigateDown => Self::CloseReasonNavigateDown,
            AppEvent::CloseReasonSelect => Self::CloseReasonSelect,
            AppEvent::CloseReasonDuplicateSearchChar(c) => Self::CloseReasonDuplicateSearchChar(c),
            AppEvent::CloseReasonDuplicateSearchBackspace => {
                Self::CloseReasonDuplicateSearchBackspace
            }
            AppEvent::CloseReasonDuplicateSearchNavigateUp => {
                Self::CloseReasonDuplicateSearchNavigateUp
            }
            AppEvent::CloseReasonDuplicateSearchNavigateDown => {
                Self::CloseReasonDuplicateSearchNavigateDown
            }
            AppEvent::CloseReasonConfirm => Self::CloseReasonConfirm,
            AppEvent::CloseReasonCancel => Self::CloseReasonCancel,
            _ => unreachable!("non-close-reason AppEvent routed to close-reason converter"),
        }
    }

    /// Self-assignment follow-up event (issue #186) — extracted from
    /// `from_app_event_controls` to stay within the per-function line budget.
    fn from_app_event_self_assignment(event: AppEvent) -> Self {
        match event {
            AppEvent::IssueSelfAssignmentFailed {
                owner_repo,
                issue_number,
                error,
            } => Self::IssueSelfAssignmentFailed {
                owner_repo,
                issue_number,
                error,
            },
            _ => unreachable!("non-self-assignment AppEvent routed to self-assignment converter"),
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
            | Self::ListSilentRefreshed { .. }
            | Self::ListSilentRefreshFailed { .. } => self.into_app_event_list(),
            Self::DetailLoaded { .. }
            | Self::DetailLoadFailed { .. }
            | Self::DetailSilentRefreshed { .. }
            | Self::DetailSilentRefreshFailed { .. } => self.into_app_event_detail(),
            other => other.into_app_event_comments_and_controls(),
        }
    }

    /// List loaded/error payload messages.
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
            _ => unreachable!("non-list IssuesMessage routed to list converter"),
        }
    }

    /// Convert silent-refresh issue messages back into `AppEvent` (issue #175).
    /// Detail loaded/error payload messages (including silent refresh, issue #175).
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
            _ => unreachable!("non-detail IssuesMessage routed to detail converter"),
        }
    }

    /// Comments payloads, then controls; further delegates to controls helper.
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
            other => other.into_app_event_controls(),
        }
    }

    fn into_app_event_controls(self) -> AppEvent {
        match self {
            Self::OpenFilterControls
            | Self::CloseFilterControls
            | Self::ApplyFilter
            | Self::ClearFilter
            | Self::ClearDraftFilter
            | Self::FilterNavigateNext
            | Self::FilterNavigatePrev
            | Self::CycleFilterState
            | Self::FocusSearchInput
            | Self::BlurSearchInput
            | Self::SetSearchQuery { .. }
            | Self::ApplySearch
            | Self::ClearSearch
            | Self::UpdateDraftFilter { .. }
            | Self::OpenNewIssueComposer
            | Self::OpenNewCommentComposer
            | Self::OpenReplyComposer { .. }
            | Self::OpenInlineEditor { .. }
            | Self::InlineChar(_)
            | Self::InlineNewline
            | Self::InlineBackspace
            | Self::InlineDelete
            | Self::InlineCursorLeft
            | Self::InlineCursorRight
            | Self::InlineCursorUp
            | Self::InlineCursorDown
            | Self::InlineSubmit
            | Self::InlineCancelOrEsc => self.into_app_event_simple_controls(),
            property if is_issue_property_msg(&property) => property.into_app_event_property(),
            other => other.into_app_event_mutation_and_agent(),
        }
    }

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

    /// Composer-open and inline-editor messages; delegates mutation/agent and
    /// further messages to `into_app_event_mutation_and_agent`.
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
            Self::InlineSubmit => AppEvent::InlineSubmit,
            Self::InlineCancelOrEsc => AppEvent::InlineCancelOrEsc,
            other => other.into_app_event_mutation_and_agent(),
        }
    }

    /// Mutation-lifecycle and agent-chooser messages; delegates close-family
    /// messages to `into_app_event_close_family`.
    fn into_app_event_mutation_and_agent(self) -> AppEvent {
        match self {
            Self::MutationSubmitted { .. }
            | Self::IssueCreated { .. }
            | Self::CommentCreated { .. }
            | Self::CommentCreateFailed { .. }
            | Self::IssueBodyUpdated { .. }
            | Self::CommentUpdated { .. }
            | Self::MutationFailed { .. } => self.into_app_event_mutation(),
            Self::OpenAgentChooser => AppEvent::OpenAgentChooser,
            Self::AgentChooserNavigateUp => AppEvent::AgentChooserNavigateUp,
            Self::AgentChooserNavigateDown => AppEvent::AgentChooserNavigateDown,
            Self::AgentChooserConfirm => AppEvent::AgentChooserConfirm,
            Self::AgentChooserCancel => AppEvent::AgentChooserCancel,
            Self::SendToAgentCompleted => AppEvent::SendToAgentCompleted,
            Self::SendToAgentFailed { error } => AppEvent::SendToAgentFailed { error },
            other => other.into_app_event_close_family(),
        }
    }

    /// Close/delete lifecycle, close-reason chooser, and self-assignment messages.
    fn into_app_event_close_family(self) -> AppEvent {
        match self {
            Self::CloseIssue
            | Self::OpenDeleteIssueConfirm
            | Self::IssueDeleteConfirm
            | Self::IssueDeleteCancel
            | Self::IssueClosed { .. }
            | Self::IssueDeleted { .. } => self.into_app_event_lifecycle(),
            Self::OpenCloseReasonChooser
            | Self::CloseReasonNavigateUp
            | Self::CloseReasonNavigateDown
            | Self::CloseReasonSelect
            | Self::CloseReasonDuplicateSearchChar(_)
            | Self::CloseReasonDuplicateSearchBackspace
            | Self::CloseReasonDuplicateSearchNavigateUp
            | Self::CloseReasonDuplicateSearchNavigateDown
            | Self::CloseReasonConfirm
            | Self::CloseReasonCancel => self.into_app_event_close_reason(),
            Self::IssueSelfAssignmentFailed { .. } => self.into_app_event_self_assignment(),
            _ => unreachable!("non-issues IssuesMessage routed to issues converter"),
        }
    }

    /// Close/delete lifecycle messages (issue #182) — extracted from
    /// `into_app_event_controls` to stay within the per-function line budget.
    fn into_app_event_lifecycle(self) -> AppEvent {
        match self {
            Self::CloseIssue => AppEvent::CloseIssue,
            Self::OpenDeleteIssueConfirm => AppEvent::OpenDeleteIssueConfirm,
            Self::IssueDeleteConfirm => AppEvent::IssueDeleteConfirm,
            Self::IssueDeleteCancel => AppEvent::IssueDeleteCancel,
            Self::IssueClosed {
                scope_repo_id,
                issue_number,
                mutation_id,
                close_reason,
                duplicate_of,
            } => AppEvent::IssueClosed {
                scope_repo_id,
                issue_number,
                mutation_id,
                close_reason,
                duplicate_of,
            },
            Self::IssueDeleted {
                scope_repo_id,
                issue_number,
                mutation_id,
            } => AppEvent::IssueDeleted {
                scope_repo_id,
                issue_number,
                mutation_id,
            },
            _ => unreachable!("non-lifecycle IssuesMessage routed to lifecycle converter"),
        }
    }

    /// Close-reason chooser messages (issue #188) — extracted from
    /// `into_app_event_lifecycle` to stay within the per-function line budget.
    fn into_app_event_close_reason(self) -> AppEvent {
        match self {
            Self::OpenCloseReasonChooser => AppEvent::OpenCloseReasonChooser,
            Self::CloseReasonNavigateUp => AppEvent::CloseReasonNavigateUp,
            Self::CloseReasonNavigateDown => AppEvent::CloseReasonNavigateDown,
            Self::CloseReasonSelect => AppEvent::CloseReasonSelect,
            Self::CloseReasonDuplicateSearchChar(c) => AppEvent::CloseReasonDuplicateSearchChar(c),
            Self::CloseReasonDuplicateSearchBackspace => {
                AppEvent::CloseReasonDuplicateSearchBackspace
            }
            Self::CloseReasonDuplicateSearchNavigateUp => {
                AppEvent::CloseReasonDuplicateSearchNavigateUp
            }
            Self::CloseReasonDuplicateSearchNavigateDown => {
                AppEvent::CloseReasonDuplicateSearchNavigateDown
            }
            Self::CloseReasonConfirm => AppEvent::CloseReasonConfirm,
            Self::CloseReasonCancel => AppEvent::CloseReasonCancel,
            _ => unreachable!("non-close-reason IssuesMessage routed to close-reason converter"),
        }
    }

    /// Self-assignment follow-up message (issue #186) — extracted from
    /// `into_app_event_controls` to stay within the per-function line budget.
    fn into_app_event_self_assignment(self) -> AppEvent {
        match self {
            Self::IssueSelfAssignmentFailed {
                owner_repo,
                issue_number,
                error,
            } => AppEvent::IssueSelfAssignmentFailed {
                owner_repo,
                issue_number,
                error,
            },
            _ => unreachable!(
                "non-self-assignment IssuesMessage routed to self-assignment converter"
            ),
        }
    }
}
