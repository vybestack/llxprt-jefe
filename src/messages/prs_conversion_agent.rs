use crate::state::AppEvent;

use super::{NavDir, PullRequestsMessage};

impl PullRequestsMessage {
    pub(super) fn into_app_event_agent(self) -> AppEvent {
        match self {
            Self::OpenAgentChooser { metadata } => AppEvent::PrOpenAgentChooser { metadata },
            Self::BeginListSendDetail { metadata } => AppEvent::BeginPrListSendDetail(metadata),
            Self::CancelListSendDetail => AppEvent::CancelPrListSendDetail,
            Self::ListSendDetailReady {
                scope_repo_id,
                pr_number,
                request_id,
            } => AppEvent::PrListSendDetailReady {
                scope_repo_id,
                pr_number,
                request_id,
            },
            Self::AgentChooserNavigate(NavDir::Up) => AppEvent::PrAgentChooserNavigateUp,
            Self::AgentChooserNavigate(NavDir::Down) => AppEvent::PrAgentChooserNavigateDown,
            Self::AgentChooserConfirm => AppEvent::PrAgentChooserConfirm,
            Self::AgentChooserCancel => AppEvent::PrAgentChooserCancel,
            Self::SendToAgentCompleted => AppEvent::PrSendToAgentCompleted,
            Self::SendToAgentFailed { error } => AppEvent::PrSendToAgentFailed { error },
            other => other.into_app_event_merge(),
        }
    }

    fn into_app_event_merge(self) -> AppEvent {
        if let Some(event) = self.thread_to_app_event() {
            return event;
        }
        if Self::is_pr_property_message(&self) {
            return self.into_app_event_property();
        }
        match self {
            Self::OpenMergeChooser => AppEvent::PrOpenMergeChooser,
            Self::MergeNavigate(NavDir::Up | NavDir::Prev) => AppEvent::PrMergeNavigateUp,
            Self::MergeNavigate(NavDir::Down | NavDir::Next) => AppEvent::PrMergeNavigateDown,
            Self::MergeConfirm => AppEvent::PrMergeConfirm,
            Self::MergeCancel => AppEvent::PrMergeCancel,
            Self::Merged {
                scope_repo_id,
                pr_number,
                method,
            } => AppEvent::PrMerged {
                scope_repo_id,
                pr_number,
                method,
            },
            Self::MergeFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            } => AppEvent::PrMergeFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            },
            Self::MergeMethodsLoaded {
                scope_repo_id,
                pr_number,
                allowed_methods,
            } => AppEvent::PrMergeMethodsLoaded {
                scope_repo_id,
                pr_number,
                allowed_methods,
            },
            Self::MergeMethodsLoadFailed {
                scope_repo_id,
                pr_number,
                error,
            } => AppEvent::PrMergeMethodsLoadFailed {
                scope_repo_id,
                pr_number,
                error,
            },
            _ => unreachable!("unrouted PullRequestsMessage variant reached merge converter"),
        }
    }

    fn is_pr_property_message(message: &Self) -> bool {
        matches!(
            message,
            Self::OpenPropertyEditor { .. }
                | Self::PropertyEditorNavigateUp
                | Self::PropertyEditorNavigateDown
                | Self::PropertyEditorToggle
                | Self::PropertyEditorConfirm
                | Self::PropertyEditorCancel
                | Self::PropertyEditorTitleChar(_)
                | Self::PropertyEditorTitleBackspace
                | Self::PropertyEditorTitleDelete
                | Self::PropertyEditorTitleCursorLeft
                | Self::PropertyEditorTitleCursorRight
                | Self::PropertyEditorTitleCursorHome
                | Self::PropertyEditorTitleCursorEnd
                | Self::PropertyEditorOptionsLoaded { .. }
                | Self::PropertyEditorOptionsFailed { .. }
                | Self::PropertyEditSucceeded { .. }
                | Self::PostMutationRefreshStarted
                | Self::PropertyEditFailed { .. }
                | Self::PropertyEditorValidationError { .. }
        )
    }
}
