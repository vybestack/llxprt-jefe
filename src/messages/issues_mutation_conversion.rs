//! Mutation message conversion for Issues mode.
//!
//! Extracted from `issues_conversion.rs` to keep that file under the
//! source-file-size hard limit. Instead of `unreachable!` on classifier
//! drift, these converters fall through to the next dispatch layer so the
//! TUI never panics on routing mistakes.

use crate::state::AppEvent;

use super::IssuesMessage;

impl IssuesMessage {
    /// Convert mutation AppEvents, or fall through to the close-family
    /// dispatcher for non-mutation events.
    pub(super) fn from_app_event_mutation_or_close(event: AppEvent) -> Self {
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
            other => Self::from_app_event_mutation_finish_or_close(other),
        }
    }

    fn from_app_event_mutation_finish_or_close(event: AppEvent) -> Self {
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
            other => Self::from_app_event_close_family(other),
        }
    }

    /// Convert mutation IssuesMessages, or fall through to the close-family
    /// dispatcher for non-mutation messages.
    pub(super) fn into_app_event_mutation_or_close(self) -> AppEvent {
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
            other => other.into_app_event_mutation_finish_or_close(),
        }
    }

    fn into_app_event_mutation_finish_or_close(self) -> AppEvent {
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
            other => other.into_app_event_close_family(),
        }
    }
}
