//! Mutation message conversion for Issues mode.
//!
//! Extracted from `issues_conversion.rs` to keep that file under the
//! source-file-size hard limit.

use crate::state::AppEvent;

use super::IssuesMessage;

impl IssuesMessage {
    pub(super) fn from_app_event_mutation(event: AppEvent) -> Self {
        match event {
            AppEvent::MutationSubmitted { .. }
            | AppEvent::IssueCreated { .. }
            | AppEvent::CommentCreated { .. }
            | AppEvent::CommentCreateFailed { .. } => Self::from_app_event_mutation_start(event),
            AppEvent::IssueBodyUpdated { .. }
            | AppEvent::CommentUpdated { .. }
            | AppEvent::MutationFailed { .. } => Self::from_app_event_mutation_finish(event),
            _ => unreachable!("non-mutation AppEvent routed to mutation converter"),
        }
    }

    fn from_app_event_mutation_start(event: AppEvent) -> Self {
        match event {
            AppEvent::MutationSubmitted {
                scope_repo_id,
                mutation_id,
                target,
            } => Self::MutationSubmitted {
                scope_repo_id,
                mutation_id,
                target,
            },
            AppEvent::IssueCreated {
                scope_repo_id,
                mutation_id,
                issue_number,
            } => Self::IssueCreated {
                scope_repo_id,
                mutation_id,
                issue_number,
            },
            AppEvent::CommentCreated {
                scope_repo_id,
                issue_number,
                mutation_id,
                comment,
            } => Self::CommentCreated {
                scope_repo_id,
                issue_number,
                mutation_id,
                comment,
            },
            AppEvent::CommentCreateFailed {
                scope_repo_id,
                issue_number,
                mutation_id,
                error,
            } => Self::CommentCreateFailed {
                scope_repo_id,
                issue_number,
                mutation_id,
                error,
            },
            _ => unreachable!("non-start mutation AppEvent routed to start converter"),
        }
    }

    fn from_app_event_mutation_finish(event: AppEvent) -> Self {
        match event {
            AppEvent::IssueBodyUpdated {
                scope_repo_id,
                issue_number,
                mutation_id,
                title,
                body,
            } => Self::IssueBodyUpdated {
                scope_repo_id,
                issue_number,
                mutation_id,
                title,
                body,
            },
            AppEvent::CommentUpdated {
                scope_repo_id,
                issue_number,
                mutation_id,
                comment_id,
                comment_index,
                body,
            } => Self::CommentUpdated {
                scope_repo_id,
                issue_number,
                mutation_id,
                comment_id,
                comment_index,
                body,
            },
            AppEvent::MutationFailed {
                scope_repo_id,
                issue_number,
                mutation_id,
                error,
            } => Self::MutationFailed {
                scope_repo_id,
                issue_number,
                mutation_id,
                error,
            },
            _ => unreachable!("non-finish mutation AppEvent routed to finish converter"),
        }
    }

    pub(super) fn into_app_event_mutation(self) -> AppEvent {
        match self {
            Self::MutationSubmitted { .. }
            | Self::IssueCreated { .. }
            | Self::CommentCreated { .. }
            | Self::CommentCreateFailed { .. } => self.into_app_event_mutation_start(),
            Self::IssueBodyUpdated { .. }
            | Self::CommentUpdated { .. }
            | Self::MutationFailed { .. } => self.into_app_event_mutation_finish(),
            _ => unreachable!("non-mutation IssuesMessage routed to mutation converter"),
        }
    }

    fn into_app_event_mutation_start(self) -> AppEvent {
        match self {
            Self::MutationSubmitted {
                scope_repo_id,
                mutation_id,
                target,
            } => AppEvent::MutationSubmitted {
                scope_repo_id,
                mutation_id,
                target,
            },
            Self::IssueCreated {
                scope_repo_id,
                mutation_id,
                issue_number,
            } => AppEvent::IssueCreated {
                scope_repo_id,
                mutation_id,
                issue_number,
            },
            Self::CommentCreated {
                scope_repo_id,
                issue_number,
                mutation_id,
                comment,
            } => AppEvent::CommentCreated {
                scope_repo_id,
                issue_number,
                mutation_id,
                comment,
            },
            Self::CommentCreateFailed {
                scope_repo_id,
                issue_number,
                mutation_id,
                error,
            } => AppEvent::CommentCreateFailed {
                scope_repo_id,
                issue_number,
                mutation_id,
                error,
            },
            _ => unreachable!("non-start mutation IssuesMessage routed to start converter"),
        }
    }

    fn into_app_event_mutation_finish(self) -> AppEvent {
        match self {
            Self::IssueBodyUpdated {
                scope_repo_id,
                issue_number,
                mutation_id,
                title,
                body,
            } => AppEvent::IssueBodyUpdated {
                scope_repo_id,
                issue_number,
                mutation_id,
                title,
                body,
            },
            Self::CommentUpdated {
                scope_repo_id,
                issue_number,
                mutation_id,
                comment_id,
                comment_index,
                body,
            } => AppEvent::CommentUpdated {
                scope_repo_id,
                issue_number,
                mutation_id,
                comment_id,
                comment_index,
                body,
            },
            Self::MutationFailed {
                scope_repo_id,
                issue_number,
                mutation_id,
                error,
            } => AppEvent::MutationFailed {
                scope_repo_id,
                issue_number,
                mutation_id,
                error,
            },
            _ => unreachable!("non-finish mutation IssuesMessage routed to finish converter"),
        }
    }
}
