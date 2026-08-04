use crate::state::AppEvent;

use super::IssuesMessage;

impl IssuesMessage {
    /// Close/delete lifecycle, close-reason chooser, and self-assignment messages.
    pub(super) fn into_app_event_close_family(self) -> AppEvent {
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
