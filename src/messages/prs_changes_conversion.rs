//! Changes-view conversion sub-handlers for `PullRequestsMessage`.

use std::ops::ControlFlow;

use crate::state::AppEvent;

use super::PullRequestsMessage;

impl PullRequestsMessage {
    pub(super) fn changes_from_app_event(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        ControlFlow::Break(match event {
            AppEvent::PrOpenChanges => Self::OpenChanges,
            AppEvent::PrChangesFocusContent => Self::ChangesFocusContent,
            AppEvent::PrChangesFocusFiles => Self::ChangesFocusFiles,
            AppEvent::PrChangesToggleView => Self::ChangesToggleView,
            AppEvent::PrOpenChangesComment => Self::OpenChangesComment,
            AppEvent::PrChangesBack => Self::ChangesBack,
            AppEvent::PrChangesLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                files,
                truncated,
            } => Self::ChangesLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                files,
                truncated,
            },
            AppEvent::PrChangesLoadFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            } => Self::ChangesLoadFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            },
            AppEvent::PrChangesBlobLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                blob_sha,
                blob,
            } => Self::ChangesBlobLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                blob_sha,
                blob,
            },
            AppEvent::PrChangesBlobLoadFailed {
                scope_repo_id,
                pr_number,
                request_id,
                blob_sha,
                error,
            } => Self::ChangesBlobLoadFailed {
                scope_repo_id,
                pr_number,
                request_id,
                blob_sha,
                error,
            },
            other => return ControlFlow::Continue(other),
        })
    }

    pub(super) fn changes_into_app_event(self) -> ControlFlow<AppEvent, Self> {
        ControlFlow::Break(match self {
            Self::OpenChanges => AppEvent::PrOpenChanges,
            Self::ChangesFocusContent => AppEvent::PrChangesFocusContent,
            Self::ChangesFocusFiles => AppEvent::PrChangesFocusFiles,
            Self::ChangesToggleView => AppEvent::PrChangesToggleView,
            Self::OpenChangesComment => AppEvent::PrOpenChangesComment,
            Self::ChangesBack => AppEvent::PrChangesBack,
            Self::ChangesLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                files,
                truncated,
            } => AppEvent::PrChangesLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                files,
                truncated,
            },
            Self::ChangesLoadFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            } => AppEvent::PrChangesLoadFailed {
                scope_repo_id,
                pr_number,
                request_id,
                error,
            },
            Self::ChangesBlobLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                blob_sha,
                blob,
            } => AppEvent::PrChangesBlobLoaded {
                scope_repo_id,
                pr_number,
                request_id,
                blob_sha,
                blob,
            },
            Self::ChangesBlobLoadFailed {
                scope_repo_id,
                pr_number,
                request_id,
                blob_sha,
                error,
            } => AppEvent::PrChangesBlobLoadFailed {
                scope_repo_id,
                pr_number,
                request_id,
                blob_sha,
                error,
            },
            other => return ControlFlow::Continue(other),
        })
    }
}
