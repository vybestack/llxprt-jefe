//! Property-editor + thread conversion sub-handlers for `PullRequestsMessage`
//! (issue #175). Extracted from `prs_conversion.rs` to keep that file under
//! the source-file-size limit.
//!
//! These methods are defined in a separate `impl PullRequestsMessage` block
//! (Rust allows multiple impl blocks across files within the same crate).

use crate::state::AppEvent;

use super::PullRequestsMessage;

impl PullRequestsMessage {
    /// Property-editor events → message (issue #175).
    pub(super) fn from_app_event_property(event: AppEvent) -> Self {
        match event {
            AppEvent::PrPropertyEditorOptionsLoaded { .. }
            | AppEvent::PrPropertyEditorOptionsFailed { .. }
            | AppEvent::PrPropertyEditSucceeded { .. }
            | AppEvent::PrPropertyEditFailed { .. } => Self::from_app_event_property_payload(event),
            other => Self::from_app_event_property_simple(other),
        }
    }

    /// Simple property-editor events (no payload extraction needed).
    fn from_app_event_property_simple(event: AppEvent) -> Self {
        match event {
            AppEvent::PrOpenPropertyEditor { kind } => Self::OpenPropertyEditor { kind },
            AppEvent::PrPropertyEditorNavigateUp => Self::PropertyEditorNavigateUp,
            AppEvent::PrPropertyEditorNavigateDown => Self::PropertyEditorNavigateDown,
            AppEvent::PrPropertyEditorToggle => Self::PropertyEditorToggle,
            AppEvent::PrPropertyEditorConfirm => Self::PropertyEditorConfirm,
            AppEvent::PrPropertyEditorTitleChar(c) => Self::PropertyEditorTitleChar(c),
            AppEvent::PrPropertyEditorTitleBackspace => Self::PropertyEditorTitleBackspace,
            AppEvent::PrPropertyEditorTitleDelete => Self::PropertyEditorTitleDelete,
            AppEvent::PrPropertyEditorTitleCursorLeft => Self::PropertyEditorTitleCursorLeft,
            AppEvent::PrPropertyEditorTitleCursorRight => Self::PropertyEditorTitleCursorRight,
            // Non-property events should never reach this converter; the
            // routing guard filters them upstream. If one slips through,
            // it is safer to no-op (close the editor) than to panic or
            // enter an unrelated mode.
            _ => Self::PropertyEditorCancel,
        }
    }

    /// Property-editor events that carry payloads (options/succeeded/failed).
    fn from_app_event_property_payload(event: AppEvent) -> Self {
        match event {
            AppEvent::PrPropertyEditorOptionsLoaded {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
                options,
            } => Self::PropertyEditorOptionsLoaded {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
                options,
            },
            AppEvent::PrPropertyEditorOptionsFailed {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
                error,
            } => Self::PropertyEditorOptionsFailed {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
                error,
            },
            AppEvent::PrPropertyEditSucceeded {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
            } => Self::PropertyEditSucceeded {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
            },
            AppEvent::PrPropertyEditFailed {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
                error,
            } => Self::PropertyEditFailed {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
                error,
            },
            _ => Self::PropertyEditorCancel,
        }
    }

    /// Convert thread-related `AppEvent` variants into PR messages.
    ///
    /// @requirement REQ-PR-009
    pub(super) fn from_app_event_thread(event: &AppEvent) -> Option<Self> {
        match event {
            AppEvent::PrOpenThreadReplyComposer { thread_index } => Some(Self::OpenThreadReply {
                thread_index: *thread_index,
            }),
            AppEvent::PrToggleThreadResolve { thread_index } => Some(Self::ToggleThreadResolve {
                thread_index: *thread_index,
            }),
            AppEvent::PrThreadResolveSucceeded {
                scope_repo_id,
                thread_index,
                is_resolved,
                request_id,
            } => Some(Self::ThreadResolveSucceeded {
                scope_repo_id: scope_repo_id.clone(),
                thread_index: *thread_index,
                is_resolved: *is_resolved,
                request_id: *request_id,
            }),
            AppEvent::PrThreadResolveFailed {
                scope_repo_id,
                thread_index,
                request_id,
                error,
            } => Some(Self::ThreadResolveFailed {
                scope_repo_id: scope_repo_id.clone(),
                thread_index: *thread_index,
                request_id: *request_id,
                error: error.clone(),
            }),
            _ => None,
        }
    }

