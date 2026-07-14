//! Property-editor message conversion for Issues mode (issue #175).
//!
//! Extracted from `issues_conversion.rs` to keep that file under the
//! source-file-size hard limit. These methods convert between the property
//! editor `AppEvent` variants and `IssuesMessage` variants.

use crate::state::AppEvent;

use super::IssuesMessage;

impl IssuesMessage {
    /// AppEvent → IssuesMessage for property-editor events.
    pub(super) fn from_app_event_property(event: AppEvent) -> Self {
        match event {
            AppEvent::IssuePropertyEditorOptionsLoaded { .. }
            | AppEvent::IssuePropertyEditorOptionsFailed { .. }
            | AppEvent::IssuePropertyEditSucceeded { .. }
            | AppEvent::IssuePropertyEditFailed { .. } => {
                Self::from_app_event_property_payload(event)
            }
            AppEvent::IssuePropertyEditorValidationError { kind, error } => {
                Self::PropertyEditorValidationError { kind, error }
            }
            AppEvent::IssuePostMutationRefreshStarted => Self::PostMutationRefreshStarted,
            other => Self::from_app_event_property_simple(other),
        }
    }

    /// Simple property-editor events (no payload extraction).
    fn from_app_event_property_simple(event: AppEvent) -> Self {
        match event {
            AppEvent::IssueOpenPropertyEditor { kind } => Self::OpenPropertyEditor { kind },
            AppEvent::IssuePropertyEditorNavigateUp => Self::PropertyEditorNavigateUp,
            AppEvent::IssuePropertyEditorNavigateDown => Self::PropertyEditorNavigateDown,
            AppEvent::IssuePropertyEditorToggle => Self::PropertyEditorToggle,
            AppEvent::IssuePropertyEditorConfirm => Self::PropertyEditorConfirm,
            AppEvent::IssuePropertyEditorTitleChar(c) => Self::PropertyEditorTitleChar(c),
            AppEvent::IssuePropertyEditorTitleBackspace => Self::PropertyEditorTitleBackspace,
            AppEvent::IssuePropertyEditorTitleDelete => Self::PropertyEditorTitleDelete,
            AppEvent::IssuePropertyEditorTitleCursorLeft => Self::PropertyEditorTitleCursorLeft,
            AppEvent::IssuePropertyEditorTitleCursorRight => Self::PropertyEditorTitleCursorRight,
            // Non-property events should never reach this converter; the
            // routing guard (`is_issue_property_app_event`) filters them
            // upstream. If one slips through, it is safer to no-op (close
            // the editor) than to panic or enter an unrelated mode.
            _ => Self::PropertyEditorCancel,
        }
    }

    /// Property-editor events with payloads → IssuesMessage.
    fn from_app_event_property_payload(event: AppEvent) -> Self {
        match event {
            AppEvent::IssuePropertyEditorOptionsLoaded {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
                options,
            } => Self::PropertyEditorOptionsLoaded {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
                options,
            },
            AppEvent::IssuePropertyEditorOptionsFailed {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
                error,
            } => Self::PropertyEditorOptionsFailed {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
                error,
            },
            AppEvent::IssuePropertyEditSucceeded {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
            } => Self::PropertyEditSucceeded {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
            },
            AppEvent::IssuePropertyEditFailed {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
                error,
            } => Self::PropertyEditFailed {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
                error,
            },
            _ => Self::PropertyEditorCancel,
        }
    }

    /// IssuesMessage → AppEvent for property-editor messages.
    pub(super) fn into_app_event_property(self) -> AppEvent {
        match self {
            Self::PropertyEditorOptionsLoaded { .. }
            | Self::PropertyEditorOptionsFailed { .. }
            | Self::PropertyEditSucceeded { .. }
            | Self::PropertyEditFailed { .. } => self.into_app_event_property_payload(),
            Self::PropertyEditorValidationError { kind, error } => {
                AppEvent::IssuePropertyEditorValidationError { kind, error }
            }
            Self::PostMutationRefreshStarted => AppEvent::IssuePostMutationRefreshStarted,
            other => other.into_app_event_property_simple(),
        }
    }

    /// Simple property-editor messages (no payload extraction).
    fn into_app_event_property_simple(self) -> AppEvent {
        match self {
            Self::OpenPropertyEditor { kind } => AppEvent::IssueOpenPropertyEditor { kind },
            Self::PropertyEditorNavigateUp => AppEvent::IssuePropertyEditorNavigateUp,
            Self::PropertyEditorNavigateDown => AppEvent::IssuePropertyEditorNavigateDown,
            Self::PropertyEditorToggle => AppEvent::IssuePropertyEditorToggle,
            Self::PropertyEditorConfirm => AppEvent::IssuePropertyEditorConfirm,
            Self::PropertyEditorTitleChar(c) => AppEvent::IssuePropertyEditorTitleChar(c),
            Self::PropertyEditorTitleBackspace => AppEvent::IssuePropertyEditorTitleBackspace,
            Self::PropertyEditorTitleDelete => AppEvent::IssuePropertyEditorTitleDelete,
            Self::PropertyEditorTitleCursorLeft => AppEvent::IssuePropertyEditorTitleCursorLeft,
            Self::PropertyEditorTitleCursorRight => AppEvent::IssuePropertyEditorTitleCursorRight,
            // Non-property messages should never reach this converter. If
            // one slips through, close the editor rather than entering an
            // unrelated mode.
            _ => AppEvent::IssuePropertyEditorCancel,
        }
    }

    /// Property-editor messages with payloads → AppEvent.
    fn into_app_event_property_payload(self) -> AppEvent {
        match self {
            Self::PropertyEditorOptionsLoaded {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
                options,
            } => AppEvent::IssuePropertyEditorOptionsLoaded {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
                options,
            },
            Self::PropertyEditorOptionsFailed {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
                error,
            } => AppEvent::IssuePropertyEditorOptionsFailed {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
                error,
            },
            Self::PropertyEditSucceeded {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
            } => AppEvent::IssuePropertyEditSucceeded {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
            },
            Self::PropertyEditFailed {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
                error,
            } => AppEvent::IssuePropertyEditFailed {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
                error,
            },
            _ => AppEvent::IssuePropertyEditorCancel,
        }
    }
}
