//! Silent-refresh message conversion for Issues mode (issue #175).
//!
//! Extracted from `issues_conversion.rs` to keep that file under the
//! source-file-size hard limit. These methods are additional arms of the
//! `IssuesMessage` ↔ `AppEvent` conversion that handle the silent background
//! refresh events added for property editing.

use crate::state::AppEvent;

use super::IssuesMessage;

impl IssuesMessage {
    /// AppEvent → IssuesMessage for the list silent-refresh events.
    pub(super) fn from_app_event_silent_refresh(event: &AppEvent) -> Option<Self> {
        match event {
            AppEvent::IssueListSilentRefreshed {
                scope_repo_id,
                filter,
                request_id,
                issues,
                cursor,
                has_more,
            } => Some(Self::ListSilentRefreshed {
                scope_repo_id: scope_repo_id.clone(),
                filter: filter.clone(),
                request_id: *request_id,
                issues: issues.clone(),
                cursor: cursor.clone(),
                has_more: *has_more,
            }),
            AppEvent::IssueListSilentRefreshFailed {
                scope_repo_id,
                request_id,
            } => Some(Self::ListSilentRefreshFailed {
                scope_repo_id: scope_repo_id.clone(),
                request_id: *request_id,
            }),
            _ => None,
        }
    }

    /// IssuesMessage → AppEvent for the list silent-refresh messages.
    pub(super) fn silent_refresh_to_app_event(&self) -> Option<AppEvent> {
        match self {
            Self::ListSilentRefreshed {
                scope_repo_id,
                filter,
                request_id,
                issues,
                cursor,
                has_more,
            } => Some(AppEvent::IssueListSilentRefreshed {
                scope_repo_id: scope_repo_id.clone(),
                filter: filter.clone(),
                request_id: *request_id,
                issues: issues.clone(),
                cursor: cursor.clone(),
                has_more: *has_more,
            }),
            Self::ListSilentRefreshFailed {
                scope_repo_id,
                request_id,
            } => Some(AppEvent::IssueListSilentRefreshFailed {
                scope_repo_id: scope_repo_id.clone(),
                request_id: *request_id,
            }),
            _ => None,
        }
    }
}