    /// Property-editor messages → AppEvent (issue #175).
    pub(super) fn into_app_event_property(self) -> AppEvent {
        match self {
            Self::PropertyEditorOptionsLoaded { .. }
            | Self::PropertyEditorOptionsFailed { .. }
            | Self::PropertyEditSucceeded { .. }
            | Self::PropertyEditFailed { .. } => self.into_app_event_property_payload(),
            other => other.into_app_event_property_simple(),
        }
    }

    /// Simple property-editor messages (no payload extraction needed).
    fn into_app_event_property_simple(self) -> AppEvent {
        match self {
            Self::OpenPropertyEditor { kind } => AppEvent::PrOpenPropertyEditor { kind },
            Self::PropertyEditorNavigateUp => AppEvent::PrPropertyEditorNavigateUp,
            Self::PropertyEditorNavigateDown => AppEvent::PrPropertyEditorNavigateDown,
            Self::PropertyEditorToggle => AppEvent::PrPropertyEditorToggle,
            Self::PropertyEditorConfirm => AppEvent::PrPropertyEditorConfirm,
            Self::PropertyEditorTitleChar(c) => AppEvent::PrPropertyEditorTitleChar(c),
            Self::PropertyEditorTitleBackspace => AppEvent::PrPropertyEditorTitleBackspace,
            Self::PropertyEditorTitleDelete => AppEvent::PrPropertyEditorTitleDelete,
            Self::PropertyEditorTitleCursorLeft => AppEvent::PrPropertyEditorTitleCursorLeft,
            Self::PropertyEditorTitleCursorRight => AppEvent::PrPropertyEditorTitleCursorRight,
            // Non-property messages should never reach this converter. If
            // one slips through, close the editor rather than entering an
            // unrelated mode.
            _ => AppEvent::PrPropertyEditorCancel,
        }
    }

    /// Property-editor messages with payloads → AppEvent.
    fn into_app_event_property_payload(self) -> AppEvent {
        match self {
            Self::PropertyEditorOptionsLoaded {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
                options,
            } => AppEvent::PrPropertyEditorOptionsLoaded {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
                options,
            },
            Self::PropertyEditorOptionsFailed {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
                error,
            } => AppEvent::PrPropertyEditorOptionsFailed {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
                error,
            },
            Self::PropertyEditSucceeded {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
            } => AppEvent::PrPropertyEditSucceeded {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
            },
            Self::PropertyEditFailed {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
                error,
            } => AppEvent::PrPropertyEditFailed {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
                error,
            },
            _ => AppEvent::PrPropertyEditorCancel,
        }
    }

    /// Convert thread-related PR messages back into `AppEvent`.
    ///
    /// @requirement REQ-PR-009
    pub(super) fn thread_to_app_event(&self) -> Option<AppEvent> {
        match self {
            Self::OpenThreadReply { thread_index } => Some(AppEvent::PrOpenThreadReplyComposer {
                thread_index: *thread_index,
            }),
            Self::ToggleThreadResolve { thread_index } => Some(AppEvent::PrToggleThreadResolve {
                thread_index: *thread_index,
            }),
            Self::ThreadResolveSucceeded {
                scope_repo_id,
                thread_index,
                is_resolved,
                request_id,
            } => Some(AppEvent::PrThreadResolveSucceeded {
                scope_repo_id: scope_repo_id.clone(),
                thread_index: *thread_index,
                is_resolved: *is_resolved,
                request_id: *request_id,
            }),
            Self::ThreadResolveFailed {
                scope_repo_id,
                thread_index,
                request_id,
                error,
            } => Some(AppEvent::PrThreadResolveFailed {
                scope_repo_id: scope_repo_id.clone(),
                thread_index: *thread_index,
                request_id: *request_id,
                error: error.clone(),
            }),
            _ => None,
        }
    }
}
