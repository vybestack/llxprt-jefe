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
            AppEvent::PrOpenPropertyEditor { kind } => Self::OpenPropertyEditor { kind },
            AppEvent::PrPropertyEditorNavigateUp => Self::PropertyEditorNavigateUp,
            AppEvent::PrPropertyEditorNavigateDown => Self::PropertyEditorNavigateDown,
            AppEvent::PrPropertyEditorToggle => Self::PropertyEditorToggle,
            AppEvent::PrPropertyEditorConfirm => Self::PropertyEditorConfirm,
            AppEvent::PrPropertyEditorCancel => Self::PropertyEditorCancel,
            AppEvent::PrPropertyEditorOptionsLoaded { options } => {
                Self::PropertyEditorOptionsLoaded { options }
            }
            AppEvent::PrPropertyEditSucceeded {
                scope_repo_id,
                pr_number,
            } => Self::PropertyEditSucceeded {
                scope_repo_id,
                pr_number,
            },
            AppEvent::PrPropertyEditFailed {
                scope_repo_id,
                pr_number,
                error,
            } => Self::PropertyEditFailed {
                scope_repo_id,
                pr_number,
                error,
            },
            _ => unreachable!("non-property AppEvent routed to PR property converter"),
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
            Self::OpenPropertyEditor { kind } => AppEvent::PrOpenPropertyEditor { kind },
            Self::PropertyEditorNavigateUp => AppEvent::PrPropertyEditorNavigateUp,
            Self::PropertyEditorNavigateDown => AppEvent::PrPropertyEditorNavigateDown,
            Self::PropertyEditorToggle => AppEvent::PrPropertyEditorToggle,
            Self::PropertyEditorConfirm => AppEvent::PrPropertyEditorConfirm,
            Self::PropertyEditorCancel => AppEvent::PrPropertyEditorCancel,
            Self::PropertyEditorOptionsLoaded { options } => {
                AppEvent::PrPropertyEditorOptionsLoaded { options }
            }
            Self::PropertyEditSucceeded {
                scope_repo_id,
                pr_number,
            } => AppEvent::PrPropertyEditSucceeded {
                scope_repo_id,
                pr_number,
            },
            Self::PropertyEditFailed {
                scope_repo_id,
                pr_number,
                error,
            } => AppEvent::PrPropertyEditFailed {
                scope_repo_id,
                pr_number,
                error,
            },
            _ => unreachable!("unrouted PullRequestsMessage variant reached property converter"),
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
