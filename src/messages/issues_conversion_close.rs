use std::ops::ControlFlow;

use crate::domain::ErrorSource;
use crate::state::AppEvent;

use super::IssuesMessage;

impl IssuesMessage {
    /// Close/delete lifecycle, close-reason chooser, and self-assignment
    /// events. Terminal of the issues from-chain: any residual returns to the
    /// dispatcher via [`ControlFlow::Continue`] instead of panicking.
    pub(super) fn from_app_event_close_family(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::CloseIssue => ControlFlow::Break(Self::CloseIssue),
            AppEvent::OpenDeleteIssueConfirm => ControlFlow::Break(Self::OpenDeleteIssueConfirm),
            AppEvent::IssueDeleteConfirm => ControlFlow::Break(Self::IssueDeleteConfirm),
            AppEvent::IssueDeleteCancel => ControlFlow::Break(Self::IssueDeleteCancel),
            AppEvent::IssueClosed {
                scope_repo_id,
                issue_number,
                mutation_id,
                close_reason,
                duplicate_of,
            } => ControlFlow::Break(Self::IssueClosed {
                scope_repo_id,
                issue_number,
                mutation_id,
                close_reason,
                duplicate_of,
            }),
            AppEvent::IssueDeleted {
                scope_repo_id,
                issue_number,
                mutation_id,
            } => ControlFlow::Break(Self::IssueDeleted {
                scope_repo_id,
                issue_number,
                mutation_id,
            }),
            AppEvent::OpenCloseReasonChooser => ControlFlow::Break(Self::OpenCloseReasonChooser),
            AppEvent::CloseReasonNavigateUp => ControlFlow::Break(Self::CloseReasonNavigateUp),
            AppEvent::CloseReasonNavigateDown => ControlFlow::Break(Self::CloseReasonNavigateDown),
            AppEvent::CloseReasonSelect => ControlFlow::Break(Self::CloseReasonSelect),
            AppEvent::CloseReasonDuplicateSearchChar(c) => {
                ControlFlow::Break(Self::CloseReasonDuplicateSearchChar(c))
            }
            AppEvent::CloseReasonDuplicateSearchBackspace => {
                ControlFlow::Break(Self::CloseReasonDuplicateSearchBackspace)
            }
            AppEvent::CloseReasonDuplicateSearchNavigateUp => {
                ControlFlow::Break(Self::CloseReasonDuplicateSearchNavigateUp)
            }
            AppEvent::CloseReasonDuplicateSearchNavigateDown => {
                ControlFlow::Break(Self::CloseReasonDuplicateSearchNavigateDown)
            }
            AppEvent::CloseReasonConfirm => ControlFlow::Break(Self::CloseReasonConfirm),
            AppEvent::CloseReasonCancel => ControlFlow::Break(Self::CloseReasonCancel),
            AppEvent::IssueSelfAssignmentFailed {
                owner_repo,
                issue_number,
                error,
            } => ControlFlow::Break(Self::IssueSelfAssignmentFailed {
                owner_repo,
                issue_number,
                error,
            }),
            other => ControlFlow::Continue(other),
        }
    }

    /// Close/delete lifecycle, close-reason chooser, and self-assignment
    /// messages. Terminal of the issues into-chain: a residual means a new
    /// variant was added without a converter arm, which is reported as a
    /// captured converter-drift error instead of panicking.
    pub(super) fn into_app_event_close_family(self) -> AppEvent {
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
            Self::IssueSelfAssignmentFailed {
                owner_repo,
                issue_number,
                error,
            } => AppEvent::IssueSelfAssignmentFailed {
                owner_repo,
                issue_number,
                error,
            },
            other => AppEvent::CaptureSilentError(
                "Unconvertible issues message".to_owned(),
                format!("{other:?} matched no issues converter"),
                ErrorSource::Panic,
                unix_timestamp(),
            ),
        }
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
